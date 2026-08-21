// ============================================================
// 大模型管理 — IPC 命令：聊天记录持久化
// 自 handlers.rs 拆分：记录读取（文件路径转 data URL）、
// 追加/清空。
// ============================================================

use crate::llm::types::{ChatMessage, ContentPart};
use base64::Engine;
use tauri::State;

// ─── 聊天记录持久化（SQLite） ───

/// 读取某 (provider_id, model) 的聊天记录，并将文件路径转为 data URL 以便前端显示
#[tauri::command]
pub async fn get_llm_chat_history(
    provider_id: String,
    model: String,
    db: State<'_, crate::db::Database>,
) -> Result<Vec<ChatMessage>, String> {
    let rows = db
        .get_llm_chat_history(&provider_id, &model)
        .map_err(|e| format!("读取聊天记录失败: {}", e))?;
    let msgs: Vec<ChatMessage> = rows
        .into_iter()
        .map(|r| {
            let parts = r.parts_json.and_then(|json| {
                serde_json::from_str::<Vec<ContentPart>>(&json)
                    .ok()
                    .map(|parts| {
                        parts
                            .into_iter()
                            .map(|mut p| {
                                // 若含 file_path，读取文件转为 data URL
                                if let Some(fp) = &p.file_path {
                                    if let Some(data_url) = file_path_to_data_url(fp) {
                                        if p.part_type == "image_url" {
                                            p.image_url.get_or_insert_with(Default::default).url =
                                                data_url;
                                        }
                                    }
                                }
                                p
                            })
                            .collect::<Vec<_>>()
                    })
            });
            ChatMessage {
                role: r.role,
                content: r.content,
                parts,
            }
        })
        .collect();
    Ok(msgs)
}

/// 将本地文件路径转为 data: URL（用于在聊天记录中恢复附件显示）
fn file_path_to_data_url(path: &str) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let mime = match ext.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    };
    Some(format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(&data)
    ))
}

/// 追加聊天消息（用户与助手各一条）
#[tauri::command]
pub async fn append_llm_chat_messages(
    provider_id: String,
    model: String,
    messages: Vec<ChatMessage>,
    db: State<'_, crate::db::Database>,
) -> Result<(), String> {
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    for m in &messages {
        let parts_json = m
            .parts
            .as_ref()
            .map(|p| serde_json::to_string(p).unwrap_or_default());
        db.append_llm_chat_message(
            &provider_id,
            &model,
            &m.role,
            &m.content,
            parts_json.as_deref(),
            &now,
        )
        .map_err(|e| format!("保存聊天记录失败: {}", e))?;
    }
    Ok(())
}

/// 清空某 (provider_id, model) 的聊天记录
#[tauri::command]
pub async fn clear_llm_chat_history(
    provider_id: String,
    model: String,
    db: State<'_, crate::db::Database>,
) -> Result<usize, String> {
    db.clear_llm_chat_history(&provider_id, &model)
        .map_err(|e| format!("清空聊天记录失败: {}", e))
}

// ─── 代理工具调用历史（随对话持久化，按助手消息序号关联） ───

/// 保存某条助手消息的工具调用步骤（助手序号 = 该会话中第几条助手消息，从 0 起）
#[tauri::command]
pub async fn save_agent_tool_steps(
    provider_id: String,
    model: String,
    assistant_idx: i64,
    steps: Vec<serde_json::Value>,
    db: State<'_, crate::db::Database>,
) -> Result<(), String> {
    let steps_json = serde_json::to_string(&steps).map_err(|e| format!("序列化失败: {}", e))?;
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    db.save_agent_tool_steps(&provider_id, &model, assistant_idx, &steps_json, &now)
        .map_err(|e| format!("保存工具调用历史失败: {}", e))
}

/// 读取某会话全部工具调用步骤（返回 [{assistant_idx, steps}]）
#[tauri::command]
pub async fn get_agent_tool_steps(
    provider_id: String,
    model: String,
    db: State<'_, crate::db::Database>,
) -> Result<Vec<(i64, Vec<serde_json::Value>)>, String> {
    let rows = db
        .get_agent_tool_steps(&provider_id, &model)
        .map_err(|e| format!("读取工具调用历史失败: {}", e))?;
    rows.into_iter()
        .map(|(idx, json)| {
            let steps =
                serde_json::from_str(&json).map_err(|e| format!("解析工具调用历史失败: {}", e))?;
            Ok((idx, steps))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_path_to_data_url_mime_and_missing() {
        // 扩展名 → mime 映射 + base64 编码；不存在文件 → None
        let dir = std::env::temp_dir();
        let png = dir.join("hns-hist-test.png");
        std::fs::write(&png, [0x89u8, 0x50, 0x4E, 0x47]).unwrap();
        let url = file_path_to_data_url(png.to_str().unwrap()).unwrap();
        assert!(url.starts_with("data:image/png;base64,"), "png mime: {url}");
        assert!(url.ends_with("iVBORw=="), "base64 应编码 4 字节: {url}");
        // 大写扩展名（.JPG）大小写不敏感
        let jpg = dir.join("hns-hist-test.JPG");
        std::fs::write(&jpg, b"jpeg").unwrap();
        let url = file_path_to_data_url(jpg.to_str().unwrap()).unwrap();
        assert!(
            url.starts_with("data:image/jpeg;base64,"),
            "JPG mime: {url}"
        );
        // 未知扩展名 → octet-stream
        let bin = dir.join("hns-hist-test.xyz");
        std::fs::write(&bin, b"data").unwrap();
        let url = file_path_to_data_url(bin.to_str().unwrap()).unwrap();
        assert!(
            url.starts_with("data:application/octet-stream;base64,"),
            "未知扩展名: {url}"
        );
        // 不存在 → None
        assert!(file_path_to_data_url("C:/nonexistent/file.png").is_none());
        // 清理
        for f in [
            "hns-hist-test.png",
            "hns-hist-test.JPG",
            "hns-hist-test.xyz",
        ] {
            let _ = std::fs::remove_file(dir.join(f));
        }
    }
}
