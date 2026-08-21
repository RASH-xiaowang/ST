// ============================================================
// 消息通道 — 数据层
// bot_accounts：ClawBot 账号（token 加密存储、24h 有效期、游标）
// bot_logs：收发消息日志
// ============================================================

use crate::wechat::modules::common::table_columns;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

/// ClawBot 账号
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BotAccount {
    pub id: i64,
    pub bot_id: String,
    pub name: String,
    pub owner_id: String,
    #[serde(skip)]
    pub token_enc: String,
    pub base_url: String,
    pub cdn_base_url: String,
    /// 通道平台：wechat | qqbot
    pub platform: String,
    /// 默认推送目标（qqbot 填 openid；其余群机器人通道已移除）
    pub target_id: String,
    pub status: String, // connecting | online | expiring | expired | error | disabled
    pub connected_at: Option<String>,
    pub expires_at: Option<String>,
    pub last_active_at: Option<String>,
    pub last_error: String,
    pub sync_buf: String,
    pub context_tokens_json: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 消息日志
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BotLog {
    pub id: i64,
    pub account_id: i64,
    pub direction: String, // in | out
    pub msg_type: String,  // text | image | voice | file | video
    pub peer: String,
    pub content: String,
    pub local_path: String,
    pub status: String, // ok | failed | pending
    pub error: String,
    pub created_at: String,
}

pub const DEFAULT_CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

/// 建表（幂等）
pub fn init_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS bot_accounts (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            bot_id              TEXT NOT NULL DEFAULT '',
            name                TEXT NOT NULL DEFAULT '',
            owner_id            TEXT NOT NULL DEFAULT '',
            platform            TEXT NOT NULL DEFAULT 'wechat',
            target_id           TEXT NOT NULL DEFAULT '',
            token_enc           TEXT NOT NULL DEFAULT '',
            base_url            TEXT NOT NULL DEFAULT '',
            cdn_base_url        TEXT NOT NULL DEFAULT 'https://novac2c.cdn.weixin.qq.com/c2c',
            status              TEXT NOT NULL DEFAULT 'disabled',
            connected_at        TEXT,
            expires_at          TEXT,
            last_active_at      TEXT,
            last_error          TEXT NOT NULL DEFAULT '',
            sync_buf            TEXT NOT NULL DEFAULT '',
            context_tokens_json TEXT NOT NULL DEFAULT '{}',
            created_at          TEXT NOT NULL DEFAULT (datetime('now','localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now','localtime'))
        );

        CREATE TABLE IF NOT EXISTS bot_logs (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id  INTEGER NOT NULL,
            direction   TEXT NOT NULL DEFAULT 'in',
            msg_type    TEXT NOT NULL DEFAULT 'text',
            peer        TEXT NOT NULL DEFAULT '',
            content     TEXT NOT NULL DEFAULT '',
            local_path  TEXT NOT NULL DEFAULT '',
            status      TEXT NOT NULL DEFAULT 'ok',
            error       TEXT NOT NULL DEFAULT '',
            created_at  TEXT NOT NULL DEFAULT (datetime('now','localtime'))
        );
        CREATE INDEX IF NOT EXISTS idx_bot_logs_account ON bot_logs(account_id, id);

        -- QQ 官方机器人：从网关事件自动收集的 openid（用户/群），
        -- 官方后台无法检索 openid，这里让前端可直接选择发送目标
        -- last_event_id：最近一次消息事件 id——群消息无主动权限时，
        -- 5 分钟窗口内用它被动回复（官方 40034105 的限制绕过）
        CREATE TABLE IF NOT EXISTS qqbot_contacts (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id    INTEGER NOT NULL,
            kind          TEXT NOT NULL DEFAULT 'private', -- private | group
            openid        TEXT NOT NULL,
            display       TEXT NOT NULL DEFAULT '',
            last_content  TEXT NOT NULL DEFAULT '',
            last_event_id TEXT NOT NULL DEFAULT '',
            last_seen_at  TEXT NOT NULL DEFAULT (datetime('now','localtime')),
            UNIQUE(account_id, kind, openid)
        );
        CREATE INDEX IF NOT EXISTS idx_qqbot_contacts_account ON qqbot_contacts(account_id, last_seen_at DESC);
        "#,
    )
}

