// ============================================================
// 微信图片 .dat 解密 — 独立解析层
// 自 image.rs 拆分：不依赖监控缓存的浏览/命令路径——MD5 定位、
// dat 文件查找/择优、data URL 解码、CDN 兜底。
// ============================================================

use crate::wechat::db_cache::MonitorDBCache;
use std::path::{Path, PathBuf};

use super::*;

/// 图片解析上下文（目录/密钥等稳定配置）
pub struct ImageResolveCtx<'a> {
    pub wechat_base_dir: &'a Path,
    /// 本地消息库路径（离线模式）；live 模式传 None
    pub res_db_path: Option<&'a Path>,
    /// 监控缓存（live 模式）；离线模式传 None
    pub db_cache: Option<&'a MonitorDBCache>,
    pub decrypted_dir: &'a Path,
    pub decoded_dir: &'a Path,
    pub aes_key: Option<&'a [u8]>,
    pub xor_key: u8,
}

/// 图片查询（会话 + 消息定位）
pub struct ImageQuery<'a> {
    pub username: &'a str,
    pub local_id: i64,
    pub hd: bool,
    /// true = 仅解析本地（dat + md5 变体），跳过 CDN 回退。
    /// 用于「本地 → ilink 原图 → CDN」的加载顺序编排：
    /// 调用方先在本地模式下解析，失败再走 ilink，最后才允许 CDN。
    pub skip_cdn: bool,
}

// ============ 独立解析函数（浏览/命令路径，不依赖监控缓存）============

/// 与 PC 微信一致的 attach 目录名：MD5(username) 小写 hex。
///
/// 【历史 bug】这里曾用 DefaultHasher（SipHash）冒充 MD5，
/// 导致 msg/attach/<hash> 目录永远定位失败、图片永远找不到。
pub fn attach_dir_name(username: &str) -> String {
    use md5::{Digest, Md5};
    format!("{:x}", Md5::digest(username.as_bytes()))
}

