// ============================================================
// 每日总结模块 — 任务/记录 CRUD
// 自 daily_summary.rs 拆分：总结任务与结果记录的持久化操作。
// ============================================================

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

use crate::wechat::modules::common;

use super::connect;

// ─── 任务 CRUD ───

#[derive(Debug, Clone, Serialize)]
pub struct SummaryTask {
    pub id: i64,
    pub group_username: String,
    pub group_name: String,
    pub target_users: Vec<String>,
    pub provider_id: String,
    pub model: String,
    pub format: String,
    pub custom_prompt: String,
    pub schedule_time: String,
    pub enabled: bool,
    pub last_run_at: Option<i64>,
    pub last_status: String,
    pub last_error: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SummaryRecord {
    pub id: i64,
    pub task_id: i64,
    pub group_username: String,
    pub group_name: String,
    pub target_users: Vec<String>,
    pub summary_date: String,
    pub provider_id: String,
    pub model: String,
    pub format: String,
    pub summary: String,
    pub char_count: i64,
    pub message_count: i64,
    pub status: String,
    pub error: String,
    pub created_at: i64,
    /// 模型调用耗时（毫秒）
    pub duration_ms: i64,
    /// 输入 / 输出 / 合计 tokens
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    /// 输入给模型的聊天片段样例（前若干条，便于核对）
    pub message_sample: String,
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<SummaryTask> {
    let target: String = row.get("target_users")?;
    Ok(SummaryTask {
        id: row.get("id")?,
        group_username: row.get("group_username")?,
        group_name: row.get("group_name")?,
        target_users: serde_json::from_str(&target).unwrap_or_default(),
        provider_id: row.get("provider_id")?,
        model: row.get("model")?,
        format: row.get("format")?,
        custom_prompt: row.get("custom_prompt")?,
        schedule_time: row.get("schedule_time")?,
        enabled: row.get::<_, i64>("enabled")? != 0,
        last_run_at: row.get("last_run_at")?,
        last_status: row.get("last_status")?,
        last_error: row.get("last_error")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<SummaryRecord> {
    let target: String = row.get("target_users")?;
    Ok(SummaryRecord {
        id: row.get("id")?,
        task_id: row.get("task_id")?,
        group_username: row.get("group_username")?,
        group_name: row.get("group_name")?,
        target_users: serde_json::from_str(&target).unwrap_or_default(),
        summary_date: row.get("summary_date")?,
        provider_id: row.get("provider_id")?,
        model: row.get("model")?,
        format: row.get("format")?,
        summary: row.get("summary")?,
        char_count: row.get("char_count")?,
        message_count: row.get("message_count")?,
        status: row.get("status")?,
        error: row.get("error")?,
        created_at: row.get("created_at")?,
        duration_ms: row.get("duration_ms")?,
        prompt_tokens: row.get("prompt_tokens")?,
        completion_tokens: row.get("completion_tokens")?,
        total_tokens: row.get("total_tokens")?,
        message_sample: row.get("message_sample")?,
    })
}

pub fn list_tasks() -> Result<Vec<SummaryTask>, String> {
    let conn = connect()?;
    repair_group_names(&conn);
    let mut stmt = conn
        .prepare("SELECT * FROM summary_tasks ORDER BY enabled DESC, id DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], row_to_task).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows.flatten() {
        out.push(r);
    }
    Ok(out)
}

pub fn get_task(id: i64) -> Result<Option<SummaryTask>, String> {
    let conn = connect()?;
    conn.query_row(
        "SELECT * FROM summary_tasks WHERE id=?1",
        rusqlite::params![id],
        row_to_task,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn save_task(task: serde_json::Value) -> Result<SummaryTask, String> {
    let conn = connect()?;
    let group_username = task
        .get("group_username")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if group_username.is_empty() {
        return Err("请选择群聊".to_string());
    }
    let group_name = task
        .get("group_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    // 清洗群名：空值或 JSON 数组垃圾值（早期版本把关注成员写进群名）→ 服务端解析真实群名
    let group_name = if group_name.trim().is_empty() || group_name.trim_start().starts_with('[') {
        let cfg = crate::wechat::config::WeChatConfig::load().ok();
        cfg.as_ref()
            .and_then(|c| resolve_group_name(&c.decrypted_dir, &group_username))
            .unwrap_or_else(|| group_name.clone())
    } else {
        group_name
    };
    let target_users = match task.get("target_users") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let target_json = serde_json::to_string(&target_users).unwrap_or_else(|_| "[]".to_string());
    let provider_id = task
        .get("provider_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let model = task
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let format = task
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("brief")
        .to_string();
    let custom_prompt = task
        .get("custom_prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let schedule_time = task
        .get("schedule_time")
        .and_then(|v| v.as_str())
        .filter(|s| s.len() == 5 && s.contains(':'))
        .unwrap_or("08:00")
        .to_string();
    let enabled = task
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let ts = common::now_ms();

    match task.get("id").and_then(|v| v.as_i64()) {
        Some(id) if id > 0 => {
            conn.execute(
                "UPDATE summary_tasks SET group_username=?1, group_name=?2, target_users=?3,
                 provider_id=?4, model=?5, format=?6, custom_prompt=?7, schedule_time=?8,
                 enabled=?9, updated_at=?10 WHERE id=?11",
                rusqlite::params![
                    group_username,
                    group_name,
                    target_json,
                    provider_id,
                    model,
                    format,
                    custom_prompt,
                    schedule_time,
                    enabled as i64,
                    ts,
                    id
                ],
            )
            .map_err(|e| e.to_string())?;
            get_task(id)?.ok_or_else(|| "任务不存在".to_string())
        }
        _ => {
            conn.execute(
                "INSERT INTO summary_tasks(group_username, group_name, target_users, provider_id,
                 model, format, custom_prompt, schedule_time, enabled, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
                rusqlite::params![
                    group_username,
                    group_name,
                    target_json,
                    provider_id,
                    model,
                    format,
                    custom_prompt,
                    schedule_time,
                    enabled as i64,
                    ts
                ],
            )
            .map_err(|e| e.to_string())?;
            let id = conn.last_insert_rowid();
            get_task(id)?.ok_or_else(|| "任务创建失败".to_string())
        }
    }
}

pub fn delete_task(id: i64) -> Result<(), String> {
    let conn = connect()?;
    conn.execute(
        "DELETE FROM summary_tasks WHERE id=?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    // 同时清理该任务的历史记录
    conn.execute(
        "DELETE FROM summary_records WHERE task_id=?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn toggle_task(id: i64, enabled: bool) -> Result<(), String> {
    let conn = connect()?;
    conn.execute(
        "UPDATE summary_tasks SET enabled=?1, updated_at=?2 WHERE id=?3",
        rusqlite::params![enabled as i64, common::now_ms(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn update_task_run_state(id: i64, ok: bool, error: &str) -> Result<(), String> {
    let conn = connect()?;
    conn.execute(
        "UPDATE summary_tasks SET last_run_at=?1, last_status=?2, last_error=?3, updated_at=?1 WHERE id=?4",
        rusqlite::params![
            common::now_ms(),
            if ok { "success" } else { "error" }.to_string(),
            error.to_string(),
            id
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ─── 记录 CRUD ───

pub fn list_records(task_id: Option<i64>) -> Result<Vec<SummaryRecord>, String> {
    let conn = connect()?;
    repair_group_names(&conn);
    let (sql, params) = match task_id {
        Some(id) => (
            "SELECT * FROM summary_records WHERE task_id=?1 ORDER BY summary_date DESC, id DESC LIMIT 200".to_string(),
            vec![rusqlite::types::Value::Integer(id)],
        ),
        None => (
            "SELECT * FROM summary_records ORDER BY id DESC LIMIT 200".to_string(),
            Vec::new(),
        ),
    };
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params), row_to_record)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows.flatten() {
        out.push(r);
    }
    Ok(out)
}

/// 自愈修复：把存成 JSON 数组（或空串）的群名替换为服务端解析的真实群名。
/// 早期版本曾把关注成员列表写进 group_name，导致界面显示 `["wxid_xxx"]`。
fn repair_group_names(conn: &Connection) {
    let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
        return;
    };
    let names = crate::wechat::annual::load_display_names(&cfg.decrypted_dir, &[]);
    for table in ["summary_tasks", "summary_records"] {
        let rows: Vec<(i64, String, String)> = {
            let Ok(mut stmt) = conn.prepare(&format!(
                "SELECT id, group_username, group_name FROM {}",
                table
            )) else {
                continue;
            };
            let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0).unwrap_or(0),
                    r.get::<_, String>(1).unwrap_or_default(),
                    r.get::<_, String>(2).unwrap_or_default(),
                ))
            }) else {
                continue;
            };
            rows.flatten().collect()
        };
        for (id, uname, gname) in rows {
            let dirty = gname.trim().is_empty() || gname.trim_start().starts_with('[');
            if dirty {
                if let Some(real) = names.get(&uname).filter(|n| !n.is_empty() && **n != uname) {
                    let _ = conn.execute(
                        &format!("UPDATE {} SET group_name=?1 WHERE id=?2", table),
                        rusqlite::params![real, id],
                    );
                }
            }
        }
    }
}

/// 服务端解析群聊显示名（联系人备注/昵称 > 会话标题 > username）
fn resolve_group_name(decrypted_dir: &Path, group_username: &str) -> Option<String> {
    let names = crate::wechat::annual::load_display_names(decrypted_dir, &[]);
    names
        .get(group_username)
        .filter(|n| !n.is_empty() && **n != group_username)
        .cloned()
}

pub fn delete_record(id: i64) -> Result<(), String> {
    let conn = connect()?;
    conn.execute(
        "DELETE FROM summary_records WHERE id=?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn insert_record(rec: &SummaryRecord) -> Result<(), String> {
    let conn = connect()?;
    conn.execute(
        "INSERT INTO summary_records(task_id, group_username, group_name, target_users,
         summary_date, provider_id, model, format, summary, char_count, message_count,
         status, error, created_at, duration_ms, prompt_tokens, completion_tokens,
         total_tokens, message_sample)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
        rusqlite::params![
            rec.task_id,
            rec.group_username,
            rec.group_name,
            serde_json::to_string(&rec.target_users).unwrap_or_else(|_| "[]".to_string()),
            rec.summary_date,
            rec.provider_id,
            rec.model,
            rec.format,
            rec.summary,
            rec.char_count,
            rec.message_count,
            rec.status,
            rec.error,
            rec.created_at,
            rec.duration_ms,
            rec.prompt_tokens,
            rec.completion_tokens,
            rec.total_tokens,
            rec.message_sample,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
