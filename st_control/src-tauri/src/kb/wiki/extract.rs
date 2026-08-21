// ════════════════════════════════════════════════════════════
// Wiki 摘要与实体提取（LLM 严格基于正文）
// 自 wiki.rs 拆分：语言检测、LLM 调用、实体 JSON 解析、
// 页面元数据补充、实体页确保、提炼精化与解析。
// ════════════════════════════════════════════════════════════

use rusqlite::params;

use super::mutate::rebuild_kb_links;
use super::utils::{slugify, truncate_for_llm};
use crate::kb::db::KbDatabase;

const PAGE_SEP: &str = "<<<PAGE>>>";
const PAGE_END: &str = "<<<END>>>";

// ════════════════════════════════════════════════════════════
// 摘要与实体提取（WeKnora 风格：LLM 严格基于正文，不编造）
// ════════════════════════════════════════════════════════════

/// 判断文本是否以中文为主
fn detect_lang(text: &str) -> &'static str {
    let total = text.chars().count().max(1);
    let cjk = text
        .chars()
        .filter(|c| matches!(c, '\u{4e00}'..='\u{9fff}'))
        .count();
    if cjk as f32 / total as f32 > 0.15 {
        "中文"
    } else {
        "English"
    }
}

/// 调用 LLM（复用全局大模型通道）
async fn llm_chat(
    system: &str,
    user: &str,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<String, String> {
    let request = crate::llm::types::ChatRequest {
        provider_id: provider_id.map(|s| s.to_string()),
        model: model.map(|s| s.to_string()),
        role_id: None,
        messages: vec![
            crate::llm::types::ChatMessage {
                role: "system".into(),
                content: system.to_string(),
                parts: None,
            },
            crate::llm::types::ChatMessage {
                role: "user".into(),
                content: user.to_string(),
                parts: None,
            },
        ],
        max_tokens: Some(3000),
        temperature: Some(0.2),
        top_p: None,
        presence_penalty: None,
        frequency_penalty: None,
    };
    Ok(crate::llm::handlers::chat_with_llm(request).await?.content)
}

/// 解析实体 JSON（容忍 ```json 包裹与前后缀，只取数组）
fn parse_entity_json(raw: &str) -> Vec<(String, String, Option<String>)> {
    let start = raw.find('[');
    let end = raw.rfind(']');
    let Some((s, e)) = start.zip(end) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw[s..=e]) else {
        return Vec::new();
    };
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in arr {
        let Some(name) = item
            .get("title")
            .and_then(|x| x.as_str())
            .map(|s| s.trim().to_string())
        else {
            continue;
        };
        if name.is_empty() || name.len() > 60 {
            continue;
        }
        let etype = item
            .get("type")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let desc = item
            .get("description")
            .and_then(|x| x.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let key = format!("{}|{}", name, etype);
        if seen.insert(key) {
            out.push((name, etype, desc));
        }
    }
    out.truncate(30);
    out
}

/// 对单页执行「摘要 + 实体」提取（LLM 严格基于正文），结果写回 wiki_pages / wiki_page_entities
pub async fn extract_page_meta(
    db: &KbDatabase,
    _uid: i64,
    page_id: i64,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<(), String> {
    let (kb_id, title, content, cur_summary): (i64, String, String, String) = {
        let conn = db.conn_lock();
        conn.query_row(
            "SELECT kb_id, title, content_md, COALESCE(summary,'') FROM wiki_pages WHERE id = ?1",
            params![page_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(|_| "页面不存在".to_string())?
    };
    if content.trim().is_empty() {
        return Err("页面内容为空，无法提取".to_string());
    }
    {
        let conn = db.conn_lock();
        let _ = conn.execute(
            "UPDATE wiki_pages SET extract_status = 'pending' WHERE id = ?1",
            params![page_id],
        );
    }
    let lang = detect_lang(&content);

    // 1) 摘要（借鉴 WeKnora generate_summary：严格基于正文、不编造）
    let summary_prompt = format!(
        "你是精准的文档摘要专家。请严格基于用户提供的正文总结核心信息，禁止依据标题或外部线索编造。\n\
         要求：\n\
         1. 输出 60-200 字的连贯摘要，突出关键信息、要点与结论；\n\
         2. 只基于提供内容，不添加文档中没有的信息；\n\
         3. 客观中立的第三人称叙述；\n\
         4. 使用{}语言；\n\
         5. 直接输出摘要正文，不要任何前后缀。",
        lang
    );
    let summary = match llm_chat(&summary_prompt, &content, provider_id, model).await {
        Ok(s) => {
            let s = s.trim().to_string();
            if s.is_empty() || s.to_lowercase().contains("no textual content") {
                cur_summary
            } else {
                s
            }
        }
        Err(e) => {
            log::warn!("页面 {} 摘要提取失败: {}", page_id, e);
            cur_summary
        }
    };

    // 2) 实体（借鉴 WeKnora graph_extraction：类型限定、JSON 输出）
    let entity_prompt = format!(
        "从用户提供的文本中抽取实体，实体类型严格限定为：[人物, 组织, 地点, 产品, 事件, 日期, 作品, 概念, 资源, 类别, 操作]。\n\
         要求：\n\
         1. 只输出 JSON 数组，不要任何解释或前后缀；\n\
         2. 每个实体包含 title、type、description 三个字段；\n\
         3. type 必须来自上述列表，无法判断时跳过该实体；\n\
         4. title 按原文提取，description 用{}语言、基于正文简要描述；\n\
         5. 没有实体时输出 []。",
        lang
    );
    let entities = match llm_chat(&entity_prompt, &content, provider_id, model).await {
        Ok(s) => parse_entity_json(&s),
        Err(e) => {
            log::warn!("页面 {} 实体提取失败: {}", page_id, e);
            Vec::new()
        }
    };

    // 写回
    let conn = db.conn_lock();
    conn.execute(
        "UPDATE wiki_pages SET summary = ?1, extract_status = 'done' WHERE id = ?2",
        params![summary, page_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM wiki_page_entities WHERE page_id = ?1",
        params![page_id],
    )
    .map_err(|e| e.to_string())?;
    for (name, etype, desc) in &entities {
        conn.execute(
            "INSERT INTO wiki_page_entities (kb_id, page_id, name, entity_type, description) VALUES (?1,?2,?3,?4,?5)",
            params![kb_id, page_id, name, etype, desc],
        )
        .map_err(|e| e.to_string())?;
    }
    log::info!(
        "页面 {} 摘要与实体提取完成: {} 个实体",
        page_id,
        entities.len()
    );
    drop(conn);
    // 自动为实体创建页面并按类型归档（不存在时）
    if let Err(e) = ensure_entity_pages(db, kb_id, page_id, &title, &entities) {
        log::warn!("实体页自动创建失败 page={} err={}", page_id, e);
    }
    Ok(())
}

/// 确保存在「实体/<类型>」目录，返回目录 id
fn ensure_entity_dir(
    conn: &rusqlite::Connection,
    kb_id: i64,
    entity_type: &str,
) -> Result<i64, String> {
    let root: Option<i64> = conn
        .query_row(
            "SELECT id FROM kb_directories WHERE kb_id = ?1 AND parent_id IS NULL AND name = '实体'",
            params![kb_id],
            |r| r.get(0),
        )
        .ok();
    let root = match root {
        Some(id) => id,
        None => {
            conn.execute(
                "INSERT INTO kb_directories (kb_id, parent_id, name) VALUES (?1,NULL,'实体')",
                params![kb_id],
            )
            .map_err(|e| e.to_string())?;
            conn.last_insert_rowid()
        }
    };
    let t = if entity_type.trim().is_empty() {
        "未分类".to_string()
    } else {
        entity_type.trim().to_string()
    };
    let sub: Option<i64> = conn
        .query_row(
            "SELECT id FROM kb_directories WHERE kb_id = ?1 AND parent_id = ?2 AND name = ?3",
            params![kb_id, root, t],
            |r| r.get(0),
        )
        .ok();
    match sub {
        Some(id) => Ok(id),
        None => {
            conn.execute(
                "INSERT INTO kb_directories (kb_id, parent_id, name) VALUES (?1,?2,?3)",
                params![kb_id, root, t],
            )
            .map_err(|e| e.to_string())?;
            Ok(conn.last_insert_rowid())
        }
    }
}

/// 为抽取的实体自动创建「实体页」（不存在时），并按类型归档到 实体/<类型> 目录，
/// 页面正文回链来源页，自动重建链接后纳入知识图谱。
pub(crate) fn ensure_entity_pages(
    db: &KbDatabase,
    kb_id: i64,
    source_page_id: i64,
    source_title: &str,
    entities: &[(String, String, Option<String>)],
) -> Result<usize, String> {
    if entities.is_empty() {
        return Ok(0);
    }
    let mut created = 0usize;
    {
        let conn = db.conn_lock();
        for (name, etype, desc) in entities {
            let slug = slugify(name);
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM wiki_pages WHERE kb_id = ?1 AND (slug = ?2 OR title = ?3)",
                    params![kb_id, slug, name],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if exists > 0 {
                continue;
            }
            let dir_id = ensure_entity_dir(&conn, kb_id, etype)?;
            let mut md = String::new();
            if !etype.is_empty() {
                md.push_str(&format!("**类型**：{}\n\n", etype));
            }
            if let Some(d) = desc {
                if !d.is_empty() {
                    md.push_str(d);
                    md.push_str("\n\n");
                }
            }
            md.push_str(&format!("相关页面：[[{}]]", source_title));
            let summary = desc.clone().unwrap_or_else(|| format!("实体「{}」", name));
            conn.execute(
                "INSERT INTO wiki_pages (kb_id, dir_id, title, slug, summary, content_md, status, extract_status, created_by)
                 VALUES (?1,?2,?3,?4,?5,?6,'draft','done',NULL)",
                params![kb_id, dir_id, name, slug, summary, md],
            )
            .map_err(|e| e.to_string())?;
            created += 1;
        }
    }
    if created > 0 {
        // 让实体页的 [[来源页]] 出链生效，纳入知识图谱
        let conn = db.conn_lock();
        rebuild_kb_links(&conn, kb_id)?;
    }
    let _ = source_page_id;
    Ok(created)
}

/// LLM 提炼结果
pub(crate) struct RefinedPage {
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) content: String,
}

/// 调用大模型将文档正文提炼为相互链接的多页面 Markdown。
/// 要求模型按 `<<<PAGE>>>...<<<END>>>` 分段输出，正文内用 [[页面标题]] 表示链接。
pub(crate) async fn refine_with_llm(
    text: &str,
    doc_title: &str,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<Vec<RefinedPage>, String> {
    let system = "你是企业知识库编辑，擅长将原始文档提炼为相互链接的 Markdown 知识库。\
    要求：\
    1. 按主题拆分为多个相互独立的页面，每页一个核心知识点；\
    2. 页面之间用 [[页面标题]] 语法互相引用，构成知识网络；\
    3. 输出必须严格遵循以下格式，每页之间用分隔标记：\n\
    <<<PAGE>>>\n\
    标题\n\
    摘要\n\
    ---\n\
    正文（Markdown）\n\
    <<<END>>>\n\
    不要输出任何其他文字。";
    let user = format!(
        "请将以下文档《{}》提炼为相互链接的知识库页面：\n\n{}",
        doc_title,
        truncate_for_llm(text, 30000)
    );
    let request = crate::llm::types::ChatRequest {
        provider_id: provider_id.map(|s| s.to_string()),
        model: model.map(|s| s.to_string()),
        role_id: None,
        messages: vec![
            crate::llm::types::ChatMessage {
                role: "system".into(),
                content: system.to_string(),
                parts: None,
            },
            crate::llm::types::ChatMessage {
                role: "user".into(),
                content: user,
                parts: None,
            },
        ],
        max_tokens: Some(6000),
        temperature: Some(0.3),
        top_p: None,
        presence_penalty: None,
        frequency_penalty: None,
    };
    let result = crate::llm::handlers::chat_with_llm(request).await?;
    parse_refined_pages(&result.content)
}

/// 解析 LLM 输出的多页面 Markdown
pub(crate) fn parse_refined_pages(raw: &str) -> Result<Vec<RefinedPage>, String> {
    let mut pages = Vec::new();
    for block in raw.split(PAGE_SEP).skip(1) {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let end = block.find(PAGE_END).unwrap_or(block.len());
        let body = &block[..end];
        // 首行标题，第二行摘要，之后是正文
        let mut lines = body.lines();
        let title = lines.next().unwrap_or("").trim().to_string();
        let summary = lines.next().unwrap_or("").trim().to_string();
        let content = body
            .lines()
            .skip(2)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        if !title.is_empty() && !content.is_empty() {
            pages.push(RefinedPage {
                title,
                summary,
                content,
            });
        }
    }
    if pages.is_empty() {
        // 兜底：未按标记输出时，视为单个页面
        pages.push(RefinedPage {
            title: "知识库总览".to_string(),
            summary: String::new(),
            content: raw.trim().to_string(),
        });
    }
    Ok(pages)
}
