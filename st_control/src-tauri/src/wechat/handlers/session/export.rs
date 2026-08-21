// ============================================================
// 微信 IPC — 会话导出域（export_session_messages / batch_export_sessions）
// 依赖：helpers（run_blocking/exports_dir/write_export_file*）/
//   modules::messages（get_conversation_messages / ChatMessage）/ config
// ============================================================

use crate::wechat::handlers::helpers;
use crate::wechat::modules::messages::ChatMessage;

#[tauri::command]
pub async fn export_session_messages(
    username: String,
    format: String,
    count: usize,
    path: Option<String>,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let self_username = cfg.wxid().unwrap_or_default();
        let all_msgs =
            collect_messages_for_export(&cfg.decrypted_dir, &username, &self_username, count)?;
        let msg_count = all_msgs.len();
        let imgs = if format == "html" {
            collect_export_images(&cfg, &username, &all_msgs)
        } else {
            std::collections::HashMap::new()
        };
        let text = format_messages(all_msgs, &format, &imgs)?;
        let ext = if format == "html" {
            "html"
        } else if format == "excel" {
            "xls"
        } else if format == "csv" {
            "csv"
        } else {
            "txt"
        };
        // 用户指定保存路径（前端保存弹窗选择）→ 直接写入该路径
        if let Some(p) = path.as_deref().filter(|p| !p.trim().is_empty()) {
            let filepath =
                helpers::write_export_file_at(std::path::Path::new(p), &text, format == "csv")?;
            let filename = filepath
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            log::info!(
                "[export] 导出成功: {} ({}条)",
                filepath.display(),
                msg_count
            );
            return Ok(serde_json::json!({
                "path": filepath.to_string_lossy().to_string(),
                "filename": filename,
                "count": msg_count,
            }));
        }
        let export_dir = cfg
            .decrypted_dir
            .parent()
            .unwrap_or(&cfg.decrypted_dir)
            .join("exports");
        std::fs::create_dir_all(&export_dir).map_err(|e| format!("创建导出目录失败: {}", e))?;
        let sanitized_name: String = if username.ends_with("@chatroom") {
            username
                .trim_end_matches("@chatroom")
                .chars()
                .take(16)
                .collect()
        } else {
            username.chars().take(16).collect()
        };
        let filename = format!(
            "{}_{}_{}.{}",
            sanitized_name,
            helpers::chrono_now(),
            if count == 0 {
                "all".to_string()
            } else {
                count.to_string()
            },
            ext
        );
        let filepath = export_dir.join(&filename);
        std::fs::write(&filepath, text.as_bytes()).map_err(|e| format!("写入文件失败: {}", e))?;
        log::info!(
            "[export] 导出成功: {} ({}条)",
            filepath.display(),
            msg_count
        );
        Ok(serde_json::json!({
            "path": filepath.to_string_lossy().to_string(),
            "filename": filename,
            "count": msg_count,
        }))
    })
    .await
}

#[tauri::command]
pub async fn batch_export_sessions(
    usernames: Vec<String>,
    format: String,
    dir: Option<String>,
) -> Result<serde_json::Value, String> {
    if usernames.is_empty() {
        return Err("未选择要导出的会话".to_string());
    }
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let self_username = cfg.wxid().unwrap_or_default();
        // 用户指定保存目录（前端目录选择）→ 写入该目录；否则用默认导出目录
        let export_dir = if let Some(d) = dir.as_deref().filter(|d| !d.trim().is_empty()) {
            let p = std::path::PathBuf::from(d);
            std::fs::create_dir_all(&p).map_err(|e| format!("创建导出目录失败: {}", e))?;
            p
        } else {
            helpers::exports_dir(&cfg)?
        };
        let ext = if format == "html" { "html" } else if format == "csv" { "csv" } else { "txt" };
        let mut files = Vec::new();
        let mut total_msgs = 0usize;
        for username in &usernames {
            let msgs = match collect_messages_for_export(&cfg.decrypted_dir, username, &self_username, 0) {
                Ok(m) => m,
                Err(e) => {
                    log::warn!("[data-mgmt] 收集 {} 消息失败: {}", username, e);
                    files.push(serde_json::json!({ "username": username, "error": e }));
                    continue;
                }
            };
            let count = msgs.len();
            let imgs = if format == "html" {
                collect_export_images(&cfg, username, &msgs)
            } else {
                std::collections::HashMap::new()
            };
            let text = format_messages(msgs, &format, &imgs)?;
            let sanitized: String = username.trim_end_matches("@chatroom").chars().take(16).collect();
            let filename = format!("{}_{}_all.{}", sanitized, helpers::chrono_now(), ext);
            match helpers::write_export_file(&export_dir, &filename, &text, format == "csv") {
                Ok(_) => {
                    total_msgs += count;
                    files.push(serde_json::json!({ "username": username, "filename": filename, "count": count }));
                }
                Err(e) => files.push(serde_json::json!({ "username": username, "error": e })),
            }
        }
        Ok(serde_json::json!({ "dir": export_dir.to_string_lossy().to_string(), "files": files, "sessions": usernames.len(), "total_messages": total_msgs }))
    })
    .await
}

