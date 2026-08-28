// ============================================================
// 大模型客户端 — 对话补全域（chat/completions）
// 自 client.rs 拆分：消息序列化、参数对象、非流式/流式补全、
// token 兜底估算与 temperature 精度修正。
// ============================================================

use crate::llm::types::{ChatMessage, ProviderConfig};
use serde_json::{json, Value};
use std::error::Error;
use tokio_stream::StreamExt;

use super::transport::{
    apply_auth, estimate_cost, http_client, http_client_no_proxy, record_usage,
};
use super::urls::chat_url;

/// 将单条消息转换为 OpenAI 兼容的请求消息对象：
/// - 多模态 parts → content 片段数组；
/// - 其余角色直接输出 { role, content }。
pub(crate) fn build_message(m: &ChatMessage) -> Value {
    let content = match &m.parts {
        Some(parts) if !parts.is_empty() => {
            let mut arr: Vec<Value> = Vec::new();
            // 若 content 非空，作为首个文本片段（向后兼容）
            if !m.content.trim().is_empty() {
                arr.push(json!({ "type": "text", "text": m.content }));
            }
            for p in parts {
                match p.part_type.as_str() {
                    "image_url" => {
                        if let Some(iu) = &p.image_url {
                            arr.push(
                                json!({ "type": "image_url", "image_url": { "url": iu.url } }),
                            );
                        }
                    }
                    "file" => {
                        // 通用 chat 接口不支持二进制文件，转写为可读的文本描述片段
                        let name = p.name.clone().unwrap_or_default();
                        let mime = p.mime.clone().unwrap_or_default();
                        let note = format!("[用户上传文件] 名称: {} 类型: {}", name, mime);
                        arr.push(json!({ "type": "text", "text": note }));
                    }
                    _ => {
                        if let Some(t) = &p.text {
                            if !t.trim().is_empty() {
                                arr.push(json!({ "type": "text", "text": t }));
                            }
                        }
                    }
                }
            }
            Value::Array(arr)
        }
        _ => Value::String(m.content.clone()),
    };

    json!({ "role": m.role, "content": content })
}

/// LLM 补全参数（provider 单独传入；on_delta 仅流式版使用）
pub struct CompletionParams<'a> {
    pub model: &'a str,
    pub messages: &'a [ChatMessage],
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    /// 工具定义（OpenAI tools 格式的 JSON 数组）；None 表示不启用工具
    pub tools: Option<&'a Value>,
    /// 工具选择策略（"auto" / "none" / "required"）
    pub tool_choice: Option<&'a str>,
}

/// 非流式补全结果（含遥测：DSH 统计条数据源）
#[derive(Clone, Debug)]
pub struct CompletionWithTools {
    pub content: String,
    pub tool_calls: Option<Vec<crate::llm::types::ToolCall>>,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    /// 缓存命中 token（OpenAI prompt_tokens_details.cached_tokens /
    /// DeepSeek prompt_cache_hit_tokens；缺省 0）
    pub cached_tokens: u64,
    /// 请求墙钟耗时（毫秒，含重试）
    pub wall_ms: u64,
    /// 首 token / 首字节延迟（毫秒：响应体首个网络块到达时间）
    pub first_token_ms: u64,
}

/// 解析 OpenAI 非流式响应里的 tool_calls
fn parse_tool_calls(data: &Value) -> Option<Vec<crate::llm::types::ToolCall>> {
    data["choices"]
        .get(0)
        .and_then(|c| c["message"]["tool_calls"].as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    serde_json::from_value::<crate::llm::types::ToolCall>(tc.clone()).ok()
                })
                .collect()
        })
        .filter(|v: &Vec<crate::llm::types::ToolCall>| !v.is_empty())
}

/// 发起一次对话补全（支持工具调用），返回
/// (内容, tool_calls, prompt_tokens, completion_tokens, total_tokens)
pub async fn chat_completion_with_tools(
    provider: &ProviderConfig,
    params: &CompletionParams<'_>,
) -> Result<CompletionWithTools, String> {
    let msg_json: Vec<Value> = params.messages.iter().map(build_message).collect();
    let tools = match params.tools {
        Some(t) => t.clone(),
        None => json!([]),
    };
    chat_completion_with_tools_raw(
        provider,
        params.model,
        &msg_json,
        params.max_tokens,
        params.temperature,
        params.top_p,
        params.presence_penalty,
        params.frequency_penalty,
        &tools,
        params.tool_choice.unwrap_or("auto"),
    )
    .await
}

