// ============================================================
// 朋友圈图片解密模块 — 下载与解密域
// 自 sns_image.rs 拆分：CDN 直连下载 / ISAAC XOR 解密 / 嗅探。
// ============================================================

use std::time::Duration;

use super::Isaac64;

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_IMAGE_BYTES: usize = 25 * 1024 * 1024;

/// 临时诊断日志（定位朋友圈图片加载失败原因）
pub(crate) fn diag_log(msg: &str) {
    use std::io::Write;
    let log_path = crate::wechat::config::default_st_result_dir().join("moment_image.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let _ = writeln!(f, "  [sns_image] {}", msg);
    }
}

// ============ 图片嗅探 / 工具 ============

/// 从头部识别图片格式，返回 (扩展名, mime)
pub(crate) fn sniff_image(data: &[u8]) -> Option<(&'static str, &'static str)> {
    if data.len() >= 3 && data[..3] == [0xFF, 0xD8, 0xFF] {
        return Some(("jpg", "image/jpeg"));
    }
    if data.len() >= 8 && data[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some(("png", "image/png"));
    }
    if data.len() >= 6 && (&data[..6] == b"GIF87a" || &data[..6] == b"GIF89a") {
        return Some(("gif", "image/gif"));
    }
    if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Some(("webp", "image/webp"));
    }
    if data.len() >= 2 && data[..2] == [0x42, 0x4D] {
        return Some(("bmp", "image/bmp"));
    }
    if data.len() >= 12 && &data[4..8] == b"ftyp" {
        let brand = String::from_utf8_lossy(&data[8..12]).to_ascii_lowercase();
        if brand.contains("avif") || brand.contains("avis") {
            return Some(("avif", "image/avif"));
        }
        if brand.contains("heic")
            || brand.contains("heix")
            || brand.contains("hevc")
            || brand.contains("hevx")
            || brand.contains("mif1")
            || brand.contains("msf1")
        {
            return Some(("heic", "image/heic"));
        }
    }
    None
}

/// 拼接 CDN 下载 URL：强制 https，必要时追加 token/idx
pub(crate) fn normalize_cdn_url(url: &str, token: &str) -> String {
    let mut u = url.trim().replace("&amp;", "&");
    if let Some(rest) = u.strip_prefix("http://") {
        u = format!("https://{}", rest);
    }
    let token = token.trim();
    if !token.is_empty() && !u.contains("token=") {
        let connector = if u.contains('?') { '&' } else { '?' };
        u = format!("{}{}token={}&idx=1", u, connector, token);
    }
    u
}

/// 下载朋友圈图片原始数据（不校验是否已解密）
fn download_raw(url: &str, token: &str) -> Result<Vec<u8>, String> {
    let target = normalize_cdn_url(url, token);
    let client = reqwest::blocking::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .user_agent("MicroMessenger Client")
        // 直连腾讯 CDN：忽略环境/系统代理（本机残留代理配置会导致连接被拒）
        .no_proxy()
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let resp = client.get(&target).send().map_err(|e| {
        let msg = format!("下载失败 ({}): {}", target, e);
        diag_log(&msg);
        msg
    })?;
    if !resp.status().is_success() {
        let msg = format!("朋友圈图片下载失败: HTTP {} ({})", resp.status(), target);
        diag_log(&msg);
        return Err(msg);
    }
    let body = resp
        .bytes()
        .map_err(|e| format!("读取图片响应失败: {}", e))?;
    if body.len() > MAX_IMAGE_BYTES {
        let msg = format!("朋友圈图片过大 ({} bytes)", body.len());
        diag_log(&msg);
        return Err(msg);
    }
    Ok(body.to_vec())
}

/// 下载并解密朋友圈图片字节
pub(crate) fn fetch_and_decrypt(url: &str, key: &str, token: &str) -> Result<Vec<u8>, String> {
    let raw = download_raw(url, token)?;

    // 直链已返回明文图片（视频号等场景）：无需解密
    if sniff_image(&raw).is_some() {
        return Ok(raw);
    }

    let key = key.trim();
    if key.is_empty() {
        let msg = "图片数据非明文且缺少解密 key".to_string();
        diag_log(&msg);
        return Err(msg);
    }
    let seed = key.parse::<u64>().map_err(|_| {
        let msg = format!("无效的解密 key: {}", key);
        diag_log(&msg);
        msg
    })?;
    let keystream = Isaac64::new(seed).keystream(raw.len());
    let decrypted: Vec<u8> = raw
        .iter()
        .zip(keystream.iter())
        .map(|(&b, &k)| b ^ k)
        .collect();
    if sniff_image(&decrypted).is_none() {
        let msg = format!("解密后无法识别图片格式（key={} 可能失效）", key);
        diag_log(&msg);
        return Err(msg);
    }
    Ok(decrypted)
}

/// 组装 base64 data URL
pub(crate) fn data_url(bytes: &[u8], mime: &str) -> String {
    use base64::Engine as _;
    format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}
