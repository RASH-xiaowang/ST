// ============================================================
// HTTP API — 媒体（按需即时解密 / 转码 / Range 断点）
// 自 http_api.rs 拆分：聊天图片/视频、朋友圈视频、表情、文件媒体。
// ============================================================

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
};
use std::collections::HashMap;
use std::sync::Arc;

use super::{check_auth, load_cfg, ApiError, ApiResult, ApiServerState};

// ============ 6. 媒体（按需即时解密）============

pub(crate) async fn get_media(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Path((username, local_id)): Path<(String, i64)>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    check_auth(&state, &headers, &q, None)?;
    // 默认返回高清/原图（用户要求：不管任何图片都返回高清原图），仅显式 size=thumb 才用缩略图
    let hd = q.get("size").map(|s| s != "thumb").unwrap_or(true);
    let cfg = load_cfg()?;
    let aes_key: Option<Vec<u8>> = cfg
        .image_aes_key
        .as_ref()
        .filter(|k| k.len() == 16)
        .map(|k| k.as_bytes().to_vec());
    let xor_key = cfg.image_xor_key;
    let base_dir = cfg.wechat_base_dir.clone();
    let decrypted_dir = cfg.decrypted_dir.clone();
    let res_db = cfg
        .decrypted_dir
        .join("message")
        .join("message_resource.db");
    let decoded_dir = cfg.decoded_image_dir.clone();
    let uname = username.clone();
    // 优先使用监控任务的实时解密缓存（始终读取最新 message_resource.db），
    // 避免静态解密副本过期导致新消息图片 NOT_FOUND。
    let live_cache = state.monitor.db_cache();

    let result = tokio::task::spawn_blocking(move || {
        if let Some(dbc) = live_cache.as_deref() {
            crate::wechat::image::resolve_message_image_bytes_live(
                &crate::wechat::image::ImageResolveCtx {
                    wechat_base_dir: &base_dir,
                    res_db_path: None,
                    db_cache: Some(dbc),
                    decrypted_dir: &decrypted_dir,
                    decoded_dir: &decoded_dir,
                    aes_key: aes_key.as_deref(),
                    xor_key,
                },
                &crate::wechat::image::ImageQuery {
                    username: &uname,
                    local_id,
                    hd,
                    skip_cdn: false,
                },
            )
        } else {
            crate::wechat::image::resolve_message_image_bytes(
                &crate::wechat::image::ImageResolveCtx {
                    wechat_base_dir: &base_dir,
                    res_db_path: Some(&res_db),
                    db_cache: None,
                    decrypted_dir: &decrypted_dir,
                    decoded_dir: &decoded_dir,
                    aes_key: aes_key.as_deref(),
                    xor_key,
                },
                &crate::wechat::image::ImageQuery {
                    username: &uname,
                    local_id,
                    hd,
                    skip_cdn: false,
                },
            )
        }
    })
    .await
    .map_err(|e| ApiError::internal(format!("解密任务失败: {}", e)))?;

    match result {
        Some((bytes, mime)) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, "public, max-age=86400, immutable")
            .body(Body::from(bytes))
            .unwrap()),
        None => Err(ApiError::not_found(format!(
            "图片不存在或解密失败 (local_id={})",
            local_id
        ))),
    }
}

