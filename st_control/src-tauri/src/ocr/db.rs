// ============================================================
// 图文识别 — 数据层
// 独立连接 control.db（WAL 模式支持多连接并发），
// 表 ocr_resources 保存接收的资源、分类结果与 OCR 结果。
// ============================================================

use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub(crate) fn db_path() -> PathBuf {
    crate::common::st_data_dir().join("control.db")
}

/// 一条图文识别资源记录
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrResource {
    pub id: i64,
    pub sender_username: String,
    pub session_type: String,
    pub timestamp: String,
    pub username: String,
    pub media_url: String,
    pub media_path: String,
    pub category: String,
    pub category_desc: String,
    pub status: String,
    pub error: String,
    /// 开源 OCR 预检识别出的文本（未识别到时为空）
    pub precheck_text: String,
    pub classify_raw: String,
    pub ocr_raw: String,
    pub ocr_fields: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 资源列表 + 总数（分页）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrPage {
    pub total: i64,
    pub items: Vec<OcrResource>,
}

/// 统计（按状态 / 按分类）
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrStats {
    pub total: i64,
    pub by_status: HashMap<String, i64>,
    pub by_category: HashMap<String, i64>,
}

pub struct OcrDb {
    conn: Mutex<Connection>,
}

impl OcrDb {
    pub fn open() -> rusqlite::Result<Self> {
        let path = db_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        let db = OcrDb {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS ocr_resources (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                sender_username TEXT NOT NULL,
                session_type    TEXT NOT NULL DEFAULT '',
                timestamp       TEXT NOT NULL DEFAULT '',
                username        TEXT NOT NULL DEFAULT '',
                media_url       TEXT NOT NULL,
                media_path      TEXT NOT NULL DEFAULT '',
                category        TEXT NOT NULL DEFAULT '',
                category_desc   TEXT NOT NULL DEFAULT '',
                status          TEXT NOT NULL DEFAULT 'pending',
                error           TEXT NOT NULL DEFAULT '',
                precheck_text   TEXT NOT NULL DEFAULT '',
                classify_raw    TEXT NOT NULL DEFAULT '',
                ocr_raw         TEXT NOT NULL DEFAULT '',
                ocr_fields      TEXT NOT NULL DEFAULT '',
                created_at      TEXT NOT NULL DEFAULT (datetime('now','localtime')),
                updated_at      TEXT NOT NULL DEFAULT (datetime('now','localtime'))
            );
            CREATE INDEX IF NOT EXISTS idx_ocr_status ON ocr_resources(status);
            CREATE INDEX IF NOT EXISTS idx_ocr_category ON ocr_resources(category);
            CREATE INDEX IF NOT EXISTS idx_ocr_created ON ocr_resources(created_at DESC);
            ",
        )?;
        // 旧库迁移：补充预检文本列
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(ocr_resources)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !cols.iter().any(|c| c == "precheck_text") {
            conn.execute(
                "ALTER TABLE ocr_resources ADD COLUMN precheck_text TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    // ─── 配置 ───

    pub fn get_config_map(&self) -> HashMap<String, String> {
        let conn = self.lock();
        let mut map = HashMap::new();
        if let Ok(mut stmt) = conn.prepare("SELECT key, value FROM _config") {
            if let Ok(rows) =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            {
                for row in rows.flatten() {
                    map.insert(row.0, row.1);
                }
            }
        }
        map
    }

    pub fn set_config(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO _config (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ─── 资源 CRUD ───

    pub fn insert_resource(
        &self,
        sender_username: &str,
        session_type: &str,
        timestamp: &str,
        username: &str,
        media_url: &str,
    ) -> rusqlite::Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO ocr_resources
             (sender_username, session_type, timestamp, username, media_url, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
            params![
                sender_username,
                session_type,
                timestamp,
                username,
                media_url
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// 本地文件导入：media_url 存文件名，media_path 存真实路径（fetch_media 支持本地路径直接读取）
    pub fn insert_local_resource(
        &self,
        sender_username: &str,
        session_type: &str,
        timestamp: &str,
        username: &str,
        media_url: &str,
        media_path: &str,
    ) -> rusqlite::Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO ocr_resources
             (sender_username, session_type, timestamp, username, media_url, media_path, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending')",
            params![
                sender_username,
                session_type,
                timestamp,
                username,
                media_url,
                media_path
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// 人工校对：覆盖识别字段并标记为 corrected
    pub fn update_ocr_fields(&self, id: i64, fields: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE ocr_resources SET ocr_fields=?1, status='corrected',
             updated_at=datetime('now','localtime') WHERE id=?2",
            params![fields, id],
        )?;
        Ok(())
    }

    /// 全量资源（导出用，按创建时间倒序）
    pub fn all_resources(&self) -> rusqlite::Result<Vec<OcrResource>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, sender_username, session_type, timestamp, username, media_url,
                    media_path, category, category_desc, status, error, precheck_text,
                    classify_raw, ocr_raw, ocr_fields, created_at, updated_at
             FROM ocr_resources ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], row_to_resource)?;
        rows.collect()
    }

    pub fn get_resource(&self, id: i64) -> rusqlite::Result<Option<OcrResource>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, sender_username, session_type, timestamp, username, media_url,
                    media_path, category, category_desc, status, error, precheck_text,
                    classify_raw, ocr_raw, ocr_fields, created_at, updated_at
             FROM ocr_resources WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_resource)?;
        rows.next().transpose()
    }

    pub fn list_resources(
        &self,
        limit: i64,
        offset: i64,
        status: Option<&str>,
        category: Option<&str>,
        keyword: Option<&str>,
    ) -> rusqlite::Result<OcrPage> {
        let conn = self.lock();
        let mut sql = String::from(
            "SELECT id, sender_username, session_type, timestamp, username, media_url,
                    media_path, category, category_desc, status, error, precheck_text,
                    classify_raw, ocr_raw, ocr_fields, created_at, updated_at
             FROM ocr_resources WHERE 1=1",
        );
        let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(s) = status {
            if !s.is_empty() {
                sql.push_str(" AND status = ?");
                args.push(Box::new(s.to_string()));
            }
        }
        if let Some(c) = category {
            if !c.is_empty() {
                sql.push_str(" AND category = ?");
                args.push(Box::new(c.to_string()));
            }
        }
        if let Some(k) = keyword {
            let k = k.trim();
            if !k.is_empty() {
                sql.push_str(
                    " AND (sender_username LIKE ? OR username LIKE ? OR media_url LIKE ?)",
                );
                let pat = format!("%{k}%");
                args.push(Box::new(pat.clone()));
                args.push(Box::new(pat.clone()));
                args.push(Box::new(pat));
            }
        }

        let total: i64 = {
            let count_sql = sql.replacen(
                "SELECT id, sender_username, session_type, timestamp, username, media_url,
                    media_path, category, category_desc, status, error, precheck_text,
                    classify_raw, ocr_raw, ocr_fields, created_at, updated_at",
                "SELECT COUNT(*)",
                1,
            );
            let mut stmt = conn.prepare(&count_sql)?;
            let mut rows =
                stmt.query(rusqlite::params_from_iter(args.iter().map(|b| b.as_ref())))?;
            match rows.next()? {
                Some(r) => r.get(0)?,
                None => 0,
            }
        };

        sql.push_str(" ORDER BY id DESC LIMIT ? OFFSET ?");
        args.push(Box::new(limit));
        args.push(Box::new(offset));
        let mut stmt = conn.prepare(&sql)?;
        let items = stmt
            .query_map(
                rusqlite::params_from_iter(args.iter().map(|b| b.as_ref())),
                row_to_resource,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(OcrPage { total, items })
    }

    pub fn stats(&self) -> rusqlite::Result<OcrStats> {
        let conn = self.lock();
        let mut stats = OcrStats {
            total: conn.query_row("SELECT COUNT(*) FROM ocr_resources", [], |r| r.get(0))?,
            ..Default::default()
        };
        {
            let mut stmt =
                conn.prepare("SELECT status, COUNT(*) FROM ocr_resources GROUP BY status")?;
            for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
                let (k, v) = row?;
                stats.by_status.insert(k, v);
            }
        }
        {
            let mut stmt =
                conn.prepare("SELECT category, COUNT(*) FROM ocr_resources GROUP BY category")?;
            for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
                let (k, v) = row?;
                stats.by_category.insert(k, v);
            }
        }
        Ok(stats)
    }

    pub fn update_processing(&self, id: i64) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE ocr_resources SET status='processing', error='',
             updated_at=datetime('now','localtime') WHERE id=?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn update_media_path(&self, id: i64, path: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE ocr_resources SET media_path=?1, updated_at=datetime('now','localtime') WHERE id=?2",
            params![path, id],
        )?;
        Ok(())
    }

    pub fn update_classified(
        &self,
        id: i64,
        category: &str,
        desc: &str,
        classify_raw: &str,
    ) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE ocr_resources SET category=?1, category_desc=?2, classify_raw=?3,
             updated_at=datetime('now','localtime') WHERE id=?4",
            params![category, desc, classify_raw, id],
        )?;
        Ok(())
    }

    pub fn update_ocr_result(
        &self,
        id: i64,
        status: &str,
        ocr_raw: &str,
        ocr_fields: &str,
        error: &str,
    ) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE ocr_resources SET status=?1, ocr_raw=?2, ocr_fields=?3, error=?4,
             updated_at=datetime('now','localtime') WHERE id=?5",
            params![status, ocr_raw, ocr_fields, error, id],
        )?;
        Ok(())
    }

    pub fn update_failed(&self, id: i64, status: &str, error: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE ocr_resources SET status=?1, error=?2,
             updated_at=datetime('now','localtime') WHERE id=?3",
            params![status, error, id],
        )?;
        Ok(())
    }

    /// 记录开源 OCR 预检识别出的文本
    pub fn update_precheck_text(&self, id: i64, text: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE ocr_resources SET precheck_text=?1, updated_at=datetime('now','localtime') WHERE id=?2",
            params![text, id],
        )?;
        Ok(())
    }

    pub fn delete_resource(&self, id: i64) -> rusqlite::Result<Option<OcrResource>> {
        // 注意：不能先持锁再调用 self.get_resource（std Mutex 不可重入，会死锁）
        let item = self.get_resource(id)?;
        if item.is_some() {
            let conn = self.lock();
            conn.execute("DELETE FROM ocr_resources WHERE id=?1", params![id])?;
        }
        Ok(item)
    }

    /// 数据目录（资源归档根目录）
    pub fn storage_root() -> PathBuf {
        crate::common::st_data_dir().join("ocr")
    }
}

fn row_to_resource(row: &rusqlite::Row<'_>) -> rusqlite::Result<OcrResource> {
    Ok(OcrResource {
        id: row.get(0)?,
        sender_username: row.get(1)?,
        session_type: row.get(2)?,
        timestamp: row.get(3)?,
        username: row.get(4)?,
        media_url: row.get(5)?,
        media_path: row.get(6)?,
        category: row.get(7)?,
        category_desc: row.get(8)?,
        status: row.get(9)?,
        error: row.get(10)?,
        precheck_text: row.get(11)?,
        classify_raw: row.get(12)?,
        ocr_raw: row.get(13)?,
        ocr_fields: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}
