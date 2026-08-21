//! 微信消息全文搜索索引（FTS5）
//!
//! 对标 WeChatDataAnalysis 的 chat_search_index：
//! 从解密消息库提取文本消息构建 FTS5 索引（`st_result/wechat_search.db`），
//! 搜索走索引而非全表扫描，大幅提升大库搜索速度。
//!
//! 索引结构：
//! - `message_fts`：FTS5 虚拟表（text 可检索，username/时间等 UNINDEXED 列）
//! - `message_meta`：普通表（完整字段，用于结果回填）
//! - `meta`：构建元信息（状态/时间/来源/行数）

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::PathBuf;
use std::time::Instant;

fn index_db_path() -> PathBuf {
    crate::wechat::config::default_st_result_dir().join("wechat_search.db")
}

fn open_index(writable: bool) -> Result<Connection, String> {
    let p = index_db_path();
    let flags = if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };
    Connection::open_with_flags(&p, flags).map_err(|e| format!("打开搜索索引库失败: {}", e))
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE VIRTUAL TABLE IF NOT EXISTS message_fts USING fts5(
            text,
            username UNINDEXED,
            create_time UNINDEXED,
            local_id UNINDEXED,
            sort_seq UNINDEXED,
            tokenize='unicode61'
        );
        CREATE TABLE IF NOT EXISTS message_meta (
            rowid INTEGER PRIMARY KEY,
            text TEXT NOT NULL,
            username TEXT NOT NULL,
            create_time INTEGER NOT NULL DEFAULT 0,
            sort_seq INTEGER NOT NULL DEFAULT 0,
            local_id INTEGER NOT NULL DEFAULT 0
        );
        "#,
    )
    .map_err(|e| format!("初始化索引表失败: {}", e))
}

/// 索引状态
pub fn get_search_index_status() -> serde_json::Value {
    let p = index_db_path();
    if !p.is_file() {
        return serde_json::json!({ "exists": false, "rows": 0, "built_at": null });
    }
    let Ok(conn) = open_index(false) else {
        return serde_json::json!({ "exists": true, "rows": 0, "built_at": null });
    };
    let rows = conn
        .query_row("SELECT COUNT(*) FROM message_meta", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap_or(0);
    let built_at: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key='built_at'", [], |r| {
            r.get(0)
        })
        .optional()
        .ok()
        .flatten();
    serde_json::json!({ "exists": true, "rows": rows, "built_at": built_at })
}

/// 构建 / 重建搜索索引
pub fn build_search_index(force: bool) -> Result<serde_json::Value, String> {
    let cfg =
        crate::wechat::config::WeChatConfig::load().map_err(|e| format!("读取配置失败: {}", e))?;
    let decrypted = cfg.decrypted_dir.clone();
    let conn = open_index(true)?;
    init_schema(&conn)?;

    if !force {
        let existing: i64 = conn
            .query_row("SELECT COUNT(*) FROM message_meta", [], |r| r.get(0))
            .unwrap_or(0);
        if existing > 0 {
            return Ok(serde_json::json!({
                "status": "exists",
                "rows": existing,
                "message": "索引已存在，使用 force=true 可重建",
            }));
        }
    }

    // 彻底重建索引表，保证 FTS rowid 与 meta rowid 干净一致
    let _ = conn.execute("DROP TABLE IF EXISTS message_fts", []);
    let _ = conn.execute("DROP TABLE IF EXISTS message_meta", []);
    init_schema(&conn)?;
    let _ = conn.execute("DELETE FROM meta WHERE key='built_at'", []);

    let usernames = crate::wechat::annual::load_session_usernames(&decrypted);
    let started = Instant::now();
    let mut total = 0i64;

    // 事务批量插入：显式 BEGIN/COMMIT，避免每行自动提交
    conn.execute("BEGIN", [])
        .map_err(|e| format!("开启事务失败: {}", e))?;
    let mut batch: Vec<(String, String, i64, i64, i64)> = Vec::new();
    let flush = |conn: &Connection,
                 batch: &mut Vec<(String, String, i64, i64, i64)>|
     -> Result<(), String> {
        if batch.is_empty() {
            return Ok(());
        }
        let rowids: Vec<i64> = {
            let mut rids = Vec::with_capacity(batch.len());
            for (text, username, create_time, local_id, sort_seq) in batch.iter() {
                conn.execute(
                    "INSERT INTO message_meta(text, username, create_time, sort_seq, local_id) VALUES(?1,?2,?3,?4,?5)",
                    rusqlite::params![text, username, create_time, sort_seq, local_id],
                )
                .map_err(|e| format!("写入索引失败: {}", e))?;
                rids.push(conn.last_insert_rowid());
            }
            rids
        };
        let params: Vec<Vec<rusqlite::types::Value>> = batch
            .iter()
            .zip(rowids.iter())
            .map(
                |((text, username, create_time, local_id, sort_seq), rowid)| {
                    vec![
                        rusqlite::types::Value::from(*rowid),
                        rusqlite::types::Value::from(text.clone()),
                        rusqlite::types::Value::from(username.clone()),
                        rusqlite::types::Value::from(*create_time),
                        rusqlite::types::Value::from(*local_id),
                        rusqlite::types::Value::from(*sort_seq),
                    ]
                },
            )
            .collect();
        for p in params {
            conn.execute(
                "INSERT INTO message_fts(rowid, text, username, create_time, local_id, sort_seq) VALUES(?1,?2,?3,?4,?5,?6)",
                rusqlite::params_from_iter(p.iter()),
            )
            .map_err(|e| format!("写入 FTS 索引失败: {}", e))?;
        }
        batch.clear();
        Ok(())
    };

    for username in &usernames {
        let table = crate::wechat::modules::common::msg_table_name(username);
        let mut dbs = crate::wechat::modules::common::find_db_files(&decrypted, "message_");
        dbs.retain(|p| crate::wechat::modules::common::is_message_shard_file(p));
        dbs.sort();
        for db_path in dbs {
            let Ok(conn2) = crate::wechat::modules::common::open_readonly_db(&db_path) else {
                continue;
            };
            if !crate::wechat::modules::common::table_exists(&conn2, &table) {
                continue;
            }
            let sql = format!(
                "SELECT local_id, create_time, sort_seq, message_content FROM \"{}\" WHERE local_type=1",
                table
            );
            let Ok(mut stmt) = conn2.prepare(&sql) else {
                continue;
            };
            let Ok(mut rows) = stmt.query([]) else {
                continue;
            };
            while let Ok(Some(row)) = rows.next() {
                let local_id: i64 = row.get(0).unwrap_or(0);
                let create_time: i64 = row.get(1).unwrap_or(0);
                let sort_seq: i64 = row.get(2).unwrap_or(local_id);
                let content: Option<Vec<u8>> = crate::wechat::modules::common::get_bytes(row, 3);
                let text = content
                    .as_deref()
                    .map(crate::wechat::modules::common::decode_blob_text)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if text.is_empty() || text.starts_with('<') {
                    continue;
                }
                batch.push((text, username.clone(), create_time, local_id, sort_seq));
                if batch.len() >= 500 {
                    flush(&conn, &mut batch)?;
                }
            }
        }
        if batch.len() >= 500 {
            flush(&conn, &mut batch)?;
        }
    }
    flush(&conn, &mut batch)?;
    conn.execute("COMMIT", [])
        .map_err(|e| format!("提交事务失败: {}", e))?;
    total = conn
        .query_row("SELECT COUNT(*) FROM message_meta", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap_or(total);

    let built_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let _ = conn.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES('built_at', ?1)",
        [&built_at],
    );
    Ok(serde_json::json!({
        "status": "ok",
        "rows": total,
        "built_at": built_at,
        "elapsed_ms": started.elapsed().as_millis(),
    }))
}

