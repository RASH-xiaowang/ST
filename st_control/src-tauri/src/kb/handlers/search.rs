// ============================================================
// 知识库管理 — 检索 / RAG
// 自 handlers.rs 拆分：BM25/向量混合检索、RAG 问答与流式、高亮、检索历史。
// ============================================================

use crate::kb::db::KbDatabase;
use crate::kb::rag;
use crate::kb::retrieval::{self, visible_kb_ids, RetrievedChunk};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{faq_match, log_metric_event, read_model_setting, resolve_embedding_pair, MetricEvent};

// ─── 检索 ───

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchInput {
    pub user_id: i64,
    pub kb_id: Option<i64>,
    pub query: String,
    pub top_k: Option<usize>,
    pub mode: Option<String>, // hybrid / vector / bm25
    pub provider_id: Option<String>,
    pub model: Option<String>,
}

#[tauri::command]
pub async fn kb_search(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    mut input: SearchInput,
) -> Result<Vec<RetrievedChunk>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    input.user_id = uid;
    let top_k = input.top_k.unwrap_or(10).clamp(1, 100);
    let mode = input.mode.clone().unwrap_or_else(|| "hybrid".to_string());
    let visible = match input.kb_id {
        Some(id) => {
            // 显式指定知识库时，必须校验可见性（防止越权检索）
            if !crate::kb::retrieval::can_access_kb(&db, id, uid) {
                return Err("无权限：你无权访问该知识库".to_string());
            }
            vec![id]
        }
        None => visible_kb_ids(&db, uid),
    };
    if visible.is_empty() {
        return Err("当前用户无可访问的知识库".to_string());
    }
    // 查询向量使用「模型设置」中的 Embeddings 配置（优先），否则退回调用方传入的模型
    let (emb_provider, emb_model) = {
        let conn = db.conn_lock();
        read_model_setting(&conn, "embedding")
            .map(|(p, m)| (Some(p), Some(m)))
            .unwrap_or((input.provider_id.clone(), input.model.clone()))
    };
    let results = match mode.as_str() {
        "vector" => {
            vector_search_wrap(
                &db,
                &input.query,
                &visible,
                top_k,
                emb_provider.as_deref(),
                emb_model.as_deref(),
            )
            .await?
        }
        "bm25" => retrieval::bm25_search(&db, &input.query, &visible, top_k)?,
        _ => {
            let b = retrieval::bm25_search(&db, &input.query, &visible, top_k)?;
            // FTS 候选池预筛：只对候选分片做向量精排，避免每次搜索全量载入所有 embedding
            let candidates = retrieval::fts_candidate_ids(
                &db,
                &input.query,
                &visible,
                (top_k * 30).clamp(100, 2000),
            )?;
            match retrieval::vector_search_in_candidates(
                &db,
                &input.query,
                &visible,
                &candidates,
                top_k,
                emb_provider.as_deref(),
                emb_model.as_deref(),
            )
            .await
            {
                Ok(v) => retrieval::rrf_fuse(v, b, 60),
                Err(e) => {
                    // 未配置/不可用的嵌入模型时降级为纯 BM25，保证检索按钮可用
                    log::warn!("向量检索不可用，混合检索降级为 BM25: {}", e);
                    if b.is_empty() {
                        return Err(e);
                    }
                    b
                }
            }
        }
    };
    // Rerank：配置了重排序模型时，对检索结果智能重排序（失败保留原顺序）
    let results = crate::kb::retrieval::rerank_chunks(&db, &input.query, results).await;
    // 记录检索历史
    log_search(
        &db,
        input.kb_id,
        uid,
        &input.query,
        &mode,
        results.len() as i64,
    );
    // 埋点：检索事件（hitCount 供召回率统计）
    let detail = serde_json::json!({ "mode": mode, "topK": top_k, "hitCount": results.len() });
    log_metric_event(
        &db,
        &MetricEvent {
            uid,
            event_type: "search",
            kb_id: input.kb_id,
            doc_id: None,
            page_id: None,
            session_id: None,
            detail: Some(&detail.to_string()),
        },
    );
    Ok(results)
}

