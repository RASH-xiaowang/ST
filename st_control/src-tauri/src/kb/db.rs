// ============================================================
// 知识库数据库 — 独立 SQLite 库（与 control.db 分离）
// 业务元数据 + 文档分片 + 向量（以 FLOAT BLOB 存储，检索时在 Rust 侧计算余弦相似度）
// 注：中小企业单机场景，采用 BLOB 存向量 + 内存/SQL 计算，避免引入外部向量库依赖。
// 并发：使用 r2d2 同步连接池（max 8），替代单 Mutex 串行化，WAL 下读写可并行。
// ============================================================

use rusqlite::{Connection, Result as SqlResult};
use std::path::PathBuf;

/// 知识库数据库连接（单例，由 Tauri State 管理）。
/// 内部为 r2d2 连接池，可廉价 Clone 供后台异步任务持有。
#[derive(Clone)]
pub struct KbDatabase {
    pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
}

impl KbDatabase {
    /// 初始化知识库数据库（默认路径 %APPDATA%/st-control/knowledge_base.db）
    pub fn new() -> Result<Self, String> {
        Self::open_at(Self::db_path())
    }

    /// 使用自定义路径打开知识库数据库（集成测试 / 独立工具用）
    pub fn open_at(db_path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let manager = r2d2_sqlite::SqliteConnectionManager::file(&db_path).with_init(|conn| {
            conn.execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;",
            )
        });
        let pool = r2d2::Pool::builder()
            .max_size(8)
            .min_idle(Some(1))
            .connection_timeout(std::time::Duration::from_secs(5))
            .build(manager)
            .map_err(|e| format!("创建知识库连接池失败: {}", e))?;
        {
            let conn = pool
                .get()
                .map_err(|e| format!("获取知识库连接失败: {}", e))?;
            Self::init_tables(&conn).map_err(|e| e.to_string())?;
        }
        log::info!("知识库数据库已初始化: {}", db_path.display());
        Ok(KbDatabase { pool })
    }

    fn db_path() -> PathBuf {
        crate::common::st_data_dir().join("knowledge_base.db")
    }

    fn init_tables(conn: &Connection) -> SqlResult<()> {
        // 迁移①：旧版 FTS 使用外部内容表（content='...'）。此类表先删后插时对未索引
        // rowid 执行 DELETE 会报 "database disk image is malformed"（数据库损坏）。
        // 检测到旧 DDL 则先删除，交由下方 schema 重建为普通 FTS5 表。
        let mut fts_migrated = false;
        for name in ["chunks_fts", "wiki_pages_fts"] {
            let ddl: Option<String> = conn
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
                    rusqlite::params![name],
                    |r| r.get(0),
                )
                .ok();
            if let Some(d) = ddl {
                if d.contains("content=") {
                    log::warn!("知识库 FTS 表 {} 为旧版外部内容表，重建为普通 FTS5", name);
                    conn.execute_batch(&format!("DROP TABLE IF EXISTS {};", name))?;
                    fts_migrated = true;
                }
            }
        }
        conn.execute_batch(include_str!("schema.sql"))?;
        // 迁移⑦：删除历史遗留的死表（org_members / role_permissions / permissions /
        // organizations / password_reset_tokens）。这些表应用从未读写，属预留垃圾；
        // 对老库幂等清理，保证新旧库结构一致。
        for dead in [
            "org_members",
            "role_permissions",
            "permissions",
            "organizations",
            "password_reset_tokens",
        ] {
            conn.execute_batch(&format!("DROP TABLE IF EXISTS {};", dead))?;
        }
        // 迁移②：为旧库补充 document_chunks.parent_id 列（父子分块支持）
        let has_parent: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('document_chunks') WHERE name='parent_id'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if has_parent == 0 {
            conn.execute_batch("ALTER TABLE document_chunks ADD COLUMN parent_id INTEGER")?;
        }
        // 迁移④：为旧库补充 knowledge_bases.pinned / is_system 列（置顶与系统知识库）
        let kb_cols: Vec<String> = conn
            .prepare("PRAGMA table_info(knowledge_bases)")
            .and_then(|mut st| {
                let rows = st.query_map([], |r| r.get::<_, String>(1))?;
                let mut v = Vec::new();
                for c in rows.flatten() {
                    v.push(c);
                }
                Ok(v)
            })
            .unwrap_or_default();
        if !kb_cols.iter().any(|c| c == "pinned") {
            conn.execute_batch(
                "ALTER TABLE knowledge_bases ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
            )?;
        }
        if !kb_cols.iter().any(|c| c == "is_system") {
            conn.execute_batch(
                "ALTER TABLE knowledge_bases ADD COLUMN is_system INTEGER NOT NULL DEFAULT 0",
            )?;
        }
        // 迁移⑥：为旧库补充 documents.source 列（文档来源：上传/网页抓取/手动）
        let doc_cols: Vec<String> = conn
            .prepare("PRAGMA table_info(documents)")
            .and_then(|mut st| {
                let rows = st.query_map([], |r| r.get::<_, String>(1))?;
                let mut v = Vec::new();
                for c in rows.flatten() {
                    v.push(c);
                }
                Ok(v)
            })
            .unwrap_or_default();
        if !doc_cols.iter().any(|c| c == "source") {
            conn.execute_batch(
                "ALTER TABLE documents ADD COLUMN source TEXT NOT NULL DEFAULT 'upload'",
            )?;
        }
        // 迁移⑤：为旧库补充 wiki_pages.extract_status 列（摘要与实体提取状态）
        let wp_cols: Vec<String> = conn
            .prepare("PRAGMA table_info(wiki_pages)")
            .and_then(|mut st| {
                let rows = st.query_map([], |r| r.get::<_, String>(1))?;
                let mut v = Vec::new();
                for c in rows.flatten() {
                    v.push(c);
                }
                Ok(v)
            })
            .unwrap_or_default();
        if !wp_cols.iter().any(|c| c == "extract_status") {
            conn.execute_batch(
                "ALTER TABLE wiki_pages ADD COLUMN extract_status TEXT NOT NULL DEFAULT ''",
            )?;
        }
        if !wp_cols.iter().any(|c| c == "dir_id") {
            conn.execute_batch("ALTER TABLE wiki_pages ADD COLUMN dir_id INTEGER REFERENCES kb_directories(id) ON DELETE SET NULL")?;
        }
        // 迁移③：FTS 表重建后回填索引（含汉字间隔预处理）
        if fts_migrated {
            Self::rebuild_fts_indexes(conn)?;
        }
        Ok(())
    }

    /// 从内容表重建 FTS 索引（迁移/修复用；写入侧需与 cjk_spaced 保持一致）
    fn rebuild_fts_indexes(conn: &Connection) -> SqlResult<()> {
        conn.execute("DELETE FROM chunks_fts", [])?;
        conn.execute("DELETE FROM wiki_pages_fts", [])?;
        {
            let mut stmt = conn.prepare("SELECT id, content FROM document_chunks")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
            let items: Vec<(i64, String)> = rows.filter_map(|r| r.ok()).collect();
            drop(stmt);
            let mut ins = conn.prepare("INSERT INTO chunks_fts (rowid, content) VALUES (?1,?2)")?;
            for (id, content) in items {
                ins.execute(rusqlite::params![id, crate::kb::cjk_spaced(&content)])?;
            }
        }
        {
            let mut stmt = conn.prepare(
                "SELECT id, COALESCE(title,''), COALESCE(summary,''), COALESCE(content_md,'') FROM wiki_pages",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?;
            let items: Vec<(i64, String, String, String)> = rows.filter_map(|r| r.ok()).collect();
            drop(stmt);
            let mut ins = conn.prepare(
                "INSERT INTO wiki_pages_fts (rowid, title, summary, content_md) VALUES (?1,?2,?3,?4)",
            )?;
            for (id, title, summary, content_md) in items {
                ins.execute(rusqlite::params![
                    id,
                    crate::kb::cjk_spaced(&title),
                    crate::kb::cjk_spaced(&summary),
                    crate::kb::cjk_spaced(&content_md),
                ])?;
            }
        }
        Ok(())
    }

    // ─── 通用访问 ───

    pub fn conn_lock(&self) -> r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager> {
        // 连接池获取失败极罕见；panic 语义与旧 Mutex unwrap 一致
        self.pool
            .get()
            .unwrap_or_else(|e| panic!("获取知识库连接失败: {}", e))
    }

    /// 尝试获取连接（不等待）。用于埋点等非关键路径：池繁忙时直接跳过。
    pub fn try_conn_lock(
        &self,
    ) -> Option<r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>> {
        self.pool
            .get_timeout(std::time::Duration::from_millis(1))
            .ok()
    }

    pub fn db_path_string() -> String {
        Self::db_path().display().to_string()
    }
}

