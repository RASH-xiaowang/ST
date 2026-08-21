// ============================================================
// 微信 IPC — 全局消息搜索域（搜索 / 索引构建 / 每日计数）
// 依赖：helpers / chat_search_index / annual / modules::common / config
// ============================================================

use crate::wechat::handlers::helpers;

// ============================================================
// 全局消息搜索（迁移自 WeChatDataAnalysis 的聊天记录搜索）
// ============================================================

/// 在所有会话中搜索文本消息（解码后模糊匹配，返回命中列表）。
///
/// 优先走 FTS5 搜索索引，索引未构建/不可用时回退全表扫描。
/// 供 IPC 命令 `search_wechat_messages` 与 AI 问答（ask_wechat）共用。
pub(crate) fn scan_search_messages(
    query: String,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    let q = query.trim().to_string();
    if q.is_empty() {
        return Ok(serde_json::json!({ "hits": [], "total": 0 }));
    }
    // 优先走 FTS5 搜索索引（大数据量下快一个量级）
    if let Ok(indexed) = crate::wechat::chat_search_index::search_indexed(&q, limit.unwrap_or(100))
    {
        // 索引命中但结果为 0 时不能直接返回：单字/口语短语在 FTS 分词下常搜不到，
        // 需要继续回退全表扫描
        if indexed
            .get("indexed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            && indexed
                .get("hits")
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false)
        {
            return Ok(indexed);
        }
    }
    // 索引未构建/不可用 → 回退全表扫描
    {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let usernames = crate::wechat::annual::load_session_usernames(&cfg.decrypted_dir);
        let names = crate::wechat::annual::load_display_names(&cfg.decrypted_dir, &usernames);
        let hit_limit = limit.unwrap_or(100).min(300);
        let q_lower = q.to_lowercase();

        let mut hits: Vec<serde_json::Value> = Vec::new();
        let mut scan_budget: i64 = 800_000;

        'outer: for username in usernames.iter().take(800) {
            if hits.len() >= hit_limit {
                break;
            }
            let table = crate::wechat::modules::common::msg_table_name(username);
            let mut dbs =
                crate::wechat::modules::common::find_db_files(&cfg.decrypted_dir, "message_");
            dbs.extend(crate::wechat::modules::common::find_db_files(
                &cfg.decrypted_dir,
                "biz_message_",
            ));
            dbs.sort();
            dbs.dedup();
            dbs.retain(|p| crate::wechat::modules::common::is_message_shard_file(p));
            for db_path in dbs {
                if hits.len() >= hit_limit || scan_budget <= 0 {
                    break 'outer;
                }
                let conn = match crate::wechat::modules::common::open_readonly_db(&db_path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                if !crate::wechat::modules::common::table_exists(&conn, &table) {
                    continue;
                }
                let sql = format!(
                    "SELECT local_id, create_time, message_content FROM \"{}\" WHERE local_type=1",
                    table
                );
                let mut stmt = match conn.prepare(&sql) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let rows = stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                        r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                        crate::wechat::modules::common::get_bytes(r, 2)
                            .map(|b| crate::wechat::modules::common::decode_blob_text(&b))
                            .unwrap_or_default(),
                    ))
                });
                let rows = match rows {
                    Ok(rs) => rs,
                    Err(_) => continue,
                };
                for row in rows.flatten() {
                    scan_budget -= 1;
                    if hits.len() >= hit_limit || scan_budget <= 0 {
                        break 'outer;
                    }
                    let (local_id, ts, text) = row;
                    if !text.to_lowercase().contains(&q_lower) {
                        continue;
                    }
                    // 提取匹配片段
                    let mut snippet = text.replace('\n', " ");
                    let lower = snippet.to_lowercase();
                    if let Some(pos) = lower.find(&q_lower) {
                        let chars: Vec<char> = snippet.chars().collect();
                        let char_pos = snippet[..pos].chars().count();
                        let start = char_pos.saturating_sub(20);
                        let end = (char_pos + q_lower.chars().count() + 40).min(chars.len());
                        let mid: String = chars[start..end].iter().collect();
                        snippet = if start > 0 {
                            format!("…{}", mid)
                        } else {
                            mid
                        };
                    }
                    let name = names
                        .get(username)
                        .cloned()
                        .unwrap_or_else(|| username.clone());
                    hits.push(serde_json::json!({
                        "username": username,
                        "name": name,
                        "local_id": local_id,
                        "ts": ts,
                        "time": crate::wechat::modules::common::format_full_time(ts),
                        "snippet": snippet,
                    }));
                    if hits.len() >= hit_limit {
                        break 'outer;
                    }
                }
            }
        }
        Ok(serde_json::json!({ "hits": hits, "total": hits.len() }))
    }
}