/// 从 message_resource.db 查询图片 MD5
pub fn get_image_md5_from_db(res_db_path: &Path, username: &str, local_id: i64) -> Option<String> {
    // 只读打开：可写打开会创建 -wal/-shm 并占用文件句柄，
    // 干扰 db_cache 对解密副本的原子替换
    let conn = rusqlite::Connection::open_with_flags(
        res_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;

    // 先查 ChatName2Id → chat_id
    let chat_id: i64 = conn
        .query_row(
            "SELECT rowid FROM ChatName2Id WHERE user_name = ?1",
            rusqlite::params![username],
            |row| row.get(0),
        )
        .ok()?;

    // 查 MessageResourceInfo（packed_info 已验证为 BLOB 存储）
    let row: Option<Vec<u8>> = conn
        .query_row(
            "SELECT packed_info FROM MessageResourceInfo \
             WHERE chat_id = ?1 AND message_local_id = ?2 \
             AND (message_local_type = 3 OR message_local_type % 4294967296 = 3) \
             ORDER BY message_create_time DESC LIMIT 1",
            rusqlite::params![chat_id, local_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    row.as_deref().and_then(extract_md5_from_packed_info)
}

/// 从消息分库的 `packed_info_data`（protobuf）提取图片 MD5。
///
/// 微信 4.x 的图片消息在消息表里直接携带 MD5（protobuf 字段），而
/// `message_resource.db` 解密副本长期停留在批量解密时刻（监控只按需解密
/// 消息分库），新图片在资源表里查不到。消息表副本由监控实时解密，
/// 从这里取 MD5 可彻底绕开过期资源表。
pub fn get_image_md5_from_msg_tables(
    decrypted_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<String> {
    let table = crate::wechat::modules::common::msg_table_name(username);
    let mut dbs = crate::wechat::modules::common::find_db_files(decrypted_dir, "message_");
    dbs.extend(crate::wechat::modules::common::find_db_files(
        decrypted_dir,
        "biz_message_",
    ));
    dbs.sort();
    dbs.dedup();
    dbs.retain(|p| !p.to_string_lossy().contains("monitor_cache"));
    dbs.retain(|p| crate::wechat::modules::common::is_message_shard_file(p));

    for path in dbs {
        let conn = match crate::wechat::modules::common::open_readonly_db(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if !crate::wechat::modules::common::table_exists(&conn, &table) {
            continue;
        }
        let cols = crate::wechat::modules::common::table_columns(&conn, &table);
        let packed_col = match cols
            .iter()
            .find(|c| c.to_ascii_lowercase().contains("packed"))
        {
            Some(c) => c,
            None => continue,
        };
        let sql = format!(
            "SELECT \"{}\" FROM \"{}\" WHERE local_id = ?1 \
             AND (local_type = 3 OR local_type % 4294967296 = 3) LIMIT 1",
            packed_col.replace('"', "\"\""),
            table.replace('"', "\"\""),
        );
        let blob: Option<Vec<u8>> = conn
            .query_row(&sql, rusqlite::params![local_id], |r| r.get(0))
            .ok()
            .flatten();
        if let Some(b) = blob {
            if let Some(md5) = extract_md5_from_packed_info(&b) {
                return Some(md5);
            }
        }
    }
    None
}

/// 获取图片 MD5：消息表 packed_info_data（实时副本）优先，
/// 兜底 message_resource.db（历史路径）。
pub fn get_image_md5_with_fallback(
    decrypted_dir: &Path,
    res_db_path: &Path,
    username: &str,
    local_id: i64,
) -> Option<String> {
    if let Some(m) = cached_md5(username, local_id) {
        return Some(m);
    }
    let md5 = get_image_md5_from_msg_tables(decrypted_dir, username, local_id)
        .or_else(|| get_image_md5_from_db(res_db_path, username, local_id));
    if let Some(m) = md5.as_ref() {
        store_md5(username, local_id, m.clone());
    }
    md5
}

/// 在单个 <md5(username)> 目录下扫描匹配 dat 文件
fn scan_attach_dir(dir: &Path, file_md5: &str) -> Vec<PathBuf> {
    let mut results = Vec::new();
    if !dir.is_dir() {
        return results;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let img_dir = entry.path().join("Img");
            if !img_dir.is_dir() {
                continue;
            }
            if let Ok(imgs) = std::fs::read_dir(&img_dir) {
                for img in imgs.flatten() {
                    let p = img.path();
                    if let Some(name) = p.file_stem().and_then(|n| n.to_str()) {
                        if name.starts_with(file_md5) && p.extension().is_some_and(|e| e == "dat") {
                            results.push(p);
                        }
                    }
                }
            }
        }
    }
    results
}

/// 在 attach 目录下查找 .dat 文件。
///
/// 主路径：msg/attach/<MD5(username)>/<YYYY-MM>/Img/<md5>*.dat。
/// 兜底：主目录未命中时扫描当前账号的所有会话目录——群聊中消息归属会话与
/// 文件实际存储目录不一致时仍能命中（借鉴 WeFlow 的 worker 全盘扫描思路）。
///
/// 【唯一性】严格限定在 wechat_base_dir（当前账号目录）内查找：
/// 每个账号的图片只能来自自己的目录。绝不扫描兄弟 wxid_* 账号目录——
/// 不同账号收到过同一张图时（md5 相同）跨账号扫描会命中别人账号的文件，
/// 造成图片串号（A 账号会话里显示出 B 账号的图）。
pub fn find_dat_files(wechat_base_dir: &Path, username: &str, file_md5: &str) -> Vec<PathBuf> {
    let attach = wechat_base_dir.join("msg").join("attach");
    let mut results = scan_attach_dir(&attach.join(attach_dir_name(username)), file_md5);
    if results.is_empty() {
        if let Ok(entries) = std::fs::read_dir(&attach) {
            for e in entries.flatten() {
                let dir = e.path();
                if !dir.is_dir() {
                    continue;
                }
                results = scan_attach_dir(&dir, file_md5);
                if !results.is_empty() {
                    break;
                }
            }
        }
    }
    results.sort();
    results.dedup();
    results
}

/// 选择最佳 dat 文件：_t 缩略图优先（气泡展示足够清晰且解密快、体积小），
/// 其次 _h / _w，最后原图
pub fn select_best_dat(files: &[PathBuf]) -> Option<PathBuf> {
    let mut ranked: Vec<(&Path, i32, u64)> = files
        .iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_lowercase();
            let sz = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            let rank = if name.contains("_t.") {
                0
            } else if name.contains("_h.") {
                1
            } else if name.contains("_w.") {
                2
            } else {
                3
            };
            (p.as_path(), rank, sz)
        })
        .collect();
    ranked.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| b.2.cmp(&a.2)));
    ranked.first().map(|(p, _, _)| p.to_path_buf())
}

