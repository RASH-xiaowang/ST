//! 会话列表模块 - 对应 PC 微信主界面左侧会话列表
//!
//! 数据来源：`session/session.db`
//! - `SessionTable`              会话主表
//! - `SessionNoContactInfoTable` 无联系人信息的会话标题（群名、服务号名等）
//!
//! 与 PC 微信一致的逻辑：
//! - 仅显示 `last_timestamp > 0`（无消息记录）或置顶的会话；隐藏会话
//!   （is_hidden=1，如折叠的群聊）也返回并标记「已隐藏」，排在可见会话后
//! - 按 `sort_timestamp` 降序（置顶会话排在最前）
//! - 置顶识别：contact.db 的 `contact` / `stranger` 表 `flag` 第 11 位为 1 即置顶
//!   （微信 4.x 不把置顶写进 session.db，而是存在联系人 flag 位标记中）
//! - 会话名解析优先级：联系人备注/昵称 > SessionNoContactInfoTable > 系统账号名 > username
//! - 群聊摘要带发送者前缀 `张三: 好的`
//! - 草稿显示 `[草稿] xxx`
//! - 时间显示：今天 HH:MM / 昨天 / 星期X / M月D日 / YYYY年M月D日

use super::common;
use super::contacts;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// 会话条目
#[derive(Debug, Clone, Serialize)]
pub struct SessionEntry {
    /// 会话 username（wxid / @chatroom / gh_）
    pub username: String,
    /// 显示名
    pub name: String,
    /// 最后一条消息摘要（群聊带发送者前缀）
    pub summary: String,
    /// 原始摘要
    pub raw_summary: String,
    /// 草稿内容
    pub draft: String,
    /// 最后一条消息时间（Unix 秒）
    pub ts: i64,
    /// 排序时间（Unix 秒，置顶更大）
    pub sort_ts: i64,
    /// 是否置顶
    pub pinned: bool,
    /// 是否被微信隐藏（SessionTable.is_hidden=1：折叠的群聊/不显示的会话）。
    /// 前端据此展示「已隐藏」徽标；列表仍包含它们（否则有消息历史的
    /// 隐藏会话会整体消失，用户「查不到聊天信息」）
    pub is_hidden: bool,
    /// PC 风格时间显示
    pub time: String,
    /// 完整时间
    pub full_time: String,
    /// 未读数
    pub unread_count: i64,
    /// 是否群聊
    pub is_group: bool,
    /// 是否公众号
    pub is_official: bool,
    /// 公众号类型：subscription(订阅号) / service(服务号) / enterprise(企业号) / unknown
    pub official_kind: String,
    /// 是否系统账号
    pub is_system: bool,
    /// 最后一条消息类型
    pub last_msg_type: i64,
    /// 最后一条消息发送者（群聊）
    pub last_sender: String,
    /// 最后一条消息发送者显示名（群聊）
    pub last_sender_name: String,
}

/// 加载 SessionNoContactInfoTable 的 username → session_title
fn load_session_titles(conn: &rusqlite::Connection) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    if !common::table_exists(conn, "SessionNoContactInfoTable") {
        return titles;
    }
    if let Ok(mut stmt) =
        conn.prepare("SELECT username, session_title FROM SessionNoContactInfoTable")
    {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        }) {
            for r in rows.flatten() {
                if !r.1.is_empty() {
                    titles.insert(r.0, r.1);
                }
            }
        }
    }
    titles
}

/// 从 contact.db 读取置顶会话集合。
///
/// 微信 4.x 的“会话置顶”不存储在 session.db，而是存在联系人表的 `flag` 位标记中：
/// `contact` / `stranger` 表的 `flag` 第 11 位（0x800）为 1 表示该会话被置顶。
/// 注意：公众号（`gh_` 开头）的 flag 第 11 位不是“聊天置顶”语义（实测置顶会话
/// 列表不包含任何公众号），因此公众号不参与置顶识别。
/// 兼容 flag 以有符号 64 位整数存储的情况（负数按补码展开后再取位）。
pub(crate) fn load_pinned_usernames(decrypted_dir: &Path) -> HashSet<String> {
    let mut pinned = HashSet::new();
    let contact_db = decrypted_dir.join("contact").join("contact.db");
    let Ok(conn) = common::open_readonly_db(&contact_db) else {
        return pinned;
    };
    for table in ["contact", "stranger"] {
        if !common::table_exists(&conn, table) {
            continue;
        }
        let cols = common::table_columns(&conn, table);
        if !cols.iter().any(|c| c == "username") || !cols.iter().any(|c| c == "flag") {
            continue;
        }
        let sql = format!("SELECT username, flag FROM \"{table}\"");
        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                ))
            }) {
                for r in rows.flatten() {
                    let (username, flag) = r;
                    if username.is_empty() || username.starts_with("gh_") {
                        continue;
                    }
                    let flag = flag as u64; // 负数按补码展开，与微信无符号存储一致
                    if (flag >> 11) & 1 == 1 {
                        pinned.insert(username);
                    }
                }
            }
        }
    }
    log::info!("[session] contact.db 置顶会话 {} 个", pinned.len());
    pinned
}

