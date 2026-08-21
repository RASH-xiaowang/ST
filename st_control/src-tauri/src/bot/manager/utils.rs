// ============================================================
// 消息通道 — 工具函数
// 自 manager.rs 拆分：账号命名、SVG 二维码、媒体扩展名嗅探。
// ============================================================

use std::path::Path;

pub(crate) fn default_account_name(bot_id: &str, user_id: &str) -> String {
    let short = bot_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(6)
        .collect::<String>();
    let owner_short = user_id
        .split('@')
        .next()
        .unwrap_or("")
        .trim_start_matches("wxid_")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(6)
        .collect::<String>();
    if owner_short.is_empty() {
        format!("微信机器人-{short}")
    } else {
        format!("微信-{owner_short}-{short}")
    }
}

/// 用 qrcode_img_content（扫码内容 URL）在本地生成 SVG 二维码，
/// 不依赖腾讯图片/CDN 接口，保证前端始终可显示。
pub(crate) fn qr_svg_data_url(content: &str) -> Result<String, String> {
    use base64::Engine;
    let code =
        qrcode::QrCode::new(content.as_bytes()).map_err(|e| format!("二维码生成失败: {e}"))?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(320, 320)
        .build();
    let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
    Ok(format!("data:image/svg+xml;base64,{b64}"))
}

pub(crate) fn sniff_ext(kind: &str, data: &[u8], file_name: Option<&str>) -> String {
    if let Some(name) = file_name {
        if let Some(ext) = Path::new(name).extension().and_then(|e| e.to_str()) {
            let ext = ext.to_lowercase();
            if matches!(
                ext.as_str(),
                "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "bmp"
                    | "mp4"
                    | "mov"
                    | "avi"
                    | "mkv"
                    | "silk"
                    | "amr"
                    | "wav"
                    | "mp3"
                    | "m4a"
                    | "ogg"
                    | "pdf"
                    | "txt"
                    | "doc"
                    | "docx"
                    | "xls"
                    | "xlsx"
                    | "zip"
                    | "rar"
                    | "7z"
            ) {
                return ext;
            }
        }
    }
    match kind {
        "image" => {
            if data.starts_with(&[0x89, b'P', b'N', b'G']) {
                "png"
            } else if data.starts_with(&[0xFF, 0xD8]) {
                "jpg"
            } else if data.starts_with(b"GIF8") {
                "gif"
            } else if data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
                "webp"
            } else {
                "img"
            }
        }
        "video" => {
            if data.len() > 8 && &data[4..8] == b"ftyp" {
                "mp4"
            } else {
                "video"
            }
        }
        "voice" => {
            if data.starts_with(b"#!SILK") {
                "silk"
            } else if data.starts_with(b"#!AMR") {
                "amr"
            } else if data.starts_with(b"RIFF") {
                "wav"
            } else if data.starts_with(&[0xFF, 0xFB]) || data.starts_with(b"ID3") {
                "mp3"
            } else {
                "voice"
            }
        }
        _ => "bin",
    }
    .to_string()
}
