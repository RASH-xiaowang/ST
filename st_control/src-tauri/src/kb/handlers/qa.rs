// ============================================================
// 知识库管理 — QA 会话（RAG 持久化）命令
// 自 handlers.rs 拆分：问答会话创建/列表/消息/删除。
// ============================================================

use crate::kb::db::KbDatabase;
use serde::Serialize;
use tauri::State;

// ════════════════════════════════════════════════════════════
// QA 会话（RAG 持久化）
// ════════════════════════════════════════════════════════════

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct QaSessionItem {
    pub id: i64,
    pub kbId: Option<i64>,
    pub title: Option<String>,
    pub createdAt: String,
    pub updatedAt: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct QaMessageItem {
    pub id: i64,
    pub role: String,
    pub content: Option<String>,
    pub citations: Option<String>,
    pub createdAt: String,
}

/// 创建（或复用）当前用户在某知识库的问答会话
#[tauri::command]
pub async fn kb_qa_create_session(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: Option<i64>,
    title: Option<String>,
) -> Result<i64, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let conn = db.conn_lock();
    conn.execute(
        "INSERT INTO qa_sessions (user_id, kb_id, title) VALUES (?1,?2,?3)",
        rusqlite::params![uid, kb_id, title],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

/// 列出当前用户的问答会话
#[tauri::command]
pub async fn kb_qa_list_sessions(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
) -> Result<Vec<QaSessionItem>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let conn = db.conn_lock();
    let mut stmt = conn.prepare(
        "SELECT id, kb_id, title, created_at, updated_at FROM qa_sessions WHERE user_id = ?1 ORDER BY updated_at DESC"
    ).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![uid], |row| {
            Ok(QaSessionItem {
                id: row.get(0)?,
                kbId: row.get(1)?,
                title: row.get(2)?,
                createdAt: row.get(3)?,
                updatedAt: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 列出某会话的消息
#[tauri::command]
pub async fn kb_qa_list_messages(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    session_id: i64,
) -> Result<Vec<QaMessageItem>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    // 校验会话归属
    let conn = db.conn_lock();
    let owner: Option<i64> = conn
        .query_row(
            "SELECT user_id FROM qa_sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .ok();
    if owner != Some(uid) {
        return Err("无权限：该会话不属于当前用户".to_string());
    }
    let mut stmt = conn.prepare(
        "SELECT id, role, content, citations, created_at FROM qa_messages WHERE session_id = ?1 ORDER BY id ASC"
    ).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![session_id], |row| {
            Ok(QaMessageItem {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                citations: row.get(3)?,
                createdAt: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 删除问答会话
#[tauri::command]
pub async fn kb_qa_delete_session(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    session_id: i64,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let conn = db.conn_lock();
    let owner: Option<i64> = conn
        .query_row(
            "SELECT user_id FROM qa_sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .ok();
    if owner != Some(uid) {
        return Err("无权限：该会话不属于当前用户".to_string());
    }
    conn.execute(
        "DELETE FROM qa_sessions WHERE id = ?1",
        rusqlite::params![session_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