/// 选择高清/原图 dat：无后缀原图 > _h（高清）> _w > _t（缩略图）
///
/// 用于图片查看器"查看原图"：原图/高清图清晰度高，但文件更大、解密更耗时。
pub fn select_hd_dat(files: &[PathBuf]) -> Option<PathBuf> {
    let mut ranked: Vec<(&Path, i32, u64)> = files
        .iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_lowercase();
            let sz = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            let rank = if !name.contains("_t.") && !name.contains("_h.") && !name.contains("_w.") {
                0 // 原图
            } else if name.contains("_h.") {
                1
            } else if name.contains("_w.") {
                2
            } else {
                3 // _t 缩略图
            };
            (p.as_path(), rank, sz)
        })
        .collect();
    ranked.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| b.2.cmp(&a.2)));
    ranked.first().map(|(p, _, _)| p.to_path_buf())
}

/// 按尺寸要求选择 dat：hd=true 用高清/原图，否则用缩略图（浏览默认）
pub fn pick_dat(files: &[PathBuf], hd: bool) -> Option<PathBuf> {
    if hd {
        select_hd_dat(files)
    } else {
        select_best_dat(files)
    }
}

/// 解密 dat 文件并返回 base64 data URL（带磁盘缓存，命中直接读）
pub fn decode_dat_to_data_url(
    dat_path: &Path,
    cache_dir: &Path,
    file_md5: &str,
    aes_key: Option<&[u8]>,
    xor_key: u8,
) -> Option<String> {
    std::fs::create_dir_all(cache_dir).ok();

    // 磁盘缓存命中：直接读取已解密文件
    for ext in ["jpg", "png", "gif", "webp", "bmp", "tif"] {
        let cached = cache_dir.join(format!("{}.{}", file_md5, ext));
        if cached.is_file() {
            let bytes = std::fs::read(&cached).ok()?;
            return Some(data_url(ext, &bytes));
        }
    }

    let (out, fmt) = decrypt_dat_file(dat_path, Some(cache_dir), aes_key, xor_key).ok()?;
    if fmt == "hevc" {
        // wxgf/HEVC：浏览器无法直接渲染，经系统 HEVC 解码器转码为 JPEG
        // 转码结果缓存为 {md5}.jpg，下次直接命中缓存
        #[cfg(target_os = "windows")]
        {
            let wxgf = std::fs::read(&out).ok()?;
            let jpg = crate::wechat::hevc::wxgf_to_jpeg(&wxgf)?;
            let cached = cache_dir.join(format!("{}.jpg", file_md5));
            if std::fs::write(&cached, &jpg).is_err() {
                return None;
            }
            return Some(data_url("jpg", &jpg));
        }
        #[cfg(not(target_os = "windows"))]
        {
            return None;
        }
    }
    let bytes = std::fs::read(&out).ok()?;
    Some(data_url(fmt, &bytes))
}

/// 组装 data:image/* URL
fn data_url(fmt: &str, bytes: &[u8]) -> String {
    let mime = match fmt {
        "jpg" | "jpeg" => "jpeg",
        other => other,
    };
    format!(
        "data:image/{};base64,{}",
        mime,
        crate::wechat::modules::avatar::base64_encode(bytes)
    )
}