async fn vector_search_wrap(
    db: &KbDatabase,
    query: &str,
    visible: &[i64],
    top_k: usize,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<Vec<RetrievedChunk>, String> {
    retrieval::vector_search(db, query, visible, top_k, provider, model).await
}

// ─── RAG ───

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RagInput {
    pub user_id: i64,
    pub kb_id: Option<i64>,
    pub query: String,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub top_k: Option<usize>,
    /// 检索模式：hybrid（默认，混合）/ vector / bm25
    pub mode: Option<String>,
    pub session_id: Option<i64>, // 可选：落到指定 QA 会话
    /// 人工编辑后的检索片段覆盖（指定 chunk 与可选改写内容，跳过自动检索）
    #[serde(default)]
    pub chunks: Option<Vec<RagChunkOverride>>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RagChunkOverride {
    pub chunk_id: i64,
    pub content: Option<String>,
}

#[tauri::command]
pub async fn kb_rag(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    mut input: RagInput,
) -> Result<rag::RagAnswer, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    input.user_id = uid;
    let top_k = input.top_k.unwrap_or(5).clamp(1, 100);
    // 显式指定知识库时校验可见性（rag_answer 内部会再走 visible_kb_ids）
    if let Some(id) = input.kb_id {
        if !crate::kb::retrieval::can_access_kb(&db, id, uid) {
            return Err("无权限：你无权访问该知识库".to_string());
        }
    }
    let mode = input.mode.clone().unwrap_or_else(|| "hybrid".to_string());
    // 未显式指定模型时，使用「模型设置」中的推理模型（问答生成）
    if input.provider_id.is_none() && input.model.is_none() {
        if let Some((p, m)) = {
            let conn = db.conn_lock();
            read_model_setting(&conn, "inference")
        } {
            input.provider_id = Some(p);
            input.model = Some(m);
        }
    }
    // 检索查询向量使用「模型设置」中的 Embeddings 配置（与回答生成模型分离）
    let (emb_provider, emb_model) = resolve_embedding_pair(&db, None, None);
    // 人工编辑片段覆盖（chunkId + 可选改写内容）
    let chunk_overrides: Vec<(i64, Option<String>)> = input
        .chunks
        .clone()
        .map(|v| v.into_iter().map(|c| (c.chunk_id, c.content)).collect())
        .unwrap_or_default();
    let chunk_overrides_opt = if chunk_overrides.is_empty() {
        None
    } else {
        Some(chunk_overrides.as_slice())
    };
    // FAQ 优先：命中标准问答对时直接给出答案，不走 RAG 检索与生成
    let mut faq_hit: Option<String> = None;
    let ans = if let Some(kid) = input.kb_id {
        let matched = {
            let conn = db.conn_lock();
            faq_match(&conn, kid, &input.query)
        };
        if let Some((question, answer)) = matched {
            faq_hit = Some(question.clone());
            rag::RagAnswer {
                answer: answer.clone(),
                context: vec![rag::RagContextItem {
                    chunk_id: 0,
                    doc_id: 0,
                    kb_id: kid,
                    content: answer.clone(),
                    score: 1.0,
                    doc_title: format!("FAQ：{}", question),
                    section: Some("FAQ 问答".to_string()),
                    page_no: None,
                }],
                model: input.model.clone().unwrap_or_default(),
                provider: input.provider_id.clone().unwrap_or_default(),
            }
        } else {
            rag::rag_answer(
                &db,
                &rag::RagRequest {
                    user_id: uid,
                    kb_id: input.kb_id,
                    query: &input.query,
                    embed_provider_id: emb_provider.as_deref(),
                    embed_model: emb_model.as_deref(),
                    gen_provider_id: input.provider_id.as_deref(),
                    gen_model: input.model.as_deref(),
                    top_k,
                    mode: &mode,
                    chunk_overrides: chunk_overrides_opt,
                },
            )
            .await?
        }
    } else {
        rag::rag_answer(
            &db,
            &rag::RagRequest {
                user_id: uid,
                kb_id: input.kb_id,
                query: &input.query,
                embed_provider_id: emb_provider.as_deref(),
                embed_model: emb_model.as_deref(),
                gen_provider_id: input.provider_id.as_deref(),
                gen_model: input.model.as_deref(),
                top_k,
                mode: &mode,
                chunk_overrides: chunk_overrides_opt,
            },
        )
        .await?
    };

    // 若指定了会话，持久化 user/assistant 消息与 citations
    if let Some(sid) = input.session_id {
        persist_qa_exchange(&db, uid, sid, &input.query, &ans.answer, &ans.context);
    }
    // 埋点：FAQ 命中（detail=问题原文）或 RAG 生成（detail 含上下文片段数）
    if let Some(q) = faq_hit {
        log_metric_event(
            &db,
            &MetricEvent {
                uid,
                event_type: "faq_hit",
                kb_id: input.kb_id,
                doc_id: None,
                page_id: None,
                session_id: input.session_id,
                detail: Some(&q),
            },
        );
    } else {
        let detail =
            serde_json::json!({ "mode": mode, "topK": top_k, "contextCount": ans.context.len() });
        log_metric_event(
            &db,
            &MetricEvent {
                uid,
                event_type: "rag",
                kb_id: input.kb_id,
                doc_id: None,
                page_id: None,
                session_id: input.session_id,
                detail: Some(&detail.to_string()),
            },
        );
    }
    Ok(ans)
}

/// 将一次问答持久化到 QA 会话（user + assistant 消息 + 引用），
/// 校验会话归属；失败静默（不影响回答展示）。
fn persist_qa_exchange(
    db: &KbDatabase,
    uid: i64,
    session_id: i64,
    query: &str,
    answer: &str,
    context: &[rag::RagContextItem],
) {
    let conn = db.conn_lock();
    let owner: Option<i64> = conn
        .query_row(
            "SELECT user_id FROM qa_sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .ok();
    if owner != Some(uid) {
        return;
    }
    // 首次提问时自动用问题前 24 个字作为会话标题（默认「问答 …」标题被替换）
    let title: String = {
        let t: String = query.trim().chars().take(24).collect();
        if t.is_empty() {
            "问答".to_string()
        } else {
            t
        }
    };
    let _ = conn.execute(
        "UPDATE qa_sessions SET title = ?1, updated_at = datetime('now')
         WHERE id = ?2 AND (title IS NULL OR title = '' OR title LIKE '问答 %')",
        rusqlite::params![title, session_id],
    );
    let _ = conn.execute(
        "INSERT INTO qa_messages (session_id, role, content, citations) VALUES (?1,'user',?2,NULL)",
        rusqlite::params![session_id, query],
    );
    let citations = serde_json::to_string(&context.iter().map(|c| serde_json::json!({
        "doc_id": c.doc_id, "chunk_id": c.chunk_id, "kb_id": c.kb_id, "doc_title": c.doc_title,
        "score": c.score, "section": c.section, "page_no": c.page_no
    })).collect::<Vec<_>>()).unwrap_or_default();
    let _ = conn.execute(
        "INSERT INTO qa_messages (session_id, role, content, citations) VALUES (?1,'assistant',?2,?3)",
        rusqlite::params![session_id, answer, citations],
    );
    let _ = conn.execute(
        "UPDATE qa_sessions SET updated_at = datetime('now') WHERE id = ?1",
        rusqlite::params![session_id],
    );
}

/// RAG 流式问答：检索组装上下文后通过 Channel 逐段推送
/// `{"type":"delta","content":...}` / `{"type":"done",...}` / `{"type":"error",...}` 帧；
/// 结束时将问答落库到会话并记录埋点。
#[tauri::command]
pub async fn kb_rag_stream(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    input: RagInput,
    on_chunk: tauri::ipc::Channel<String>,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let top_k = input.top_k.unwrap_or(5).clamp(1, 100);
    // 显式指定知识库时校验可见性
    if let Some(id) = input.kb_id {
        if !crate::kb::retrieval::can_access_kb(&db, id, uid) {
            return Err("无权限：你无权访问该知识库".to_string());
        }
    }
    let mode = input.mode.clone().unwrap_or_else(|| "hybrid".to_string());
    // 未显式指定生成模型时，使用「模型设置」中的推理模型
    let mut gen_provider = input.provider_id.clone();
    let mut gen_model = input.model.clone();
    if gen_provider.is_none() && gen_model.is_none() {
        if let Some((p, m)) = {
            let conn = db.conn_lock();
            read_model_setting(&conn, "inference")
        } {
            gen_provider = Some(p);
            gen_model = Some(m);
        }
    }
    let (emb_provider, emb_model) = resolve_embedding_pair(&db, None, None);
    let chunk_overrides: Vec<(i64, Option<String>)> = input
        .chunks
        .clone()
        .map(|v| v.into_iter().map(|c| (c.chunk_id, c.content)).collect())
        .unwrap_or_default();
    let chunk_overrides_opt = if chunk_overrides.is_empty() {
        None
    } else {
        Some(chunk_overrides.as_slice())
    };

    // FAQ 优先：命中标准问答对时直接推送 done（无需生成）
    if let Some(kid) = input.kb_id {
        let matched = {
            let conn = db.conn_lock();
            faq_match(&conn, kid, &input.query)
        };
        if let Some((question, answer)) = matched {
            let _ =
                on_chunk.send(serde_json::json!({ "type": "done", "content": answer }).to_string());
            if let Some(sid) = input.session_id {
                let ctx = vec![rag::RagContextItem {
                    chunk_id: 0,
                    doc_id: 0,
                    kb_id: kid,
                    content: answer.clone(),
                    score: 1.0,
                    doc_title: format!("FAQ：{}", question),
                    section: Some("FAQ 问答".to_string()),
                    page_no: None,
                }];
                persist_qa_exchange(&db, uid, sid, &input.query, &answer, &ctx);
            }
            log_metric_event(
                &db,
                &MetricEvent {
                    uid,
                    event_type: "faq_hit",
                    kb_id: input.kb_id,
                    doc_id: None,
                    page_id: None,
                    session_id: input.session_id,
                    detail: Some(&question),
                },
            );
            return Ok(serde_json::json!({ "streamed": true, "faq": true }));
        }
    }

    let result = rag::rag_stream(
        &db,
        &rag::RagRequest {
            user_id: uid,
            kb_id: input.kb_id,
            query: &input.query,
            embed_provider_id: emb_provider.as_deref(),
            embed_model: emb_model.as_deref(),
            gen_provider_id: gen_provider.as_deref(),
            gen_model: gen_model.as_deref(),
            top_k,
            mode: &mode,
            chunk_overrides: chunk_overrides_opt,
        },
        |delta: &str| {
            let _ =
                on_chunk.send(serde_json::json!({ "type": "delta", "content": delta }).to_string());
        },
    )
    .await;

    match result {
        Ok((
            content,
            context,
            provider_id,
            model,
            prompt_tokens,
            completion_tokens,
            total_tokens,
        )) => {
            let _ = on_chunk.send(
                serde_json::json!({
                    "type": "done",
                    "content": content,
                    "model": model,
                    "prompt_tokens": prompt_tokens,
                    "completion_tokens": completion_tokens,
                    "total_tokens": total_tokens,
                })
                .to_string(),
            );
            if let Some(sid) = input.session_id {
                persist_qa_exchange(&db, uid, sid, &input.query, &content, &context);
            }
            let detail =
                serde_json::json!({ "mode": mode, "topK": top_k, "contextCount": context.len() });
            log_metric_event(
                &db,
                &MetricEvent {
                    uid,
                    event_type: "rag",
                    kb_id: input.kb_id,
                    doc_id: None,
                    page_id: None,
                    session_id: input.session_id,
                    detail: Some(&detail.to_string()),
                },
            );
            Ok(serde_json::json!({ "streamed": true, "model": model, "provider": provider_id }))
        }
        Err(e) => {
            let _ = on_chunk.send(serde_json::json!({ "type": "error", "message": e }).to_string());
            Err(e)
        }
    }
}

// ─── 高亮辅助（前端可直接调用也可本地算） ───

#[tauri::command]
pub async fn kb_highlight(
    content: String,
    query: String,
) -> Result<Vec<rag::HighlightSegment>, String> {
    Ok(rag::highlight(&content, &query))
}

// ════════════════════════════════════════════════════════════
// 检索历史
// ════════════════════════════════════════════════════════════

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct SearchLogItem {
    pub id: i64,
    pub kbId: Option<i64>,
    pub query: String,
    pub mode: String,
    pub hitCount: i64,
    pub createdAt: String,
}

/// 记录检索历史（由 kb_search 内部调用）
fn log_search(
    db: &KbDatabase,
    kb_id: Option<i64>,
    uid: i64,
    query: &str,
    mode: &str,
    hit_count: i64,
) {
    let conn = db.conn_lock();
    let _ = conn.execute(
        "INSERT INTO search_logs (kb_id, user_id, query, mode, hit_count) VALUES (?1,?2,?3,?4,?5)",
        rusqlite::params![kb_id, uid, query, mode, hit_count],
    );
}

/// 列出当前用户的检索历史
#[tauri::command]
pub async fn kb_search_history(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    limit: Option<i64>,
) -> Result<Vec<SearchLogItem>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let lim = limit.unwrap_or(50).clamp(1, 500);
    let conn = db.conn_lock();
    let mut stmt = conn.prepare(
        "SELECT id, kb_id, query, mode, hit_count, created_at FROM search_logs WHERE user_id = ?1 ORDER BY id DESC LIMIT ?2"
    ).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![uid, lim], |row| {
            Ok(SearchLogItem {
                id: row.get(0)?,
                kbId: row.get(1)?,
                query: row.get(2)?,
                mode: row.get(3)?,
                hitCount: row.get(4)?,
                createdAt: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}
