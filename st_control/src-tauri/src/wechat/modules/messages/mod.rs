//! 聊天消息模块 - 对应 PC 微信聊天窗口
//!
//! 数据来源：
//! - `message/message_*.db`     私聊/群聊消息（表名 `Msg_` + MD5(username)）
//! - `biz_message/biz_message_0.db` 公众号消息（同结构）
//! - 各库内 `Name2Id` 表        发送者 ID → username 映射
//!
//! 与 PC 微信一致的逻辑：
//! - 消息按 `sort_seq`（缺失则 `local_id`）升序排列
//! - 发送者通过 `real_sender_id` 关联 `Name2Id` 得到真实 username，
//!   再经通讯录解析为显示名（群聊中显示在气泡上方）
//! - `is_self` = 发送者 username == 本机 wxid（从账号目录名获取）
//! - 文本消息直接显示；其他类型按 PC 规则解析 XML 渲染
//!   （图片/语音/视频/表情/文件/链接/引用/转账/系统/撤回）
//! - 系统消息(10000)与撤回消息(10002)居中灰色显示
//!
//! 分页说明：
//! 使用游标分页（cursor-based）以替代 OFFSET 分页，避免后端
//! 每页多查（skip+page_size）行的性能浪费。首次调用传 cursor=None
//! 获取最新 page_size 条；后续传前一次返回的 next_cursor 获取更早消息。
//! 每次每个分库只读 page_size 行，合并后取 page_size 行，查询量恒定。

mod types;

mod query;
pub use query::get_conversation_messages;
pub use query::get_session_message_type_stats;

mod shards;

mod parse;
pub use types::*;

#[cfg(test)]
mod tests;
