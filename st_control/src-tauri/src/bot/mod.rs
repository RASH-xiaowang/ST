// ============================================================
// 消息通道（Bot）模块 — 微信 ClawBot（iLink）接入
// 职责：多账号扫码绑定 / 24h 到期管理 / 双向消息收发 /
//       全媒体（CDN AES-128-ECB）/ 自动化引擎桥接
// ============================================================

pub mod bridge;
pub mod channel;
pub mod channels;
pub mod db;
pub mod handlers;
pub mod manager;
pub mod qqbot_gateway;
pub mod qqbot_inbound;
pub(crate) mod reply_tasks;
pub(crate) mod secret; // crate 内共享 AES 加密助手（harness 凭据落盘复用）

mod ilink;
