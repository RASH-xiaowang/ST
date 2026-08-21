// ============================================================
// 聊天消息 — 数据类型
// 自 messages.rs 拆分：ChatMessage / MessagePage（游标分页）。
// ============================================================

use serde::Serialize;

/// 单条聊天消息
#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    /// 本地 ID
    pub local_id: i64,
    /// 服务端 ID
    pub server_id: i64,
    /// 排序序号
    pub sort_seq: i64,
    /// 发送时间（Unix 秒）
    pub ts: i64,
    /// 完整时间 `YYYY-MM-DD HH:MM:SS`
    pub time: String,
    /// PC 风格时间分隔条文本
    pub divider: String,
    /// 是否我发送的
    pub is_self: bool,
    /// 消息类型（规范化后）
    #[serde(rename = "type")]
    pub msg_type: i64,
    /// 类型中文名
    pub type_label: String,
    /// 显示文本（PC 气泡内容）
    pub text: String,
    /// 发送者 username（群聊）
    pub sender_username: String,
    /// 发送者显示名（群聊，经通讯录解析）
    pub sender_name: String,
    /// 是否系统/撤回类消息（居中显示）
    pub is_notice: bool,
    /// 富媒体解析结果
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rich: Option<serde_json::Value>,
    /// 图片 URL（图片消息解密成功时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

/// 消息分页结果（游标分页）
#[derive(Debug, Serialize)]
pub struct MessagePage {
    pub messages: Vec<ChatMessage>,
    /// 该会话消息总数（仅作展示，不精确时可为 0）
    pub total: usize,
    /// 已废弃，保留向前兼容
    pub page: usize,
    pub page_size: usize,
    pub has_more: bool,
    /// 下一页游标值（最小的 sort_seq），供下次请求传入 before_sort_seq
    /// 如果 has_more=false 则此值无意义
    pub next_cursor: i64,
    /// 会话显示名
    pub chat_name: String,
    /// 本机 wxid
    pub self_username: String,
}
