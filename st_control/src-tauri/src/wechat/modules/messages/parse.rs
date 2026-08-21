// ============================================================
// 聊天消息 — XML 富媒体解析
// 自 messages.rs 拆分：图片/语音/视频/表情/文件/链接/引用/转账/
// 系统/撤回等消息类型的 PC 风格显示内容渲染。
// ============================================================

use crate::wechat::modules::common;

/// 从消息 XML 解析出 PC 风格显示内容
pub(crate) fn parse_display_content(
    msg_type: i64,
    content: &str,
    is_group: bool,
    sender_from_prefix: &mut Option<String>,
) -> (String, Option<serde_json::Value>) {
    let mut body = content;
    if is_group {
        if let Some(pos) = content.find(":\n") {
            let (head, tail) = content.split_at(pos);
            if head.len() <= 64
                && !head.contains('<')
                && !head.contains(' ')
                && !tail.trim_start().starts_with("<?xml")
            {
                *sender_from_prefix = Some(head.to_string());
                body = &content[pos + 2..];
            }
        }
    }

    match msg_type {
        1 => {
            let text = body.trim_start_matches('\n');
            // mmreader 图文推送（腾讯新闻等）：local_type=1 但内容是图文卡片 XML。
            // 必须先于通用 strip_xml_tags 处理——后者会把 pub_time/tweetid/
            // play_length 等数字节点拼成 "0 1 1785143577" 这样的乱码。
            if text.contains("<mmreader>") {
                let rich = crate::wechat::media::parse_mmreader(text)
                    .and_then(|r| serde_json::to_value(r).ok());
                if rich.is_some() {
                    return (String::new(), rich);
                }
            }
            if text.starts_with("<msg>") || text.starts_with("<?xml") {
                let stripped = common::strip_xml_tags(text);
                let stripped = stripped.trim().to_string();
                if !stripped.is_empty() {
                    return (stripped, None);
                }
            }
            (text.to_string(), None)
        }
        3 => (String::new(), None),
        34 => {
            let rich = crate::wechat::media::parse_rich_content(body, 34)
                .and_then(|r| serde_json::to_value(r).ok());
            (String::new(), rich)
        }
        42 => {
            let rich = crate::wechat::media::parse_rich_content(body, 42)
                .and_then(|r| serde_json::to_value(r).ok());
            (String::new(), rich)
        }
        43 => {
            let rich = crate::wechat::media::parse_rich_content(body, 43)
                .and_then(|r| serde_json::to_value(r).ok());
            (String::new(), rich)
        }
        47 => {
            let rich = crate::wechat::media::parse_rich_content(body, 47)
                .and_then(|r| serde_json::to_value(r).ok());
            (String::new(), rich)
        }
        48 => {
            let rich = crate::wechat::media::parse_rich_content(body, 48)
                .and_then(|r| serde_json::to_value(r).ok());
            (String::new(), rich)
        }
        49 => {
            let rich = crate::wechat::media::parse_rich_content(body, 49)
                .and_then(|r| serde_json::to_value(r).ok());
            let text = match &rich {
                Some(v) => match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "file" => format!(
                        "[文件] {}",
                        v.get("title").and_then(|t| t.as_str()).unwrap_or("")
                    ),
                    "link" => format!(
                        "[链接] {}",
                        v.get("title").and_then(|t| t.as_str()).unwrap_or("")
                    ),
                    "quote" => v
                        .get("title")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                    "miniapp" => format!(
                        "[小程序] {}",
                        v.get("title").and_then(|t| t.as_str()).unwrap_or("")
                    ),
                    "channels" => format!(
                        "[视频号] {}",
                        v.get("title").and_then(|t| t.as_str()).unwrap_or("")
                    ),
                    "chatlog" => format!(
                        "[聊天记录] {}",
                        v.get("title").and_then(|t| t.as_str()).unwrap_or("")
                    ),
                    "transfer" => "[转账]".to_string(),
                    _ => String::new(),
                },
                None => {
                    let title = common::xml_tag_text(body, "title").unwrap_or_default();
                    if title.is_empty() {
                        String::new()
                    } else {
                        format!("[链接] {}", title)
                    }
                }
            };
            (text, rich)
        }
        50 => ("[语音通话]".to_string(), None),
        10000 => {
            let stripped = common::strip_xml_tags(body);
            let text = if stripped.trim().is_empty() {
                body.trim().to_string()
            } else {
                stripped.trim().to_string()
            };
            (text, None)
        }
        10002 => {
            let stripped = common::strip_xml_tags(body);
            let text = stripped.trim().to_string();
            let text = if text.is_empty() {
                "撤回了一条消息".to_string()
            } else {
                text
            };
            (text, None)
        }
        _ => {
            if !body.is_empty() && !body.contains('<') && body.len() <= 500 {
                (body.to_string(), None)
            } else {
                (
                    format!("[{}]", common::msg_type_placeholder(msg_type)),
                    None,
                )
            }
        }
    }
}
