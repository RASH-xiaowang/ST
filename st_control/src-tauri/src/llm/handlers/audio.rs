// ============================================================
// 大模型管理 — IPC 命令：语音合成 / 转写
// 自 handlers.rs 拆分：TTS 全局调用、STT 云端优先 + 本地兜底、
// Windows SAPI 离线合成与配套测试。
// ============================================================

use crate::llm::client;
use crate::llm::config;
use crate::llm::types::{SpeechRequest, SpeechResult};
use base64::Engine;
use serde_json::json;

// ─── 语音合成（TTS）全局调用 ───

/// 语音合成全局调用：将文本合成为语音并返回 base64 音频
#[tauri::command]
pub async fn create_speech(request: SpeechRequest) -> Result<SpeechResult, String> {
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

    let voice = request
        .voice
        .clone()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "alloy".to_string());
    let fmt = request
        .response_format
        .clone()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "mp3".to_string());
    // 语速倍率：默认 1.0，限制 0.5~2.0（OpenAI /audio/speech 合法区间）
    let speed = request.speed.unwrap_or(1.0).clamp(0.5, 2.0);

    let (audio, format) =
        client::create_speech(&provider, &model_id, &request.input, &voice, &fmt, speed).await?;

    config::set_last_chat(&provider_id, &model_id)?;

    Ok(SpeechResult {
        provider_id,
        provider_name: provider.name,
        model: model_id,
        audio_data: base64::engine::general_purpose::STANDARD.encode(&audio),
        format,
        voice,
    })
}

/// 语音对话转写（STT）：优先本地 whisper.cpp（离线、免费），未启用/无模型/识别失败时
/// 自动回退到 OpenAI 兼容 `/audio/transcriptions`（云端 SenseVoice/Whisper 等）。
/// 返回识别文本与所用引擎，供前端展示识别来源。
#[tauri::command]
pub async fn transcribe_voice_audio(
    audio: Vec<u8>,
    ext: Option<String>,
) -> Result<serde_json::Value, String> {
    if audio.is_empty() {
        return Err("录音内容为空，请重新录制".to_string());
    }
    let ext = ext.unwrap_or_default();
    // default features（local-stt）下本地转写分支还会再次赋值，需保持 mut；
    // no-default-features 编译时该分支被 cfg 移除，故条件性保留 mut
    #[cfg_attr(not(feature = "local-stt"), allow(unused_mut))]
    let mut last_err: String;

    // 1) 云端转写（优先已配置的提供方，如硅基流动 TeleAI/TeleSpeechASR）
    {
        let cfg = config::load_config();
        match client::resolve_transcription_provider(&cfg) {
            Ok((provider, model)) => {
                match client::transcribe_audio(&provider, &model, &audio, &ext).await {
                    Ok(text) if !text.trim().is_empty() => {
                        return Ok(json!({
                            "text": text.trim(),
                            "engine": format!("云端 {}", provider.name),
                        }));
                    }
                    Ok(_) => last_err = "云端转写结果为空".to_string(),
                    Err(e) => last_err = e,
                }
            }
            Err(e) => last_err = e,
        }
    }

    // 2) 本地离线转写（whisper.cpp，feature local-stt）作为兜底
    #[cfg(feature = "local-stt")]
    {
        let stt_cfg = crate::stt::load_config();
        if stt_cfg.enabled && !stt_cfg.model_path.trim().is_empty() {
            let model_path = stt_cfg.model_path.trim().to_string();
            let language = stt_cfg.language.clone();
            let translate = stt_cfg.translate;
            let audio_clone = audio.clone();
            let local = tauri::async_runtime::spawn_blocking(move || {
                crate::stt::ensure_model_loaded(&model_path)?;
                crate::stt::transcribe_wav(&audio_clone, &language, translate)
            })
            .await;
            match local {
                Ok(Ok(text)) if !text.trim().is_empty() => {
                    return Ok(json!({ "text": text.trim(), "engine": "本地 Whisper" }));
                }
                Ok(Ok(_)) => {
                    if last_err.is_empty() {
                        last_err = "本地转写结果为空".to_string();
                    }
                }
                Ok(Err(e)) => {
                    if last_err.is_empty() {
                        last_err = e;
                    }
                }
                Err(e) => {
                    if last_err.is_empty() {
                        last_err = format!("本地转写任务异常: {}", e);
                    }
                }
            }
        }
    }

    Err(format!(
        "语音转写失败：{}（请配置硅基流动 TeleAI/TeleSpeechASR 或启用本地 Whisper 模型）",
        last_err
    ))
}

