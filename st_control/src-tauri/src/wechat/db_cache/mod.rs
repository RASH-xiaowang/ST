//! DB 解密缓存层
//!
//! 管理微信加密数据库的按需解密，自动检测 mtime 变化并重新解密。
//! 线程安全，支持 per-DB 并发锁。
//!
//! 实时性优化（v2）：
//! 1. 派生密钥缓存：同一 DB 的 PBKDF2(256k 轮) 派生结果按 salt 缓存，
//!    避免每次 WAL patch 重做耗时百毫秒级的密钥派生。
//! 2. checkpoint 跳过：主库 mtime 变化但 WAL 变短（被 checkpoint 截断）时，
//!    主库只是合并了我们已 patch 过的 frame，副本内容等价，跳过全量解密。
//! 3. 新鲜副本种子化：首次访问时若解密副本不比源文件旧（如批量解密产物），
//!    直接信任为基线，避免监控启动时对全部大库做全量解密。

use crate::wechat::keys::Keys;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

mod decrypt;
mod get;
mod keycache;
mod types;
use types::{CacheState, KeyCacheEntry};
mod files;
pub(crate) use files::{
    cleanup_db_staging, replace_decrypted, sqlite_healthy, stage_one, stage_source_snapshot,
};

/// 轻量 DB 解密缓存，mtime 检测变化时重新解密（线程安全）
pub struct MonitorDBCache {
    keys: std::sync::Arc<Keys>,
    db_dir: PathBuf,
    cache_dir: PathBuf,
    /// 为 true 时按原始相对路径（如 `message/message_0.db`）写入 cache_dir，
    /// 使解密输出与手动批量解密目录结构一致，浏览界面可读到实时解密的数据
    preserve_structure: bool,
    state: Mutex<HashMap<String, CacheState>>,
    per_key_locks: Mutex<HashMap<String, std::sync::Arc<std::sync::Mutex<()>>>>,
    /// 派生密钥缓存：rel_key → 缓存条目
    /// salt 不变则密钥有效；DB 文件被重建（salt 变化）时自动重新派生
    key_cache: Mutex<HashMap<String, KeyCacheEntry>>,
}

impl MonitorDBCache {
    /// 创建新的 DB 缓存实例
    pub fn new(keys: std::sync::Arc<Keys>, db_dir: PathBuf, cache_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&cache_dir).ok();
        Self {
            keys,
            db_dir,
            cache_dir,
            preserve_structure: false,
            state: Mutex::new(HashMap::new()),
            per_key_locks: Mutex::new(HashMap::new()),
            key_cache: Mutex::new(HashMap::new()),
        }
    }

    /// 让缓存输出保持与源库一致的相对目录结构。
    /// 当 cache_dir 指向 `decrypted_dir` 时使用，使浏览界面读取到实时解密结果。
    pub fn with_preserved_structure(mut self) -> Self {
        self.preserve_structure = true;
        self
    }

    /// 获取或创建 per-key 锁，防止并发解密同一 DB
    fn get_lock(&self, rel_key: &str) -> std::sync::Arc<std::sync::Mutex<()>> {
        let mut locks = self.per_key_locks.lock().unwrap();
        locks
            .entry(rel_key.to_string())
            .or_insert_with(|| std::sync::Arc::new(std::sync::Mutex::new(())))
            .clone()
    }

    /// 缓存输出路径 (rel_key → 缓存文件)
    fn cache_path(&self, rel_key: &str) -> PathBuf {
        if self.preserve_structure {
            // 保持相对目录结构：message/message_0.db → cache_dir/message/message_0.db
            let rel = rel_key
                .replace('\\', "/")
                .replace('/', std::path::MAIN_SEPARATOR_STR);
            self.cache_dir.join(rel)
        } else {
            let name = rel_key.replace(['\\', '/'], "_");
            self.cache_dir.join(name)
        }
    }

    /// 强制清除缓存状态，下次 `get()` 会重新全量解密
    pub fn invalidate(&self, rel_key: &str) {
        let mut state = self.state.lock().unwrap();
        state.remove(rel_key);
    }

    /// 返回当前已解密文件路径，**不触发**重新解密
    pub fn peek(&self, rel_key: &str) -> Option<PathBuf> {
        self.keys.get_key_info(rel_key)?;
        let path = self.cache_path(rel_key);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }
}
