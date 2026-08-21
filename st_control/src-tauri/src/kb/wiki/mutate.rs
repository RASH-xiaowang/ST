// ════════════════════════════════════════════════════════════
// Wiki 写入（CRUD + 链接图维护）
// 自 wiki.rs 拆分：页面创建/更新/删除、出链重建与整库链接刷新。
// ════════════════════════════════════════════════════════════

use rusqlite::params;

use super::fts::sync_fts_upsert;
use super::types::WikiPageInput;
use super::utils::{extract_wiki_links, slugify};
use crate::kb::db::KbDatabase;

// ────────────────────────────────────────────────────────────
// 写入
// ────────────────────────────────────────────────────────────

/// 手工创建页面
pub fn create_page(db: &KbDatabase, input: &WikiPageInput, uid: i64) -> Result<i64, String> {
    let slug = slugify(&input.title);
    let conn = db.conn_lock();
    let dup: Option<String> = conn
        .query_row(
            "SELECT slug FROM wiki_pages WHERE kb_id = ?1 AND slug = ?2",
            params![input.kb_id, slug],
            |r| r.get(0),
        )
        .ok();
    if dup.is_some() {
        return Err("同知识库已存在同标题页面".to_string());
    }
    conn.execute(
        "INSERT INTO wiki_pages (kb_id, doc_id, title, slug, summary, content_md, status, created_by, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,'draft',?7,datetime('now'))",
        params![
            input.kb_id,
            input.doc_id,
            input.title,
            slug,
            input.summary.clone().unwrap_or_default(),
            input.content_md.clone().unwrap_or_default(),
            uid
        ],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    // 写入全文索引（FTS5）
    sync_fts_upsert(&conn, id)?;
    // 重建整库链接：新页面会获得既有页面的反向链接，自身出链也能解析到目标页
    rebuild_kb_links(&conn, input.kb_id)?;
    Ok(id)
}

/// 更新页面内容（标题变更会重建 slug，正文变更会重建链接）
pub fn update_page(db: &KbDatabase, page_id: i64, input: &WikiPageInput) -> Result<(), String> {
    let conn = db.conn_lock();
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM wiki_pages WHERE id = ?1",
            params![page_id],
            |_| Ok(()),
        )
        .is_ok();
    if !exists {
        return Err("页面不存在".to_string());
    }
    let slug = slugify(&input.title);
    // 标题变更后可能与其他页面 slug 冲突，先给出友好错误（UNIQUE 约束错误信息不直观）
    let dup: Option<i64> = conn
        .query_row(
            "SELECT id FROM wiki_pages WHERE kb_id = ?1 AND slug = ?2 AND id != ?3",
            params![input.kb_id, slug, page_id],
            |r| r.get(0),
        )
        .ok();
    if dup.is_some() {
        return Err("同知识库已存在同标题页面".to_string());
    }
    conn.execute(
        "UPDATE wiki_pages SET title = ?1, slug = ?2,
                summary = COALESCE(?3, summary),
                content_md = COALESCE(?4, content_md),
                updated_at = datetime('now')
         WHERE id = ?5",
        params![input.title, slug, input.summary, input.content_md, page_id],
    )
    .map_err(|e| e.to_string())?;
    // 同步全文索引
    sync_fts_upsert(&conn, page_id)?;
    // 重建整库链接：标题变更 / 正文变更后双向连接保持一致
    rebuild_kb_links(&conn, input.kb_id)?;
    Ok(())
}

/// 删除页面（级联删除链接，并清理全文索引）
pub fn delete_page(db: &KbDatabase, page_id: i64) -> Result<(), String> {
    let conn = db.conn_lock();
    conn.execute(
        "DELETE FROM wiki_pages_fts WHERE rowid = ?1",
        params![page_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM wiki_pages WHERE id = ?1", params![page_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 根据 Markdown 正文中的 [[标题]] 语法重建页面的出链
pub(crate) fn rebuild_links_for_page(
    conn: &rusqlite::Connection,
    page_id: i64,
    kb_id: i64,
    content_md: &str,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM wiki_links WHERE from_page_id = ?1",
        params![page_id],
    )
    .map_err(|e| e.to_string())?;
    let targets = extract_wiki_links(content_md);
    for (title, count) in targets {
        // 精确匹配标题；找不到则尝试 slug 匹配
        let to_page: Option<i64> = conn
            .query_row(
                "SELECT id FROM wiki_pages WHERE kb_id = ?1 AND (title = ?2 OR slug = ?2)",
                params![kb_id, title],
                |r| r.get(0),
            )
            .ok();
        if let Some(to) = to_page {
            if to != page_id {
                conn.execute(
                    "INSERT INTO wiki_links (kb_id, from_page_id, to_page_id, link_type, weight)
                     VALUES (?1,?2,?3,'related',?4)
                     ON CONFLICT(from_page_id, to_page_id, link_type) DO UPDATE SET weight = ?4",
                    params![kb_id, page_id, to, count.min(5) as f64],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// 重建知识库内全部页面的出链（创建/更新页面后调用，保证双向连接一致）
pub(crate) fn rebuild_kb_links(conn: &rusqlite::Connection, kb_id: i64) -> Result<(), String> {
    let all: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, content_md FROM wiki_pages WHERE kb_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![kb_id], |r| Ok((r.get(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    for (pid, md) in all {
        rebuild_links_for_page(conn, pid, kb_id, &md)?;
    }
    Ok(())
}
