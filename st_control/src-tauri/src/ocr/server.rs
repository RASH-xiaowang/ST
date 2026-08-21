// ============================================================
// 图文识别 — HTTP 资源接收服务 + 处理管线
// POST /api/ocr/ingest  接收资源（必填：sender_username, session_type,
//                       timestamp, username, mediaUrl）
// GET  /api/ocr/health  服务状态
// 处理管线：下载/读取资源 → 存 incoming → 开源 OCR 预检（无文本则过滤）
//          → TextIn 证件分类 → 按分类归档 → 对应 OCR 识别 → 结果入库 → 事件推送
// ============================================================

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use super::config::OcrConfig;
use super::db::OcrDb;
use super::textin;
use super::OcrState;

const MAX_MEDIA_BYTES: usize = 10 * 1024 * 1024;

/// 模拟测试内嵌图片（100x100 PNG，含 TEST 字样；满足 TextIn 图片尺寸下限）
const TEST_IMAGE_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAGQAAABkCAYAAABw4pVUAAAAAXNSR0IArs4c6QAAAARnQU1BAACxjwv8YQUAAAAJcEhZcwAADsMAAA7DAcdvqGQAAAIJSURBVHhe7dfLbcJAGABhKkkl6YIeqCEXGuBIB1xdBeJEBRSAuFHERhFB4H8XPyB2BmtW+i5Z22sz8iOzj8+vJI5Z/IP+l0FgDAJjEBiDwBgExiAwBoExCIxBYAwCYxAYg8AYBMYgMAaBMQiMQWAMAmMQGIPAGATGIDAGgTEIjEFgDAJjEBiDwBgExiAwBoExCIxBYAwCYxAYg8AYBMYgMAaBMQiMQWAMAjN4kOU+dRyHtOyx/bFaZ2vNq3PcLKV9lW33zHnF/YcykSBV2sUNaiP/UbuuU9p3SJMIUrwz4gh3Std1Jhckuv0Q5Qttm8+t0+Z03eecNou7ucU2Ha9TLcfrv+4wphXktE3zMH+7e0KsoP+6wwAHaRr1feMjq/TCb9N2XmOZRJD6YyuO5jsjXzcee1wTCXIR75TaKDzOyuuWjz0WcJDyfHf5p3DTo+zv1n3N+we5+5Iq/+B3j7MH/yT+6L3uQN4/SLgTdqswvzrcJg2Sa7vwbu+Q+juh6z7lOygeo3xeY5lEkOavrN/hS72s7cKfC3Lx6Csre4wVtJ3XWEYPomYGgTEIjEFgDAJjEBiDwBgExiAwBoExCIxBYAwCYxAYg8AYBMYgMAaBMQiMQWAMAmMQGIPAGATGIDAGgTEIjEFgDAJjEBiDwBgExiAwBoExCIxBYAwCYxAYg8AYBMYgMAaB+QYW57ofkxOtHgAAAABJRU5ErkJggg==";

static TEST_IMAGE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();

fn test_image_bytes() -> &'static [u8] {
    TEST_IMAGE.get_or_init(|| {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(TEST_IMAGE_B64)
            .unwrap_or_default()
    })
}

/// 模拟测试静态图片（一汽乘用车评估测试图，内嵌于二进制）
pub const TEST_IMAGES: &[(&str, &[u8])] = &[
    ("test1.jpg", include_bytes!("assets/test1.jpg")),
    ("test2.jpg", include_bytes!("assets/test2.jpg")),
    ("test3.jpg", include_bytes!("assets/test3.jpg")),
    ("test4.jpg", include_bytes!("assets/test4.jpg")),
];

/// 按名称（如 test1.jpg）取内置测试图片
pub fn resolve_test_image(name: &str) -> Option<&'static [u8]> {
    TEST_IMAGES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, b)| *b)
}

// ─────────────────────────── 路由 ───────────────────────────

pub fn build_router(state: Arc<OcrState>) -> Router {
    Router::new()
        .route("/api/ocr/health", get(health))
        .route("/api/ocr/ingest", post(ingest))
        // 仅放行应用自身来源，拒绝任意站点跨域投递
        .layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::AllowOrigin::predicate(
                    move |origin, _| {
                        let o = origin.as_bytes();
                        o.is_empty()
                            || matches!(
                                o,
                                b"http://localhost:1420"
                                    | b"http://127.0.0.1:1420"
                                    | b"tauri://localhost"
                                    | b"http://tauri.localhost"
                            )
                    },
                ))
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::PUT,
                    axum::http::Method::DELETE,
                    axum::http::Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::ACCEPT,
                ]),
        )
        .with_state(state)
}

