// ════════════════════════════════════════════════════════════
// Wiki 自动提炼（LLM 生成多页面 Markdown 知识库）
// 自 wiki.rs 拆分：就绪文档筛选、单文档/批量提炼流水线。
// ════════════════════════════════════════════════════════════

use rusqlite::params;

use super::extract::{extract_page_meta, refine_with_llm};
use super::fts::sync_fts_upsert;
use super::mutate::rebuild_links_for_page;
use super::types::WikiGenerateInput;
use super::utils::slugify;
use crate::kb::db::KbDatabase;
use crate::kb::parse;

// ────────────────────────────────────────────────────────────
// 代理自动提炼（LLM 生成多页面 Markdown 知识库）
// ────────────────────────────────────────────────────────────

/// 列出待提炼的已就绪文档 (doc_id, file_type, title)
pub fn list_ready_docs(
    db: &KbDatabase,
    kb_id: i64,
    doc_id: Option<i64>,
) -> Result<Vec<(i64, String, String)>, String> {
    let docs: Vec<(i64, String, String)> = {
        let conn = db.conn_lock();
        let mut stmt = conn
            .prepare(
                "SELECT d.id, COALESCE(d.file_type,'txt'), COALESCE(d.title,'')
                 FROM documents d
                 WHERE d.kb_id = ?1 AND d.status = 'ready'",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![kb_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    match doc_id {
        Some(did) => Ok(docs.into_iter().filter(|(id, _, _)| *id == did).collect()),
        None => Ok(docs),
    }
}

/// 对知识库（或指定文档）执行自动提炼，生成相互链接的 wiki 页面。
/// 返回生成的页面 id 列表。
pub async fn generate(
    db: &KbDatabase,
    uid: i64,
    input: &WikiGenerateInput,
) -> Result<Vec<i64>, String> {
    let docs = list_ready_docs(db, input.kb_id, input.doc_id)?;
    if docs.is_empty() {
        return Err(if input.doc_id.is_some() {
            "指定文档不存在或未就绪".to_string()
        } else {
            "知识库内没有已就绪（ready）的文档，请先上传并完成处理".to_string()
        });
    }
    generate_with_jobs(
        db.clone(),
        uid,
        input.kb_id,
        docs,
        input.provider_id.as_deref(),
        input.model.as_deref(),
    )
    .await
}

/// 批量提炼流水线（供后台任务调用）：逐文档创建 processing_jobs 任务，
/// 提炼成功标记 done，失败标记 failed 并记录 processing_logs。
pub async fn generate_with_jobs(
    db: KbDatabase,
    uid: i64,
    kb_id: i64,
    docs: Vec<(i64, String, String)>,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<Vec<i64>, String> {
    let mark_job_failed = |job_id: i64, err: &str| {
        let conn = db.conn_lock();
        let _ = conn.execute(
            "UPDATE processing_jobs SET stage='failed', progress=1.0, error=?1 WHERE id = ?2",
            params![err, job_id],
        );
        let _ = conn.execute(
            "INSERT INTO processing_logs (job_id, level, message) VALUES (?1,'error',?2)",
            params![job_id, err],
        );
        log::error!("Wiki 提炼失败: job={} err={}", job_id, err);
    };

    let mut created = Vec::new();
    for (doc_id, file_type, doc_title) in docs {
        // 停止检查：批量取消标记已置位（kb_stop_processing）时，终止整个批量
        {
            let conn = db.conn_lock();
            let cancelled: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM kb_chunk_settings WHERE key = ?1 AND value = '1'",
                    params![format!("generate_cancel_{}", kb_id)],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if cancelled > 0 {
                break;
            }
        }
        // 创建处理任务
        let job_id: i64 = {
            let conn = db.conn_lock();
            conn.execute(
                "INSERT INTO processing_jobs (doc_id, version_id, stage) VALUES (?1,?2,'generating')",
                params![doc_id, {
                    conn.query_row(
                        "SELECT current_version_id FROM documents WHERE id = ?1",
                        params![doc_id],
                        |r| r.get::<_, Option<i64>>(0),
                    )
                    .unwrap_or(None)
                }],
            )
            .map_err(|e| e.to_string())?;
            let _ = conn.execute(
                "UPDATE documents SET process_status='generating' WHERE id = ?1",
                params![doc_id],
            );
            conn.last_insert_rowid()
        };
        // 读取文档全文
        let text = {
            let conn = db.conn_lock();
            let blob: Option<Vec<u8>> = conn
                .query_row(
                    "SELECT fo.blob_data FROM document_versions dv
                     JOIN file_objects fo ON fo.id = dv.file_object_id
                     WHERE dv.doc_id = ?1 ORDER BY dv.version_no DESC LIMIT 1",
                    params![doc_id],
                    |r| r.get::<_, Vec<u8>>(0),
                )
                .ok();
            match blob {
                Some(b) => parse::parse_document(&file_type, &b)
                    .map(|p| p.text)
                    .unwrap_or_default(),
                None => String::new(),
            }
        };
        if text.trim().is_empty() {
            mark_job_failed(job_id, "文档正文为空，无法提炼");
            continue;
        }
        // 调用 LLM 提炼
        let pages = match refine_with_llm(&text, &doc_title, provider_id, model).await {
            Ok(p) => p,
            Err(e) => {
                mark_job_failed(job_id, &e);
                continue;
            }
        };
        let page_count = pages.len();
        // 停止检查：任务已被手动停止（kb_stop_processing 标记为 failed）时不落库
        {
            let conn = db.conn_lock();
            let still_active: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM processing_jobs WHERE id = ?1 AND stage = 'generating'",
                    params![job_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if still_active == 0 {
                let _ = conn.execute(
                    "UPDATE documents SET process_status='ready' WHERE id = ?1",
                    params![doc_id],
                );
                continue;
            }
        }
        // 落库（同一文档重复提炼时按 kb_id+slug 覆盖旧页面）
        {
            let conn = db.conn_lock();
            for p in pages {
                if p.title.trim().is_empty() {
                    continue;
                }
                let slug = slugify(&p.title);
                conn.execute(
                    "INSERT INTO wiki_pages (kb_id, doc_id, title, slug, summary, content_md, status, created_by, updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,'published',?7,datetime('now'))
                     ON CONFLICT(kb_id, slug) DO UPDATE SET
                        doc_id = excluded.doc_id,
                        title = excluded.title,
                        summary = excluded.summary,
                        content_md = excluded.content_md,
                        status = 'published',
                        updated_at = datetime('now')",
                    params![kb_id, doc_id, p.title, slug, p.summary, p.content, uid],
                )
                .map_err(|e| e.to_string())?;
                // upsert 命中更新时 last_insert_rowid 行为依赖 SQLite 版本，显式取回页面 id
                let pid: i64 = conn
                    .query_row(
                        "SELECT id FROM wiki_pages WHERE kb_id = ?1 AND slug = ?2",
                        params![kb_id, slug],
                        |r| r.get(0),
                    )
                    .map_err(|e| format!("读取 Wiki 页面 id 失败: {}", e))?;
                sync_fts_upsert(&conn, pid)?;
                created.push(pid);
            }
        }
        // 任务完成
        {
            let conn = db.conn_lock();
            let _ = conn.execute(
                "UPDATE processing_jobs SET stage='done', progress=1.0 WHERE id = ?1",
                params![job_id],
            );
            let _ = conn.execute(
                "UPDATE documents SET process_status='ready' WHERE id = ?1",
                params![doc_id],
            );
            let _ = conn.execute(
                "INSERT INTO processing_logs (job_id, level, message) VALUES (?1,'info',?2)",
                params![job_id, format!("Wiki 提炼完成：{} 个页面", page_count)],
            );
        }
    }

    // 批量结束（全部完成或被手动停止）后清除取消标记
    {
        let conn = db.conn_lock();
        let _ = conn.execute(
            "DELETE FROM kb_chunk_settings WHERE key = ?1",
            params![format!("generate_cancel_{}", kb_id)],
        );
    }
    // 全部页面落库后统一重建链接（[[标题]] 可能引用其他文档提炼的页面）
    {
        let conn = db.conn_lock();
        let all: Vec<(i64, i64, String)> = conn
            .prepare("SELECT id, kb_id, content_md FROM wiki_pages WHERE kb_id = ?1")
            .map_err(|e| e.to_string())?
            .query_map(params![kb_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        for (pid, kbid, md) in all {
            rebuild_links_for_page(&conn, pid, kbid, &md)?;
        }
    }
    // 提炼完成后自动补充摘要与实体提取（LLM 严格基于正文；锁已释放）
    for pid in &created {
        if let Err(e) = extract_page_meta(&db, uid, *pid, provider_id, model).await {
            log::warn!("提炼后摘要/实体提取失败 page={} err={}", pid, e);
        }
    }
    Ok(created)
}
