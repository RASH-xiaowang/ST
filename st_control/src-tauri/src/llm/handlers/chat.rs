// ============================================================
// 大模型管理 — IPC 命令：全局对话调用
// 自 handlers.rs 拆分：AI 角色提示词注入、非流式/流式对话、
// 配额管控与助手消息持久化。
// ============================================================

use crate::ai_role;
use crate::llm::client;
use crate::llm::config;
use crate::llm::types::{ChatMessage, ChatRequest, ChatResult};
use serde_json::json;
use tauri::ipc::Channel;
use tauri::State;

// ─── 全局调用 ───

/// 若请求携带 role_id，则从共享角色库读取角色并合成系统提示词，插入到消息列表最前。
pub(crate) fn inject_role_system_prompt(messages: &mut Vec<ChatMessage>, role_id: &Option<String>) {
    let Some(id) = role_id else { return };
    if id.trim().is_empty() {
        return;
    }
    if let Some(role) = ai_role::get_ai_role(id.clone()) {
        let prompt = ai_role::compose_system_prompt(&role);
        if !prompt.is_empty() {
            messages.insert(
                0,
                ChatMessage {
                    role: "system".to_string(),
                    content: prompt,
                    parts: None,
                },
            );
        }
    }
}

/// 统一的全局调用入口：选择提供方与模型，执行对话，记录用量并执行配额管控
#[tauri::command]
pub async fn chat_with_llm(request: ChatRequest) -> Result<ChatResult, String> {
    let cfg = config::load_config();

    // 1. 选定提供方
    let provider_id = match &request.provider_id {
        Some(id) => id.clone(),
        None => cfg
            .default_provider_id
            .clone()
            .ok_or_else(|| "未配置默认提供方，请在「接入配置」中设置".to_string())?,
    };
    let provider = config::find_provider(&cfg, &provider_id)
        .ok_or_else(|| "指定的提供方不存在".to_string())?
        .clone();

    if !provider.enabled {
        return Err("该提供方已被禁用".to_string());
    }

    // 2. 选定模型
    let model = request
        .model
        .clone()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| provider.default_model.clone());
    if model.is_empty() {
        return Err("未指定模型，且提供方未配置默认模型".to_string());
    }

    // 3. 配额管控（token）
    let usage = config::current_month_usage(&provider_id);
    if let Some(limit) = provider.monthly_token_limit {
        if usage.total_tokens >= limit {
            return Err(format!("该提供方本月 token 配额已用尽（上限 {}）", limit));
        }
    }
    // 配额管控（成本）
    if let Some(limit) = provider.monthly_cost_limit {
        if usage.cost >= limit {
            return Err(format!("该提供方本月成本配额已用尽（上限 ${:.2}）", limit));
        }
    }

    // 4. 注入 AI 角色系统提示词（跨模块角色复用）
    let mut messages = request.messages.clone();
    inject_role_system_prompt(&mut messages, &request.role_id);

    // 5. 执行调用
    let (content, prompt, completion, total) = client::chat_completion(
        &provider,
        &client::CompletionParams {
            model: &model,
            messages: &messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
            presence_penalty: request.presence_penalty,
            frequency_penalty: request.frequency_penalty,
            tools: None,
            tool_choice: None,
        },
    )
    .await?;

    // 5. 用量与成本已由 client::chat_completion 统一计入「大模型管理 → 流量与成本」
    let cost = client::estimate_cost(&provider, prompt, completion);

    Ok(ChatResult {
        content,
        model,
        provider_id,
        provider_name: provider.name,
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: total,
        cost,
    })
}

/// 流式全局调用：每个内容增量通过 on_chunk 通道实时推送到前端，
/// 结束时推送 done（含完整文本），出错时推送 error。后端同时负责记录用量与持久化助手消息。
#[tauri::command]
pub async fn chat_with_llm_stream(
    request: ChatRequest,
    on_chunk: Channel<String>,
    db: State<'_, crate::db::Database>,
) -> Result<(), String> {
    let provider_id = request
        .provider_id
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| config::load_config().default_provider_id.clone())
        .ok_or_else(|| "未指定提供方，且未配置全局默认提供方".to_string())?;

    let cfg = config::load_config();
    let provider = cfg
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| "指定的提供方不存在".to_string())?
        .clone();

    let model = request
        .model
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| provider.default_model.clone());
    if model.is_empty() {
        return Err("未指定模型，且提供方未配置默认模型".to_string());
    }

    // 配额管控（token）
    let usage = config::current_month_usage(&provider_id);
    if let Some(limit) = provider.monthly_token_limit {
        if usage.total_tokens >= limit {
            let msg = format!("该提供方本月 token 配额已用尽（上限 {}）", limit);
            let _ = on_chunk.send(json!({ "type": "error", "message": msg }).to_string());
            return Err(msg);
        }
    }
    if let Some(limit) = provider.monthly_cost_limit {
        if usage.cost >= limit {
            let msg = format!("该提供方本月成本配额已用尽（上限 ${:.2}）", limit);
            let _ = on_chunk.send(json!({ "type": "error", "message": msg }).to_string());
            return Err(msg);
        }
    }

    // 注入 AI 角色系统提示词（跨模块角色复用）
    let mut messages = request.messages.clone();
    inject_role_system_prompt(&mut messages, &request.role_id);

    let (content, prompt, completion, total) = client::chat_completion_stream(
        &provider,
        &client::CompletionParams {
            model: &model,
            messages: &messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
            presence_penalty: request.presence_penalty,
            frequency_penalty: request.frequency_penalty,
            tools: None,
            tool_choice: None,
        },
        |delta: &str| {
            let _ = on_chunk.send(json!({ "type": "delta", "content": delta }).to_string());
        },
    )
    .await?;

    // 计算成本
    let cost = client::estimate_cost(&provider, prompt, completion);

    // 通知前端完成（附带用量信息）
    let _ = on_chunk.send(
        json!({
            "type": "done",
            "content": content,
            "model": model,
            "prompt_tokens": prompt,
            "completion_tokens": completion,
            "total_tokens": total,
            "cost": cost,
        })
        .to_string(),
    );

    // 用量与成本已由 client::chat_completion_stream 统一计入「大模型管理 → 流量与成本」

    // 持久化助手消息（用户消息由前端在发送时写入，避免重复）
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    db.append_llm_chat_message(&provider_id, &model, "assistant", &content, None, &now)
        .map_err(|e| format!("保存聊天记录失败: {}", e))?;

    Ok(())
}
