// ============================================================
// 知识库管理 — Tauri IPC 命令门面
// 按域拆分的子模块在此汇总 re-export（lib.rs 注册点保持不变）；
// 本文件仅保留跨域共享设施：孤儿文件清理、存储配额、指标埋点。
// ============================================================

use crate::kb::db::KbDatabase;

mod analytics_settings;
pub(crate) use analytics_settings::*;
mod analytics;
pub(crate) use analytics::*;
mod jobs;
pub(crate) use jobs::*;
mod qa;
pub(crate) use qa::*;
mod docs;
pub(crate) use docs::*;
mod versions;
pub(crate) use versions::*;
mod search;
pub(crate) use search::*;
mod chunks;
pub(crate) use chunks::*;
mod access;
pub(crate) use access::*;
mod settings;
pub(crate) use settings::*;
mod wiki;
pub(crate) use wiki::*;

/// 清理不再被任何 document_versions 引用的 file_objects（孤儿原始文件 BLOB）。
/// 必须先收集候选 id（在删除引用前），再在删除后调用。
pub fn cleanup_orphan_file_objects(
    conn: &rusqlite::Connection,
    candidate_ids: &[i64],
) -> Result<(), String> {
    if candidate_ids.is_empty() {
        return Ok(());
    }
    let placeholders = (0..candidate_ids.len())
        .map(|i| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "DELETE FROM file_objects WHERE id IN ({}) AND NOT EXISTS (
            SELECT 1 FROM document_versions dv WHERE dv.file_object_id = file_objects.id
        )",
        placeholders
    );
    let binds: Vec<&dyn rusqlite::types::ToSql> = candidate_ids
        .iter()
        .map(|v| v as &dyn rusqlite::types::ToSql)
        .collect();
    conn.execute(&sql, binds.as_slice())
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ════════════════════════════════════════════════════════════
// 全局统计（概览页）
// ════════════════════════════════════════════════════════════

/// 存储配额：2GB（本地单机部署）
pub(crate) const KB_STORAGE_QUOTA: i64 = 2 * 1024 * 1024 * 1024;

// ════════════════════════════════════════════════════════════
// 指标事件（埋点）与首页统计
// ════════════════════════════════════════════════════════════

/// 写入指标事件（fire-and-forget：记录失败不影响主流程）
/// 指标事件（埋点维度）
pub(crate) struct MetricEvent<'a> {
    pub uid: i64,
    pub event_type: &'a str,
    pub kb_id: Option<i64>,
    pub doc_id: Option<i64>,
    pub page_id: Option<i64>,
    pub session_id: Option<i64>,
    pub detail: Option<&'a str>,
}

pub(crate) fn log_metric_event(db: &KbDatabase, ev: &MetricEvent<'_>) {
    let uid = ev.uid;
    let event_type = ev.event_type;
    let kb_id = ev.kb_id;
    let doc_id = ev.doc_id;
    let page_id = ev.page_id;
    let session_id = ev.session_id;
    let detail = ev.detail;
    // 非阻塞加锁：若调用方当前已持有连接锁（如 kb_get_document 在返回前埋点），
    // 直接跳过记录，避免同线程重复加锁死锁导致前端命令永久挂起。
    let Some(conn) = db.try_conn_lock() else {
        return;
    };
    let _ = conn.execute(
        "INSERT INTO kb_metric_events (event_type, kb_id, doc_id, page_id, user_id, session_id, detail)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        rusqlite::params![event_type, kb_id, doc_id, page_id, uid, session_id, detail],
    );
}

/// 指标配置默认显示名
pub(crate) const ANALYTICS_METRIC_DEFAULTS: [(&str, &str); 8] = [
    ("messages", "消息量"),
    ("sessions", "会话量"),
    ("recall", "整体召回率"),
    ("handoff", "转人工率"),
    ("faq", "常用问答"),
    ("llm", "LLM问答"),
    ("task", "任务技能"),
    ("recommend", "问题推荐"),
];

pub(crate) fn analytics_settings_map(
    conn: &rusqlite::Connection,
) -> std::collections::HashMap<String, (String, bool)> {
    let mut map = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT key, label, visible FROM kb_analytics_settings") {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        }) {
            for r in rows.flatten() {
                map.insert(r.0, (r.1, r.2 != 0));
            }
        }
    }
    map
}