// 兼容旧库：逐列检查、逐条补齐（不能放在同一个 batch 里——
// 老库若已有 owner_id，第一条 ALTER 报“重复列”会让整批失败，
// 导致 platform / target_id 永远补不上）
pub fn migrate(conn: &Connection) {
    let existing = table_columns(conn, "bot_accounts");
    for (col, ddl) in [
        (
            "owner_id",
            "ALTER TABLE bot_accounts ADD COLUMN owner_id TEXT DEFAULT ''",
        ),
        (
            "platform",
            "ALTER TABLE bot_accounts ADD COLUMN platform TEXT DEFAULT 'wechat'",
        ),
        (
            "target_id",
            "ALTER TABLE bot_accounts ADD COLUMN target_id TEXT DEFAULT ''",
        ),
    ] {
        if !existing.iter().any(|c| c == col) {
            conn.execute_batch(ddl).ok();
        }
    }
    // qqbot_contacts 旧库补 last_event_id（被动回复用）
    let contact_cols = table_columns(conn, "qqbot_contacts");
    if !contact_cols.iter().any(|c| c == "last_event_id") {
        conn.execute_batch("ALTER TABLE qqbot_contacts ADD COLUMN last_event_id TEXT DEFAULT ''")
            .ok();
    }
}

const ACCOUNT_COLS: &str = "id,bot_id,name,owner_id,token_enc,base_url,cdn_base_url,platform,target_id,status,connected_at,expires_at,last_active_at,last_error,sync_buf,context_tokens_json,created_at,updated_at";

fn row_to_account(row: &rusqlite::Row<'_>) -> rusqlite::Result<BotAccount> {
    Ok(BotAccount {
        id: row.get(0)?,
        bot_id: row.get(1)?,
        name: row.get(2)?,
        owner_id: row.get(3)?,
        token_enc: row.get(4)?,
        base_url: row.get(5)?,
        cdn_base_url: row.get(6)?,
        platform: row.get(7)?,
        target_id: row.get(8)?,
        status: row.get(9)?,
        connected_at: row.get(10)?,
        expires_at: row.get(11)?,
        last_active_at: row.get(12)?,
        last_error: row.get(13)?,
        sync_buf: row.get(14)?,
        context_tokens_json: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

pub fn list_accounts(conn: &Connection) -> rusqlite::Result<Vec<BotAccount>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ACCOUNT_COLS} FROM bot_accounts ORDER BY id DESC"
    ))?;
    let rows = stmt.query_map([], row_to_account)?;
    rows.collect()
}

pub fn get_account(conn: &Connection, id: i64) -> rusqlite::Result<Option<BotAccount>> {
    conn.query_row(
        &format!("SELECT {ACCOUNT_COLS} FROM bot_accounts WHERE id=?1"),
        params![id],
        row_to_account,
    )
    .optional()
}

pub fn insert_account(conn: &Connection, acc: &BotAccount) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO bot_accounts
         (bot_id, name, owner_id, token_enc, base_url, cdn_base_url, platform, target_id, status, connected_at, expires_at,
          last_active_at, last_error, sync_buf, context_tokens_json)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        params![
            acc.bot_id,
            acc.name,
            acc.owner_id,
            acc.token_enc,
            acc.base_url,
            acc.cdn_base_url,
            acc.platform,
            acc.target_id,
            acc.status,
            acc.connected_at,
            acc.expires_at,
            acc.last_active_at,
            acc.last_error,
            acc.sync_buf,
            acc.context_tokens_json,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_account(conn: &Connection, acc: &BotAccount) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE bot_accounts SET bot_id=?1, name=?2, owner_id=?3, token_enc=?4, base_url=?5, cdn_base_url=?6,
         platform=?7, target_id=?8, status=?9, connected_at=?10, expires_at=?11, last_active_at=?12, last_error=?13,
         sync_buf=?14, context_tokens_json=?15, updated_at=datetime('now','localtime')
         WHERE id=?16",
        params![
            acc.bot_id,
            acc.name,
            acc.owner_id,
            acc.token_enc,
            acc.base_url,
            acc.cdn_base_url,
            acc.platform,
            acc.target_id,
            acc.status,
            acc.connected_at,
            acc.expires_at,
            acc.last_active_at,
            acc.last_error,
            acc.sync_buf,
            acc.context_tokens_json,
            acc.id,
        ],
    )?;
    Ok(())
}

pub fn patch_account(
    conn: &Connection,
    id: i64,
    status: &str,
    last_error: &str,
    sync_buf: Option<&str>,
    last_active_at: Option<&str>,
    context_tokens_json: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE bot_accounts SET status=?1, last_error=?2,
         sync_buf=CASE WHEN ?3 IS NULL THEN sync_buf ELSE ?3 END,
         last_active_at=CASE WHEN ?4 IS NULL THEN last_active_at ELSE ?4 END,
         context_tokens_json=CASE WHEN ?5 IS NULL THEN context_tokens_json ELSE ?5 END,
         updated_at=datetime('now','localtime')
         WHERE id=?6",
        params![
            status,
            last_error,
            sync_buf,
            last_active_at,
            context_tokens_json,
            id
        ],
    )?;
    Ok(())
}

