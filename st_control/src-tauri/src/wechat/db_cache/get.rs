// ============================================================
// DB 解密缓存层 — 获取编排域
// 自 db_cache.rs 拆分：mtime 决策 / 失败冷却 / 基线推进。
// ============================================================

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::wechat::crypto::decrypt_wal as crypto_decrypt_wal;

use super::{cleanup_db_staging, sqlite_healthy, stage_one, CacheState, MonitorDBCache};

/// 解密失败后的冷却时间：微信持续写入同一大库（尤其 message 分库）时，
/// 每秒轮询重试全量解密会反复烧 CPU（实测可达 9 核）。
/// 失败后至少等待该时长再重试，避免忙等空转。
const DECRYPT_FAIL_COOLDOWN: Duration = Duration::from_secs(30);

impl MonitorDBCache {
    /// 返回解密后的缓存文件路径，mtime 变化时自动重新解密
    ///
    /// 这是一个同步阻塞调用（解密操作是 CPU 密集的）。
    pub fn get(&self, rel_key: &str) -> std::io::Result<Option<PathBuf>> {
        let key_info = match self.keys.get_key_info(rel_key) {
            Some(k) => k,
            None => return Ok(None),
        };
        let enc_key_hex = key_info.enc_key.clone();

        let lock_arc = self.get_lock(rel_key);
        let _lock = lock_arc.lock().unwrap();

        let rel_path = rel_key
            .replace('\\', "/")
            .replace('/', std::path::MAIN_SEPARATOR_STR);
        let db_path = self.db_dir.join(&rel_path);
        let wal_path = db_path.with_extension("db-wal");

        if !db_path.exists() {
            return Ok(None);
        }

        let db_mtime = std::fs::metadata(&db_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let (wal_mtime, wal_len) = if wal_path.exists() {
            let meta = std::fs::metadata(&wal_path).ok();
            let mt = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let len = meta.map(|m| m.len()).unwrap_or(0);
            (mt, len)
        } else {
            (SystemTime::UNIX_EPOCH, 0)
        };

        let out_path = self.cache_path(rel_key);
        let current_state = CacheState {
            db_mtime,
            wal_mtime,
            last_fail: None,
        };

        // 读取上一次的基线状态（读后立即释放全局锁，重活不阻塞其他 DB）
        let prev = self.state.lock().unwrap().get(rel_key).cloned();

        /// 需要执行的动作
        enum Action {
            /// 无需操作（无变化，或 checkpoint 后内容等价）
            Nothing,
            /// 仅 WAL 增量 patch（毫秒级）
            WalPatch,
            /// 全量解密（首次或源库被整体重建）
            Full,
        }

        let action = match &prev {
            None => {
                // 首次访问：解密副本已存在且不比源文件旧（如配置面板批量解密
                // 产物，或上次监控运行的输出），信任其为基线，避免启动时对
                // 全部大库做全量解密。
                let out_fresh = std::fs::metadata(&out_path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|om| om >= db_mtime)
                    .unwrap_or(false);
                if out_fresh {
                    // WAL 中可能有副本之后的新 frame，补一次增量 patch（幂等）
                    if wal_len > 0 {
                        Action::WalPatch
                    } else {
                        Action::Nothing
                    }
                } else {
                    Action::Full
                }
            }
            Some(p) => {
                if p.db_mtime != db_mtime {
                    // 主库 mtime 变化（通常是微信 checkpoint 把 WAL 合并进主库）：
                    // 副本此前已逐次增量 patch 过这些 frame，内容等价。
                    // 一律优先走 WAL 增量（当前 WAL 可能已截断 → 无帧 no-op），
                    // 避免每次 checkpoint 都对数百 MB 大库做全量解密，
                    // 导致消息推送秒级延迟（"时快时慢"的根因）。
                    // 副本缺失/不健康时，WalPatch 分支会自动升级为全量重建。
                    Action::WalPatch
                } else if p.wal_mtime != wal_mtime {
                    Action::WalPatch
                } else {
                    Action::Nothing
                }
            }
        };

        match action {
            Action::Full => {
                // 失败冷却：上次全量解密刚失败过（源库大概率仍被微信写入），
                // 直接跳过本轮，避免每秒对数百 MB 大库反复全量解密烧 CPU。
                let in_cooldown = prev
                    .as_ref()
                    .and_then(|p| p.last_fail)
                    .map(|t| t.elapsed().unwrap_or(Duration::ZERO) < DECRYPT_FAIL_COOLDOWN)
                    .unwrap_or(false);
                if in_cooldown {
                    log::warn!(
                        "[db_cache] {} 解密刚失败过，冷却 {}s 内跳过全量解密，返回已有副本",
                        rel_key,
                        DECRYPT_FAIL_COOLDOWN.as_secs()
                    );
                } else {
                    match self.decrypt_full_atomic(
                        rel_key,
                        &db_path,
                        &wal_path,
                        &out_path,
                        &enc_key_hex,
                    ) {
                        Ok(()) => {
                            // 成功后清除失败标记
                            if let Some(p) = self.state.lock().unwrap().get_mut(rel_key) {
                                p.last_fail = None;
                            }
                        }
                        Err(e) => {
                            // 记录失败时间，冷却期内不再重试
                            self.state
                                .lock()
                                .unwrap()
                                .entry(rel_key.to_string())
                                .or_insert_with(|| CacheState {
                                    db_mtime,
                                    wal_mtime,
                                    last_fail: None,
                                })
                                .last_fail = Some(SystemTime::now());
                            return Err(e);
                        }
                    }
                }
            }
            Action::WalPatch => {
                // 清理旧版"可写打开"遗留的 -shm/-wal，避免干扰只读健康校验
                let _ = std::fs::remove_file(out_path.with_extension("db-wal"));
                let _ = std::fs::remove_file(out_path.with_extension("db-shm"));
                // 现有副本不健康（历史损坏残留 / 解密半成品）：升级为全量重建
                if !out_path.exists() || !sqlite_healthy(&out_path) {
                    log::warn!(
                        "[db_cache] {} 现有副本不健康（0 表或损坏），升级为全量重建",
                        rel_key
                    );
                    self.decrypt_full_atomic(
                        rel_key,
                        &db_path,
                        &wal_path,
                        &out_path,
                        &enc_key_hex,
                    )?;
                } else {
                    let enc_key = self.derived_key(rel_key, &enc_key_hex, &db_path)?;
                    // 先双复制/稳定校验暂存 WAL，避免读到微信写一半的帧
                    let staging_wal = out_path.with_extension("db.stage_wal");
                    let staged = match stage_one(&wal_path, &staging_wal) {
                        Ok(()) => true,
                        Err(e) => {
                            log::warn!(
                                "[db_cache] {} WAL 暂存不稳定 ({}), 跳过本轮 patch 下轮重试",
                                rel_key,
                                e
                            );
                            false
                        }
                    };
                    let patch_res = if staged {
                        crypto_decrypt_wal(&staging_wal, &out_path, &enc_key)
                    } else {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::WouldBlock,
                            "WAL 暂存不稳定",
                        ))
                    };
                    cleanup_db_staging(&[&staging_wal]);
                    if let Err(e) = patch_res {
                        log::error!(
                            "[db_cache] {} WAL patch 失败: {}，不推进基线，下轮重试",
                            rel_key,
                            e
                        );
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("WAL patch 失败: {}", e),
                        ));
                    }
                    // patch 后自检：异常（页错位/写坏）时立即全量重建自愈
                    if !sqlite_healthy(&out_path) {
                        log::warn!(
                            "[db_cache] {} WAL patch 后副本异常，升级为全量重建",
                            rel_key
                        );
                        self.decrypt_full_atomic(
                            rel_key,
                            &db_path,
                            &wal_path,
                            &out_path,
                            &enc_key_hex,
                        )?;
                    }
                }
            }
            Action::Nothing => {}
        }

        // 推进基线（仅成功路径到达这里；失败已在上面提前返回）
        self.state
            .lock()
            .unwrap()
            .insert(rel_key.to_string(), current_state);

        Ok(Some(out_path))
    }
}
