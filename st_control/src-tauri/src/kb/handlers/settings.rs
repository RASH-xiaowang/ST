// ============================================================
// 知识库管理 — 模型与处理设置
// 自 handlers.rs 拆分：模型列表/默认模型解析、模型设置、分块设置。
// ============================================================

use crate::kb::db::KbDatabase;
use serde::{Deserialize, Serialize};
use tauri::State;

// ─── 模型列表（供前端选择嵌入 / 生成模型） ───

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct KbModelInfo {
    pub providerId: String,
    pub providerName: String,
    pub model: String,
    pub isDefault: bool,
    /// 大模型管理里标记的模型类型（对话/生图/视频/语音/嵌入/重排序 等）
    pub modelType: Option<String>,
}

/// 列出所有可用提供方下的模型，标记每个提供方的默认模型
#[tauri::command]
pub async fn kb_list_models() -> Result<Vec<KbModelInfo>, String> {
    let cfg = crate::llm::config::load_config();
    let mut out = Vec::new();
    for p in &cfg.providers {
        if !p.enabled {
            continue;
        }
        for m in &p.models {
            out.push(KbModelInfo {
                providerId: p.id.clone(),
                providerName: p.name.clone(),
                model: m.clone(),
                isDefault: m == &p.default_model,
                modelType: p.model_meta.get(m).and_then(|meta| meta.model_type.clone()),
            });
        }
    }
    if out.is_empty() {
        return Err("未配置任何模型提供方，请先在「大模型管理」中添加".to_string());
    }
    Ok(out)
}

/// 判断模型是否被标记为「嵌入」类型（兼容中文「嵌入」与英文 embedding）
fn is_embedding_model(provider: &crate::llm::types::ProviderConfig, model: &str) -> bool {
    provider
        .model_meta
        .get(model)
        .and_then(|meta| meta.model_type.as_deref())
        .map(|t| t == "嵌入" || t.eq_ignore_ascii_case("embedding"))
        .unwrap_or(false)
}

/// 判断模型是否被标记为「对话」类型（兼容中文「对话」与英文 chat）
fn is_chat_model(provider: &crate::llm::types::ProviderConfig, model: &str) -> bool {
    provider
        .model_meta
        .get(model)
        .and_then(|meta| meta.model_type.as_deref())
        .map(|t| t == "对话" || t.eq_ignore_ascii_case("chat"))
        .unwrap_or(false)
}

/// 判断模型是否被标记为「重排序」类型（兼容中文「重排序」与英文 rerank）
fn is_rerank_model(provider: &crate::llm::types::ProviderConfig, model: &str) -> bool {
    provider
        .model_meta
        .get(model)
        .and_then(|meta| meta.model_type.as_deref())
        .map(|t| t == "重排序" || t.eq_ignore_ascii_case("rerank"))
        .unwrap_or(false)
}

fn default_embedding_model_from_cfg(
    cfg: &crate::llm::types::LlmConfig,
) -> Option<(String, String)> {
    // 默认提供方中的嵌入模型 → 任意启用提供方中的嵌入模型
    if let Some(dpid) = &cfg.default_provider_id {
        if let Some(p) = cfg.providers.iter().find(|p| p.id == *dpid && p.enabled) {
            if let Some(m) = p.models.iter().find(|m| is_embedding_model(p, m)) {
                return Some((p.id.clone(), m.clone()));
            }
        }
    }
    for p in &cfg.providers {
        if p.enabled {
            if let Some(m) = p.models.iter().find(|m| is_embedding_model(p, m)) {
                return Some((p.id.clone(), m.clone()));
            }
        }
    }
    // 上次嵌入调用使用的模型
    if let (Some(pid), Some(model)) = (&cfg.last_embedding_provider_id, &cfg.last_embedding_model) {
        if !pid.is_empty() && !model.is_empty() {
            return Some((pid.clone(), model.clone()));
        }
    }
    // 兜底：首个启用提供方的默认模型
    for p in &cfg.providers {
        if p.enabled && !p.default_model.is_empty() {
            return Some((p.id.clone(), p.default_model.clone()));
        }
    }
    None
}