/// 底层实现：直接接受已序列化的消息数组（代理循环用，可携带
/// assistant tool_calls / role=tool 消息），返回带 tool_calls 的结果。
#[allow(clippy::too_many_arguments)]
pub async fn chat_completion_with_tools_raw(
    provider: &ProviderConfig,
    model: &str,
    messages: &[Value],
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    presence_penalty: Option<f32>,
    frequency_penalty: Option<f32>,
    tools: &Value,
    tool_choice: &str,
) -> Result<CompletionWithTools, String> {
    let started = std::time::Instant::now();
    let url = chat_url(provider, model);

    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": false,
        "tools": tools,
        "tool_choice": tool_choice,
    });
    // 推理等级透传（DSH reasoningEffort 迁移：off / high / max 等；
    // 部署级默认来自 ProviderConfig，可被会话级设置覆盖）
    if let Some(effort) = &provider.default_reasoning_effort {
        if !effort.is_empty() {
            body["reasoning_effort"] = json!(effort);
        }
    }
    if let Some(mt) = max_tokens {
        body["max_tokens"] = json!(mt);
    }
    if let Some(t) = temperature {
        body["temperature"] = json!(t);
    }
    if let Some(v) = top_p {
        body["top_p"] = json!(v);
    }
    if let Some(v) = presence_penalty {
        body["presence_penalty"] = json!(v);
    }
    if let Some(v) = frequency_penalty {
        body["frequency_penalty"] = json!(v);
    }
    if let Some(org) = &provider.organization {
        if !org.is_empty() {
            body["organization"] = json!(org);
        }
    }

    // 网络请求：对瞬时连接错误自动重试（最多 4 次），
    // 代理连不上时自动回退直连，并把底层原因展开便于诊断。
    let mut resp = None;
    let mut last_err: Option<String> = None;
    let mut use_proxy = true;
    let mut attempts = 0usize;
    while attempts < 4 {
        attempts += 1;
        let client = if use_proxy {
            http_client()
        } else {
            http_client_no_proxy()
        };
        let build_req = || {
            let body_str = serialize_body_with_temp(&body, temperature).ok()?;
            let mut req = client
                .post(&url)
                .header("Content-Type", "application/json")
                .body(body_str);
            req = apply_auth(req, provider);
            for (k, v) in &provider.extra_headers {
                req = req.header(k, v);
            }
            Some(req)
        };
        let attempt_req = match build_req() {
            Some(r) => r,
            None => break,
        };
        match attempt_req.send().await {
            Ok(r) => {
                resp = Some(r);
                break;
            }
            Err(e) => {
                let mut detail = e.to_string();
                let mut src = e.source();
                while let Some(s) = src {
                    detail.push_str(&format!("\n  └─ {}", s));
                    src = s.source();
                }
                let proxy_hint = if std::env::var("HTTPS_PROXY").is_ok()
                    || std::env::var("https_proxy").is_ok()
                    || std::env::var("ALL_PROXY").is_ok()
                {
                    if use_proxy {
                        "\n  └─ 提示：检测到代理环境变量但代理连接失败，已自动回退直连重试"
                    } else {
                        "\n  └─ 提示：已尝试直连仍失败；如本机需经代理访问，请启动代理软件后重启应用"
                    }
                } else {
                    ""
                };
                last_err = Some(format!(
                    "请求 {} 失败（第 {} 次尝试）: {}{}",
                    url, attempts, detail, proxy_hint
                ));
                // 代理连不上：立即换直连再试一次
                if e.is_connect() && use_proxy {
                    use_proxy = false;
                    log::warn!("[llm] 代理连接失败，自动回退直连: {}", url);
                    continue;
                }
                if attempts < 4 {
                    tokio::time::sleep(std::time::Duration::from_millis(500 * attempts as u64))
                        .await;
                }
            }
        }
    }
    let resp = resp.ok_or_else(|| last_err.unwrap_or_else(|| format!("请求 {} 失败", url)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("API 返回错误 {}: {}", status, text));
    }

    // 读取响应体并计量首 token / 首字节延迟：逐块收取，首个网络块的
    // 到达时间即 TTFT 代理（服务端开始产出即送达，非流式同样成立）
    // 若 body 流读取失败（网络中断 / chunk 解码错误），向上层返回可重试的错误
    let mut stream = resp.bytes_stream();
    let mut first_token_ms: Option<u64> = None;
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            // 标记为可重试错误：body 流中断不应直接失败，上层可重试整个请求
            format!("读取响应失败（网络中断，请重试）: {}", e)
        })?;
        if first_token_ms.is_none() {
            first_token_ms = Some(started.elapsed().as_millis() as u64);
        }
        body.extend_from_slice(&chunk);
    }
    // 响应体为空时也视为可重试错误
    if body.is_empty() {
        return Err("响应体为空，请重试".to_string());
    }
    let wall_ms = started.elapsed().as_millis() as u64;
    let data: Value = serde_json::from_slice(&body).map_err(|e| format!("解析响应失败: {}", e))?;

    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();
    // 推理模型（如 deepseek-v4）偶发把全部输出放进 reasoning_content 而
    // content 为空：此时回退取推理内容，避免下游拿到空文本解析失败
    let content = if content.trim().is_empty() {
        data["choices"][0]["message"]["reasoning_content"]
            .as_str()
            .unwrap_or("")
            .to_string()
    } else {
        content
    };
    let tool_calls = parse_tool_calls(&data);
    let usage = &data["usage"];
    let prompt = usage["prompt_tokens"].as_u64().unwrap_or(0);
    let completion = usage["completion_tokens"].as_u64().unwrap_or(0);
    let total = usage["total_tokens"]
        .as_u64()
        .unwrap_or(prompt + completion);
    // 缓存命中 token：OpenAI prompt_tokens_details.cached_tokens /
    // DeepSeek prompt_cache_hit_tokens（缓存命中率数据源）
    let cached_tokens = usage["prompt_tokens_details"]["cached_tokens"]
        .as_u64()
        .or_else(|| usage["prompt_cache_hit_tokens"].as_u64())
        .unwrap_or(0);

    // 统一计入「大模型管理 → 流量与成本」
    record_usage(
        provider,
        prompt,
        completion,
        total,
        estimate_cost(provider, prompt, completion),
    );

    Ok(CompletionWithTools {
        content,
        tool_calls,
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: total,
        cached_tokens,
        wall_ms,
        first_token_ms: first_token_ms.unwrap_or(wall_ms),
    })
}

