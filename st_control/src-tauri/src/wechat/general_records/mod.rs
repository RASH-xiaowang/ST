//! 微信 general.db 记录类查询（撤回 / 转账 / 红包 / 视频号 / 小程序）
//!
//! 数据源为解密后的 `decrypted/general/general.db`（微信内部配置库），
//! 对标 WeChatDataAnalysis 的 `routers/general.py`：
//! - `revokebatchmessage`：撤回消息缓存
//! - `transferTable` / `redEnvelopeTable`：转账 / 红包
//! - `wcfinderlivestatus` / `wcfinderuserpage`：视频号直播 / 用户页
//! - `wacontact`(type=小程序) + `WeAppBizAttrSyncBufferTableV02`：小程序

mod db;
pub(crate) use db::{clamp, open_general, rows_to_json, total};
mod export;
pub use export::*;
mod lists;
pub use lists::*;
mod stats;
pub use stats::*;

#[cfg(test)]
mod tests;
