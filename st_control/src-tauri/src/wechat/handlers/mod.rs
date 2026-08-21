// ============================================================
// 微信 IPC — 模块入口
// ============================================================
// 按领域拆分为四个子模块 + 共享辅助函数，
// 删除任意子模块不会影响其他模块的运行。
//
// 子模块：
//   helpers  — 跨模块共享的辅助函数
//   session  — 会话 / 消息 / 导出
//   data     — 通讯录 / 朋友圈 / 收藏 / 表情 / 文件 / 状态
//   config   — 配置 / 密钥 / 解密
//   monitor  — 监控状态 / 生命周期
// ============================================================

pub mod annual;
pub mod archive;
pub mod config;
pub mod daily_summary;
pub mod data;
pub mod general;
pub mod helpers;
pub mod monitor;
pub mod session;

// 将所有 IPC 命令再导出到 handlers 根，保持 existing calling convention
pub use annual::*;
pub use archive::*;
pub use config::*;
pub use daily_summary::*;
pub use data::*;
pub use general::*;
pub use monitor::*;
pub use session::*;
