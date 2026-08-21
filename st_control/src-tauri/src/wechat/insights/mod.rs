//! 微信社交关系图谱模块
//!
//! `get_relationship_graph`：聚合会话消息统计（条数/活跃天数/最近互动时间）与
//! 群成员共群关系，输出以「我」为中心的力导向图数据：
//! - 节点：我 / 联系人（含公众号）/ 群聊
//! - 边：我 ↔ 联系人/群（消息强度），联系人 ↔ 联系人（共群关系）
//!
//! 全部只读访问解密副本；统计结果按解密目录签名缓存，数据刷新后自动失效。

mod types;

mod stats;
pub use types::*;
mod progress;
pub(crate) use progress::*;
mod cache;
pub(crate) use cache::*;

mod graph;
pub use graph::build_relationship_graph;

mod api;
pub use api::*;

#[cfg(test)]
mod tests;
