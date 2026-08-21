//! 年度总结模块（迁移自 WeChatDataAnalysis 的 wrapped 年度报告）
//!
//! 数据来源：解密后的 `message/message_*.db` 与 `biz_message/biz_message_*.db`
//! （表名 `Msg_<md5(username)>`），会话 username 由 `session/session.db` 提供。
//!
//! 提供：
//! - `available_years`     有数据的年份列表（降序）
//! - `annual_summary`      指定年份的全局统计（消息量/活跃天数/热力图/词频/表情等）

mod scan;
pub(crate) use scan::{
    list_msg_tables, list_shard_dbs, load_display_names, load_session_usernames, read_text,
};
mod types;
pub use types::*;
mod utils;
use utils::{
    fmt_date, fmt_time, is_emoji_char, is_valid_phrase, kind_label, kind_label_zh, local_datetime,
    plain_text, year_range,
};

#[allow(unused_imports)]
use chrono::Datelike;

use std::collections::HashMap;
use std::path::Path;

use super::modules::common;

// ─── 主计算 ───

/// 有消息数据的年份（降序）
pub fn available_years(decrypted_dir: &Path) -> Vec<i32> {
    let mut years: std::collections::BTreeSet<i32> = std::collections::BTreeSet::new();
    for db_path in list_shard_dbs(decrypted_dir) {
        let conn = match common::open_readonly_db(&db_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for table in list_msg_tables(&conn) {
            let sql = format!(
                "SELECT MIN({expr}), MAX({expr}) FROM \"{t}\"",
                expr = common::ts_expr(),
                t = table
            );
            if let Ok(mut stmt) = conn.prepare(&sql) {
                if let Ok(mut rows) = stmt.query_map([], |r| {
                    Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?))
                }) {
                    if let Some(Ok((mn, mx))) = rows.next() {
                        for ts in [mn, mx].into_iter().flatten() {
                            if let Some(dt) = local_datetime(ts) {
                                years.insert(dt.year());
                            }
                        }
                    }
                }
            }
        }
    }
    years.into_iter().rev().collect()
}