/// 启动接收服务；绑定失败时每 10 秒重试（配置变更通过重启任务切换）
pub async fn serve(state: Arc<OcrState>) {
    loop {
        let (bind, port, enabled) = {
            let cfg = state.config.read().unwrap_or_else(|p| p.into_inner());
            (cfg.bind_host.clone(), cfg.port, cfg.enabled)
        };
        if !enabled {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            continue;
        }
        let addr = format!("{bind}:{port}");
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                log::error!("[ocr] 绑定 {addr} 失败: {e}，10 秒后重试");
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                continue;
            }
        };
        log::info!("[ocr] 图文识别资源接收服务已启动: http://{addr}/api/ocr/ingest");
        let app = build_router(state.clone());
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("[ocr] 服务异常退出: {e}，5 秒后重启");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        } else {
            // 正常关闭（任务被 abort / 配置变更）
            break;
        }
    }
}

fn err_response(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "success": false,
            "error": { "code": code, "message": message }
        })),
    )
        .into_response()
}

// ─────────────────────────── 健康检查 ───────────────────────────

async fn health(State(state): State<Arc<OcrState>>) -> Json<serde_json::Value> {
    let cfg = state
        .config
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let stats = state.db.stats().unwrap_or_default();
    Json(serde_json::json!({
        "success": true,
        "service": "st-control-ocr",
        "configured": cfg.has_credentials(),
        "stats": {
            "total": stats.total,
            "by_status": stats.by_status,
        }
    }))
}

// ─────────────────────────── 接收资源 ───────────────────────────

async fn ingest(State(state): State<Arc<OcrState>>, headers: HeaderMap, body: String) -> Response {
    let cfg = state
        .config
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    if !cfg.enabled {
        return err_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "DISABLED",
            "资源接收服务已停用",
        );
    }

    // 鉴权（配置了 token 时：Authorization: Bearer / ?access_token= / body access_token）
    if !cfg.token.trim().is_empty() && !authorized(&cfg.token, &headers, &body) {
        return err_response(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "访问令牌无效");
    }

    let payload: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            return err_response(
                StatusCode::BAD_REQUEST,
                "BAD_JSON",
                &format!("请求体不是合法 JSON: {e}"),
            )
        }
    };
    let obj = match payload.as_object() {
        Some(o) => o,
        None => return err_response(StatusCode::BAD_REQUEST, "BAD_JSON", "请求体应为 JSON 对象"),
    };

    let get = |k: &str, alias: Option<&str>| -> String {
        obj.get(k)
            .and_then(|v| v.as_str())
            .or_else(|| alias.and_then(|a| obj.get(a).and_then(|v| v.as_str())))
            .unwrap_or("")
            .trim()
            .to_string()
    };

    let sender_username = get("sender_username", None);
    let session_type = get("session_type", None);
    let timestamp = get("timestamp", None);
    let username = get("username", None);
    let media_url = get("mediaUrl", Some("media_url"));

    let mut missing: Vec<&str> = Vec::new();
    if sender_username.is_empty() {
        missing.push("sender_username");
    }
    if session_type.is_empty() {
        missing.push("session_type");
    }
    if timestamp.is_empty() {
        missing.push("timestamp");
    }
    if username.is_empty() {
        missing.push("username");
    }
    if media_url.is_empty() {
        missing.push("mediaUrl");
    }
    if !missing.is_empty() {
        return err_response(
            StatusCode::BAD_REQUEST,
            "MISSING_PARAM",
            &format!("缺少必填参数: {}", missing.join(", ")),
        );
    }

    let id = match state.db.insert_resource(
        &sender_username,
        &session_type,
        &timestamp,
        &username,
        &media_url,
    ) {
        Ok(id) => id,
        Err(e) => {
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                &format!("写入数据库失败: {e}"),
            )
        }
    };

    let st = state.clone();
    tauri::async_runtime::spawn(async move {
        process_resource(st, id).await;
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "success": true,
            "id": id,
            "status": "pending"
        })),
    )
        .into_response()
}