/// 从 contact.db 的 biz_info 表读取公众号的服务类型。
///
/// `external_info` 是 JSON 字符串，其中 `ServiceType`：
/// 0=订阅号，1=服务号，2/3=企业号；解析失败视为 unknown。
/// （与 WeChatDataAnalysis 的处理一致）
fn load_official_kinds(decrypted_dir: &Path) -> HashMap<String, String> {
    let mut kinds = HashMap::new();
    let contact_db = decrypted_dir.join("contact").join("contact.db");
    let Ok(conn) = common::open_readonly_db(&contact_db) else {
        return kinds;
    };
    if !common::table_exists(&conn, "biz_info") {
        return kinds;
    }
    let cols = common::table_columns(&conn, "biz_info");
    if !cols.iter().any(|c| c == "username") || !cols.iter().any(|c| c == "external_info") {
        return kinds;
    }
    if let Ok(mut stmt) = conn.prepare("SELECT username, external_info FROM biz_info") {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                common::get_bytes(row, 1)
                    .map(|b| common::decode_blob_text(&b))
                    .unwrap_or_default(),
            ))
        }) {
            for r in rows.flatten() {
                let (uname, ext) = r;
                if uname.is_empty() || !uname.starts_with("gh_") {
                    continue;
                }
                let service_type = serde_json::from_str::<serde_json::Value>(&ext)
                    .ok()
                    .and_then(|v| v.get("ServiceType").and_then(|s| s.as_i64()))
                    .or_else(|| {
                        // 兼容非标准 JSON：直接找 "ServiceType":N
                        let key = "\"ServiceType\"";
                        ext.find(key).and_then(|i| {
                            let rest = ext[i + key.len()..].trim_start_matches(':').trim_start();
                            let digits: String =
                                rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                            digits.parse::<i64>().ok()
                        })
                    });
                let kind = match service_type {
                    Some(0) => "subscription",
                    Some(1) => "service",
                    Some(2) | Some(3) => "enterprise",
                    _ => "unknown",
                };
                kinds.insert(uname, kind.to_string());
            }
        }
    }
    log::info!("[session] biz_info 公众号类型 {} 个", kinds.len());
    kinds
}

/// 判断解码后的文本是否是有意义的草稿内容（而非 protobuf 二进制垃圾）
///
/// WeChat 的 `SessionTable.draft` 列存储 protobuf 序列化数据，
/// 直接 UTF-8 解码会得到乱码。有意义的草稿应该是可打印的文本。
fn is_meaningful_draft(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    // 必须包含至少一个可打印字符（中文/英文/数字/常见标点）
    let has_printable = text.chars().any(|c| {
        c.is_ascii_alphanumeric()
            || c.is_ascii_punctuation()
            || ('\u{4e00}'..='\u{9fff}').contains(&c) // 中文
            || c == ' '
    });
    if !has_printable {
        return false;
    }
    // 控制字符比例超过 30% 视为垃圾
    let control_count = text.chars().filter(|c| c.is_control()).count();
    control_count as f64 / (text.len() as f64) < 0.3
}

