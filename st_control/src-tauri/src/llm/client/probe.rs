// ============================================================
// 大模型客户端 — 模型列表 / 连接探测
// 自 client.rs 拆分：候选端点构造、响应解析、模型列表探测
// 与最小补全连接测试（ASR 模型改用 /models 探活）。
// ============================================================

use crate::llm::types::{ChatMessage, ProviderConfig, ProviderType};
use serde_json::Value;

use super::audio::is_transcription_model;
use super::chat::{chat_completion, CompletionParams};
use super::transport::{apply_auth, http_client, http_client_no_proxy};
use super::urls::{api_base, normalize_base_url};

/// 模型列表响应的解析方式
enum ModelParseKind {
    /// OpenAI 兼容：data[].id
    OpenAI,
    /// Ollama 原生：models[].name
    OllamaTags,
}

/// 依据提供方类型返回候选的模型列表端点（按顺序尝试，首个成功即返回）
fn models_endpoints(provider: &ProviderConfig) -> Vec<(String, ModelParseKind)> {
    let base = normalize_base_url(&provider.base_url);
    let mut list: Vec<(String, ModelParseKind)> = Vec::new();
    match provider.provider_type {
        ProviderType::Ollama => {
            // 优先用原生 /api/tags，回退到 OpenAI 兼容 /v1/models
            list.push((format!("{}/api/tags", base), ModelParseKind::OllamaTags));
            list.push((format!("{}/v1/models", base), ModelParseKind::OpenAI));
            list.push((format!("{}/models", base), ModelParseKind::OpenAI));
        }
        ProviderType::Azure => {
            if let Some(v) = &provider.azure_api_version {
                if !v.is_empty() {
                    list.push((
                        format!("{}/openai/models?api-version={}", base, v),
                        ModelParseKind::OpenAI,
                    ));
                }
            }
            list.push((format!("{}/openai/models", base), ModelParseKind::OpenAI));
            list.push((format!("{}/models", base), ModelParseKind::OpenAI));
        }
        _ => {
            // OpenAI / 自定义兼容网关：优先 /models，base 不含 /v1 时回退 /v1/models
            list.push((format!("{}/models", base), ModelParseKind::OpenAI));
            if !base.ends_with("/v1") {
                list.push((format!("{}/v1/models", base), ModelParseKind::OpenAI));
            }
        }
    }
    list
}

fn parse_models(data: &Value, kind: ModelParseKind) -> Vec<String> {
    let mut models: Vec<String> = Vec::new();
    match kind {
        ModelParseKind::OpenAI => {
            if let Some(arr) = data["data"].as_array() {
                for item in arr {
                    if let Some(id) = item["id"].as_str() {
                        models.push(id.to_string());
                    }
                }
            }
        }
        ModelParseKind::OllamaTags => {
            if let Some(arr) = data["models"].as_array() {
                for item in arr {
                    if let Some(name) = item["name"].as_str() {
                        models.push(name.to_string());
                    }
                }
            }
        }
    }
    models
}

/// 探测提供方支持的模型列表，按类型尝试多个候选端点
pub async fn fetch_models(provider: &ProviderConfig) -> Result<Vec<String>, String> {
    let client = http_client();
    let endpoints = models_endpoints(provider);

    let mut last_error = "未配置有效的模型列表端点".to_string();
    for (url, kind) in endpoints {
        let mut req = client.get(&url);
        req = apply_auth(req, provider);
        for (k, v) in &provider.extra_headers {
            req = req.header(k, v);
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(_) => {
                // 代理连不上时回退直连
                let direct = http_client_no_proxy().get(&url);
                let direct = apply_auth(direct, provider);
                match direct.send().await {
                    Ok(r) => r,
                    Err(e2) => {
                        last_error = format!("请求 {} 失败: {}", url, e2);
                        continue;
                    }
                }
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            last_error = format!("API 返回错误 {}: {}", status, text);
            continue;
        }

        let data: Value = match resp.json().await {
            Ok(d) => d,
            Err(e) => {
                last_error = format!("解析响应失败: {}", e);
                continue;
            }
        };

        let models = parse_models(&data, kind);
        return Ok(models);
    }

    Err(format!("探测失败：{}", last_error))
}

/// 连接测试：用默认模型发起一次最小补全（max_tokens=1），返回 (是否成功, 耗时ms, 模型, 错误信息)
pub async fn test_connection(
    provider: &ProviderConfig,
) -> (bool, u128, Option<String>, Option<String>) {
    let model = if provider.default_model.is_empty() {
        "gpt-3.5-turbo"
    } else {
        &provider.default_model
    };
    let start = std::time::Instant::now();

    // ASR/语音转写模型不支持 /chat/completions（如硅基流动 TeleAI/TeleSpeechASR），
    // 改用 GET /models 探活：验密钥与连通性即可。
    if is_transcription_model(model) {
        let url = format!("{}/models", api_base(provider));
        let mut req = http_client().get(&url);
        req = apply_auth(req, provider);
        for (k, v) in &provider.extra_headers {
            req = req.header(k, v);
        }
        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                return (
                    true,
                    start.elapsed().as_millis(),
                    Some(model.to_string()),
                    None,
                );
            }
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return (
                    false,
                    start.elapsed().as_millis(),
                    Some(model.to_string()),
                    Some(format!("模型列表接口返回错误 {}: {}", status, text)),
                );
            }
            Err(e) => {
                return (
                    false,
                    start.elapsed().as_millis(),
                    Some(model.to_string()),
                    Some(format!("请求失败: {}", e)),
                );
            }
        }
    }

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: "ping".to_string(),
        parts: None,
    }];

    match chat_completion(
        provider,
        &CompletionParams {
            model,
            messages: &messages,
            max_tokens: Some(1),
            temperature: Some(0.0),
            top_p: None,
            presence_penalty: None,
            frequency_penalty: None,
            tools: None,
            tool_choice: None,
        },
    )
    .await
    {
        Ok((_, _, _, _)) => (
            true,
            start.elapsed().as_millis(),
            Some(model.to_string()),
            None,
        ),
        Err(e) => (
            false,
            start.elapsed().as_millis(),
            Some(model.to_string()),
            Some(e),
        ),
    }
}
