//! 社交关系图谱导出 — 头像下载
//!
//! 头像来自微信 CDN 等跨域地址，浏览器画布无法直接使用；
//! 本模块由后端下载并转为 `data:image/...;base64,...`，供导出图安全嵌入。

/// 下载远程图片并转为 data URL（供图谱导出嵌入头像）。
/// 头像来自微信 CDN 等跨域地址，浏览器画布无法直接使用；
/// 后端下载可绕过 CORS，返回 `data:image/...;base64,...` 由前端安全绘制。
#[tauri::command]
pub async fn fetch_image_data_url(url: String) -> Result<String, String> {
    use base64::Engine as _;
    let u = url.trim().to_string();
    if !(u.starts_with("http://") || u.starts_with("https://")) {
        return Err("仅支持 http/https 图片地址".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("创建下载客户端失败: {}", e))?;
    let client_direct = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .no_proxy()
        .build()
        .unwrap_or_else(|_| client.clone());

    let bytes = match download_remote_image(&client, &u).await {
        Ok(b) => b,
        // 代理不可达时回退直连（与 LLM 客户端同策略）
        Err(_) => download_remote_image(&client_direct, &u)
            .await
            .map_err(|e| format!("下载头像失败: {}", e))?,
    };
    let mime = sniff_image_mime(&bytes);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, b64))
}

async fn download_remote_image(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("请求头像失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("下载头像失败: HTTP {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取头像失败: {}", e))?;
    if bytes.len() > 3 * 1024 * 1024 {
        return Err("头像文件过大（超过 3MB）".to_string());
    }
    Ok(bytes.to_vec())
}

fn sniff_image_mime(b: &[u8]) -> &'static str {
    if b.len() >= 8 && &b[0..8] == b"\x89PNG\r\n\x1a\n" {
        "image/png"
    } else if b.len() >= 3 && &b[0..3] == b"\xff\xd8\xff" {
        "image/jpeg"
    } else if (b.len() >= 6 && &b[0..6] == b"GIF87a") || (b.len() >= 6 && &b[0..6] == b"GIF89a") {
        "image/gif"
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        "image/webp"
    } else if b.len() >= 2 && &b[0..2] == b"BM" {
        "image/bmp"
    } else {
        "image/jpeg"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_mime_sniff() {
        assert_eq!(sniff_image_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(sniff_image_mime(b"\x89PNG\r\n\x1a\nxxxx"), "image/png");
        assert_eq!(sniff_image_mime(b"GIF89a..."), "image/gif");
        assert_eq!(sniff_image_mime(b"RIFF....WEBP"), "image/webp");
        assert_eq!(sniff_image_mime(b"BM.."), "image/bmp");
        assert_eq!(sniff_image_mime(&[0x00, 0x11, 0x22]), "image/jpeg");
    }
}
