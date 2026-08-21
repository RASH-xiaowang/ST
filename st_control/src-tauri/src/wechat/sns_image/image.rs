// ============================================================
// 朋友圈图片解密模块 — 图片解析域
// 自 sns_image.rs 拆分：下载+解密+磁盘缓存 → data URL。
// ============================================================

use std::path::{Path, PathBuf};

use md5::{Digest, Md5};

use super::{data_url, diag_log, fetch_and_decrypt, normalize_cdn_url, sniff_image};

/// 一站式：下载 + 解密 + 磁盘缓存，返回 data URL
///
/// 缓存路径：`<decoded_image_dir>/moments/<md5(标准化URL)>.<ext>`
pub fn resolve_moment_image_data_url(
    url: &str,
    key: &str,
    token: &str,
    decoded_image_dir: &Path,
) -> Option<String> {
    if url.trim().is_empty() {
        return None;
    }
    let cache_dir = decoded_image_dir.join("moments");
    std::fs::create_dir_all(&cache_dir).ok()?;

    let normalized = normalize_cdn_url(url, token);
    let cache_key = format!("{:x}", Md5::digest(normalized.as_bytes()));
    let cache_base: PathBuf = cache_dir.join(&cache_key);

    // 磁盘缓存命中
    for ext in ["jpg", "png", "gif", "webp", "bmp", "avif", "heic"] {
        let cached = cache_base.with_extension(ext);
        if cached.is_file() {
            if let Ok(bytes) = std::fs::read(&cached) {
                let mime = match ext {
                    "jpg" => "image/jpeg",
                    other => &format!("image/{}", other),
                };
                return Some(data_url(&bytes, mime));
            }
        }
    }

    let bytes = match fetch_and_decrypt(url, key, token) {
        Ok(b) => b,
        Err(e) => {
            diag_log(&format!("resolve 失败 url={} err={}", url, e));
            return None;
        }
    };
    let (ext, mime) = match sniff_image(&bytes) {
        Some(x) => x,
        None => {
            diag_log("解密结果无法识别图片格式");
            return None;
        }
    };
    let out = cache_base.with_extension(ext);
    std::fs::write(&out, &bytes).ok()?;
    Some(data_url(&bytes, mime))
}