fn default_chat_model_from_cfg(cfg: &crate::llm::types::LlmConfig) -> Option<(String, String)> {
    if let Some(dpid) = &cfg.default_provider_id {
        if let Some(p) = cfg.providers.iter().find(|p| p.id == *dpid && p.enabled) {
            if let Some(m) = p.models.iter().find(|m| is_chat_model(p, m)) {
                return Some((p.id.clone(), m.clone()));
            }
        }
    }
    for p in &cfg.providers {
        if p.enabled {
            if let Some(m) = p.models.iter().find(|m| is_chat_model(p, m)) {
                return Some((p.id.clone(), m.clone()));
            }
        }
    }
    if let (Some(pid), Some(model)) = (&cfg.last_chat_provider_id, &cfg.last_chat_model) {
        if !pid.is_empty() && !model.is_empty() {
            return Some((pid.clone(), model.clone()));
        }
    }
    for p in &cfg.providers {
        if p.enabled && !p.default_model.is_empty() {
            return Some((p.id.clone(), p.default_model.clone()));
        }
    }
    None
}

fn default_rerank_model_from_cfg(cfg: &crate::llm::types::LlmConfig) -> Option<(String, String)> {
    for p in &cfg.providers {
        if p.enabled {
            if let Some(m) = p.models.iter().find(|m| is_rerank_model(p, m)) {
                return Some((p.id.clone(), m.clone()));
            }
        }
    }
    None
}

/// 知识库模型设置（推理/解析/嵌入/重排序）
#[derive(Serialize, Deserialize, Clone)]
#[allow(non_snake_case)]
pub struct ModelSetting {
    pub providerId: String,
    pub model: String,
}

const MODEL_ROLES: [&str; 4] = ["inference", "parsing", "embedding", "rerank"];

pub(crate) fn read_model_setting(
    conn: &rusqlite::Connection,
    role: &str,
) -> Option<(String, String)> {
    conn.query_row(
        "SELECT provider_id, model FROM kb_model_settings WHERE role = ?1",
        rusqlite::params![role],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )
    .ok()
    .filter(|(p, m)| !p.is_empty() && !m.is_empty())
}

/// 嵌入模型解析：优先使用调用方显式传入的（前端选择），否则用「模型设置」中的 embedding 配置
pub(crate) fn resolve_embedding_pair(
    db: &KbDatabase,
    passed_provider: Option<String>,
    passed_model: Option<String>,
) -> (Option<String>, Option<String>) {
    if passed_provider.is_some() || passed_model.is_some() {
        (passed_provider, passed_model)
    } else {
        let conn = db.conn_lock();
        read_model_setting(&conn, "embedding")
            .map(|(p, m)| (Some(p), Some(m)))
            .unwrap_or((None, None))
    }
}

/// 读取四类模型设置；未手动配置时返回按类型推导的默认值
#[tauri::command]
pub async fn kb_get_model_settings(db: State<'_, KbDatabase>) -> Result<serde_json::Value, String> {
    let cfg = crate::llm::config::load_config();
    let conn = db.conn_lock();
    let get = |role: &str| -> Option<ModelSetting> {
        read_model_setting(&conn, role).map(|(p, m)| ModelSetting {
            providerId: p,
            model: m,
        })
    };
    let to_setting = |pair: Option<(String, String)>| -> Option<ModelSetting> {
        pair.map(|(p, m)| ModelSetting {
            providerId: p,
            model: m,
        })
    };
    Ok(serde_json::json!({
        "inference": get("inference").or_else(|| to_setting(default_chat_model_from_cfg(&cfg))),
        "parsing": get("parsing").or_else(|| to_setting(default_chat_model_from_cfg(&cfg))),
        "embedding": get("embedding").or_else(|| to_setting(default_embedding_model_from_cfg(&cfg))),
        "rerank": get("rerank").or_else(|| to_setting(default_rerank_model_from_cfg(&cfg))),
    }))
}

