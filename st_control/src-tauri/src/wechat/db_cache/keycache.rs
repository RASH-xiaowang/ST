// ============================================================
// DB 解密缓存层 — 派生密钥缓存域
// 自 db_cache.rs 拆分：per-DB salt 级 PBKDF2 派生与缓存。
// ============================================================

use std::path::Path;

use crate::wechat::crypto::{derive_enc_key, SALT_SZ};

use super::types::KeyCacheEntry;
use super::MonitorDBCache;

impl MonitorDBCache {
    /// 获取该 DB 的派生密钥（带 salt 级缓存，避免重复 PBKDF2）
    pub(crate) fn derived_key(
        &self,
        rel_key: &str,
        enc_key_hex: &str,
        db_path: &Path,
    ) -> std::io::Result<std::sync::Arc<Vec<u8>>> {
        let raw = hex::decode(enc_key_hex).unwrap_or_default();

        if self.keys.key_format.as_deref() != Some("wx_key_v4.1") {
            // v4.0：raw key 即最终 AES 密钥，无需派生
            return Ok(std::sync::Arc::new(raw));
        }

        // v4.1：需要 per-DB salt 做 PBKDF2(256k) 派生
        let salt = {
            let mut f = std::fs::File::open(db_path)?;
            use std::io::Read;
            let mut buf = vec![0u8; SALT_SZ];
            f.read_exact(&mut buf)?;
            buf
        };

        // 命中缓存（salt 未变）则直接返回
        if let Some(entry) = self.key_cache.lock().unwrap().get(rel_key) {
            if entry.salt == salt {
                return Ok(entry.key.clone());
            }
        }

        let key = std::sync::Arc::new(derive_enc_key(&raw, &salt, self.keys.key_format.as_deref()));
        self.key_cache.lock().unwrap().insert(
            rel_key.to_string(),
            KeyCacheEntry {
                salt,
                key: key.clone(),
            },
        );
        Ok(key)
    }
}