/// 聊天视频消息：按 (username, local_id) 定位附件视频文件并返回
pub(crate) async fn get_media_video(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Path((username, local_id)): Path<(String, i64)>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    check_auth(&state, &headers, &q, None)?;
    let cfg = load_cfg()?;
    let base_dir = cfg.wechat_base_dir.clone();
    let decrypted_dir = cfg.decrypted_dir.clone();
    let state2 = state.clone();
    let path = tokio::task::spawn_blocking(move || {
        // 先让监控缓存刷新解密 hardlink.db（源库变化时自动重新解密），
        // 新视频入库后无需手动「批量解密」即可解析到本地文件
        if let Some(cache) = state2.monitor.db_cache() {
            let _ = cache.get("hardlink/hardlink.db");
        }
        crate::wechat::voice::resolve_message_video_file(
            &base_dir,
            &decrypted_dir,
            &username,
            local_id,
        )
    })
    .await
    .map_err(|e| ApiError::internal(format!("视频定位任务失败: {}", e)))?;

    let Some(path) = path else {
        return Err(ApiError::not_found(format!(
            "视频附件不存在 (local_id={})",
            local_id
        )));
    };
    let mime = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| match e.to_lowercase().as_str() {
            "mp4" => "video/mp4",
            "m4v" => "video/x-m4v",
            "mov" => "video/quicktime",
            _ => "application/octet-stream",
        })
        .unwrap_or("application/octet-stream")
        .to_string();
    let total = tokio::fs::metadata(&path)
        .await
        .map_err(|e| ApiError::internal(format!("读取视频元数据失败: {}", e)))?
        .len() as usize;
    let total_usize = total;

    // Range 请求（浏览器拖动进度条 / 分段加载）：只读请求分片，显著降低首帧等待与大文件内存占用
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    if let Some(r) = range {
        if let Some(spec) = r.strip_prefix("bytes=") {
            if let Some((s, e)) = parse_range(spec, total_usize) {
                use tokio::io::{AsyncReadExt, AsyncSeekExt};
                let mut file = tokio::fs::File::open(&path)
                    .await
                    .map_err(|e| ApiError::internal(format!("打开视频失败: {}", e)))?;
                file.seek(std::io::SeekFrom::Start(s as u64))
                    .await
                    .map_err(|e| ApiError::internal(format!("视频定位失败: {}", e)))?;
                let mut buf = vec![0u8; e - s];
                file.read_exact(&mut buf)
                    .await
                    .map_err(|e| ApiError::internal(format!("读取视频分片失败: {}", e)))?;
                drop(file);
                return Ok(Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(header::CONTENT_TYPE, &mime)
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(
                        header::CONTENT_RANGE,
                        format!("bytes {}-{}/{}", s, e - 1, total_usize),
                    )
                    .header(header::CONTENT_LENGTH, buf.len())
                    .body(Body::from(buf))
                    .unwrap());
            }
        }
    }

    // 无 Range：全量返回（仍标记 Accept-Ranges 供后续 seek）
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| ApiError::internal(format!("读取视频失败: {}", e)))?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(bytes))
        .unwrap())
}

/// 聊天视频封面：返回 `<file>_thumb.jpg`（真实视频帧缩略图），无封面时回退同名 jpg
pub(crate) async fn get_media_video_thumb(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Path((username, local_id)): Path<(String, i64)>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    check_auth(&state, &headers, &q, None)?;
    let cfg = load_cfg()?;
    let base_dir = cfg.wechat_base_dir.clone();
    let decrypted_dir = cfg.decrypted_dir.clone();
    let state2 = state.clone();
    let path = tokio::task::spawn_blocking(move || {
        // 与视频端点一致：先刷新解密 hardlink.db 再解析封面
        if let Some(cache) = state2.monitor.db_cache() {
            let _ = cache.get("hardlink/hardlink.db");
        }
        crate::wechat::voice::resolve_message_video_thumb(
            &base_dir,
            &decrypted_dir,
            &username,
            local_id,
        )
    })
    .await
    .map_err(|e| ApiError::internal(format!("视频封面定位任务失败: {}", e)))?;

    let Some(path) = path else {
        return Err(ApiError::not_found(format!(
            "视频封面不存在 (local_id={})",
            local_id
        )));
    };
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| ApiError::internal(format!("读取视频封面失败: {}", e)))?;
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") && bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/jpeg"
    };
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "public, max-age=86400, immutable")
        .body(Body::from(bytes))
        .unwrap())
}

// ============ 6.1 朋友圈视频播放 ============

/// 服务已解密的本地朋友圈视频（MP4），支持 Range 断点（拖动进度条）
///
/// `file_key` 为 `get_moment_video` IPC 返回的 MD5 值，文件位于
/// `decoded_image_dir/moments_video/<file_key>.mp4`。
pub(crate) async fn get_sns_video(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Path(file_key): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    check_auth(&state, &headers, &q, None)?;
    if file_key.is_empty()
        || file_key.len() > 64
        || !file_key.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(ApiError::bad_request("无效的文件 key"));
    }
    let cfg = load_cfg()?;
    let path = cfg
        .decoded_image_dir
        .join("moments_video")
        .join(format!("{}.mp4", file_key));
    if !path.is_file() {
        return Err(ApiError::not_found(
            "视频未解密或不存在，请先在朋友圈点击播放",
        ));
    }
    let bytes =
        std::fs::read(&path).map_err(|e| ApiError::internal(format!("读取视频失败: {}", e)))?;

    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Some(r) = range {
        if let Some(spec) = r.strip_prefix("bytes=") {
            if let Some((s, e)) = parse_range(spec, bytes.len()) {
                let body = bytes[s..e].to_vec();
                return Ok(Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(header::CONTENT_TYPE, "video/mp4")
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(
                        header::CONTENT_RANGE,
                        format!("bytes {}-{}/{}", s, e - 1, bytes.len()),
                    )
                    .header(header::CONTENT_LENGTH, body.len())
                    .body(Body::from(body))
                    .unwrap());
            }
        }
    }

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "public, max-age=86400, immutable")
        .body(Body::from(bytes))
        .unwrap())
}