/// 指定年份的全局年度总结
pub fn annual_summary(decrypted_dir: &Path, year: i32) -> Result<AnnualSummary, String> {
    let (start_ts, end_ts) = year_range(year);
    let session_usernames = load_session_usernames(decrypted_dir);
    let display_names = load_display_names(decrypted_dir, &session_usernames);

    // 表名 → username
    let mut table_owner: HashMap<String, String> = HashMap::new();
    for u in &session_usernames {
        table_owner.insert(common::msg_table_name(u), u.clone());
    }

    let mut total_messages: i64 = 0;
    let mut text_messages: i64 = 0;
    let mut total_chars: i64 = 0;
    let mut kind_counts: HashMap<String, i64> = HashMap::new();
    let mut monthly_counts = vec![0i64; 12];
    let mut heatmap = vec![vec![0i64; 24]; 7];
    let mut per_conversation: HashMap<String, i64> = HashMap::new();
    let mut phrase_counts: HashMap<String, i64> = HashMap::new();
    let mut emoji_counts: HashMap<String, i64> = HashMap::new();
    let mut day_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut earliest: Option<(i64, i64, String, String)> = None; // (ts, local_id, table, text)
    let mut latest: Option<(i64, i64, String, String)> = None;

    for db_path in list_shard_dbs(decrypted_dir) {
        let conn = match common::open_readonly_db(&db_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for table in list_msg_tables(&conn) {
            let owner = table_owner
                .get(&table)
                .cloned()
                .unwrap_or_else(|| table.clone());

            // 1) 年度内行数 / 活跃天数 / 最早最晚
            let sql_meta = format!(
                "SELECT COUNT(*), MIN({expr}), MAX({expr}) FROM \"{t}\" WHERE {expr} >= ?1 AND {expr} < ?2",
                expr = common::ts_expr(),
                t = table
            );
            let (cnt, mn, mx): (i64, Option<i64>, Option<i64>) =
                match conn.query_row(&sql_meta, rusqlite::params![start_ts, end_ts], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                }) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
            if cnt <= 0 {
                continue;
            }
            total_messages += cnt;
            *per_conversation.entry(owner.clone()).or_insert(0) += cnt;

            // 活跃天数：全局按日去重（同一日期跨多分片只计一次）
            let sql_days = format!(
                "SELECT DISTINCT date(datetime({expr},'unixepoch','localtime')) FROM \"{t}\" \
                 WHERE {expr} >= ?1 AND {expr} < ?2",
                expr = common::ts_expr(),
                t = table
            );
            if let Ok(mut stmt) = conn.prepare(&sql_days) {
                if let Ok(rows) = stmt.query_map(rusqlite::params![start_ts, end_ts], |r| {
                    r.get::<_, String>(0)
                }) {
                    for r in rows.flatten() {
                        day_set.insert(r);
                    }
                }
            }

            // 2) 类型分布 / 月度 / 热力图（一次 GROUP BY 各一）
            let sql_kind = format!(
                "SELECT local_type, COUNT(*) FROM \"{t}\" WHERE {expr} >= ?1 AND {expr} < ?2 GROUP BY local_type",
                expr = common::ts_expr(),
                t = table
            );
            if let Ok(mut stmt) = conn.prepare(&sql_kind) {
                if let Ok(rows) = stmt.query_map(rusqlite::params![start_ts, end_ts], |r| {
                    Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, i64>(1)?))
                }) {
                    for r in rows.flatten() {
                        let k = kind_label(r.0.unwrap_or(0)).to_string();
                        *kind_counts.entry(k).or_insert(0) += r.1;
                    }
                }
            }

            let sql_month = format!(
                "SELECT CAST(strftime('%m', datetime({expr},'unixepoch','localtime')) AS INTEGER) - 1 AS m, COUNT(*) \
                 FROM \"{t}\" WHERE {expr} >= ?1 AND {expr} < ?2 GROUP BY m",
                expr = common::ts_expr(),
                t = table
            );
            if let Ok(mut stmt) = conn.prepare(&sql_month) {
                if let Ok(rows) = stmt.query_map(rusqlite::params![start_ts, end_ts], |r| {
                    Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, i64>(1)?))
                }) {
                    for r in rows.flatten() {
                        if let Some(m) = r.0 {
                            if (0..12).contains(&m) {
                                monthly_counts[m as usize] += r.1;
                            }
                        }
                    }
                }
            }

            let sql_heat = format!(
                "SELECT ((CAST(strftime('%w', datetime({expr},'unixepoch','localtime')) AS INTEGER)+6)%7) AS w, \
                 CAST(strftime('%H', datetime({expr},'unixepoch','localtime')) AS INTEGER) AS h, COUNT(*) \
                 FROM \"{t}\" WHERE {expr} >= ?1 AND {expr} < ?2 GROUP BY w, h",
                expr = common::ts_expr(),
                t = table
            );
            if let Ok(mut stmt) = conn.prepare(&sql_heat) {
                if let Ok(rows) = stmt.query_map(rusqlite::params![start_ts, end_ts], |r| {
                    Ok((
                        r.get::<_, Option<i64>>(0)?,
                        r.get::<_, Option<i64>>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                }) {
                    for r in rows.flatten() {
                        if let (Some(w), Some(h)) = (r.0, r.1) {
                            if (0..7).contains(&w) && (0..24).contains(&h) {
                                heatmap[w as usize][h as usize] += r.2;
                            }
                        }
                    }
                }
            }

            // 3) 文本内容扫描（短语/表情/字数/最早最晚文本）
            let sql_text = format!(
                "SELECT local_id, local_type, {expr}, message_content FROM \"{t}\" \
                 WHERE {expr} >= ?1 AND {expr} < ?2 AND local_type IN (1, 47) \
                 ORDER BY {expr} ASC LIMIT ?3",
                expr = common::ts_expr(),
                t = table
            );
            // 先统计该表文本行数，避免 LIMIT 截断影响短语/字数统计
            let sql_text_count = format!(
                "SELECT COUNT(*) FROM \"{t}\" WHERE {expr} >= ?1 AND {expr} < ?2 AND local_type IN (1, 47)",
                expr = common::ts_expr(),
                t = table
            );
            let text_rows: i64 = conn
                .query_row(&sql_text_count, rusqlite::params![start_ts, end_ts], |r| {
                    r.get(0)
                })
                .unwrap_or(0);
            if text_rows > 0 {
                if let Ok(mut stmt) = conn.prepare(&sql_text) {
                    if let Ok(rows) =
                        stmt.query_map(rusqlite::params![start_ts, end_ts, text_rows], |r| {
                            Ok((
                                r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                                r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                                read_text(r, 3),
                            ))
                        })
                    {
                        for r in rows.flatten() {
                            let (local_id, local_type, ts, content) = r;
                            if local_type == 1 {
                                let text = plain_text(&content);
                                if !text.is_empty() {
                                    text_messages += 1;
                                    total_chars += text.chars().count() as i64;
                                    if is_valid_phrase(&text) {
                                        *phrase_counts.entry(text.clone()).or_insert(0) += 1;
                                    }
                                    for c in text.chars() {
                                        if is_emoji_char(c) {
                                            *emoji_counts.entry(c.to_string()).or_insert(0) += 1;
                                        }
                                    }
                                }
                                let is_earlier = earliest
                                    .as_ref()
                                    .map(|(e_ts, _, _, _)| ts < *e_ts)
                                    .unwrap_or(true);
                                if is_earlier && ts > 0 {
                                    earliest = Some((ts, local_id, table.clone(), text.clone()));
                                }
                                let is_later = latest
                                    .as_ref()
                                    .map(|(l_ts, _, _, _)| ts > *l_ts)
                                    .unwrap_or(true);
                                if is_later && ts > 0 {
                                    latest = Some((ts, local_id, table.clone(), text));
                                }
                            } else if local_type == 47 {
                                let e = content.trim().to_string();
                                if !e.is_empty()
                                    && e.len() <= 48
                                    && !e.chars().all(|c| c.is_ascii_hexdigit())
                                    && !e.starts_with('<')
                                {
                                    *emoji_counts.entry(e.clone()).or_insert(0) += 1;
                                }
                            }
                        }
                    }
                }
            }

            let _ = mn;
            let _ = mx;
        }
    }
    let active_days = day_set.len() as i64;

    // 排序输出
    let top_items = |map: &HashMap<String, i64>, limit: usize| -> Vec<TopItem> {
        let mut v: Vec<(String, i64)> = map.iter().map(|(k, c)| (k.clone(), *c)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.into_iter()
            .take(limit)
            .map(|(k, c)| TopItem {
                key: k.clone(),
                name: k.clone(),
                count: c,
            })
            .collect()
    };

    let mut contacts: Vec<TopItem> = per_conversation
        .iter()
        .filter(|(u, _)| !u.ends_with("@chatroom") && !u.starts_with("gh_"))
        .map(|(u, c)| TopItem {
            key: u.clone(),
            name: display_names.get(u).cloned().unwrap_or_else(|| u.clone()),
            count: *c,
        })
        .collect();
    contacts.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    contacts.truncate(5);

    let mut groups: Vec<TopItem> = per_conversation
        .iter()
        .filter(|(u, _)| u.ends_with("@chatroom"))
        .map(|(u, c)| TopItem {
            key: u.clone(),
            name: display_names.get(u).cloned().unwrap_or_else(|| u.clone()),
            count: *c,
        })
        .collect();
    groups.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    groups.truncate(5);

    let top_phrases: Vec<TopItem> = top_items(&phrase_counts, 12);
    let top_emojis: Vec<TopItem> = top_items(&emoji_counts, 12);

    let kind_counts = {
        let mut v: Vec<(String, i64)> = kind_counts.into_iter().collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.1));
        v.into_iter()
            .map(|(k, c)| serde_json::json!({ "kind": k, "label": kind_label_zh(&k), "count": c }))
            .collect()
    };

    let heatmap_total: i64 = heatmap.iter().map(|row| row.iter().sum::<i64>()).sum();
    let heatmap = serde_json::json!({
        "weekdayLabels": ["周一","周二","周三","周四","周五","周六","周日"],
        "hourLabels": (0..24).map(|h| format!("{:02}", h)).collect::<Vec<_>>(),
        "matrix": heatmap,
        "total": heatmap_total,
    });

    let to_moment = |item: Option<(i64, i64, String, String)>| -> Option<MomentItem> {
        let (ts, _local_id, table, text) = item?;
        let username = table_owner
            .get(&table)
            .cloned()
            .unwrap_or_else(|| table.clone());
        Some(MomentItem {
            ts,
            time: fmt_time(ts),
            date: fmt_date(ts),
            username: username.clone(),
            name: display_names
                .get(&username)
                .cloned()
                .unwrap_or_else(|| username.clone()),
            text: {
                let mut s = text;
                s = s.replace('\n', " ");
                if s.chars().count() > 80 {
                    s = s.chars().take(80).collect::<String>() + "…";
                }
                s
            },
        })
    };

    Ok(AnnualSummary {
        year,
        total_messages,
        text_messages,
        active_days,
        total_chars,
        avg_chars: if text_messages > 0 {
            total_chars as f64 / text_messages as f64
        } else {
            0.0
        },
        kind_counts,
        monthly_counts,
        heatmap,
        top_contacts: contacts,
        top_groups: groups,
        top_phrases,
        top_emojis,
        earliest: to_moment(earliest),
        latest: to_moment(latest),
    })
}

#[cfg(test)]
mod tests;
