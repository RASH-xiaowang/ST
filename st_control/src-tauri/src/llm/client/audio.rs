// ============================================================
// 大模型客户端 — 音频域（转写 STT / 合成 TTS）
// 自 client.rs 拆分：语音转写（multipart 上传 + 代理回退重试）、
// 语音合成、ASR 模型识别与转写提供方解析。
// ============================================================

use crate::llm::types::{LlmConfig, ProviderConfig};
use serde_json::{json, Value};
use std::error::Error;

use super::transport::{apply_auth, http_client, http_client_no_proxy, record_usage};
use super::urls::{speech_url, transcription_url};

/// 根据音频字节魔数嗅探实际格式（部分服务端会忽略 response_format）
fn sniff_audio_format(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 4 && &bytes[0..4] == b"RIFF" {
        "wav"
    } else if bytes.starts_with(b"OggS") {
        "ogg"
    } else if bytes.starts_with(b"fLaC") || bytes.starts_with(b"FLAC") {
        "flac"
    } else {
        // ID3 / MPEG 帧头或无特征时统一按 mp3 处理（未知格式回退）
        "mp3"
    }
}

/// 语音转写（STT）：兼容 OpenAI `/audio/transcriptions`，multipart 上传音频，
/// 返回识别文本。带代理回退与指数退避重试（与 post_json_with_retry 同策略）。
pub async fn transcribe_audio(
    provider: &ProviderConfig,
    model: &str,
    audio_bytes: &[u8],
    ext: &str,
) -> Result<String, String> {
    let url = transcription_url(provider);
    let ext = if ext.is_empty() { "wav" } else { ext };
    let mime = match ext.to_lowercase().as_str() {
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "webm" => "audio/webm",
        "m4a" | "mp4" => "audio/mp4",
        "amr" => "audio/amr",
        "flac" => "audio/flac",
        _ => "audio/wav",
    };
    let mut last_err: Option<String> = None;
    let mut use_proxy = true;
    let mut attempts = 0usize;
    while attempts < 4 {
        attempts += 1;
        let file_part = reqwest::multipart::Part::bytes(audio_bytes.to_vec())
            .file_name(format!("voice.{}", ext))
            .mime_str(mime)
            .map_err(|e| format!("构造音频文件失败: {}", e))?;
        // OpenAI Whisper 支持 language 参数；部分平台（如硅基流动 TeleAI/TeleSpeechASR、
        // FunAudioLLM/SenseVoiceSmall）未声明该字段，仅对 whisper 系模型发送，避免请求被拒。
        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", model.to_string());
        if model.to_lowercase().contains("whisper") {
            form = form.text("language", "zh");
        }
        let client = if use_proxy {
            http_client()
        } else {
            http_client_no_proxy()
        };
        let mut req = client.post(&url).multipart(form);
        req = apply_auth(req, provider);
        for (k, v) in &provider.extra_headers {
            req = req.header(k, v);
        }
        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                if status.is_success() {
                    let v: Value = serde_json::from_str(&text)
                        .map_err(|e| format!("解析转写响应失败: {}（{}）", e, text))?;
                    let out = v
                        .get("text")
                        .and_then(|t| t.as_str())
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();
                    if out.is_empty() {
                        return Err(format!("转写接口未返回文本（响应: {}）", text));
                    }
                    // 统一计入「大模型管理 → 流量与成本」（转写按调用次数计）
                    record_usage(provider, 0, 0, 0, 0.0);
                    return Ok(out);
                }
                return Err(format!(
                    "转写接口返回错误 {}: {}（模型 {}，提供方 {}）",
                    status, text, model, provider.name
                ));
            }
            Err(e) => {
                let mut detail = format!("{}", e);
                let mut src = e.source();
                while let Some(s) = src {
                    detail.push_str(&format!(" → {}", s));
                    src = s.source();
                }
                last_err = Some(format!("转写请求失败: {}（{}）", url, detail));
                if use_proxy {
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
    Err(last_err.unwrap_or_else(|| format!("转写请求失败: {}", url)))
}

/// 判断模型名是否为语音转写（ASR）类模型
pub(crate) fn is_transcription_model(m: &str) -> bool {
    let l = m.to_lowercase();
    l.contains("sensevoice")
        || l.contains("whisper")
        || l.contains("fun_audio")
        || l.contains("audio-transcri")
        || l.contains("telespeech")
        || l.contains("speechasr")
        || l == "asr"
}

/// 寻找支持语音转写的提供方与模型
/// （模型名含 SenseVoice / whisper / TeleSpeechASR / fun_audio / audio-transcri 等）。
/// 硅基流动未在模型列表列出转写模型时，自动补默认 `FunAudioLLM/SenseVoiceSmall`。
pub fn resolve_transcription_provider(cfg: &LlmConfig) -> Result<(ProviderConfig, String), String> {
    for p in &cfg.providers {
        if !p.enabled {
            continue;
        }
        // 模型列表之外，默认模型也算候选（新增提供方时默认模型可能尚未加入列表）
        let mut candidates = p.models.clone();
        if !p.default_model.is_empty() && !candidates.contains(&p.default_model) {
            candidates.push(p.default_model.clone());
        }
        for m in &candidates {
            if is_transcription_model(m) {
                return Ok((p.clone(), m.clone()));
            }
        }
    }
    // 硅基流动默认提供 SenseVoiceSmall（免费、中文识别效果好）
    for p in &cfg.providers {
        if p.enabled && p.base_url.to_lowercase().contains("siliconflow") {
            return Ok((p.clone(), "FunAudioLLM/SenseVoiceSmall".to_string()));
        }
    }
    Err(
        "未找到支持语音转写的提供方。请在 LLM 设置中添加带 SenseVoice/Whisper/TeleSpeechASR 的提供方（如硅基流动 TeleAI/TeleSpeechASR 或 FunAudioLLM/SenseVoiceSmall）"
            .to_string(),
    )
}

/// 语音合成（TTS）：兼容 OpenAI /audio/speech，返回音频字节与格式
/// `speed` 为语速倍率（0.5~2.0）
pub async fn create_speech(
    provider: &ProviderConfig,
    model: &str,
    input: &str,
    voice: &str,
    response_format: &str,
    speed: f64,
) -> Result<(Vec<u8>, String), String> {
    if input.trim().is_empty() {
        return Err("语音合成输入文本为空".to_string());
    }

    let url = speech_url(provider);
    let fmt = if response_format.is_empty() {
        "mp3".to_string()
    } else {
        response_format.to_string()
    };
    let body = json!({
        "model": model,
        "input": input,
        "voice": voice,
        "response_format": fmt,
        "speed": speed,
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
        .map_err(|e| format!("请求语音合成接口失败: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("语音合成接口返回错误 {}: {}", status, text));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取语音合成响应失败: {}", e))?;

    if bytes.is_empty() {
        return Err("语音合成接口返回了空音频".to_string());
    }

    // 以服务端实际返回为准（嗅探魔数），无视请求中的 response_format
    let actual_format = sniff_audio_format(&bytes).to_string();
    // 统一计入「大模型管理 → 流量与成本」（语音合成按调用次数计）
    record_usage(provider, 0, 0, 0, 0.0);
    Ok((bytes.to_vec(), actual_format))
}