/// 解析 `Range: bytes=` 规格："start-end" / "start-" / "-suffix"，返回 [start, end)（end 独占）
fn parse_range(spec: &str, total: usize) -> Option<(usize, usize)> {
    if total == 0 {
        return None;
    }
    let (s, e) = spec.split_once('-')?;
    let start: usize = if s.is_empty() {
        let suffix: usize = e.trim().parse().ok()?;
        if suffix == 0 {
            return None;
        }
        total.saturating_sub(suffix)
    } else {
        s.trim().parse().ok()?
    };
    let end: usize = if e.trim().is_empty() {
        total
    } else {
        e.trim().parse::<usize>().ok()?.min(total).saturating_add(1)
    };
    if start >= end || start >= total {
        return None;
    }
    Some((start, end.min(total)))
}

// ============ 6.2 自定义表情图片（CDN 下载 + 本地缓存） ============

/// 自定义表情图片：按 MD5 从微信 CDN 下载并本地缓存后返回
///
/// 微信表情库（emoticon.db）只存元数据与 CDN 地址；前端 WebView 直接加载
/// `http://wxapp.tc.qq.com/...` 会被混合内容策略拦截，因此统一由后端下载，
/// 缓存到 `decoded_image_dir/emoticons/<md5>.img`，再经本地 API 提供。
pub(crate) async fn get_emoticon_image(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Path(md5): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    check_auth(&state, &headers, &q, None)?;
    let md5 = md5.trim().to_lowercase();
    if md5.len() != 32 || !md5.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request("无效的表情 MD5"));
    }
    let cfg = load_cfg()?;
    let decrypted_dir = cfg.decrypted_dir.clone();
    let decoded_dir = cfg.decoded_image_dir.clone();
    let (bytes, mime) = tokio::task::spawn_blocking(move || {
        let path = crate::wechat::modules::emoticons::ensure_emoticon_cached(
            &decrypted_dir,
            &decoded_dir,
            &md5,
        )?;
        let bytes = std::fs::read(&path).map_err(|e| format!("读取表情缓存失败: {}", e))?;
        let mime = crate::wechat::modules::emoticons::detect_emoticon_mime(&bytes).to_string();
        Ok::<(Vec<u8>, String), String>((bytes, mime))
    })
    .await
    .map_err(|e| ApiError::internal(format!("表情下载任务失败: {}", e)))?
    .map_err(ApiError::internal)?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "public, max-age=86400, immutable")
        .body(Body::from(bytes))
        .unwrap())
}

// ============ 6.3 文件管理资源（图片 / 视频 / 封面） ============

fn valid_md5(md5: &str) -> bool {
    md5.len() == 32 && md5.chars().all(|c| c.is_ascii_hexdigit())
}

