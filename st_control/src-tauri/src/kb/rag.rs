// ============================================================
// RAG 检索增强生成
//  - 检索 → 上下文组装 → 调用 LLM 生成（复用 chat_with_llm）
//  - 高亮定位：在分片内容中标记与 query 命中的词，供前端高亮
// ============================================================

use crate::kb::db::KbDatabase;
use crate::kb::retrieval::{
    bm25_search, rerank_chunks, rrf_fuse, vector_search_capped, visible_kb_ids, RetrievedChunk,
};
use crate::llm::types::{ChatMessage, ChatRequest, ChatResult};
use rusqlite::params;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

/// RAG 流式生成取消机制（序列号方案）：
/// RAG_SEQ 为全局递增序列号，每次 RAG 流式生成开始时分配唯一 ID；
/// RAG_CANCEL_ID 为请求取消的目标 ID（0 = 无取消请求）。
/// 比旧 AtomicBool 方案的优势：快速连续发起多个 RAG 请求时，取消信号精准匹配目标，
/// 不会因 clear_rag_cancel() 误清除其他请求的取消标记。
static RAG_SEQ: AtomicU64 = AtomicU64::new(1);
static RAG_CANCEL_ID: AtomicU64 = AtomicU64::new(0);

/// 分配一个新的 RAG 流式生成 ID（每次 rag_stream 开始时调用）
pub fn next_rag_id() -> u64 {
    RAG_SEQ.fetch_add(1, Ordering::SeqCst)
}

/// 请求取消指定 RAG 流式生成（精准取消，不影响其他并发请求）
pub fn request_rag_cancel(rag_id: u64) {
    RAG_CANCEL_ID.store(rag_id, Ordering::SeqCst);
}

/// 检查指定 RAG 流式生成是否已请求取消
pub fn rag_cancel_requested(rag_id: u64) -> bool {
    RAG_CANCEL_ID.load(Ordering::SeqCst) == rag_id
}

/// 分片 + 文档标题查询行（SELECT：kb_id, doc_id, doc_title, section, page_no, content）
struct ChunkRow(i64, i64, String, Option<String>, Option<i64>, String);

/// RAG 检索/生成请求（db 与流式回调单独传入）
pub struct RagRequest<'a> {
    pub user_id: i64,
    pub kb_id: Option<i64>,
    pub query: &'a str,
    pub embed_provider_id: Option<&'a str>,
    pub embed_model: Option<&'a str>,
    pub gen_provider_id: Option<&'a str>,
    pub gen_model: Option<&'a str>,
    pub top_k: usize,
    pub mode: &'a str,
    pub chunk_overrides: Option<&'a [(i64, Option<String>)]>,
    /// 问答会话 ID（用于加载多轮对话历史）
    pub session_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RagContextItem {
    pub chunk_id: i64,
    pub doc_id: i64,
    pub kb_id: i64,
    pub content: String,
    pub score: f64,
    pub doc_title: String,
    /// 所属章节标题路径（标题感知分块时填充，供引用溯源）
    pub section: Option<String>,
    /// 页码（PDF 预留，供引用溯源）
    pub page_no: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RagAnswer {
    pub answer: String,
    pub context: Vec<RagContextItem>,
    pub model: String,
    pub provider: String,
}

/// 高亮片段：在 content 中标记命中词区间（供前端高亮展示）
#[derive(Debug, Clone, Serialize)]
pub struct HighlightSegment {
    pub text: String,
    pub hit: bool,
}

/// 将文本按 query 关键词切分为高亮片段
///
/// 基于 char 索引扫描，避免 `content.to_lowercase()` 改变字节长度时（如
/// 'İ'(U+0130) → "i\u{0307}"）字节偏移错位导致 panic。
pub fn highlight(content: &str, query: &str) -> Vec<HighlightSegment> {
    let terms: Vec<Vec<char>> = query
        .split(|c: char| c.is_whitespace() || "，。；！？,.;!?\"'".contains(c))
        .filter(|t| t.chars().count() >= 2)
        .map(|t| t.to_lowercase().chars().collect())
        .collect();
    if terms.is_empty() {
        return vec![HighlightSegment {
            text: content.to_string(),
            hit: false,
        }];
    }

    // 逐字符 lowercase，保持与 content 1:1 字符对应（取 to_lowercase() 首字符）
    let content_chars: Vec<char> = content.chars().collect();
    let lower_chars: Vec<char> = content_chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    // 预计算每个 char_index → content 中的字节偏移
    let mut byte_offsets = Vec::with_capacity(content_chars.len() + 1);
    let mut off = 0usize;
    for c in &content_chars {
        byte_offsets.push(off);
        off += c.len_utf8();
    }
    byte_offsets.push(off); // 哨兵：最后一个字符之后的偏移

    let n = content_chars.len();
    let mut segments = Vec::new();
    let mut i = 0usize; // char 索引
    while i < n {
        // 尝试在当前位置匹配任一 term
        let mut match_char_len = 0usize;
        for term in &terms {
            let tlen = term.len();
            if tlen > 0 && i + tlen <= n && lower_chars[i..i + tlen] == *term {
                match_char_len = tlen;
                break;
            }
        }
        if match_char_len > 0 {
            segments.push(HighlightSegment {
                text: content[byte_offsets[i]..byte_offsets[i + match_char_len]].to_string(),
                hit: true,
            });
            i += match_char_len;
        } else {
            // 向后扫描到下一个命中位置
            let mut next = n;
            'scan: for j in i..n {
                for term in &terms {
                    let tlen = term.len();
                    if tlen > 0 && j + tlen <= n && lower_chars[j..j + tlen] == *term {
                        next = j;
                        break 'scan;
                    }
                }
            }
            let seg = &content[byte_offsets[i]..byte_offsets[next]];
            if !seg.is_empty() {
                segments.push(HighlightSegment {
                    text: seg.to_string(),
                    hit: false,
                });
            }
            i = next;
            // next == i 不会发生（上面分支已处理命中），但防御性保底
            if i == byte_offsets.len() - 1 {
                break;
            }
        }
    }
    if segments.is_empty() {
        segments.push(HighlightSegment {
            text: content.to_string(),
            hit: false,
        });
    }
    segments
}

