use crate::wechat::modules::common;
// ============================================================
// 消息编辑侧库（message_edits.db）
// 记录被编辑消息的原始快照，支持一键恢复原消息。
// 数据位置：<st_result>/message_edits.db
// ============================================================

use rusqlite::Connection;
use std::path::PathBuf;

pub fn edit_db_path() -> PathBuf {
    crate::wechat::config::default_st_result_dir().join("message_edits.db")
}

fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS message_edits (
            account TEXT NOT NULL,
            session_id TEXT NOT NULL,
            db TEXT NOT NULL,
            table_name TEXT NOT NULL,
            local_id INTEGER NOT NULL,
            first_edited_at INTEGER NOT NULL,
            last_edited_at INTEGER NOT NULL,
            edit_count INTEGER NOT NULL,
            original_msg_json TEXT NOT NULL,
            edited_cols_json TEXT,
            PRIMARY KEY (account, session_id, db, table_name, local_id)
        );
        CREATE INDEX IF NOT EXISTS idx_edits_account_session ON message_edits(account, session_id);
        CREATE INDEX IF NOT EXISTS idx_edits_account_last ON message_edits(account, last_edited_at);",
    )
}

fn connect() -> Result<Connection, String> {
    let path = edit_db_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(&path).map_err(|e| format!("打开编辑记录库失败: {}", e))?;
    ensure_schema(&conn).map_err(|e| format!("初始化编辑记录库失败: {}", e))?;
    Ok(conn)
}

/// 记录消息被编辑（首次写入原始快照；后续仅累加次数）
pub fn record_edit(
    account: &str,
    session_id: &str,
    db: &str,
    table_name: &str,
    local_id: i64,
    original_msg_json: &str,
) -> Result<(), String> {
    let conn = connect()?;
    let ts = common::now_ms();
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM message_edits WHERE account=?1 AND session_id=?2 AND db=?3 AND table_name=?4 AND local_id=?5)",
            rusqlite::params![account, session_id, db, table_name, local_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        conn.execute(
            "INSERT INTO message_edits(account, session_id, db, table_name, local_id, first_edited_at, last_edited_at, edit_count, original_msg_json, edited_cols_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 1, ?7, '[\"message_content\"]')",
            rusqlite::params![account, session_id, db, table_name, local_id, ts, original_msg_json],
        )
        .map_err(|e| e.to_string())?;
    } else {
        conn.execute(
            "UPDATE message_edits SET last_edited_at=?1, edit_count=edit_count+1
             WHERE account=?2 AND session_id=?3 AND db=?4 AND table_name=?5 AND local_id=?6",
            rusqlite::params![ts, account, session_id, db, table_name, local_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 查询消息编辑状态
pub fn get_edit_status(
    account: &str,
    session_id: &str,
    db: &str,
    table_name: &str,
    local_id: i64,
) -> Option<serde_json::Value> {
    let conn = connect().ok()?;
    let row = conn
        .query_row(
            "SELECT edit_count, first_edited_at, last_edited_at, original_msg_json
             FROM message_edits
             WHERE account=?1 AND session_id=?2 AND db=?3 AND table_name=?4 AND local_id=?5 LIMIT 1",
            rusqlite::params![account, session_id, db, table_name, local_id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                ))
            },
        )
        .ok()?;
    Some(serde_json::json!({
        "modified": true,
        "edit_count": row.0,
        "first_edited_at": row.1,
        "last_edited_at": row.2,
        "original_msg_json": row.3,
    }))
}

/// 读取原始消息快照（JSON 字符串），不存在返回 None
pub fn get_original_snapshot(
    account: &str,
    session_id: &str,
    db: &str,
    table_name: &str,
    local_id: i64,
) -> Option<String> {
    let conn = connect().ok()?;
    conn.query_row(
        "SELECT original_msg_json FROM message_edits
         WHERE account=?1 AND session_id=?2 AND db=?3 AND table_name=?4 AND local_id=?5 LIMIT 1",
        rusqlite::params![account, session_id, db, table_name, local_id],
        |r| r.get(0),
    )
    .ok()
}

/// 删除编辑记录（恢复原消息后调用）
pub fn delete_edit(
    account: &str,
    session_id: &str,
    db: &str,
    table_name: &str,
    local_id: i64,
) -> Result<bool, String> {
    let conn = connect()?;
    let n = conn
        .execute(
            "DELETE FROM message_edits
             WHERE account=?1 AND session_id=?2 AND db=?3 AND table_name=?4 AND local_id=?5",
            rusqlite::params![account, session_id, db, table_name, local_id],
        )
        .map_err(|e| e.to_string())?;
    Ok(n > 0)
}

/// 列出某会话全部已编辑消息的 local_id
pub fn list_session_edits(
    account: &str,
    session_id: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = connect()?;
    let mut stmt = conn
        .prepare(
            "SELECT db, table_name, local_id, edit_count, last_edited_at
             FROM message_edits
             WHERE account=?1 AND session_id=?2
             ORDER BY last_edited_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![account, session_id], |r| {
            Ok(serde_json::json!({
                "db": r.get::<_, String>(0)?,
                "table_name": r.get::<_, String>(1)?,
                "local_id": r.get::<_, i64>(2)?,
                "edit_count": r.get::<_, i64>(3)?,
                "last_edited_at": r.get::<_, i64>(4)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows.flatten() {
        out.push(r);
    }
    Ok(out)
}
