// ============================================================
// 知识库管理 — 分块处理
// 自 handlers.rs 拆分：分块编辑/重新向量化、文档重新处理。
// ============================================================

use crate::kb::db::KbDatabase;
use crate::kb::embed;
use crate::kb::parse::{self, Chunk, ChunkConfig};
use tauri::State;

use super::{log_metric_event, refresh_wiki_for_doc, resolve_embedding_pair, MetricEvent};

/// 编辑单个分块内容：更新文本与全文索引，并重新向量化该分块
#[tauri::command]
pub async fn kb_update_chunk(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    chunk_id: i64,
    content: String,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("分块内容不能为空".to_string());
    }
    let (kb_id, doc_id): (i64, i64) = {
        let conn = db.conn_lock();
        conn.query_row(
            "SELECT kb_id, doc_id FROM document_chunks WHERE id = ?1",
            rusqlite::params![chunk_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "分块不存在".to_string())?
    };
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可编辑分块".to_string());
    }
    log_metric_event(
        &db,
        &MetricEvent {
            uid,
            event_type: "doc_edit_chunk",
            kb_id: Some(kb_id),
            doc_id: Some(doc_id),
            page_id: None,
            session_id: None,
            detail: Some(&chunk_id.to_string()),
        },
    );
    // 更新内容 + 全文索引，清空旧向量
    {
        let conn = db.conn_lock();
        conn.execute(
            "UPDATE document_chunks SET content = ?1, token_count = ?2, embedding_blob = NULL WHERE id = ?3",
            rusqlite::params![content, content.chars().count() as i64, chunk_id],
        )
        .map_err(|e| e.to_string())?;
        crate::kb::db::fts_update_chunk(&conn, chunk_id, &content)?;
    }
    // 重新向量化该分块（失败则保持无向量，可稍后重处理）
    let (embedding_provider, embedding_model) = resolve_embedding_pair(&db, None, None);
    let chunk = parse::Chunk {
        seq: 0,
        content: content.clone(),
        char_start: 0,
        char_end: 0,
        token_count: content.chars().count(),
        section: None,
        page_no: None,
        parent_id: None,
    };
    let embedded = match embed::embed_chunks(
        &db,
        kb_id,
        &[(chunk_id, chunk)],
        embedding_provider.as_deref(),
        embedding_model.as_deref(),
    )
    .await
    {
        Ok((ok, _, _)) => ok,
        Err(e) => {
            log::warn!("分块 {} 重新向量化失败: {}", chunk_id, e);
            return Ok(serde_json::json!({
                "chunkId": chunk_id,
                "docId": doc_id,
                "embedded": 0,
                "content": content,
                "warning": format!("分块内容已保存，但重新向量化失败：{}", e),
            }));
        }
    };
    Ok(
        serde_json::json!({ "chunkId": chunk_id, "docId": doc_id, "embedded": embedded, "content": content }),
    )
}

