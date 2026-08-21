// ============================================================
// 微信 IPC — 配置 / 密钥 / 解密（门面）
// ============================================================

mod auto;
mod image;
mod io;
mod keys;
pub use auto::*;
pub use image::*;
pub use io::*;
pub use keys::*;

// 共享设施：emit_op_progress 已收敛至 helpers.rs（T-288），
// 经此处 re-export 保持子模块 `super::emit_op_progress` 调用零改动
pub(crate) use crate::wechat::handlers::helpers::emit_op_progress;
