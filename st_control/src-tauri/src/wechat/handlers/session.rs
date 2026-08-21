// ============================================================
// 微信 IPC — 会话 / 消息 / 导出（门面）
// ============================================================

mod edit;
mod export;
mod search;
pub use edit::*;
pub use export::*;
pub use search::*;

use crate::wechat::handlers::helpers;

#[tauri::command]
pub async fn get_session_list() -> Result<Vec<serde_json::Value>, String> {
    helpers::run_blocking(|| {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let sessions = crate::wechat::modules::sessions::get_session_list(&cfg.decrypted_dir)?;
        serde_json::to_value(sessions)
            .map_err(|e| e.to_string())?
            .as_array()
            .cloned()
            .ok_or_else(|| "序列化失败".to_string())
    })
    .await
}

/// 强制刷新会话列表：重新解密 session.db（全量 + WAL 增量），然后返回最新会话列表。
///
/// 区别于普通 get_session_list，本命令在执行前会先解密最新数据，
/// 确保用户点击"刷新"按钮时能看到微信的最新会话状态。
#[tauri::command]
pub async fn refresh_wechat_sessions() -> Result<Vec<serde_json::Value>, String> {
    helpers::run_blocking(|| {
        use std::io::Read;

        // 与 monitor 的实时解密互斥：避免并发替换同一个 session.db 解密副本
        let _guard = helpers::session_refresh_lock();
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let keys = std::sync::Arc::new(
            crate::wechat::keys::Keys::from_file(&cfg.keys_file)
                .map_err(|e| format!("读取密钥文件失败: {}", e))?,
        );
        let session_key = keys
            .get_key_info("session/session.db")
            .ok_or("密钥文件缺少 session.db".to_string())?;

        let session_db = cfg.db_dir.join("session").join("session.db");
        let decrypted_session = cfg.decrypted_dir.join("session").join("session.db");

        // 派生加密密钥（兼容 v4.0 和 wx_key_v4.1）
        let enc_key = if keys.key_format.as_deref() == Some("wx_key_v4.1") {
            let mut f = std::fs::File::open(&session_db)
                .map_err(|e| format!("打开 session.db 失败: {}", e))?;
            let mut salt = vec![0u8; crate::wechat::crypto::SALT_SZ];
            f.read_exact(&mut salt)
                .map_err(|e| format!("读取 salt 失败: {}", e))?;
            crate::wechat::crypto::derive_enc_key(
                &hex::decode(&session_key.enc_key).map_err(|e| format!("hex 解码失败: {}", e))?,
                &salt,
                keys.key_format.as_deref(),
            )
        } else {
            hex::decode(&session_key.enc_key).map_err(|e| format!("hex 解码失败: {}", e))?
        };

        let wal_path = session_db.with_extension("db-wal");
        // mtime 门控：源库（db + wal）未比解密副本新时直接跳过全量解密，
        // 避免用户反复点击"刷新"每次都解密数百 MB 的 session.db
        let src_newest = crate::wechat::modules::common::file_sig(&session_db)
            .map(|(t, _)| t)
            .max(crate::wechat::modules::common::file_sig(&wal_path).map(|(t, _)| t));
        let out_mtime =
            crate::wechat::modules::common::file_sig(&decrypted_session).map(|(t, _)| t);
        if let (Some(s), Some(o)) = (src_newest, out_mtime) {
            if s <= o {
                log::debug!("[refresh] session.db 解密副本已最新，跳过全量解密");
                let sessions =
                    crate::wechat::modules::sessions::get_session_list(&cfg.decrypted_dir)?;
                return serde_json::to_value(sessions)
                    .map_err(|e| e.to_string())?
                    .as_array()
                    .cloned()
                    .ok_or_else(|| "序列化失败".to_string());
            }
        }

        // 解密到临时文件（全量 + WAL），再原子替换，避免中途读取到损坏文件
        let temp_path = decrypted_session.with_extension("db.refresh_temp");
        crate::wechat::crypto::full_decrypt(&session_db, &temp_path, &enc_key)
            .map_err(|e| format!("解密 session.db 失败: {}", e))?;

        if wal_path.exists() {
            if let Err(e) = crate::wechat::crypto::decrypt_wal(&wal_path, &temp_path, &enc_key) {
                log::warn!("[refresh] WAL 增量解密失败: {}", e);
            }
        }

        // 健康校验：源库被写入中断时解密结果可能损坏，不发布坏副本
        if !crate::wechat::db_cache::sqlite_healthy(&temp_path) {
            let _ = std::fs::remove_file(&temp_path);
            return Err("解密结果无效（源库可能正在被写入），请稍后重试".to_string());
        }

        // 原子替换
        let _ = std::fs::remove_file(&decrypted_session);
        std::fs::rename(&temp_path, &decrypted_session)
            .map_err(|e| format!("替换解密文件失败: {}", e))?;

        log::info!("[refresh] 会话数据库已强制刷新");

        // 读取并返回会话列表
        let sessions = crate::wechat::modules::sessions::get_session_list(&cfg.decrypted_dir)?;
        serde_json::to_value(sessions)
            .map_err(|e| e.to_string())?
            .as_array()
            .cloned()
            .ok_or_else(|| "序列化失败".to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_conversation_messages(
    username: String,
    _page: Option<usize>,
    page_size: Option<usize>,
    before_sort_seq: Option<i64>,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let self_username = cfg.wxid().unwrap_or_default();
        let result = crate::wechat::modules::messages::get_conversation_messages(
            &cfg.decrypted_dir,
            &username,
            &self_username,
            before_sort_seq,
            page_size.unwrap_or(10),
        )?;
        serde_json::to_value(result).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_conversation_messages(username: String) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let table = crate::wechat::modules::common::msg_table_name(&username);
        let mut dbs = crate::wechat::modules::common::find_db_files(&cfg.decrypted_dir, "message_");
        dbs.extend(crate::wechat::modules::common::find_db_files(&cfg.decrypted_dir, "biz_message_"));
        dbs.sort();
        dbs.dedup();
        dbs.retain(|p| !p.to_string_lossy().contains("monitor_cache"));
        let mut total: i64 = 0;
        let mut details = Vec::new();
        for path in dbs {
            let conn = match helpers::open_writable_db(&path) {
                Ok(c) => c,
                Err(e) => { log::warn!("[data-mgmt] {}", e); continue; }
            };
            if !crate::wechat::modules::common::table_exists(&conn, &table) { continue; }
            match conn.execute(&format!("DELETE FROM \"{}\"", table), []) {
                Ok(n) => {
                    if n > 0 {
                        details.push(serde_json::json!({ "db": path.file_name().map(|f| f.to_string_lossy().to_string()), "deleted": n }));
                        total += n as i64;
                    }
                }
                Err(e) => log::warn!("[data-mgmt] 删除失败 {}: {}", path.display(), e),
            }
        }
        log::info!("[data-mgmt] 清空会话 {} 的本地聊天记录，共 {} 行", username, total);
        Ok(serde_json::json!({ "username": username, "deleted": total, "details": details }))
    })
    .await
}

#[tauri::command]
pub async fn delete_favorite_items(local_ids: Vec<i64>) -> Result<serde_json::Value, String> {
    if local_ids.is_empty() {
        return Err("未选择要删除的收藏条目".to_string());
    }
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let db_path = cfg.decrypted_dir.join("favorite").join("favorite.db");
        if !db_path.exists() {
            return Err("收藏数据库不存在".to_string());
        }
        let conn = helpers::open_writable_db(&db_path)?;
        let mut deleted = 0usize;
        for id in &local_ids {
            match conn.execute(
                "DELETE FROM fav_db_item WHERE local_id = ?1",
                rusqlite::params![id],
            ) {
                Ok(n) => deleted += n,
                Err(e) => log::warn!("[data-mgmt] 删除收藏 {} 失败: {}", id, e),
            }
        }
        log::info!(
            "[data-mgmt] 删除收藏 {} 条（请求 {} 条）",
            deleted,
            local_ids.len()
        );
        Ok(serde_json::json!({ "deleted": deleted, "requested": local_ids.len() }))
    })
    .await
}

