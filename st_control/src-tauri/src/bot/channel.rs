// ============================================================
// 统一消息通道抽象
// 业务层只依赖 Channel trait，后续接入企业微信 / Telegram 时
// 新增实现即可，无需改动自动化引擎与前端选择器。
// ============================================================

use async_trait::async_trait;
use serde::Serialize;
use std::path::Path;

/// 消息通道标识
pub const CHANNEL_ILINK: &str = "ilink";
/// QQ 官方机器人通道（网关事件 → 自动化流水线 → 待回复应答器）
pub const CHANNEL_QQBOT: &str = "qqbot";
/// 通道状态快照（前端展示用）
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatus {
    pub channel: String,
    pub online: bool,
    pub detail: String,
}

/// 统一消息通道接口
#[allow(dead_code)] // 抽象规范：iLink 当前经 BotManager 直连，后续通道实现此 trait
#[async_trait]
pub trait Channel: Send + Sync {
    /// 通道标识（ilink / qqbot ...）
    fn channel_id(&self) -> &'static str;

    /// 发送文本消息，返回消息 ID
    async fn send_text(
        &self,
        to: &str,
        text: &str,
        context_token: Option<&str>,
    ) -> Result<String, String>;

    /// 发送本地文件（图片/文件/语音/视频按扩展名自动路由）
    async fn send_media(
        &self,
        to: &str,
        path: &Path,
        context_token: Option<&str>,
    ) -> Result<String, String>;

    /// 当前通道状态
    async fn status(&self) -> ChannelStatus;
}
