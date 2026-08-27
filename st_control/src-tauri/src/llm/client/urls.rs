// ============================================================
// 大模型客户端 — URL / API 端点构造辅助
// 自 client.rs 拆分：纯路径构造，不发起网络请求，便于独立维护与测试。
// ============================================================

use crate::llm::types::{ProviderConfig, ProviderType};

/// 去除 base_url 末尾的 `/`，避免拼接出 `//chat/completions`
pub(crate) fn normalize_base_url(base: &str) -> String {
    let b = base.trim();
    if let Some(stripped) = b.strip_suffix('/') {
        stripped.to_string()
    } else {
        b.to_string()
    }
}

/// 判断 base_url 是否只有主机（无路径），用于自动补全 /v1
fn is_host_only(base: &str) -> bool {
    match reqwest::Url::parse(base) {
        Ok(u) => u.path().is_empty() || u.path() == "/",
        Err(_) => false,
    }
}

/// 计算 API 路径前缀：主机-only 的 base_url（如 https://api.siliconflow.cn）
/// 自动补全 /v1，OpenAI 兼容网关（含 Ollama /v1 兼容端点）都遵循该约定；
/// Azure 使用 /openai/... 前缀，不补 /v1；已带路径（如 …/api/paas/v4）则原样保留。
pub(crate) fn api_base(provider: &ProviderConfig) -> String {
    let base = normalize_base_url(&provider.base_url);
    match provider.provider_type {
        ProviderType::Azure => base,
        _ => {
            if is_host_only(&base) && !base.ends_with("/v1") {
                format!("{}/v1", base)
            } else {
                base
            }
        }
    }
}

/// 解析实际发送的嵌入模型：若提供方内已有标记为「嵌入」类型的模型，
/// 且请求的模型为空或不是嵌入类型（例如误选了对话模型），
/// 自动回退到该提供方的嵌入模型，避免 /embeddings 报「Model does not exist」。
pub(crate) fn resolve_embedding_model(provider: &ProviderConfig, requested: &str) -> String {
    let embed_models: Vec<&String> = provider
        .models
        .iter()
        .filter(|m| {
            provider
                .model_meta
                .get(*m)
                .and_then(|meta| meta.model_type.as_deref())
                .map(|t| t == "嵌入" || t.eq_ignore_ascii_case("embedding"))
                .unwrap_or(false)
        })
        .collect();
    if !embed_models.is_empty() {
        let req = requested.trim();
        if req.is_empty() || !embed_models.iter().any(|m| m.as_str() == req) {
            return embed_models[0].clone();
        }
    }
    requested.to_string()
}

/// 判断模型是否被标记为「嵌入」类型
pub(crate) fn is_embedding_marked(provider: &ProviderConfig, model: &str) -> bool {
    provider
        .model_meta
        .get(model)
        .and_then(|meta| meta.model_type.as_deref())
        .map(|t| t == "嵌入" || t.eq_ignore_ascii_case("embedding"))
        .unwrap_or(false)
}

/// 根据提供方类型构造对应的聊天补全 URL
pub(crate) fn chat_url(provider: &ProviderConfig, model: &str) -> String {
    let base = normalize_base_url(&provider.base_url);
    match provider.provider_type {
        ProviderType::Azure => {
            // Azure OpenAI：部署名即模型名，api-version 作为查询参数
            let mut url = format!("{}/openai/deployments/{}/chat/completions", base, model);
            if let Some(v) = &provider.azure_api_version {
                if !v.is_empty() {
                    url.push_str(&format!("?api-version={}", v));
                }
            }
            url
        }
        _ => {
            // 统一由 api_base 处理主机-only 时补全 /v1
            format!("{}/chat/completions", api_base(provider))
        }
    }
}

/// 图像生成 URL（OpenAI 兼容 /images/generations）
pub(crate) fn image_url(provider: &ProviderConfig) -> String {
    format!("{}/images/generations", api_base(provider))
}

/// 视频生成 URL（OpenAI 兼容 /videos/generations）
pub(crate) fn video_url(provider: &ProviderConfig) -> String {
    format!("{}/videos/generations", api_base(provider))
}

pub(crate) fn embedding_url(provider: &ProviderConfig) -> String {
    format!("{}/embeddings", api_base(provider))
}

pub(crate) fn rerank_url(provider: &ProviderConfig) -> String {
    format!("{}/rerank", api_base(provider))
}