/// 清除指定会话的草稿（仅写解密副本，微信源库不受影响；
/// 微信端下次同步/刷新时可能恢复，真正清空需在微信客户端操作）
#[tauri::command]
pub async fn clear_session_draft(username: String) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let db_path = cfg.decrypted_dir.join("session").join("session.db");
        if !db_path.exists() {
            return Err("解密 session.db 不存在".to_string());
        }
        let conn = helpers::open_writable_db(&db_path)?;
        let updated = conn
            .execute(
                "UPDATE SessionTable SET draft = '' WHERE username = ?1",
                rusqlite::params![username],
            )
            .map_err(|e| format!("清除草稿失败: {}", e))?;
        log::info!("[draft] 已清除会话 {} 的草稿", username);
        Ok(serde_json::json!({ "username": username, "updated": updated }))
    })
    .await
}

/// 清空所有会话的草稿（解密副本），返回被清除的草稿列表，
/// 供前端记录"已清除"状态以屏蔽源库恢复的残留。
#[tauri::command]
pub async fn clear_all_session_drafts() -> Result<serde_json::Value, String> {
    helpers::run_blocking(|| {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let db_path = cfg.decrypted_dir.join("session").join("session.db");
        if !db_path.exists() {
            return Err("解密 session.db 不存在".to_string());
        }
        let conn = helpers::open_writable_db(&db_path)?;
        // 先读取当前所有草稿（前端记录后用于屏蔽源库恢复的残留）
        let mut stmt = conn
            .prepare(
                "SELECT username, draft FROM SessionTable \
                 WHERE draft IS NOT NULL AND length(draft) > 0",
            )
            .map_err(|e| format!("查询草稿失败: {}", e))?;
        let drafts: Vec<serde_json::Value> = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "username": row.get::<_, String>(0).unwrap_or_default(),
                    "draft": crate::wechat::modules::common::get_bytes(row, 1)
                        .map(|b| crate::wechat::modules::common::decode_blob_text(&b))
                        .unwrap_or_default(),
                }))
            })
            .map_err(|e| format!("读取草稿失败: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        let updated = conn
            .execute(
                "UPDATE SessionTable SET draft = '' \
                 WHERE draft IS NOT NULL AND length(draft) > 0",
                [],
            )
            .map_err(|e| format!("清除草稿失败: {}", e))?;
        log::info!("[draft] 已清空 {} 个会话草稿", updated);
        Ok(serde_json::json!({ "updated": updated, "drafts": drafts }))
    })
    .await
}

