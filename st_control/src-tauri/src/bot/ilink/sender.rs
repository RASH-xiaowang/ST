//! 消息发送：getconfig → sendtyping → sendmessage（文本 / 媒体）

use base64::Engine;
use md5::Digest;
use rand::RngCore;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::cdn;
use super::client::HttpApiClient;
use super::crypto;
use super::types::{
    build_base_info, CdnMedia, FileItem, GetUploadUrlRequest, ImageItem, MessageItem,
    MessageItemType, MessageState, MessageType, SendMessageRequest, SendTypingRequest,
    TypingStatus, UploadMediaType, VideoItem, VoiceItem, WeixinMessage,
};

const TEXT_CHUNK_LIMIT: usize = 4000;

pub struct Sender {
    pub client: Arc<HttpApiClient>,
    pub cdn_base_url: String,
    typing_tickets: Mutex<HashMap<String, (String, Instant)>>,
}

impl Sender {
    pub fn new(client: Arc<HttpApiClient>, cdn_base_url: String) -> Self {
        Self {
            client,
            cdn_base_url,
            typing_tickets: Mutex::new(HashMap::new()),
        }
    }

    pub async fn send_text(
        &self,
        to: &str,
        text: &str,
        context_token: Option<&str>,
    ) -> Result<String, String> {
        if text.is_empty() {
            return Err("发送内容为空".to_string());
        }
        let ticket = self.ensure_typing_ticket(to, context_token).await;
        self.typing(to, &ticket, TypingStatus::Typing).await.ok();

        let client_id = generate_client_id();
        let chunks = split_text(text, TEXT_CHUNK_LIMIT);
        for (i, chunk) in chunks.iter().enumerate() {
            let cid = if i == 0 {
                client_id.clone()
            } else {
                generate_client_id()
            };
            let mut last_err = String::new();
            let mut sent = false;
            // 先带 context_token 发送；若失败（历史 token 可能失效）则去掉 token 重试一次
            for attempt in 0..2 {
                let token = if attempt == 0 { context_token } else { None };
                if attempt == 1 && context_token.is_none() {
                    break;
                }
                let req = build_text_req(to, chunk, &cid, token);
                match self.client.send_message(&req).await {
                    Ok(()) => {
                        sent = true;
                        break;
                    }
                    Err(e) => last_err = e,
                }
            }
            if !sent {
                self.typing(to, &ticket, TypingStatus::Cancel).await.ok();
                return Err(last_err);
            }
        }
        self.typing(to, &ticket, TypingStatus::Cancel).await.ok();
        Ok(client_id)
    }

    /// 发送本地文件（按扩展名路由图片/视频/文件/语音）
    pub async fn send_media(
        &self,
        to: &str,
        path: &Path,
        context_token: Option<&str>,
    ) -> Result<String, String> {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file.bin")
            .to_owned();
        let media_type = media_type_from_filename(&filename);
        let plaintext = tokio::fs::read(path)
            .await
            .map_err(|e| format!("读取文件失败: {e}"))?;
        if plaintext.is_empty() {
            return Err("文件为空".to_string());
        }

        // 明文 MD5 + 随机 AES key + 密文大小
        let mut hasher = md5::Md5::new();
        hasher.update(&plaintext);
        let rawfilemd5 = format!("{:x}", hasher.finalize());
        let filesize = crypto::padded_size(plaintext.len()) as u64;
        let filekey = random_hex(16);
        let mut aes_key = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut aes_key);
        let aes_key_hex = hex::encode(aes_key);
        log::info!(
            "[ilink] 上传媒体 {filename}（type={media_type:?} raw={} cipher={} md5={rawfilemd5}）→ {to}",
            plaintext.len(),
            filesize,
        );

        let upload_resp = self
            .client
            .get_upload_url(&GetUploadUrlRequest {
                filekey: filekey.clone(),
                media_type,
                to_user_id: to.to_owned(),
                rawsize: plaintext.len() as u64,
                rawfilemd5,
                filesize,
                no_need_thumb: Some(true),
                thumb_rawsize: None,
                thumb_rawfilemd5: None,
                thumb_filesize: None,
                aeskey: aes_key_hex.clone(),
                base_info: build_base_info(),
            })
            .await
            .map_err(|e| format!("获取上传地址失败: {e}"))?;