/// 发起一次普通对话补全，返回 (内容, prompt_tokens, completion_tokens, total_tokens)
pub async fn chat_completion(
    provider: &ProviderConfig,
    params: &CompletionParams<'_>,
) -> Result<(String, u64, u64, u64), String> {
    let comp = chat_completion_with_tools(provider, params).await?;
    Ok((
        comp.content,
        comp.prompt_tokens,
        comp.completion_tokens,
        comp.total_tokens,
    ))
}

/// 粗略估算 token 数（用于部分 API 不在流式响应中返回 usage 时的兜底）
fn estimate_tokens(text: &str) -> u64 {
    let mut tokens = 0u64;
    let mut latin = 0usize;
    for ch in text.chars() {
        if ch.is_whitespace() {
            continue;
        }
        if (ch as u32) > 0x2E80 {
            // CJK 等宽字符大致按 1 token 计
            tokens += 1;
        } else {
            latin += 1;
        }
    }
    tokens += ((latin as f64) / 4.0).ceil() as u64;
    tokens.max(1)
}

/// 从一条流式响应 JSON 中拆出 (正文增量, 思考增量)。
/// 推理模型的 reasoning_content 是内部思考过程，不属于回答正文——
/// 必须与 content 分开返回，调用方只把 content 流入 on_delta。
fn parse_stream_delta(v: &Value) -> (Option<String>, Option<String>) {
    let choice = v["choices"].get(0);
    let content = choice
        .and_then(|c| c["delta"]["content"].as_str())
        .map(|s| s.to_string());
    let reasoning = choice
        .and_then(|c| c["delta"]["reasoning_content"].as_str())
        .map(|s| s.to_string());
    (content, reasoning)
}

