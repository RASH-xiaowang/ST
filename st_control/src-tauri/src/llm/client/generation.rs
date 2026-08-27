// ============================================================
// 大模型客户端 — 生成域（图像 / 视频）
// 自 client.rs 拆分：图像生成、视频生成（同步 + 异步任务轮询）。
// ============================================================

use crate::llm::types::ProviderConfig;
use serde_json::{json, Value};

use super::transport::{apply_auth, http_client, record_usage};
use super::urls::{api_base, image_url, video_url};

/// 发起一次图像生成，返回生成的图像 URL 列表（兼容 OpenAI /images/generations）。
/// 自动处理 data URL 与 https URL 两种返回形式。
pub async fn generate_image(
    provider: &ProviderConfig,
    model: &str,
    prompt: &str,
    n: u32,
    size: Option<&str>,
) -> Result<Vec<String>, String> {
    let client = http_client();
    let url = image_url(provider);

    let mut body = json!({
        "model": model,
        "prompt": prompt,
        "n": n,
    });
    if let Some(s) = size {
        if !s.is_empty() {
            body["size"] = json!(s);
        }
    }

    let mut req = client.post(&url).json(&body);
    req = apply_auth(req, provider);
    for (k, v) in &provider.extra_headers {
        req = req.header(k, v);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("请求 {} 失败: {}", url, e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("API 返回错误 {}: {}", status, text));
    }

    let data: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let mut urls: Vec<String> = Vec::new();
    if let Some(arr) = data["data"].as_array() {
        for item in arr {
            if let Some(u) = item["url"].as_str() {
                urls.push(u.to_string());
            } else if let Some(b64) = item["b64_json"].as_str() {
                let mime = item["mime_type"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "image/png".to_string());
                urls.push(format!("data:{};base64,{}", mime, b64));
            }
        }
    }
    if urls.is_empty() {
        return Err("图像生成未返回任何结果".to_string());
    }
    // 统一计入「大模型管理 → 流量与成本」（图像 API 不返回 token，按调用次数计）
    record_usage(provider, 0, 0, 0, 0.0);
    Ok(urls)
}

/// 发起一次视频生成，返回生成的视频 URL 列表。
/// 兼容多种响应结构：
/// - 同步的 { data:[{url}] } 系列；
/// - 异步任务式（如 SiliconFlow / CogVideoX：先返回 task_id，再轮询 /video/status/{id}）。
///
/// 部分模型（如 SiliconFlow 的视频模型）不支持 SYNC 同步调用，会以
/// 400 + code 1212（"当前模型不支持SYNC调用方式"）拒绝 /videos/generations，
/// 此时自动切换到异步任务式（/video/submit → /video/status/{id}）。
pub async fn generate_video(
    provider: &ProviderConfig,
    model: &str,
    prompt: &str,
    n: u32,
) -> Result<Vec<String>, String> {
    let result = generate_video_inner(provider, model, prompt, n).await;
    if result.is_ok() {
        // 统一计入「大模型管理 → 流量与成本」（视频 API 不返回 token，按调用次数计）
        record_usage(provider, 0, 0, 0, 0.0);
    }
    result
}

