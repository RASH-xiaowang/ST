//! Windows SAPI 离线语音合成（System.Speech → WAV，16kHz 单声道 16bit）。
//! 零配置、无需联网，作为语音回复的兜底引擎；文本经临时文件传递，
//! 不进入命令行，避免引号/注入问题。

use base64::Engine;
use std::process::Stdio;

pub async fn synthesize_wav(text: &str, rate: i32) -> Result<Vec<u8>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("语音合成输入文本为空".to_string());
    }

    let id = uuid::Uuid::new_v4().to_string();
    let txt_path = std::env::temp_dir().join(format!("st_tts_text_{}.txt", id));
    let wav_path = std::env::temp_dir().join(format!("st_tts_{}.wav", id));
    let txt_s = txt_path.to_string_lossy().into_owned();
    let wav_s = wav_path.to_string_lossy().into_owned();

    std::fs::write(&txt_path, text.as_bytes()).map_err(|e| format!("写入临时文本失败: {}", e))?;

    // 脚本保持纯 ASCII；用户文本从 UTF-8 临时文件读取。
    // Rate 为 SAPI 语速（-10~10，负值更慢）：默认 -2 更接近自然语速，
    // 同时按标点注入更自然的停顿（SAPI 对中文句读本身已有停顿处理）。
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer
$s.Rate = {rate}
$v = $s.GetInstalledVoices() | Where-Object {{ $_.VoiceInfo.Culture.Name -eq 'zh-CN' }} | Select-Object -First 1
if ($v) {{ $s.SelectVoice($v.VoiceInfo.Name) }}
$fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
$s.SetOutputToWaveFile('{wav}', $fmt)
$text = [System.IO.File]::ReadAllText('{txt}', [System.Text.Encoding]::UTF8)
$s.Speak($text)
$s.Dispose()
"#,
        rate = rate.clamp(-10, 10),
        wav = ps_quote(&wav_s),
        txt = ps_quote(&txt_s),
    );

    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16_le_bytes(&script));
    let out = tokio::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encoded,
        ])
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| format!("启动 PowerShell 失败: {}", e))?;

    let _ = std::fs::remove_file(&txt_path);
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let _ = std::fs::remove_file(&wav_path);
        return Err(if err.is_empty() {
            format!("系统语音合成失败（退出码 {}）", out.status)
        } else {
            format!("系统语音合成失败: {}", err)
        });
    }

    let bytes = std::fs::read(&wav_path).map_err(|e| format!("读取合成 WAV 失败: {}", e))?;
    let _ = std::fs::remove_file(&wav_path);
    if bytes.len() <= 44 {
        return Err("系统语音合成结果为空".to_string());
    }
    Ok(bytes)
}

/// PowerShell 单引号字符串的内部转义（单引号翻倍；外层引号由模板提供）
fn ps_quote(s: &str) -> String {
    s.replace('\'', "''")
}

/// UTF-16 LE 编码（PowerShell -EncodedCommand 要求）
fn utf16_le_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 2);
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}
