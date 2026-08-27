// ============================================================
// 向量化服务 (Embedding)
// 复用 llm::handlers::create_embedding 生成向量，写入 document_chunks.embedding_blob
// 批量处理：并发多批调用（带重试与限流），返回成功/失败统计
// ============================================================

use crate::kb::db::{serialize_embedding, KbDatabase};
use crate::kb::parse::Chunk;
use rusqlite::params;
use std::sync::Arc;

/// 每个请求最多嵌入的分片数
const EMBED_BATCH_SIZE: usize = 16;
/// 最大并发批次数（控制 API 并发压力）
const EMBED_CONCURRENCY: usize = 3;

/// 检查知识库已有向量与当前嵌入模型是否一致。
pub fn ensure_embedding_compatible(db: &KbDatabase, kb_id: i64, model: &str) -> Result<(), String> {
    if model.trim().is_empty() {
        return Ok(());
    }
    let conn = db.conn_lock();
    let (stored_model, dim): (Option<String>, Option<i64>) = conn
        .query_row(
            "SELECT embedding_model, embedding_dim FROM knowledge_bases WHERE id = ?1",
            params![kb_id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<i64>>(1)?)),
        )
        .map_err(|e| format!("读取知识库嵌入元数据失败: {}", e))?;
    drop(conn);
    let Some(dim) = dim else {
        return Ok(());
    };
    let Some(stored) = stored_model else {
        return Ok(());
    };
    if stored.trim().is_empty() {
        return Ok(());
    }
    if stored.trim() != model.trim() {
        return Err(format!(
            "知识库已使用嵌入模型「{}」（维度 {}），当前模型「{}」与其不一致。\
             混用嵌入模型会导致向量检索失效，请先在设置中恢复原模型，\
             或对知识库内全部文档执行重处理以统一模型。",
            stored.trim(),
            dim,
            model.trim()
        ));
    }
    Ok(())
}

