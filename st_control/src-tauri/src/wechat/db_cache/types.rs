// ============================================================
// DB 解密缓存层 — 数据类型
// 自 db_cache.rs 拆分：状态条目与派生密钥缓存条目。
// ============================================================

use std::time::SystemTime;

/// 解密 DB 缓存的状态条目
#[derive(Debug, Clone)]
pub(crate) struct CacheState {
    pub(crate) db_mtime: SystemTime,
    pub(crate) wal_mtime: SystemTime,
    /// 最近一次全量解密失败时间（None = 未失败 / 已成功）
    pub(crate) last_fail: Option<SystemTime>,
}

/// 派生密钥缓存条目（salt 不变则密钥有效；DB 文件被重建时自动重新派生）
pub(crate) struct KeyCacheEntry {
    pub(crate) salt: Vec<u8>,
    pub(crate) key: std::sync::Arc<Vec<u8>>,
}
