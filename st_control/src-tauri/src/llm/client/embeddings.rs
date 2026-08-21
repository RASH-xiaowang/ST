// ============================================================
// 大模型客户端 — 嵌入 / 重排序域
// 自 client.rs 拆分：文本嵌入（单条/批量）、跨提供方嵌入模型解析、
// 重排序（Cohere /rerank 兼容）。
// ============================================================

use crate::llm::types::{LlmConfig, ProviderConfig, RerankItem};
use serde_json::json;

use super::transport::{
    apply_auth, estimate_cost, http_client, post_json_with_retry, record_usage,
};
use super::urls::{embedding_url, is_embedding_marked, rerank_url, resolve_embedding_model};

/// 跨提供方解析嵌入模型：优先使用指定提供方内的嵌入模型；
/// 若该提供方没有嵌入模型（例如把对话模型当嵌入用），
/// 自动切换到任意启用提供方中的嵌入模型，避免 /embeddings 报错。
pub(crate) fn resolve_embedding_provider(
    cfg: &LlmConfig,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> (ProviderConfig, String) {
    let pid = provider_id.or(cfg.default_provider_id.as_deref());
    if let Some(pid) = pid {
        if let Some(p) = cfg.providers.iter().find(|p| p.id == pid && p.enabled) {
            let req = model.filter(|m| !m.is_empty()).map(|s| s.to_string());
            let has_embed = p.models.iter().any(|m| is_embedding_marked(p, m));
            let m = req
                .clone()
                .filter(|m| is_embedding_marked(p, m))
                .or_else(|| {
                    if has_embed {
                        p.models.iter().find(|m| is_embedding_marked(p, m)).cloned()
                    } else {
                        req.clone().or_else(|| {
                            if p.default_model.is_empty() {
                                None
                            } else {
                                Some(p.default_model.clone())
                            }
                        })
                    }
                });
            if let Some(m) = m {
                // 请求的模型是嵌入类型，或该提供方本就有嵌入模型 → 直接用
                if is_embedding_marked(p, &m) || has_embed {
                    return (p.clone(), m);
                }
                // 该提供方无嵌入模型 → 跨提供方寻找
            }
        }
    }
    for p in &cfg.providers {
        if p.enabled {
            if let Some(m) = p.models.iter().find(|m| is_embedding_marked(p, m)) {
                return (p.clone(), m.clone());
            }
        }
    }
    // 兜底：原请求提供方 + 请求/默认模型（仍可能失败，错误信息会带模型名）
    if let Some(pid) = pid {
        if let Some(p) = cfg.providers.iter().find(|p| p.id == pid) {
            let m = model
                .filter(|m| !m.is_empty())
                .map(|s| s.to_string())
                .or_else(|| {
                    if p.default_model.is_empty() {
                        None
                    } else {
                        Some(p.default_model.clone())
                    }
                });
            if let Some(m) = m {
                return (p.clone(), m);
            }
        }
    }
    (
        cfg.providers.first().cloned().unwrap_or_default(),
        String::new(),
    )
}

/// 文本嵌入：兼容 OpenAI /embeddings，input 可为单条文本或多条（按行拆分）
pub async fn create_embedding(
    provider: &ProviderConfig,
    model: &str,
    input: &str,
) -> Result<(Vec<Vec<f64>>, u64, u64), String> {
    let url = embedding_url(provider);
    let model = resolve_embedding_model(provider, model);

    let inputs: Vec<String> = if input.contains('\n') {
        input
            .split('\n')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![input.to_string()]
    };
    if inputs.is_empty() {
        return Err("嵌入输入内容为空".to_string());
    }

    let body = json!({
        "model": model,
        "input": inputs,
    });

    let resp = post_json_with_retry(&url, &body, provider).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "嵌入接口返回错误 {}: {}（模型 {}，提供方 {}）",
            status, text, model, provider.name
        ));
    }

    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析嵌入响应失败: {}", e))?;
    log::info!("嵌入生成原始响应: {}", v);

    let data = v
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| "嵌入响应缺少 data 字段".to_string())?;

    let mut embeddings = Vec::with_capacity(data.len());
    for item in data {
        let emb = item
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| "嵌入项缺少 embedding 字段".to_string())?;
        let vec: Vec<f64> = emb.iter().filter_map(|x| x.as_f64()).collect();
        embeddings.push(vec);
    }

    if embeddings.is_empty() {
        return Err("嵌入响应未包含任何向量".to_string());
    }

    let usage = v.get("usage").cloned().unwrap_or(json!({}));
    let prompt_tokens = usage
        .get("prompt_tokens")
        .and_then(|t| t.as_u64())
        .or_else(|| usage.get("total_tokens").and_then(|t| t.as_u64()))
        .unwrap_or(0);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(|t| t.as_u64())
        .unwrap_or(prompt_tokens);

    // 统一计入「大模型管理 → 流量与成本」（嵌入按输入 token 计费，输出计 0）
    record_usage(
        provider,
        prompt_tokens,
        0,
        total_tokens,
        estimate_cost(provider, prompt_tokens, 0),
    );

    Ok((embeddings, prompt_tokens, total_tokens))
}

