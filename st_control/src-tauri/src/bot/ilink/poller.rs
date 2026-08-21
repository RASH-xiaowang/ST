//! getupdates 长轮询：游标管理 + 消息解析

use super::client::HttpApiClient;
use super::types::{
    build_base_info, GetUpdatesRequest, MessageItemType, MessageState, MessageType, WeixinMessage,
    SESSION_EXPIRED_ERRCODE,
};

/// 入站媒体信息
#[derive(Debug, Clone)]
pub struct PolledMedia {
    pub kind: String, // image | voice | file | video
    pub cdn_media: super::types::CdnMedia,
    pub aes_key: Option<String>,
    pub file_name: Option<String>,
}

/// 解析后的入站消息
#[derive(Debug, Clone)]
pub struct PolledMessage {
    pub msg_id: Option<i64>,
    pub from: String,
    pub ts: i64,
    pub body: Option<String>,
    pub media: Option<PolledMedia>,
    pub context_token: Option<String>,
}

#[derive(Debug)]
pub enum PollError {
    SessionExpired,
    Api { code: i64, msg: String },
    Other(String),
}

impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PollError::SessionExpired => write!(f, "会话已过期"),
            PollError::Api { code, msg } => write!(f, "API 错误 {code}: {msg}"),
            PollError::Other(e) => write!(f, "{e}"),
        }
    }
}

/// 轮询一次，返回（新游标, 消息列表）
pub async fn poll_once(
    client: &HttpApiClient,
    sync_buf: &str,
) -> Result<(String, Vec<PolledMessage>), PollError> {
    let req = GetUpdatesRequest {
        get_updates_buf: sync_buf.to_owned(),
        base_info: build_base_info(),
    };
    let resp = client.get_updates(&req).await.map_err(PollError::Other)?;

    if resp.ret.unwrap_or(0) != 0 || resp.errcode.unwrap_or(0) != 0 {
        let code = resp.errcode.or(resp.ret).unwrap_or(0);
        let msg = resp.errmsg.unwrap_or_default();
        if code == SESSION_EXPIRED_ERRCODE {
            return Err(PollError::SessionExpired);
        }
        return Err(PollError::Api { code, msg });
    }

    let new_buf = resp
        .get_updates_buf
        .as_deref()
        .or(resp.sync_buf.as_deref())
        .filter(|b| !b.is_empty())
        .unwrap_or(sync_buf)
        .to_owned();

    let msgs = resp.msgs.unwrap_or_default();
    let parsed = msgs
        .iter()
        .filter(|m| should_process(m))
        .map(parse_message)
        .collect();
    Ok((new_buf, parsed))
}

fn should_process(msg: &WeixinMessage) -> bool {
    if msg.message_type != Some(MessageType::User) {
        return false;
    }
    if msg.delete_time_ms.unwrap_or(0) > 0 {
        return false;
    }
    if msg.message_state == Some(MessageState::Generating) {
        return false;
    }
    true
}

fn parse_message(msg: &WeixinMessage) -> PolledMessage {
    let items = msg.item_list.as_deref().unwrap_or(&[]);
    PolledMessage {
        msg_id: msg.message_id,
        from: msg.from_user_id.clone().unwrap_or_default(),
        ts: msg.create_time_ms.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        }),
        body: extract_body(items),
        media: extract_media(items),
        context_token: msg.context_token.clone(),
    }
}

fn extract_body(items: &[super::types::MessageItem]) -> Option<String> {
    for item in items {
        if item.item_type == Some(MessageItemType::Text) {
            if let Some(text) = item.text_item.as_ref().and_then(|t| t.text.as_deref()) {
                if !text.is_empty() {
                    return Some(text.to_owned());
                }
            }
        }
        if item.item_type == Some(MessageItemType::Voice) {
            if let Some(text) = item.voice_item.as_ref().and_then(|v| v.text.as_deref()) {
                if !text.is_empty() {
                    return Some(text.to_owned());
                }
            }
        }
    }
    None
}

fn extract_media(items: &[super::types::MessageItem]) -> Option<PolledMedia> {
    for item in items {
        let media = match item.item_type? {
            MessageItemType::Image => {
                let img = item.image_item.as_ref()?;
                let aes_key = img
                    .aeskey
                    .clone()
                    .or_else(|| img.media.as_ref().and_then(|m| m.aes_key.clone()));
                PolledMedia {
                    kind: "image".to_owned(),
                    cdn_media: img.media.clone()?,
                    aes_key,
                    file_name: None,
                }
            }
            MessageItemType::Voice => {
                let v = item.voice_item.as_ref()?;
                PolledMedia {
                    kind: "voice".to_owned(),
                    cdn_media: v.media.clone()?,
                    aes_key: v.media.as_ref().and_then(|m| m.aes_key.clone()),
                    file_name: None,
                }
            }
            MessageItemType::File => {
                let f = item.file_item.as_ref()?;
                PolledMedia {
                    kind: "file".to_owned(),
                    cdn_media: f.media.clone()?,
                    aes_key: f.media.as_ref().and_then(|m| m.aes_key.clone()),
                    file_name: f.file_name.clone(),
                }
            }
            MessageItemType::Video => {
                let v = item.video_item.as_ref()?;
                PolledMedia {
                    kind: "video".to_owned(),
                    cdn_media: v.media.clone()?,
                    aes_key: v.media.as_ref().and_then(|m| m.aes_key.clone()),
                    file_name: None,
                }
            }
            _ => continue,
        };
        return Some(media);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::ilink::types::*;

    #[test]
    fn should_process_filters() {
        let user = WeixinMessage {
            message_type: Some(MessageType::User),
            ..Default::default()
        };
        assert!(should_process(&user));
        let bot = WeixinMessage {
            message_type: Some(MessageType::Bot),
            ..Default::default()
        };
        assert!(!should_process(&bot));
    }

    #[test]
    fn text_body() {
        let msg = WeixinMessage {
            item_list: Some(vec![MessageItem {
                item_type: Some(MessageItemType::Text),
                text_item: Some(TextItem {
                    text: Some("你好".into()),
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert_eq!(
            extract_body(msg.item_list.as_deref().unwrap()),
            Some("你好".into())
        );
    }

    #[test]
    fn image_media_has_aeskey() {
        let items = vec![MessageItem {
            item_type: Some(MessageItemType::Image),
            image_item: Some(ImageItem {
                aeskey: Some("0123456789abcdef0123456789abcdef".into()),
                media: Some(CdnMedia {
                    full_url: Some("https://x/img".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }];
        let media = extract_media(&items).unwrap();
        assert_eq!(media.kind, "image");
        assert!(media.aes_key.is_some());
        assert!(media.cdn_media.full_url.is_some());
    }
}