/// 保存某类模型设置（inference / parsing / embedding / rerank）
#[tauri::command]
pub async fn kb_set_model_settings(
    db: State<'_, KbDatabase>,
    role: String,
    provider_id: String,
    model: String,
) -> Result<(), String> {
    if !MODEL_ROLES.contains(&role.as_str()) {
        return Err(format!("未知的模型角色: {}", role));
    }
    let provider_id = provider_id.trim().to_string();
    let model = model.trim().to_string();
    if provider_id.is_empty() || model.is_empty() {
        return Err("请选择提供方与模型".to_string());
    }
    let conn = db.conn_lock();
    conn.execute(
        "INSERT INTO kb_model_settings (role, provider_id, model, updated_at)
         VALUES (?1,?2,?3,datetime('now'))
         ON CONFLICT(role) DO UPDATE SET provider_id=excluded.provider_id, model=excluded.model, updated_at=datetime('now')",
        rusqlite::params![role, provider_id, model],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 读取全局分块设置（strategy / size / overlap），未配置时返回默认值
#[tauri::command]
pub async fn kb_get_chunk_settings(db: State<'_, KbDatabase>) -> Result<serde_json::Value, String> {
    let conn = db.conn_lock();
    let mut stmt = conn
        .prepare("SELECT key, value FROM kb_chunk_settings")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (k, v) = row.map_err(|e| e.to_string())?;
        map.insert(k, v);
    }
    let parse_i64 =
        |key: &str, dft: i64| -> i64 { map.get(key).and_then(|v| v.parse().ok()).unwrap_or(dft) };
    let strategy = map
        .get("strategy")
        .cloned()
        .unwrap_or_else(|| "recursive".to_string());
    Ok(serde_json::json!({
        "strategy": strategy,
        "size": parse_i64("size", 800),
        "overlap": parse_i64("overlap", 128),
    }))
}

/// 保存全局分块设置（上传 / 重处理 / 新版本共用）
#[tauri::command]
pub async fn kb_set_chunk_settings(
    db: State<'_, KbDatabase>,
    strategy: String,
    size: i64,
    overlap: i64,
) -> Result<(), String> {
    if !["recursive", "title", "parent_child"].contains(&strategy.as_str()) {
        return Err("未知的分块策略".to_string());
    }
    if !(100..=8000).contains(&size) {
        return Err("分块大小应在 100 ~ 8000 之间".to_string());
    }
    if overlap < 0 || overlap >= size {
        return Err("重叠值应在 0 ~ 分块大小-1 之间".to_string());
    }
    let conn = db.conn_lock();
    let items = [
        ("strategy", strategy),
        ("size", size.to_string()),
        ("overlap", overlap.to_string()),
    ];
    for (key, value) in items {
        conn.execute(
            "INSERT INTO kb_chunk_settings (key, value, updated_at) VALUES (?1,?2,datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=datetime('now')",
            rusqlite::params![key, value],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 返回知识库应使用的默认嵌入（提供方, 模型）：优先已保存的 embedding 设置
#[tauri::command]
pub async fn kb_get_default_model(db: State<'_, KbDatabase>) -> Result<(String, String), String> {
    if let Some((p, m)) = {
        let conn = db.conn_lock();
        read_model_setting(&conn, "embedding")
    } {
        return Ok((p, m));
    }
    let cfg = crate::llm::config::load_config();
    default_embedding_model_from_cfg(&cfg).ok_or_else(|| "未找到可用的默认模型".to_string())
}

/// 返回问答/RAG 应使用的默认生成（提供方, 模型）：优先已保存的 inference 设置
#[tauri::command]
pub async fn kb_get_default_chat_model(
    db: State<'_, KbDatabase>,
) -> Result<(String, String), String> {
    if let Some((p, m)) = {
        let conn = db.conn_lock();
        read_model_setting(&conn, "inference")
    } {
        return Ok((p, m));
    }
    let cfg = crate::llm::config::load_config();
    default_chat_model_from_cfg(&cfg).ok_or_else(|| "未找到可用的对话模型".to_string())
}