#[cfg(test)]
mod stream_delta_tests {
    use super::parse_stream_delta;
    use serde_json::json;

    #[test]
    fn reasoning_content_is_separated_from_content() {
        // 仅思考增量：content 必须为 None（不得流入答案）
        let v = json!({"choices": [{"delta": {"reasoning_content": "我们需要理解当前状态"}}]});
        let (c, r) = parse_stream_delta(&v);
        assert_eq!(c, None);
        assert_eq!(r.as_deref(), Some("我们需要理解当前状态"));

        // 仅正文增量
        let v2 = json!({"choices": [{"delta": {"content": "你好！"}}]});
        let (c2, r2) = parse_stream_delta(&v2);
        assert_eq!(c2.as_deref(), Some("你好！"));
        assert_eq!(r2, None);

        // 同帧混合：两者都要拿到，但互不混淆
        let v3 = json!({"choices": [{"delta": {"content": "回答", "reasoning_content": "思考"}}]});
        let (c3, r3) = parse_stream_delta(&v3);
        assert_eq!(c3.as_deref(), Some("回答"));
        assert_eq!(r3.as_deref(), Some("思考"));

        // 空帧
        let (c4, r4) = parse_stream_delta(&json!({"choices": [{"delta": {}}]}));
        assert_eq!(c4, None);
        assert_eq!(r4, None);
    }
}

