use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Result as SqlResult};
use std::path::PathBuf;

/// 表浏览/查询共用类型与实现（与外部库共用同一引擎）
pub use crate::sql_browse::{ColumnInfo, TableData};

/// 数据库管理器 — 单例模式（连接池可克隆：供 Harness 运行时服务持有）
#[derive(Clone)]
pub struct Database {
    pool: Pool<SqliteConnectionManager>,
}

impl Database {
    /// 初始化并打开/创建数据库文件
    pub fn new() -> Result<Self, String> {
        let db_path = Self::db_path();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let manager = SqliteConnectionManager::file(&db_path).with_init(|conn| {
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
        });
        let pool = Pool::builder()
            .max_size(8)
            .min_idle(Some(1))
            .connection_timeout(std::time::Duration::from_secs(5))
            .build(manager)
            .map_err(|e| format!("创建数据库连接池失败: {}", e))?;
        let db = Database { pool };
        db.init_tables().map_err(|e| e.to_string())?;
        log::info!("数据库已初始化: {}", db_path.display());
        Ok(db)
    }

    /// 数据库文件路径。测试构建（cfg!(test)）使用按进程隔离的临时文件，
    /// 杜绝单测/集成测试写入真实 data/control.db（历史教训：测试创建
    /// 的会话曾泄漏进真实库、测试失败 panic 时清理代码不执行更会残留）。
    fn db_path() -> PathBuf {
        if cfg!(test) {
            return std::env::temp_dir().join(format!("st-control-test-{}.db", std::process::id()));
        }
        crate::common::st_data_dir().join("control.db")
    }

    /// 内置库文件路径（供备份/恢复使用）
    pub fn path(&self) -> PathBuf {
        Self::db_path()
    }

    /// 将 WAL 合并回主库（备份前调用，保证数据完整落地）
    pub fn checkpoint(&self) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    /// 获取池化连接（Deref 到 rusqlite::Connection）
    /// 连接池超时（5s）时 panic；在此之前记录 error 日志以便诊断。
    /// 关键路径应使用 try_lock_conn() 获取 Result 以优雅降级。
    pub(crate) fn lock_conn(&self) -> r2d2::PooledConnection<SqliteConnectionManager> {
        self.pool.get().unwrap_or_else(|e| {
            log::error!(
                "数据库连接池耗尽（pool_size={}）: {}",
                self.pool.state().connections,
                e
            );
            panic!("获取数据库连接失败: {}", e)
        })
    }

    /// 尝试获取池化连接（不 panic，超时返回 Err）
    #[allow(dead_code)] // 新增备用方法，关键路径逐步迁移使用
    pub(crate) fn try_lock_conn(
        &self,
    ) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, String> {
        self.pool.get().map_err(|e| {
            log::error!("数据库连接池耗尽: {}", e);
            format!("获取数据库连接失败: {}", e)
        })
    }