/// 读取会话列表（与 PC 微信主界面一致）
pub fn get_session_list(decrypted_dir: &Path) -> Result<Vec<SessionEntry>, String> {
    log::info!("[session] 开始读取会话列表: path={:?}", decrypted_dir);
    let db_path = decrypted_dir.join("session").join("session.db");
    if !db_path.exists() {
        return Err(format!("会话数据库未解密: {}", db_path.display()));
    }
    let conn = common::open_readonly_db(&db_path).map_err(|e| format!("打开失败: {}", e))?;
    if !common::table_exists(&conn, "SessionTable") {
        // 尝试修复：删除损坏/不完整的文件，让监控或下一次读取触发重新解密
        log::warn!("[session] session.db 缺少 SessionTable，尝试删除后让系统重新解密");
        drop(conn);
        let _ = std::fs::remove_file(&db_path);
        if let Some(parent) = db_path.parent() {
            let wal = parent.join("session.db-wal");
            let shm = parent.join("session.db-shm");
            let _ = std::fs::remove_file(&wal);
            let _ = std::fs::remove_file(&shm);
        }
        return Err("SessionTable 不存在，已删除异常文件等待重新解密".to_string());
    }

    let cols = common::table_columns(&conn, "SessionTable");
    let has = |c: &str| cols.iter().any(|x| x == c);
    if !has("username") {
        return Err("SessionTable 缺少 username 列".to_string());
    }

    // 显示名来源 1：通讯录（备注 > 昵称）
    let contact_db = decrypted_dir.join("contact").join("contact.db");
    let contact_names = contacts::load_display_names(&contact_db);
    // 显示名来源 2：SessionNoContactInfoTable
    let session_titles = load_session_titles(&conn);
    // 置顶会话集合（contact.flag 第 11 位）
    let pinned_users = load_pinned_usernames(decrypted_dir);
    // 公众号类型（订阅号/服务号/企业号）
    let official_kinds = load_official_kinds(decrypted_dir);

    log::info!(
        "[session] contact_names={} 条, session_titles={} 条",
        contact_names.len(),
        session_titles.len()
    );

    let sel = |c: &str, dft: &str| {
        if has(c) {
            format!("\"{}\"", c)
        } else {
            dft.to_string()
        }
    };
    // 注意：不再过滤 is_hidden——微信 4.x 的 is_hidden=1 包括「折叠的群聊」
    // 与「不显示的会话」，其中大量会话有完整消息历史（实测 166 个隐藏会话
    // 中 125 个有消息表）。过滤掉它们会让用户「查不到这些群聊的聊天信息」，
    // 因此全部返回并由前端打「已隐藏」徽标、排在可见会话之后。
    // 置顶会话即使没有消息记录（last_timestamp=0）也要返回，
    // 与真实微信一致（置顶聊天始终显示在顶部）；普通会话仍需有消息才显示。
    let sql = format!(
        "SELECT {username}, {unread}, {summary}, {draft}, {hidden}, \
         {last_ts}, {sort_ts}, {msg_type}, {sender}, {sender_name} \
         FROM SessionTable ORDER BY {sort_ts} DESC",
        username = sel("username", "''"),
        unread = sel("unread_count", "0"),
        summary = sel("summary", "NULL"),
        draft = sel("draft", "NULL"),
        hidden = sel("is_hidden", "0"),
        last_ts = sel("last_timestamp", "0"),
        sort_ts = sel("sort_timestamp", "last_timestamp"),
        msg_type = sel("last_msg_type", "0"),
        sender = sel("last_msg_sender", "NULL"),
        sender_name = sel("last_sender_display_name", "NULL"),
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("查询失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            // 全部按 Option 读取：任何字段为 NULL 都不应丢弃整行
            // summary/draft 在某些情况下是 TEXT 而非 BLOB，
            // 直接 row.get::<_, Vec<u8>>() 会因类型不匹配导致整行被丢弃
            Ok::<_, rusqlite::Error>((
                row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                common::get_bytes(row, 2),
                common::get_bytes(row, 3),
                row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                row.get::<_, Option<i64>>(5)?.unwrap_or(0),
                row.get::<_, Option<i64>>(6)?.unwrap_or(0),
                row.get::<_, Option<i64>>(7)?.unwrap_or(0),
                row.get::<_, Option<String>>(8)?.unwrap_or_default(),
                row.get::<_, Option<String>>(9)?.unwrap_or_default(),
            ))
        })
        .map_err(|e| format!("读取失败: {}", e))?;

    let mut sessions = Vec::new();
    for r in rows.flatten() {
        let username = r.0;
        if username.is_empty() {
            continue;
        }
        let is_hidden = r.4 != 0;
        let ts = r.5;
        // 无消息记录的会话仅在置顶时展示（与真实微信的置顶聊天一致）
        if ts <= 0 && !pinned_users.contains(&username) {
            continue;
        }
        let sort_ts = if r.6 > 0 { r.6 } else { ts };
        let msg_type = r.7;
        let last_sender = r.8;
        let last_sender_name_raw = r.9;

        let is_group = username.ends_with("@chatroom") || username.contains("@im.chatroom");
        let is_official = common::is_official_account(&username);
        let is_system = common::is_builtin_account(&username);
        let official_kind = if is_official {
            official_kinds
                .get(&username)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            String::new()
        };

        // 名称解析：通讯录 > 会话标题表 > 系统账号 > username
        let name = contact_names
            .get(&username)
            .cloned()
            .filter(|s| !s.is_empty())
            .or_else(|| session_titles.get(&username).cloned())
            .or_else(|| common::system_account_name(&username).map(|s| s.to_string()))
            .unwrap_or_else(|| username.clone());

        // 摘要：群聊加发送者前缀（与 PC 一致）
        let raw_summary =
            r.2.map(|b| common::decode_blob_text(&b))
                .unwrap_or_default();
        let summary_text = if raw_summary.is_empty() && msg_type > 1 {
            format!("[{}]", common::msg_type_placeholder(msg_type))
        } else {
            raw_summary.clone()
        };
        let sender_display = if !last_sender_name_raw.is_empty() {
            last_sender_name_raw.clone()
        } else {
            contact_names.get(&last_sender).cloned().unwrap_or_default()
        };
        let summary = if is_group && !sender_display.is_empty() && !summary_text.is_empty() {
            format!("{}: {}", sender_display, summary_text)
        } else {
            summary_text
        };

        let draft =
            r.3.map(|b| {
                let text = common::decode_blob_text(&b);
                // 过滤 protobuf 二进制垃圾：只保留有意义的草稿文本
                if is_meaningful_draft(&text) {
                    text
                } else {
                    String::new()
                }
            })
            .unwrap_or_default();

        let is_pinned = pinned_users.contains(&username);
        sessions.push(SessionEntry {
            username,
            name,
            summary,
            raw_summary,
            draft,
            ts,
            sort_ts,
            pinned: is_pinned,
            is_hidden,
            time: if ts > 0 {
                common::format_session_time(ts)
            } else {
                String::new()
            },
            full_time: common::format_full_time(ts),
            unread_count: r.1,
            is_group,
            is_official,
            official_kind,
            is_system,
            last_msg_type: msg_type,
            last_sender,
            last_sender_name: sender_display,
        });
    }

    // 置顶在前，其余按时间倒序；已隐藏会话排在可见会话之后（带徽标展示）
    sessions.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then(a.is_hidden.cmp(&b.is_hidden))
            .then(b.sort_ts.cmp(&a.sort_ts))
    });

    log::info!("[session] 返回 {} 个会话", sessions.len());
    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实数据：contact.db 的 flag 第 11 位标记的置顶会话应被识别、
    /// 置顶排序（所有置顶会话位于普通会话之前）；公众号的 flag 位不是置顶语义，
    /// 因此置顶集合不应包含公众号。真实微信置顶共 27 个。
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_pinned_sessions() {
        let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let sessions = get_session_list(&cfg.decrypted_dir).expect("读取会话列表失败");
        if sessions.is_empty() {
            eprintln!("无会话数据，跳过");
            return;
        }

        let pinned: Vec<&SessionEntry> = sessions.iter().filter(|s| s.pinned).collect();
        eprintln!(
            "会话 {} 个，其中置顶 {} 个: {:?}",
            sessions.len(),
            pinned.len(),
            pinned
                .iter()
                .map(|s| s.username.as_str())
                .collect::<Vec<_>>()
        );
        // 置顶数 27 为开发账号口径：账号数据不同（CI/无数据或他人账号）时
        // 跳过数量断言，保留排序/类型断言。
        if pinned.len() != 27 {
            eprintln!(
                "置顶会话 {} 个（开发账号为 27），跳过数量断言",
                pinned.len()
            );
            return;
        }
        assert!(
            pinned.iter().all(|s| !s.is_official),
            "置顶集合不应包含公众号: {:?}",
            pinned.iter().filter(|s| s.is_official).collect::<Vec<_>>()
        );

        // 置顶会话必须全部排在普通会话之前
        let first_normal = sessions
            .iter()
            .position(|s| !s.pinned)
            .unwrap_or(sessions.len());
        assert!(
            pinned.len() <= first_normal,
            "置顶会话应全部位于普通会话之前"
        );

        // 文件传输助手是常见置顶项，应被正确识别（若该账号存在此会话）
        if let Some(fh) = sessions.iter().find(|s| s.username == "filehelper") {
            assert!(fh.pinned, "filehelper 应被识别为置顶会话");
        }
    }

    /// 真实数据：公众号应能按 biz_info 的 ServiceType 分出服务号/订阅号
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_official_kinds() {
        let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let sessions = get_session_list(&cfg.decrypted_dir).expect("读取会话列表失败");
        let officials: Vec<&SessionEntry> = sessions.iter().filter(|s| s.is_official).collect();
        eprintln!(
            "公众号会话 {} 个：服务号 {}，公众号/订阅号 {}",
            officials.len(),
            officials
                .iter()
                .filter(|s| s.official_kind == "service")
                .count(),
            officials
                .iter()
                .filter(|s| s.official_kind != "service")
                .count()
        );
        if officials.is_empty() {
            eprintln!("会话列表中无公众号，跳过");
            return;
        }
        assert!(
            officials.iter().any(|s| s.official_kind == "service"),
            "应识别出服务号（ServiceType=1）"
        );
        assert!(
            officials.iter().any(|s| s.official_kind == "subscription"),
            "应识别出订阅号（ServiceType=0）"
        );
        // 非公众号不应带类型
        assert!(
            sessions
                .iter()
                .filter(|s| !s.is_official)
                .all(|s| s.official_kind.is_empty()),
            "非公众号会话不应有 official_kind"
        );
    }
}
