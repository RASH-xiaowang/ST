// ============================================================
// 微信 CDN 原图下载 — 回退编排域
// 自 cdn_image.rs 拆分：wxid 目录解析 + CDN 回退主流程。
// ============================================================

use std::path::{Path, PathBuf};

use super::{download_original_image, is_cdn_enabled, lookup_image_cdn_info};

/// 整合 CDN 回退：查消息 XML → 下载原图 → 写缓存，返回图片字节。
/// `cache_dir` 为解码缓存目录（decoded_dir/{username}），缓存键用 fileid。
pub fn try_cdn_fallback(
    wechat_base_dir: &Path,
    decrypted_dir: &Path,
    cache_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<Vec<u8>> {
    if !is_cdn_enabled() {
        return None;
    }
    // wechat_base_dir 可能是账号目录本身（st_control 配置约定），也可能是 xwechat_files 根
    let wxid_dir = if looks_like_account_dir(wechat_base_dir) {
        wechat_base_dir.to_path_buf()
    } else {
        resolve_wxid_dir(wechat_base_dir, username)?
    };
    if !wxid_dir.is_dir() {
        return None;
    }
    let (fileid, aeskey, has_big) = lookup_image_cdn_info(decrypted_dir, username, local_id)?;
    if !has_big {
        // 仅中图 fileid：本地无文件且 CDN 网关不响应中图，快速失败
        log::info!(
            "[cdn_image] 消息缺少 cdnbigimgurl，跳过 CDN: {} local_id={}",
            username,
            local_id
        );
        return None;
    }

    // 缓存命中（同一 fileid 的原图）
    let cache_key = format!("cdn_{}.jpg", fileid);
    let cached = cache_dir.join(&cache_key);
    if let Ok(bytes) = std::fs::read(&cached) {
        if !bytes.is_empty() {
            return Some(bytes);
        }
    }

    log::info!("[cdn_image] 本地无原图，尝试 CDN 下载 fileid={}", fileid);
    let bytes = download_original_image(&wxid_dir, &fileid, &aeskey).ok()?;
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(&cached, &bytes);
    log::info!(
        "[cdn_image] CDN 原图下载成功: fileid={} bytes={}",
        fileid,
        bytes.len()
    );
    Some(bytes)
}

/// 目录名是否形如账号目录（wxid_xxx 或 wxid_xxx_f312）
fn looks_like_account_dir(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("wxid_"))
        .unwrap_or(false)
}

/// 从 xwechat_files 根目录定位账号数据目录：
/// clean wxid（wxid_xxx）→ 实际目录（wxid_xxx_f312 或 wxid_xxx）
pub fn resolve_wxid_dir(base: &Path, username: &str) -> Option<PathBuf> {
    let direct = base.join(username);
    if direct.is_dir() {
        return Some(direct);
    }
    let Ok(entries) = std::fs::read_dir(base) else {
        return None;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if (name == username
            || (name.starts_with(username) && name[username.len()..].starts_with('_')))
            && entry.path().is_dir()
        {
            return Some(entry.path());
        }
    }
    None
}