// ─── 内部辅助（消息收集 + 格式化） ───

fn collect_messages_for_export(
    decrypted_dir: &std::path::Path,
    username: &str,
    self_username: &str,
    count: usize,
) -> Result<Vec<ChatMessage>, String> {
    if count == 0 {
        let mut all_msgs = Vec::new();
        let mut cursor: Option<i64> = None;
        loop {
            let page = crate::wechat::modules::messages::get_conversation_messages(
                decrypted_dir,
                username,
                self_username,
                cursor,
                100,
            )?;
            if page.messages.is_empty() {
                break;
            }
            cursor = Some(page.next_cursor);
            all_msgs.extend(page.messages);
            if !page.has_more || all_msgs.len() > 50000 {
                break;
            }
        }
        Ok(all_msgs)
    } else {
        let count = count.clamp(1, 50000);
        let page = crate::wechat::modules::messages::get_conversation_messages(
            decrypted_dir,
            username,
            self_username,
            None,
            count,
        )?;
        let mut all_msgs = page.messages;
        let mut cursor = Some(page.next_cursor);
        let mut has_more = page.has_more;
        while all_msgs.len() < count && has_more {
            let next = crate::wechat::modules::messages::get_conversation_messages(
                decrypted_dir,
                username,
                self_username,
                cursor,
                (count - all_msgs.len()).clamp(1, 100),
            )?;
            if next.messages.is_empty() {
                break;
            }
            cursor = Some(next.next_cursor);
            has_more = next.has_more;
            all_msgs.extend(next.messages);
            if all_msgs.len() > 50000 {
                break;
            }
        }
        Ok(all_msgs)
    }
}