fn authorized(token: &str, headers: &HeaderMap, body: &str) -> bool {
    // 1. Header: Authorization: Bearer <token>
    if let Some(h) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if h.strip_prefix("Bearer ")
            .map(|t| t.trim() == token)
            .unwrap_or(false)
        {
            return true;
        }
    }
    // 2. Query: ?access_token=  （body 由调用方拼接，这里简单兼容）
    if body.contains("access_token") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(t) = v.get("access_token").and_then(|x| x.as_str()) {
                if t.trim() == token {
                    return true;
                }
            }
        }
    }
    false
}

// ─────────────────────────── 处理管线 ───────────────────────────

/// 异步处理一条资源：下载 → 存档 → 分类 → 归档 → OCR → 入库
pub async fn process_resource(state: Arc<OcrState>, id: i64) {
    log::info!("[ocr] process_resource 开始: id={id}");
    if state.db.update_processing(id).is_err() {
        log::warn!("[ocr] process_resource 无法标记处理中: id={id}");
        return;
    }
    let resource = match state.db.get_resource(id) {
        Ok(Some(r)) => r,
        _ => {
            log::warn!("[ocr] process_resource 记录不存在: id={id}");
            return;
        }
    };
    let cfg = state
        .config
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .clone();

    let outcome = run_pipeline(&state, &cfg, &resource).await;
    if let Err(e) = outcome {
        log::warn!("[ocr] process_resource 失败: id={id} err={e}");
        let _ = state.db.update_failed(id, "failed", &e);
        state.emit_event(id, "failed", &resource.category, Some(&e));
    } else {
        log::info!("[ocr] process_resource 完成: id={id}");
    }
}

async fn run_pipeline(
    state: &OcrState,
    cfg: &OcrConfig,
    res: &super::db::OcrResource,
) -> Result<(), String> {
    // 1. 获取资源（http/https 下载，file:// 或本地路径直接读取）
    let bytes = fetch_media(&res.media_url).await?;
    log::info!("[ocr] 资源已获取: id={} bytes={}", res.id, bytes.len());

    // 2. 先落到 incoming 目录
    let incoming = save_file(&bytes, "incoming", &res.media_url)?;
    let _ = state
        .db
        .update_media_path(res.id, incoming.to_string_lossy().as_ref());

    // 2.5 开源 OCR 预检（仅图片）：识别出有效文本才继续调用证件分类；
    //     无文本图片直接过滤归档，避免浪费证件分类接口
    if cfg.precheck_enabled {
        let ext = guess_ext(&res.media_url, &bytes);
        if is_image_ext(&ext) {
            let text = state.run_precheck(cfg, &incoming).await?;
            let trimmed = text.trim().to_string();
            let char_count = trimmed.chars().count();
            let _ = state.db.update_precheck_text(res.id, &trimmed);
            state.emit_event(res.id, "precheck", "", None);
            if char_count < cfg.precheck_min_chars {
                let msg = format!(
                    "开源 OCR 预检未识别到有效文本（{} 字符 < {}），已跳过证件分类",
                    char_count, cfg.precheck_min_chars
                );
                log::info!("[ocr] 预检过滤无文本图片: id={} {}", res.id, msg);
                let filtered = match archive_file(&incoming, "filtered") {
                    Ok(p) => p,
                    Err(_) => incoming.clone(),
                };
                let _ = state
                    .db
                    .update_media_path(res.id, filtered.to_string_lossy().as_ref());
                let _ = state.db.update_failed(res.id, "filtered", &msg);
                state.emit_event(res.id, "filtered", "", Some(&msg));
                return Ok(());
            }
            log::info!("[ocr] 预检通过: id={} chars={}", res.id, char_count);
        } else {
            log::info!(
                "[ocr] 跳过开源 OCR 预检（非图片格式）: id={} ext={}",
                res.id,
                ext
            );
        }
    }

    // 3. TextIn 证件分类
    let (raw, outcome) = textin::classify(cfg, &bytes).await?;
    log::info!(
        "[ocr] 分类完成: id={} category={} desc={}",
        res.id,
        outcome.category,
        outcome.description
    );
    let _ = state
        .db
        .update_classified(res.id, &outcome.category, &outcome.description, &raw);
    state.emit_event(res.id, "saved", &outcome.category, None);

    // 4. 归档到分类目录（移动，删除 incoming 副本）
    let archived = match archive_file(&incoming, &outcome.category) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("[ocr] 归档失败（保留 incoming）: {e}");
            incoming.clone()
        }
    };
    let _ = state
        .db
        .update_media_path(res.id, archived.to_string_lossy().as_ref());

    // 5. 按分类调用对应 OCR 接口（配置映射优先，回落内置映射）
    let endpoint = match textin::resolve_endpoint(cfg, &outcome.category) {
        Some(ep) => ep,
        None => {
            let _ = state.db.update_ocr_result(
                res.id,
                "saved",
                "",
                "{}",
                "该分类未配置 OCR 接口，仅完成分类归档",
            );
            state.emit_event(res.id, "saved", &outcome.category, None);
            return Ok(());
        }
    };

    match textin::ocr(cfg, &endpoint, &bytes).await {
        Ok((raw_ocr, out)) => {
            let fields = serde_json::to_string(&out.fields).unwrap_or_else(|_| "{}".to_string());
            let _ = state
                .db
                .update_ocr_result(res.id, "success", &raw_ocr, &fields, "");
            state.emit_event(res.id, "success", &outcome.category, None);
            Ok(())
        }
        Err(e) => {
            // 分类与归档已完成，OCR 失败保留可重试状态
            let _ = state
                .db
                .update_ocr_result(res.id, "ocr_failed", "", "{}", &e);
            state.emit_event(res.id, "ocr_failed", &outcome.category, Some(&e));
            Ok(())
        }
    }
}

