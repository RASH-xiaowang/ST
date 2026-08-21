// ============================================================
// 知识库管理 — 处理任务（jobs）命令
// 自 handlers.rs 拆分：任务列表 / 任务日志查询。
// ============================================================

use crate::kb::db::KbDatabase;
use serde::Serialize;
use tauri::State;

// 处理任务中心（processing_jobs 进度查询）
// ════════════════════════════════════════════════════════════

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct JobItem {
    pub id: i64,
    pub docId: i64,
    pub docTitle: String,
    pub stage: String,
    pub progress: f64,
    pub error: Option<String>,
    pub createdAt: String,
    pub updatedAt: String,
}

/// 列出当前用户可见知识库下的处理任务（独立函数，便于集成测试；命令为薄封装）。
/// `kb_id=None` 表示"全部可见知识库"，LIMIT 使用匿名占位符与显式绑定，
/// 避免旧实现把数字拼成 `?50` 这类编号参数却未绑定导致的必然失败。
pub fn list_jobs(
    db: &KbDatabase,
    uid: i64,
    kb_id: Option<i64>,
    limit: i64,
) -> Result<Vec<JobItem>, String> {
    let lim = limit.clamp(1, 200);
    let visible = crate::kb::retrieval::visible_kb_ids(db, uid);
    if visible.is_empty() {
        return Ok(Vec::new());
    }
    let conn = db.conn_lock();
    let placeholders = visible.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = if kb_id.is_some() {
        "SELECT j.id, j.doc_id, d.title, j.stage, j.progress, j.error, j.created_at, j.updated_at
             FROM processing_jobs j JOIN documents d ON d.id = j.doc_id
             WHERE d.kb_id = ?1 ORDER BY j.id DESC LIMIT ?2"
            .to_string()
    } else {
        format!(
            "SELECT j.id, j.doc_id, d.title, j.stage, j.progress, j.error, j.created_at, j.updated_at
             FROM processing_jobs j JOIN documents d ON d.id = j.doc_id
             WHERE d.kb_id IN ({}) ORDER BY j.id DESC LIMIT ?",
            placeholders
        )
    };
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(k) = kb_id {
        let rows = stmt
            .query_map(rusqlite::params![k, lim], |row| {
                Ok(JobItem {
                    id: row.get(0)?,
                    docId: row.get(1)?,
                    docTitle: row.get(2)?,
                    stage: row.get(3)?,
                    progress: row.get(4)?,
                    error: row.get(5)?,
                    createdAt: row.get(6)?,
                    updatedAt: row.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?;
        out.extend(rows.filter_map(|r| r.ok()));
    } else {
        let mut binds: Vec<&dyn rusqlite::types::ToSql> = visible
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        binds.push(&lim);
        let rows = stmt
            .query_map(binds.as_slice(), |row| {
                Ok(JobItem {
                    id: row.get(0)?,
                    docId: row.get(1)?,
                    docTitle: row.get(2)?,
                    stage: row.get(3)?,
                    progress: row.get(4)?,
                    error: row.get(5)?,
                    createdAt: row.get(6)?,
                    updatedAt: row.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?;
        out.extend(rows.filter_map(|r| r.ok()));
    }
    Ok(out)
}

#[tauri::command]
pub async fn kb_list_jobs(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<JobItem>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    list_jobs(&db, uid, kb_id, limit.unwrap_or(50))
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct JobLogItem {
    pub id: i64,
    pub level: String,
    pub message: String,
    pub detail: Option<String>,
    pub createdAt: String,
}

/// 查看处理任务的详细日志（processing_logs）
#[tauri::command]
pub async fn kb_get_job_logs(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    job_id: i64,
) -> Result<Vec<JobLogItem>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let visible = crate::kb::retrieval::visible_kb_ids(&db, uid);
    if visible.is_empty() {
        return Ok(Vec::new());
    }
    let conn = db.conn_lock();
    let placeholders = visible.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT pl.id, pl.level, pl.message, pl.detail, pl.created_at
         FROM processing_logs pl
         JOIN processing_jobs j ON j.id = pl.job_id
         JOIN documents d ON d.id = j.doc_id
         WHERE pl.job_id = ?1 AND d.kb_id IN ({})
         ORDER BY pl.id ASC",
        placeholders
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut binds: Vec<&dyn rusqlite::types::ToSql> = vec![&job_id];
    for v in &visible {
        binds.push(v as &dyn rusqlite::types::ToSql);
    }
    let rows = stmt
        .query_map(binds.as_slice(), |row| {
            Ok(JobLogItem {
                id: row.get(0)?,
                level: row.get(1)?,
                message: row.get(2)?,
                detail: row.get(3)?,
                createdAt: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}
