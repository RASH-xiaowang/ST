// ============================================================
// 年度总结 — 消息库扫描
// 自 annual.rs 拆分：分片库/表枚举、会话与显示名加载、blob 文本读取。
// ============================================================

use rusqlite::{Connection, Row};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::wechat::modules::common;

// ─── 消息库扫描 ───

pub(crate) fn list_shard_dbs(decrypted_dir: &Path) -> Vec<PathBuf> {
    let mut dbs = common::find_db_files(decrypted_dir, "message_");
    dbs.extend(common::find_db_files(decrypted_dir, "biz_message_"));
    dbs.sort();
    dbs.dedup();
    dbs.retain(|p| common::is_message_shard_file(p));
    dbs
}

pub(crate) fn list_msg_tables(conn: &Connection) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'Msg\\_%' ESCAPE '\\'",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for r in rows.flatten() {
                if r.len() > 4 {
                    out.push(r);
                }
            }
        }
    }
    out.sort();
    out
}

/// 从 session.db 读取全部会话 username（失败返回空表）
pub(crate) fn load_session_usernames(decrypted_dir: &Path) -> Vec<String> {
    let db_path = decrypted_dir.join("session").join("session.db");
    if !db_path.exists() {
        return Vec::new();
    }
    let conn = match common::open_readonly_db(&db_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let table = if common::table_exists(&conn, "SessionTable") {
        "SessionTable"
    } else if common::table_exists(&conn, "Session") {
        "Session"
    } else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&format!("SELECT username FROM \"{}\"", table)) {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for r in rows.flatten() {
                let u = r.trim().to_string();
                if !u.is_empty() {
                    out.push(u);
                }
            }
        }
    }
    out
}

/// 会话显示名（联系人备注/昵称 > SessionNoContactInfoTable > username）
pub(crate) fn load_display_names(
    decrypted_dir: &Path,
    usernames: &[String],
) -> HashMap<String, String> {
    let mut names = HashMap::new();
    let contact_db = decrypted_dir.join("contact").join("contact.db");
    if contact_db.exists() {
        names.extend(crate::wechat::modules::contacts::load_display_names(
            &contact_db,
        ));
    }
    // 无联系人信息的会话标题（群名/服务号名）
    let session_db = decrypted_dir.join("session").join("session.db");
    if session_db.exists() {
        if let Ok(conn) = common::open_readonly_db(&session_db) {
            if common::table_exists(&conn, "SessionNoContactInfoTable") {
                if let Ok(mut stmt) =
                    conn.prepare("SELECT username, session_title FROM SessionNoContactInfoTable")
                {
                    if let Ok(rows) =
                        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                    {
                        for r in rows.flatten() {
                            names.entry(r.0).or_insert(r.1);
                        }
                    }
                }
            }
        }
    }
    for u in usernames {
        names.entry(u.clone()).or_insert_with(|| u.clone());
    }
    names
}

pub(crate) fn read_text(row: &Row<'_>, idx: usize) -> String {
    match common::get_bytes(row, idx) {
        Some(b) => common::decode_blob_text(&b),
        None => String::new(),
    }
}