/// 系统离线语音合成（Windows SAPI）：把文本合成为 16kHz 单声道 WAV（base64），
/// 作为语音回复的零配置兜底，不依赖提供方 /audio/speech 与 WebView2 speechSynthesis。
/// `rate` 为 SAPI 语速（-10 ~ 10，默认 -2 更接近自然语速）。
#[tauri::command]
pub async fn synthesize_native_speech(
    text: String,
    rate: Option<i32>,
) -> Result<SpeechResult, String> {
    let audio = crate::native_tts::synthesize_wav(&text, rate.unwrap_or(-2).clamp(-10, 10)).await?;
    Ok(SpeechResult {
        provider_id: "windows-sapi".to_string(),
        provider_name: "Windows 系统语音".to_string(),
        model: "System.Speech（Microsoft Huihui）".to_string(),
        audio_data: base64::engine::general_purpose::STANDARD.encode(&audio),
        format: "wav".to_string(),
        voice: "Microsoft Huihui".to_string(),
    })
}

#[cfg(test)]
mod voice_transcribe_tests {
    use super::transcribe_voice_audio;

    /// 空录音必须在触碰本地模型/云端配置之前被拒绝（命令入口行为）
    #[tokio::test]
    async fn empty_audio_rejected() {
        let err = transcribe_voice_audio(Vec::new(), None).await.unwrap_err();
        assert!(
            err.contains("录音内容为空"),
            "空录音应提示重新录制，实际错误: {}",
            err
        );
    }

    /// 真实命令级云端转写：使用本机 llm_config.json 中的硅基流动密钥，
    /// 直接调用 transcribe_voice_audio 验证“云端 硅基流动”链路（默认忽略）。
    /// 运行：cargo test --lib live_transcribe_voice_command_uses_siliconflow -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_transcribe_voice_command_uses_siliconflow() {
        let voices_dir = crate::common::st_result_dir()
            .join("decoded_images")
            .join("voices");
        let mut wav = None;
        if let Ok(entries) = std::fs::read_dir(&voices_dir) {
            for e in entries.flatten() {
                let p = e.path();
                let ok_size = p
                    .metadata()
                    .map(|m| (100_000..=500_000).contains(&m.len()))
                    .unwrap_or(false);
                if p.extension().and_then(|x| x.to_str()) == Some("wav") && ok_size {
                    wav = Some(std::fs::read(&p).unwrap_or_default());
                    break;
                }
            }
        }
        let wav = wav.expect("未找到合适的缓存 WAV（100KB~500KB）");
        let res = transcribe_voice_audio(wav, Some("wav".to_string()))
            .await
            .expect("命令级转写应成功");
        let engine = res
            .get("engine")
            .and_then(|e| e.as_str())
            .unwrap_or("")
            .to_string();
        assert!(
            engine.contains("硅基流动"),
            "应使用硅基流动，实际引擎: {}",
            engine
        );
        let text = res
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        assert!(!text.trim().is_empty(), "转写文本不应为空");
        eprintln!("命令级转写: engine={} text={}", engine, text);
    }

    /// Windows SAPI 离线合成：应产出有效 16kHz 单声道 WAV（无需网络）
    #[tokio::test]
    async fn native_tts_produces_wav() {
        let audio = crate::native_tts::synthesize_wav("你好，语音合成测试", -2)
            .await
            .expect("系统语音合成应成功");
        assert!(audio.len() > 44, "WAV 不应为空");
        assert_eq!(&audio[0..4], b"RIFF", "应为 RIFF 头");
        let rate = u32::from_le_bytes([audio[24], audio[25], audio[26], audio[27]]);
        assert_eq!(rate, 16000, "应为 16kHz");
        let channels = u16::from_le_bytes([audio[22], audio[23]]);
        assert_eq!(channels, 1, "应为单声道");
    }
}