        let cdn_url = if let Some(full) = upload_resp
            .upload_full_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            normalize_upload_url(full)
        } else if let Some(param) = upload_resp.upload_param.as_deref() {
            format!(
                "{}/upload?encrypted_query_param={}&filekey={}",
                self.cdn_base_url.trim_end_matches('/'),
                urlencoding::encode(param),
                urlencoding::encode(&filekey),
            )
        } else {
            return Err("getuploadurl 未返回上传地址".to_string());
        };

        let encrypt_query_param = cdn::upload_buffer_to_cdn(&plaintext, &aes_key, &cdn_url).await?;
        let aes_key_base64 =
            base64::engine::general_purpose::STANDARD.encode(aes_key_hex.as_bytes());

        let ticket = self.ensure_typing_ticket(to, context_token).await;
        self.typing(to, &ticket, TypingStatus::Typing).await.ok();

        let client_id = generate_client_id();
        let item = build_media_item(
            media_type,
            &filename,
            &encrypt_query_param,
            &aes_key_base64,
            plaintext.len(),
            filesize,
        );
        log::info!("[ilink] CDN 上传成功（{filename}），开始发送媒体消息 → {to}");
        let mut last_err = String::new();
        let mut sent = false;
        for attempt in 0..2 {
            let token = if attempt == 0 { context_token } else { None };
            if attempt == 1 && context_token.is_none() {
                break;
            }
            let req = SendMessageRequest {
                msg: WeixinMessage {
                    from_user_id: Some(String::new()),
                    to_user_id: Some(to.to_owned()),
                    client_id: Some(client_id.clone()),
                    message_type: Some(MessageType::Bot),
                    message_state: Some(MessageState::Finish),
                    item_list: Some(vec![item.clone()]),
                    context_token: token.map(String::from),
                    ..Default::default()
                },
                base_info: build_base_info(),
            };
            match self.client.send_message(&req).await {
                Ok(()) => {
                    sent = true;
                    break;
                }
                Err(e) => last_err = e,
            }
        }
        self.typing(to, &ticket, TypingStatus::Cancel).await.ok();
        if !sent {
            return Err(format!("发送媒体消息失败: {last_err}"));
        }
        Ok(client_id)
    }

    async fn ensure_typing_ticket(&self, user_id: &str, context_token: Option<&str>) -> String {
        {
            let map = self
                .typing_tickets
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            if let Some((ticket, at)) = map.get(user_id) {
                if at.elapsed() < Duration::from_secs(12 * 3600) {
                    return ticket.clone();
                }
            }
        }
        match self.client.get_config(user_id, context_token).await {
            Ok(resp) => {
                if let Some(ticket) = resp.typing_ticket.filter(|t| !t.is_empty()) {
                    let mut map = self
                        .typing_tickets
                        .lock()
                        .unwrap_or_else(|p| p.into_inner());
                    map.insert(user_id.to_owned(), (ticket.clone(), Instant::now()));
                    ticket
                } else {
                    String::new()
                }
            }
            Err(_) => String::new(),
        }
    }

    async fn typing(
        &self,
        user_id: &str,
        ticket: &str,
        status: TypingStatus,
    ) -> Result<(), String> {
        let req = SendTypingRequest {
            ilink_user_id: user_id.to_owned(),
            typing_ticket: if ticket.is_empty() {
                None
            } else {
                Some(ticket.to_owned())
            },
            status,
            base_info: build_base_info(),
        };
        self.client.send_typing(&req).await
    }
}

fn build_text_req(
    to: &str,
    text: &str,
    client_id: &str,
    context_token: Option<&str>,
) -> SendMessageRequest {
    SendMessageRequest {
        msg: WeixinMessage {
            from_user_id: Some(String::new()),
            to_user_id: Some(to.to_owned()),
            client_id: Some(client_id.to_owned()),
            message_type: Some(MessageType::Bot),
            message_state: Some(MessageState::Finish),
            item_list: Some(vec![MessageItem {
                item_type: Some(MessageItemType::Text),
                text_item: Some(super::types::TextItem {
                    text: Some(text.to_owned()),
                }),
                ..Default::default()
            }]),
            context_token: context_token.map(String::from),
            ..Default::default()
        },
        base_info: build_base_info(),
    }
}

