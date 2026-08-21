//! 微信数据目录配置检测模块
//!
//! 自动检测微信 4.x 数据库存储路径（跨平台），管理配置加载。
//!
//! 配置优先级:
//!   1. 环境变量 `ST_WECHAT_DB_DIR`
//!   2. `<应用基目录>/config.json`（唯一配置文件；路径字段支持
//!      绝对路径 / 相对路径（相对应用基目录）/ 留空自动检测）
//!   3. 自动检测（最活跃账号 → 系统探测）

mod detect;

mod io;
pub use detect::*;
pub use io::*;

mod paths;
pub use paths::*;

mod types;
pub use types::*;

// ============ 默认值 ============

pub const DEFAULT_PROCESS: &str = "Weixin.exe";
pub const DEFAULT_KEYS_FILE: &str = "all_keys.json";
pub const DEFAULT_MONITOR_CACHE: &str = "monitor_cache";
pub const DEFAULT_IMAGE_XOR_KEY: u8 = 0x88;

// ============ 配置加载 ============

/// 加载 config.json 原始配置

#[cfg(test)]
mod tests;
