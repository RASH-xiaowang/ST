// ============================================================
// 大模型管理 — 外部 API 客户端
// 基于 reqwest 调用 OpenAI 兼容的 chat/completions 与 models 接口，
// 支持 openai / azure / ollama / custom 四种鉴权方式。
// ============================================================

mod audio;
pub(crate) mod chat;
mod embeddings;
mod generation;
mod probe;
mod transport;
mod urls;

pub use audio::{create_speech, resolve_transcription_provider, transcribe_audio};
pub use chat::{
    chat_completion, chat_completion_stream, chat_completion_with_tools_raw,
    chat_completion_with_tools_stream, CompletionParams,
};
pub(crate) use embeddings::resolve_embedding_provider;
pub use embeddings::{create_embedding, create_embeddings_batch, rerank};
pub use generation::{generate_image, generate_video};
pub use probe::{fetch_models, test_connection};
pub use transport::estimate_cost;

#[cfg(test)]
mod resolve_tests {
    use super::audio::is_transcription_model;
    use super::urls::resolve_embedding_model;
    use super::*;
    use crate::llm::types::{ModelMeta, ProviderConfig, ProviderType};

    fn provider_with_embedding() -> ProviderConfig {
        ProviderConfig {
            id: "p1".into(),
            name: "硅基流动".into(),
            provider_type: ProviderType::OpenAI,
            base_url: "https://api.siliconflow.cn".into(),
            default_model: "Qwen/Qwen3-VL-Embedding-8B".into(),
            models: vec![
                "tencent/Hunyuan-MT-7B".into(),
                "Qwen/Qwen3-VL-Embedding-8B".into(),
                "BAAI/bge-m3".into(),
            ],
            model_meta: [
                (
                    "tencent/Hunyuan-MT-7B".into(),
                    ModelMeta {
                        model_type: Some("对话".into()),
                        tags: vec![],
                        reasoning_efforts: Vec::new(),
                        context_window: None,
                    },
                ),
                (
                    "Qwen/Qwen3-VL-Embedding-8B".into(),
                    ModelMeta {
                        model_type: Some("嵌入".into()),
                        tags: vec![],
                        reasoning_efforts: Vec::new(),
                        context_window: None,
                    },
                ),
                (
                    "BAAI/bge-m3".into(),
                    ModelMeta {
                        model_type: Some("嵌入".into()),
                        tags: vec![],
                        reasoning_efforts: Vec::new(),
                        context_window: None,
                    },
                ),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn test_resolve_embedding_model_fallback_when_chat_model_selected() {
        let p = provider_with_embedding();
        // 误选了对话模型 → 自动回退到提供方首个嵌入模型
        assert_eq!(
            resolve_embedding_model(&p, "tencent/Hunyuan-MT-7B"),
            "Qwen/Qwen3-VL-Embedding-8B"
        );
        // 空模型 → 同样回退
        assert_eq!(
            resolve_embedding_model(&p, ""),
            "Qwen/Qwen3-VL-Embedding-8B"
        );
        // 已选嵌入模型 → 保持
        assert_eq!(resolve_embedding_model(&p, "BAAI/bge-m3"), "BAAI/bge-m3");
    }

    #[test]
    fn test_resolve_embedding_model_keeps_request_when_no_marked_models() {
        let p = ProviderConfig {
            id: "p2".into(),
            name: "无标记".into(),
            provider_type: ProviderType::OpenAI,
            base_url: "https://example.com/v1".into(),
            default_model: "model-a".into(),
            models: vec!["model-a".into(), "model-b".into()],
            ..Default::default()
        };
        // 提供方没有标记「嵌入」的模型时，按请求原样发送
        assert_eq!(resolve_embedding_model(&p, "model-b"), "model-b");
        assert_eq!(resolve_embedding_model(&p, ""), "");
    }

    #[test]
    fn test_resolve_embedding_provider_cross_provider_fallback() {
        // DeepSeek 只有对话模型（无嵌入模型）→ 自动切到硅基流动的嵌入模型
        let deepseek = ProviderConfig {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            provider_type: ProviderType::OpenAI,
            base_url: "https://api.deepseek.com".into(),
            default_model: "deepseek-v4-flash".into(),
            models: vec!["deepseek-v4-flash".into(), "deepseek-v4-pro".into()],
            model_meta: [(
                "deepseek-v4-flash".into(),
                ModelMeta {
                    model_type: Some("对话".into()),
                    tags: vec![],
                    reasoning_efforts: Vec::new(),
                    context_window: None,
                },
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        let silicon = provider_with_embedding();
        let cfg = crate::llm::types::LlmConfig {
            default_provider_id: Some("deepseek".into()),
            providers: vec![deepseek.clone(), silicon],
            ..Default::default()
        };
        let (p, m) = resolve_embedding_provider(&cfg, Some("deepseek"), Some("deepseek-v4-flash"));
        assert_eq!(p.id, "p1", "应切换到有嵌入模型的提供方");
        assert_eq!(m, "Qwen/Qwen3-VL-Embedding-8B");
    }

    #[test]
    fn test_resolve_transcription_provider_silicon_flow_fallback() {
        let p = ProviderConfig {
            id: "sf".into(),
            name: "硅基流动".into(),
            provider_type: ProviderType::OpenAI,
            base_url: "https://api.siliconflow.cn".into(),
            default_model: "Qwen/Qwen3-VL-Embedding-8B".into(),
            models: vec!["BAAI/bge-m3".into()],
            ..Default::default()
        };
        let cfg = crate::llm::types::LlmConfig {
            providers: vec![p],
            ..Default::default()
        };
        let (p2, m2) = resolve_transcription_provider(&cfg).expect("硅基流动应有默认转写模型");
        assert_eq!(p2.id, "sf");
        assert_eq!(m2, "FunAudioLLM/SenseVoiceSmall");
    }

    #[test]
    fn test_resolve_transcription_provider_prefers_sensevoice_in_list() {
        let p = ProviderConfig {
            id: "p".into(),
            name: "x".into(),
            provider_type: ProviderType::OpenAI,
            base_url: "https://example.com/v1".into(),
            default_model: "a".into(),
            models: vec!["chat-a".into(), "FunAudioLLM/SenseVoiceSmall".into()],
            ..Default::default()
        };
        let cfg = crate::llm::types::LlmConfig {
            providers: vec![p],
            ..Default::default()
        };
        let (_, m) = resolve_transcription_provider(&cfg).unwrap();
        assert_eq!(m, "FunAudioLLM/SenseVoiceSmall");
    }

    #[test]
    fn test_resolve_transcription_provider_telespeech() {
        let p = ProviderConfig {
            id: "sf".into(),
            name: "硅基流动".into(),
            provider_type: ProviderType::OpenAI,
            base_url: "https://api.siliconflow.cn/v1".into(),
            // 新增提供方时默认模型可能尚未加入模型列表，也应能识别
            default_model: "TeleAI/TeleSpeechASR".into(),
            models: vec![],
            ..Default::default()
        };
        let cfg = crate::llm::types::LlmConfig {
            providers: vec![p],
            ..Default::default()
        };
        let (p2, m2) = resolve_transcription_provider(&cfg).expect("TeleSpeechASR 应被识别");
        assert_eq!(p2.base_url, "https://api.siliconflow.cn/v1");
        assert_eq!(m2, "TeleAI/TeleSpeechASR");
    }

    #[test]
    fn test_is_transcription_model() {
        assert!(is_transcription_model("TeleAI/TeleSpeechASR"));
        assert!(is_transcription_model("FunAudioLLM/SenseVoiceSmall"));
        assert!(is_transcription_model("whisper-1"));
        assert!(!is_transcription_model("deepseek-v4-flash"));
        assert!(!is_transcription_model("Qwen/Qwen3-8B"));
    }

    /// 真实语音转写（联网，默认忽略；本地验证用 `--ignored` 运行）
    #[tokio::test]
    #[ignore]
    async fn live_transcribe_cached_wav() {
        let cfg = crate::llm::config::load_config();
        let (provider, model) = resolve_transcription_provider(&cfg)
            .map_err(|e| panic!("{}", e))
            .unwrap();
        let voices_dir = crate::common::st_result_dir()
            .join("decoded_images")
            .join("voices");
        let mut wav = None;
        if let Ok(entries) = std::fs::read_dir(&voices_dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("wav") {
                    wav = Some(std::fs::read(&p).unwrap_or_default());
                    break;
                }
            }
        }
        let Some(wav) = wav else {
            eprintln!("未找到缓存 WAV，跳过");
            return;
        };
        assert!(!wav.is_empty());
        let text = transcribe_audio(&provider, &model, &wav, "wav")
            .await
            .expect("转写应成功");
        eprintln!("转写结果: {}", text);
        assert!(!text.trim().is_empty(), "转写文本不应为空");
    }
}

#[cfg(test)]
mod probe_tests {
    use super::chat::{chat_completion, CompletionParams};
    use super::embeddings::create_embeddings_batch_with;
    use crate::llm::types::{ChatMessage, ProviderConfig, ProviderType};

    #[tokio::test]
    #[ignore = "手动诊断用：需要外网访问 SiliconFlow"]
    async fn probe_siliconflow_connect() {
        let provider = ProviderConfig {
            id: "probe".into(),
            name: "probe".into(),
            provider_type: ProviderType::OpenAI,
            base_url: "https://api.siliconflow.cn".into(),
            api_key: "sk-probe-dummy".into(),
            ..Default::default()
        };
        let messages = vec![ChatMessage {
            role: "user".into(),
            content: "hi".into(),
            parts: None,
        }];
        match chat_completion(
            &provider,
            &CompletionParams {
                model: "tencent/Hunyuan-MT-7B",
                messages: &messages,
                max_tokens: Some(16),
                temperature: Some(0.4),
                top_p: None,
                presence_penalty: None,
                frequency_penalty: None,
                tools: None,
                tool_choice: None,
            },
        )
        .await
        {
            Ok((c, _, _, _)) => println!("PROBE OK: {}", c),
            Err(e) => println!("PROBE ERR: {}", e),
        }
    }

    #[tokio::test]
    #[ignore = "手动诊断用：需要外网访问 SiliconFlow"]
    async fn probe_siliconflow_embeddings() {
        // 用假密钥验证嵌入链路（URL / TLS / 请求体）本身可通：
        // 预期得到 401 API 错误，而不是「error sending request」传输错误。
        let provider = ProviderConfig {
            id: "probe".into(),
            name: "probe".into(),
            provider_type: ProviderType::OpenAI,
            base_url: "https://api.siliconflow.cn".into(),
            api_key: "sk-probe-dummy".into(),
            default_model: "Qwen/Qwen3-VL-Embedding-8B".into(),
            models: vec!["Qwen/Qwen3-VL-Embedding-8B".into()],
            ..Default::default()
        };
        let inputs = vec!["第一段测试内容".to_string(), "第二段测试内容".to_string()];
        match create_embeddings_batch_with(&provider, &provider.default_model, &inputs).await {
            Ok(v) => println!("EMBED PROBE OK: dim={:?}", v.first().map(|x| x.len())),
            Err(e) => println!("EMBED PROBE ERR: {}", e),
        }
    }
}
