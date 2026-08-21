//! iLink 协议类型（参考官方 openclaw-weixin 与开源 Rust SDK）

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

pub const ILINK_APP_ID: &str = "bot";
pub const CHANNEL_VERSION: &str = "2.4.3";
pub const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com/";
pub const DEFAULT_BOT_TYPE: &str = "3";
pub const SESSION_EXPIRED_ERRCODE: i64 = -14;

/// 上传媒体类型
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr, PartialEq, Eq)]
#[repr(u8)]
pub enum UploadMediaType {
    Image = 1,
    Video = 2,
    File = 3,
    Voice = 4,
}

/// 消息发送方类型
#[derive(Debug, Clone, Copy, Default, Serialize_repr, Deserialize_repr, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    #[default]
    None = 0,
    User = 1,
    Bot = 2,
}

/// 消息条目类型
#[derive(Debug, Clone, Copy, Default, Serialize_repr, Deserialize_repr, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageItemType {
    #[default]
    None = 0,
    Text = 1,
    Image = 2,
    Voice = 3,
    File = 4,
    Video = 5,
}

/// 消息生成状态
#[derive(Debug, Clone, Copy, Default, Serialize_repr, Deserialize_repr, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageState {
    #[default]
    New = 0,
    Generating = 1,
    Finish = 2,
}

/// 输入状态
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr, PartialEq, Eq)]
#[repr(u8)]
pub enum TypingStatus {
    Typing = 1,
    Cancel = 2,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BaseInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_agent: Option<String>,
}

pub fn build_base_info() -> BaseInfo {
    BaseInfo {
        channel_version: Some(CHANNEL_VERSION.to_owned()),
        bot_agent: Some("st-control/1.0.0 (rust)".to_owned()),
    }
}

/// CDN 媒体引用
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CdnMedia {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypt_query_param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aes_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypt_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TextItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aeskey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mid_size: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encode_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playtime: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VideoItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_length: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_md5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_media: Option<CdnMedia>,
}

/// 消息条目
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageItem {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub item_type: Option<MessageItemType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_item: Option<TextItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_item: Option<ImageItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_item: Option<VoiceItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_item: Option<FileItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_item: Option<VideoItem>,
}

/// getupdates 返回的消息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WeixinMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<MessageType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_state: Option<MessageState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_list: Option<Vec<MessageItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUpdatesRequest {
    pub get_updates_buf: String,
    pub base_info: BaseInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetUpdatesResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ret: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errcode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msgs: Option<Vec<WeixinMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_buf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get_updates_buf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longpolling_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub msg: WeixinMessage,
    pub base_info: BaseInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUploadUrlRequest {
    pub filekey: String,
    pub media_type: UploadMediaType,
    pub to_user_id: String,
    pub rawsize: u64,
    pub rawfilemd5: String,
    pub filesize: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_need_thumb: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_rawsize: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_rawfilemd5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_filesize: Option<u64>,
    pub aeskey: String,
    pub base_info: BaseInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetUploadUrlResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_upload_param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_full_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetConfigRequest {
    pub ilink_user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    pub base_info: BaseInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetConfigResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ret: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typing_ticket: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendTypingRequest {
    pub ilink_user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typing_ticket: Option<String>,
    pub status: TypingStatus,
    pub base_info: BaseInfo,
}

/// 二维码响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrCodeResponse {
    pub qrcode: String,
    pub qrcode_img_content: String,
}

/// 二维码状态响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrStatusResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ilink_bot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ilink_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_host: Option<String>,
}
