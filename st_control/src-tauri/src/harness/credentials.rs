// ============================================================
// Harness — 凭据引用能力（DSH credentials 迁移）
//
// 凭据引用：键值凭据存储（data/harness/credentials.json，AES-256-CBC
// 加密落盘，密钥复用 data/bot_secret.key；旧明文文件自动迁移加密；
// 解密失败响亮拒绝覆盖）+ .env 文件提供者（data/harness/.env 明文，
// 与 DSH 一致，供 hooks/MCP/终端等子进程使用）。
// 消费：子进程启动时注入 HARNESS_CREDENTIAL_<KEY> 环境变量；
// 模型不可直接读值（避免泄露），仅在提示中可引用键名。
// ============================================================

use crate::bot::secret::TokenCipher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CredentialStore {
    #[serde(default)]
    pub credentials: HashMap<String, String>,
}

/// 凭据视图（值掩码，不泄露明文）
#[derive(Serialize, Clone, Debug)]
pub struct CredentialView {
    pub key: String,
    pub masked: String,
}

fn credentials_path() -> std::path::PathBuf {
    crate::common::st_data_dir()
        .join("harness")
        .join("credentials.json")
}

fn env_path() -> std::path::PathBuf {
    crate::common::st_data_dir().join("harness").join(".env")
}

/// 最近一次加载/解密失败信息：一旦置位，持久化被拒绝（防止把空存储
/// 或解密失败后的占位写回原文件，造成凭据数据丢失）。
static LOAD_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn load_error_slot() -> &'static Mutex<Option<String>> {
    LOAD_ERROR.get_or_init(|| Mutex::new(None))
}

fn set_load_error(msg: String) {
    if let Ok(mut g) = load_error_slot().lock() {
        *g = Some(msg);
    }
}

fn take_load_error() -> Option<String> {
    load_error_slot().lock().ok().and_then(|mut g| g.take())
}

/// 加密整份凭据 JSON（AES-256-CBC，密钥 = data/bot_secret.key）
fn encrypt_blob(plain: &str) -> Result<String, String> {
    TokenCipher::load(&crate::common::st_data_dir()).and_then(|c| c.encrypt(plain))
}

/// 解密凭据 JSON；兼容旧明文（以 `{` 开头 = 迁移期旧格式，直接透传，
/// 下次 persist 即加密落盘）
fn decrypt_blob(enc: &str) -> Result<String, String> {
    let t = enc.trim();
    if t.starts_with('{') {
        return Ok(t.to_string());
    }
    TokenCipher::load(&crate::common::st_data_dir()).and_then(|c| c.decrypt(t))
}

fn store() -> &'static Mutex<CredentialStore> {
    static S: OnceLock<Mutex<CredentialStore>> = OnceLock::new();
    S.get_or_init(|| {
        let loaded = match std::fs::read_to_string(credentials_path()) {
            Ok(text) if !text.trim().is_empty() => {
                let legacy = text.trim_start().starts_with('{');
                match decrypt_blob(&text).and_then(|json| {
                    serde_json::from_str(&json).map_err(|e| format!("解析失败: {e}"))
                }) {
                    Ok(s) => {
                        // 旧明文文件：立即加密重写，避免明文残留
                        if legacy {
                            if let Err(e) = persist_store(&s) {
                                log::warn!("[harness] 凭据明文迁移加密失败: {e}");
                            }
                        }
                        s
                    }
                    Err(e) => {
                        let reason = format!(
                            "凭据文件读取/解密失败（{}）: {e}",
                            credentials_path().display()
                        );
                        set_load_error(reason.clone());
                        log::error!("[harness] {reason}；以空存储加载并拒绝覆盖原文件");
                        CredentialStore::default()
                    }
                }
            }
            _ => CredentialStore::default(),
        };
        Mutex::new(loaded)
    })
}

/// 落盘（含加密）：仅当无加载/解密错误时执行
fn persist(s: &CredentialStore) -> Result<(), String> {
    if let Some(err) = take_load_error() {
        return Err(format!("{err}；已中止保存，原凭据文件未改动"));
    }
    persist_store(s)
}

/// 序列化 + AES-256-CBC 加密 + 原子替换
fn persist_store(s: &CredentialStore) -> Result<(), String> {
    let path = credentials_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建凭据目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(s).map_err(|e| format!("序列化失败: {}", e))?;
    let enc = encrypt_blob(&json).map_err(|e| format!("凭据加密失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, enc.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换失败: {}", e))?;
    Ok(())
}

