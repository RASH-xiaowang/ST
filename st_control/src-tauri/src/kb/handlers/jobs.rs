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

/// 按分类统计任务数量（从数据库直接统计，不依赖已加载的 jobs 数组）
pub fn count_jobs_by_category(
    db: &KbDatabase,
    uid: i64,
    kb_id: Option<i64>,
) -> std::collections::HashMap<String, i64> {
    let mut map = std::collections::HashMap::new();
    let visible = crate::kb::retrieval::visible_kb_ids(db, uid);
    if visible.is_empty() {
        return map;
    }
    let conn = db.conn_lock();
    let (kb_filter, binds): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(k) = kb_id
    {
        ("d.kb_id = ?1".to_string(), vec![Box::new(k)])
    } else {
        let ph = visible.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        (
            format!("d.kb_id IN ({})", ph),
            visible
                .iter()
                .map(|v| Box::new(*v) as Box<dyn rusqlite::types::ToSql>)
                .collect(),
        )
    };
    let stages = [
        "pending",
        "parsing",
        "chunking",
        "embedding",
        "generating",
        "done",
        "failed",
        "embed_error",
    ];
    for stage in stages {
        let sql = format!(
            "SELECT COUNT(*) FROM processing_jobs j JOIN documents d ON d.id = j.doc_id WHERE {} AND j.stage = ?",
            kb_filter
        );
        let mut all_binds: Vec<&dyn rusqlite::types::ToSql> =
            binds.iter().map(|b| b.as_ref()).collect();
        all_binds.push(&stage);
        let count: i64 = conn
            .query_row(&sql, all_binds.as_slice(), |r| r.get(0))
            .unwrap_or(0);
        map.insert(stage.to_string(), count);
    }
    map
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
    let counts = count_jobs_by_category(&db, uid, kb_id);
    Ok(serde_json::json!({
        "items": items,
        "total": total,
        "counts": {
            "pending": counts.get("pending").copied().unwrap_or(0),
            "processing": counts.get("parsing").copied().unwrap_or(0)
                + counts.get("chunking").copied().unwrap_or(0)
                + counts.get("embedding").copied().unwrap_or(0)
                + counts.get("generating").copied().unwrap_or(0),
            "done": counts.get("done").copied().unwrap_or(0),
            "failed": counts.get("failed").copied().unwrap_or(0)
                + counts.get("embed_error").copied().unwrap_or(0),
        }
    }))
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
    // 安全：jobs/logs 清理作用于多个知识库，仅限用户具备「编辑者」及以上权限的库；
    // history 为当前用户检索历史，任何已登录用户可清空自己的记录。
    let visible = crate::kb::retrieval::editable_kb_ids(&db, uid);
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
    // 安全：停止处理是编辑级操作；显式指定库时要求 editor+，未指定时仅作用于可编辑库
    let target_kbs: Vec<i64> = match kb_id {
        Some(k) => {
            crate::kb::retrieval::require_kb_role(&db, k, uid, "editor")?;
            vec![k]
        }
        None => crate::kb::retrieval::editable_kb_ids(&db, uid),
    };
    if target_kbs.is_empty() {
        return Ok(serde_json::json!({ "stopped": 0 }));
    }
    let conn = db.conn_lock();
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

/// 重试单个失败任务：将 failed/embed_error 状态的任务重新提交处理
#[tauri::command]
pub async fn kb_retry_job(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    job_id: i64,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let conn = db.conn_lock();
    // 校验任务存在且为可重试状态，并要求用户对该库具备 editor+ 权限
    let (doc_id, kb_id, stage): (i64, i64, String) = conn
        .query_row(
            "SELECT j.doc_id, d.kb_id, j.stage FROM processing_jobs j
             JOIN documents d ON d.id = j.doc_id WHERE j.id = ?1",
            rusqlite::params![job_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| "任务不存在".to_string())?;
    crate::kb::retrieval::require_kb_role(&db, kb_id, uid, "editor")?;
    if stage != "failed" && stage != "embed_error" {
        return Err(format!("当前状态「{}」不可重试，仅失败任务可重试", stage));
    }
    // 重置任务状态
    conn.execute(
        "UPDATE processing_jobs SET stage='pending', progress=0.0, error=NULL, updated_at=datetime('now') WHERE id = ?1",
        rusqlite::params![job_id],
    ).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE documents SET status='processing', process_status='pending', updated_at=datetime('now') WHERE id = ?1",
        rusqlite::params![doc_id],
    ).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO processing_logs (job_id, level, message) VALUES (?1,'info','任务已重新提交')",
        rusqlite::params![job_id],
    )
    .map_err(|e| e.to_string())?;
    drop(conn);

    // 后台异步重新处理
    let db_task = (*db).clone();
    let provider_model = crate::kb::handlers::resolve_embedding_pair(&db, None, None);
    tauri::async_runtime::spawn(async move {
        // 读取文档信息并重新走处理流水线
        let (file_type, version_id): (String, i64) = {
            let c = db_task.conn_lock();
            let ft: String = c
                .query_row(
                    "SELECT COALESCE(file_type,'txt') FROM documents WHERE id = ?1",
                    rusqlite::params![doc_id],
                    |r| r.get(0),
                )
                .unwrap_or_default();
            let vid: i64 = c
                .query_row(
                    "SELECT COALESCE(current_version_id, 0) FROM documents WHERE id = ?1",
                    rusqlite::params![doc_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            (ft, vid)
        };
        // 读取文件数据
        let data: Vec<u8> = {
            let c = db_task.conn_lock();
            c.query_row(
                "SELECT fo.blob_data FROM document_versions dv
                 JOIN file_objects fo ON fo.id = dv.file_object_id
                 WHERE dv.doc_id = ?1 ORDER BY dv.version_no DESC LIMIT 1",
                rusqlite::params![doc_id],
                |r| r.get(0),
            )
            .unwrap_or_default()
        };
        if data.is_empty() {
            let c = db_task.conn_lock();
            let _ = c.execute(
                "UPDATE processing_jobs SET stage='failed', error='文件数据为空，无法重试' WHERE id = ?1",
                rusqlite::params![job_id],
            );
            return;
        }
        // 调用文档处理流水线
        crate::kb::handlers::docs::process_document_for_retry(
            db_task,
            doc_id,
            version_id,
            job_id,
            file_type,
            data,
            provider_model.0,
            provider_model.1,
        )
        .await;
    });

    Ok(serde_json::json!({ "retried": true, "jobId": job_id }))
}

/// 批量重试所有失败任务
#[tauri::command]
pub async fn kb_retry_failed_jobs(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: Option<i64>,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    // 安全：批量重试是编辑级操作；显式指定库时要求 editor+，未指定时仅作用于可编辑库
    let target_kbs: Vec<i64> = match kb_id {
        Some(k) => {
            crate::kb::retrieval::require_kb_role(&db, k, uid, "editor")?;
            vec![k]
        }
        None => crate::kb::retrieval::editable_kb_ids(&db, uid),
    };
    if target_kbs.is_empty() {
        return Ok(serde_json::json!({ "retried": 0 }));
    }
    let conn = db.conn_lock();
    if target_kbs.is_empty() {
        return Ok(serde_json::json!({ "retried": 0 }));
    }
    let ph = target_kbs.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    // 找出所有失败任务
    let failed_jobs: Vec<(i64, i64)> = {
        let sql = format!(
            "SELECT j.id, j.doc_id FROM processing_jobs j
             JOIN documents d ON d.id = j.doc_id
             WHERE j.stage IN ('failed','embed_error') AND d.kb_id IN ({})",
            ph
        );
        let binds: Vec<&dyn rusqlite::types::ToSql> = target_kbs
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(binds.as_slice(), |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let count = failed_jobs.len();
    if count == 0 {
        return Ok(serde_json::json!({ "retried": 0 }));
    }
    // 批量重置状态
    for (jid, did) in &failed_jobs {
        let _ = conn.execute(
            "UPDATE processing_jobs SET stage='pending', progress=0.0, error=NULL, updated_at=datetime('now') WHERE id = ?1",
            rusqlite::params![jid],
        );
        let _ = conn.execute(
            "UPDATE documents SET status='processing', process_status='pending', updated_at=datetime('now') WHERE id = ?1",
            rusqlite::params![did],
        );
        let _ = conn.execute(
            "INSERT INTO processing_logs (job_id, level, message) VALUES (?1,'info','批量重试：任务已重新提交')",
            rusqlite::params![jid],
        );
    }
    drop(conn);

    // 后台逐个重新处理（复用单个重试逻辑）
    let db_task = (*db).clone();
    let provider_model = crate::kb::handlers::resolve_embedding_pair(&db, None, None);
    tauri::async_runtime::spawn(async move {
        for (jid, did) in failed_jobs {
            let (file_type, version_id): (String, i64) = {
                let c = db_task.conn_lock();
                let ft: String = c
                    .query_row(
                        "SELECT COALESCE(file_type,'txt') FROM documents WHERE id = ?1",
                        rusqlite::params![did],
                        |r| r.get(0),
                    )
                    .unwrap_or_default();
                let vid: i64 = c
                    .query_row(
                        "SELECT COALESCE(current_version_id, 0) FROM documents WHERE id = ?1",
                        rusqlite::params![did],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                (ft, vid)
            };
            let data: Vec<u8> = {
                let c = db_task.conn_lock();
                c.query_row(
                    "SELECT fo.blob_data FROM document_versions dv
                     JOIN file_objects fo ON fo.id = dv.file_object_id
                     WHERE dv.doc_id = ?1 ORDER BY dv.version_no DESC LIMIT 1",
                    rusqlite::params![did],
                    |r| r.get(0),
                )
                .unwrap_or_default()
            };
            if data.is_empty() {
                let c = db_task.conn_lock();
                let _ = c.execute(
                    "UPDATE processing_jobs SET stage='failed', error='文件数据为空' WHERE id = ?1",
                    rusqlite::params![jid],
                );
                continue;
            }
            crate::kb::handlers::docs::process_document_for_retry(
                db_task.clone(),
                did,
                version_id,
                jid,
                file_type,
                data,
                provider_model.0.clone(),
                provider_model.1.clone(),
            )
            .await;
        }
    });

    Ok(serde_json::json!({ "retried": count }))
}