/// 收集导出所需的图片字节（msg_type=3 消息 → base64 可嵌入 HTML）
fn collect_export_images(
    cfg: &crate::wechat::config::WeChatConfig,
    username: &str,
    msgs: &[ChatMessage],
) -> std::collections::HashMap<i64, (Vec<u8>, String)> {
    let aes_key: Option<Vec<u8>> = cfg
        .image_aes_key
        .as_ref()
        .filter(|k| k.len() == 16)
        .map(|k| k.as_bytes().to_vec());
    let xor_key = cfg.image_xor_key;
    let base_dir = cfg.wechat_base_dir.clone();
    let decrypted_dir = cfg.decrypted_dir.clone();
    let decoded_dir = cfg.decoded_image_dir.clone();
    let res_db = cfg
        .decrypted_dir
        .join("message")
        .join("message_resource.db");
    let mut out = std::collections::HashMap::new();
    let ctx = crate::wechat::image::ImageResolveCtx {
        wechat_base_dir: &base_dir,
        res_db_path: Some(&res_db),
        db_cache: None,
        decrypted_dir: &decrypted_dir,
        decoded_dir: &decoded_dir,
        aes_key: aes_key.as_deref(),
        xor_key,
    };
    for m in msgs {
        if m.msg_type != 3 {
            continue;
        }
        if let Some((bytes, mime)) = crate::wechat::image::resolve_message_image_bytes(
            &ctx,
            &crate::wechat::image::ImageQuery {
                username,
                local_id: m.local_id,
                hd: true,
                skip_cdn: false,
            },
        ) {
            out.insert(m.local_id, (bytes, mime));
        }
    }
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn format_messages(
    msgs: Vec<ChatMessage>,
    format: &str,
    imgs: &std::collections::HashMap<i64, (Vec<u8>, String)>,
) -> Result<String, String> {
    match format {
        "excel" => {
            // Excel 兼容的 HTML table（.xls 扩展名，Excel/WPS 可直接打开）
            let mut rows = String::new();
            for m in &msgs {
                let sender = if m.sender_name.is_empty() {
                    if m.is_self {
                        "我".to_string()
                    } else {
                        m.sender_username.clone()
                    }
                } else {
                    m.sender_name.clone()
                };
                rows.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    html_escape(&m.time),
                    html_escape(&sender),
                    html_escape(&m.type_label),
                    html_escape(&m.text),
                ));
            }
            Ok(format!(
                r#"<html xmlns:x="urn:schemas-microsoft-com:office:excel"><head><meta charset="utf-8"></head>
<body><table border="1"><tr><th>时间</th><th>发送者</th><th>类型</th><th>内容</th></tr>{}</table></body></html>"#,
                rows
            ))
        }
        "html" => {
            let mut body = String::new();
            let mut last_day = String::new();
            for m in &msgs {
                // 日期分隔
                let day = m.time.split(' ').next().unwrap_or("").to_string();
                if day != last_day {
                    last_day = day.clone();
                    body.push_str(&format!(
                        "<div class=\"date-divider\"><span>{}</span></div>",
                        html_escape(&day)
                    ));
                }
                if m.is_notice {
                    body.push_str(&format!(
                        "<div class=\"notice\">{}</div>",
                        html_escape(&m.text)
                    ));
                    continue;
                }
                let sender = if m.sender_name.is_empty() {
                    if m.is_self {
                        "我".to_string()
                    } else {
                        m.sender_username.clone()
                    }
                } else {
                    m.sender_name.clone()
                };
                let side = if m.is_self { "right" } else { "left" };
                let mut content = String::new();
                if m.msg_type == 3 {
                    if let Some((bytes, mime)) = imgs.get(&m.local_id) {
                        let b64 = crate::wechat::modules::avatar::base64_encode(bytes);
                        content.push_str(&format!(
                            "<img class=\"msg-img\" src=\"data:{};base64,{}\" alt=\"图片\"/>",
                            mime, b64
                        ));
                    } else {
                        content.push_str("<span class=\"muted\">[图片]</span>");
                    }
                } else if let Some(rich) = &m.rich {
                    let rtype = rich.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    let g = |k: &str| -> String {
                        rich.get(k)
                            .and_then(|v| v.as_str())
                            .map(html_escape)
                            .unwrap_or_default()
                    };
                    match rtype {
                        "link" => {
                            content.push_str(&format!(
                                "<div class=\"link-card\"><a href=\"{}\" target=\"_blank\"><b>{}</b></a><p>{}</p><span class=\"muted\">{}</span></div>",
                                g("url"), g("title"), g("des"), g("source")
                            ));
                        }
                        "file" => {
                            let size =
                                rich.get("file_size").and_then(|v| v.as_i64()).unwrap_or(0) / 1024;
                            content.push_str(&format!(
                                "<div class=\"file-card\">📄 {} <span class=\"muted\">({} · {} KB)</span></div>",
                                g("title"), g("file_ext"), size
                            ));
                        }
                        "quote" => {
                            content.push_str(&format!(
                                "<div class=\"quote-card\"><div class=\"quote-ref\">{}</div><p>{}</p></div><p>{}</p>",
                                g("ref_name"), g("ref_content"), g("title")
                            ));
                        }
                        "miniapp" => {
                            content.push_str(&format!(
                                "<div class=\"link-card\"><a href=\"{}\" target=\"_blank\"><b>🟢 {}</b></a><span class=\"muted\">{}</span></div>",
                                g("url"), g("title"), g("source")
                            ));
                        }
                        "transfer" => {
                            content.push_str(&format!(
                                "<div class=\"pay-card\">💰 {}<span class=\"muted\">{} {}</span><p>{}</p></div>",
                                g("title"), g("paysubtype"), g("fee_desc"), g("pay_memo")
                            ));
                        }
                        "channels" => {
                            content.push_str(&format!(
                                "<div class=\"muted\">🎬 [视频号] {}</div>",
                                g("title")
                            ));
                        }
                        "emoji" => {
                            content.push_str(&format!(
                                "<span class=\"emoji\">[表情] {} {}</span>",
                                g("emoji_url"),
                                g("description")
                            ));
                        }
                        "video" => {
                            let d = rich.get("duration").and_then(|v| v.as_i64()).unwrap_or(0);
                            content.push_str(&format!(
                                "<span class=\"muted\">🎬 [视频] {}s</span>",
                                d
                            ));
                        }
                        "voice" => {
                            let d = rich.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            content.push_str(&format!(
                                "<span class=\"muted\">🎤 [语音] {:.0}s</span>",
                                d
                            ));
                        }
                        "chatlog" => {
                            content.push_str(&format!(
                                "<div class=\"link-card\"><b>{}</b><p>{}</p></div>",
                                g("title"),
                                g("des")
                            ));
                        }
                        "newsfeed" => {
                            content.push_str(&format!(
                                "<span class=\"muted\">📰 [图文] {}</span>",
                                g("name")
                            ));
                        }
                        _ => content.push_str(&html_escape(&m.text)),
                    }
                } else {
                    content.push_str(&html_escape(&m.text));
                }
                body.push_str(&format!(
                    "<div class=\"row {}\"><div class=\"bubble\"><div class=\"sender\">{}</div><div class=\"content\">{}</div><div class=\"time\">{}</div></div></div>",
                    side,
                    html_escape(&sender),
                    content,
                    html_escape(&m.time)
                ));
            }
            Ok(format!(
                r#"<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>微信聊天记录导出</title>
<style>
body{{font-family:-apple-system,'Segoe UI','Microsoft YaHei',sans-serif;background:#ededed;margin:0;padding:24px 12px;color:#1f1f1f}}
.wrap{{max-width:760px;margin:0 auto}}
.hd{{text-align:center;padding:16px 0 8px}}
.hd h1{{font-size:18px;margin:0 0 4px}}
.hd p{{font-size:12px;color:#888;margin:0}}
.date-divider{{text-align:center;margin:18px 0 10px}}
.date-divider span{{background:#c8c8c8;color:#fff;font-size:11px;padding:2px 12px;border-radius:999px}}
.notice{{text-align:center;color:#999;font-size:12px;margin:8px 0}}
.row{{display:flex;margin:10px 0}}
.row.right{{justify-content:flex-end}}
.bubble{{max-width:72%;padding:9px 12px;border-radius:8px;background:#fff;position:relative;box-shadow:0 1px 2px rgba(0,0,0,.08)}}
.row.right .bubble{{background:#95ec69}}
.sender{{font-size:11px;color:#576b95;margin-bottom:3px}}
.row.right .sender{{text-align:right}}
.content{{font-size:14px;line-height:1.5;word-break:break-word}}
.time{{font-size:10px;color:#aaa;margin-top:4px;text-align:right}}
.msg-img{{max-width:100%;border-radius:6px;display:block}}
.muted{{color:#999;font-size:12px}}
.link-card{{border:1px solid #eee;border-radius:6px;padding:8px 10px;background:#fafafa}}
.link-card b{{font-size:13px;color:#333;text-decoration:none}}
.link-card p{{font-size:12px;color:#666;margin:4px 0 0}}
.file-card{{background:#f0f6ff;border-radius:6px;padding:8px 10px;font-size:13px}}
.quote-card{{border-left:3px solid #ccc;padding-left:8px;background:#f7f7f7;border-radius:4px;margin-bottom:6px}}
.quote-ref{{font-size:11px;color:#576b95}}
.pay-card{{background:#fff7e6;border-radius:6px;padding:8px 10px}}
</style></head><body><div class="wrap">
<div class="hd"><h1>微信聊天记录</h1><p>共 {} 条消息 · 导出时间 {}</p></div>
{}</div></body></html>"#,
                msgs.len(),
                html_escape(&crate::wechat::handlers::helpers::chrono_now()),
                body
            ))
        }
        "csv" => {
            let mut lines = vec!["时间,发送者,类型,内容".to_string()];
            for m in &msgs {
                let sender = if m.sender_name.is_empty() {
                    if m.is_self {
                        "我".to_string()
                    } else {
                        m.sender_username.clone()
                    }
                } else {
                    m.sender_name.clone()
                };
                let content = m.text.replace('"', "\"\"");
                let msg_type = if m.msg_type == 1 {
                    "文本".to_string()
                } else {
                    m.type_label.clone()
                };
                lines.push(format!(
                    "\"{}\",\"{}\",\"{}\",\"{}\"",
                    m.time, sender, msg_type, content
                ));
            }
            Ok(lines.join("\n"))
        }
        // txt 等文本格式统一走通用导出路径
        _ => {
            let mut lines = vec![format!("消息导出 ({})", msgs.len())];
            lines.push("=".repeat(48));
            lines.push(String::new());
            for m in &msgs {
                let sender = if m.sender_name.is_empty() {
                    if m.is_self {
                        "我".to_string()
                    } else {
                        m.sender_username.clone()
                    }
                } else {
                    m.sender_name.clone()
                };
                let prefix = format!("[{}] {}: ", m.time, sender);
                lines.push(format!("{}{}", prefix, m.text));
            }
            Ok(lines.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 HTML 导出（真实消息 + 图片嵌入）
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_html_export() {
        let Some(cfg) = crate::wechat::config::WeChatConfig::load().ok() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let self_username = cfg.wxid().unwrap_or_default();
        let usernames = crate::wechat::annual::load_session_usernames(&cfg.decrypted_dir);
        let Some(username) = usernames.first() else {
            eprintln!("无会话，跳过");
            return;
        };
        let msgs = collect_messages_for_export(&cfg.decrypted_dir, username, &self_username, 50)
            .unwrap_or_default();
        if msgs.is_empty() {
            eprintln!("无消息，跳过");
            return;
        }
        let imgs = collect_export_images(&cfg, username, &msgs);
        let html = format_messages(msgs, "html", &imgs).expect("HTML 导出失败");
        println!("HTML 长度: {} 字节，图片数: {}", html.len(), imgs.len());
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("date-divider"));
        if !imgs.is_empty() {
            assert!(html.contains("data:image/"));
        }
        // 写临时文件供人工检查
        let out = std::env::temp_dir().join("wx_html_export_test.html");
        std::fs::write(&out, &html).unwrap();
        println!("已写入: {}", out.display());
    }
}