/// 一站式解析：(会话, local_id) → base64 data URL
///
/// 供 IPC 命令调用；全部只读操作 + 解密缓存写入 decoded_dir。
///
/// 使用静态解密副本 `message_resource.db`。
pub fn resolve_message_image_data_url(ctx: &ImageResolveCtx, q: &ImageQuery) -> Option<String> {
    let wechat_base_dir = ctx.wechat_base_dir;
    let res_db_path = ctx.res_db_path?;
    let decrypted_dir = ctx.decrypted_dir;
    let decoded_dir = ctx.decoded_dir;
    let username = q.username;
    let local_id = q.local_id;
    let hd = q.hd;
    let aes_key = ctx.aes_key;
    let xor_key = ctx.xor_key;
    let file_md5 = get_image_md5_with_fallback(decrypted_dir, res_db_path, username, local_id)?;
    let dats = find_dat_files(wechat_base_dir, username, &file_md5);
    let Some(best) = pick_dat(&dats, hd) else {
        // 本地无高清/原图 → 先补查 md5 变体（originsourcemd5/hdmd5），再回退 CDN
        return local_or_cdn_data_url(ctx, q);
    };
    decode_dat_to_data_url(
        &best,
        &decoded_dir.join(username),
        &file_md5,
        aes_key,
        xor_key,
    )
}

/// 实时版：始终从监控任务实时解密缓存读取最新 `message_resource.db`，
/// 避免静态解密副本过期导致新消息图片查不到。
pub fn resolve_message_image_data_url_live(
    ctx: &ImageResolveCtx,
    q: &ImageQuery,
) -> Option<String> {
    let db_cache = ctx.db_cache?;
    let wechat_base_dir = ctx.wechat_base_dir;
    let decrypted_dir = ctx.decrypted_dir;
    let decoded_dir = ctx.decoded_dir;
    let username = q.username;
    let local_id = q.local_id;
    let hd = q.hd;
    let aes_key = ctx.aes_key;
    let xor_key = ctx.xor_key;
    // MD5 缓存命中：直接走文件解码，跳过 resource 库查询/重解
    if let Some(md5) = cached_md5(username, local_id) {
        let dats = find_dat_files(wechat_base_dir, username, &md5);
        if let Some(best) = pick_dat(&dats, hd) {
            return decode_dat_to_data_url(
                &best,
                &decoded_dir.join(username),
                &md5,
                aes_key,
                xor_key,
            );
        }
        return local_or_cdn_data_url(ctx, q);
    }
    // 优先从消息表（实时解密副本）取 MD5：不触发 resource 库重解，
    // 避免"微信写入资源库后第一个图片请求等大库解密"的秒级延迟
    if let Some(md5) = get_image_md5_from_msg_tables(decrypted_dir, username, local_id) {
        store_md5(username, local_id, md5.clone());
        let dats = find_dat_files(wechat_base_dir, username, &md5);
        if let Some(best) = pick_dat(&dats, hd) {
            return decode_dat_to_data_url(
                &best,
                &decoded_dir.join(username),
                &md5,
                aes_key,
                xor_key,
            );
        }
        return local_or_cdn_data_url(ctx, q);
    }
    let res_path = db_cache.get("message/message_resource.db").ok()??;
    let file_md5 = get_image_md5_with_fallback(decrypted_dir, &res_path, username, local_id)?;
    let dats = find_dat_files(wechat_base_dir, username, &file_md5);
    let Some(best) = pick_dat(&dats, hd) else {
        return local_or_cdn_data_url(ctx, q);
    };
    decode_dat_to_data_url(
        &best,
        &decoded_dir.join(username),
        &file_md5,
        aes_key,
        xor_key,
    )
}

