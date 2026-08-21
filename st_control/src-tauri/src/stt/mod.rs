//! 本地离线语音转写（whisper.cpp 绑定，MIT 开源，无需联网/API）
//!
//! - 模型：OpenAI Whisper GGML（tiny ~39MB / base ~142MB / small ~466MB），
//!   支持 99 种语言自动识别（language=auto），也可指定语言。
//! - 输入：silk_decoder_rs 输出的 WAV（PCM16 单声道 24kHz），内部重采样到 16kHz。
//! - 配置：`<应用数据目录>/stt_config.json`，模型默认放同目录 models/。
//! - 首次加载模型较慢（百毫秒~秒级），加载后常驻内存复用。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// ============ 配置 ============

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SttConfig {
    pub enabled: bool,
    pub model_path: String,
    /// "auto" 或 Whisper 语言码（zh/en/ja/ko/…）
    pub language: String,
    /// 是否把非英文语音翻译为英文（Whisper translate）
    pub translate: bool,
    /// 最近一次使用的下载尺寸（tiny/base/small），用于 UI 记忆
    pub model_size: String,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            model_path: String::new(),
            language: "auto".to_string(),
            translate: false,
            model_size: "base".to_string(),
        }
    }
}

pub const SUPPORTED_LANGUAGES: &[(&str, &str)] = &[
    ("auto", "自动检测"),
    ("zh", "中文"),
    ("en", "英语"),
    ("ja", "日语"),
    ("ko", "韩语"),
    ("yue", "粤语"),
    ("fr", "法语"),
    ("de", "德语"),
    ("es", "西班牙语"),
    ("ru", "俄语"),
    ("pt", "葡萄牙语"),
    ("it", "意大利语"),
    ("ar", "阿拉伯语"),
    ("th", "泰语"),
    ("vi", "越南语"),
    ("id", "印尼语"),
];

pub const AVAILABLE_MODELS: &[(&str, &str)] = &[
    ("tiny", "Tiny（约 78MB，最快）"),
    ("base", "Base（约 148MB，推荐）"),
    ("small", "Small（约 485MB，最准）"),
];

pub fn config_dir() -> PathBuf {
    crate::common::st_data_dir()
}

pub fn config_path() -> PathBuf {
    config_dir().join("stt_config.json")
}

pub fn default_model_dir() -> PathBuf {
    config_dir().join("models")
}

pub fn model_file_for_size(size: &str) -> String {
    format!("ggml-{}.bin", size)
}

pub fn load_config() -> SttConfig {
    match std::fs::read_to_string(config_path()) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or_default(),
        _ => SttConfig::default(),
    }
}

pub fn save_config(cfg: &SttConfig) -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(config_path(), json)?;
    Ok(())
}

// ============ 引擎（whisper.cpp 常驻实例） ============

struct EngineState {
    ctx: whisper_rs::WhisperContext,
    model_path: String,
}

static ENGINE: OnceLock<Mutex<Option<EngineState>>> = OnceLock::new();

fn engine() -> &'static Mutex<Option<EngineState>> {
    ENGINE.get_or_init(|| Mutex::new(None))
}

pub fn model_loaded(model_path: &str) -> bool {
    engine()
        .lock()
        .map(|g| {
            g.as_ref()
                .map(|s| s.model_path == model_path)
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

pub fn ensure_model_loaded(model_path: &str) -> Result<(), String> {
    if !Path::new(model_path).exists() {
        return Err(format!("模型文件不存在: {}", model_path));
    }
    let mut guard = engine().lock().unwrap_or_else(|e| e.into_inner());
    if guard
        .as_ref()
        .map(|s| s.model_path == model_path)
        .unwrap_or(false)
    {
        return Ok(());
    }
    // 更换模型时释放旧实例（whisper.cpp 模型对象较大）
    *guard = None;
    let mut params = whisper_rs::WhisperContextParameters::default();
    params.use_gpu(false);
    let t0 = std::time::Instant::now();
    let ctx = whisper_rs::WhisperContext::new_with_params(model_path, params)
        .map_err(|e| format!("加载 Whisper 模型失败: {}", e))?;
    log::info!(
        "[stt] 本地模型已加载: {}（耗时 {:.1}s）",
        model_path,
        t0.elapsed().as_secs_f64()
    );
    *guard = Some(EngineState {
        ctx,
        model_path: model_path.to_string(),
    });
    Ok(())
}

// ============ WAV 解析与重采样 ============

/// 解析 RIFF/WAVE 音频数据：返回 (PCM16 单声道采样, 采样率)。
/// 兼容常见 16-bit PCM；若为多声道则取第一声道。
fn parse_wav_pcm16(data: &[u8]) -> Result<(Vec<i16>, u32), String> {
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("无效的 WAV 文件".to_string());
    }
    let channels = u16::from_le_bytes([data[22], data[23]]) as usize;
    let sample_rate = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
    let bits = u16::from_le_bytes([data[34], data[35]]);

    // 扫描 data 块（头部可能带扩展，不假设固定偏移 44）
    let mut off = 12usize;
    let mut data_start = None;
    let mut data_len = 0usize;
    while off + 8 <= data.len() {
        let id = &data[off..off + 4];
        let len = u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]])
            as usize;
        if id == b"data" {
            data_start = Some(off + 8);
            data_len = len.min(data.len().saturating_sub(off + 8));
            break;
        }
        off += 8 + len + (len & 1); // 块按 2 字节对齐
    }
    let data_start = data_start.ok_or("WAV 缺少 data 块")?;
    let pcm = &data[data_start..data_start + data_len];

    if bits != 16 {
        return Err(format!("不支持的位深: {} bit（仅支持 16-bit PCM）", bits));
    }
    let samples_per_ch = pcm.len() / 2 / channels.max(1);
    let mut out = Vec::with_capacity(samples_per_ch);
    for i in 0..samples_per_ch {
        let base = i * 2 * channels;
        out.push(i16::from_le_bytes([pcm[base], pcm[base + 1]]));
    }
    Ok((out, sample_rate))
}