/// 文件管理图片：按 md5 从本地附件解密返回（磁盘缓存，浏览器可缓存）
pub(crate) async fn get_file_image(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Path(md5): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    check_auth(&state, &headers, &q, None)?;
    let md5 = md5.trim().to_lowercase();
    if !valid_md5(&md5) {
        return Err(ApiError::bad_request("无效的图片 MD5"));
    }
    let cfg = load_cfg()?;
    let base_dir = cfg.wechat_base_dir.clone();
    let decrypted_dir = cfg.decrypted_dir.clone();
    let decoded_dir = cfg.decoded_image_dir.clone();
    let aes_key: Option<Vec<u8>> = cfg
        .image_aes_key
        .as_ref()
        .filter(|k| k.len() == 16)
        .map(|k| k.as_bytes().to_vec());
    let xor_key = cfg.image_xor_key;
    let username = cfg.wxid().unwrap_or_default();

    let (bytes, mime) = tokio::task::spawn_blocking(move || {
        // 1) hardlink 精确定位；2) 兜底扫描 attach
        let dat = crate::wechat::modules::files::resolve_file_path(&decrypted_dir, &base_dir, &md5)
            .or_else(|| {
                let dats = crate::wechat::image::find_dat_files(&base_dir, &username, &md5);
                crate::wechat::image::pick_dat(&dats, false)
            })
            .ok_or_else(|| "找不到图片文件".to_string())?;
        let cache_dir = decoded_dir.join("files_images");
        let data_url = crate::wechat::image::decode_dat_to_data_url(
            &dat,
            &cache_dir,
            &md5,
            aes_key.as_deref(),
            xor_key,
        )
        .ok_or_else(|| "图片解密失败".to_string())?;
        let (mime, b64) = data_url
            .strip_prefix("data:image/")
            .and_then(|s| s.split_once(";base64,"))
            .ok_or_else(|| "图片数据格式异常".to_string())?;
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("图片解码失败: {}", e))?;
        Ok::<(Vec<u8>, String), String>((bytes, format!("image/{}", mime)))
    })
    .await
    .map_err(|e| ApiError::internal(format!("图片解密任务失败: {}", e)))?
    .map_err(ApiError::internal)?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "public, max-age=86400, immutable")
        .body(Body::from(bytes))
        .unwrap())
}

/// 文件管理视频：按 md5 定位本地视频并返回（支持 Range 拖动进度）
pub(crate) async fn get_file_video(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Path(md5): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    check_auth(&state, &headers, &q, None)?;
    let md5 = md5.trim().to_lowercase();
    if !valid_md5(&md5) {
        return Err(ApiError::bad_request("无效的视频 MD5"));
    }
    let cfg = load_cfg()?;
    let base_dir = cfg.wechat_base_dir.clone();
    let decrypted_dir = cfg.decrypted_dir.clone();
    let md5c = md5.clone();
    let path = tokio::task::spawn_blocking(move || {
        crate::wechat::modules::files::resolve_file_path(&decrypted_dir, &base_dir, &md5c)
    })
    .await
    .map_err(|e| ApiError::internal(format!("视频定位任务失败: {}", e)))?
    .ok_or_else(|| ApiError::not_found(format!("视频文件不存在 (md5={})", md5)))?;

    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| ApiError::internal(format!("读取视频失败: {}", e)))?;
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    if let Some(r) = range {
        if let Some(spec) = r.strip_prefix("bytes=") {
            if let Some((s, e)) = parse_range(spec, bytes.len()) {
                let body = bytes[s..e].to_vec();
                return Ok(Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(header::CONTENT_TYPE, "video/mp4")
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(
                        header::CONTENT_RANGE,
                        format!("bytes {}-{}/{}", s, e - 1, bytes.len()),
                    )
                    .header(header::CONTENT_LENGTH, body.len())
                    .body(Body::from(body))
                    .unwrap());
            }
        }
    }
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "public, max-age=86400, immutable")
        .body(Body::from(bytes))
        .unwrap())
}

/// 文件管理视频封面：按视频 md5 定位同名封面图
pub(crate) async fn get_file_video_thumb(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Path(md5): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    check_auth(&state, &headers, &q, None)?;
    let md5 = md5.trim().to_lowercase();
    if !valid_md5(&md5) {
        return Err(ApiError::bad_request("无效的视频 MD5"));
    }
    let cfg = load_cfg()?;
    let base_dir = cfg.wechat_base_dir.clone();
    let decrypted_dir = cfg.decrypted_dir.clone();
    let md5c = md5.clone();
    let path = tokio::task::spawn_blocking(move || {
        crate::wechat::modules::files::resolve_video_cover_path(&decrypted_dir, &base_dir, &md5c)
    })
    .await
    .map_err(|e| ApiError::internal(format!("封面定位任务失败: {}", e)))?
    .ok_or_else(|| ApiError::not_found(format!("视频封面不存在 (md5={})", md5)))?;

    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| ApiError::internal(format!("读取封面失败: {}", e)))?;
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") && bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/jpeg"
    };
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "public, max-age=86400, immutable")
        .body(Body::from(bytes))
        .unwrap())
}