/// 单批次嵌入 + 重试（指数退避，最多 3 次）
async fn embed_batch_with_retry(
    provider_id: Option<&str>,
    model: Option<&str>,
    texts: &[String],
) -> Result<Vec<Vec<f64>>, String> {
    let mut attempt = 0usize;
    loop {
        attempt += 1;
        match crate::llm::client::create_embeddings_batch(provider_id, model, texts).await {
            Ok(vecs) => return Ok(vecs),
            Err(e) => {
                if attempt >= 3 {
                    return Err(e.to_string());
                }
                let delay =
                    std::time::Duration::from_millis(300 * (1usize << (attempt - 1)) as u64);
                log::warn!(
                    "分片批次嵌入第 {} 次失败（{}/3），{}ms 后重试: {}",
                    attempt,
                    3,
                    delay.as_millis(),
                    e
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// 批量嵌入分片并写库（并发多批处理，带重试与限流）
/// 跳过已有向量的分片（重处理时复用，节省 API 调用）。
/// 返回 (成功数, 失败数, 首个成功嵌入的向量维度)
pub async fn embed_chunks(
    db: &KbDatabase,
    kb_id: i64,
    chunks: &[(i64, Chunk)],
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<(usize, usize, Option<usize>), String> {
    if let Some(m) = model {
        ensure_embedding_compatible(db, kb_id, m)?;
    }
    if chunks.is_empty() {
        return Ok((0, 0, None));
    }

    // 跳过已有向量的分片（重处理时复用，节省 API 调用）
    let existing_ids: std::collections::HashSet<i64> = {
        let conn = db.conn_lock();
        let ids: Vec<i64> = chunks.iter().map(|(id, _)| *id).collect();
        let ph = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id FROM document_chunks WHERE id IN ({}) AND embedding_blob IS NOT NULL",
            ph
        );
        let binds: Vec<&dyn rusqlite::types::ToSql> = ids
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(binds.as_slice(), |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let skipped = existing_ids.len();

    // 过滤掉已有向量的分片
    let chunks_to_embed: Vec<&(i64, Chunk)> = chunks
        .iter()
        .filter(|(id, _)| !existing_ids.contains(id))
        .collect();

    if chunks_to_embed.is_empty() {
        log::info!("所有 {} 个分片已有向量，跳过向量化", skipped);
        return Ok((0, 0, None));
    }

    let provider_owned = provider_id.map(|s| s.to_string());
    let model_owned = model.map(|s| s.to_string());

    // 分批：每批 EMBED_BATCH_SIZE 个分片
    let batches: Vec<Vec<(i64, String)>> = chunks_to_embed
        .chunks(EMBED_BATCH_SIZE)
        .map(|batch| {
            batch
                .iter()
                .map(|(id, c)| (*id, c.content.clone()))
                .collect()
        })
        .collect();

    let total_batches = batches.len();
    let semaphore = Arc::new(tokio::sync::Semaphore::new(EMBED_CONCURRENCY));
    let mut join_set = tokio::task::JoinSet::new();

    for (batch_idx, batch) in batches.into_iter().enumerate() {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let pid = provider_owned.clone();
        let mid = model_owned.clone();
        join_set.spawn(async move {
            let _permit = permit;
            let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
            let result = embed_batch_with_retry(pid.as_deref(), mid.as_deref(), &texts).await;
            (batch_idx, batch, result)
        });
    }

    let mut ok = 0usize;
    let mut fail = 0usize;
    let mut embed_dim: Option<usize> = None;
    let mut all_writes: Vec<(i64, Vec<u8>)> = Vec::new();

    while let Some(res) = join_set.join_next().await {
        let (_, batch, result) = res.map_err(|e| format!("嵌入任务 panic: {}", e))?;
        match result {
            Ok(vecs) => {
                for ((chunk_id, _), vec) in batch.iter().zip(vecs.iter()) {
                    let blob = serialize_embedding(vec);
                    all_writes.push((*chunk_id, blob));
                    ok += 1;
                    if embed_dim.is_none() {
                        embed_dim = Some(vec.len());
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    "分片批次嵌入失败（{} 个分片，已重试 3 次）: {}",
                    batch.len(),
                    e
                );
                fail += batch.len();
            }
        }
    }

    // 批量写入 DB
    if !all_writes.is_empty() {
        let conn = db.conn_lock();
        conn.execute_batch("BEGIN TRANSACTION")
            .map_err(|e| e.to_string())?;
        {
            let mut stmt = conn
                .prepare("UPDATE document_chunks SET embedding_blob = ?1 WHERE id = ?2")
                .map_err(|e| e.to_string())?;
            for (chunk_id, blob) in &all_writes {
                let _ = stmt.execute(params![blob, chunk_id]);
            }
        }
        conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
        // 通知向量索引缓存失效（下次检索时重新加载）
        crate::vector_index::VECTOR_INDEX.invalidate();
    }

    log::info!(
        "向量化完成: 批次={} 并发={} 成功={} 失败={} 跳过={} 维度={:?}",
        total_batches,
        EMBED_CONCURRENCY,
        ok,
        fail,
        skipped,
        embed_dim
    );
    Ok((ok, fail, embed_dim))
}

/// 更新知识库记录的嵌入模型与维度（首条成功嵌入时调用）。
/// 返回 Ok(None) 表示正常更新；Ok(Some(warning)) 表示维度不一致的告警信息（需透传到前端）。
pub fn record_embedding_meta(
    db: &KbDatabase,
    kb_id: i64,
    model: &str,
    dim: usize,
) -> Result<Option<String>, String> {
    let conn = db.conn_lock();
    // 知识库已有向量维度时，仅当维度一致才更新模型名；维度变化说明混用了不同嵌入模型，
    // 此时覆盖元数据会让检索维度记录失真（不同维度向量余弦相似度恒为 0），故保持原值并告警。
    let cur_dim: Option<i64> = conn
        .query_row(
            "SELECT embedding_dim FROM knowledge_bases WHERE id = ?1",
            params![kb_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    if let Some(cur) = cur_dim {
        if cur as usize != dim {
            let warning = format!(
                "知识库已使用嵌入维度 {}，本次嵌入维度 {} 不一致（疑似混用不同嵌入模型），\
                 新向量未写入元数据。建议对全部文档执行重处理以统一模型。",
                cur, dim
            );
            log::warn!("知识库 {} {}", kb_id, warning);
            return Ok(Some(warning));
        }
        conn.execute(
            "UPDATE knowledge_bases SET embedding_model = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![model, kb_id],
        )
        .map_err(|e| e.to_string())?;
        return Ok(None);
    }
    conn.execute(
        "UPDATE knowledge_bases SET embedding_model = ?1, embedding_dim = ?2, updated_at = datetime('now') WHERE id = ?3",
        params![model, dim as i64, kb_id],
    ).map_err(|e| e.to_string())?;
    Ok(None)
}