// ─────────────────────────── 工具函数 ───────────────────────────

async fn fetch_media(url: &str) -> Result<Vec<u8>, String> {
    let t = url.trim();
    if t.is_empty() {
        return Err("mediaUrl 为空".to_string());
    }
    let lower = t.to_lowercase();
    if lower.starts_with("data:") {
        // data:[<mediatype>][;base64],<data>（微信图片查看器未启用 HTTP API 时的回退）
        let comma = t
            .find(',')
            .ok_or_else(|| "data URL 缺少逗号分隔".to_string())?;
        let meta = &t[..comma];
        let data = &t[comma + 1..];
        let bytes = if meta.to_lowercase().ends_with(";base64") {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| format!("data URL base64 解码失败: {e}"))?
        } else {
            percent_decode(data)?
        };
        if bytes.len() > MAX_MEDIA_BYTES {
            return Err(format!("资源超过 10M 上限（{} bytes）", bytes.len()));
        }
        Ok(bytes)
    } else if lower.starts_with("http://") || lower.starts_with("https://") {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("创建下载客户端失败: {e}"))?;
        let resp = client
            .get(t)
            .send()
            .await
            .map_err(|e| format!("下载资源失败: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("下载资源失败: HTTP {}", resp.status()));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("读取下载内容失败: {e}"))?;
        if bytes.len() > MAX_MEDIA_BYTES {
            return Err(format!("资源超过 10M 上限（{} bytes）", bytes.len()));
        }
        Ok(bytes.to_vec())
    } else if lower.starts_with("file://") {
        let p = t
            .trim_start_matches("file://")
            .trim_start_matches("file:///");
        read_local(p)
    } else if lower.starts_with("builtin://") {
        // 模拟测试内嵌图片：builtin://test/testN.jpg → 静态测试图；其余回退旧 PNG
        let key = t.trim_start_matches("builtin://").to_lowercase();
        if let Some(name) = key.strip_prefix("test/") {
            resolve_test_image(name)
                .map(|b| b.to_vec())
                .ok_or_else(|| format!("未知内置测试图片: {name}"))
        } else {
            Ok(test_image_bytes().to_vec())
        }
    } else {
        read_local(t)
    }
}

/// 简单百分号解码（data URL 非 base64 场景）
fn percent_decode(s: &str) -> Result<Vec<u8>, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                .map_err(|_| "data URL 解码失败".to_string())?;
            let v = u8::from_str_radix(hex, 16).map_err(|_| "data URL 解码失败".to_string())?;
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Ok(out)
}

fn read_local(path: &str) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取本地资源失败（{path}）: {e}"))?;
    if bytes.len() > MAX_MEDIA_BYTES {
        return Err(format!("资源超过 10M 上限（{} bytes）", bytes.len()));
    }
    Ok(bytes)
}

