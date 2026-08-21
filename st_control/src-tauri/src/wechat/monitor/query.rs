//! 微信实时消息监听 — 数据库查询 / 刷新
//! 自 monitor.rs 拆分：会话状态查询、全量/增量解密刷新、
//! 消息分库解析与水位线/最新消息查询。

use std::collections::HashMap;

use super::util::{
    cleanup_staging, connect_db, db_mtime, load_name2id, stage_full_snapshot, stage_stable_copy,
};
use super::{SessionEntry, SessionMonitor};
use crate::wechat::crypto::{decrypt_wal, full_decrypt};

impl SessionMonitor {
    /// 查询当前已解密副本的 session 状态
    pub(crate) fn query_state(&self) -> Result<HashMap<String, SessionEntry>, rusqlite::Error> {
        let conn = connect_db(&self.decrypted_session)?;
        let mut stmt = conn.prepare(
            "SELECT username, unread_count, summary, last_timestamp, \
             last_msg_type, last_msg_sender, last_sender_display_name \
             FROM SessionTable WHERE last_timestamp > 0",
        )?;

        let mut state = HashMap::new();
        let rows = stmt.query_map([], |row| {
            let username: String = row.get(0)?;
            let entry = SessionEntry {
                unread: row.get(1)?,
                // summary 实际为 TEXT 存储（zstd 压缩时才是 BLOB），
                // 必须用 get_bytes 兼容读取；直接读 Vec<u8> 会让整行被
                // flatten 静默丢弃，导致会话状态快照为空、实时推送永不触发。
                summary: crate::wechat::modules::common::get_bytes(row, 2)
                    .map(|b| crate::wechat::modules::common::decode_blob_text(&b))
                    .unwrap_or_default(),
                timestamp: row.get(3)?,
                msg_type: row.get(4)?,
                sender: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                sender_name: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
            };
            Ok((username, entry))
        })?;

        for r in rows.flatten() {
            state.insert(r.0, r.1);
        }

        Ok(state)
    }

