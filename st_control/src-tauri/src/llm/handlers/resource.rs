// ============================================================
// 大模型管理 — IPC 命令：生成资源保存
// 自 handlers.rs 拆分：上传文件落盘、远程/data URL 下载保存。
// ============================================================

use base64::Engine;

/// 保存上传的文件到 st_result/llm_attachments/ 目录，返回文件绝对路径
/// 用于持久化附件使其在聊天记录中可恢复显示
#[tauri::command]
pub async fn save_uploaded_file(file_name: String, file_data: Vec<u8>) -> Result<String, String> {
    save_bytes_to_attachments(&file_name, &file_data)
}

/// 从图片地址（http/https 或 data: URL）下载并保存到资源目录，返回本地路径。
/// 用于「保存生成资源」：服务端下载可绕过浏览器跨域限制。
#[tauri::command]
pub async fn save_resource_from_url(
    url: String,
    file_name: Option<String>,
) -> Result<String, String> {
    // data: URL 直接解码 base64
    if let Some(rest) = url.strip_prefix("data:") {
        let (meta, b64data) = rest
            .split_once(',')
            .ok_or_else(|| "非法的 data URL".to_string())?;
        let buf = base64::engine::general_purpose::STANDARD
            .decode(b64data.trim())
            .map_err(|e| format!("data URL 解码失败: {}", e))?;
        let mime = meta.split(';').next().unwrap_or("image/png");
        let name = file_name.unwrap_or_else(|| format!("image.{}", ext_for_mime(mime)));
        return save_bytes_to_attachments(&name, &buf);
    }
    // 远程地址：reqwest 下载
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("下载资源失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("下载资源失败，HTTP {}", resp.status()));
    }
    let buf = resp
        .bytes()
        .await
        .map_err(|e| format!("读取资源失败: {}", e))?;
    let name = file_name
        .or_else(|| derive_name_from_url(&url))
        .unwrap_or_else(|| "image.png".into());
    save_bytes_to_attachments(&name, &buf)
}

/// 将字节写入 <应用数据目录>/llm_attachments/ 目录，返回文件绝对路径
fn save_bytes_to_attachments(file_name: &str, file_data: &[u8]) -> Result<String, String> {
    let target_dir = crate::common::wechat_data_dir().join("llm_attachments");
    std::fs::create_dir_all(&target_dir).map_err(|e| format!("创建附件目录失败: {}", e))?;

    // 生成唯一文件名：timestamp_random_original
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let mut rng: u64 = rand::random();
    if rng == 0 {
        rng = 1;
    }
    // 清理原始文件名中的不安全字符
    let safe_name: String = file_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let save_name = format!("{}_{:016x}_{}", ts, rng, safe_name);
    let save_path = target_dir.join(&save_name);

    std::fs::write(&save_path, file_data).map_err(|e| format!("写入文件失败: {}", e))?;

    Ok(save_path.to_string_lossy().to_string())
}

fn ext_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/bmp" => "bmp",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/aac" => "aac",
        "audio/opus" => "opus",
        "audio/x-m4a" | "audio/mp4" => "m4a",
        _ => "bin",
    }
}

fn derive_name_from_url(url: &str) -> Option<String> {
    let path = url.split('?').next().unwrap_or(url);
    let last = path.rsplit('/').next()?;
    if last.is_empty() || !last.contains('.') {
        None
    } else {
        Some(last.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ext_for_mime_maps_known_and_falls_back() {
        assert_eq!(ext_for_mime("image/png"), "png");
        assert_eq!(ext_for_mime("image/jpeg"), "jpg");
        assert_eq!(ext_for_mime("image/jpg"), "jpg");
        assert_eq!(ext_for_mime("image/webp"), "webp");
        assert_eq!(ext_for_mime("audio/mpeg"), "mp3");
        assert_eq!(ext_for_mime("audio/wav"), "wav");
        assert_eq!(ext_for_mime("audio/x-m4a"), "m4a");
        // 未知 mime → bin 兜底
        assert_eq!(ext_for_mime("application/octet-stream"), "bin");
        assert_eq!(ext_for_mime(""), "bin");
    }

    #[test]
    fn derive_name_from_url_extracts_filename() {
        assert_eq!(
            derive_name_from_url("https://x.com/images/photo.png"),
            Some("photo.png".to_string())
        );
        // 查询参数剥离
        assert_eq!(
            derive_name_from_url("https://x.com/a.jpg?size=large&v=2"),
            Some("a.jpg".to_string())
        );
        // 无文件名 / 无扩展名 → None
        assert!(derive_name_from_url("https://x.com/images/").is_none());
        assert!(derive_name_from_url("https://x.com/path").is_none());
        assert!(derive_name_from_url("").is_none());
    }
}
