// ============================================================
// 智能体管理
// 智能体 = 「AI 角色（系统提示词）+ 大模型（提供方/模型）+ 知识库（RAG 上下文）」
// - 对话复用 llm::handlers::chat_with_llm_stream：自动注入角色提示词、
//   记录流量与成本（大模型管理）、持久化聊天记录；
// - 绑定知识库时先做 RAG 检索，把知识上下文并入用户消息。
// ============================================================

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::Database;
use crate::kb::db::KbDatabase;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentItem {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub role_id: String,
    pub provider_id: String,
    pub model: String,
    pub kb_id: Option<i64>,
    pub temperature: f64,
    pub max_tokens: i64,
    pub top_p: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentInput {
    pub name: String,
    pub description: Option<String>,
    pub role_id: Option<String>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub kb_id: Option<i64>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub top_p: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentChatInput {
    pub agent_id: i64,
    pub query: String,
}

const AGENT_COLS: &str = "id, name, COALESCE(description,''), COALESCE(role_id,''), COALESCE(provider_id,''), \
     COALESCE(model,''), kb_id, COALESCE(temperature,0.7), COALESCE(max_tokens,2048), COALESCE(top_p,1.0), \
     created_at, updated_at";

fn row_to_agent(row: &rusqlite::Row) -> rusqlite::Result<AgentItem> {
    Ok(AgentItem {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        role_id: row.get(3)?,
        provider_id: row.get(4)?,
        model: row.get(5)?,
        kb_id: row.get(6)?,
        temperature: row.get(7)?,
        max_tokens: row.get(8)?,
        top_p: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn validate(input: &AgentInput) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("智能体名称不能为空".to_string());
    }
    Ok(())
}

/// 列出全部智能体
#[tauri::command]
pub fn agent_list(db: State<'_, Database>) -> Result<Vec<AgentItem>, String> {
    let conn = db.lock_conn();
    let sql = format!("SELECT {} FROM agents ORDER BY id DESC", AGENT_COLS);
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_agent)
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 获取单个智能体
#[tauri::command]
pub fn agent_get(db: State<'_, Database>, id: i64) -> Result<AgentItem, String> {
    let conn = db.lock_conn();
    let sql = format!("SELECT {} FROM agents WHERE id = ?1", AGENT_COLS);
    conn.query_row(&sql, params![id], row_to_agent)
        .map_err(|e| format!("智能体不存在: {}", e))
}

/// 按 id 查询智能体（供自动化 Worker 等内部模块使用；不存在返回 None）
pub(crate) fn get_agent_by_id(db: &Database, id: i64) -> Result<Option<AgentItem>, String> {
    let conn = db.lock_conn();
    let sql = format!("SELECT {} FROM agents WHERE id = ?1", AGENT_COLS);
    conn.query_row(&sql, params![id], row_to_agent)
        .optional()
        .map_err(|e| format!("查询智能体失败: {}", e))
}

/// 创建智能体
#[tauri::command]
pub fn agent_create(db: State<'_, Database>, input: AgentInput) -> Result<i64, String> {
    validate(&input)?;
    let conn = db.lock_conn();
    conn.execute(
        "INSERT INTO agents (name, description, role_id, provider_id, model, kb_id, temperature, max_tokens, top_p, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,datetime('now'))",
        params![
            input.name.trim(),
            input.description.clone().unwrap_or_default(),
            input.role_id.clone().unwrap_or_default(),
            input.provider_id.clone().unwrap_or_default(),
            input.model.clone().unwrap_or_default(),
            input.kb_id,
            input.temperature.unwrap_or(0.7),
            input.max_tokens.unwrap_or(2048),
            input.top_p.unwrap_or(1.0),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

/// 更新智能体
#[tauri::command]
pub fn agent_update(db: State<'_, Database>, id: i64, input: AgentInput) -> Result<(), String> {
    validate(&input)?;
    let conn = db.lock_conn();
    let n = conn
        .execute(
            "UPDATE agents SET name=?1, description=?2, role_id=?3, provider_id=?4, model=?5, kb_id=?6,
                    temperature=?7, max_tokens=?8, top_p=?9, updated_at=datetime('now') WHERE id=?10",
            params![
                input.name.trim(),
                input.description.clone().unwrap_or_default(),
                input.role_id.clone().unwrap_or_default(),
                input.provider_id.clone().unwrap_or_default(),
                input.model.clone().unwrap_or_default(),
                input.kb_id,
                input.temperature.unwrap_or(0.7),
                input.max_tokens.unwrap_or(2048),
                input.top_p.unwrap_or(1.0),
                id,
            ],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("智能体不存在".to_string());
    }
    Ok(())
}

/// 删除智能体
#[tauri::command]
pub fn agent_delete(db: State<'_, Database>, id: i64) -> Result<(), String> {
    let conn = db.lock_conn();
    conn.execute("DELETE FROM agents WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 智能体自动执行（非流式核心，供自动化 Worker 复用）：
/// 与 agent_chat_stream 相同的组装 —— 绑定知识库时 RAG 检索并入上下文、
/// role_id 注入角色系统提示词、按智能体配置的 provider/model/采样参数调用，
/// 一次调用返回完整文本。不写聊天记录（任务执行场景由任务库负责可观测）。
pub async fn agent_execute(
    agent: &AgentItem,
    kb_db: &KbDatabase,
    query: &str,
) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("任务内容为空".to_string());
    }
    let mut user_content = query.to_string();
    // 绑定知识库：RAG 检索并把知识上下文并入用户消息（失败时降级为直接提问）
    if let Some(kid) = agent.kb_id {
        let (emb_provider, emb_model) = {
            let conn = kb_db.conn_lock();
            conn.query_row(
                "SELECT provider_id, model FROM kb_model_settings WHERE role='embedding'",
                [],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .ok()
            .filter(|(p, m)| !p.is_empty() && !m.is_empty())
            .unwrap_or_default()
        };
        match crate::kb::rag::rag_context(
            kb_db,
            &crate::kb::rag::RagRequest {
                user_id: 1,
                kb_id: Some(kid),
                query,
                embed_provider_id: (!emb_provider.is_empty()).then_some(emb_provider.as_str()),
                embed_model: (!emb_model.is_empty()).then_some(emb_model.as_str()),
                gen_provider_id: None,
                gen_model: None,
                top_k: 5,
                mode: "hybrid",
                chunk_overrides: None,
            },
        )
        .await
        {
            Ok((_ctx, ctx_text)) => {
                if !ctx_text.trim().is_empty() {
                    user_content = format!(
                        "【知识库上下文】\n{}\n\n【用户问题】{}",
                        ctx_text.trim(),
                        query
                    );
                }
            }
            Err(e) => {
                log::warn!("智能体知识库检索失败，跳过上下文: {}", e);
            }
        }
    }
    // 角色系统提示词（role_id 非空且角色存在）
    let role_prompt = if agent.role_id.trim().is_empty() {
        String::new()
    } else {
        crate::ai_role::get_ai_roles()
            .into_iter()
            .find(|r| r.id == agent.role_id && r.enabled)
            .map(|r| crate::ai_role::compose_system_prompt(&r))
            .unwrap_or_default()
    };
    // 提供方/模型：智能体配置优先，缺失时回退全局默认提供方
    let cfg = crate::llm::config::load_config();
    let provider_id = (!agent.provider_id.is_empty())
        .then(|| agent.provider_id.clone())
        .or(cfg.default_provider_id.clone())
        .ok_or_else(|| "智能体未配置提供方，且未设置全局默认提供方".to_string())?;
    let provider = cfg
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.enabled)
        .cloned()
        .ok_or_else(|| format!("智能体的提供方不存在或已停用: {provider_id}"))?;
    let model = if agent.model.trim().is_empty() {
        provider.default_model.clone()
    } else {
        agent.model.clone()
    };
    if model.is_empty() {
        return Err("智能体未指定模型，且提供方未配置默认模型".to_string());
    }
    let mut messages = Vec::new();
    if !role_prompt.is_empty() {
        messages.push(crate::llm::types::ChatMessage {
            role: "system".to_string(),
            content: role_prompt,
            parts: None,
        });
    }
    messages.push(crate::llm::types::ChatMessage {
        role: "user".to_string(),
        content: user_content,
        parts: None,
    });
    let (text, ..) = crate::llm::client::chat_completion(
        &provider,
        &crate::llm::client::CompletionParams {
            model: &model,
            messages: &messages,
            max_tokens: Some(agent.max_tokens.clamp(1, 100_000) as u32),
            temperature: Some(agent.temperature.clamp(0.0, 2.0) as f32),
            top_p: Some(agent.top_p.clamp(0.0, 1.0) as f32),
            presence_penalty: None,
            frequency_penalty: None,
            tools: None,
            tool_choice: None,
        },
    )
    .await
    .map_err(|e| format!("智能体调用失败: {e}"))?;
    Ok(text)
}

/// 智能体流式对话：
/// - 角色：chat_with_llm_stream 按 role_id 自动注入角色系统提示词；
/// - 知识库：绑定知识库时先 RAG 检索，上下文并入用户消息；
/// - 流量与成本：chat_with_llm_stream 统一计入「大模型管理」的用量/成本；
/// - 聊天记录：持久化到 llm_chat_messages。
#[tauri::command]
pub async fn agent_chat_stream(
    db: State<'_, Database>,
    kb_db: State<'_, KbDatabase>,
    input: AgentChatInput,
    on_chunk: tauri::ipc::Channel<String>,
) -> Result<(), String> {
    let agent = {
        let conn = db.lock_conn();
        let sql = format!("SELECT {} FROM agents WHERE id = ?1", AGENT_COLS);
        conn.query_row(&sql, params![input.agent_id], row_to_agent)
            .map_err(|e| format!("智能体不存在: {}", e))?
    };
    let query = input.query.trim();
    if query.is_empty() {
        return Err("请输入要发送的内容".to_string());
    }

    let mut user_content = query.to_string();
    // 绑定知识库：RAG 检索并把知识上下文并入用户消息（失败时降级为直接提问）
    if let Some(kid) = agent.kb_id {
        let (emb_provider, emb_model) = {
            let conn = kb_db.conn_lock();
            conn.query_row(
                "SELECT provider_id, model FROM kb_model_settings WHERE role='embedding'",
                [],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .ok()
            .filter(|(p, m)| !p.is_empty() && !m.is_empty())
            .unwrap_or_default()
        };
        match crate::kb::rag::rag_context(
            &kb_db,
            &crate::kb::rag::RagRequest {
                user_id: 1,
                kb_id: Some(kid),
                query,
                embed_provider_id: (!emb_provider.is_empty()).then_some(emb_provider.as_str()),
                embed_model: (!emb_model.is_empty()).then_some(emb_model.as_str()),
                gen_provider_id: None,
                gen_model: None,
                top_k: 5,
                mode: "hybrid",
                chunk_overrides: None,
            },
        )
        .await
        {
            Ok((_ctx, ctx_text)) => {
                if !ctx_text.trim().is_empty() {
                    user_content = format!(
                        "【知识库上下文】\n{}\n\n【用户问题】{}",
                        ctx_text.trim(),
                        query
                    );
                }
            }
            Err(e) => {
                log::warn!("智能体知识库检索失败，跳过上下文: {}", e);
            }
        }
    }

    let req = crate::llm::types::ChatRequest {
        provider_id: if agent.provider_id.is_empty() {
            None
        } else {
            Some(agent.provider_id.clone())
        },
        model: if agent.model.is_empty() {
            None
        } else {
            Some(agent.model.clone())
        },
        role_id: if agent.role_id.is_empty() {
            None
        } else {
            Some(agent.role_id.clone())
        },
        messages: vec![crate::llm::types::ChatMessage {
            role: "user".to_string(),
            content: user_content,
            parts: None,
        }],
        max_tokens: Some(agent.max_tokens.clamp(1, 100_000) as u32),
        temperature: Some(agent.temperature.clamp(0.0, 2.0) as f32),
        top_p: Some(agent.top_p.clamp(0.0, 1.0) as f32),
        presence_penalty: None,
        frequency_penalty: None,
    };
    crate::llm::handlers::chat_with_llm_stream(req, on_chunk, db.clone()).await
}
