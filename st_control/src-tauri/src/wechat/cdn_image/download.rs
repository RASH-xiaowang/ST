// ============================================================
// 微信 CDN 原图下载 — 下载与解密域
// 自 cdn_image.rs 拆分：CDN GET 与本地/服务端 AES 解密。
// ============================================================

use std::path::Path;
use std::process::Command;

use super::{fetch_cdn_token, is_cdn_local_decrypt};

const DOWNLOAD_URL: &str = "https://wxcdn.c3o.re/download";

/// 按 fileid 从 CDN 拉取原图字节；token 过期(401/403)自动刷新一次重试。
pub fn download_original_image(
    wxid_dir: &Path,
    fileid: &str,
    aes_key_hex: &str,
) -> Result<Vec<u8>, String> {
    let fileid = fileid.trim();
    if fileid.is_empty() {
        return Err("缺少 fileid，无法从 CDN 获取原图".to_string());
    }
    let aes = aes_key_hex.trim();
    let local_decrypt = is_cdn_local_decrypt();
    // 本地解密：type=orig 不带 key，拉原始加密字节，aeskey 不出本机；
    // 服务端解密：type=orig 并把 aeskey 交给 CDN 服务代为解密。
    let url = if local_decrypt {
        format!("{}?fileid={}&type=orig", DOWNLOAD_URL, fileid)
    } else {
        format!(
            "{}?fileid={}&type=orig{}",
            DOWNLOAD_URL,
            fileid,
            if aes.is_empty() {
                String::new()
            } else {
                format!("&key={}", aes)
            }
        )
    };
    let token = fetch_cdn_token(wxid_dir)?;
    let raw = curl_get_bytes(&url, &token)?;

    // 服务端解密模式：返回的已是原图字节
    if !local_decrypt {
        return Ok(raw);
    }

    // 本地解密模式：若 CDN 直接返回了原图（如公众号图），无需解密
    let head_len = raw.len().min(16);
    if crate::wechat::image::detect_image_format(&raw[..head_len]) != "bin" {
        return Ok(raw);
    }
    if aes.is_empty() {
        return Err(
            "本地解密模式需要图片消息的 aeskey，当前消息缺少该字段（可在 设置→微信配置 切换为服务端解密）"
                .to_string(),
        );
    }
    let key = crate::wechat::image::decode_cdn_aes_key(aes).ok_or_else(|| {
        format!(
            "aeskey 无法解析（{}），可在 设置→微信配置 切换为服务端解密",
            aes
        )
    })?;
    let decrypted = crate::wechat::image::aes_ecb_decrypt_file(&key, &raw)?;
    let head_len = decrypted.len().min(16);
    if crate::wechat::image::detect_image_format(&decrypted[..head_len]) == "bin" {
        return Err(
            "本地解密结果无法识别为图片（aeskey 可能不匹配），可在 设置→微信配置 切换为服务端解密"
                .to_string(),
        );
    }
    Ok(decrypted)
}

/// curl GET 下载（--max-time 15：失效 fileid 快速失败，不拖垮图片加载队列）
fn curl_get_bytes(url: &str, token: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("curl.exe")
        .args([
            "-s",
            "-f",
            "--max-time",
            "15",
            "-H",
            &format!("Authorization: Bearer {}", token),
            url,
        ])
        .output()
        .map_err(|e| format!("调用 curl 下载失败: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "CDN 原图下载失败: curl exit={} {}",
            output.status,
            stderr.chars().take(120).collect::<String>()
        ));
    }
    if output.stdout.is_empty() {
        return Err("CDN 原图下载返回为空".to_string());
    }
    Ok(output.stdout)
}
