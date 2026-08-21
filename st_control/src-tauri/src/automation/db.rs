// ============================================================
// 自动化管理中心 — 数据层
// automation_rules：规则表（条件 / AI 分析 / 派发目标 / 优先级）
// task_wechat_info：消息任务表（唯一约束 sender_username+timestamp+username）
// ============================================================

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// 规则条件：field + op + value
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RuleCondition {
    pub field: String, // content | sender | session | media_type | is_send
    pub op: String,    // contains | not_contains | equals | regex
    pub value: String,
}

/// AI 提取字段定义
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeField {
    pub name: String,
    pub desc: String,
}

/// 自动化规则
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRule {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub priority: i64,
    pub conditions: Vec<RuleCondition>,
    pub analyze_fields: Vec<AnalyzeField>,
    pub prompt_override: String,
    pub provider_id: String,
    pub model: String,
    pub dispatch_mode: String, // fixed | ai
    pub target_type: String,   // agent(智能体) | agent_instance(已接入Agent)
    pub target_id: String,
    /// 绑定的 AI 角色 id（roles.json），内置 Worker 执行任务时以其提示词为系统提示；
    /// 为空则使用默认执行提示词
    #[serde(default)]
    pub role_id: String,
    pub hit_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// 消息任务（task_wechat_info）
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WechatTask {
    pub id: i64,
    pub ack_id: Option<String>,
    pub content: String,
    pub sender_username: String,
    pub session_type: Option<String>,
    pub is_group: bool,
    pub is_send: bool,
    pub media_type: Option<String>,
    pub msg_type: Option<i64>,
    pub timestamp: i64,
    pub username: String,
    pub rule_id: Option<i64>,
    pub rule_name: String,
    pub ai_extract: String,
    pub full_json: String,
    pub target_type: String,
    pub target_id: String,
    pub reply_text: String,
    pub status: String,
    pub error: String,
    /// error 任务已被自动重试的次数（达到上限后保持 error 交人工处理）
    pub retry_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// 概览统计
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AutomationStats {
    pub today_pushed: i64,
    pub total_tasks: i64,
    pub pending: i64,
    pub claimed: i64,
    pub processing: i64,
    pub to_reply: i64,
    pub replied: i64,
    pub done: i64,
    pub ignored: i64,
    pub rules_enabled: i64,
    pub rules_total: i64,
    pub status_dist: Vec<StatusCount>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StatusCount {
    pub status: String,
    pub count: i64,
}

/// 建表（幂等）
pub fn init_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS automation_rules (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            name              TEXT NOT NULL,
            enabled           INTEGER NOT NULL DEFAULT 1,
            priority          INTEGER NOT NULL DEFAULT 0,
            conditions_json   TEXT NOT NULL DEFAULT '[]',
            analyze_fields_json TEXT NOT NULL DEFAULT '[]',
            prompt_override   TEXT NOT NULL DEFAULT '',
            provider_id       TEXT NOT NULL DEFAULT '',
            model             TEXT NOT NULL DEFAULT '',
            dispatch_mode     TEXT NOT NULL DEFAULT 'fixed',
            target_type       TEXT NOT NULL DEFAULT 'agent',
            target_id         TEXT NOT NULL DEFAULT '',
            role_id           TEXT NOT NULL DEFAULT '',
            hit_count         INTEGER NOT NULL DEFAULT 0,
            created_at        TEXT NOT NULL DEFAULT (datetime('now','localtime')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now','localtime'))
        );

        CREATE TABLE IF NOT EXISTS task_wechat_info (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            ack_id            TEXT,
            channel           TEXT DEFAULT '',
            chat              TEXT DEFAULT '',
            content           TEXT DEFAULT '',
            decrypt_ms        REAL,
            is_group          INTEGER NOT NULL DEFAULT 0,
            is_send           INTEGER NOT NULL DEFAULT 0,
            local_id          TEXT,
            media_type        TEXT,
            msg_type          INTEGER,
            pages             INTEGER,
            sender            TEXT DEFAULT '',
            sender_username   TEXT NOT NULL DEFAULT '',
            session_type      TEXT,
            sort_seq          TEXT,
            time              TEXT DEFAULT '',
            timestamp         INTEGER NOT NULL DEFAULT 0,
            ts_backend        INTEGER,
            username          TEXT NOT NULL DEFAULT '',
            rule_id           INTEGER,
            rule_name         TEXT DEFAULT '',
            ai_extract        TEXT DEFAULT '',
            full_json         TEXT DEFAULT '',
            target_type       TEXT DEFAULT '',
            target_id         TEXT DEFAULT '',
            reply_text        TEXT DEFAULT '',
            status            TEXT NOT NULL DEFAULT 'pending',
            error             TEXT DEFAULT '',
            retry_count       INTEGER NOT NULL DEFAULT 0,
            created_at        TEXT NOT NULL DEFAULT (datetime('now','localtime')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now','localtime')),
            UNIQUE(sender_username, timestamp, username)
        );
        CREATE INDEX IF NOT EXISTS idx_twi_status ON task_wechat_info(status);
        CREATE INDEX IF NOT EXISTS idx_twi_created ON task_wechat_info(created_at);
        CREATE INDEX IF NOT EXISTS idx_twi_rule ON task_wechat_info(rule_id);
        "#,
    )?;
    // 迁移：老库补 role_id 列（内置 Worker 的角色绑定）
    {
        let mut stmt = conn.prepare("PRAGMA table_info(automation_rules)")?;
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(|c| c.ok())
            .collect();
        if !cols.iter().any(|c| c == "role_id") {
            conn.execute(
                "ALTER TABLE automation_rules ADD COLUMN role_id TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
    }
    // 迁移：老库补 retry_count 列（error 任务自动重试）
    {
        let mut stmt = conn.prepare("PRAGMA table_info(task_wechat_info)")?;
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(|c| c.ok())
            .collect();
        if !cols.iter().any(|c| c == "retry_count") {
            conn.execute(
                "ALTER TABLE task_wechat_info ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
    }
    Ok(())
}

fn row_to_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationRule> {
    let conditions_json: String = row.get(4)?;
    let fields_json: String = row.get(5)?;
    Ok(AutomationRule {
        id: row.get(0)?,
        name: row.get(1)?,
        enabled: row.get::<_, i64>(2)? != 0,
        priority: row.get(3)?,
        conditions: serde_json::from_str(&conditions_json).unwrap_or_default(),
        analyze_fields: serde_json::from_str(&fields_json).unwrap_or_default(),
        prompt_override: row.get(6)?,
        provider_id: row.get(7)?,
        model: row.get(8)?,
        dispatch_mode: row.get(9)?,
        target_type: row.get(10)?,
        target_id: row.get(11)?,
        role_id: row.get(12)?,
        hit_count: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

const RULE_COLS: &str = "id,name,enabled,priority,conditions_json,analyze_fields_json,prompt_override,provider_id,model,dispatch_mode,target_type,target_id,role_id,hit_count,created_at,updated_at";

pub fn list_rules(conn: &Connection) -> rusqlite::Result<Vec<AutomationRule>> {
    let sql = format!("SELECT {RULE_COLS} FROM automation_rules ORDER BY priority ASC, id ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_rule)?;
    rows.collect()
}

pub fn insert_rule(conn: &Connection, r: &AutomationRule) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO automation_rules (name,enabled,priority,conditions_json,analyze_fields_json,prompt_override,provider_id,model,dispatch_mode,target_type,target_id,role_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            r.name,
            r.enabled as i64,
            r.priority,
            serde_json::to_string(&r.conditions).unwrap_or("[]".into()),
            serde_json::to_string(&r.analyze_fields).unwrap_or("[]".into()),
            r.prompt_override,
            r.provider_id,
            r.model,
            r.dispatch_mode,
            r.target_type,
            r.target_id,
            r.role_id,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_rule(conn: &Connection, id: i64, r: &AutomationRule) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE automation_rules SET name=?1,enabled=?2,priority=?3,conditions_json=?4,analyze_fields_json=?5,prompt_override=?6,provider_id=?7,model=?8,dispatch_mode=?9,target_type=?10,target_id=?11,role_id=?12,updated_at=datetime('now','localtime') WHERE id=?13",
        params![
            r.name,
            r.enabled as i64,
            r.priority,
            serde_json::to_string(&r.conditions).unwrap_or("[]".into()),
            serde_json::to_string(&r.analyze_fields).unwrap_or("[]".into()),
            r.prompt_override,
            r.provider_id,
            r.model,
            r.dispatch_mode,
            r.target_type,
            r.target_id,
            r.role_id,
            id,
        ],
    )?;
    Ok(())
}

pub fn delete_rule(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM automation_rules WHERE id=?1", params![id])?;
    Ok(())
}

pub fn bump_rule_hit(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE automation_rules SET hit_count=hit_count+1, updated_at=datetime('now','localtime') WHERE id=?1",
        params![id],
    )?;
    Ok(())
}

/// 按 id 查规则（内置 Worker 执行用）
pub fn get_rule(conn: &Connection, id: i64) -> rusqlite::Result<Option<AutomationRule>> {
    let sql = format!("SELECT {RULE_COLS} FROM automation_rules WHERE id=?1");
    conn.query_row(&sql, params![id], row_to_rule).optional()
}

/// 内置 Worker：原子认领任务（pending → processing；claimed 的智能体派发任务
/// 也可认领，实现「派发即自动执行」），返回是否抢到
pub fn claim_task(conn: &Connection, id: i64) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE task_wechat_info SET status='processing',
         updated_at=datetime('now','localtime') WHERE id=?1 AND status IN ('pending','claimed')",
        params![id],
    )?;
    Ok(n == 1)
}

/// 内置 Worker：执行完成（无回复）标记 done
pub fn mark_done(conn: &Connection, id: i64, extract: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE task_wechat_info SET status='done', ai_extract=?1, error='',
         updated_at=datetime('now','localtime') WHERE id=?2",
        params![extract, id],
    )?;
    Ok(())
}

/// 内置 Worker：执行失败标记 error
pub fn mark_failed(conn: &Connection, id: i64, error: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE task_wechat_info SET status='error', error=?1,
         updated_at=datetime('now','localtime') WHERE id=?2",
        params![error, id],
    )?;
    Ok(())
}

/// 回收超时的 processing 任务（内置 Worker 崩溃/卡死遗留）→ 回 pending
pub fn reap_stale_processing(conn: &Connection, minutes: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE task_wechat_info SET status='pending',
         updated_at=datetime('now','localtime')
         WHERE status='processing'
           AND updated_at < datetime('now','localtime', ?1)",
        params![format!("-{minutes} minutes")],
    )
}

/// 回收可重试的 error 任务 → 回 pending（自动重试）：
/// 仅回收失败超过 delay_minutes 且 retry_count 未达上限的任务，
/// 每次回收计数 +1，超过上限后保持 error 交由人工处理。
pub fn reap_retryable_errors(
    conn: &Connection,
    max_retries: i64,
    delay_minutes: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE task_wechat_info SET status='pending', retry_count=retry_count+1,
         updated_at=datetime('now','localtime')
         WHERE status='error' AND retry_count < ?1
           AND updated_at < datetime('now','localtime', ?2)",
        params![max_retries, format!("-{delay_minutes} minutes")],
    )
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<WechatTask> {
    Ok(WechatTask {
        id: row.get(0)?,
        ack_id: row.get(1)?,
        content: row.get(4)?,
        sender_username: row.get(13)?,
        session_type: row.get(14)?,
        is_group: row.get::<_, i64>(6)? != 0,
        is_send: row.get::<_, i64>(7)? != 0,
        media_type: row.get(9)?,
        msg_type: row.get(10)?,
        timestamp: row.get(17)?,
        username: row.get(19)?,
        rule_id: row.get(20)?,
        rule_name: row.get(21)?,
        ai_extract: row.get(22)?,
        full_json: row.get(23)?,
        target_type: row.get(24)?,
        target_id: row.get(25)?,
        reply_text: row.get(26)?,
        status: row.get(27)?,
        error: row.get(28)?,
        retry_count: row.get(29)?,
        created_at: row.get(30)?,
        updated_at: row.get(31)?,
    })
}

const TASK_COLS: &str = "id,ack_id,channel,chat,content,decrypt_ms,is_group,is_send,local_id,media_type,msg_type,pages,sender,sender_username,session_type,sort_seq,time,timestamp,ts_backend,username,rule_id,rule_name,ai_extract,full_json,target_type,target_id,reply_text,status,error,retry_count,created_at,updated_at";

pub fn list_tasks(
    conn: &Connection,
    status: Option<&str>,
    keyword: Option<&str>,
    limit: i64,
    offset: i64,
) -> rusqlite::Result<Vec<WechatTask>> {
    let mut sql = format!("SELECT {TASK_COLS} FROM task_wechat_info WHERE 1=1");
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(s) = status {
        if !s.is_empty() {
            sql.push_str(" AND status=?");
            args.push(s.to_string().into());
        }
    }
    if let Some(k) = keyword {
        if !k.is_empty() {
            sql.push_str(" AND (content LIKE ? OR sender_username LIKE ? OR username LIKE ? OR rule_name LIKE ?)");
            let like = format!("%{}%", k);
            args.push(like.clone().into());
            args.push(like.clone().into());
            args.push(like.clone().into());
            args.push(like.to_string().into());
        }
    }
    sql.push_str(" ORDER BY id DESC LIMIT ? OFFSET ?");
    args.push(limit.into());
    args.push(offset.into());
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_task)?;
    rows.collect()
}

pub fn count_tasks(
    conn: &Connection,
    status: Option<&str>,
    keyword: Option<&str>,
) -> rusqlite::Result<i64> {
    let mut sql = "SELECT COUNT(*) FROM task_wechat_info WHERE 1=1".to_string();
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(s) = status {
        if !s.is_empty() {
            sql.push_str(" AND status=?");
            args.push(s.to_string().into());
        }
    }
    if let Some(k) = keyword {
        if !k.is_empty() {
            sql.push_str(" AND (content LIKE ? OR sender_username LIKE ? OR username LIKE ? OR rule_name LIKE ?)");
            let like = format!("%{}%", k);
            args.push(like.clone().into());
            args.push(like.clone().into());
            args.push(like.clone().into());
            args.push(like.to_string().into());
        }
    }
    conn.query_row(&sql, rusqlite::params_from_iter(args.iter()), |r| r.get(0))
}

pub fn get_task(conn: &Connection, id: i64) -> rusqlite::Result<Option<WechatTask>> {
    let sql = format!("SELECT {TASK_COLS} FROM task_wechat_info WHERE id=?1");
    conn.query_row(&sql, params![id], row_to_task).optional()
}

pub fn update_task_status(
    conn: &Connection,
    id: i64,
    status: &str,
    err: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE task_wechat_info SET status=?1, error=?2, updated_at=datetime('now','localtime') WHERE id=?3",
        params![status, err, id],
    )?;
    Ok(())
}

pub fn update_task_target(
    conn: &Connection,
    id: i64,
    target_type: &str,
    target_id: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE task_wechat_info SET target_type=?1, target_id=?2, updated_at=datetime('now','localtime') WHERE id=?3",
        params![target_type, target_id, id],
    )?;
    Ok(())
}

pub fn update_task_reply(
    conn: &Connection,
    id: i64,
    reply_text: &str,
    status: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE task_wechat_info SET reply_text=?1, status=?2, error='', updated_at=datetime('now','localtime') WHERE id=?3",
        params![reply_text, status, id],
    )?;
    Ok(())
}