    fn init_tables(&self) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp   TEXT    NOT NULL,
                event_type  TEXT    NOT NULL,
                source      TEXT    NOT NULL DEFAULT '',
                title       TEXT    NOT NULL DEFAULT '',
                detail      TEXT    NOT NULL DEFAULT '',
                level       TEXT    NOT NULL DEFAULT 'info'
            );

            CREATE TABLE IF NOT EXISTS tasks (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at  TEXT    NOT NULL,
                agent_id    TEXT    NOT NULL,
                task_type   TEXT    NOT NULL,
                content     TEXT    NOT NULL DEFAULT '',
                status      TEXT    NOT NULL DEFAULT 'pending',
                result      TEXT    NOT NULL DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS agent_log (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp   TEXT    NOT NULL,
                agent_id    TEXT    NOT NULL,
                action      TEXT    NOT NULL,
                detail      TEXT    NOT NULL DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS _config (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS llm_chat_messages (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                provider_id TEXT NOT NULL,
                model       TEXT NOT NULL,
                role        TEXT NOT NULL,
                content     TEXT NOT NULL DEFAULT '',
                parts_json  TEXT,
                created_at  TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_llm_chat ON llm_chat_messages(provider_id, model);

            CREATE TABLE IF NOT EXISTS llm_agent_tool_steps (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                provider_id   TEXT NOT NULL,
                model         TEXT NOT NULL,
                assistant_idx INTEGER NOT NULL,
                steps_json    TEXT NOT NULL,
                created_at    TEXT NOT NULL,
                UNIQUE(provider_id, model, assistant_idx)
            );
            CREATE INDEX IF NOT EXISTS idx_llm_agent_steps ON llm_agent_tool_steps(provider_id, model);

            CREATE TABLE IF NOT EXISTS harness_sessions (
                id           TEXT PRIMARY KEY,
                title        TEXT NOT NULL DEFAULT '',
                preset_id    TEXT NOT NULL DEFAULT '',
                workspace_id TEXT NOT NULL DEFAULT '',
                created_at   TEXT NOT NULL,
                updated_at   TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS harness_events (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                seq        INTEGER NOT NULL,
                type       TEXT NOT NULL,
                payload    TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(session_id, seq)
            );
            CREATE INDEX IF NOT EXISTS idx_harness_events_session ON harness_events(session_id, seq);

            CREATE TABLE IF NOT EXISTS harness_usage (
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id        TEXT NOT NULL,
                provider          TEXT NOT NULL DEFAULT '',
                model             TEXT NOT NULL DEFAULT '',
                prompt_tokens     INTEGER NOT NULL DEFAULT 0,
                completion_tokens INTEGER NOT NULL DEFAULT 0,
                cost              REAL NOT NULL DEFAULT 0,
                llm_wall_ms       INTEGER NOT NULL DEFAULT 0,
                first_token_ms    INTEGER NOT NULL DEFAULT 0,
                requests          INTEGER NOT NULL DEFAULT 0,
                cached_tokens     INTEGER NOT NULL DEFAULT 0,
                created_at        TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_harness_usage_session ON harness_usage(session_id);

            CREATE TABLE IF NOT EXISTS harness_feedback (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                rating     TEXT NOT NULL DEFAULT '',
                comment    TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_harness_feedback_session ON harness_feedback(session_id);

            CREATE TABLE IF NOT EXISTS harness_kv (
                k TEXT PRIMARY KEY,
                v TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agents (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL,
                description TEXT DEFAULT '',
                role_id     TEXT DEFAULT '',
                provider_id TEXT DEFAULT '',
                model       TEXT DEFAULT '',
                kb_id       INTEGER,
                temperature REAL DEFAULT 0.7,
                max_tokens  INTEGER DEFAULT 2048,
                top_p       REAL DEFAULT 1.0,
                created_at  TEXT DEFAULT (datetime('now')),
                updated_at  TEXT DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_events_time   ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_tasks_agent   ON tasks(agent_id);
            CREATE INDEX IF NOT EXISTS idx_agent_log_agt ON agent_log(agent_id);
        ",
        )?;
        // 迁移：为旧表添加 parts_json 列（若已存在则忽略错误）
        conn.execute_batch("ALTER TABLE llm_chat_messages ADD COLUMN parts_json TEXT;")
            .ok();
        // 迁移：harness_sessions 增加 preset_id（每会话预设作用域）
        conn.execute_batch(
            "ALTER TABLE harness_sessions ADD COLUMN preset_id TEXT NOT NULL DEFAULT '';",
        )
        .ok();
        // 迁移：harness_sessions 增加 workspace_id（会话归属工作区，DSH 工作区浏览器）
        conn.execute_batch(
            "ALTER TABLE harness_sessions ADD COLUMN workspace_id TEXT NOT NULL DEFAULT '';",
        )
        .ok();
        // 迁移：harness_sessions 增加 archived（会话归档标记，DSH 归档会话）
        conn.execute_batch(
            "ALTER TABLE harness_sessions ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;",
        )
        .ok();
        // 迁移：harness_sessions 增加 order_index（手动拖拽排序，DSH 手动排序；
        // 0 = 未设置，按最近更新兜底）
        conn.execute_batch(
            "ALTER TABLE harness_sessions ADD COLUMN order_index INTEGER NOT NULL DEFAULT 0;",
        )
        .ok();
        // 迁移：harness_feedback 增加 message_seq（按助手消息级反馈）
        conn.execute_batch("ALTER TABLE harness_feedback ADD COLUMN message_seq INTEGER;")
            .ok();
        // 迁移：harness_usage 增加遥测列（DSH 统计条：LLM/首 token 耗时、
        // 请求次数、缓存命中 token；旧库无列则忽略错误）
        conn.execute_batch(
            "ALTER TABLE harness_usage ADD COLUMN llm_wall_ms INTEGER NOT NULL DEFAULT 0;",
        )
        .ok();
        conn.execute_batch(
            "ALTER TABLE harness_usage ADD COLUMN first_token_ms INTEGER NOT NULL DEFAULT 0;",
        )
        .ok();
        conn.execute_batch(
            "ALTER TABLE harness_usage ADD COLUMN requests INTEGER NOT NULL DEFAULT 0;",
        )
        .ok();
        conn.execute_batch(
            "ALTER TABLE harness_usage ADD COLUMN cached_tokens INTEGER NOT NULL DEFAULT 0;",
        )
        .ok();
        conn.execute_batch(
            "ALTER TABLE harness_usage ADD COLUMN tool_wall_ms INTEGER NOT NULL DEFAULT 0;",
        )
        .ok();
        // 迁移：harness_usage 增加 reasoning_effort（DSH
        // AssistantRequestConfig.reasoningEffort：本轮实际推理等级）
        conn.execute_batch("ALTER TABLE harness_usage ADD COLUMN reasoning_effort TEXT;")
            .ok();
        // 写入默认配置
        conn.execute(
            "INSERT OR IGNORE INTO _config (key, value) VALUES ('retention_days', '90')",
            [],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO _config (key, value) VALUES ('max_event_rows', '10000')",
            [],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO _config (key, value) VALUES ('auto_vacuum', '1')",
            [],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO _config (key, value) VALUES ('page_size', '4096')",
            [],
        )?;
        if let Ok(auto) = conn.query_row(
            "SELECT value FROM _config WHERE key='auto_vacuum'",
            [],
            |r| r.get::<_, String>(0),
        ) {
            conn.execute_batch(&format!("PRAGMA auto_vacuum={};", auto))
                .ok();
        }
        Ok(())
    }

    // ─── 配置管理 ───

    pub fn get_config(&self) -> SqlResult<Vec<ConfigItem>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare("SELECT key, value FROM _config ORDER BY key")?;
        let rows = stmt.query_map([], |row| {
            Ok(ConfigItem {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })?;
        let mut items = Vec::new();
        for r in rows {
            items.push(r?);
        }
        Ok(items)
    }

    pub fn set_config(&self, key: &str, value: &str) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT OR REPLACE INTO _config (key, value) VALUES (?1,?2)",
            params![key, value],
        )?;
        if key == "auto_vacuum" {
            conn.execute_batch(&format!("PRAGMA auto_vacuum={};", value))
                .ok();
        }
        Ok(())
    }

    // ─── 表浏览 / CRUD（查询委托共享引擎 sql_browse） ───

    /// 列出数据库中所有表（含下划线开头 / 系统表，保证全部可见）
    pub fn list_tables(&self) -> Result<Vec<String>, String> {
        let conn = self.lock_conn();
        crate::sql_browse::list_tables(&conn)
    }

    /// 获取表的列信息
    pub fn table_schema(&self, table: &str) -> Result<Vec<ColumnInfo>, String> {
        let conn = self.lock_conn();
        crate::sql_browse::table_schema(&conn, table)
    }

    /// 查询表数据（分页/过滤/排序；cursor+direction 为 keyset 分页，recount=false 时跳过 COUNT）
    pub fn query_table(
        &self,
        params: &crate::sql_browse::TableQueryParams,
    ) -> Result<TableData, String> {
        let conn = self.lock_conn();
        crate::sql_browse::query_table(&conn, params)
    }

    /// 插入行（按 JSON 值类型绑定，保留数值/布尔/空语义）
    pub fn insert_row(
        &self,
        table: &str,
        data: &serde_json::Map<String, serde_json::Value>,
    ) -> SqlResult<i64> {
        let conn = self.lock_conn();
        let safe_table = crate::sql_browse::safe_name(table);
        let cols: Vec<&str> = data.keys().map(|k| k.as_str()).collect();
        let placeholders: Vec<String> = (0..cols.len()).map(|i| format!("?{}", i + 1)).collect();
        let vals: Vec<rusqlite::types::Value> = data
            .values()
            .map(crate::sql_browse::json_to_sql_value)
            .collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            safe_table,
            cols.iter()
                .map(|c| crate::sql_browse::safe_name(c))
                .collect::<Vec<_>>()
                .join(","),
            placeholders.join(",")
        );
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = vals
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        conn.execute(&sql, param_refs.as_slice())?;
        Ok(conn.last_insert_rowid())
    }

    /// 更新行（按 rowid 定位，JSON 值按类型绑定）
    pub fn update_row(
        &self,
        table: &str,
        rowid: i64,
        data: &serde_json::Map<String, serde_json::Value>,
    ) -> SqlResult<()> {
        let conn = self.lock_conn();
        let safe_table = crate::sql_browse::safe_name(table);
        let sets: Vec<String> = data
            .iter()
            .enumerate()
            .map(|(i, (k, _))| format!("{} = ?{}", crate::sql_browse::safe_name(k), i + 1))
            .collect();
        let vals: Vec<rusqlite::types::Value> = data
            .values()
            .map(crate::sql_browse::json_to_sql_value)
            .collect();
        let sql = format!(
            "UPDATE {} SET {} WHERE rowid = ?{}",
            safe_table,
            sets.join(","),
            vals.len() + 1
        );
        let mut param_refs: Vec<&dyn rusqlite::types::ToSql> = vals
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        let rowid_param: i64 = rowid;
        param_refs.push(&rowid_param);
        conn.execute(&sql, param_refs.as_slice())?;
        Ok(())
    }

    /// 删除行
    pub fn delete_row(&self, table: &str, rowid: i64) -> SqlResult<()> {
        let conn = self.lock_conn();
        let safe_table = crate::sql_browse::safe_name(table);
        conn.execute(
            &format!("DELETE FROM {} WHERE rowid = ?1", safe_table),
            params![rowid],
        )?;
        Ok(())
    }

    /// 清理旧数据（按保留天数）
    pub fn cleanup_old_data(&self) -> SqlResult<CleanupResult> {
        let conn = self.lock_conn();
        let days: i64 = conn
            .query_row(
                "SELECT value FROM _config WHERE key='retention_days'",
                [],
                |r| r.get::<_, String>(0),
            )
            .map(|s| s.parse().unwrap_or(90))
            .unwrap_or(90);
        let cutoff = chrono::Local::now() - chrono::Duration::days(days);
        let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%S").to_string();

        let deleted_events = conn.execute(
            "DELETE FROM events WHERE timestamp < ?1",
            params![cutoff_str],
        )?;
        let deleted_agent = conn.execute(
            "DELETE FROM agent_log WHERE timestamp < ?1",
            params![cutoff_str],
        )?;

        conn.execute_batch("PRAGMA optimize;").ok();
        Ok(CleanupResult {
            deleted_events,
            deleted_agent,
            days,
        })
    }

    /// 读取某行某列的原始值（用于查看完整 BLOB / 文本内容）
    pub fn read_cell(
        &self,
        table: &str,
        rowid: i64,
        column: &str,
    ) -> Result<rusqlite::types::Value, String> {
        let conn = self.lock_conn();
        crate::sql_browse::read_cell(&conn, table, rowid, column)
    }

    // ─── 大模型聊天记录持久化 ───

    /// 读取某 (provider_id, model) 的聊天记录，按时间顺序返回
    pub fn get_llm_chat_history(
        &self,
        provider_id: &str,
        model: &str,
    ) -> SqlResult<Vec<LlmChatMessage>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, role, content, parts_json, created_at FROM llm_chat_messages WHERE provider_id=?1 AND model=?2 ORDER BY id ASC"
        )?;
        let rows = stmt.query_map(params![provider_id, model], |row| {
            Ok(LlmChatMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                parts_json: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row?);
        }
        Ok(list)
    }

    /// 追加一条聊天消息
    pub fn append_llm_chat_message(
        &self,
        provider_id: &str,
        model: &str,
        role: &str,
        content: &str,
        parts_json: Option<&str>,
        created_at: &str,
    ) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO llm_chat_messages (provider_id, model, role, content, parts_json, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![provider_id, model, role, content, parts_json, created_at],
        )?;
        Ok(())
    }

    /// 清空某 (provider_id, model) 的聊天记录，返回删除条数
    pub fn clear_llm_chat_history(&self, provider_id: &str, model: &str) -> SqlResult<usize> {
        let conn = self.lock_conn();
        let n = conn.execute(
            "DELETE FROM llm_chat_messages WHERE provider_id=?1 AND model=?2",
            params![provider_id, model],
        )?;
        // 工具调用历史随对话一并清空
        let _ = conn.execute(
            "DELETE FROM llm_agent_tool_steps WHERE provider_id=?1 AND model=?2",
            params![provider_id, model],
        )?;
        Ok(n)
    }

    // ─── 代理工具调用历史（随对话持久化） ───

    /// 保存/覆盖某条助手消息（按助手序号定位）的工具调用步骤
    pub fn save_agent_tool_steps(
        &self,
        provider_id: &str,
        model: &str,
        assistant_idx: i64,
        steps_json: &str,
        created_at: &str,
    ) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO llm_agent_tool_steps (provider_id, model, assistant_idx, steps_json, created_at)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(provider_id, model, assistant_idx) DO UPDATE SET steps_json=?4, created_at=?5",
            params![provider_id, model, assistant_idx, steps_json, created_at],
        )?;
        Ok(())
    }

    /// 读取某 (provider_id, model) 全部工具调用步骤（助手序号 → steps_json，按序号升序）
    pub fn get_agent_tool_steps(
        &self,
        provider_id: &str,
        model: &str,
    ) -> SqlResult<Vec<(i64, String)>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT assistant_idx, steps_json FROM llm_agent_tool_steps WHERE provider_id=?1 AND model=?2 ORDER BY assistant_idx ASC",
        )?;
        let rows = stmt.query_map(params![provider_id, model], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row?);
        }
        Ok(list)
    }

    // ─── Harness 运行时（DSH 纯原生迁移）：会话日志持久化 ───

    /// 新建 Harness 会话（workspace_id = "" 表示默认工作区；
    /// order_index = 现有最大序号 + 1，拖拽排序基准）
    pub fn create_harness_session(
        &self,
        id: &str,
        created_at: &str,
        workspace_id: &str,
    ) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO harness_sessions (id, title, workspace_id, created_at, updated_at, order_index)
             VALUES (?1, '', ?3, ?2, ?2,
                     (SELECT COALESCE(MAX(order_index), 0) + 1 FROM harness_sessions))",
            params![id, created_at, workspace_id],
        )?;
        Ok(())
    }

    /// 设置会话手动排序序号（DSH 拖拽排序：交换双方各写一次）
    pub fn set_harness_session_order(&self, id: &str, order_index: i64) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "UPDATE harness_sessions SET order_index=?2 WHERE id=?1",
            params![id, order_index],
        )?;
        Ok(())
    }

    /// 交换两个会话的手动排序序号（DSH 拖拽排序：前端拖放即交换）
    pub fn swap_harness_session_order(&self, a: &str, b: &str) -> SqlResult<()> {
        let conn = self.lock_conn();
        let oa: i64 = conn
            .query_row(
                "SELECT order_index FROM harness_sessions WHERE id=?1",
                params![a],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let ob: i64 = conn
            .query_row(
                "SELECT order_index FROM harness_sessions WHERE id=?1",
                params![b],
                |r| r.get(0),
            )
            .unwrap_or(0);
        conn.execute(
            "UPDATE harness_sessions SET order_index=?2 WHERE id=?1",
            params![a, ob],
        )?;
        conn.execute(
            "UPDATE harness_sessions SET order_index=?2 WHERE id=?1",
            params![b, oa],
        )?;
        Ok(())
    }

    /// 设置会话归属工作区（DSH 工作区浏览器：会话可在工作区间移动）
    pub fn set_harness_session_workspace(
        &self,
        id: &str,
        workspace_id: &str,
        updated_at: &str,
    ) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "UPDATE harness_sessions SET workspace_id=?2, updated_at=?3 WHERE id=?1",
            params![id, workspace_id, updated_at],
        )?;
        Ok(())
    }

    /// 读取会话归属工作区
    pub fn get_harness_session_workspace(&self, id: &str) -> SqlResult<Option<String>> {
        let conn = self.lock_conn();
        conn.query_row(
            "SELECT workspace_id FROM harness_sessions WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| {
            if e == rusqlite::Error::QueryReturnedNoRows {
                Ok(None)
            } else {
                Err(e)
            }
        })
    }

    /// Harness 会话列表（消息数 = 用户消息数；手动排序优先，未设置按最近更新倒序）
    pub fn list_harness_sessions(&self) -> SqlResult<Vec<HarnessSessionRow>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.preset_id, s.workspace_id, s.created_at, s.updated_at,
                    (SELECT COUNT(*) FROM harness_events e WHERE e.session_id = s.id AND e.type = 'user_message'),
                    s.archived
             FROM harness_sessions s
             ORDER BY CASE WHEN s.order_index = 0 THEN 9223372036854775807 ELSE s.order_index END,
                      s.updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)? as usize,
                row.get::<_, i64>(7)? != 0,
            ))
        })?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row?);
        }
        Ok(list)
    }

    /// 设置会话归档标记（DSH workspace.archiveSession：归档后从工作区
    /// 常规列表隐去，保留在「已归档」分组；可恢复）
    pub fn set_harness_session_archived(&self, id: &str, archived: bool) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "UPDATE harness_sessions SET archived=?2, updated_at=?3 WHERE id=?1",
            params![
                id,
                if archived { 1 } else { 0 },
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
            ],
        )?;
        Ok(())
    }

    /// 追加一条 Harness 会话事件，返回写入后的 seq（会话内单调递增）
    pub fn append_harness_event(
        &self,
        session_id: &str,
        event_type: &str,
        payload: &str,
        created_at: &str,
    ) -> SqlResult<i64> {
        let conn = self.lock_conn();
        // L11：INSERT...RETURNING 直接返回本行实际分配的 seq——
        // 消除「INSERT 后重查 MAX(seq)」在并发写入下返回偏高序号
        // 的错位（影响标题投影 is_first 与前端 seq 锚点）
        let seq: i64 = conn.query_row(
            "INSERT INTO harness_events (session_id, seq, type, payload, created_at)
             SELECT ?1, COALESCE(MAX(seq), 0) + 1, ?2, ?3, ?4 FROM harness_events WHERE session_id = ?1
             RETURNING seq",
            params![session_id, event_type, payload, created_at],
            |r| r.get(0),
        )?;
        Ok(seq)
    }

    /// 读取某会话 seq > after_seq 的事件（增量恢复），返回 (seq, type, payload, created_at)
    pub fn get_harness_events(
        &self,
        session_id: &str,
        after_seq: i64,
    ) -> SqlResult<Vec<(i64, String, String, String)>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT seq, type, payload, created_at FROM harness_events WHERE session_id=?1 AND seq>?2 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![session_id, after_seq], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row?);
        }
        Ok(list)
    }

    /// 更新会话标题（首条用户消息投影）与更新时间
    pub fn set_harness_session_title(
        &self,
        id: &str,
        title: &str,
        updated_at: &str,
    ) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "UPDATE harness_sessions SET title=?2, updated_at=?3 WHERE id=?1",
            params![id, title, updated_at],
        )?;
        Ok(())
    }

    /// 读取会话标题（L4：清空后新首条消息重新投影标题的判断依据）
    pub fn get_harness_session_title(&self, id: &str) -> SqlResult<String> {
        let conn = self.lock_conn();
        conn.query_row(
            "SELECT title FROM harness_sessions WHERE id=?1",
            params![id],
            |row| row.get::<_, String>(0),
        )
    }

    /// 重命名会话（用户显式改名，与标题投影区分）
    pub fn rename_harness_session(&self, id: &str, title: &str) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "UPDATE harness_sessions SET title=?2 WHERE id=?1",
            params![id, title],
        )?;
        Ok(())
    }

    /// 触摸会话更新时间
    pub fn touch_harness_session(&self, id: &str, updated_at: &str) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "UPDATE harness_sessions SET updated_at=?2 WHERE id=?1",
            params![id, updated_at],
        )?;
        Ok(())
    }

    /// 删除会话及其全部事件
    pub fn delete_harness_session(&self, id: &str) -> SqlResult<usize> {
        let conn = self.lock_conn();
        let n = conn.execute("DELETE FROM harness_sessions WHERE id=?1", params![id])?;
        let _ = conn.execute(
            "DELETE FROM harness_events WHERE session_id=?1",
            params![id],
        )?;
        let _ = conn.execute("DELETE FROM harness_usage WHERE session_id=?1", params![id])?;
        Ok(n)
    }

    /// 清空会话聊天记录：删除全部事件与用量行（保留会话元信息/预设/角色）
    pub fn clear_harness_session(&self, id: &str) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "DELETE FROM harness_events WHERE session_id=?1",
            params![id],
        )?;
        conn.execute("DELETE FROM harness_usage WHERE session_id=?1", params![id])?;
        Ok(())
    }

    /// 设置会话预设（每会话预设作用域；空 = 跟随全局默认）
    pub fn set_harness_session_preset(&self, id: &str, preset_id: &str) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "UPDATE harness_sessions SET preset_id=?2 WHERE id=?1",
            params![id, preset_id],
        )?;
        Ok(())
    }

    /// 读取会话预设
    pub fn get_harness_session_preset(&self, id: &str) -> SqlResult<Option<String>> {
        let conn = self.lock_conn();
        let v = conn
            .query_row(
                "SELECT preset_id FROM harness_sessions WHERE id=?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .ok();
        Ok(v)
    }

    /// 会话分叉：把源会话 seq <= boundary 的事件复制到新会话
    pub fn fork_harness_session(
        &self,
        source: &str,
        child: &str,
        boundary_seq: i64,
        created_at: &str,
    ) -> SqlResult<usize> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO harness_sessions (id, title, preset_id, workspace_id, created_at, updated_at)
             SELECT ?2, s.title || '（分叉）', s.preset_id, s.workspace_id, ?3, ?3 FROM harness_sessions s WHERE s.id = ?1",
            params![source, child, created_at],
        )?;
        let n = conn.execute(
            "INSERT INTO harness_events (session_id, seq, type, payload, created_at)
             SELECT ?2, seq, type, payload, created_at FROM harness_events WHERE session_id=?1 AND seq<=?3",
            params![source, child, boundary_seq],
        )?;
        Ok(n)
    }

    /// 追加一条 Harness 会话用量记录（每轮对话一条；含 DSH 统计条遥测）
    pub fn append_harness_usage(&self, record: &HarnessUsageRecord) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO harness_usage (session_id, provider, model, reasoning_effort, prompt_tokens, completion_tokens, cost, llm_wall_ms, first_token_ms, requests, cached_tokens, tool_wall_ms, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                record.session_id,
                record.provider,
                record.model,
                record.reasoning_effort,
                record.prompt_tokens as i64,
                record.completion_tokens as i64,
                record.cost,
                record.llm_wall_ms as i64,
                record.first_token_ms as i64,
                record.requests as i64,
                record.cached_tokens as i64,
                record.tool_wall_ms as i64,
                record.created_at
            ],
        )?;
        Ok(())
    }

    /// 会话用量聚合：(轮数, 输入, 输出, 成本, LLM 墙钟, 首 token, 请求数, 缓存命中)
    pub fn harness_usage_summary(&self, session_id: &str) -> SqlResult<HarnessUsageAgg> {
        let conn = self.lock_conn();
        let row = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(prompt_tokens),0), COALESCE(SUM(completion_tokens),0),
                    COALESCE(SUM(cost),0), COALESCE(SUM(llm_wall_ms),0), COALESCE(SUM(first_token_ms),0),
                    COALESCE(SUM(requests),0), COALESCE(SUM(cached_tokens),0)
             FROM harness_usage WHERE session_id=?1",
            params![session_id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)? as usize,
                    r.get::<_, i64>(1)? as u64,
                    r.get::<_, i64>(2)? as u64,
                    r.get::<_, f64>(3)?,
                    r.get::<_, i64>(4)? as u64,
                    r.get::<_, i64>(5)? as u64,
                    r.get::<_, i64>(6)? as u64,
                    r.get::<_, i64>(7)? as u64,
                ))
            },
        )?;
        Ok(row)
    }

    // ─── Harness 反馈 / 存储 / 会话查询 ───

    /// 追加一条反馈（rating: good / bad；comment 可选；message_seq = 助手消息序号）
    pub fn append_harness_feedback(
        &self,
        session_id: &str,
        rating: &str,
        comment: &str,
        message_seq: Option<i64>,
        created_at: &str,
    ) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO harness_feedback (session_id, rating, comment, message_seq, created_at)
             VALUES (?1,?2,?3,?4,?5)",
            params![session_id, rating, comment, message_seq, created_at],
        )?;
        Ok(())
    }

    /// 反馈列表（按时间倒序）
    pub fn list_harness_feedback(&self) -> SqlResult<Vec<HarnessFeedbackRow>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, rating, comment, message_seq, created_at
             FROM harness_feedback ORDER BY id DESC LIMIT 200",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row?);
        }
        Ok(list)
    }

    /// KV 存储：put（UPSERT）
    pub fn harness_kv_put(&self, key: &str, value: &str) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO harness_kv (k, v) VALUES (?1,?2) ON CONFLICT(k) DO UPDATE SET v=?2",
            params![key, value],
        )?;
        Ok(())
    }

    /// KV 存储：get
    pub fn harness_kv_get(&self, key: &str) -> SqlResult<Option<String>> {
        let conn = self.lock_conn();
        let v = conn
            .query_row("SELECT v FROM harness_kv WHERE k=?1", params![key], |r| {
                r.get::<_, String>(0)
            })
            .ok();
        Ok(v)
    }

    /// KV 存储：delete
    pub fn harness_kv_delete(&self, key: &str) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute("DELETE FROM harness_kv WHERE k=?1", params![key])?;
        Ok(())
    }

    /// 会话查询：按关键词搜索事件载荷（user_message / assistant_message）。
    /// 多词按 AND 语义（每个词均须命中），返回 (session_id, type, content 片段)
    /// （按命中倒序，最多 50 条）
    pub fn search_harness_sessions(&self, query: &str) -> SqlResult<Vec<(String, String, String)>> {
        let conn = self.lock_conn();
        let words: Vec<String> = query
            .split_whitespace()
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty())
            .collect();
        // 片段：以首个命中词为中心取上下文（长 tool_result 中段命中可见），
        // 无命中时退回载荷头部（与 event_search 的片段语义一致）
        let first_word = words[0].clone();
        let sql = format!(
            "SELECT e.session_id, e.type,
               CASE WHEN instr(lower(e.payload), lower(?1)) > 0
                    THEN substr(e.payload, max(1, instr(lower(e.payload), lower(?1)) - 60), 200)
                    ELSE substr(e.payload, 1, 200) END
             FROM harness_events e
             WHERE e.type IN ('user_message','assistant_message','assistant_tool_calls',
                              'tool_result','subagent_reported','todo_update',
                              'goal_set','goal_update','compaction')
               AND {} ORDER BY e.id DESC LIMIT 50",
            words
                .iter()
                .enumerate()
                .map(|(i, _)| format!("e.payload LIKE ?{}", i + 2))
                .collect::<Vec<_>>()
                .join(" AND ")
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut params: Vec<rusqlite::types::Value> =
            vec![rusqlite::types::Value::from(first_word.clone())];
        params.extend(
            words
                .iter()
                .map(|w| rusqlite::types::Value::from(format!("%{w}%"))),
        );
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row?);
        }
        Ok(list)
    }

    // ─── 事件日志 ───

    pub fn insert_event(
        &self,
        ts: &str,
        event_type: &str,
        source: &str,
        title: &str,
        detail: &str,
        level: &str,
    ) -> SqlResult<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO events (timestamp, event_type, source, title, detail, level) VALUES (?1,?2,?3,?4,?5,?6)",
            params![ts, event_type, source, title, detail, level],
        )?;
        Ok(())
    }

    /// 批量插入事件（单事务），供事件转发循环批量落库，避免逐条提交的开销
    pub fn insert_events_batch(&self, events: &[crate::EventLog]) -> SqlResult<usize> {
        if events.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock_conn();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO events (timestamp, event_type, source, title, detail, level) VALUES (?1,?2,?3,?4,?5,?6)",
            )?;
            for ev in events {
                stmt.execute(params![
                    ev.timestamp,
                    ev.event_type,
                    ev.source,
                    ev.title,
                    ev.detail,
                    ev.level
                ])?;
            }
        }
        tx.commit()?;
        Ok(events.len())
    }

    pub fn query_events(&self, limit: usize, offset: usize) -> SqlResult<Vec<crate::EventLog>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, event_type, source, title, detail, level FROM events ORDER BY id DESC LIMIT ?1 OFFSET ?2"
        )?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
            Ok(crate::EventLog {
                id: row.get::<_, i64>(0)? as usize,
                timestamp: row.get(1)?,
                event_type: row.get(2)?,
                source: row.get(3)?,
                title: row.get(4)?,
                detail: row.get(5)?,
                level: row.get(6)?,
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    // ─── Agent 日志 ───

    pub fn query_agent_log(&self, agent_id: &str, limit: usize) -> SqlResult<Vec<AgentLogRow>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, agent_id, action, detail FROM agent_log WHERE agent_id=?1 ORDER BY id DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![agent_id, limit as i64], |row| {
            Ok(AgentLogRow {
                id: row.get::<_, i64>(0)? as usize,
                timestamp: row.get(1)?,
                agent_id: row.get(2)?,
                action: row.get(3)?,
                detail: row.get(4)?,
            })
        })?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row?);
        }
        Ok(list)
    }

    // ─── 数据库信息 ───

    pub fn db_info(&self) -> SqlResult<DbInfo> {
        let conn = self.lock_conn();
        let event_count: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
        let task_count: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))?;
        let agent_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM agent_log", [], |r| r.get(0))?;
        let size = std::fs::metadata(Self::db_path())
            .map(|m| m.len())
            .unwrap_or(0);
        Ok(DbInfo {
            path: Self::db_path().display().to_string(),
            size_bytes: size,
            event_count: event_count as usize,
            task_count: task_count as usize,
            agent_log_count: agent_count as usize,
        })
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        // 退出前将 WAL 中的记录合并回主库，确保聊天记录等数据可靠落地
        if let Ok(conn) = self.pool.get() {
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }
    }
}

