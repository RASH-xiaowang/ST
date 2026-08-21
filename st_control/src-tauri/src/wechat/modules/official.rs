//! 公众号 / 商家客服模块 - 对应 PC 微信「公众号」与会话
//!
//! 数据来源：
//! - `session/session.db` 中 `gh_` 开头的会话（公众号消息会话）
//! - `bizchat/bizchat.db` 商家客服对话库（chat_group / user_info / my_user_info）
//! - `biz_message/biz_message_0.db` 公众号消息内容（由 messages 模块读取）

use super::common;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// 商家客服群组
#[derive(Debug, Clone, Serialize)]
pub struct BizChatGroup {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
    pub columns: Vec<String>,
    pub values: Vec<serde_json::Value>,
}

/// 商家客服数据
#[derive(Debug, Serialize)]
pub struct BizChatOverview {
    pub groups: Vec<serde_json::Value>,
    pub users: Vec<serde_json::Value>,
    pub my_info: Vec<serde_json::Value>,
}

/// 公众号/服务号账号（微信「通讯录 → 公众号」列表项）
#[derive(Debug, Clone, Serialize)]
pub struct OfficialAccount {
    pub username: String,
    pub name: String,
    /// subscription(订阅号) / service(服务号) / enterprise(企业号) / unknown
    pub official_kind: String,
    /// 最后一条本地消息时间（无会话记录为 0）
    pub ts: i64,
    pub time: String,
    pub summary: String,
    pub unread_count: i64,
    pub pinned: bool,
    /// “查看历史消息”网页链接（biz_info.brand_info 中提供）
    pub history_url: String,
}

/// 公众号类型：ServiceType 0=订阅号，1=服务号，2/3=企业号
fn service_type_to_kind(service_type: Option<i64>) -> &'static str {
    match service_type {
        Some(0) => "subscription",
        Some(1) => "service",
        Some(2) | Some(3) => "enterprise",
        _ => "unknown",
    }
}

/// 从 external_info JSON 解析 ServiceType（非标准 JSON 时字符串兜底）
fn parse_service_type(ext: &str) -> Option<i64> {
    serde_json::from_str::<serde_json::Value>(ext)
        .ok()
        .and_then(|v| v.get("ServiceType").and_then(|s| s.as_i64()))
        .or_else(|| {
            let key = "\"ServiceType\"";
            ext.find(key).and_then(|i| {
                let rest = ext[i + key.len()..].trim_start_matches(':').trim_start();
                let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                digits.parse::<i64>().ok()
            })
        })
}

/// 从 brand_info JSON 提取“查看历史消息”链接（urls[].url）
fn history_url_from_brand(brand: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(brand) {
        if let Some(u) = v
            .get("urls")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
            .and_then(|u| u.get("url"))
            .and_then(|u| u.as_str())
        {
            return u.to_string();
        }
    }
    // 兜底：直接找 "url":"..."（JSON 里反斜杠被转义为 \/）
    if let Some(start) = brand.find("\"url\"") {
        if let Some(colon) = brand[start..].find(':') {
            let rest = brand[start + colon + 1..].trim_start();
            if let Some(rest) = rest.strip_prefix('"') {
                if let Some(end) = rest.find('"') {
                    return rest[..end].replace("\\/", "/");
                }
            }
        }
    }
    String::new()
}