#[tauri::command]
pub async fn get_session_snapshots() -> Result<Vec<serde_json::Value>, String> {
    helpers::run_blocking(|| {
        use std::io::Read;
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let decrypted = cfg.decrypted_dir.join("session").join("session.db");
        if !decrypted.exists() {
            return Err("解密 session.db 不存在".to_string());
        }
        let mut hdr = [0u8; 16];
        if let Ok(mut f) = std::fs::File::open(&decrypted) {
            if f.read_exact(&mut hdr).is_err() || &hdr != b"SQLite format 3\0" {
                return Err("解密 session.db 头部损坏，请重新解密".to_string());
            }
        }
        let conn = rusqlite::Connection::open_with_flags(
            &decrypted,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|e| format!("打开数据库失败: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT username, last_timestamp, last_msg_locald_id, last_msg_type, last_msg_sender, \
             last_sender_display_name, unread_count, summary FROM SessionTable \
             WHERE last_timestamp > 0 ORDER BY last_timestamp DESC LIMIT 50"
        ).map_err(|e| format!("查询失败: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "username": row.get::<_, String>(0).unwrap_or_default(),
                    "last_timestamp": row.get::<_, i64>(1).unwrap_or(0),
                    "last_msg_locald_id": row.get::<_, i64>(2).unwrap_or(0),
                    "last_msg_type": row.get::<_, i64>(3).unwrap_or(0),
                    "last_msg_sender": row.get::<_, String>(4).unwrap_or_default(),
                    "last_sender_display_name": row.get::<_, String>(5).unwrap_or_default(),
                    "unread_count": row.get::<_, i64>(6).unwrap_or(0),
                    "summary": row.get::<_, String>(7).unwrap_or_default(),
                }))
            })
            .map_err(|e| format!("读取失败: {}", e))?;
        let mut result = Vec::new();
        for v in rows.flatten() {
            result.push(v);
        }
        Ok(result)
    })
    .await
}