/// 流式对话补全：每收到一段内容增量就调用 on_delta，最终返回完整结果与 token 用量。
/// 兼容 OpenAI / Azure / Ollama 等返回 SSE（data: {...}）的接口。
/// on_delta 返回 false 表示调用方请求提前停止（如用户点击「停止生成」），
/// 此时立即中断流式读取并返回已累积的内容。
pub async fn chat_completion_stream<F>(
    provider: &ProviderConfig,
    params: &CompletionParams<'_>,
    mut on_delta: F,
) -> Result<(String, u64, u64, u64), String>
where
    F: FnMut(&str) -> bool,
{
    let model = params.model;
    let messages = params.messages;
    let max_tokens = params.max_tokens;
    let temperature = params.temperature;
    let top_p = params.top_p;
    let presence_penalty = params.presence_penalty;
    let frequency_penalty = params.frequency_penalty;
    let url = chat_url(provider, model);

    let msg_json: Vec<Value> = messages.iter().map(build_message).collect();

    let mut body = json!({
        "model": model,
        "messages": msg_json,
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    if let Some(mt) = max_tokens {
        body["max_tokens"] = json!(mt);
    }
    if let Some(t) = temperature {
        body["temperature"] = json!(t);
    }
    if let Some(v) = top_p {
        body["top_p"] = json!(v);
    }
    if let Some(v) = presence_penalty {
        body["presence_penalty"] = json!(v);
    }
    if let Some(v) = frequency_penalty {
        body["frequency_penalty"] = json!(v);
    }
    if let Some(org) = &provider.organization {
        if !org.is_empty() {
            body["organization"] = json!(org);
        }
    }

    let body_str = serialize_body_with_temp(&body, temperature)?;
    let build = |client: &reqwest::Client| {
        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body_str.clone());
        req = apply_auth(req, provider);
        for (k, v) in &provider.extra_headers {
            req = req.header(k, v);
        }
        req
    };
    let resp = match build(&http_client()).send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() => {
            // 代理连不上：回退直连
            log::warn!("[llm] 流式请求代理连接失败，自动回退直连: {}", url);
            build(&http_client_no_proxy())
                .send()
                .await
                .map_err(|e2| format!("请求 {} 失败: {}", url, e2))?
        }
        Err(e) => return Err(format!("请求 {} 失败: {}", url, e)),
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("API 返回错误 {}: {}", status, text));
    }

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut full = String::new();
    // 推理模型的思考过程（reasoning_content）：不属于回答正文，
    // 不能流向 on_delta（否则前端会把思考过程当答案展示并语音播报）。
    // 单独收集：仅当模型最终没有任何正文输出时才回退使用。
    let mut reasoning = String::new();
    let mut prompt_tokens: u64 = 0;
    let mut completion_tokens: u64 = 0;
    let mut stopped = false;

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("读取流失败: {}", e))?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));

        // 按行解析 SSE，逐条处理已收到的完整行（索引游标代替逐行 drain，
        // 避免长流中每行都搬移剩余缓冲导致 O(n²)）
        let mut start = 0;
        while let Some(nl) = buffer[start..].find('\n') {
            let abs_nl = start + nl;
            let line = &buffer[start..abs_nl];
            let line = line.trim();
            start = abs_nl + 1;
            if line.is_empty() || !line.starts_with("data:") {
                continue;
            }
            let data_str = line.trim_start_matches("data:").trim();
            if data_str == "[DONE]" {
                continue;
            }
            let v: Value = match serde_json::from_str::<Value>(data_str) {
                Ok(v) => v,
                Err(_) => continue,
            };
            // OpenAI 兼容接口在最后一个 chunk 附带 usage
            if let Some(usage) = v.get("usage") {
                if usage.is_object() {
                    if let Some(p) = usage["prompt_tokens"].as_u64() {
                        prompt_tokens = p;
                    }
                    if let Some(c) = usage["completion_tokens"].as_u64() {
                        completion_tokens = c;
                    }
                }
            }
            let (content_delta, reasoning_delta) = parse_stream_delta(&v);
            if let Some(d) = content_delta {
                if !d.is_empty() {
                    full.push_str(&d);
                    if !on_delta(&d) {
                        // 调用方请求停止：中断流式读取，返回已累积内容
                        stopped = true;
                        break;
                    }
                }
            }
            // 思考过程只收集不展示：若与正文同流混发，绝不能进入 on_delta
            if let Some(d) = reasoning_delta {
                if !d.is_empty() {
                    reasoning.push_str(&d);
                }
            }
        }
        // 清理已处理完的行，保留未完成的尾部（JSON 可能跨 chunk 截断）
        buffer.drain(..start);
    }

    // 极少数推理模型只输出 reasoning_content 而没有正文：
    // 此时回退用思考内容，避免下游拿到空文本（保留原兜底意图；手动停止时不回退）
    if !stopped && full.trim().is_empty() && !reasoning.trim().is_empty() {
        log::warn!(
            "[llm] 流式响应无正文，回退使用 reasoning_content（{} 字符）",
            reasoning.chars().count()
        );
        full = reasoning;
    }

    // 兜底：若流式响应未返回 usage，按字符粗略估算
    if prompt_tokens == 0 && completion_tokens == 0 {
        completion_tokens = estimate_tokens(&full);
        let prompt_text: String = messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        prompt_tokens = estimate_tokens(&prompt_text);
    }

    // 统一计入「大模型管理 → 流量与成本」
    record_usage(
        provider,
        prompt_tokens,
        completion_tokens,
        prompt_tokens + completion_tokens,
        estimate_cost(provider, prompt_tokens, completion_tokens),
    );

    Ok((
        full,
        prompt_tokens,
        completion_tokens,
        prompt_tokens + completion_tokens,
    ))
}