    /// 全量解密 DB + WAL patch（用 temp 文件避免写坏已发布的副本）
    ///
    /// 流程：
    /// 1. 先双复制暂存主库 + WAL 一致性快照（避免读取微信写入中的撕裂页）
    /// 2. 对暂存副本解密主库到 temp 文件
    /// 3. 对 temp 应用 WAL 增量
    /// 4. 成功后用 temp 替换正式副本；失败时保留旧副本，不推进 mtime，
    ///    并在本轮内短暂重试（微信写入窗口是瞬时的，通常一次重试即成功）
    ///
    /// 历史教训（防止回归）：
    /// decrypt_wal 直接写入正本曾导致 SQLite 报 "file is not a database"，
    /// 引发 clean → corrupt → delete → clean → corrupt 死循环。
    /// 改用 temp + rename 原子替换后，正本始终合法可读。
    pub(crate) fn do_full_refresh(&self) -> std::io::Result<u32> {
        // 与手动刷新（refresh_wechat_sessions）互斥，避免并发替换同一解密副本
        let _guard = crate::wechat::handlers::helpers::session_refresh_lock();
        let t0 = std::time::Instant::now();
        if let Some(parent) = self.decrypted_session.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let (db_ms, wal_ms) = self.session_file_state();

        // 暂存路径（session_refresh_lock 串行化，不会与其它刷新冲突）
        let temp_path = self
            .decrypted_session
            .with_extension("db.full_decrypt_temp");
        let staging_db = self.decrypted_session.with_extension("db.stage_src");
        let staging_wal = self.decrypted_session.with_extension("db.stage_wal");
        let mut last_err = std::io::Error::other("全量解密失败");

        // 写入窗口瞬时性：最多重试 3 次，间隔 250ms，避免整轮等待下个轮询
        for attempt in 0..3u32 {
            // 1. 暂存一致性快照（主库 + WAL）
            if let Err(e) = stage_full_snapshot(&self.session_db, &staging_db, &staging_wal) {
                log::debug!("[monitor] 快照暂存不稳定（第 {} 次）: {}", attempt + 1, e);
                last_err = e;
                cleanup_staging(&[&staging_db, &staging_wal]);
                if attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                continue;
            }

            // 2. 解密暂存主库到临时文件
            let pages = match full_decrypt(&staging_db, &temp_path, &self.enc_key) {
                Ok(p) => p,
                Err(e) => {
                    last_err = e;
                    cleanup_staging(&[&staging_db, &staging_wal]);
                    let _ = std::fs::remove_file(&temp_path);
                    if attempt < 2 {
                        std::thread::sleep(std::time::Duration::from_millis(250));
                    }
                    continue;
                }
            };

            // 3. 对 temp 文件应用 WAL 增量（从暂存副本读取，避免读到写一半的 WAL）
            let wal_patched = if staging_wal.exists() {
                match decrypt_wal(&staging_wal, &temp_path, &self.enc_key) {
                    Ok(n) => {
                        log::debug!("[monitor] full_refresh WAL 增量 {} 页", n);
                        n
                    }
                    Err(e) => {
                        log::error!("[monitor] full_refresh WAL 应用失败 ({}), 继续使用 base", e);
                        0u32
                    }
                }
            } else {
                0u32
            };

            // 4. 健康校验：源库被微信写入中断时解密结果可能损坏，丢弃重试
            if !crate::wechat::db_cache::sqlite_healthy(&temp_path) {
                log::error!(
                    "[monitor] 全量解密结果无效（第 {} 次），丢弃临时文件重试",
                    attempt + 1
                );
                last_err = std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "解密结果无效（源库可能正在被写入）",
                );
                cleanup_staging(&[&staging_db, &staging_wal]);
                let _ = std::fs::remove_file(&temp_path);
                if attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                continue;
            }

            // 5. 原子替换：temp → 正式
            let _ = std::fs::remove_file(&self.decrypted_session);
            if let Err(e) = std::fs::rename(&temp_path, &self.decrypted_session) {
                last_err = e;
                cleanup_staging(&[&staging_db, &staging_wal]);
                if attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                continue;
            }
            cleanup_staging(&[&staging_db, &staging_wal]);

            let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
            self.decrypt_ms
                .store(total_ms as u64, std::sync::atomic::Ordering::Relaxed);
            self.patched_pages
                .store(pages + wal_patched, std::sync::atomic::Ordering::Relaxed);
            self.mark_session_refreshed(db_ms, wal_ms);
            return Ok(pages + wal_patched);
        }