/// 执行 RAG 问答
/// `mode`: hybrid（混合检索，默认）/ vector（仅向量）/ bm25（仅全文）
pub async fn rag_answer(db: &KbDatabase, req: &RagRequest<'_>) -> Result<RagAnswer, String> {
    let query = req.query;
    let gen_provider_id = req.gen_provider_id;
    let gen_model = req.gen_model;
    let (context, ctx_text) = rag_context(db, req).await?;

    // 4. 调用 LLM（复用 chat_with_llm）
    let base_prompt = {
        let conn = db.conn_lock();
        crate::kb::handlers::read_rag_system_prompt(&conn)
    };
    let system_prompt = base_prompt + "\n\n【知识上下文】" + &ctx_text;
    // 加载多轮对话历史（最近 5 轮），实现上下文记忆
    let history = req
        .session_id
        .map(|sid| load_conversation_history(db, sid, 5))
        .unwrap_or_default();
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system_prompt,
        parts: None,
    }];
    messages.extend(history);
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: query.to_string(),
        parts: None,
    });
    let req = ChatRequest {
        provider_id: gen_provider_id.map(|s| s.to_string()),
        model: gen_model.map(|s| s.to_string()),
        role_id: None,
        messages,
        max_tokens: None,
        temperature: Some(0.3),
        top_p: None,
        presence_penalty: None,
        frequency_penalty: None,
    };
    let res: ChatResult = crate::llm::handlers::chat_with_llm(req).await?;

    Ok(RagAnswer {
        answer: res.content.clone(),
        context,
        model: res.model.clone(),
        provider: res.provider_id.clone(),
    })
}