/// 当前登录用户占位（后续接入真实登录态后替换为运行时身份）
pub const CURRENT_USER: i64 = 1;

/// 将 f64 向量序列化为 BLOB（小端 f32，节省空间）
pub fn serialize_embedding(vec: &[f64]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(vec.len() * 4);
    for v in vec {
        buf.extend_from_slice(&(*v as f32).to_le_bytes());
    }
    buf
}

/// 从 BLOB 反序列化 f64 向量
pub fn deserialize_embedding(blob: &[u8]) -> Vec<f64> {
    let n = blob.len() / 4;
    let mut vec = Vec::with_capacity(n);
    for i in 0..n {
        let mut b = [0u8; 4];
        b.copy_from_slice(&blob[i * 4..i * 4 + 4]);
        vec.push(f32::from_le_bytes(b) as f64);
    }
    vec
}

/// 余弦相似度（1 - 余弦距离），范围 [-1, 1]
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_roundtrip() {
        let orig = vec![0.5f64, -1.25, 3.75, 0.0, 100.0];
        let blob = serialize_embedding(&orig);
        // 每维 4 字节（f32 小端）
        assert_eq!(blob.len(), orig.len() * 4);
        let back = deserialize_embedding(&blob);
        assert_eq!(back.len(), orig.len());
        for (a, b) in orig.iter().zip(back.iter()) {
            assert!((a - b).abs() < 1e-6, "f32 精度损失应可忽略: {} vs {}", a, b);
        }
    }

    #[test]
    fn test_serialize_empty() {
        assert!(serialize_embedding(&[]).is_empty());
        assert!(deserialize_embedding(&[]).is_empty());
    }

    #[test]
    fn test_embedding_little_endian() {
        // 1.0f32 的小端字节序为 00 00 80 3F
        let blob = serialize_embedding(&[1.0]);
        assert_eq!(blob, vec![0x00, 0x00, 0x80, 0x3F]);
    }

    #[test]
    fn test_cosine_identical() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_cosine_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((cosine_similarity(&a, &b) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_cosine_opposite() {
        let a = vec![1.0, 2.0];
        let b = vec![-1.0, -2.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_cosine_proportional() {
        let a = vec![1.0, 2.0];
        let b = vec![2.0, 4.0];
        assert!(
            (cosine_similarity(&a, &b) - 1.0).abs() < 1e-9,
            "方向相同则相似度为 1"
        );
    }

    #[test]
    fn test_cosine_invalid_inputs() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0, "空向量返回 0");
        assert_eq!(
            cosine_similarity(&[1.0], &[1.0, 2.0]),
            0.0,
            "维度不一致返回 0"
        );
        assert_eq!(
            cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]),
            0.0,
            "零向量返回 0"
        );
    }
}
