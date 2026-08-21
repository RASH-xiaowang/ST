// ============================================================
// 微信 general.db 记录查询 — 统计域
// 自 general_records.rs 拆分：转账/红包统计（AI 问答工具用）。
// ============================================================

use super::open_general;

/// Unix 秒 → "YYYYMMDD"（本地时区；非法值返回空串）
fn epoch_to_ymd(ts: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%Y%m%d").to_string())
        .unwrap_or_default()
}

/// 统计转账笔数（可按会话与时间过滤），供 AI 问答的统计工具使用。
/// 返回 `{ total, sessions: [{ name, count }] }`；sessions 为该时间窗内按笔数降序的 Top 会话。
pub fn stats_transfers(
    target: Option<&str>,
    time_from: Option<i64>,
    time_to: Option<i64>,
) -> Result<serde_json::Value, String> {
    let conn = open_general().ok_or_else(|| "未找到解密后的 general.db".to_string())?;
    let mut where_parts: Vec<String> = Vec::new();
    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(t) = target.filter(|t| !t.is_empty()) {
        where_parts.push("(session_name = ? OR pay_payer = ? OR pay_receiver = ?)".to_string());
        params.push(rusqlite::types::Value::Text(t.to_string()));
        params.push(rusqlite::types::Value::Text(t.to_string()));
        params.push(rusqlite::types::Value::Text(t.to_string()));
    }
    if let Some(f) = time_from {
        where_parts.push("begin_transfer_time >= ?".to_string());
        params.push(rusqlite::types::Value::Integer(f));
    }
    if let Some(t) = time_to {
        where_parts.push("begin_transfer_time <= ?".to_string());
        params.push(rusqlite::types::Value::Integer(t));
    }
    let where_sql = if where_parts.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_parts.join(" AND "))
    };
    let total: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM transferTable{}", where_sql),
            rusqlite::params_from_iter(params.iter()),
            |r| r.get(0),
        )
        .unwrap_or(0);
    let mut sessions: Vec<serde_json::Value> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&format!(
        "SELECT session_name, COUNT(*) AS c FROM transferTable{} \
         GROUP BY session_name ORDER BY c DESC, MAX(begin_transfer_time) DESC LIMIT 10",
        where_sql
    )) {
        if let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
            Ok((
                r.get::<_, String>(0).unwrap_or_default(),
                r.get::<_, i64>(1).unwrap_or(0),
            ))
        }) {
            for row in rows.flatten() {
                sessions.push(serde_json::json!({ "name": row.0, "count": row.1 }));
            }
        }
    }
    Ok(serde_json::json!({ "total": total, "sessions": sessions }))
}

/// 统计红包个数（可按会话与时间过滤），供 AI 问答的统计工具使用。
/// 返回 `{ total, sessions: [{ name, count }] }`。
///
/// redEnvelopeTable 无时间戳列；send_id 内嵌发送日期
/// （`1000039901 + YYYYMMDD + …`，本机 65 条实测全部符合），
/// 时间过滤用 `substr(send_id, 11, 8)` 匹配日期。
pub fn stats_redpackets(
    target: Option<&str>,
    time_from: Option<i64>,
    time_to: Option<i64>,
) -> Result<serde_json::Value, String> {
    let conn = open_general().ok_or_else(|| "未找到解密后的 general.db".to_string())?;
    let mut where_parts: Vec<String> = Vec::new();
    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(t) = target.filter(|t| !t.is_empty()) {
        where_parts.push("(session_name = ? OR sender_user_name = ?)".to_string());
        params.push(rusqlite::types::Value::Text(t.to_string()));
        params.push(rusqlite::types::Value::Text(t.to_string()));
    }
    if let Some(f) = time_from {
        where_parts.push("substr(send_id, 11, 8) >= ?".to_string());
        params.push(rusqlite::types::Value::Text(epoch_to_ymd(f)));
    }
    if let Some(t) = time_to {
        where_parts.push("substr(send_id, 11, 8) <= ?".to_string());
        params.push(rusqlite::types::Value::Text(epoch_to_ymd(t)));
    }
    let where_sql = if where_parts.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_parts.join(" AND "))
    };
    let total: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM redEnvelopeTable{}", where_sql),
            rusqlite::params_from_iter(params.iter()),
            |r| r.get(0),
        )
        .unwrap_or(0);
    let mut sessions: Vec<serde_json::Value> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&format!(
        "SELECT session_name, COUNT(*) AS c FROM redEnvelopeTable{} \
         GROUP BY session_name ORDER BY c DESC, MAX(message_server_id) DESC LIMIT 10",
        where_sql
    )) {
        if let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
            Ok((
                r.get::<_, String>(0).unwrap_or_default(),
                r.get::<_, i64>(1).unwrap_or(0),
            ))
        }) {
            for row in rows.flatten() {
                sessions.push(serde_json::json!({ "name": row.0, "count": row.1 }));
            }
        }
    }
    Ok(serde_json::json!({ "total": total, "sessions": sessions }))
}
