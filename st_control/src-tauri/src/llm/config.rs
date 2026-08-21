// ============================================================
// 大模型管理 — 配置持久化
// 配置与用量分别落盘到应用数据目录下的两个 JSON 文件，
// 采用「读-改-写」+ 全局写锁的方式，避免并发竞争。
// ============================================================

use crate::llm::types::{LlmConfig, LlmUsage};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

/// 序列化写操作的全局锁，避免并发读改写导致数据丢失
static WRITE_LOCK: Mutex<()> = Mutex::new(());
/// 最近一次配置解析失败的信息：一旦置位，写操作会被拒绝，防止空配置覆盖原文件
static CONFIG_LOAD_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn load_error_slot() -> &'static Mutex<Option<String>> {
    CONFIG_LOAD_ERROR.get_or_init(|| Mutex::new(None))
}

fn set_load_error(msg: String) {
    if let Ok(mut g) = load_error_slot().lock() {
        *g = Some(msg);
    }
}

fn take_load_error() -> Option<String> {
    load_error_slot().lock().ok().and_then(|mut g| g.take())
}

/// 当前是否有配置解析错误（供前端展示诊断信息）
pub fn load_error() -> Option<String> {
    load_error_slot().lock().ok().and_then(|g| g.clone())
}

fn config_dir() -> std::path::PathBuf {
    crate::common::st_data_dir()
}

fn config_path() -> std::path::PathBuf {
    let mut p = config_dir();
    p.push("llm_config.json");
    p
}

fn usage_path() -> std::path::PathBuf {
    let mut p = config_dir();
    p.push("llm_usage.json");
    p
}

/// 读取全局大模型配置；文件不存在时返回空配置。
///
/// 容灾：主文件为空（进程被强杀截断）或解析失败时，
/// 自动从 `llm_config.json.bak` 恢复并写回主文件，避免
/// 「读-改-写」把空配置覆盖原文件导致提供方配置丢失。
pub fn load_config() -> LlmConfig {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(s) if !s.trim().is_empty() => match serde_json::from_str(&s) {
            Ok(cfg) => {
                let _ = take_load_error();
                cfg
            }
            Err(e) => {
                let reason = format!("大模型配置解析失败（{}）：{}", path.display(), e);
                if let Some(restored) = try_restore_from_backup(&reason) {
                    return restored;
                }
                set_load_error(reason);
                LlmConfig::default()
            }
        },
        Ok(_) => {
            // 文件存在但内容为空：多半是被异常中断截断（如进程强杀）
            let reason = format!(
                "大模型配置文件为空（{}），疑似被异常中断截断",
                path.display()
            );
            if let Some(restored) = try_restore_from_backup(&reason) {
                return restored;
            }
            set_load_error(reason);
            LlmConfig::default()
        }
        Err(e) => {
            if path.exists() {
                set_load_error(format!("读取大模型配置失败（{}）: {}", path.display(), e));
            }
            LlmConfig::default()
        }
    }
}

/// 主配置损坏时尝试从 .bak 恢复：解析成功则写回主文件（原子写）并返回
fn try_restore_from_backup(reason: &str) -> Option<LlmConfig> {
    let bak = config_path().with_extension("json.bak");
    let s = std::fs::read_to_string(&bak).ok()?;
    let cfg: LlmConfig = match serde_json::from_str(&s) {
        Ok(c) => c,
        Err(_) => return None,
    };
    log::warn!(
        "[llm] {}；已从备份自动恢复（{} 个提供方）",
        reason,
        cfg.providers.len()
    );
    // 立即写回主文件，自愈（原子替换，失败仅记日志不阻断使用）
    if let Err(e) = write_config_atomic(&cfg, true) {
        log::warn!("[llm] 恢复配置写回失败: {}", e);
    }
    let _ = take_load_error();
    Some(cfg)
}

/// 持久化全局配置
pub fn save_config(cfg: &LlmConfig) -> Result<(), String> {
    let _guard = WRITE_LOCK.lock().unwrap();
    // 配置解析失败时拒绝保存，避免“读-改-写”把空配置覆盖回原文件
    if let Some(err) = take_load_error() {
        return Err(format!(
            "{}；已中止保存，原配置文件未改动。可手工修复该文件或删除后重新配置。",
            err
        ));
    }
    write_config_atomic(cfg, true)
}