/// 线性插值重采样（语音场景足够；whisper 对轻微混叠不敏感）
fn resample_pcm16(src: &[i16], from: u32, to: u32) -> Vec<i16> {
    if src.is_empty() || from == to {
        return src.to_vec();
    }
    let out_len = (src.len() as u64 * to as u64 / from as u64) as usize;
    let mut out = Vec::with_capacity(out_len);
    let ratio = from as f64 / to as f64;
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos.floor() as usize;
        let frac = pos - idx as f64;
        let a = src.get(idx).copied().unwrap_or(0) as f64;
        let b = src.get(idx + 1).copied().map(|v| v as f64).unwrap_or(a);
        out.push((a + (b - a) * frac) as i16);
    }
    out
}

/// 对 WAV 做本地转写，返回识别文本
///
/// 注意：CPU 密集（whisper 推理），调用方应放入 spawn_blocking。
pub fn transcribe_wav(wav: &[u8], language: &str, translate: bool) -> Result<String, String> {
    use whisper_rs::{FullParams, SamplingStrategy};

    let guard = engine().lock().unwrap_or_else(|e| e.into_inner());
    let state = guard.as_ref().ok_or("本地转写模型未加载")?;

    let (pcm, sample_rate) = parse_wav_pcm16(wav)?;
    let pcm16 = if sample_rate != 16_000 {
        resample_pcm16(&pcm, sample_rate, 16_000)
    } else {
        pcm
    };
    let samples: Vec<f32> = pcm16.iter().map(|&s| s as f32 / 32768.0).collect();
    if samples.is_empty() {
        return Err("语音内容为空".to_string());
    }

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    if language.is_empty() || language == "auto" {
        params.set_language(None); // 自动检测
    } else {
        params.set_language(Some(language));
    }
    params.set_translate(translate);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_no_timestamps(true);
    params.set_n_threads(
        std::thread::available_parallelism()
            .map(|n| n.get().min(8) as i32)
            .unwrap_or(4),
    );

    let mut st = state
        .ctx
        .create_state()
        .map_err(|e| format!("创建转写状态失败: {}", e))?;
    st.full(params, &samples)
        .map_err(|e| format!("转写失败: {}", e))?;

    let n = st.full_n_segments();
    let mut text = String::new();
    for i in 0..n {
        if let Some(seg) = st.get_segment(i) {
            if let Ok(t) = seg.to_str() {
                text.push_str(t);
            }
        }
    }
    Ok(text.trim().to_string())
}

// ============ IPC ============