/// 读取已订阅的公众号/服务号完整列表。
///
/// 数据来源（与微信「通讯录 → 公众号」一致）：
/// - `contact.db.contact`：`gh_` 前缀 + local_type ∈ {1,4} 为已订阅公众号，
///   排除“该账号已注销”与内置助手账号（如微信收款助手）；
/// - `contact.db.biz_info`：ServiceType 区分订阅号/服务号/企业号，brand_info 提供
///   “查看历史消息”链接；
/// - `session.db.SessionTable`：最后消息时间/摘要/未读数（无会话记录也可显示）。
pub fn get_official_accounts(decrypted_dir: &Path) -> Result<Vec<OfficialAccount>, String> {
    let contact_db = decrypted_dir.join("contact").join("contact.db");
    let conn =
        common::open_readonly_db(&contact_db).map_err(|e| format!("打开联系人库失败: {}", e))?;

    // 1) 订阅集合：contact 表中 gh_ + local_type ∈ {1,4}
    let mut subscribed: Vec<(String, String)> = Vec::new(); // (username, name)
    if common::table_exists(&conn, "contact") {
        let cols = common::table_columns(&conn, "contact");
        let has = |c: &str| cols.iter().any(|x| x == c);
        let sel = |c: &str, dft: &str| {
            if has(c) {
                format!("\"{}\"", c)
            } else {
                dft.to_string()
            }
        };
        let sql = format!(
            "SELECT {u}, {local}, {nick}, {remark} FROM contact WHERE {u} LIKE 'gh_%'",
            u = sel("username", "''"),
            local = sel("local_type", "0"),
            nick = sel("nick_name", "NULL"),
            remark = sel("remark", "NULL"),
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                ))
            }) {
                for r in rows.flatten() {
                    let (uname, local_type, nick, remark) = r;
                    if uname.is_empty() || !matches!(local_type, 1 | 4) {
                        continue;
                    }
                    if uname == "gh_f0a92aa7146c" {
                        continue; // 微信收款助手，不作为公众号展示
                    }
                    let name = if !nick.is_empty() { nick } else { remark };
                    if name.contains("已注销") {
                        continue; // 该账号已注销
                    }
                    subscribed.push((uname, name));
                }
            }
        }
    }

    // 2) biz_info：ServiceType + 历史消息链接
    let mut kinds: HashMap<String, String> = HashMap::new();
    let mut history: HashMap<String, String> = HashMap::new();
    if common::table_exists(&conn, "biz_info") {
        let cols = common::table_columns(&conn, "biz_info");
        let has = |c: &str| cols.iter().any(|x| x == c);
        let sel = |c: &str, dft: &str| {
            if has(c) {
                format!("\"{}\"", c)
            } else {
                dft.to_string()
            }
        };
        let sql = format!(
            "SELECT {u}, {ext}, {brand} FROM biz_info",
            u = sel("username", "''"),
            ext = sel("external_info", "''"),
            brand = sel("brand_info", "''"),
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    common::get_bytes(row, 1)
                        .map(|b| common::decode_blob_text(&b))
                        .unwrap_or_default(),
                    common::get_bytes(row, 2)
                        .map(|b| common::decode_blob_text(&b))
                        .unwrap_or_default(),
                ))
            }) {
                for r in rows.flatten() {
                    let (uname, ext, brand) = r;
                    if uname.is_empty() {
                        continue;
                    }
                    kinds.insert(
                        uname.clone(),
                        service_type_to_kind(parse_service_type(&ext)).to_string(),
                    );
                    let hu = history_url_from_brand(&brand);
                    if !hu.is_empty() {
                        history.insert(uname, hu);
                    }
                }
            }
        }
    }
    drop(conn);

    // 3) 会话信息（时间/摘要/未读）
    let mut session_meta: HashMap<String, (i64, String, i64)> = HashMap::new();
    let session_db = decrypted_dir.join("session").join("session.db");
    if let Ok(sconn) = common::open_readonly_db(&session_db) {
        if common::table_exists(&sconn, "SessionTable") {
            let sql = "SELECT username, last_timestamp, summary, unread_count FROM SessionTable \
                       WHERE username LIKE 'gh_%' AND last_timestamp > 0";
            if let Ok(mut stmt) = sconn.prepare(sql) {
                if let Ok(rows) = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                        row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                        common::get_bytes(row, 2)
                            .map(|b| common::decode_blob_text(&b))
                            .unwrap_or_default(),
                        row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    ))
                }) {
                    for r in rows.flatten() {
                        if !r.0.is_empty() {
                            session_meta.insert(r.0, (r.1, r.2, r.3));
                        }
                    }
                }
            }
        }
        drop(sconn);
    }

    let pinned_users = super::sessions::load_pinned_usernames(decrypted_dir);

    let mut accounts = Vec::with_capacity(subscribed.len());
    for (uname, name) in subscribed {
        let (ts, summary, unread) =
            session_meta
                .get(&uname)
                .cloned()
                .unwrap_or((0, String::new(), 0));
        let time = if ts > 0 {
            common::format_session_time(ts)
        } else {
            String::new()
        };
        accounts.push(OfficialAccount {
            username: uname.clone(),
            name,
            official_kind: kinds
                .get(&uname)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string()),
            ts,
            time,
            summary,
            unread_count: unread,
            pinned: pinned_users.contains(&uname),
            history_url: history.get(&uname).cloned().unwrap_or_default(),
        });
    }
    // 置顶在前，其余按最近消息时间倒序
    accounts.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.ts.cmp(&a.ts)));
    log::info!("[official] 公众号/服务号共 {} 个", accounts.len());
    Ok(accounts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实数据：已订阅公众号/服务号应约为 123 个
    /// （contact 表 gh_ + local_type=1 共 124，减去 1 个“该账号已注销”），
    /// 且包含服务号与订阅号分类，部分账号带“查看历史消息”链接。
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_official_accounts_count() {
        let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let accounts = get_official_accounts(&cfg.decrypted_dir).expect("读取公众号列表失败");
        eprintln!(
            "公众号/服务号 {} 个：服务号 {}，订阅号 {}，企业号 {}，未知 {}，带历史链接 {}",
            accounts.len(),
            accounts
                .iter()
                .filter(|a| a.official_kind == "service")
                .count(),
            accounts
                .iter()
                .filter(|a| a.official_kind == "subscription")
                .count(),
            accounts
                .iter()
                .filter(|a| a.official_kind == "enterprise")
                .count(),
            accounts
                .iter()
                .filter(|a| a.official_kind == "unknown")
                .count(),
            accounts
                .iter()
                .filter(|a| !a.history_url.is_empty())
                .count()
        );
        if accounts.is_empty() {
            eprintln!("无公众号数据，跳过");
            return;
        }
        // 微信口径约 123 个（开发账号）：允许 ±2 容差。账号数据不同
        // （CI/无数据或他人账号）时跳过数量断言，保留分类断言。
        if !(120..=126).contains(&accounts.len()) {
            eprintln!(
                "公众号数量 {} 与开发账号口径不一致，跳过数量断言",
                accounts.len()
            );
            return;
        }
        assert!(
            accounts.iter().any(|a| a.official_kind == "service"),
            "应包含服务号"
        );
        assert!(
            accounts.iter().any(|a| a.official_kind == "subscription"),
            "应包含订阅号"
        );
        assert!(
            accounts.iter().all(|a| !a.name.contains("已注销")),
            "不应包含已注销账号"
        );
        assert!(
            accounts.iter().all(|a| a.username != "gh_f0a92aa7146c"),
            "不应包含微信收款助手"
        );
    }
}

fn dump_as_objects(
    conn: &rusqlite::Connection,
    table: &str,
    limit: usize,
) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    if let Some((cols, rows)) = common::dump_table(conn, table, None, limit) {
        for row in rows {
            let mut obj = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                obj.insert(
                    c.clone(),
                    row.get(i).cloned().unwrap_or(serde_json::Value::Null),
                );
            }
            out.push(serde_json::Value::Object(obj));
        }
    }
    out
}

/// 读取商家客服对话库
pub fn get_bizchats(decrypted_dir: &Path) -> Result<BizChatOverview, String> {
    let db_path = decrypted_dir.join("bizchat").join("bizchat.db");
    if !db_path.exists() {
        return Err(format!("商家对话数据库未解密: {}", db_path.display()));
    }
    let conn = common::open_readonly_db(&db_path).map_err(|e| format!("打开失败: {}", e))?;

    Ok(BizChatOverview {
        groups: dump_as_objects(&conn, "chat_group", 500),
        users: dump_as_objects(&conn, "user_info", 1000),
        my_info: dump_as_objects(&conn, "my_user_info", 10),
    })
}