/// 检索 + 组装上下文（RAG 问答、流式问答、智能体对话共用）：
/// 返回 (上下文片段, 拼接后的上下文文本)
pub(crate) async fn rag_context(
    db: &KbDatabase,
    req: &RagRequest<'_>,
) -> Result<(Vec<RagContextItem>, String), String> {
    let user_id = req.user_id;
    let kb_id = req.kb_id;
    let query = req.query;
    let embed_provider_id = req.embed_provider_id;
    let embed_model = req.embed_model;
    let top_k = req.top_k;
    let mode = req.mode;
    let chunk_overrides = req.chunk_overrides;
    // 1. 确定可见知识库范围
    let visible = match kb_id {
        Some(id) => vec![id],
        None => visible_kb_ids(db, user_id),
    };
    if visible.is_empty() {
        return Err("当前用户无可访问的知识库".to_string());
    }

    // 2. 检索：支持 hybrid/vector/bm25 三种模式；
    //    若提供了人工编辑的片段覆盖，则跳过自动检索与重排，直接使用指定分片
    let retrieve = async || -> Result<Vec<RetrievedChunk>, String> {
        let top = match mode {
            "vector" => {
                // 大知识库自动走 FTS 候选池预筛 + 向量精排
                vector_search_capped(db, query, &visible, top_k, embed_provider_id, embed_model)
                    .await?
            }
            "bm25" => bm25_search(db, query, &visible, top_k)?,
            _ => {
                let bm25 = bm25_search(db, query, &visible, top_k)?;
                match vector_search_capped(
                    db,
                    query,
                    &visible,
                    top_k,
                    embed_provider_id,
                    embed_model,
                )
                .await
                {
                    Ok(vector) => rrf_fuse(vector, bm25, 60)
                        .into_iter()
                        .take(top_k)
                        .collect::<Vec<_>>(),
                    Err(e) => {
                        // 未配置/不可用的嵌入模型时降级为纯 BM25，保证问答可用
                        log::warn!("向量检索不可用，RAG 降级为 BM25: {}", e);
                        if bm25.is_empty() {
                            return Err(e);
                        }
                        bm25
                    }
                }
            }
        };
        // Rerank：配置了重排序模型时对检索结果智能重排序
        Ok(rerank_chunks(db, query, top).await)
    };
    let top = match chunk_overrides {
        Some(ov) if !ov.is_empty() => load_override_chunks(db, &visible, ov)?,
        _ => retrieve().await?,
    };

    if top.is_empty() {
        return Err("未检索到相关知识片段".to_string());
    }

    // 3. 组装上下文 + 文档标题（conn 仅用于同步查询，await 前必须释放）
    //    父子分块增强：命中子块时，把父块内容一并作为上下文，提升回答质量
    let mut context = Vec::new();
    let mut ctx_text = String::new();
    {
        let conn = db.conn_lock();
        for c in &top {
            let (title, section, page_no): (String, Option<String>, Option<i64>) = conn
                .query_row(
                    "SELECT d.title, dc.section, dc.page_no FROM document_chunks dc
                     JOIN documents d ON d.id = dc.doc_id
                     WHERE dc.id = ?1",
                    params![c.chunk_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap_or((String::new(), None, None));
            let mut content = c.content.clone();
            // 若为子块，附加父块内容（限定长度，避免上下文过大）
            if let Ok(Some(pid)) = conn.query_row(
                "SELECT parent_id FROM document_chunks WHERE id = ?1",
                params![c.chunk_id],
                |r| r.get::<_, Option<i64>>(0),
            ) {
                if let Ok(parent_content) = conn.query_row(
                    "SELECT content FROM document_chunks WHERE id = ?1",
                    params![pid],
                    |r| r.get::<_, String>(0),
                ) {
                    if parent_content.len() > content.len() {
                        let max_len = content.len().saturating_mul(3).max(2400);
                        let p = truncate_utf8(parent_content, max_len);
                        content = format!("（上下文）{}\n{}", p, content);
                    }
                }
            }
            let head = if let Some(sec) = &section {
                format!("[文档: {} / {}]", title, sec)
            } else {
                format!("[文档: {}]", title)
            };
            ctx_text.push_str(&format!("\n{}\n{}\n", head, content));
            context.push(RagContextItem {
                chunk_id: c.chunk_id,
                doc_id: c.doc_id,
                kb_id: c.kb_id,
                content,
                score: c.score,
                doc_title: title,
                section,
                page_no,
            });
        }
    }

    Ok((context, ctx_text))
}

/// 加载会话最近 N 轮消息作为对话历史（排除当前轮）。
/// 返回 (历史消息列表, 截断后的总 token 估算长度)。
fn load_conversation_history(
    db: &KbDatabase,
    session_id: i64,
    max_rounds: usize,
) -> Vec<ChatMessage> {
    let conn = db.conn_lock();
    // 取最近 max_rounds*2 条消息（每轮 = user + assistant），排除最后一条（当前用户提问）
    let limit = (max_rounds * 2) as i64;
    let mut stmt = match conn.prepare(
        "SELECT role, content FROM qa_messages WHERE session_id = ?1 AND content IS NOT NULL
         ORDER BY id DESC LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows: Vec<(String, String)> = match stmt
        .query_map(rusqlite::params![session_id, limit], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => return Vec::new(),
    };
    // 按时间正序（查询是 DESC），跳过第一条（当前用户提问）
    let mut history: Vec<ChatMessage> = Vec::new();
    let mut total_len = 0usize;
    const MAX_HISTORY_CHARS: usize = 4000; // 历史消息总长度上限，防止 token 溢出
    for (role, content) in rows.into_iter().rev() {
        // 跳过当前轮的用户消息（已在 req.query 中）
        if history.is_empty() && role == "user" {
            continue;
        }
        if total_len + content.len() > MAX_HISTORY_CHARS {
            break;
        }
        total_len += content.len();
        history.push(ChatMessage {
            role,
            content,
            parts: None,
        });
    }
    history
}

/// RAG 流式生成：检索并组装上下文后，通过 on_delta 回调逐段返回生成内容。
/// 返回 (完整回答, 上下文, 生成提供方, 模型, prompt/completion/total tokens)，
/// 并记录本次 LLM 用量。
/// `rag_id` 为本次流式生成的唯一序列号（由 next_rag_id() 分配），用于精准取消。
pub async fn rag_stream<F>(
    db: &KbDatabase,
    req: &RagRequest<'_>,
    rag_id: u64,
    mut on_delta: F,
) -> Result<(String, Vec<RagContextItem>, String, String, u64, u64, u64), String>
where
    F: FnMut(&str),
{
    let query = req.query;
    let gen_provider_id = req.gen_provider_id;
    let gen_model = req.gen_model;
    let (context, ctx_text) = rag_context(db, req).await?;
    let base_prompt = {
        let conn = db.conn_lock();
        crate::kb::handlers::read_rag_system_prompt(&conn)
    };
    let system_prompt = base_prompt + "\n\n【知识上下文】" + &ctx_text;
    // 加载多轮对话历史（最近 5 轮），实现上下文记忆
    let history = req
        .session_id
        .map(|sid| load_conversation_history(db, sid, 5))
        .unwrap_or_default();
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system_prompt,
        parts: None,
    }];
    messages.extend(history);
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: query.to_string(),
        parts: None,
    });
    // 解析生成提供方与模型（与 chat_with_llm_stream 一致：未指定时用默认提供方/默认模型）
    let cfg = crate::llm::config::load_config();
    let provider_id = gen_provider_id
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| cfg.default_provider_id.clone())
        .ok_or_else(|| "未指定提供方，且未配置全局默认提供方".to_string())?;
    let provider = cfg
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.enabled)
        .cloned()
        .ok_or_else(|| format!("提供方「{}」不存在或已被禁用", provider_id))?;
    let model = gen_model
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| provider.default_model.clone());
    if model.is_empty() {
        return Err("未指定模型，且提供方未配置默认模型".to_string());
    }
    let (content, prompt, completion, total) = crate::llm::client::chat_completion_stream(
        &provider,
        &crate::llm::client::CompletionParams {
            model: &model,
            messages: &messages,
            max_tokens: None,
            temperature: Some(0.3),
            top_p: None,
            presence_penalty: None,
            frequency_penalty: None,
            tools: None,
            tool_choice: None,
        },
        |delta: &str| {
            if rag_cancel_requested(rag_id) {
                // 用户点击「停止生成」：通知底层中断流式读取（精准匹配本次请求）
                false
            } else {
                on_delta(delta);
                true
            }
        },
    )
    .await?;
    // 用量与成本已由 client::chat_completion_stream 统一计入「大模型管理 → 流量与成本」
    Ok((
        content,
        context,
        provider.id,
        model,
        prompt,
        completion,
        total,
    ))
}