fn guess_ext(url: &str, bytes: &[u8]) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    if let Some(ext) = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        let ext = ext.to_lowercase();
        if [
            "jpg", "jpeg", "png", "bmp", "pdf", "tif", "tiff", "webp", "gif",
        ]
        .contains(&ext.as_str())
        {
            return if ext == "jpeg" {
                "jpg".to_string()
            } else {
                ext
            };
        }
    }
    if bytes.len() >= 5 && &bytes[..5] == b"%PDF-" {
        return "pdf".to_string();
    }
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        return "jpg".to_string();
    }
    if bytes.len() >= 8 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" {
        return "png".to_string();
    }
    if bytes.len() >= 4 && &bytes[..4] == b"GIF8" {
        return "gif".to_string();
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return "webp".to_string();
    }
    if bytes.len() >= 2 && &bytes[..2] == b"BM" {
        return "bmp".to_string();
    }
    "img".to_string()
}

/// 预检仅对图片生效（PDF 等其它格式沿用原流程）
fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext,
        "jpg" | "jpeg" | "png" | "bmp" | "webp" | "gif" | "tif" | "tiff"
    )
}

fn date_path(now: &chrono::DateTime<chrono::Local>) -> String {
    now.format("%Y/%m/%d").to_string()
}

/// 保存文件到 {root}/{stage}/yyyy/MM/dd/{uuid}.{ext}
fn save_file(bytes: &[u8], stage: &str, url: &str) -> Result<std::path::PathBuf, String> {
    let ext = guess_ext(url, bytes);
    let now = chrono::Local::now();
    let mut dir = OcrDb::storage_root();
    dir.push(stage);
    dir.push(date_path(&now));
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {e}"))?;
    let name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| format!("保存文件失败: {e}"))?;
    Ok(path)
}

/// 把 incoming 文件移动到分类目录（同卷 rename，失败则复制后删除原文件）
fn archive_file(incoming: &std::path::Path, category: &str) -> Result<std::path::PathBuf, String> {
    let now = chrono::Local::now();
    let mut dir = OcrDb::storage_root();
    dir.push(category);
    dir.push(date_path(&now));
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建分类目录失败: {e}"))?;
    let target = dir.join(
        incoming
            .file_name()
            .ok_or_else(|| "无法获取文件名".to_string())?,
    );
    match std::fs::rename(incoming, &target) {
        Ok(()) => {}
        Err(_) => {
            std::fs::copy(incoming, &target).map_err(|e| format!("复制归档失败: {e}"))?;
            let _ = std::fs::remove_file(incoming);
        }
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ext_from_url_and_magic() {
        assert_eq!(guess_ext("https://x/a.png?t=1", b"abc"), "png");
        assert_eq!(guess_ext("https://x/a.JPEG", b"abc"), "jpg");
        assert_eq!(guess_ext("https://x/file", b"%PDF-1.7"), "pdf");
        assert_eq!(guess_ext("https://x/file", b"\x89PNG\r\n\x1a\nrest"), "png");
        assert_eq!(guess_ext("https://x/file", &[0xFF, 0xD8, 0xFF]), "jpg");
        assert_eq!(guess_ext("https://x/file", b"unknown"), "img");
    }

    #[test]
    fn builtin_test_images_are_valid_jpeg() {
        assert_eq!(TEST_IMAGES.len(), 4, "应内置 4 张静态测试图");
        for (name, bytes) in TEST_IMAGES {
            assert!(bytes.len() > 10_000, "{name} 应包含真实图片内容");
            assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF], "{name} 应为 JPEG 格式");
            assert_eq!(resolve_test_image(name), Some(*bytes));
        }
        assert_eq!(resolve_test_image("nope.jpg"), None);
    }

    #[tokio::test]
    async fn fetch_rejects_empty_url() {
        assert!(fetch_media("").await.is_err());
        assert!(fetch_media("   ").await.is_err());
    }

    #[tokio::test]
    async fn fetch_data_url_base64() {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(b"\xFF\xD8\xFF\xE0test");
        let url = format!("data:image/jpeg;base64,{b64}");
        let bytes = fetch_media(&url).await.expect("data URL 应可读取");
        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF]);
    }

    #[test]
    fn authorized_checks_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer secret123".parse().unwrap(),
        );
        assert!(authorized("secret123", &headers, "{}"));
        assert!(!authorized("other", &headers, "{}"));
        assert!(authorized(
            "secret123",
            &HeaderMap::new(),
            r#"{"access_token":"secret123"}"#
        ));
        assert!(!authorized(
            "secret123",
            &HeaderMap::new(),
            r#"{"access_token":"bad"}"#
        ));
    }
}