async fn generate_video_inner(
    provider: &ProviderConfig,
    model: &str,
    prompt: &str,
    n: u32,
) -> Result<Vec<String>, String> {
    let client = http_client();
    let url = video_url(provider);

    let body = json!({
        "model": model,
        "prompt": prompt,
        "n": n,
    });

    let mut req = client.post(&url).json(&body);
    req = apply_auth(req, provider);
    for (k, v) in &provider.extra_headers {
        req = req.header(k, v);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("请求 {} 失败: {}", url, e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        // 模型不支持 SYNC 同步调用：切换到异步任务式（如 SiliconFlow /video/submit）
        if needs_async_flow(&text) {
            log::info!("视频模型不支持 SYNC 调用（{}），改用异步任务式提交", text);
            return run_async_video(provider, model, prompt, n).await;
        }
        return Err(format!("API 返回错误 {}: {}", status, text));
    }

    let data: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    log::info!("视频生成原始响应: {}", data);

    // 1) 同步结构：尝试多种常见形态
    if let Some(urls) = extract_video_urls(&data) {
        if !urls.is_empty() {
            return Ok(urls);
        }
    }

    // 2) 异步任务（返回 task_id / id，但未直接带视频地址）
    if let Some(tid) = extract_task_id(&data) {
        return poll_video_task(provider, &tid).await;
    }

    // 3) 仍无法解析：打印真实响应，便于定位接口结构
    let preview = data.to_string();
    let preview = if preview.len() > 1000 {
        // 安全截断：按字符边界切分，避免多字节 UTF-8 字符 panic
        let truncated: String = preview.chars().take(1000).collect();
        format!("{}…(已截断)", truncated)
    } else {
        preview
    };
    log::error!("视频生成响应无法解析: {}", preview);
    Err(format!("视频生成返回了未预期的结构：{}", preview))
}

/// 判断接口错误是否表示需要切换到异步任务式调用。
fn needs_async_flow(text: &str) -> bool {
    let lowered = text.to_lowercase();
    if lowered.contains("1212") {
        return true;
    }
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        let msg = v
            .pointer("/error/message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_lowercase();
        let code = v
            .pointer("/error/code")
            .and_then(|c| c.as_str())
            .or_else(|| v.get("code").and_then(|c| c.as_str()))
            .unwrap_or("");
        if msg.contains("sync") || msg.contains("异步") || msg.contains("task") {
            return true;
        }
        if code == "1212" {
            return true;
        }
    }
    false
}

/// 从响应中提取任务 ID（兼容 SiliconFlow 的 requestId / data.requestId，
/// 以及通用的 task_id / data.task_id / id / taskId）。
fn extract_task_id(data: &Value) -> Option<String> {
    data.get("requestId")
        .or_else(|| data.get("data").and_then(|d| d.get("requestId")))
        .or_else(|| data.get("task_id"))
        .or_else(|| data.get("data").and_then(|d| d.get("task_id")))
        .or_else(|| data.get("id"))
        .or_else(|| data.get("taskId"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// 异步任务式视频生成：提交任务并返回 task_id，再轮询状态。
async fn run_async_video(
    provider: &ProviderConfig,
    model: &str,
    prompt: &str,
    n: u32,
) -> Result<Vec<String>, String> {
    let task_id = submit_video_task(provider, model, prompt, n).await?;
    poll_video_task(provider, &task_id).await
}

/// 提交异步视频生成任务（兼容 SiliconFlow 风格 /video/submit）。
async fn submit_video_task(
    provider: &ProviderConfig,
    model: &str,
    prompt: &str,
    n: u32,
) -> Result<String, String> {
    let url = format!("{}/video/submit", api_base(provider));
    let body = json!({
        "model": model,
        "prompt": prompt,
        "image_size": "1280x720",
        "n": n,
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
        .map_err(|e| format!("提交视频任务 {} 失败: {}", url, e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("提交视频任务返回错误 {}: {}", status, text));
    }

    let data: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析提交响应失败: {}", e))?;
    log::info!("视频任务提交响应: {}", data);

    if let Some(tid) = extract_task_id(&data) {
        Ok(tid)
    } else {
        let preview = data.to_string();
        let preview = if preview.len() > 1000 {
            let truncated: String = preview.chars().take(1000).collect();
            format!("{}…(已截断)", truncated)
        } else {
            preview
        };
        Err(format!("视频任务提交未返回 task_id：{}", preview))
    }
}

/// 从响应值中提取视频 URL，兼容多种形态：
/// data[].url / data[].video_url / data[].video.url / videos[].* / output[].* /
/// results.videos[].* / result.videos[].* 以及任意 http(s):// 或 data: 字符串
fn extract_video_urls(v: &Value) -> Option<Vec<String>> {
    let mut urls: Vec<String> = Vec::new();

    let candidates: Vec<&Value> = vec![
        v.get("data"),
        v.get("videos"),
        v.get("output"),
        v.get("items"),
        v.get("list"),
        v.get("result"),
        v.get("results"),
    ]
    .into_iter()
    .flatten()
    .collect();

    for c in &candidates {
        if let Some(arr) = c.as_array() {
            for item in arr {
                collect_url_from_item(item, &mut urls);
            }
        } else if c.is_object() {
            collect_url_from_item(c, &mut urls);
        }
    }

    // 顶层直接出现 url / video.url / video_url
    collect_url_from_item(v, &mut urls);

    // 兜底：递归查找任意 URL 字符串
    if urls.is_empty() {
        collect_urls_recursive(v, &mut urls);
    }

    if urls.is_empty() {
        None
    } else {
        Some(urls)
    }
}

fn collect_url_from_item(item: &Value, urls: &mut Vec<String>) {
    for key in [
        "url",
        "video_url",
        "src",
        "uri",
        "download_url",
        "file_url",
        "play_url",
    ] {
        if let Some(u) = item.get(key).and_then(|x| x.as_str()) {
            if is_url_like(u) {
                urls.push(u.to_string());
            }
        }
    }
    // 嵌套 video 对象
    if let Some(vid) = item.get("video") {
        if let Some(u) = vid.get("url").and_then(|x| x.as_str()) {
            if is_url_like(u) {
                urls.push(u.to_string());
            }
        }
    }
}

fn is_url_like(u: &str) -> bool {
    u.starts_with("http://") || u.starts_with("https://") || u.starts_with("data:")
}

fn collect_urls_recursive(v: &Value, urls: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            if is_url_like(s) {
                urls.push(s.clone());
            }
        }
        Value::Array(a) => {
            for x in a {
                collect_urls_recursive(x, urls);
            }
        }
        Value::Object(m) => {
            for (_, x) in m {
                collect_urls_recursive(x, urls);
            }
        }
        _ => {}
    }
}

/// 轮询异步视频任务状态。
/// SiliconFlow 风格：POST {base}/video/status，请求体 {"requestId": "..."}，
/// 响应以 { code, msg, data:{ status, results } } 包裹。
async fn poll_video_task(provider: &ProviderConfig, task_id: &str) -> Result<Vec<String>, String> {
    let status_url = format!("{}/video/status", api_base(provider));
    log::info!(
        "视频任务 {} 提交成功，开始轮询状态：{}",
        task_id,
        status_url
    );

    let client = http_client();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    let mut attempt: u32 = 0;
    loop {
        if std::time::Instant::now() > deadline {
            return Err(format!(
                "视频生成任务 {} 轮询超时（>180s），请稍后在提供方控制台查看结果",
                task_id
            ));
        }
        attempt += 1;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // SiliconFlow 以 POST + {"requestId": "..."} 轮询状态
        let mut req = client
            .post(&status_url)
            .json(&json!({ "requestId": task_id }));
        req = apply_auth(req, provider);
        for (k, v) in &provider.extra_headers {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("轮询视频任务状态失败: {}", e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let t = resp.text().await.unwrap_or_default();
            return Err(format!("轮询视频任务状态返回错误 {}: {}", status, t));
        }
        let data: Value = resp
            .json()
            .await
            .map_err(|e| format!("解析任务状态响应失败: {}", e))?;

        // SiliconFlow 风格用 { code, msg, data:{ status, results } } 包裹，
        // 因此优先取内层 data，再回退到顶层。
        let inner = data.get("data");
        let status = data
            .get("status")
            .or_else(|| inner.and_then(|d| d.get("status")))
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_lowercase();

        if status == "failed" || status == "fail" {
            let msg = data
                .get("message")
                .or_else(|| data.get("reason"))
                .or_else(|| data.get("errmsg"))
                .or_else(|| inner.and_then(|d| d.get("reason")))
                .or_else(|| inner.and_then(|d| d.get("message")))
                .and_then(|x| x.as_str())
                .unwrap_or("未知失败原因");
            return Err(format!("视频生成任务失败：{}", msg));
        }

        if status == "succeed" || status == "success" || status == "completed" || status == "done" {
            // 优先从内层 data 解析（含 SiliconFlow 的 results.videos[].url），
            // 再回退到整包递归扫描。
            let search = inner.unwrap_or(&data);
            if let Some(urls) = extract_video_urls(search) {
                if !urls.is_empty() {
                    return Ok(urls);
                }
            }
            return Err("视频生成任务已完成，但响应中未包含视频地址".to_string());
        }

        log::info!(
            "视频任务 {} 状态：{}（第 {} 次轮询）",
            task_id,
            status,
            attempt
        );
    }
}