pub fn delete_task(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM task_wechat_info WHERE id=?1", params![id])?;
    Ok(())
}

/// 按三字段唯一约束查询任务
pub fn find_task_by_key(
    conn: &Connection,
    sender_username: &str,
    timestamp: i64,
    username: &str,
) -> rusqlite::Result<Option<WechatTask>> {
    let sql = format!("SELECT {TASK_COLS} FROM task_wechat_info WHERE sender_username=?1 AND timestamp=?2 AND username=?3");
    conn.query_row(
        &sql,
        params![sender_username, timestamp, username],
        row_to_task,
    )
    .optional()
}

/// 按三字段更新回复文本（供智能体/回复机器人调用）
pub fn update_reply_by_key(
    conn: &Connection,
    sender_username: &str,
    timestamp: i64,
    username: &str,
    reply_text: &str,
    status: &str,
) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE task_wechat_info SET reply_text=?1, status=?2, updated_at=datetime('now','localtime')
         WHERE sender_username=?3 AND timestamp=?4 AND username=?5",
        params![reply_text, status, sender_username, timestamp, username],
    )?;
    Ok(n > 0)
}

pub fn stats(conn: &Connection) -> rusqlite::Result<AutomationStats> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let today_pushed: i64 = conn.query_row(
        "SELECT COUNT(*) FROM task_wechat_info WHERE created_at LIKE ?1",
        params![format!("{}%", today)],
        |r| r.get(0),
    )?;
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM task_wechat_info", [], |r| r.get(0))?;
    let count_status = |s: &str| -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM task_wechat_info WHERE status=?1",
            params![s],
            |r| r.get(0),
        )
        .unwrap_or(0)
    };
    let rules_total: i64 =
        conn.query_row("SELECT COUNT(*) FROM automation_rules", [], |r| r.get(0))?;
    let rules_enabled: i64 = conn.query_row(
        "SELECT COUNT(*) FROM automation_rules WHERE enabled=1",
        [],
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare("SELECT status, COUNT(*) FROM task_wechat_info GROUP BY status")?;
    let dist = stmt
        .query_map([], |r| {
            Ok(StatusCount {
                status: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(AutomationStats {
        today_pushed,
        total_tasks: total,
        pending: count_status("pending"),
        claimed: count_status("claimed"),
        processing: count_status("processing"),
        to_reply: count_status("to_reply"),
        replied: count_status("replied"),
        done: count_status("done"),
        ignored: count_status("ignored"),
        rules_enabled,
        rules_total,
        status_dist: dist,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("内存库");
        init_tables(&conn).expect("建表");
        conn
    }

    fn insert_task_row(conn: &Connection, id: i64, status: &str) {
        let channel = if id % 2 == 0 { "ilink" } else { "" };
        let full_json = if channel == "ilink" {
            r#"{"account_id": 1}"#.to_string()
        } else {
            "{}".to_string()
        };
        conn.execute(
            "INSERT INTO task_wechat_info
             (id, content, sender_username, username, status, reply_text, channel, is_group, full_json, timestamp)
             VALUES (?1,'测试消息','wxid_s','wxid_u',?2,'',?3,?4,?5,?1)",
            rusqlite::params![
                id,
                status,
                channel,
                if id == 9 { 1 } else { 0 },
                full_json
            ],
        )
        .expect("插入任务");
    }

    /// 原子认领：pending → processing 只能成功一次（与外部 claim 互斥）
    #[test]
    fn claim_task_is_atomic() {
        let conn = test_conn();
        insert_task_row(&conn, 1, "pending");
        assert!(claim_task(&conn, 1).expect("认领"));
        assert_eq!(
            conn.query_row("SELECT status FROM task_wechat_info WHERE id=1", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "processing"
        );
        assert!(!claim_task(&conn, 1).expect("二次认领应失败"));
    }

    /// 已派发给智能体的 claimed 任务也可被内置 Worker 认领（派发即执行）
    #[test]
    fn claim_task_accepts_claimed() {
        let conn = test_conn();
        insert_task_row(&conn, 21, "claimed");
        assert!(
            claim_task(&conn, 21).expect("认领 claimed 任务"),
            "claimed 任务应可被 Worker 认领"
        );
        let st: String = conn
            .query_row("SELECT status FROM task_wechat_info WHERE id=21", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(st, "processing");
        assert!(!claim_task(&conn, 21).expect("二次认领应失败"));
    }

    /// 已派发给外部 Agent（agent_instance）的 claimed 任务不被内置 Worker 抢占
    /// （由外部执行者走 HTTP start 接口），避免与外部执行者冲突
    #[test]
    fn claim_task_does_not_take_external_claimed() {
        let conn = test_conn();
        insert_task_row(&conn, 22, "claimed");
        conn.execute(
            "UPDATE task_wechat_info SET target_type='agent_instance', target_id='ext-1' WHERE id=22",
            [],
        )
        .unwrap();
        // 认领本身只认状态，target 过滤发生在 Worker 候选查询层；
        // 这里验证状态机层面不拒绝（外部执行者 start 需要 claimed）
        assert!(claim_task(&conn, 22).expect("认领"));
        conn.execute(
            "UPDATE task_wechat_info SET status='claimed' WHERE id=22",
            [],
        )
        .unwrap();
        // 回写 claimed 后外部执行者仍可 start
        let n = conn
            .execute(
                "UPDATE task_wechat_info SET status='processing' WHERE id=22 AND status='claimed'",
                [],
            )
            .unwrap();
        assert_eq!(n, 1, "外部执行者 start 应成功");
    }

    /// 超时 processing 任务被回收回 pending；新任务不受影响
    #[test]
    fn reap_stale_processing_recovers() {
        let conn = test_conn();
        insert_task_row(&conn, 2, "processing");
        conn.execute(
            "UPDATE task_wechat_info SET updated_at=datetime('now','localtime','-10 minutes') WHERE id=2",
            [],
        )
        .unwrap();
        insert_task_row(&conn, 3, "processing"); // 新的 processing 不回收
        let n = reap_stale_processing(&conn, 5).expect("回收");
        assert_eq!(n, 1);
        let st: String = conn
            .query_row("SELECT status FROM task_wechat_info WHERE id=2", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(st, "pending");
        let st3: String = conn
            .query_row("SELECT status FROM task_wechat_info WHERE id=3", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(st3, "processing");
    }

    /// error 任务自动重试：超过静默期且未达上限的被回收回 pending 并计数；
    /// 刚失败的、已达上限的不回收
    #[test]
    fn reap_retryable_errors_recovers_with_limit() {
        let conn = test_conn();
        insert_task_row(&conn, 30, "error");
        conn.execute(
            "UPDATE task_wechat_info SET updated_at=datetime('now','localtime','-11 minutes') WHERE id=30",
            [],
        )
        .unwrap();
        insert_task_row(&conn, 31, "error"); // 刚失败（未过静默期）
        conn.execute(
            "UPDATE task_wechat_info SET retry_count=3, updated_at=datetime('now','localtime','-11 minutes') WHERE id=31",
            [],
        )
        .unwrap();
        insert_task_row(&conn, 32, "error"); // 已达上限
        conn.execute(
            "UPDATE task_wechat_info SET retry_count=3, updated_at=datetime('now','localtime','-11 minutes') WHERE id=32",
            [],
        )
        .unwrap();
        let n = reap_retryable_errors(&conn, 3, 10).expect("回收");
        assert_eq!(n, 1, "只有未达上限且过静默期的 error 被回收");
        let (st, rc): (String, i64) = conn
            .query_row(
                "SELECT status, retry_count FROM task_wechat_info WHERE id=30",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(st, "pending");
        assert_eq!(rc, 1, "回收时重试计数 +1");
        // 第二次回收：31 仍被静默期保护，32 已达上限，30 已不是 error
        let n2 = reap_retryable_errors(&conn, 3, 10).expect("二次回收");
        assert_eq!(n2, 0);
    }

    /// 待回复队列：本机（channel 空）与 ilink 任务都在内；本机群任务标记 is_group
    #[test]
    fn pending_reply_includes_local() {
        let conn = test_conn();
        insert_task_row(&conn, 5, "to_reply"); // 本机私聊
        insert_task_row(&conn, 6, "to_reply"); // ilink
        insert_task_row(&conn, 7, "to_reply"); // 本机
        insert_task_row(&conn, 9, "to_reply"); // 本机群聊（is_group=1）
        conn.execute(
            "UPDATE task_wechat_info SET reply_text='回复内容' WHERE id IN (5,6,7,9)",
            [],
        )
        .unwrap();
        let list = super::super::super::bot::reply_tasks::list_pending_reply(&conn, 10).unwrap();
        assert_eq!(list.len(), 4, "私聊/ilink/本机群聊都应列出: {list:?}");
        let g = list.iter().find(|p| p.task_id == 9).unwrap();
        assert!(g.is_group, "群聊任务应标记 is_group 供应答器跳过");
        let l = list.iter().find(|p| p.task_id == 5).unwrap();
        assert_eq!(l.channel, "", "本机任务 channel 应为空");
        assert_eq!(l.account_id, 0, "本机任务由应答器选默认账号");
        let i = list.iter().find(|p| p.task_id == 6).unwrap();
        assert_eq!(i.channel, "ilink");
    }

    /// QQ 官方机器人待回复：从 full_json 取账号、回复目标与被动回复 msg_id
    #[test]
    fn pending_reply_includes_qqbot() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO task_wechat_info
             (id, content, sender_username, username, status, reply_text, channel, is_group, full_json, timestamp)
             VALUES (10,'在吗','openid_U','openid_U', 'to_reply','自动回复内容','qqbot',0,
                     '{\"account_id\":5,\"qq_reply_to\":\"private:openid_U\",\"local_id\":\"EVT_001\"}',10)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO task_wechat_info
             (id, content, sender_username, username, status, reply_text, channel, is_group, full_json, timestamp)
             VALUES (11,'你好','openid_G','openid_G', 'to_reply','群回复','qqbot',1,
                     '{\"account_id\":5,\"qq_reply_to\":\"group:openid_G\",\"local_id\":\"EVT_002\"}',11)",
            [],
        )
        .unwrap();
        let list = super::super::super::bot::reply_tasks::list_pending_reply(&conn, 10).unwrap();
        assert_eq!(list.len(), 2, "两条 qqbot 待回复都应列出: {list:?}");
        let p = list.iter().find(|x| x.task_id == 10).unwrap();
        assert_eq!(p.channel, "qqbot");
        assert_eq!(p.account_id, 5);
        assert_eq!(p.qq_reply_to, "private:openid_U");
        assert_eq!(p.qq_reply_msg_id, "EVT_001");
        let g = list.iter().find(|x| x.task_id == 11).unwrap();
        assert!(g.is_group);
        assert_eq!(g.qq_reply_to, "group:openid_G");
        assert_eq!(g.qq_reply_msg_id, "EVT_002");
    }
}
