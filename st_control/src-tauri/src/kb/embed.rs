// ============================================================
// 向量化服务 (Embedding)
// 复用 llm::handlers::create_embedding 生成向量，写入 document_chunks.embedding_blob
// 批量处理：逐分片嵌入（带简单限流），返回成功/失败统计
// ============================================================

use crate::kb::db::{serialize_embedding, KbDatabase};
use crate::kb::parse::Chunk;
use rusqlite::params;

/// 每个请求最多嵌入的分片数（一个请求一个批量，显著降低请求数，
/// 规避 SiliconFlow 等免费档 QPS/并发限制导致的连接被丢弃）
const EMBED_BATCH_SIZE: usize = 16;

/// 检查知识库已有向量与当前嵌入模型是否一致。
/// 同一知识库混用不同嵌入模型时，向量维度/分布不一致会导致向量检索结果失真，
/// 因此在写入前拦截，避免「嵌入成功但检索不到」的静默失效。
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
    // 首次嵌入：直接放行
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

/// 批量嵌入分片并写库（分批调用，带重试与批次间隔）
/// 返回 (成功数, 失败数, 首个成功嵌入的向量维度)
pub async fn embed_chunks(
    db: &KbDatabase,
    kb_id: i64,
    chunks: &[(i64, Chunk)],
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<(usize, usize, Option<usize>), String> {
    // 写入前先做模型一致性校验，避免同一知识库混用不同嵌入模型
    if let Some(m) = model {
        ensure_embedding_compatible(db, kb_id, m)?;
    }
    let mut ok = 0usize;
    let mut fail = 0usize;
    let mut embed_dim: Option<usize> = None;
    for batch in chunks.chunks(EMBED_BATCH_SIZE) {
        let texts: Vec<String> = batch.iter().map(|(_, c)| c.content.clone()).collect();
        // 批次级重试：瞬时网络抖动 / 限流时自动重试（最多 3 次，指数退避）
        let mut attempt = 0usize;
        let batch_result = loop {
            attempt += 1;
            match crate::llm::client::create_embeddings_batch(provider_id, model, &texts).await {
                Ok(vecs) => break Ok(vecs),
                Err(e) => {
                    if attempt >= 3 {
                        break Err(e.to_string());
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
        };
        match batch_result {
            Ok(vecs) => {
                for ((chunk_id, _), vec) in batch.iter().zip(vecs.iter()) {
                    let blob = serialize_embedding(vec);
                    let conn = db.conn_lock();
                    conn.execute(
                        "UPDATE document_chunks SET embedding_blob = ?1 WHERE id = ?2",
                        params![blob, chunk_id],
                    )
                    .map_err(|e| e.to_string())?;
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
        // 批次间小幅间隔，降低 QPS 压力（免费档模型限流较严）
        if chunks.len() > EMBED_BATCH_SIZE {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }
    Ok((ok, fail, embed_dim))
}

/// 更新知识库记录的嵌入模型与维度（首条成功嵌入时调用）
pub fn record_embedding_meta(
    db: &KbDatabase,
    kb_id: i64,
    model: &str,
    dim: usize,
) -> Result<(), String> {
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
            log::warn!(
                "知识库 {} 已有嵌入维度 {}，本次嵌入维度 {} 不一致（疑似混用不同嵌入模型），保持原维度记录",
                kb_id, cur, dim
            );
            return Ok(());
        }
        conn.execute(
            "UPDATE knowledge_bases SET embedding_model = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![model, kb_id],
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    conn.execute(
        "UPDATE knowledge_bases SET embedding_model = ?1, embedding_dim = ?2, updated_at = datetime('now') WHERE id = ?3",
        params![model, dim as i64, kb_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}