/// 在所有会话中搜索文本消息（IPC 入口）
#[tauri::command]
pub async fn search_wechat_messages(
    query: String,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || scan_search_messages(query, limit)).await
}

/// 构建 / 重建微信消息搜索索引（FTS5）
#[tauri::command]
pub async fn build_wechat_search_index(force: Option<bool>) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        crate::wechat::chat_search_index::build_search_index(force.unwrap_or(false))
    })
    .await
}

/// 微信消息搜索索引状态
#[tauri::command]
pub async fn get_wechat_search_index_status() -> Result<serde_json::Value, String> {
    Ok(crate::wechat::chat_search_index::get_search_index_status())
}

/// 某月每日消息数（热力图数据）：返回 { "1": count, "2": count, ... }
#[tauri::command]
pub async fn get_chat_daily_counts(
    username: String,
    year: i64,
    month: i64,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        if !(1..=12).contains(&month) {
            return Err("无效月份".to_string());
        }
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let decrypted = cfg.decrypted_dir.clone();
        let (start_ts, end_ts) = {
            use chrono::{Local, TimeZone};
            let start = Local
                .with_ymd_and_hms(year as i32, month as u32, 1, 0, 0, 0)
                .single()
                .ok_or_else(|| "无效日期".to_string())?;
            let end = if month == 12 {
                Local
                    .with_ymd_and_hms(year as i32 + 1, 1, 1, 0, 0, 0)
                    .single()
                    .unwrap_or(start)
            } else {
                Local
                    .with_ymd_and_hms(year as i32, month as u32 + 1, 1, 0, 0, 0)
                    .single()
                    .unwrap_or(start)
            };
            (start.timestamp(), end.timestamp())
        };
        let table = crate::wechat::modules::common::msg_table_name(&username);
        let msg_dir = decrypted.join("message");
        let mut counts: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        if let Ok(entries) = std::fs::read_dir(&msg_dir) {
            let mut dbs: Vec<std::path::PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().and_then(|e| e.to_str()) == Some("db")
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| {
                                n.starts_with("message_")
                                    && !n.contains("fts")
                                    && !n.contains("resource")
                                    && !n.contains("media")
                            })
                            .unwrap_or(false)
                })
                .collect();
            dbs.sort();
            for db in dbs {
                let Ok(conn) = crate::wechat::modules::common::open_readonly_db(&db) else {
                    continue;
                };
                if !crate::wechat::modules::common::table_exists(&conn, &table) {
                    continue;
                }
                let sql = format!(
                    "SELECT strftime('%d', create_time, 'unixepoch', 'localtime') AS d, COUNT(*) \
                     FROM \"{}\" WHERE create_time >= ?1 AND create_time < ?2 GROUP BY d",
                    table
                );
                {
                    let mut stmt = match conn.prepare(&sql) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    let rows: Vec<(String, i64)> = stmt
                        .query_map(rusqlite::params![start_ts, end_ts], |row| {
                            Ok((
                                row.get::<_, String>(0).unwrap_or_default(),
                                row.get::<_, i64>(1).unwrap_or(0),
                            ))
                        })
                        .map(|r| r.flatten().collect())
                        .unwrap_or_default();
                    for (day, cnt) in rows {
                        counts.insert(day, serde_json::json!(cnt));
                    }
                }
            }
        }
        Ok(serde_json::json!({ "counts": counts, "year": year, "month": month }))
    })
    .await
}

/// 会话消息构成统计（各消息类型条数，聊天头部画像）
#[tauri::command]
pub async fn get_session_message_stats(username: String) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let stats = crate::wechat::modules::messages::get_session_message_type_stats(
            &cfg.decrypted_dir,
            &username,
        )?;
        serde_json::to_value(stats).map_err(|e| e.to_string())
    })
    .await
}