/// 流式 + 工具调用补全：正文增量逐段回调 on_delta（content, reasoning_delta；
/// reasoning 供「Think 推理行」展示，模型可见 ⟺ 落日志），tool_calls 分片按
/// index 合并，最终返回与 chat_completion_with_tools_raw 相同的
/// CompletionWithTools（含首 token 延迟 / 墙钟 / 缓存命中遥测）。
/// 兼容 OpenAI / Azure / DeepSeek / Ollama 的 SSE（data: {...}）。
#[allow(clippy::too_many_arguments)]
pub async fn chat_completion_with_tools_stream<F>(
    provider: &ProviderConfig,
    model: &str,
    messages: &[Value],
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    presence_penalty: Option<f32>,
    frequency_penalty: Option<f32>,
    tools: &Value,
    tool_choice: &str,
    mut on_delta: F,
) -> Result<CompletionWithTools, String>
where
    F: FnMut(&str, Option<&str>),
{
    let started = std::time::Instant::now();
    let url = chat_url(provider, model);

    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
        "tools": tools,
        "tool_choice": tool_choice,
    });
    // 推理等级透传（DSH reasoningEffort 迁移）
    if let Some(effort) = &provider.default_reasoning_effort {
        if !effort.is_empty() {
            body["reasoning_effort"] = json!(effort);
        }
    }
    if let Some(mt) = max_tokens {
        body["max_tokens"] = json!(mt);
    }
    if let Some(t) = temperature {
        body["temperature"] = json!(t);
    }
    if let Some(v) = top_p {
        body["top_p"] = json!(v);
    }
    if let Some(v) = presence_penalty {
        body["presence_penalty"] = json!(v);
    }
    if let Some(v) = frequency_penalty {
        body["frequency_penalty"] = json!(v);
    }
    if let Some(org) = &provider.organization {
        if !org.is_empty() {
            body["organization"] = json!(org);
        }
    }

    let body_str = serialize_body_with_temp(&body, temperature)?;
    let build = |client: &reqwest::Client| {
        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body_str.clone());
        req = apply_auth(req, provider);
        for (k, v) in &provider.extra_headers {
            req = req.header(k, v);
        }
        req
    };
    let resp = match build(&http_client()).send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() => {
            log::warn!("[llm] 流式工具请求代理连接失败，自动回退直连: {}", url);
            build(&http_client_no_proxy())
                .send()
                .await
                .map_err(|e2| format!("请求 {} 失败: {}", url, e2))?
        }
        Err(e) => return Err(format!("请求 {} 失败: {}", url, e)),
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("API 返回错误 {}: {}", status, text));
    }

    // 工具调用分片（OpenAI 流式：delta.tool_calls 数组，同 index 多次分片）
    #[derive(Default)]
    struct Frag {
        id: String,
        name: String,
        arguments: String,
    }
    let mut frags: Vec<Frag> = Vec::new();
    let mut full = String::new();
    let mut reasoning = String::new();
    let mut prompt_tokens: u64 = 0;
    let mut completion_tokens: u64 = 0;
    let mut cached_tokens: u64 = 0;
    let mut first_token_ms: Option<u64> = None;

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("读取流失败: {}", e))?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));
        let mut start = 0;
        while let Some(nl) = buffer[start..].find('\n') {
            let abs_nl = start + nl;
            let line = &buffer[start..abs_nl];
            let line = line.trim();
            start = abs_nl + 1;
            if line.is_empty() || !line.starts_with("data:") {
                continue;
            }
            let data_str = line.trim_start_matches("data:").trim();
            if data_str == "[DONE]" {
                continue;
            }
            let v: Value = match serde_json::from_str::<Value>(data_str) {
                Ok(v) => v,
                Err(_) => continue,
            };
            // 末块 usage（OpenAI/DeepSeek include_usage）：含缓存命中字段
            if let Some(usage) = v.get("usage") {
                if usage.is_object() {
                    if let Some(p) = usage["prompt_tokens"].as_u64() {
                        prompt_tokens = p;
                    }
                    if let Some(c) = usage["completion_tokens"].as_u64() {
                        completion_tokens = c;
                    }
                    cached_tokens = usage["prompt_tokens_details"]["cached_tokens"]
                        .as_u64()
                        .or_else(|| usage["prompt_cache_hit_tokens"].as_u64())
                        .unwrap_or(0);
                }
            }
            let choice = v["choices"].get(0);
            // 工具调用分片合并
            if let Some(tc_arr) = choice.and_then(|c| c["delta"]["tool_calls"].as_array()) {
                for tc in tc_arr {
                    let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                    while frags.len() <= idx {
                        frags.push(Frag::default());
                    }
                    let frag = &mut frags[idx];
                    if let Some(id) = tc["id"].as_str() {
                        frag.id = id.to_string();
                    }
                    if let Some(name) = tc["function"]["name"].as_str() {
                        frag.name.push_str(name);
                    }
                    if let Some(args) = tc["function"]["arguments"].as_str() {
                        frag.arguments.push_str(args);
                    }
                }
            }
            let (content_delta, reasoning_delta) = parse_stream_delta(&v);
            if let Some(d) = content_delta {
                if !d.is_empty() {
                    full.push_str(&d);
                    on_delta(&d, None);
                }
            }
            if let Some(d) = reasoning_delta {
                if !d.is_empty() {
                    reasoning.push_str(&d);
                    on_delta("", Some(&d));
                }
            }
            // 首 token / 首字节延迟：首个有效增量（正文/思考/工具分片）
            if first_token_ms.is_none()
                && (!full.is_empty() || !reasoning.is_empty() || !frags.is_empty())
            {
                first_token_ms = Some(started.elapsed().as_millis() as u64);
            }
        }
        buffer.drain(..start);
    }
    let wall_ms = started.elapsed().as_millis() as u64;

    // 推理模型「先思考后调工具」的回合：有 tool_calls 时无正文是正常行为，
    // 不回退、不告警（思考已由调用方经 reasoning_delta 单独收集展示）。
    // 仅「无正文且无工具调用」的退化响应（纯思考无输出）才回退为正文。
    if full.trim().is_empty() && !reasoning.trim().is_empty() && frags.is_empty() {
        log::warn!(
            "[llm] 流式工具响应无正文，回退使用 reasoning_content（{} 字符）",
            reasoning.chars().count()
        );
        full = reasoning;
    }
    // usage 兜底估算
    if prompt_tokens == 0 && completion_tokens == 0 {
        let prompt_text: String = messages
            .iter()
            .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
        prompt_tokens = estimate_tokens(&prompt_text);
        completion_tokens = estimate_tokens(&full);
    }
    // 组装工具调用（缺 id 时合成，保证 ToolResult 可关联）
    let tool_calls = if frags.is_empty() {
        None
    } else {
        Some(
            frags
                .into_iter()
                .map(|f| crate::llm::types::ToolCall {
                    id: if f.id.is_empty() {
                        format!("call_{}", uuid::Uuid::new_v4().simple())
                    } else {
                        f.id
                    },
                    call_type: "function".to_string(),
                    function: crate::llm::types::ToolCallFunction {
                        name: f.name,
                        arguments: f.arguments,
                    },
                })
                .collect(),
        )
    };

    record_usage(
        provider,
        prompt_tokens,
        completion_tokens,
        prompt_tokens + completion_tokens,
        estimate_cost(provider, prompt_tokens, completion_tokens),
    );

    Ok(CompletionWithTools {
        content: full,
        tool_calls,
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
        cached_tokens,
        wall_ms,
        first_token_ms: first_token_ms.unwrap_or(wall_ms),
    })
}