/// 本地无原图时 CDN 回退，返回 base64 data URL
fn cdn_fallback_data_url(
    wechat_base_dir: &Path,
    decrypted_dir: &Path,
    decoded_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<String> {
    let (bytes, fmt) = cdn_fallback_image(
        wechat_base_dir,
        decrypted_dir,
        decoded_dir,
        username,
        local_id,
    )?;
    Some(data_url(fmt, &bytes))
}

/// 本地无原图时 CDN 回退，返回 (图片字节, 格式)
fn cdn_fallback_image(
    wechat_base_dir: &Path,
    decrypted_dir: &Path,
    decoded_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<(Vec<u8>, &'static str)> {
    let bytes = crate::wechat::cdn_image::try_cdn_fallback(
        wechat_base_dir,
        decrypted_dir,
        &decoded_dir.join(username),
        username,
        local_id,
    )?;
    let fmt = detect_image_format(&bytes);
    Some((bytes, fmt))
}

/// 给定图片 md5，定位并解密 .dat 文件，返回 (字节, MIME)。
fn decode_image_via_md5(
    wechat_base_dir: &Path,
    decoded_dir: &Path,
    username: &str,
    file_md5: &str,
    hd: bool,
    aes_key: Option<&[u8]>,
    xor_key: u8,
) -> Option<(Vec<u8>, String)> {
    let dats = find_dat_files(wechat_base_dir, username, file_md5);
    let best = pick_dat(&dats, hd)?;
    let cache_dir = decoded_dir.join(username);
    std::fs::create_dir_all(&cache_dir).ok();

    // 磁盘缓存命中
    for ext in ["jpg", "png", "gif", "webp", "bmp", "tif"] {
        let cached = cache_dir.join(format!("{}.{}", file_md5, ext));
        if cached.is_file() {
            let bytes = std::fs::read(&cached).ok()?;
            return Some((bytes, mime_of(ext).to_string()));
        }
    }

    let (out, fmt) = decrypt_dat_file(&best, Some(&cache_dir), aes_key, xor_key).ok()?;
    if fmt == "hevc" {
        #[cfg(target_os = "windows")]
        {
            let wxgf = std::fs::read(&out).ok()?;
            let jpg = crate::wechat::hevc::wxgf_to_jpeg(&wxgf)?;
            let cached = cache_dir.join(format!("{}.jpg", file_md5));
            std::fs::write(&cached, &jpg).ok()?;
            return Some((jpg, "image/jpeg".to_string()));
        }
        #[cfg(not(target_os = "windows"))]
        {
            return None;
        }
    }
    let bytes = std::fs::read(&out).ok()?;
    Some((bytes, mime_of(fmt).to_string()))
}

/// 主 md5 本地未命中时，用 XML 的其它 md5 变体（originsourcemd5/hdmd5）依次补查本地 dat 并解码（字节版）
fn decode_image_via_md5_variants(
    wechat_base_dir: &Path,
    decoded_dir: &Path,
    username: &str,
    md5s: &[String],
    hd: bool,
    aes_key: Option<&[u8]>,
    xor_key: u8,
) -> Option<(Vec<u8>, String)> {
    for m in md5s {
        if let Some(r) = decode_image_via_md5(
            wechat_base_dir,
            decoded_dir,
            username,
            m,
            hd,
            aes_key,
            xor_key,
        ) {
            return Some(r);
        }
    }
    None
}

/// 主 md5 本地未命中时，用 XML 的其它 md5 变体依次补查本地 dat 并解码（data URL 版）
fn decode_data_url_via_md5_variants(
    wechat_base_dir: &Path,
    decoded_dir: &Path,
    username: &str,
    md5s: &[String],
    hd: bool,
    aes_key: Option<&[u8]>,
    xor_key: u8,
) -> Option<String> {
    for m in md5s {
        let dats = find_dat_files(wechat_base_dir, username, m);
        if let Some(best) = pick_dat(&dats, hd) {
            if let Some(url) =
                decode_dat_to_data_url(&best, &decoded_dir.join(username), m, aes_key, xor_key)
            {
                return Some(url);
            }
        }
    }
    None
}

/// 本地主/变体 md5 均未命中时，回退 CDN 原图（data URL 版）。
///
/// skip_cdn=true 时只做本地变体解析（加载顺序编排用：本地 → ilink → CDN）。
fn local_or_cdn_data_url(ctx: &ImageResolveCtx, q: &ImageQuery) -> Option<String> {
    let wechat_base_dir = ctx.wechat_base_dir;
    let decrypted_dir = ctx.decrypted_dir;
    let decoded_dir = ctx.decoded_dir;
    let username = q.username;
    let local_id = q.local_id;
    let hd = q.hd;
    let aes_key = ctx.aes_key;
    let xor_key = ctx.xor_key;
    let md5s =
        crate::wechat::cdn_image::lookup_image_md5_variants(decrypted_dir, username, local_id);
    let local = decode_data_url_via_md5_variants(
        wechat_base_dir,
        decoded_dir,
        username,
        &md5s,
        hd,
        aes_key,
        xor_key,
    );
    if q.skip_cdn {
        return local;
    }
    local.or_else(|| {
        cdn_fallback_data_url(
            wechat_base_dir,
            decrypted_dir,
            decoded_dir,
            username,
            local_id,
        )
    })
}

/// 本地主/变体 md5 均未命中时，回退 CDN 原图（字节版）。
///
/// skip_cdn=true 时只做本地变体解析（加载顺序编排用：本地 → ilink → CDN）。
fn local_or_cdn_bytes(ctx: &ImageResolveCtx, q: &ImageQuery) -> Option<(Vec<u8>, String)> {
    let wechat_base_dir = ctx.wechat_base_dir;
    let decrypted_dir = ctx.decrypted_dir;
    let decoded_dir = ctx.decoded_dir;
    let username = q.username;
    let local_id = q.local_id;
    let hd = q.hd;
    let aes_key = ctx.aes_key;
    let xor_key = ctx.xor_key;
    let md5s =
        crate::wechat::cdn_image::lookup_image_md5_variants(decrypted_dir, username, local_id);
    let local = decode_image_via_md5_variants(
        wechat_base_dir,
        decoded_dir,
        username,
        &md5s,
        hd,
        aes_key,
        xor_key,
    );
    if q.skip_cdn {
        return local;
    }
    local.or_else(|| {
        cdn_fallback_bytes(
            wechat_base_dir,
            decrypted_dir,
            decoded_dir,
            username,
            local_id,
        )
    })
}

/// 一站式解析：(会话, local_id) → (图片字节, MIME 类型)
///
/// 供 HTTP API 媒体接口直接返回二进制内容；
/// wxgf/HEVC 自动转码为 JPEG。全部只读 + 解密缓存。
///
/// 使用静态解密副本 `message_resource.db`（仅在监控任务刷新该库时更新）。
pub fn resolve_message_image_bytes(
    ctx: &ImageResolveCtx,
    q: &ImageQuery,
) -> Option<(Vec<u8>, String)> {
    let wechat_base_dir = ctx.wechat_base_dir;
    let res_db_path = ctx.res_db_path?;
    let decrypted_dir = ctx.decrypted_dir;
    let decoded_dir = ctx.decoded_dir;
    let username = q.username;
    let local_id = q.local_id;
    let hd = q.hd;
    let aes_key = ctx.aes_key;
    let xor_key = ctx.xor_key;
    let file_md5 = get_image_md5_with_fallback(decrypted_dir, res_db_path, username, local_id)?;
    decode_image_via_md5(
        wechat_base_dir,
        decoded_dir,
        username,
        &file_md5,
        hd,
        aes_key,
        xor_key,
    )
    .or_else(|| local_or_cdn_bytes(ctx, q))
}

/// 一站式解析（实时版）：始终从监控任务的实时解密缓存读取最新 `message_resource.db`，
/// 避免静态解密副本过期导致新消息图片查不到（NOT_FOUND）。
pub fn resolve_message_image_bytes_live(
    ctx: &ImageResolveCtx,
    q: &ImageQuery,
) -> Option<(Vec<u8>, String)> {
    let db_cache = ctx.db_cache?;
    let wechat_base_dir = ctx.wechat_base_dir;
    let decrypted_dir = ctx.decrypted_dir;
    let decoded_dir = ctx.decoded_dir;
    let username = q.username;
    let local_id = q.local_id;
    let hd = q.hd;
    let aes_key = ctx.aes_key;
    let xor_key = ctx.xor_key;
    // MD5 缓存命中：直接走文件解码，跳过 resource 库查询/重解
    if let Some(md5) = cached_md5(username, local_id) {
        return decode_image_via_md5(
            wechat_base_dir,
            decoded_dir,
            username,
            &md5,
            hd,
            aes_key,
            xor_key,
        )
        .or_else(|| local_or_cdn_bytes(ctx, q));
    }
    // 优先消息表（实时副本），不触发 resource 库重解
    if let Some(md5) = get_image_md5_from_msg_tables(decrypted_dir, username, local_id) {
        store_md5(username, local_id, md5.clone());
        return decode_image_via_md5(
            wechat_base_dir,
            decoded_dir,
            username,
            &md5,
            hd,
            aes_key,
            xor_key,
        )
        .or_else(|| local_or_cdn_bytes(ctx, q));
    }
    let res_path = db_cache.get("message/message_resource.db").ok()??;
    let file_md5 = get_image_md5_with_fallback(decrypted_dir, &res_path, username, local_id)?;
    decode_image_via_md5(
        wechat_base_dir,
        decoded_dir,
        username,
        &file_md5,
        hd,
        aes_key,
        xor_key,
    )
    .or_else(|| local_or_cdn_bytes(ctx, q))
}

/// CDN 回退，返回 (图片字节, MIME)
fn cdn_fallback_bytes(
    wechat_base_dir: &Path,
    decrypted_dir: &Path,
    decoded_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<(Vec<u8>, String)> {
    let (bytes, fmt) = cdn_fallback_image(
        wechat_base_dir,
        decrypted_dir,
        decoded_dir,
        username,
        local_id,
    )?;
    Some((bytes, mime_of(fmt).to_string()))
}

fn mime_of(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 唯一性回归：兄弟账号目录里存在同名（同 md5）dat 文件时，
    /// find_dat_files 必须只返回当前账号目录内的路径，绝不跨账号串图。
    #[test]
    fn find_dat_files_stays_in_current_account() {
        let root = std::env::temp_dir().join(format!("st-uniq-img-{}", std::process::id()));
        let base = root.join("wxid_aaa");
        let sibling = root.join("wxid_bbb");
        let username = "friend@chatroom";
        let file_md5 = "0123456789abcdef0123456789abcdef";
        let dir = attach_dir_name(username);
        for b in [&base, &sibling] {
            let img = b
                .join("msg")
                .join("attach")
                .join(&dir)
                .join("2026-08")
                .join("Img");
            std::fs::create_dir_all(&img).expect("创建测试目录");
            std::fs::write(img.join(format!("{}_t.dat", file_md5)), b"x").expect("写入测试文件");
        }
        let found = find_dat_files(&base, username, file_md5);
        assert_eq!(found.len(), 1, "应只命中当前账号的文件，实际 {:?}", found);
        assert!(
            found[0].starts_with(&base),
            "命中路径必须位于当前账号目录内: {:?}",
            found[0]
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// 兜底全盘扫描同样限定当前账号：文件只存在于兄弟账号时返回空。
    #[test]
    fn find_dat_files_does_not_leak_sibling_only_file() {
        let root = std::env::temp_dir().join(format!("st-uniq-img2-{}", std::process::id()));
        let base = root.join("wxid_aaa");
        let sibling = root.join("wxid_bbb");
        let username = "friend@chatroom";
        let file_md5 = "fedcba9876543210fedcba9876543210";
        let dir = attach_dir_name(username);
        // 文件只放在兄弟账号（且放在其他会话目录，触发全盘扫描分支）
        let img = sibling
            .join("msg")
            .join("attach")
            .join(&dir)
            .join("2026-08")
            .join("Img");
        std::fs::create_dir_all(&img).expect("创建测试目录");
        std::fs::write(img.join(format!("{}.dat", file_md5)), b"x").expect("写入测试文件");
        // 当前账号目录也建出 attach 根（否则 read_dir 直接跳过）
        std::fs::create_dir_all(base.join("msg").join("attach")).expect("创建测试目录");
        let found = find_dat_files(&base, username, file_md5);
        assert!(
            found.is_empty(),
            "兄弟账号独有的文件不得被当前账号命中: {:?}",
            found
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
