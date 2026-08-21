// ============================================================
// DB 解密缓存层 — 全量解密域
// 自 db_cache.rs 拆分：temp + 健康校验 + 原子替换管线。
// ============================================================

use std::path::Path;
use std::time::Duration;

use crate::wechat::crypto::{self, decrypt_wal as crypto_decrypt_wal};

use super::{
    cleanup_db_staging, replace_decrypted, sqlite_healthy, stage_source_snapshot, MonitorDBCache,
};

impl MonitorDBCache {
    /// 全量解密 + WAL patch 到临时文件，校验健康后原子替换正式副本。
    ///
    /// 历史教训：直接写正式副本（File::create 覆盖 / 原地 WAL patch）在
    /// 源库被微信持续写入或中途失败时会留下"半成品/0 表"损坏副本，导致
    /// 消息浏览反复出现"目标表不存在（该库共 0 表）"。
    /// 改为 temp + 校验 + rename 后，失败只丢弃临时文件，正本始终合法。
    pub(crate) fn decrypt_full_atomic(
        &self,
        rel_key: &str,
        db_path: &Path,
        wal_path: &Path,
        out_path: &Path,
        enc_key_hex: &str,
    ) -> std::io::Result<()> {
        let enc_key = self.derived_key(rel_key, enc_key_hex, db_path)?;
        if enc_key.len() != crypto::KEY_SZ {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "密钥长度错误",
            ));
        }
        let temp = out_path.with_extension("db.decrypt_tmp");
        let staging_db = out_path.with_extension("db.stage_src");
        let staging_wal = out_path.with_extension("db.stage_wal");
        let mut last_err = std::io::Error::other("全量解密失败");

        // 微信写入窗口是瞬时的：最多重试 2 次，间隔 250ms
        for attempt in 0..2u32 {
            // 1. 暂存一致性快照（主库 + WAL），避免读到撕裂页
            if let Err(e) = stage_source_snapshot(db_path, wal_path, &staging_db, &staging_wal) {
                log::debug!(
                    "[db_cache] {} 快照暂存不稳定（第 {} 次）: {}",
                    rel_key,
                    attempt + 1,
                    e
                );
                last_err = e;
                cleanup_db_staging(&[&staging_db, &staging_wal]);
                let _ = std::fs::remove_file(&temp);
                if attempt < 1 {
                    std::thread::sleep(Duration::from_millis(250));
                }
                continue;
            }

            // 2. 解密暂存主库到临时文件
            if let Err(e) = crate::wechat::crypto::full_decrypt(&staging_db, &temp, &enc_key) {
                last_err = e;
                cleanup_db_staging(&[&staging_db, &staging_wal]);
                let _ = std::fs::remove_file(&temp);
                if attempt < 1 {
                    std::thread::sleep(Duration::from_millis(250));
                }
                continue;
            }

            // 3. 对临时文件应用 WAL 增量（从暂存副本读取，失败丢弃 temp）
            if staging_wal.exists() {
                if let Err(e) = crypto_decrypt_wal(&staging_wal, &temp, &enc_key) {
                    log::error!(
                        "[db_cache] {} 全量解密后 WAL patch 失败: {}，不推进基线，下轮重试",
                        rel_key,
                        e
                    );
                    cleanup_db_staging(&[&staging_db, &staging_wal]);
                    let _ = std::fs::remove_file(&temp);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("WAL patch 失败: {}", e),
                    ));
                }
            }

            // 4. 健康校验：源库被微信写中断 / 密钥错误时解密结果无效，
            //    绝不能发布（否则浏览端看到"0 表"空库）。
            if !sqlite_healthy(&temp) {
                log::error!(
                    "[db_cache] {} 解密结果无效（第 {} 次），丢弃临时文件重试",
                    rel_key,
                    attempt + 1
                );
                last_err = std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "解密结果无效（源库可能正在被写入）",
                );
                cleanup_db_staging(&[&staging_db, &staging_wal]);
                let _ = std::fs::remove_file(&temp);
                if attempt < 1 {
                    std::thread::sleep(Duration::from_millis(250));
                }
                continue;
            }

            // 5. 原子替换（与 monitor 的 do_full_refresh 一致）
            cleanup_db_staging(&[&staging_db, &staging_wal]);
            return replace_decrypted(&temp, out_path);
        }

        cleanup_db_staging(&[&staging_db, &staging_wal]);
        let _ = std::fs::remove_file(&temp);
        Err(last_err)
    }
}