/// 将 temperature 格式化为至多 2 位小数的 JSON Number。
/// 部分 API（如智谱 GLM）拒绝超过 2 位小数的 temperature 值。
/// 由于 serde_json 在序列化 f32/f64 时会输出最短表示（0.7 而非 0.70），
/// 这里在字符串层面对 temperature 字段做精确替换，确保输出形如 0.70。
fn serialize_body_with_temp(
    body: &serde_json::Value,
    temperature: Option<f32>,
) -> Result<String, String> {
    let mut body_str =
        serde_json::to_string(body).map_err(|e| format!("序列化请求体失败: {}", e))?;
    if let Some(t) = temperature {
        let formatted = format!("{:.2}", (t * 100.0).round() / 100.0);
        let key = "\"temperature\":";
        if let Some(idx) = body_str.find(key) {
            let after = idx + key.len();
            let bytes = body_str.as_bytes();
            let mut end = after;
            while end < bytes.len() {
                let b = bytes[end];
                if b == b',' || b == b'}' {
                    break;
                }
                end += 1;
            }
            let mut new_body = String::with_capacity(body_str.len() + 4);
            new_body.push_str(&body_str[..after]);
            new_body.push_str(&formatted);
            new_body.push_str(&body_str[end..]);
            body_str = new_body;
        }
    }
    Ok(body_str)
}
