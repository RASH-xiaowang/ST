// ============================================================
// 大模型客户端 — 公共传输层
// 自 client.rs 拆分：HTTP 客户端 / 代理回退重试 / 鉴权 / 用量记录，
// 各 API 域（对话、生图、视频、嵌入、语音等）统一经此收发请求。
// ============================================================

use crate::llm::types::{ProviderConfig, ProviderType};
use std::error::Error;

/// 带超时的 HTTP 客户端（避免连接挂起导致界面无响应）
/// 产品归属标识（DSH 2026-06-21 mandatory-app-attribution-headers：
/// 提供方请求按 RFC 9110 User-Agent 标识发出请求的产品）
const APP_USER_AGENT: &str = concat!("ST-Control/", env!("CARGO_PKG_VERSION"));

pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// 忽略代理环境变量、强制直连的客户端（代理不可用时回退）
pub(crate) fn http_client_no_proxy() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .timeout(std::time::Duration::from_secs(90))
        .no_proxy()
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// 发送带鉴权的 POST JSON 请求：默认走系统代理客户端；
/// 传输层失败（DNS / TLS / 连接被重置 / 超时等）时回退直连并指数退避重试，
/// 避免系统代理失效导致嵌入等请求整体失败。返回原始响应供调用方解析。
pub(crate) async fn post_json_with_retry(
    url: &str,
    body: &serde_json::Value,
    provider: &ProviderConfig,
) -> Result<reqwest::Response, String> {
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
        let mut req = client.post(url).json(body);
        req = apply_auth(req, provider);
        for (k, v) in &provider.extra_headers {
            req = req.header(k, v);
        }
        match req.send().await {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                // 完整错误链，便于定位是 DNS / TLS / 连接被重置 / 超时
                let mut detail = format!("{}", e);
                let mut src = e.source();
                while let Some(s) = src {
                    detail.push_str(&format!(" → {}", s));
                    src = s.source();
                }
                last_err = Some(format!("请求接口失败: {}（{}）", url, detail));
                if use_proxy {
                    // 任何传输层失败都先回退直连重试（系统代理可能已失效/被禁）
                    use_proxy = false;
                    continue;
                }
                if attempts < 4 {
                    tokio::time::sleep(std::time::Duration::from_millis(500 * attempts as u64))
                        .await;
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| format!("请求接口失败: {}", url)))
}

/// 根据提供方类型附加鉴权信息
pub(crate) fn apply_auth(
    req: reqwest::RequestBuilder,
    p: &ProviderConfig,
) -> reqwest::RequestBuilder {
    match p.provider_type {
        ProviderType::Azure => req.header("api-key", &p.api_key),
        ProviderType::Ollama => req, // 本地一般无需鉴权
        _ => req.bearer_auth(&p.api_key),
    }
}

/// 统一用量记录：所有大模型调用（对话/生图/视频/语音/嵌入/重排序/转写等）
/// 必须经由此处计入「大模型管理 → 流量与成本」，便于集中统计。
/// 记录失败只告警，不阻断业务调用（统计不应影响功能可用性）。
pub(crate) fn record_usage(
    provider: &ProviderConfig,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    cost: f64,
) {
    if let Err(e) = crate::llm::config::add_usage(
        &provider.id,
        prompt_tokens,
        completion_tokens,
        total_tokens,
        cost,
    ) {
        log::warn!("[llm] 用量记录失败（提供方 {}）: {}", provider.id, e);
    }
}

/// 根据 token 用量与单价估算成本（USD）
pub fn estimate_cost(provider: &ProviderConfig, prompt_tokens: u64, completion_tokens: u64) -> f64 {
    let p = prompt_tokens as f64 / 1_000_000.0 * provider.input_price_per_1m;
    let c = completion_tokens as f64 / 1_000_000.0 * provider.output_price_per_1m;
    p + c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ProviderConfig {
        ProviderConfig {
            id: "t".into(),
            name: "t".into(),
            provider_type: ProviderType::OpenAI,
            base_url: "http://x".into(),
            api_key: "k".into(),
            default_model: "m".into(),
            models: vec![],
            enabled: true,
            input_price_per_1m: 1.0,
            output_price_per_1m: 2.0,
            ..Default::default()
        }
    }

    #[test]
    fn estimate_cost_scales_by_tokens_and_prices() {
        let p = provider();
        // 0 token → 0 成本
        assert_eq!(estimate_cost(&p, 0, 0), 0.0);
        // 1M 输入 token @ $1/1M = $1；1M 输出 @ $2/1M = $2；合计 $3
        assert_eq!(estimate_cost(&p, 1_000_000, 1_000_000), 3.0);
        // 500K 输入 + 250K 输出 = 0.5 + 0.5 = 1.0
        assert_eq!(estimate_cost(&p, 500_000, 250_000), 1.0);
        // 极小量：10 token 输入 = 1e-5 * 1 = 0.00001
        let small = estimate_cost(&p, 10, 0);
        assert!(
            (small - 0.00001).abs() < 1e-12,
            "10 token 输入成本: {small}"
        );
    }
}
