// ============================================================
// 每日总结模块 — 数据检索层
// 自 daily_summary.rs 拆分：群成员读取与当日消息提取/计数。
// ============================================================

use crate::wechat::modules::common;
use rusqlite::OptionalExtension;
use std::path::Path;

// ─── 群成员 ───

/// 获取某群聊的成员列表（username / 显示名）
///
/// 数据来源：`contact.db` 的 `chatroom_member` 表（room_id → member_id），
/// 其中 member_id 指向 `contact` 表的 id。不同微信版本的关联列名可能是
/// `member_id` 或 `contact_id`，这里自动兼容。
pub fn get_group_members(
    decrypted_dir: &Path,
    group_username: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let contact_db = decrypted_dir.join("contact").join("contact.db");
    if !contact_db.exists() {
        return Ok(Vec::new());
    }
    let conn = common::open_readonly_db(&contact_db).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if !common::table_exists(&conn, "chatroom_member") {
        log::warn!("[daily_summary] contact.db 缺少 chatroom_member 表");
        return Ok(Vec::new());
    }
    // 兼容不同微信版本：member_id / contact_id
    let cols = common::table_columns(&conn, "chatroom_member");
    let member_col = if cols.iter().any(|c| c == "member_id") {
        "member_id"
    } else if cols.iter().any(|c| c == "contact_id") {
        "contact_id"
    } else {
        log::warn!("[daily_summary] chatroom_member 表缺少 member_id/contact_id 列");
        return Ok(Vec::new());
    };
    // room_id = contact 表中该群聊的 id
    let room_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM contact WHERE username=?1 LIMIT 1",
            rusqlite::params![group_username],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let room_id = match room_id {
        Some(id) => id,
        None => {
            log::warn!("[daily_summary] 群聊 {} 不在 contact 表中", group_username);
            return Ok(Vec::new());
        }
    };
    // chatroom_member.member_id → contact 表
    let sql = format!(
        "SELECT c.username, c.nick_name, c.remark
         FROM chatroom_member m
         JOIN contact c ON c.id = m.{member_col}
         WHERE m.room_id=?1
         ORDER BY c.remark IS NULL, c.remark COLLATE NOCASE, c.nick_name COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![room_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for r in rows.flatten() {
        let (username, nick, remark) = r;
        if username.is_empty() {
            continue;
        }
        let name = remark
            .filter(|s| !s.is_empty())
            .or_else(|| nick.filter(|s| !s.is_empty()))
            .unwrap_or_else(|| username.clone());
        out.push(serde_json::json!({ "username": username, "name": name }));
    }
    Ok(out)
}

// ─── 消息提取 ───

pub(crate) struct DayMessage {
    pub(crate) ts: i64,
    pub(crate) sender: String,
    pub(crate) text: String,
}

/// 读取指定时间范围内群聊的文本消息，按时间升序；可过滤关注成员
pub(crate) fn fetch_day_messages(
    decrypted_dir: &Path,
    group_username: &str,
    target_users: &[String],
    start_ts: i64,
    end_ts: i64,
    cap: usize,
) -> Vec<DayMessage> {
    let table = common::msg_table_name(group_username);
    let mut dbs = common::find_db_files(decrypted_dir, "message_");
    dbs.extend(common::find_db_files(decrypted_dir, "biz_message_"));
    dbs.sort();
    dbs.dedup();
    dbs.retain(|p| common::is_message_shard_file(p));

    let mut out: Vec<DayMessage> = Vec::new();
    let want_all = target_users.is_empty();
    let targets: std::collections::HashSet<&str> =
        target_users.iter().map(|s| s.as_str()).collect();

    for db_path in dbs {
        if out.len() >= cap {
            break;
        }
        let conn = match common::open_readonly_db(&db_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if !common::table_exists(&conn, &table) {
            continue;
        }
        // Name2Id：real_sender_id → username
        let mut name2id: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
        if common::table_exists(&conn, "Name2Id") {
            if let Ok(mut stmt) = conn.prepare("SELECT rowid, user_name FROM Name2Id") {
                if let Ok(rows) =
                    stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
                {
                    for r in rows.flatten() {
                        name2id.insert(r.0, r.1);
                    }
                }
            }
        }
        let sql = format!(
            "SELECT local_type, real_sender_id, {expr}, message_content
             FROM \"{t}\"
             WHERE {expr} >= ?1 AND {expr} < ?2 AND local_type NOT IN (10000, 10002)
             ORDER BY {expr} DESC LIMIT ?3",
            expr = common::ts_expr(),
            t = table
        );
        let per_shard = (cap - out.len()).max(1) as i64;
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let rows = stmt.query_map(rusqlite::params![start_ts, end_ts, per_shard], |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                common::get_bytes(r, 3),
            ))
        });
        let rows = match rows {
            Ok(rs) => rs,
            Err(_) => continue,
        };
        for row in rows.flatten() {
            let (local_type, sender_id, ts, content) = row;
            if out.len() >= cap {
                break;
            }
            let sender = name2id.get(&sender_id).cloned().unwrap_or_default();
            if !want_all && sender.is_empty() {
                continue;
            }
            if !want_all && !targets.contains(sender.as_str()) {
                continue;
            }
            let raw = content
                .as_deref()
                .map(common::decode_blob_text)
                .unwrap_or_default();
            let text = if local_type == 1 {
                let t = raw.trim_start();
                if t.starts_with('<') || t.starts_with("<?xml") {
                    common::strip_xml_tags(t).trim().to_string()
                } else {
                    t.to_string()
                }
            } else {
                raw.trim().to_string()
            };
            if text.is_empty() {
                continue;
            }
            out.push(DayMessage { ts, sender, text });
        }
    }
    // 统一按时间正序输出（取的是每库最近的 cap 条）
    out.sort_by_key(|m| m.ts);
    out
}

/// 统计某群在时间范围内的全部消息数（不做成员过滤，用于给出更准确的错误提示）
pub(crate) fn count_group_messages(
    decrypted_dir: &Path,
    group_username: &str,
    start_ts: i64,
    end_ts: i64,
) -> i64 {
    let table = common::msg_table_name(group_username);
    let mut dbs = common::find_db_files(decrypted_dir, "message_");
    dbs.extend(common::find_db_files(decrypted_dir, "biz_message_"));
    dbs.sort();
    dbs.dedup();
    dbs.retain(|p| common::is_message_shard_file(p));
    let mut total = 0i64;
    for db_path in dbs {
        let Ok(conn) = common::open_readonly_db(&db_path) else {
            continue;
        };
        if !common::table_exists(&conn, &table) {
            continue;
        }
        let sql = format!(
            "SELECT COUNT(*) FROM \"{}\" WHERE {} >= ?1 AND {} < ?2",
            table,
            common::ts_expr(),
            common::ts_expr()
        );
        if let Ok(v) = conn.query_row(&sql, rusqlite::params![start_ts, end_ts], |r| {
            r.get::<_, i64>(0)
        }) {
            total += v;
        }
    }
    total
}