pub fn delete_account(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM bot_logs WHERE account_id=?1", params![id])?;
    conn.execute("DELETE FROM bot_accounts WHERE id=?1", params![id])?;
    Ok(())
}

/// 日志条目（bot_logs 一行）
pub struct LogEntry<'a> {
    pub account_id: i64,
    pub direction: &'a str,
    pub msg_type: &'a str,
    pub peer: &'a str,
    pub content: &'a str,
    pub local_path: &'a str,
    pub status: &'a str,
    pub error: &'a str,
}

pub fn insert_log(conn: &Connection, log: &LogEntry<'_>) -> rusqlite::Result<i64> {
    let account_id = log.account_id;
    let direction = log.direction;
    let msg_type = log.msg_type;
    let peer = log.peer;
    let content = log.content;
    let local_path = log.local_path;
    let status = log.status;
    let error = log.error;
    conn.execute(
        "INSERT INTO bot_logs (account_id, direction, msg_type, peer, content, local_path, status, error)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![account_id, direction, msg_type, peer, content, local_path, status, error],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_logs(
    conn: &Connection,
    account_id: i64,
    page: i64,
    page_size: i64,
) -> rusqlite::Result<(Vec<BotLog>, i64)> {
    let limit = page_size.clamp(1, 200);
    let offset = (page.max(1) - 1) * limit;
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM bot_logs WHERE account_id=?1",
        params![account_id],
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT id,account_id,direction,msg_type,peer,content,local_path,status,error,created_at
         FROM bot_logs WHERE account_id=?1 ORDER BY id DESC LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt.query_map(params![account_id, limit, offset], |row| {
        Ok(BotLog {
            id: row.get(0)?,
            account_id: row.get(1)?,
            direction: row.get(2)?,
            msg_type: row.get(3)?,
            peer: row.get(4)?,
            content: row.get(5)?,
            local_path: row.get(6)?,
            status: row.get(7)?,
            error: row.get(8)?,
            created_at: row.get(9)?,
        })
    })?;
    let items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok((items, total))
}

/// QQ 官方机器人 openid 联系人
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QqbotContact {
    pub id: i64,
    pub account_id: i64,
    pub kind: String, // private | group
    pub openid: String,
    pub display: String,
    pub last_content: String,
    /// 最近一次消息事件 id（群消息被动回复窗口内使用）
    pub last_event_id: String,
    pub last_seen_at: String,
}

fn row_to_qqbot_contact(row: &rusqlite::Row<'_>) -> rusqlite::Result<QqbotContact> {
    Ok(QqbotContact {
        id: row.get(0)?,
        account_id: row.get(1)?,
        kind: row.get(2)?,
        openid: row.get(3)?,
        display: row.get(4)?,
        last_content: row.get(5)?,
        last_event_id: row.get(6)?,
        last_seen_at: row.get(7)?,
    })
}

/// 记录网关收到的消息目标（用户 openid / 群 openid），
/// 已存在则更新最后一条内容、事件 id 与时间。返回是否新插入。
/// event_id 传空表示没有可用于被动回复的事件（如群内发言人的私聊条目）。
pub fn upsert_qqbot_contact(
    conn: &Connection,
    account_id: i64,
    kind: &str,
    openid: &str,
    display: &str,
    content: &str,
    event_id: &str,
) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE qqbot_contacts SET
            display = CASE WHEN ?4 = '' THEN display ELSE ?4 END,
            last_content = ?5,
            last_event_id = CASE WHEN ?6 = '' THEN last_event_id ELSE ?6 END,
            last_seen_at = datetime('now','localtime')
         WHERE account_id = ?1 AND kind = ?2 AND openid = ?3",
        params![account_id, kind, openid, display, content, event_id],
    )?;
    if n == 0 {
        conn.execute(
            "INSERT INTO qqbot_contacts (account_id, kind, openid, display, last_content, last_event_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![account_id, kind, openid, display, content, event_id],
        )?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// 查询单个已收集目标（群被动回复时取最近事件 id）
pub fn get_qqbot_contact(
    conn: &Connection,
    account_id: i64,
    kind: &str,
    openid: &str,
) -> rusqlite::Result<Option<QqbotContact>> {
    conn.query_row(
        "SELECT id, account_id, kind, openid, display, last_content, last_event_id, last_seen_at
         FROM qqbot_contacts WHERE account_id = ?1 AND kind = ?2 AND openid = ?3",
        params![account_id, kind, openid],
        row_to_qqbot_contact,
    )
    .optional()
}

/// 某个 qqbot 账号最近收集到的 openid 列表（最新在前）
pub fn list_qqbot_contacts(
    conn: &Connection,
    account_id: i64,
    limit: i64,
) -> rusqlite::Result<Vec<QqbotContact>> {
    let mut stmt = conn.prepare(
        "SELECT id, account_id, kind, openid, display, last_content, last_event_id, last_seen_at
         FROM qqbot_contacts WHERE account_id = ?1
         ORDER BY last_seen_at DESC, id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![account_id, limit], row_to_qqbot_contact)?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 模拟老库：只有 owner_id、没有 platform/target_id，迁移后补齐且幂等
    #[test]
    fn migrate_adds_missing_columns_idempotently() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE bot_accounts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                bot_id TEXT NOT NULL DEFAULT '',
                name TEXT NOT NULL DEFAULT '',
                owner_id TEXT NOT NULL DEFAULT '',
                token_enc TEXT NOT NULL DEFAULT '',
                base_url TEXT NOT NULL DEFAULT '',
                cdn_base_url TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'disabled',
                connected_at TEXT,
                expires_at TEXT,
                last_active_at TEXT,
                last_error TEXT NOT NULL DEFAULT '',
                sync_buf TEXT NOT NULL DEFAULT '',
                context_tokens_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
            );",
        )
        .unwrap();

        migrate(&conn);
        let cols = table_columns(&conn, "bot_accounts");
        assert!(
            cols.iter().any(|c| c == "platform"),
            "platform 未补齐: {cols:?}"
        );
        assert!(
            cols.iter().any(|c| c == "target_id"),
            "target_id 未补齐: {cols:?}"
        );

        // 迁移后能正常插入/读取新列
        let acc = BotAccount {
            id: 0,
            bot_id: "qqbot".into(),
            name: "测试".into(),
            owner_id: String::new(),
            token_enc: "enc".into(),
            base_url: String::new(),
            cdn_base_url: DEFAULT_CDN_BASE_URL.into(),
            platform: "qqbot".into(),
            target_id: "111".into(),
            status: "online".into(),
            connected_at: None,
            expires_at: None,
            last_active_at: None,
            last_error: String::new(),
            sync_buf: String::new(),
            context_tokens_json: "{}".into(),
            created_at: "2026-08-12 00:00:00".into(),
            updated_at: "2026-08-12 00:00:00".into(),
        };
        let id = insert_account(&conn, &acc).unwrap();
        let read = get_account(&conn, id).unwrap().unwrap();
        assert_eq!(read.platform, "qqbot");
        assert_eq!(read.target_id, "111");

        // 再次迁移不报错、不重复加列
        migrate(&conn);
        assert_eq!(
            table_columns(&conn, "bot_accounts")
                .iter()
                .filter(|c| c.as_str() == "platform")
                .count(),
            1
        );
    }

    /// qqbot_contacts：新 openid 插入、重复更新内容、列表按最近时间排序
    #[test]
    fn qqbot_contacts_upsert_and_list() {
        let conn = Connection::open_in_memory().unwrap();
        init_tables(&conn).unwrap();

        assert!(upsert_qqbot_contact(&conn, 1, "private", "USER_A", "", "你好", "EVT_1").unwrap());
        assert!(
            !upsert_qqbot_contact(&conn, 1, "private", "USER_A", "", "第二条", "EVT_2").unwrap()
        );
        assert!(
            upsert_qqbot_contact(&conn, 1, "group", "GROUP_G", "USER_A", "at 消息", "EVT_G")
                .unwrap()
        );
        // 同一 openid 在不同 kind 下互不影响（用户 A 与群 G 可并存）
        assert!(upsert_qqbot_contact(&conn, 2, "private", "USER_A", "", "别的账号", "").unwrap());

        let list = list_qqbot_contacts(&conn, 1, 10).unwrap();
        assert_eq!(list.len(), 2);
        // 最近更新的在前
        assert_eq!(list[0].openid, "GROUP_G");
        assert_eq!(list[0].last_content, "at 消息");
        assert_eq!(list[0].last_event_id, "EVT_G");
        assert_eq!(list[1].openid, "USER_A");
        assert_eq!(list[1].last_content, "第二条");
        assert_eq!(list[1].last_event_id, "EVT_2");

        let other = list_qqbot_contacts(&conn, 2, 10).unwrap();
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].openid, "USER_A");
        // 空事件 id 不覆盖已有值（群内发言人的私聊条目）
        let _ = upsert_qqbot_contact(&conn, 1, "private", "USER_A", "", "第三条", "");
        let row = get_qqbot_contact(&conn, 1, "private", "USER_A")
            .unwrap()
            .unwrap();
        assert_eq!(row.last_event_id, "EVT_2");
        assert_eq!(row.last_content, "第三条");
    }
}
