// ============================================================
// 大模型管理 — IPC 命令：文本嵌入 / 重排序
// 自 handlers.rs 拆分：跨提供方嵌入模型解析、向量生成、
// 文档相关性重排。
// ============================================================

use crate::llm::client;
use crate::llm::config;
use crate::llm::types::{EmbeddingRequest, EmbeddingResult, RerankRequest, RerankResult};

// ─── 文本嵌入全局调用 ───

/// 文本嵌入全局调用：返回输入文本的向量表示（兼容 OpenAI /embeddings）
#[tauri::command]
pub async fn create_embedding(request: EmbeddingRequest) -> Result<EmbeddingResult, String> {
    let cfg = config::load_config();
    // 跨提供方解析嵌入模型：当前提供方无嵌入模型时自动切换
    let (provider, model_id) = client::resolve_embedding_provider(
        &cfg,
        request.provider_id.as_deref(),
        request.model.as_deref(),
    );
    if model_id.is_empty() {
        return Err("未找到可用的嵌入模型（请在大模型管理中配置嵌入类模型）".to_string());
    }

    if !provider.enabled {
        return Err(format!("提供方「{}」已被禁用，无法调用", provider.name));
    }

    let (embeddings, prompt_tokens, total_tokens) =
        client::create_embedding(&provider, &model_id, &request.input).await?;

    let dimensions = embeddings.first().map(|v| v.len()).unwrap_or(0);

    // 用量与成本已由 client::create_embedding 统一计入「大模型管理 → 流量与成本」
    // 嵌入调用只记录「上次嵌入模型」，不覆盖聊天记忆（last_chat）
    config::set_last_embedding(&provider.id, &model_id)?;

    Ok(EmbeddingResult {
        provider_id: provider.id.clone(),
        provider_name: provider.name,
        model: model_id,
        embeddings,
        dimensions,
        prompt_tokens,
        total_tokens,
    })
}

// ─── 重排序全局调用 ───

/// 重排序全局调用：根据 query 对 documents 排序并返回相关性得分（兼容 Cohere /rerank）
#[tauri::command]
pub async fn rerank(request: RerankRequest) -> Result<RerankResult, String> {
    let cfg = config::load_config();
    let (provider, provider_id, model_id) = match &request.provider_id {
        Some(pid) => {
            let p = cfg
                .providers
                .iter()
                .find(|p| p.id == *pid)
                .ok_or_else(|| format!("未找到指定提供方: {}", pid))?;
            let m = request
                .model
                .clone()
                .filter(|m| !m.is_empty())
                .or_else(|| {
                    if p.default_model.is_empty() {
                        None
                    } else {
                        Some(p.default_model.clone())
                    }
                })
                .ok_or_else(|| format!("提供方「{}」未指定模型且无默认模型", p.name))?;
            (p.clone(), pid.clone(), m)
        }
        None => {
            let pid = cfg.default_provider_id.clone().ok_or_else(|| {
                "未设置默认提供方，请先在「接入配置」中选择或指定 provider_id".to_string()
            })?;
            let p = cfg
                .providers
                .iter()
                .find(|p| p.id == pid)
                .ok_or_else(|| format!("未找到默认提供方: {}", pid))?;
            let m = request
                .model
                .clone()
                .filter(|m| !m.is_empty())
                .or_else(|| {
                    if p.default_model.is_empty() {
                        None
                    } else {
                        Some(p.default_model.clone())
                    }
                })
                .ok_or_else(|| format!("提供方「{}」未指定模型且无默认模型", p.name))?;
            (p.clone(), pid, m)
        }
    };

    if !provider.enabled {
        return Err(format!("提供方「{}」已被禁用，无法调用", provider.name));
    }

    let results = client::rerank(
        &provider,
        &model_id,
        &request.query,
        &request.documents,
        request.top_n,
    )
    .await?;

    // 调用次数已由 client::rerank 统一计入「大模型管理 → 流量与成本」
    config::set_last_chat(&provider_id, &model_id)?;

    Ok(RerankResult {
        provider_id,
        provider_name: provider.name,
        model: model_id,
        results,
    })
}