/// 批量文本嵌入：inputs 的每一项作为一个独立输入原样发送（不做按行拆分，
/// 保证分片内容含换行时也能整体嵌入），返回与 inputs 顺序一致的向量列表。
/// 带重试（指数退避）与「代理失败回退直连」，与聊天调用同等健壮。
pub async fn create_embeddings_batch(
    provider_id: Option<&str>,
    model: Option<&str>,
    inputs: &[String],
) -> Result<Vec<Vec<f64>>, String> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    // 跨提供方解析嵌入模型（当前提供方无嵌入模型时自动切换）
    let cfg = crate::llm::config::load_config();
    let (provider, model_id) = resolve_embedding_provider(&cfg, provider_id, model);
    if model_id.is_empty() {
        return Err("未找到可用的嵌入模型（请在大模型管理中配置嵌入类模型）".to_string());
    }
    if !provider.enabled {
        return Err(format!("提供方「{}」已被禁用，无法调用", provider.name));
    }
    create_embeddings_batch_with(&provider, &model_id, inputs).await
}

/// 批量嵌入核心：使用已解析的提供方直接发送（供内部与诊断测试复用）
pub(crate) async fn create_embeddings_batch_with(
    provider: &ProviderConfig,
    model_id: &str,
    inputs: &[String],
) -> Result<Vec<Vec<f64>>, String> {
    let model_id = resolve_embedding_model(provider, model_id);
    let url = embedding_url(provider);
    let body = json!({
        "model": model_id,
        "input": inputs,
    });

    let resp = post_json_with_retry(&url, &body, provider).await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "嵌入接口返回错误 {}: {}（模型 {}，提供方 {}）",
            status, text, model_id, provider.name
        ));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析嵌入响应失败: {}", e))?;
    let data = v
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| "嵌入响应缺少 data 字段".to_string())?;
    let mut out: Vec<Vec<f64>> = Vec::with_capacity(data.len());
    for item in data {
        let emb = item
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| "嵌入项缺少 embedding 字段".to_string())?;
        out.push(emb.iter().filter_map(|x| x.as_f64()).collect());
    }
    if out.len() != inputs.len() {
        return Err(format!(
            "嵌入返回数量 {} 与输入数量 {} 不一致",
            out.len(),
            inputs.len()
        ));
    }
    // 统一计入「大模型管理 → 流量与成本」（批量嵌入按调用次数计）
    record_usage(provider, 0, 0, 0, 0.0);
    Ok(out)
}

/// 重排序：兼容 Cohere /rerank（results: [{index, relevance_score}]）；
/// 同时容忍 {data:[{index, score}]} 与纯数组形态
pub async fn rerank(
    provider: &ProviderConfig,
    model: &str,
    query: &str,
    documents: &[String],
    top_n: Option<u32>,
) -> Result<Vec<RerankItem>, String> {
    if query.trim().is_empty() {
        return Err("重排序查询内容为空".to_string());
    }
    if documents.is_empty() {
        return Err("重排序文档列表为空".to_string());
    }

    let url = rerank_url(provider);
    let body = json!({
        "model": model,
        "query": query,
        "documents": documents,
        "top_n": top_n,
    });

    let client = http_client();
    let mut req = client.post(&url).json(&body);
    req = apply_auth(req, provider);
    for (k, v) in &provider.extra_headers {
        req = req.header(k, v);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("请求重排序接口失败: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("重排序接口返回错误 {}: {}", status, text));
    }

    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析重排序响应失败: {}", e))?;
    log::info!("重排序原始响应: {}", v);

    let parse_items = |arr: &[serde_json::Value]| -> Vec<RerankItem> {
        arr.iter()
            .map(|it| {
                let index = it.get("index").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                let score = it
                    .get("relevance_score")
                    .or_else(|| it.get("score"))
                    .and_then(|x| x.as_f64())
                    .unwrap_or(0.0);
                let document = documents
                    .get(index as usize)
                    .cloned()
                    .or_else(|| {
                        it.get("document")
                            .and_then(|d| d.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();
                RerankItem {
                    index,
                    document,
                    score,
                }
            })
            .collect()
    };

    let items = if let Some(arr) = v.get("results").and_then(|r| r.as_array()) {
        parse_items(arr)
    } else if let Some(arr) = v.get("data").and_then(|r| r.as_array()) {
        parse_items(arr)
    } else if let Some(arr) = v.as_array() {
        parse_items(arr)
    } else {
        return Err("重排序响应格式无法识别（缺少 results / data 数组）".to_string());
    };

    // 统一计入「大模型管理 → 流量与成本」（重排序为轻量调用，按调用次数计）
    record_usage(provider, 0, 0, 0, 0.0);

    Ok(items)
}