/// 按 UTF-8 字符边界截断字符串（String::truncate 在非字符边界会 panic，
/// 中文等多字节内容按字节长度截断几乎必然越界，这里统一回退到最近边界）
fn truncate_utf8(mut s: String, max_len: usize) -> String {
    if s.len() > max_len {
        let mut cut = max_len;
        while cut > 0 && !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── highlight 函数 Unicode 安全测试 ──

    /// 'İ'(U+0130) 小写为 "i\u{0307}"（2 字节 → 3 字节），
    /// 旧实现用同一字节索引同时切 content 和 lower 会 panic；
    /// 本测试验证不 panic 且拼接回原文、命中段正确。
    #[test]
    fn test_highlight_turkish_i_no_panic() {
        let text = "\\ İSTANBUL İstanbul\\";
        let segs = highlight(text, "istanbul");
        // 不 panic 即为通过；额外验证拼接回原文
        let reconstructed: String = segs.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(reconstructed, text, "拼接必须还原原文");
        // 至少有一个命中段
        assert!(
            segs.iter().any(|s| s.hit),
            "query 'istanbul' 应命中 'İSTANBUL' 或 'İstanbul'"
        );
    }

    /// 'İ' 出现在内容中间的混合文本，确认不会越界
    #[test]
    fn test_highlight_turkish_i_mixed_content() {
        let text = "Welcome to İSTANBUL city guide.";
        let segs = highlight(text, "istanbul");
        let reconstructed: String = segs.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(reconstructed, text);
        assert!(segs.iter().any(|s| s.hit));
    }

    /// 纯英文命中：原有行为回归
    #[test]
    fn test_highlight_english_basic() {
        let text = "Hello World, this is a test.";
        let segs = highlight(text, "world test");
        let reconstructed: String = segs.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(reconstructed, text);
        assert!(segs.iter().any(|s| s.hit && s.text == "World"));
        assert!(segs.iter().any(|s| s.hit && s.text == "test"));
    }

    /// 中文命中：原有行为回归
    #[test]
    fn test_highlight_chinese_basic() {
        let text = "知识库管理功能已更新";
        let segs = highlight(text, "知识库 更新");
        let reconstructed: String = segs.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(reconstructed, text);
        assert!(segs.iter().any(|s| s.hit && s.text == "知识库"));
        assert!(segs.iter().any(|s| s.hit && s.text == "更新"));
    }

    /// 空 query / 短词不命中
    #[test]
    fn test_highlight_empty_query_returns_full() {
        let text = "some text";
        let segs = highlight(text, "");
        assert_eq!(segs.len(), 1);
        assert!(!segs[0].hit);
        assert_eq!(segs[0].text, text);
    }

    #[test]
    fn test_truncate_utf8_boundary_no_panic() {
        // 中文内容按字节 2400 截断必须落在字符边界（回归：String::truncate 越界 panic）
        let s = "中文内容用于验证父子分块上下文截断不会在非 UTF-8 边界 panic。".repeat(200);
        let t = truncate_utf8(s, 2400);
        assert!(t.is_char_boundary(t.len()), "截断后必须在字符边界");
        assert!(t.len() <= 2400);
        assert!(
            t.len() > 2400 - 20,
            "应回退到 2400 附近最近的字符边界，实际长度 {}",
            t.len()
        );
        // 短内容与空内容原样保留
        assert_eq!(truncate_utf8("短".to_string(), 100), "短");
        assert!(truncate_utf8(String::new(), 100).is_empty());
    }
}

/// 按人工选择的片段构建上下文（跳过检索；不可见或不存在时忽略）
fn load_override_chunks(
    db: &KbDatabase,
    visible: &[i64],
    overrides: &[(i64, Option<String>)],
) -> Result<Vec<RetrievedChunk>, String> {
    let conn = db.conn_lock();
    let mut out = Vec::new();
    for (chunk_id, content_override) in overrides {
        let row: Option<ChunkRow> = conn
            .query_row(
                "SELECT c.kb_id, c.doc_id, COALESCE(d.title,''), c.section, c.page_no, c.content
                 FROM document_chunks c JOIN documents d ON d.id = c.doc_id
                 WHERE c.id = ?1",
                params![chunk_id],
                |r| {
                    Ok(ChunkRow(
                        r.get(0)?,
                        r.get(1)?,
                        r.get::<_, String>(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get::<_, String>(5)?,
                    ))
                },
            )
            .ok();
        let Some(ChunkRow(kb_id, doc_id, doc_title, section, page_no, content)) = row else {
            continue;
        };
        if !visible.contains(&kb_id) {
            continue;
        }
        let content = content_override
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or(content);
        out.push(RetrievedChunk {
            chunk_id: *chunk_id,
            doc_id,
            kb_id,
            content,
            page_no,
            section,
            score: 1.0,
            source: "manual".to_string(),
            doc_title,
        });
    }
    Ok(out)
}
