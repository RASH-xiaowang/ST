//! CDN 媒体：下载解密 / 加密上传

use super::crypto;
use super::types::CdnMedia;
use crate::common::{describe_reqwest_error, truncate};

/// 统一直连客户端：显式禁用环境代理。
///
/// 终端里残留的不可达代理（如 HTTPS_PROXY=http://127.0.0.1:51081）会让
/// `reqwest::Client::new()` 自动代理 CDN 请求，导致上传/下载在隧道阶段
/// 直接失败。iLink API 客户端已做 no_proxy + 探测回退，这里保持一致。
fn direct_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .no_proxy()
        .build()
        .unwrap_or_default()
}

/// 解析下载地址：优先 full_url，否则由 encrypt_query_param 构造
pub fn resolve_download_url(cdn_base_url: &str, media: &CdnMedia) -> Option<String> {
    if let Some(full) = &media.full_url {
        let t = full.trim();
        if !t.is_empty() {
            return Some(t.to_owned());
        }
    }
    media
        .encrypt_query_param
        .as_deref()
        .filter(|p| !p.is_empty())
        .map(|p| {
            format!(
                "{cdn_base_url}/download?encrypted_query_param={}",
                urlencoding::encode(p)
            )
        })
}

async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = direct_client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("CDN 下载请求失败: {}", describe_reqwest_error(&e)))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "CDN 下载失败 HTTP {status}: {}",
            truncate(&body, 120)
        ));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("读取 CDN 响应失败: {e}"))
}

/// 下载并解密媒体
pub async fn download_and_decrypt(
    cdn_base_url: &str,
    media: &CdnMedia,
    aes_key_input: &str,
) -> Result<Vec<u8>, String> {
    let key = crypto::parse_aes_key(aes_key_input)?;
    let url =
        resolve_download_url(cdn_base_url, media).ok_or_else(|| "媒体缺少下载地址".to_string())?;
    let encrypted = fetch_bytes(&url).await?;
    crypto::decrypt(&encrypted, &key)
}

/// 下载明文媒体（无加密）
pub async fn download_plain(cdn_base_url: &str, media: &CdnMedia) -> Result<Vec<u8>, String> {
    let url =
        resolve_download_url(cdn_base_url, media).ok_or_else(|| "媒体缺少下载地址".to_string())?;
    fetch_bytes(&url).await
}

/// 加密上传到 CDN，返回 x-encrypted-param
pub async fn upload_buffer_to_cdn(
    plaintext: &[u8],
    aes_key: &[u8; 16],
    cdn_url: &str,
) -> Result<String, String> {
    let ciphertext = crypto::encrypt(plaintext, aes_key)?;
    let client = direct_client();
    let mut last_err: Option<String> = None;
    for attempt in 1..=3 {
        log::info!(
            "[ilink] CDN 上传第 {attempt} 次（密文 {} 字节）",
            ciphertext.len()
        );
        match client
            .post(cdn_url)
            .header("Content-Type", "application/octet-stream")
            .body(ciphertext.clone())
            .send()
            .await
        {
            Ok(res) => {
                let status = res.status();
                // 4xx 是客户端问题，重试无意义，立即失败并带服务端错误头/正文
                if status.is_client_error() {
                    let header_msg = res
                        .headers()
                        .get("x-error-message")
                        .and_then(|v| v.to_str().ok())
                        .map(String::from);
                    let body = res.text().await.unwrap_or_default();
                    let detail = header_msg
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| truncate(&body, 160));
                    return Err(format!(
                        "CDN 上传失败（HTTP {status}）: {}",
                        truncate(&detail, 200)
                    ));
                }
                if status.is_success() {
                    if let Some(v) = res
                        .headers()
                        .get("x-encrypted-param")
                        .and_then(|v| v.to_str().ok())
                    {
                        return Ok(v.to_string());
                    }
                    last_err = Some("上传成功但缺少 x-encrypted-param 响应头".to_string());
                } else {
                    let body = res.text().await.unwrap_or_default();
                    last_err = Some(format!("HTTP {status}: {}", truncate(&body, 120)));
                }
            }
            Err(e) => last_err = Some(format!("请求失败: {}", describe_reqwest_error(&e))),
        }
        if attempt < 3 {
            tokio::time::sleep(std::time::Duration::from_millis(500 * attempt)).await;
        }
    }
    Err(format!(
        "CDN 上传失败（第 3 次）: {}",
        last_err.unwrap_or_default()
    ))
}