/// 重新处理文档：读取当前版本原始文件 → 重新解析/分片/向量化
#[tauri::command]
// IPC 契约要求扁平参数（前端固定传参顺序），参数对象收敛不适用于 command 入口
#[allow(clippy::too_many_arguments)]
pub async fn kb_reprocess_document(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    doc_id: i64,
    embedding_provider: Option<String>,
    embedding_model: Option<String>,
    chunk_strategy: Option<String>,
    chunk_size: Option<usize>,
    chunk_overlap: Option<usize>,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let (kb_id, file_type, blob): (i64, String, Vec<u8>) = {
        let conn = db.conn_lock();
        conn.query_row(
            "SELECT d.kb_id, COALESCE(d.file_type, ''), fo.blob_data FROM documents d
             JOIN document_versions dv ON dv.id = d.current_version_id
             JOIN file_objects fo ON fo.id = dv.file_object_id
             WHERE d.id = ?1",
            rusqlite::params![doc_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| format!("文档不存在或缺少原始文件: {}", e))?
    };
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可重新处理文档".to_string());
    }
    // 未显式指定嵌入模型时，使用「模型设置」中的 embedding 配置
    let (embedding_provider, embedding_model) =
        resolve_embedding_pair(&db, embedding_provider, embedding_model);
    // 1) 重新解析 + 分片 + 清空旧分片 + 建任务（CPU 密集 + FTS 写入，移出 tokio worker）
    let (chunks, chunk_ids, _version_id, job_id) = {
        let db_block = (*db).clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<(Vec<Chunk>, Vec<i64>, i64, i64), String> {
            let parsed = parse::parse_document(&file_type, &blob).map_err(|e| e.to_string())?;
            // 分块配置（overlap 上限依赖最终 chunk_size，先算 size 再算 overlap）
            let base_cfg = ChunkConfig::default();
            let chunk_size = chunk_size.map(|sz| sz.max(100)).unwrap_or(base_cfg.chunk_size);
            let cfg = ChunkConfig {
                strategy: chunk_strategy
                    .as_deref()
                    .unwrap_or("recursive")
                    .parse()
                    .unwrap_or(parse::ChunkStrategy::Recursive),
                chunk_size,
                overlap: chunk_overlap
                    .map(|ov| ov.min(chunk_size / 2))
                    .unwrap_or(base_cfg.overlap),
                ..base_cfg
            };
            let chunks = parse::chunk_text(&parsed.text, &cfg);
            if chunks.is_empty() {
                return Err("文档内容为空，无法重新处理".to_string());
            }
            // 2) 清空旧分片（FTS 索引与向量一并删除），并创建处理任务
            let (version_id, job_id): (i64, i64) = {
                let conn = db_block.conn_lock();
                crate::kb::db::fts_delete_chunks_by_doc(&conn, doc_id)?;
                conn.execute("DELETE FROM document_chunks WHERE doc_id = ?1", rusqlite::params![doc_id])
                    .map_err(|e| e.to_string())?;
                conn.execute(
                    "UPDATE documents SET status='processing', process_status='chunking', updated_at=datetime('now') WHERE id = ?1",
                    rusqlite::params![doc_id],
                )
                .map_err(|e| e.to_string())?;
                let version_id = conn.query_row(
                    "SELECT COALESCE(current_version_id, (SELECT MAX(id) FROM document_versions WHERE doc_id = ?2))
                     FROM documents WHERE id = ?1",
                    rusqlite::params![doc_id, doc_id],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0);
                // 防护：文档缺少版本记录时 version_id 会退化为 0，插入 processing_jobs
                // 会触发外键约束（FOREIGN KEY constraint failed）。此时给出明确错误而非裸报错。
                if version_id <= 0 {
                    return Err("文档缺少版本记录，无法重新处理。请删除后重新上传该文档。".to_string());
                }
                conn.execute(
                    "INSERT INTO processing_jobs (doc_id, version_id, stage) VALUES (?1,?2,'chunking')",
                    rusqlite::params![doc_id, version_id],
                )
                .map_err(|e| e.to_string())?;
                (version_id, conn.last_insert_rowid())
            };
            let chunk_ids = parse::save_chunks(&db_block, kb_id, doc_id, version_id, &chunks)
                .map_err(|e| e.to_string())?;
            Ok((chunks, chunk_ids, version_id, job_id))
        })
        .await
        .map_err(|e| format!("重新处理任务失败: {}", e))??
    };
    // 3) 向量化
    {
        let conn = db.conn_lock();
        conn.execute(
            "UPDATE documents SET process_status='embedding' WHERE id = ?1",
            rusqlite::params![doc_id],
        )
        .map_err(|e| e.to_string())?;
    }
    let mut id_chunk_pairs = Vec::new();
    for (idx, id) in chunk_ids.iter().enumerate() {
        id_chunk_pairs.push((*id, chunks[idx].clone()));
    }
    // 未配置嵌入模型（或传入的模型被标记为非嵌入类型被回退）：跳过向量化
    let no_embedding = embedding_provider
        .as_deref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
        || embedding_model
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true);
    let (ok, fail, embed_dim): (usize, usize, Option<usize>) = if no_embedding {
        (0, 0, None)
    } else {
        match embed::embed_chunks(
            &db,
            kb_id,
            &id_chunk_pairs,
            embedding_provider.as_deref(),
            embedding_model.as_deref(),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                // 向量化前置校验失败不再整篇标 failed：内容已解析分片完成，标记 embed_error
                let conn = db.conn_lock();
                let _ = conn.execute(
                    "UPDATE documents SET status='ready', process_status='embed_error', updated_at=datetime('now') WHERE id = ?1",
                    rusqlite::params![doc_id],
                );
                let _ = conn.execute(
                    "UPDATE processing_jobs SET stage='embed_error', progress=1.0, error=?1 WHERE id = ?2",
                    rusqlite::params![e, job_id],
                );
                log::warn!("文档重新处理向量化前置校验失败: doc={} err={}", doc_id, e);
                (0, chunks.len(), None)
            }
        }
    };
    // 4) 完成（解析/分片成功即视为 ready；向量化状态由 process_status 细分）
    {
        let conn = db.conn_lock();
        let (process_status, stage, err_msg) = if no_embedding {
            (
                "no_embedding",
                "done",
                "未配置嵌入模型，文档已解析但未向量化（可正常打开/预览/全文检索）",
            )
        } else if !chunks.is_empty() && ok == 0 {
            (
                "embed_error",
                "embed_error",
                "重新向量化全部失败（请检查嵌入模型配置）",
            )
        } else {
            ("ready", "done", "")
        };
        conn.execute(
            "UPDATE documents SET status='ready', process_status=?1, updated_at=datetime('now') WHERE id = ?2",
            rusqlite::params![process_status, doc_id],
        )
        .map_err(|e| e.to_string())?;
        if err_msg.is_empty() {
            conn.execute(
                "UPDATE processing_jobs SET stage=?1, progress=1.0 WHERE id = ?2",
                rusqlite::params![stage, job_id],
            )
            .map_err(|e| e.to_string())?;
        } else {
            conn.execute(
                "UPDATE processing_jobs SET stage=?1, progress=1.0, error=?2 WHERE id = ?3",
                rusqlite::params![stage, err_msg, job_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    // record_embedding_meta 内部会加锁，必须在锁外调用
    let mut embed_warning: Option<String> = None;
    if let Some(dim) = embed_dim {
        match embed::record_embedding_meta(
            &db,
            kb_id,
            embedding_model.as_deref().unwrap_or(""),
            dim,
        ) {
            Ok(w) => embed_warning = w,
            Err(e) => log::warn!("记录嵌入元数据失败: {}", e),
        }
    }
    // 源文档内容变化 → 自动刷新关联 Wiki 页面的摘要/实体
    if ok > 0 {
        refresh_wiki_for_doc(&db, doc_id);
    }
    log_metric_event(
        &db,
        &MetricEvent {
            uid,
            event_type: "doc_reprocess",
            kb_id: Some(kb_id),
            doc_id: Some(doc_id),
            page_id: None,
            session_id: None,
            detail: None,
        },
    );
    let mut result = serde_json::json!({
        "chunkCount": chunks.len(),
        "embedded": ok,
        "failedEmbed": fail,
    });
    if let Some(ref w) = embed_warning {
        result["embedWarning"] = serde_json::Value::String(w.clone());
    }
    Ok(result)
}
