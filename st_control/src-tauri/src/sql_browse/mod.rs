//! 通用 SQLite 表浏览引擎（内部库 control.db 与外部库共用）
//!
//! 抽取自原 `db.rs` / `external_db.rs` 中重复的查询/分页/过滤/类型转换逻辑：
//! - 内置数据库（control.db，可读写）与外部数据库（只读浏览）共用同一套查询实现
//! - 提供安全的标识符转义、LIKE 过滤、keyset 分页、BLOB 预览、友好错误提示
//! - 提供 JSON ↔ SQL 值转换，供 CRUD 命令绑定正确的 SQLite 类型

mod types;
pub use types::*;

mod utils;
pub use utils::*;

mod convert;
pub use convert::*;

mod export;
pub use export::*;

mod execute;
pub use execute::*;

mod query;

mod inspect;
pub use inspect::*;
pub use query::*;