/// 掩码：保留首 2 尾 2 字符，中间打码；过短全掩
fn mask(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 4 {
        return "*".repeat(chars.len().max(1));
    }
    format!(
        "{}{}{}",
        chars[..2].iter().collect::<String>(),
        "*".repeat(chars.len() - 4),
        chars[chars.len() - 2..].iter().collect::<String>()
    )
}

/// .env 提供者：加载 data/harness/.env 为键值
pub fn env_values() -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(env_path()) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                out.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    out
}

/// 全部凭据（含 .env 提供者）——供子进程环境注入
pub fn all_values() -> HashMap<String, String> {
    let mut out = env_values();
    for (k, v) in store().lock().unwrap().credentials.iter() {
        out.insert(k.clone(), v.clone());
    }
    out
}

/// 写入 .env 条目（提供者）
pub fn put_env(key: &str, value: &str) -> Result<(), String> {
    let mut values = env_values();
    values.insert(key.to_string(), value.to_string());
    let mut lines: Vec<String> = values.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
    lines.sort();
    let path = env_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(&path, lines.join("\n") + "\n").map_err(|e| format!("写入 .env 失败: {}", e))?;
    Ok(())
}

// ─── IPC ───

#[tauri::command]
pub async fn harness_credential_list() -> Result<Vec<CredentialView>, String> {
    let mut out: Vec<CredentialView> = Vec::new();
    for (k, v) in all_values() {
        out.push(CredentialView {
            key: k,
            masked: mask(&v),
        });
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(out)
}

#[tauri::command]
pub async fn harness_credential_put(
    key: String,
    value: String,
    store_in_env: Option<bool>,
) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("凭据键名不能为空".to_string());
    }
    if value.is_empty() {
        return Err("凭据值不能为空".to_string());
    }
    let key = key.trim().to_string();
    if store_in_env.unwrap_or(false) {
        put_env(&key, &value)?;
    } else {
        let mut s = store().lock().unwrap();
        s.credentials.insert(key, value);
        persist(&s)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn harness_credential_delete(key: String) -> Result<(), String> {
    // 同时尝试从两处删除
    let mut s = store().lock().unwrap();
    let removed = s.credentials.remove(&key).is_some();
    if removed {
        persist(&s)?;
        return Ok(());
    }
    let mut values = env_values();
    if values.remove(&key).is_some() {
        let mut lines: Vec<String> = values.iter().map(|(k, v)| format!("{k}={v}")).collect();
        lines.sort();
        std::fs::write(env_path(), lines.join("\n") + "\n")
            .map_err(|e| format!("写入 .env 失败: {}", e))?;
        return Ok(());
    }
    Err("指定的凭据不存在".to_string())
}

/// 把全部凭据注入子进程命令（供 hooks/mcp/terminal 消费）
pub fn inject_env(cmd: &mut std::process::Command) {
    for (k, v) in all_values() {
        cmd.env(format!("HARNESS_CREDENTIAL_{}", k), v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_hides_middle() {
        assert_eq!(mask("abcdefgh"), "ab****gh");
        assert_eq!(mask("abc"), "***");
        assert_eq!(mask(""), "*");
    }

    #[test]
    fn env_provider_roundtrip() {
        let key = format!("test-key-{}", uuid::Uuid::new_v4().simple());
        put_env(&key, "secret-value").unwrap();
        let values = env_values();
        assert_eq!(values.get(&key).map(|s| s.as_str()), Some("secret-value"));
        // 清理
        let mut values = env_values();
        values.remove(&key);
        let mut lines: Vec<String> = values.iter().map(|(k, v)| format!("{k}={v}")).collect();
        lines.sort();
        let _ = std::fs::write(env_path(), lines.join("\n") + "\n");
    }

    #[test]
    fn encrypt_decrypt_blob_roundtrip() {
        // 复用 data/bot_secret.key（与 bot token 同密钥）；密文不含明文
        let plain = r#"{"credentials":{"AKIA_TEST":"very-secret-value"}}"#;
        let enc = encrypt_blob(plain).unwrap();
        assert!(!enc.starts_with('{'), "加密结果不应是明文 JSON");
        assert!(!enc.contains("very-secret-value"), "密文不应包含明文");
        assert_eq!(decrypt_blob(&enc).unwrap(), plain);
    }

    #[test]
    fn decrypt_blob_accepts_legacy_plaintext() {
        // 迁移期旧明文直接透传（下次 persist 即加密）
        let plain = r#"{"credentials":{"K":"V"}}"#;
        assert_eq!(decrypt_blob(plain).unwrap(), plain);
    }
}
