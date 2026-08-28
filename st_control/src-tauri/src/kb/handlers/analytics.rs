// ============================================================
// 知识库管理 — 指标事件（埋点）与首页推荐
// 自 handlers.rs 拆分：显式埋点 + 热门问题推荐。
// ============================================================

use crate::kb::db::KbDatabase;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{analytics_settings_map, log_metric_event, MetricEvent};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackEventInput {
    pub event_type: String,
    pub kb_id: Option<i64>,
    pub doc_id: Option<i64>,
    pub page_id: Option<i64>,
    pub session_id: Option<i64>,
    pub detail: Option<String>,
}

/// 前端显式埋点（引用点击 / 转人工 / 推荐点击 / 图谱点击等纯 UI 动作）
#[tauri::command]
pub async fn kb_track_event(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    input: TrackEventInput,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    log_metric_event(
        &db,
        &MetricEvent {
            uid,
            event_type: &input.event_type,
            kb_id: input.kb_id,
            doc_id: input.doc_id,
            page_id: input.page_id,
            session_id: input.session_id,
            detail: input.detail.as_deref(),
        },
    );
    Ok(())
}

/// 热门问题推荐：FAQ 命中热点 + 高频检索词 + 兜底 FAQ，合并去重后返回。
/// 独立函数便于集成测试；命令为薄封装。
pub fn recommend_questions(
    db: &KbDatabase,
    uid: i64,
    kb_id: Option<i64>,
    limit: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let lim = limit.clamp(1, 20);
    let visible = match kb_id {
        Some(id) => {
            if !crate::kb::retrieval::can_access_kb(db, id, uid) {
                return Err("无权限：你无权访问该知识库".to_string());
            }
            vec![id]
        }
        None => crate::kb::retrieval::visible_kb_ids(db, uid),
    };
    if visible.is_empty() {
        return Ok(Vec::new());
    }
    let conn = db.conn_lock();
    let placeholders = visible.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let binds: Vec<&dyn rusqlite::types::ToSql> = visible
        .iter()
        .map(|v| v as &dyn rusqlite::types::ToSql)
        .collect();
    let query_strings = |sql: &str| -> Vec<String> {
        conn.prepare(sql)
            .ok()
            .and_then(|mut st| {
                st.query_map(binds.as_slice(), |r| r.get::<_, String>(0))
                    .ok()
                    .map(|rs| rs.filter_map(|r| r.ok()).collect::<Vec<_>>())
            })
            .unwrap_or_default()
    };
    // 1) FAQ 命中热点（detail 存问题原文；已删除的 FAQ 不推荐）
    let faq_hits: Vec<String> = query_strings(&format!(
        "SELECT e.detail FROM kb_metric_events e
         WHERE e.event_type='faq_hit' AND e.kb_id IN ({}) AND e.detail IS NOT NULL AND e.detail != ''
           AND EXISTS (SELECT 1 FROM faq_entries f WHERE f.kb_id = e.kb_id AND f.question = e.detail)
         GROUP BY e.detail ORDER BY COUNT(*) DESC",
        placeholders
    ));
    // 2) 高频检索词
    let queries: Vec<String> = query_strings(&format!(
        "SELECT query FROM search_logs WHERE kb_id IN ({}) GROUP BY query ORDER BY COUNT(*) DESC",
        placeholders
    ));
    // 3) 兜底 FAQ（未被命中过也推荐，按更新时间倒序）
    let fallback_faq: Vec<String> = query_strings(&format!(
        "SELECT question FROM faq_entries WHERE kb_id IN ({}) ORDER BY updated_at DESC",
        placeholders
    ));
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<serde_json::Value> = Vec::new();
    for q in faq_hits
        .into_iter()
        .chain(queries.into_iter().chain(fallback_faq))
    {
        let q = q.trim().to_string();
        if q.is_empty() || !seen.insert(q.clone()) {
            continue;
        }
        out.push(serde_json::json!({ "type": "faq", "question": q }));
        if out.len() >= lim {
            break;
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn kb_recommend_questions(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: Option<i64>,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    recommend_questions(&db, uid, kb_id, limit.unwrap_or(8))
}

/// 处理卡死兜底：扫描超过 10 分钟无进展的任务并标记失败，
/// 同时把没有活跃任务但仍 processing 的文档恢复为 failed。
/// 打开概览/活动页会自动触发，也可手动调用。
#[tauri::command]
pub async fn kb_housekeeping(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
) -> Result<serde_json::Value, String> {
    session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let (jobs, docs) = {
        let db_block = (*db).clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<(usize, usize), String> {
            let conn = db_block.conn_lock();
            // 1) 超时任务标记失败。普通阶段（解析/分片/向量化）10 分钟无进展即判死；
            //    Wiki 提炼（generating）单文档要串行执行 1 次提炼 + N 次页面摘要/实体提取，
            //    每次 LLM 调用上限 90s，最坏约 20 分钟，故放宽到 30 分钟——超过即视为被中断
            //    （如应用重启杀掉后台任务），标记失败让用户可重新提炼，避免永久卡在“处理中”。
            let jobs = conn
                .execute(
                    "UPDATE processing_jobs SET stage='failed', progress=1.0,
                            error='处理超时（超过 10 分钟无进展），已自动标记失败', updated_at=datetime('now')
                     WHERE stage NOT IN ('done','failed','generating') AND updated_at < datetime('now','-10 minutes')",
                    [],
                )
                .map_err(|e| e.to_string())?;
            let generating = conn
                .execute(
                    "UPDATE processing_jobs SET stage='failed', progress=1.0,
                            error='Wiki 提炼超时（超过 30 分钟无进展，可能因应用重启中断），已自动标记失败，可重新提炼',
                            updated_at=datetime('now')
                     WHERE stage='generating' AND updated_at < datetime('now','-30 minutes')",
                    [],
                )
                .map_err(|e| e.to_string())?;
            let jobs = jobs + generating;
            // 2) 无活跃任务但仍 processing 的文档 → failed
            let docs = conn
                .execute(
                    "UPDATE documents SET status='failed', process_status='failed', updated_at=datetime('now')
                     WHERE status='processing' AND NOT EXISTS (
                         SELECT 1 FROM processing_jobs j
                         WHERE j.doc_id = documents.id AND j.stage NOT IN ('done','failed')
                     )",
                    [],
                )
                .map_err(|e| e.to_string())?;
            Ok((jobs, docs))
        })
        .await
        .map_err(|e| format!("知识库维护任务失败: {}", e))??
    };
    // 3) FTS 索引一致性检查与修复
    let fts_report = {
        let db_block = (*db).clone();
        tauri::async_runtime::spawn_blocking(move || db_block.repair_fts_consistency())
            .await
            .map_err(|e| format!("FTS 一致性检查失败: {}", e))??
    };
    if !fts_report.ok {
        log::warn!(
            "FTS 索引修复后仍不一致：missing_chunks={} orphan_chunks={} missing_wiki={} orphan_wiki={}",
            fts_report.missing_chunks.len(),
            fts_report.orphan_chunks.len(),
            fts_report.missing_wiki.len(),
            fts_report.orphan_wiki.len(),
        );
    }
    Ok(serde_json::json!({
        "jobs": jobs,
        "docs": docs,
        "fts": {
            "ok": fts_report.ok,
            "fixed": fts_report.fixed,
            "missingChunks": fts_report.missing_chunks.len(),
            "orphanChunks": fts_report.orphan_chunks.len(),
            "missingWiki": fts_report.missing_wiki.len(),
            "orphanWiki": fts_report.orphan_wiki.len(),
        }
    }))
}
fn detail_hit_count(detail: &str) -> i64 {
    serde_json::from_str::<serde_json::Value>(detail)
        .ok()
        .and_then(|v| v.get("hitCount").and_then(|h| h.as_i64()))
        .unwrap_or(0)
}

/// 统计某数据源：近 7 天逐日计数 + 今日/昨日/7 日前总量。
/// `from_clause` 为完整子查询，形如 "SELECT * FROM qa_messages"
/// 或 "SELECT * FROM kb_metric_events WHERE event_type='faq_hit'"
fn metric_counts(
    conn: &rusqlite::Connection,
    from_clause: &str,
) -> (Vec<(String, i64)>, i64, i64, i64) {
    let series: Vec<(String, i64)> = {
        let sql = format!(
            "SELECT date(created_at,'localtime') d, COUNT(*) FROM ({})
             WHERE date(created_at,'localtime') >= date('now','localtime','-6 days') GROUP BY d",
            from_clause
        );
        conn.prepare(&sql)
            .ok()
            .and_then(|mut st| {
                st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                    .ok()
                    .map(|rs| rs.filter_map(|r| r.ok()).collect::<Vec<_>>())
            })
            .unwrap_or_default()
    };
    let day_total = |offset: i64| -> i64 {
        let sql = format!(
            "SELECT COUNT(*) FROM ({}) WHERE date(created_at,'localtime') = date('now','localtime', ?1)",
            from_clause
        );
        conn.query_row(&sql, rusqlite::params![format!("-{} days", offset)], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap_or(0)
    };
    (series, day_total(0), day_total(1), day_total(7))
}

/// 近 7 天补零序列（含今日，本地日期）
fn build_series_7d(conn: &rusqlite::Connection, counts: Vec<(String, i64)>) -> Vec<(String, i64)> {
    let mut m = std::collections::HashMap::new();
    for (d, c) in counts {
        m.insert(d, c);
    }
    let mut out = Vec::new();
    for i in (0..7).rev() {
        let day: String = conn
            .query_row(
                "SELECT date('now','localtime', ?1)",
                rusqlite::params![format!("-{} days", i)],
                |r| r.get(0),
            )
            .unwrap_or_default();
        out.push((day.clone(), m.get(&day).copied().unwrap_or(0)));
    }
    out
}

/// 近 7 天比率序列（按日调用 ratio_at）
fn ratio_series_7d(
    conn: &rusqlite::Connection,
    ratio_at: &dyn Fn(&str) -> i64,
) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for i in (0..7).rev() {
        let day: String = conn
            .query_row(
                "SELECT date('now','localtime', ?1)",
                rusqlite::params![format!("-{} days", i)],
                |r| r.get(0),
            )
            .unwrap_or_default();
        out.push((day.clone(), ratio_at(&day)));
    }
    out
}

/// 首页数据指标（7 项全真实口径）：
/// 消息量/会话量 = QA 记录；FAQ = faq_hit 事件；LLM 问答 = rag 事件；
/// 召回率 = 有结果检索事件占比；
/// 任务技能 = processing_jobs done；问题推荐 = recommend_click。
pub fn analytics_for(db: &KbDatabase, _uid: i64) -> Result<serde_json::Value, String> {
    let conn = db.conn_lock();

    let (msg_series, msg_today, msg_yday, msg_7d) =
        metric_counts(&conn, "SELECT * FROM qa_messages");
    let (ses_series, ses_today, ses_yday, ses_7d) =
        metric_counts(&conn, "SELECT * FROM qa_sessions");
    let (faq_series, faq_today, faq_yday, faq_7d) = metric_counts(
        &conn,
        "SELECT * FROM kb_metric_events WHERE event_type='faq_hit'",
    );
    let (rag_series, rag_today, rag_yday, rag_7d) = metric_counts(
        &conn,
        "SELECT * FROM kb_metric_events WHERE event_type='rag'",
    );
    let (rec_series, rec_today, rec_yday, rec_7d) = metric_counts(
        &conn,
        "SELECT * FROM kb_metric_events WHERE event_type='recommend_click'",
    );

    // 检索事件按日统计（hits>0, total），detail 存 hitCount
    let search_stats: std::collections::HashMap<String, (i64, i64)> = {
        let mut map = std::collections::HashMap::new();
        let sql = "SELECT date(created_at,'localtime') d, COALESCE(detail,'') FROM kb_metric_events
                   WHERE event_type='search' AND date(created_at,'localtime') >= date('now','localtime','-7 days')";
        if let Ok(mut stmt) = conn.prepare(sql) {
            if let Ok(rows) =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            {
                for r in rows.flatten() {
                    let (d, detail) = r;
                    let e = map.entry(d).or_insert((0, 0));
                    e.1 += 1;
                    if detail_hit_count(&detail) > 0 {
                        e.0 += 1;
                    }
                }
            }
        }
        map
    };
    let recall_at = |date: &str| -> i64 {
        search_stats
            .get(date)
            .filter(|(_, total)| *total > 0)
            .map(|(hit, total)| (hit * 100) / total)
            .unwrap_or(0)
    };
    let today: String = conn
        .query_row("SELECT date('now','localtime')", [], |r| r.get(0))
        .unwrap_or_default();
    let yday: String = conn
        .query_row("SELECT date('now','localtime', '-1 days')", [], |r| {
            r.get(0)
        })
        .unwrap_or_default();
    let w7d: String = conn
        .query_row("SELECT date('now','localtime', '-7 days')", [], |r| {
            r.get(0)
        })
        .unwrap_or_default();

    let recall_series = ratio_series_7d(&conn, &recall_at);

    let build_metric = |key: &str,
                        today_v: i64,
                        yday_v: i64,
                        w7_v: i64,
                        series: Vec<(String, i64)>,
                        is_rate: bool|
     -> serde_json::Value {
        let pct = |cur: i64, prev: i64| -> String {
            if prev <= 0 {
                return "--".to_string();
            }
            let v = ((cur - prev) as f64 / prev as f64 * 100.0).round() as i64;
            format!("{}{}%", if v > 0 { "+" } else { "" }, v)
        };
        let value = if is_rate {
            format!("{}%", today_v)
        } else {
            today_v.to_string()
        };
        serde_json::json!({
            "key": key,
            "value": value,
            "today": today_v,
            "daily": pct(today_v, yday_v),
            "yearly": pct(today_v, w7_v),
            "series": series.into_iter().map(|(date, v)| serde_json::json!({ "date": date, "value": v })).collect::<Vec<_>>(),
        })
    };

    let mut metrics = vec![
        build_metric(
            "messages",
            msg_today,
            msg_yday,
            msg_7d,
            build_series_7d(&conn, msg_series),
            false,
        ),
        build_metric(
            "sessions",
            ses_today,
            ses_yday,
            ses_7d,
            build_series_7d(&conn, ses_series),
            false,
        ),
        build_metric(
            "recall",
            recall_at(&today),
            recall_at(&yday),
            recall_at(&w7d),
            recall_series,
            true,
        ),
        build_metric(
            "faq",
            faq_today,
            faq_yday,
            faq_7d,
            build_series_7d(&conn, faq_series),
            false,
        ),
        build_metric(
            "llm",
            rag_today,
            rag_yday,
            rag_7d,
            build_series_7d(&conn, rag_series),
            false,
        ),
        build_metric(
            "recommend",
            rec_today,
            rec_yday,
            rec_7d,
            build_series_7d(&conn, rec_series),
            false,
        ),
    ];
    // 应用指标配置（显示名 / 可见性）
    let settings = analytics_settings_map(&conn);
    for m in &mut metrics {
        let key = m.get("key").and_then(|k| k.as_str()).unwrap_or("");
        if let Some((label, visible)) = settings.get(key) {
            m["label"] = serde_json::json!(label);
            m["visible"] = serde_json::json!(visible);
        }
    }
    Ok(serde_json::json!({ "metrics": metrics }))
}

/// 首页数据指标（完整埋点口径）
#[tauri::command]
pub async fn kb_get_analytics(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    analytics_for(&db, uid)
}
#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct KbStats {
    pub kb_count: i64,
    pub doc_count: i64,
    pub chunk_count: i64,
    pub wiki_page_count: i64,
    /// 已用存储字节数（原始文件去重后）
    pub storage_bytes: i64,
    /// 存储配额字节数
    pub storage_quota: i64,
    pub doc_ready: i64,
    pub doc_processing: i64,
    pub doc_failed: i64,
    pub job_pending: i64,
    pub job_done: i64,
    pub job_failed: i64,
}
impl Default for KbStats {
    fn default() -> Self {
        Self {
            kb_count: 0,
            doc_count: 0,
            chunk_count: 0,
            wiki_page_count: 0,
            storage_bytes: 0,
            storage_quota: super::KB_STORAGE_QUOTA,
            doc_ready: 0,
            doc_processing: 0,
            doc_failed: 0,
            job_pending: 0,
            job_done: 0,
            job_failed: 0,
        }
    }
}

/// 统计可见知识库范围内的整体数据（供概览页/测试复用）
pub fn stats_for(db: &KbDatabase, visible: &[i64]) -> Result<KbStats, String> {
    let mut s = KbStats::default();
    if visible.is_empty() {
        return Ok(s);
    }
    s.kb_count = visible.len() as i64;
    let placeholders = (0..visible.len())
        .map(|i| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let count_in_visible = |sql: &str| -> i64 {
        let conn = db.conn_lock();
        let sql = sql.replace("{}", &placeholders);
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let binds: Vec<&dyn rusqlite::types::ToSql> = visible
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        stmt.query_row(binds.as_slice(), |r| r.get::<_, i64>(0))
            .unwrap_or(0)
    };
    s.doc_count = count_in_visible("SELECT COUNT(*) FROM documents WHERE kb_id IN ({})");
    s.chunk_count = count_in_visible("SELECT COUNT(*) FROM document_chunks WHERE kb_id IN ({})");
    s.wiki_page_count = count_in_visible("SELECT COUNT(*) FROM wiki_pages WHERE kb_id IN ({})");
    s.storage_bytes = count_in_visible(
        "SELECT COALESCE(SUM(f.size),0) FROM (SELECT DISTINCT fo.id, fo.size FROM file_objects fo
         JOIN document_versions dv ON dv.file_object_id = fo.id
         JOIN documents d ON d.id = dv.doc_id
         WHERE d.kb_id IN ({})) f",
    );
    s.doc_ready =
        count_in_visible("SELECT COUNT(*) FROM documents WHERE kb_id IN ({}) AND status='ready'");
    s.doc_processing = count_in_visible(
        "SELECT COUNT(*) FROM documents WHERE kb_id IN ({}) AND status='processing'",
    );
    s.doc_failed =
        count_in_visible("SELECT COUNT(*) FROM documents WHERE kb_id IN ({}) AND status='failed'");
    s.job_pending = count_in_visible(
        "SELECT COUNT(*) FROM processing_jobs j JOIN documents d ON d.id = j.doc_id WHERE d.kb_id IN ({}) AND j.stage NOT IN ('done','failed')",
    );
    s.job_done = count_in_visible(
        "SELECT COUNT(*) FROM processing_jobs j JOIN documents d ON d.id = j.doc_id WHERE d.kb_id IN ({}) AND j.stage='done'",
    );
    s.job_failed = count_in_visible(
        "SELECT COUNT(*) FROM processing_jobs j JOIN documents d ON d.id = j.doc_id WHERE d.kb_id IN ({}) AND j.stage='failed'",
    );
    Ok(s)
}

/// 全局统计（概览页数据源；未登录时以默认用户身份统计可见范围）
#[tauri::command]
pub async fn kb_get_stats(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
) -> Result<KbStats, String> {
    let uid = session.get().map(|u| u.id).unwrap_or(1);
    let visible = crate::kb::retrieval::visible_kb_ids(&db, uid);
    stats_for(&db, &visible)
}