fn build_media_item(
    media_type: UploadMediaType,
    filename: &str,
    encrypt_query_param: &str,
    aes_key_base64: &str,
    raw_size: usize,
    cipher_size: u64,
) -> MessageItem {
    let media = CdnMedia {
        encrypt_query_param: Some(encrypt_query_param.to_owned()),
        aes_key: Some(aes_key_base64.to_owned()),
        encrypt_type: Some(1),
        full_url: None,
    };
    match media_type {
        UploadMediaType::Image => MessageItem {
            item_type: Some(MessageItemType::Image),
            image_item: Some(ImageItem {
                media: Some(media),
                mid_size: Some(cipher_size as i64),
                ..Default::default()
            }),
            ..Default::default()
        },
        UploadMediaType::Video => MessageItem {
            item_type: Some(MessageItemType::Video),
            video_item: Some(VideoItem {
                media: Some(media),
                video_size: Some(cipher_size as i64),
                ..Default::default()
            }),
            ..Default::default()
        },
        UploadMediaType::Voice => MessageItem {
            item_type: Some(MessageItemType::Voice),
            voice_item: Some(VoiceItem {
                media: Some(media),
                playtime: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        },
        UploadMediaType::File => MessageItem {
            item_type: Some(MessageItemType::File),
            file_item: Some(FileItem {
                media: Some(media),
                file_name: Some(filename.to_owned()),
                len: Some(raw_size.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        },
    }
}

fn media_type_from_filename(filename: &str) -> UploadMediaType {
    let lower = filename.to_lowercase();
    if lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".bmp")
        || lower.ends_with(".heic")
        || lower.ends_with(".heif")
    {
        UploadMediaType::Image
    } else if lower.ends_with(".mp4")
        || lower.ends_with(".mov")
        || lower.ends_with(".avi")
        || lower.ends_with(".mkv")
        || lower.ends_with(".webm")
        || lower.ends_with(".flv")
    {
        UploadMediaType::Video
    } else {
        // 音频（mp3/wav/silk/amr 等）与其余类型统一按「文件附件」发送，
        // 与官方 openclaw-weixin 一致：该通道不支持语音消息发送
        UploadMediaType::File
    }
}

/// 补全协议相对地址：服务端偶发返回 `//host/path`，reqwest 需要完整 scheme
fn normalize_upload_url(full: &str) -> String {
    let t = full.trim();
    if let Some(rest) = t.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        t.to_owned()
    }
}

fn split_text(text: &str, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if current.chars().count() >= limit {
            out.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn generate_client_id() -> String {
    format!("openclaw-weixin-{}", uuid::Uuid::new_v4().simple())
}

fn random_hex(n: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..n).map(|_| format!("{:02x}", rng.gen::<u8>())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_text_chunks() {
        let chunks = split_text(&"a".repeat(9000), 4000);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 4000);
        assert_eq!(chunks[2].len(), 1000);
    }

    #[test]
    fn media_type_routing() {
        assert_eq!(media_type_from_filename("a.PNG"), UploadMediaType::Image);
        assert_eq!(media_type_from_filename("a.mp4"), UploadMediaType::Video);
        assert_eq!(media_type_from_filename("a.silk"), UploadMediaType::File);
        assert_eq!(media_type_from_filename("a.mp3"), UploadMediaType::File);
        assert_eq!(media_type_from_filename("a.pdf"), UploadMediaType::File);
    }

    #[test]
    fn upload_url_normalized() {
        assert_eq!(
            normalize_upload_url("//cdn.example.com/upload"),
            "https://cdn.example.com/upload"
        );
        assert_eq!(
            normalize_upload_url("https://cdn.example.com/upload"),
            "https://cdn.example.com/upload"
        );
    }
}