/// 用索引搜索文本消息
pub fn search_indexed(query: &str, limit: usize) -> Result<serde_json::Value, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(serde_json::json!({ "hits": [], "total": 0, "indexed": false }));
    }
    let conn = open_index(false)?;
    let exists: i64 = conn
        .query_row("SELECT COUNT(*) FROM message_meta", [], |r| r.get(0))
        .unwrap_or(0);
    if exists == 0 {
        return Ok(serde_json::json!({ "hits": [], "total": 0, "indexed": false }));
    }
    // 显示名映射（前端搜索结果需要 name）
    let decrypted_dir = crate::wechat::config::WeChatConfig::load()
        .ok()
        .map(|c| c.decrypted_dir)
        .unwrap_or_default();
    let usernames = crate::wechat::annual::load_session_usernames(&decrypted_dir);
    let names = crate::wechat::annual::load_display_names(&decrypted_dir, &usernames);
    // FTS5 查询：普通词直接 MATCH；含特殊字符时用双引号短语
    let match_expr = if q.chars().all(|c| c.is_alphanumeric() || c.is_whitespace()) {
        q.to_string()
    } else {
        format!("\"{}\"", q.replace('"', "\"\""))
    };
    let limit = limit.min(300) as i64;
    let sql = "SELECT text, username, create_time, local_id FROM message_fts \
         WHERE message_fts MATCH ?1 ORDER BY rank LIMIT ?2"
        .to_string();
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("索引查询失败: {}", e))?;
    let hits: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![match_expr, limit], |row| {
            let text = row.get::<_, String>(0).unwrap_or_default();
            let username = row.get::<_, String>(1).unwrap_or_default();
            let create_time = row.get::<_, i64>(2).unwrap_or(0);
            let local_id = row.get::<_, i64>(3).unwrap_or(0);
            let display = names
                .get(&username)
                .cloned()
                .unwrap_or_else(|| username.clone());
            Ok(serde_json::json!({
                "text": text,
                "username": username,
                "create_time": create_time,
                "local_id": local_id,
                "name": display,
                "time": crate::wechat::modules::common::format_full_time(create_time),
                "snippet": text.chars().take(120).collect::<String>(),
            }))
        })
        .map_err(|e| format!("索引查询失败: {}", e))?
        .flatten()
        .collect();
    Ok(serde_json::json!({ "hits": hits, "total": hits.len(), "indexed": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构建索引并搜索验证（真实数据，耗时较长）
    #[test]
    #[cfg(target_os = "windows")]
    #[ignore = "构建全量索引耗时"]
    fn smoke_index_build_and_search() {
        let cfg = crate::wechat::config::WeChatConfig::load().expect("配置");
        let r = build_search_index(true).expect("构建索引失败");
        println!("索引构建: rows={} elapsed={}ms", r["rows"], r["elapsed_ms"]);
        assert!(r["rows"].as_i64().unwrap_or(0) > 0);

        // 搜索一个常见词验证索引可用
        let s = search_indexed("我", 10).expect("索引搜索失败");
        println!(
            "搜索 '我': indexed={} hits={}",
            s["indexed"],
            s["hits"].as_array().map(|a| a.len()).unwrap_or(0)
        );
        assert_eq!(s["indexed"].as_bool(), Some(true));
        let _ = cfg;
    }
}