/// 原子写配置：先写 .tmp 再整体替换，进程在任何时刻被强杀都不会留下
/// 截断/空的主文件（旧内容与新内容二选一，绝不出现半截 JSON）。
fn write_config_atomic(cfg: &LlmConfig, backup_first: bool) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败: {}", e))?;
    let path = config_path();
    // 覆盖前备份上一份配置，便于误操作后恢复
    if backup_first {
        if let Ok(existing) = std::fs::read(&path) {
            if !existing.is_empty() {
                let _ = std::fs::write(path.with_extension("json.bak"), existing);
            }
        }
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入配置临时文件失败: {}", e))?;
    // Windows 下 rename 可替换已存在目标（MoveFileEx + REPLACE_EXISTING）
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换配置文件失败: {}", e))?;
    Ok(())
}

/// 读取用量统计；文件不存在时返回空
pub fn load_usage() -> LlmUsage {
    let path = usage_path();
    match std::fs::read_to_string(&path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or_default(),
        _ => LlmUsage::default(),
    }
}

/// 持久化用量统计
pub fn save_usage(usage: &LlmUsage) -> Result<(), String> {
    let _guard = WRITE_LOCK.lock().unwrap();
    let dir = config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败: {}", e))?;
    let json = serde_json::to_string_pretty(usage).map_err(|e| format!("序列化失败: {}", e))?;
    let mut f =
        std::fs::File::create(usage_path()).map_err(|e| format!("创建用量文件失败: {}", e))?;
    f.write_all(json.as_bytes())
        .map_err(|e| format!("写入用量失败: {}", e))?;
    Ok(())
}

/// 配置文件的磁盘路径（供前端展示）
pub fn config_path_string() -> String {
    config_path().to_string_lossy().to_string()
}

/// 当前月份 key，格式 "YYYY-MM"
pub fn current_month() -> String {
    chrono::Utc::now().format("%Y-%m").to_string()
}

/// 获取当前月份某提供方的用量（不存在则 0）
pub fn current_month_usage(provider_id: &str) -> crate::llm::types::ProviderUsage {
    let usage = load_usage();
    let month = current_month();
    usage
        .months
        .get(&month)
        .and_then(|m| m.get(provider_id))
        .cloned()
        .unwrap_or_default()
}

/// 累加某提供方当月用量（线程安全）
pub fn add_usage(
    provider_id: &str,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    cost: f64,
) -> Result<(), String> {
    let mut usage = load_usage();
    let month = current_month();
    let entry = usage
        .months
        .entry(month)
        .or_default()
        .entry(provider_id.to_string())
        .or_default();
    entry.prompt_tokens += prompt_tokens;
    entry.completion_tokens += completion_tokens;
    entry.total_tokens += total_tokens;
    entry.cost += cost;
    entry.call_count += 1;
    save_usage(&usage)
}

/// 清空全部用量统计
pub fn reset_usage() -> Result<(), String> {
    save_usage(&LlmUsage::default())
}

/// 记录最后一次全局调用的 提供方/模型（落盘到 llm_config.json，重启后恢复会话）
pub fn set_last_chat(provider_id: &str, model: &str) -> Result<(), String> {
    let mut cfg = load_config();
    cfg.last_chat_provider_id = Some(provider_id.to_string());
    cfg.last_chat_model = Some(model.to_string());
    save_config(&cfg)
}

/// 记录最后一次文本嵌入调用的 提供方/模型（与聊天记忆分离）
pub fn set_last_embedding(provider_id: &str, model: &str) -> Result<(), String> {
    let mut cfg = load_config();
    cfg.last_embedding_provider_id = Some(provider_id.to_string());
    cfg.last_embedding_model = Some(model.to_string());
    save_config(&cfg)
}

/// 在配置中按 id 查找提供方
pub fn find_provider<'a>(
    cfg: &'a LlmConfig,
    id: &str,
) -> Option<&'a crate::llm::types::ProviderConfig> {
    cfg.providers.iter().find(|p| p.id == id)
}

/// 生成 ISO 时间戳
pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// 生成 uuid
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(id: &str) -> crate::llm::types::ProviderConfig {
        crate::llm::types::ProviderConfig {
            id: id.to_string(),
            name: id.to_string(),
            base_url: "http://localhost".into(),
            api_key: "k".into(),
            default_model: "m".into(),
            models: vec![],
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn find_provider_by_id() {
        let cfg = LlmConfig {
            providers: vec![provider("a"), provider("b")],
            ..Default::default()
        };
        assert_eq!(find_provider(&cfg, "a").map(|p| p.id.as_str()), Some("a"));
        assert_eq!(find_provider(&cfg, "b").map(|p| p.id.as_str()), Some("b"));
        // 未命中
        assert!(find_provider(&cfg, "nope").is_none());
        // 空配置
        let empty = LlmConfig::default();
        assert!(find_provider(&empty, "a").is_none());
    }
}