        cleanup_staging(&[&staging_db, &staging_wal]);
        let _ = std::fs::remove_file(&temp_path);
        Err(last_err)
    }

    /// WAL 增量刷新（快路径，毫秒级）
    ///
    /// 微信新数据先写入 session.db-wal，主库文件仅在 checkpoint 时变化。
    /// WAL 变更 → 临时副本解密 → 替换 base → query_state 比对 → 推送
    ///
    /// 先复制一份干净的 base 到临时文件，对临时文件应用 decrypt_wal。
    /// - 成功：用 temp 替换 base（最新消息被包含），并推进 mtime 快照
    /// - 失败：删掉 temp，base 保持干净，**不推进 mtime 快照**
    ///   （下一轮轮询会重新检测到同一 WAL 变更并重试，避免数据永久丢失）
    pub(crate) fn do_wal_refresh(&self) -> std::io::Result<u32> {
        if !self.decrypted_session.exists() {
            return self.do_full_refresh();
        }
        let _guard = crate::wechat::handlers::helpers::session_refresh_lock();
        let t0 = std::time::Instant::now();
        let (db_ms, wal_ms) = self.session_file_state();
        let wal_path = self.session_db.with_extension("db-wal");

        // 1. 复制干净 base 到临时文件
        let temp_path = self.decrypted_session.with_extension("db.temp");
        std::fs::copy(&self.decrypted_session, &temp_path)?;

        // 2. 对临时文件应用 WAL（先双复制暂存 WAL，避免读到写一半的帧；
        //    失败时 temp 被删，base 不受影响）
        let (patched, success) = if wal_path.exists() {
            let staging_wal = self.decrypted_session.with_extension("db.stage_wal");
            let staged = match stage_stable_copy(&wal_path, &staging_wal) {
                Ok(()) => true,
                Err(e) => {
                    log::warn!("[monitor] WAL 暂存不稳定 ({}), 放弃 WAL 使用原始 base", e);
                    false
                }
            };
            if !staged {
                (0u32, false)
            } else {
                let res = decrypt_wal(&staging_wal, &temp_path, &self.enc_key);
                let _ = std::fs::remove_file(&staging_wal);
                let _ = std::fs::remove_file(staging_wal.with_extension("stage_a"));
                let _ = std::fs::remove_file(staging_wal.with_extension("stage_b"));
                match res {
                    Ok(n) => {
                        // patch 后校验：异常时丢弃 temp，保留原 base 等待下次全量重建
                        if !crate::wechat::db_cache::sqlite_healthy(&temp_path) {
                            let _ = std::fs::remove_file(&temp_path);
                            log::error!("[monitor] WAL patch 后副本无效，保留原 base 下轮重建");
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "WAL patch 后副本无效",
                            ));
                        }
                        // WAL 成功 → 用 temp 替换 base
                        let _ = std::fs::remove_file(&self.decrypted_session);
                        std::fs::rename(&temp_path, &self.decrypted_session)?;
                        log::debug!("[monitor] WAL 临时解密 {} 页成功，已替换 base", n);
                        (n, true)
                    }
                    Err(e) => {
                        log::error!("[monitor] WAL 临时解密失败 ({}), 放弃 WAL 使用原始 base", e);
                        let _ = std::fs::remove_file(&temp_path);
                        (0u32, false)
                    }
                }
            }
        } else {
            (0u32, true) // WAL 不存在视为成功（无数据需要处理）
        };

        let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        self.decrypt_ms
            .store(total_ms as u64, std::sync::atomic::Ordering::Relaxed);
        self.patched_pages
            .store(patched, std::sync::atomic::Ordering::Relaxed);
        // 仅成功时推进 mtime 快照，失败时保留旧值让下轮重试
        if success {
            self.mark_session_refreshed(db_ms, wal_ms);
        } else {
            log::warn!("[monitor] WAL patch 失败，不推进 mtime 快照，下轮重试");
        }
        Ok(patched)
    }

    /// 懒加载 username → message DB 的映射。
    /// 预构建的映射依赖 Name2Id，群聊（@chatroom）等会话可能缺失，
    /// 因此在这里按 Msg_<md5> 表名扫描所有 message DB 作为兜底。
    pub(crate) async fn resolve_message_dbs(&self, username: &str) -> Vec<String> {
        {
            let map = self.username_db_map.read().await;
            if let Some(keys) = map.get(username) {
                if !keys.is_empty() {
                    return keys.clone();
                }
            }
        }

        let table = crate::wechat::modules::common::msg_table_name(username);
        let mut paths = crate::wechat::modules::common::find_db_files(&self.db_dir, "message_");
        paths.extend(crate::wechat::modules::common::find_db_files(
            &self.db_dir,
            "biz_message_",
        ));
        paths.sort();
        paths.dedup();
        paths.retain(|p| !p.to_string_lossy().contains("monitor_cache"));
        // 只探测真正的消息分片库，避免对 message_fts.db 等辅助大库做无谓解密
        paths.retain(|p| crate::wechat::modules::common::is_message_shard_file(p));

        let mut keys = Vec::new();
        for path in paths.iter().rev() {
            let rel_key = match path
                .strip_prefix(&self.db_dir)
                .ok()
                .and_then(|p| p.to_str())
            {
                Some(r) => r.replace('\\', "/"),
                None => continue,
            };
            let dec_path = match self.db_cache.get(&rel_key) {
                Ok(Some(p)) => p,
                _ => continue,
            };
            if !dec_path.exists() {
                continue;
            }
            let conn = match connect_db(&dec_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if crate::wechat::modules::common::table_exists(&conn, &table) {
                keys.push(rel_key);
            }
        }

        keys.sort_by(|a, b| {
            let ma = db_mtime(&self.db_dir, a).unwrap_or(0);
            let mb = db_mtime(&self.db_dir, b).unwrap_or(0);
            mb.cmp(&ma)
        });

        if !keys.is_empty() {
            log::info!(
                "[monitor] 懒加载 username→db 映射: {} -> {:?}（群聊/缺失会话）",
                username,
                keys
            );
            let mut map = self.username_db_map.write().await;
            map.insert(username.to_string(), keys.clone());
        }
        keys
    }

    /// 基于水位线查询某会话的新消息
    ///
    /// 使用会话级 watermark 作为下界，可捕获 session 表尚未更新的消息；
    /// 若水位线为空，退化到 create_time > cutoff 的全量查询。
    pub(crate) async fn query_messages_since_watermark(
        &self,
        username: &str,
        cutoff: i64,
    ) -> Vec<(i64, i64, i32, String, i64, i64, String)> {
        let watermark = self.watermark_store.get(username).await.unwrap_or_default();
        // 取 watermark 与 cutoff 的较大值，避免首次运行推送过旧消息
        let effective_create_time = watermark.create_time.max(cutoff);
        let effective_local_id = watermark.local_id;

        let table_name = crate::wechat::modules::common::msg_table_name(username);
        let db_keys = self.resolve_message_dbs(username).await;
        if db_keys.is_empty() {
            return vec![];
        }

        let mut db_paths = Vec::with_capacity(db_keys.len());
        for db_key in &db_keys {
            match self.db_cache.get(db_key) {
                Ok(Some(p)) if p.exists() => db_paths.push(p),
                _ => continue,
            }
        }

        if db_paths.is_empty() {
            return vec![];
        }

        let mut handles = Vec::with_capacity(db_paths.len());
        for dec_path in db_paths {
            let table_name = table_name.clone();
            let handle = tokio::task::spawn_blocking(move || {
                let conn = match connect_db(&dec_path) {
                    Ok(c) => c,
                    Err(_) => return Vec::new(),
                };

                let cols: Vec<String> = conn
                    .prepare(&format!("PRAGMA table_info(\"{}\")", table_name))
                    .ok()
                    .map(|mut s| {
                        s.query_map([], |r| r.get::<_, String>(1))
                            .ok()
                            .map(|it| it.filter_map(|r| r.ok()).collect::<Vec<String>>())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                let has_rs = cols.iter().any(|c| c == "real_sender_id");
                let name2id = load_name2id(&conn);

                // create_time 精度为秒，同一秒内可能到达多条消息（local_id 递增）。
                // 原 `local_id > ?1 AND create_time > ?2` 双条件会把"同秒但 local_id 更大"
                // 的消息永久过滤掉（群聊高发丢消息）。改为复合条件：
                //   下一秒及以后的消息：create_time > 水位线
                //   同一秒内的后续消息：create_time = 水位线 且 local_id 更大
                let query = format!(
                    "SELECT local_id, create_time, local_type, \
                     message_content, svr_id, sort_seq, {} FROM \"{}\" \
                     WHERE create_time > ?2 \
                        OR (create_time = ?2 AND local_id > ?1) \
                     ORDER BY create_time ASC, local_id ASC",
                    if has_rs { "real_sender_id" } else { "0" },
                    table_name,
                );

                let mut rows_for_db = Vec::new();
                if let Ok(mut stmt) = conn.prepare(&query) {
                    if let Ok(rows) = stmt.query_map(
                        rusqlite::params![effective_local_id, effective_create_time],
                        |row| {
                            // message_content 为 TEXT/BLOB 混合存储，必须兼容读取
                            let content: String = crate::wechat::modules::common::get_bytes(row, 3)
                                .map(|b| crate::wechat::modules::common::decode_blob_text(&b))
                                .unwrap_or_default();
                            let real_sender_id = row.get::<_, i64>(6).unwrap_or(0);
                            let sender_username =
                                name2id.get(&real_sender_id).cloned().unwrap_or_default();
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, i64>(1)?,
                                row.get::<_, i32>(2)?,
                                content,
                                row.get::<_, i64>(4)?,
                                row.get::<_, i64>(5)?,
                                sender_username,
                            ))
                        },
                    ) {
                        for r in rows.flatten() {
                            rows_for_db.push(r);
                        }
                    }
                }
                rows_for_db
            });
            handles.push(handle);
        }

        let mut all_rows = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(rows) => all_rows.extend(rows),
                Err(e) => log::warn!("[monitor] 水位线分库查询任务 join 失败: {}", e),
            }
        }

        all_rows.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        all_rows
    }

    /// 直查某会话最新一条消息（不套水位线过滤）。
    ///
    /// 水位线查询可能因 watermark 已覆盖 / WAL 时序返回空，此时会话摘要
    /// 路径需要拿真实发送者来判定方向（单聊 SessionTable 的 last_msg_sender
    /// 经常为空，直接按它判断会把「我发的消息」错放到对方一侧）。
    pub(crate) async fn query_latest_message(
        &self,
        username: &str,
    ) -> Option<(i64, i64, i32, String, i64, i64, String)> {
        let table_name = crate::wechat::modules::common::msg_table_name(username);
        let db_keys = self.resolve_message_dbs(username).await;
        if db_keys.is_empty() {
            return None;
        }

        let mut db_paths = Vec::with_capacity(db_keys.len());
        for db_key in &db_keys {
            match self.db_cache.get(db_key) {
                Ok(Some(p)) if p.exists() => db_paths.push(p),
                _ => continue,
            }
        }
        if db_paths.is_empty() {
            return None;
        }

        let mut handles = Vec::with_capacity(db_paths.len());
        for dec_path in db_paths {
            let table_name = table_name.clone();
            let handle = tokio::task::spawn_blocking(move || {
                let conn = match connect_db(&dec_path) {
                    Ok(c) => c,
                    Err(_) => return None,
                };
                let cols: Vec<String> = conn
                    .prepare(&format!("PRAGMA table_info(\"{}\")", table_name))
                    .ok()
                    .map(|mut s| {
                        s.query_map([], |r| r.get::<_, String>(1))
                            .ok()
                            .map(|it| it.filter_map(|r| r.ok()).collect::<Vec<String>>())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                let has_rs = cols.iter().any(|c| c == "real_sender_id");
                let name2id = load_name2id(&conn);
                let query = format!(
                    "SELECT local_id, create_time, local_type, message_content, \
                     svr_id, sort_seq, {} FROM \"{}\" \
                     ORDER BY sort_seq DESC, local_id DESC LIMIT 1",
                    if has_rs { "real_sender_id" } else { "0" },
                    table_name,
                );
                let mut stmt = conn.prepare(&query).ok()?;
                let row = stmt
                    .query_row([], |row| {
                        let content: String = crate::wechat::modules::common::get_bytes(row, 3)
                            .map(|b| crate::wechat::modules::common::decode_blob_text(&b))
                            .unwrap_or_default();
                        let real_sender_id = row.get::<_, i64>(6).unwrap_or(0);
                        let sender_username =
                            name2id.get(&real_sender_id).cloned().unwrap_or_default();
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i32>(2)?,
                            content,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            sender_username,
                        ))
                    })
                    .ok();
                row
            });
            handles.push(handle);
        }

        let mut best: Option<(i64, i64, i32, String, i64, i64, String)> = None;
        for handle in handles {
            if let Ok(Some(row)) = handle.await {
                let better = match &best {
                    Some(b) => row.1 > b.1 || (row.1 == b.1 && row.0 > b.0),
                    None => true,
                };
                if better {
                    best = Some(row);
                }
            }
        }
        best
    }
}
