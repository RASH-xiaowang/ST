// ════════════════════════════════════════════════════════════
// Wiki 全文检索（FTS5 external content）
// 自 wiki.rs 拆分：索引幂等写入 / 全量重建 / 安全查询转换 / BM25 检索。
// ════════════════════════════════════════════════════════════

use rusqlite::params;

use super::types::WikiPageItem;
use super::utils::OptionNone;
use crate::kb::db::KbDatabase;

// ────────────────────────────────────────────────────────────
// 全文检索（FTS5，external content 外链 wiki_pages）
// ────────────────────────────────────────────────────────────

/// 将页面幂等写入全文索引（先删后插，供 create/update/generate 共用）
pub(crate) fn sync_fts_upsert(conn: &rusqlite::Connection, page_id: i64) -> Result<(), String> {
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT COALESCE(title,''), COALESCE(summary,''), COALESCE(content_md,'') FROM wiki_pages WHERE id = ?1",
            params![page_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    if let Some((title, summary, content)) = row {
        crate::kb::db::fts_update_wiki_page(conn, page_id, &title, &summary, &content)?;
    }
    Ok(())
}

/// 重建全部页面的全文索引（索引落后于数据时由 search_pages 自动触发）
pub fn rebuild_fts(db: &KbDatabase) -> Result<(), String> {
    let conn = db.conn_lock();
    conn.execute("DELETE FROM wiki_pages_fts", [])
        .map_err(|e| e.to_string())?;
    let rows: Vec<(i64, String, String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, COALESCE(title,''), COALESCE(summary,''), COALESCE(content_md,'') FROM wiki_pages",
            )
            .map_err(|e| e.to_string())?;
        let q = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map_err(|e| e.to_string())?;
        q.filter_map(|r| r.ok()).collect()
    };
    for (pid, title, summary, content) in rows {
        crate::kb::db::fts_insert_wiki_page(&conn, pid, &title, &summary, &content)?;
    }
    Ok(())
}

/// 将用户查询转为 FTS5 安全查询（共享实现，见 crate::kb::fts_safe_query；
/// 支持中文整句按字 OR 召回，避免中文整句作为单一短语导致 0 命中）
fn fts_match_query(query: &str) -> String {
    crate::kb::fts_safe_query(query)
}

/// 在知识库内用 BM25 检索 Wiki 页面，按相关度返回（score 越低越相关）
pub fn search_pages(
    db: &KbDatabase,
    kb_id: i64,
    query: &str,
    limit: usize,
) -> Result<Vec<WikiPageItem>, String> {
    let q = fts_match_query(query);
    if q.is_empty() {
        return Ok(Vec::new());
    }
    // 索引与数据数量不一致时自动重建（新建表 / 历史数据 / 手工 SQL 变更 / 历史遗留孤儿索引）
    let need_rebuild = {
        let conn = db.conn_lock();
        let n_pages: i64 = conn
            .query_row("SELECT COUNT(*) FROM wiki_pages", [], |r| r.get(0))
            .unwrap_or(0);
        let n_fts: i64 = conn
            .query_row("SELECT COUNT(*) FROM wiki_pages_fts", [], |r| r.get(0))
            .unwrap_or(0);
        n_pages > 0 && n_fts != n_pages
    };
    if need_rebuild {
        rebuild_fts(db)?;
    }
    let conn = db.conn_lock();
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.kb_id, p.dir_id, p.doc_id, COALESCE(d.title,''), p.title, p.slug, p.summary, p.status,
                    p.created_at, p.updated_at,
                    (SELECT COUNT(*) FROM wiki_links wl WHERE wl.from_page_id = p.id) AS out_links,
                    (SELECT COUNT(*) FROM wiki_links wl WHERE wl.to_page_id = p.id) AS in_links,
                    (SELECT COUNT(*) FROM wiki_page_entities e WHERE e.page_id = p.id) AS entity_count
             FROM wiki_pages_fts
             JOIN wiki_pages p ON p.id = wiki_pages_fts.rowid
             LEFT JOIN documents d ON d.id = p.doc_id
             WHERE wiki_pages_fts MATCH ?1 AND p.kb_id = ?2
             ORDER BY bm25(wiki_pages_fts) ASC
             LIMIT ?3",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![q, kb_id, limit as i64], |row| {
            Ok(WikiPageItem {
                id: row.get(0)?,
                kb_id: row.get(1)?,
                dir_id: row.get(2)?,
                doc_id: row.get(3)?,
                doc_title: row.get::<_, String>(4)?.trim().to_string().into_none(),
                title: row.get(5)?,
                slug: row.get(6)?,
                summary: row.get(7)?,
                status: row.get(8)?,
                out_links: row.get(11)?,
                in_links: row.get(12)?,
                entity_count: row.get(13)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}