// ─── 数据结构 ───

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgentLogRow {
    pub id: usize,
    pub timestamp: String,
    pub agent_id: String,
    pub action: String,
    pub detail: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DbInfo {
    pub path: String,
    pub size_bytes: u64,
    pub event_count: usize,
    pub task_count: usize,
    pub agent_log_count: usize,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ConfigItem {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CleanupResult {
    pub deleted_events: usize,
    pub deleted_agent: usize,
    pub days: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LlmChatMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub parts_json: Option<String>,
    pub created_at: String,
}

/// Harness 会话行：(id, title, preset_id, workspace_id, created_at, updated_at, message_count, archived)
pub type HarnessSessionRow = (String, String, String, String, String, String, usize, bool);

/// Harness 会话用量记录（每轮对话一条）
#[derive(Clone, Debug)]
pub struct HarnessUsageRecord {
    pub session_id: String,
    pub provider: String,
    pub model: String,
    /// 本轮实际生效的推理等级（DSH AssistantRequestConfig.reasoningEffort；
    /// None = 未启用/默认）
    pub reasoning_effort: Option<String>,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cost: f64,
    /// LLM 请求墙钟合计（毫秒）
    pub llm_wall_ms: u64,
    /// 首 token / 首字节延迟合计（毫秒）
    pub first_token_ms: u64,
    /// 模型请求次数
    pub requests: u64,
    /// 缓存命中 token 合计
    pub cached_tokens: u64,
    /// 工具调用墙钟合计（毫秒）
    pub tool_wall_ms: u64,
    pub created_at: String,
}

/// Harness 反馈行：(id, session_id, rating, comment, created_at)
pub type HarnessFeedbackRow = (i64, String, String, String, Option<i64>, String);

/// Harness 会话用量聚合（telemetry 查询返回）
pub type HarnessUsageAgg = (usize, u64, u64, f64, u64, u64, u64, u64);

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        Database::new().unwrap()
    }

    #[test]
    fn harness_session_event_append_returns_monotonic_seq() {
        // L11：INSERT...RETURNING seq——追加事件的 seq 必须单调递增且
        // 是实际分配值（并发写入不错位）。db 层直接验证底层 SQL。
        let db = test_db();
        let id = format!("h-dbtest-{}", uuid::Uuid::new_v4().simple());
        let now = "2026-08-20T00:00:00";
        db.create_harness_session(&id, now, "").unwrap();
        // 追加 3 个事件：seq 应 1,2,3（单调）
        let s1 = db
            .append_harness_event(&id, "user_message", "{\"a\":1}", now)
            .unwrap();
        let s2 = db
            .append_harness_event(&id, "assistant_message", "{\"b\":2}", now)
            .unwrap();
        let s3 = db
            .append_harness_event(&id, "tool_result", "{\"c\":3}", now)
            .unwrap();
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
        assert_eq!(s3, 3);
        // 读取事件：seq 顺序 + 载荷完整
        let events = db.get_harness_events(&id, 0).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].1, "user_message");
        assert!(events[2].2.contains("\"c\":3"));
        // 增量读取：after_seq=1 → 后 2 条
        let tail = db.get_harness_events(&id, 1).unwrap();
        assert_eq!(tail.len(), 2);
        // 不同会话隔离（seq 各自从 1 开始）
        let id2 = format!("h-dbtest2-{}", uuid::Uuid::new_v4().simple());
        db.create_harness_session(&id2, now, "").unwrap();
        let s = db
            .append_harness_event(&id2, "user_message", "{}", now)
            .unwrap();
        assert_eq!(s, 1, "新会话 seq 应从 1 开始");
        // 清理
        let _ = db.delete_harness_session(&id);
        let _ = db.delete_harness_session(&id2);
    }

    #[test]
    fn harness_kv_put_get_delete_roundtrip() {
        // KV 底层：UPSERT / 读取 / 删除
        let db = test_db();
        db.harness_kv_put("dbtest-key", "v1").unwrap();
        assert_eq!(
            db.harness_kv_get("dbtest-key").unwrap().as_deref(),
            Some("v1")
        );
        // UPSERT 覆盖
        db.harness_kv_put("dbtest-key", "v2").unwrap();
        assert_eq!(
            db.harness_kv_get("dbtest-key").unwrap().as_deref(),
            Some("v2")
        );
        db.harness_kv_delete("dbtest-key").unwrap();
        assert!(db.harness_kv_get("dbtest-key").unwrap().is_none());
    }
}