pub(crate) fn speech_url(provider: &ProviderConfig) -> String {
    match provider.provider_type {
        ProviderType::Xiaomi => {
            // 小米 MiMo TTS：端点为 {base}/mimo-v2-5-tts（非 OpenAI /audio/speech）
            let base = normalize_base_url(&provider.base_url);
            // 若用户已把模型路径写在 base_url（如 .../mimo-v2-5-tts），直接使用
            if base.ends_with("-tts") {
                base
            } else {
                format!("{}/mimo-v2-5-tts", base)
            }
        }
        _ => format!("{}/audio/speech", api_base(provider)),
    }
}

pub(crate) fn transcription_url(provider: &ProviderConfig) -> String {
    format!("{}/audio/transcriptions", api_base(provider))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(base: &str, ptype: ProviderType) -> ProviderConfig {
        ProviderConfig {
            id: "t".into(),
            name: "t".into(),
            provider_type: ptype,
            base_url: base.into(),
            api_key: "k".into(),
            default_model: "m".into(),
            models: vec![],
            enabled: true,
            azure_api_version: Some("2024-02-15-preview".into()),
            ..Default::default()
        }
    }

    #[test]
    fn normalize_base_url_strips_trailing_slash() {
        assert_eq!(
            normalize_base_url("https://api.x.com/"),
            "https://api.x.com"
        );
        assert_eq!(normalize_base_url("https://api.x.com"), "https://api.x.com");
        assert_eq!(
            normalize_base_url("  https://api.x.com/  "),
            "https://api.x.com"
        );
    }

    #[test]
    fn api_base_auto_v1_for_host_only() {
        // 主机-only → 补 /v1；已带路径 → 原样
        assert_eq!(
            api_base(&provider("https://api.sf.cn", ProviderType::OpenAI)),
            "https://api.sf.cn/v1"
        );
        assert_eq!(
            api_base(&provider("https://api.sf.cn/", ProviderType::OpenAI)),
            "https://api.sf.cn/v1"
        );
        // 已带路径（如 …/api/paas/v4）原样
        assert_eq!(
            api_base(&provider("https://x.cn/api/paas/v4", ProviderType::OpenAI)),
            "https://x.cn/api/paas/v4"
        );
        // 已带 /v1 不重复
        assert_eq!(
            api_base(&provider("https://api.sf.cn/v1", ProviderType::OpenAI)),
            "https://api.sf.cn/v1"
        );
        // Azure 不补 /v1
        assert_eq!(
            api_base(&provider("https://az.openai.com", ProviderType::Azure)),
            "https://az.openai.com"
        );
    }

    #[test]
    fn chat_url_branches_openai_and_azure() {
        // OpenAI 兼容：api_base + /chat/completions
        assert_eq!(
            chat_url(&provider("https://api.sf.cn", ProviderType::OpenAI), "m"),
            "https://api.sf.cn/v1/chat/completions"
        );
        // Azure：/openai/deployments/<模型>/chat/completions + api-version
        let az = provider("https://az.openai.com", ProviderType::Azure);
        assert_eq!(
            chat_url(&az, "gpt-4"),
            "https://az.openai.com/openai/deployments/gpt-4/chat/completions?api-version=2024-02-15-preview"
        );
        // 无 api-version → 不带查询参数
        let mut az2 = provider("https://az.openai.com", ProviderType::Azure);
        az2.azure_api_version = None;
        assert_eq!(
            chat_url(&az2, "gpt-4"),
            "https://az.openai.com/openai/deployments/gpt-4/chat/completions"
        );
    }

    #[test]
    fn embedding_model_resolution_falls_back_to_marked_model() {
        // 请求模型为空或非嵌入类型 → 回退到标记为嵌入的模型
        let mut p = provider("https://api.sf.cn", ProviderType::OpenAI);
        p.models = vec!["chat-m".into(), "embed-m".into()];
        p.model_meta.insert(
            "embed-m".into(),
            crate::llm::types::ModelMeta {
                model_type: Some("嵌入".into()),
                ..Default::default()
            },
        );
        // 请求为空 → 回退
        assert_eq!(resolve_embedding_model(&p, ""), "embed-m");
        // 请求是对话模型（非嵌入）→ 回退
        assert_eq!(resolve_embedding_model(&p, "chat-m"), "embed-m");
        // 请求本身就是嵌入模型 → 原样
        assert_eq!(resolve_embedding_model(&p, "embed-m"), "embed-m");
        // 无嵌入模型 → 原样
        let p2 = provider("https://api.sf.cn", ProviderType::OpenAI);
        assert_eq!(resolve_embedding_model(&p2, "x"), "x");
        assert!(is_embedding_marked(&p, "embed-m"));
        assert!(!is_embedding_marked(&p, "chat-m"));
    }
}