fn status_json(cfg: &SttConfig) -> serde_json::Value {
    let model_path = cfg.model_path.trim().to_string();
    let exists = !model_path.is_empty() && Path::new(&model_path).exists();
    let size = if exists {
        std::fs::metadata(&model_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };
    serde_json::json!({
        "enabled": cfg.enabled,
        "model_path": model_path,
        "model_exists": exists,
        "model_size_bytes": size,
        "model_loaded": model_loaded(&model_path),
        "language": cfg.language,
        "translate": cfg.translate,
        "model_size": cfg.model_size,
        "default_model_dir": default_model_dir().to_string_lossy(),
        "languages": SUPPORTED_LANGUAGES.iter().map(|(v, l)| serde_json::json!({"value": v, "label": l})).collect::<Vec<_>>(),
        "available_models": AVAILABLE_MODELS.iter().map(|(v, l)| serde_json::json!({"value": v, "label": l})).collect::<Vec<_>>(),
    })
}

#[tauri::command]
pub async fn get_local_stt_status() -> Result<serde_json::Value, String> {
    Ok(status_json(&load_config()))
}

#[tauri::command]
pub async fn set_local_stt_config(config: serde_json::Value) -> Result<serde_json::Value, String> {
    let cfg: SttConfig =
        serde_json::from_value(config).map_err(|e| format!("配置格式错误: {}", e))?;
    save_config(&cfg).map_err(|e| format!("保存配置失败: {}", e))?;
    if cfg.enabled && !cfg.model_path.trim().is_empty() {
        if let Err(e) = ensure_model_loaded(cfg.model_path.trim()) {
            log::warn!("[stt] 模型加载失败: {}", e);
        }
    }
    Ok(status_json(&cfg))
}

/// 下载 Whisper GGML 模型（tiny/base/small），带进度事件 `stt-download-progress`。
/// 优先 huggingface，失败自动回退 hf-mirror（国内镜像）。
#[tauri::command]
pub async fn download_local_stt_model(
    app: tauri::AppHandle,
    size: Option<String>,
) -> Result<serde_json::Value, String> {
    use tauri::Emitter;

    let size = size.unwrap_or_else(|| "base".to_string());
    if !AVAILABLE_MODELS.iter().any(|(v, _)| *v == size) {
        return Err(format!("未知模型尺寸: {}", size));
    }
    let filename = model_file_for_size(&size);
    let dir = default_model_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建模型目录失败: {}", e))?;
    let dest = dir.join(&filename);

    let urls = [
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            filename
        ),
        format!(
            "https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main/{}",
            filename
        ),
    ];

    let client = reqwest::Client::builder()
        .user_agent("ST-Console/1.0")
        .build()
        .map_err(|e| format!("创建下载客户端失败: {}", e))?;

    let mut last_err = String::new();
    for url in urls {
        log::info!("[stt] 下载模型: {}", url);
        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = format!("{}", e);
                continue;
            }
        };
        if !resp.status().is_success() {
            last_err = format!("HTTP {}", resp.status());
            continue;
        }
        let total = resp.content_length().unwrap_or(0);
        let mut stream = resp.bytes_stream();
        let mut file = tokio::fs::File::create(&dest)
            .await
            .map_err(|e| format!("创建文件失败: {}", e))?;
        let mut done: u64 = 0;
        use tokio::io::AsyncWriteExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("下载中断: {}", e))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("写入失败: {}", e))?;
            done += chunk.len() as u64;
            if total > 0 {
                let _ = app.emit(
                    "stt-download-progress",
                    serde_json::json!({
                        "filename": filename,
                        "done": done,
                        "total": total,
                        "percent": ((done as f64 * 100.0 / total as f64).round() as u32).min(100),
                    }),
                );
            }
        }
        file.flush().await.ok();
        drop(file);

        // 下载完成后写入配置并尝试加载
        let mut cfg = load_config();
        cfg.model_path = dest.to_string_lossy().into_owned();
        cfg.model_size = size.clone();
        save_config(&cfg).map_err(|e| format!("保存配置失败: {}", e))?;
        let load_res = ensure_model_loaded(&cfg.model_path);
        let _ = app.emit("stt-download-progress", serde_json::json!({
            "filename": filename, "done": done, "total": total, "percent": 100, "finished": true,
        }));
        return Ok(serde_json::json!({
            "path": dest.to_string_lossy(),
            "size_bytes": done,
            "model_loaded": load_res.is_ok(),
            "load_error": load_res.err(),
            "status": status_json(&cfg),
        }));
    }
    Err(format!("模型下载失败（两个源均不可用）: {}", last_err))
}

use futures_util::StreamExt;

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实本地转写（依赖本机已下载的 Whisper 模型与微信语音缓存 WAV，默认忽略）。
    /// 运行：cargo test --lib live_local_transcribe_cached_wav -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_local_transcribe_cached_wav() {
        let cfg = load_config();
        assert!(cfg.enabled, "本地转写未启用");
        assert!(!cfg.model_path.trim().is_empty(), "未配置本地模型路径");
        assert!(
            Path::new(&cfg.model_path).exists(),
            "模型文件不存在: {}",
            cfg.model_path
        );
        ensure_model_loaded(&cfg.model_path).expect("模型加载失败");

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
        let Some(wav) = wav else {
            panic!("未找到合适的缓存 WAV（100KB~500KB）");
        };
        assert!(!wav.is_empty());
        let text = transcribe_wav(&wav, &cfg.language, cfg.translate).expect("转写应成功");
        assert!(!text.trim().is_empty(), "转写结果不应为空");
        eprintln!("本地转写结果: {}", text);
    }
}
