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
    let lim = limit.clamp(1, 5000);
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

/// 统计任务总数（与 list_jobs 同口径），供前端展示「共 N 条」并判断是否被截断
pub fn count_jobs(db: &KbDatabase, uid: i64, kb_id: Option<i64>) -> i64 {
    let visible = crate::kb::retrieval::visible_kb_ids(db, uid);
    if visible.is_empty() {
        return 0;
    }
    let conn = db.conn_lock();
    let sql = if kb_id.is_some() {
        "SELECT COUNT(*) FROM processing_jobs j JOIN documents d ON d.id = j.doc_id WHERE d.kb_id = ?1"
            .to_string()
    } else {
        let ph = visible.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        format!(
            "SELECT COUNT(*) FROM processing_jobs j JOIN documents d ON d.id = j.doc_id WHERE d.kb_id IN ({})",
            ph
        )
    };
    let r = if let Some(k) = kb_id {
        conn.query_row(&sql, rusqlite::params![k], |r| r.get::<_, i64>(0))
    } else {
        let binds: Vec<&dyn rusqlite::types::ToSql> = visible
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        conn.query_row(&sql, binds.as_slice(), |r| r.get::<_, i64>(0))
    };
    r.unwrap_or(0)
}

#[tauri::command]
pub async fn kb_list_jobs(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: Option<i64>,
    limit: Option<i64>,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let items = list_jobs(&db, uid, kb_id, limit.unwrap_or(50))?;
    let total = count_jobs(&db, uid, kb_id);
    Ok(serde_json::json!({ "items": items, "total": total }))
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

/// 清理活动数据：
/// - jobs    ：删除已完成 / 失败的处理任务及其日志（保留排队/执行中的任务）
/// - logs    ：删除当前用户可见知识库的处理日志
/// - history ：清空当前用户的检索历史
#[tauri::command]
pub async fn kb_clear_activity(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    scope: String,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let visible = crate::kb::retrieval::visible_kb_ids(&db, uid);
    let conn = db.conn_lock();
    let mut cleared = serde_json::Map::new();
    match scope.as_str() {
        "jobs" => {
            if visible.is_empty() {
                cleared.insert("jobs".into(), 0.into());
                cleared.insert("logs".into(), 0.into());
            } else {
                let ph = visible.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let job_ids: Vec<i64> = {
                    let mut stmt = conn
                        .prepare(&format!(
                            "SELECT j.id FROM processing_jobs j JOIN documents d ON d.id = j.doc_id
                             WHERE j.stage IN ('done','failed') AND d.kb_id IN ({})",
                            ph
                        ))
                        .map_err(|e| e.to_string())?;
                    let binds: Vec<&dyn rusqlite::types::ToSql> = visible
                        .iter()
                        .map(|v| v as &dyn rusqlite::types::ToSql)
                        .collect();
                    let rows = stmt
                        .query_map(binds.as_slice(), |r| r.get::<_, i64>(0))
                        .map_err(|e| e.to_string())?;
                    rows.filter_map(|r| r.ok()).collect()
                };
                let n = job_ids.len();
                if n > 0 {
                    let ids = job_ids
                        .iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    conn.execute(
                        &format!("DELETE FROM processing_logs WHERE job_id IN ({})", ids),
                        [],
                    )
                    .map_err(|e| e.to_string())?;
                    conn.execute(
                        &format!("DELETE FROM processing_jobs WHERE id IN ({})", ids),
                        [],
                    )
                    .map_err(|e| e.to_string())?;
                }
                cleared.insert("jobs".into(), (n as i64).into());
            }
        }
        "logs" => {
            if visible.is_empty() {
                cleared.insert("logs".into(), 0.into());
            } else {
                let ph = visible.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let sql = format!(
                    "DELETE FROM processing_logs WHERE job_id IN
                     (SELECT j.id FROM processing_jobs j JOIN documents d ON d.id = j.doc_id
                      WHERE d.kb_id IN ({}))",
                    ph
                );
                let binds: Vec<&dyn rusqlite::types::ToSql> = visible
                    .iter()
                    .map(|v| v as &dyn rusqlite::types::ToSql)
                    .collect();
                let n = conn
                    .execute(&sql, binds.as_slice())
                    .map_err(|e| e.to_string())?;
                cleared.insert("logs".into(), (n as i64).into());
            }
        }
        "history" => {
            let n = conn
                .execute(
                    "DELETE FROM search_logs WHERE user_id = ?1",
                    rusqlite::params![uid],
                )
                .map_err(|e| e.to_string())?;
            cleared.insert("history".into(), (n as i64).into());
        }
        _ => return Err("未知的清理范围".to_string()),
    }
    drop(conn);
    Ok(serde_json::Value::Object(cleared))
}

/// 停止后台处理：把进行中/待处理的任务标记为「已手动停止」，
/// 并对知识库置位批量取消标记，让正在运行的批量 Wiki 提炼在下一个文档处停止。
/// kb_id=None 时作用于全部可见知识库。
#[tauri::command]
pub async fn kb_stop_processing(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: Option<i64>,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let visible = crate::kb::retrieval::visible_kb_ids(&db, uid);
    if visible.is_empty() {
        return Ok(serde_json::json!({ "stopped": 0 }));
    }
    let conn = db.conn_lock();
    let target_kbs: Vec<i64> = match kb_id {
        Some(k) if visible.contains(&k) => vec![k],
        Some(_) => Vec::new(),
        None => visible,
    };
    let stopped: usize;
    if target_kbs.is_empty() {
        stopped = 0;
    } else {
        let ph = target_kbs.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        // 1) 标记所有进行中/待处理任务
        let sql = format!(
            "UPDATE processing_jobs SET stage='failed', progress=1.0,
                    error='已手动停止', updated_at=datetime('now')
             WHERE stage IN ('pending','parsing','chunking','embedding','generating')
               AND doc_id IN (SELECT id FROM documents WHERE kb_id IN ({}))",
            ph
        );
        let binds: Vec<&dyn rusqlite::types::ToSql> = target_kbs
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        stopped = conn
            .execute(&sql, binds.as_slice())
            .map_err(|e| e.to_string())?;
        // 2) 置位批量取消标记（批量提炼循环每处理一个文档前都会检查）
        for k in &target_kbs {
            conn.execute(
                "INSERT INTO kb_chunk_settings (key, value, updated_at) VALUES (?1,'1',datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET value='1', updated_at=datetime('now')",
                rusqlite::params![format!("generate_cancel_{}", k)],
            )
            .map_err(|e| e.to_string())?;
        }
        // 3) 进行中/待处理任务的文档复位为 ready（内容本身可用，可稍后重新处理）
        let doc_sql = format!(
            "UPDATE documents SET process_status='ready', updated_at=datetime('now')
             WHERE process_status IN ('parsing','chunking','embedding','generating') AND kb_id IN ({})",
            ph
        );
        let _ = conn.execute(&doc_sql, binds.as_slice());
    }
    drop(conn);
    Ok(serde_json::json!({ "stopped": stopped }))
}
