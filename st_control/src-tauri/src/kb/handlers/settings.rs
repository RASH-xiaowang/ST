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

/// 判断模型是否被明确标记为「非嵌入」类型（model_meta 存在且类型不是嵌入）。
/// 用于拦截把对话/其他类型模型当作向量化模型调用（如 DeepSeek 对话模型无嵌入接口，
/// 误用会导致全部文档上传后向量化 404 失败）。model_meta 缺失时无法判定，放行。
fn is_definitely_not_embedding_in(
    cfg: &crate::llm::types::LlmConfig,
    provider: &str,
    model: &str,
) -> bool {
    cfg.providers
        .iter()
        .find(|p| p.id == provider)
        .and_then(|p| p.model_meta.get(model))
        .and_then(|meta| meta.model_type.as_deref())
        .map(|t| !(t == "嵌入" || t.eq_ignore_ascii_case("embedding")))
        .unwrap_or(false)
}

fn is_definitely_not_embedding(provider: &str, model: &str) -> bool {
    let cfg = crate::llm::config::load_config();
    is_definitely_not_embedding_in(&cfg, provider, model)
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
    // 上次嵌入调用使用的模型（仅当仍是嵌入模型时沿用，避免误用对话模型）
    if let (Some(pid), Some(model)) = (&cfg.last_embedding_provider_id, &cfg.last_embedding_model) {
        if !pid.is_empty() && !model.is_empty() && !is_definitely_not_embedding(pid, model) {
            return Some((pid.clone(), model.clone()));
        }
    }
    // 注意：绝不回退到「默认/对话模型」当嵌入模型——向量化接口不支持对话模型，
    // 误用会导致全部文档上传后向量化失败。未配置嵌入模型时返回 None，由调用方跳过向量化。
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

/// 嵌入模型解析：优先使用调用方显式传入的（前端选择），否则用「模型设置」中的 embedding 配置。
/// 显式传入或已保存的模型若被明确标记为非嵌入类型（如对话模型），一律忽略并回退，
/// 避免把不支持的模型当作向量化模型调用导致上传全部失败。
pub(crate) fn resolve_embedding_pair(
    db: &KbDatabase,
    passed_provider: Option<String>,
    passed_model: Option<String>,
) -> (Option<String>, Option<String>) {
    if let (Some(p), Some(m)) = (&passed_provider, &passed_model) {
        if !is_definitely_not_embedding(p, m) {
            return (passed_provider, passed_model);
        }
        log::warn!(
            "忽略被标记为非嵌入类型的向量化模型: provider={} model={}",
            p,
            m
        );
    }
    let conn = db.conn_lock();
    read_model_setting(&conn, "embedding")
        .filter(|(p, m)| !is_definitely_not_embedding(p, m))
        .map(|(p, m)| (Some(p), Some(m)))
        .unwrap_or((None, None))
}

/// 推理模型解析：优先显式传入 → kb_model_settings(inference) → 全局默认对话模型。
/// Wiki 提炼 / 摘要实体提取等 LLM 流程共用，避免「未在设置页保存推理模型时提炼必失败」。
pub(crate) fn resolve_inference_pair(
    db: &KbDatabase,
    passed_provider: Option<String>,
    passed_model: Option<String>,
) -> (Option<String>, Option<String>) {
    if let (Some(p), Some(m)) = (&passed_provider, &passed_model) {
        if !p.is_empty() && !m.is_empty() {
            return (passed_provider, passed_model);
        }
    }
    if let Some((p, m)) = {
        let conn = db.conn_lock();
        read_model_setting(&conn, "inference")
    } {
        return (Some(p), Some(m));
    }
    let cfg = crate::llm::config::load_config();
    default_chat_model_from_cfg(&cfg)
        .map(|(p, m)| (Some(p), Some(m)))
        .unwrap_or((None, None))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::{LlmConfig, ModelMeta, ProviderConfig};

    fn provider(
        id: &str,
        models: &[&str],
        default_model: &str,
        types: &[(&str, &str)],
    ) -> ProviderConfig {
        let mut p = ProviderConfig::default();
        p.id = id.to_string();
        p.name = id.to_string();
        p.default_model = default_model.to_string();
        p.enabled = true;
        p.models = models.iter().map(|m| m.to_string()).collect();
        for (m, t) in types {
            p.model_meta.insert(
                m.to_string(),
                ModelMeta {
                    model_type: Some(t.to_string()),
                    ..Default::default()
                },
            );
        }
        p
    }

    #[test]
    fn rejects_chat_model_as_embedding() {
        let cfg = LlmConfig {
            providers: vec![provider(
                "deepseek",
                &["deepseek-v4-flash"],
                "deepseek-v4-flash",
                &[("deepseek-v4-flash", "对话")],
            )],
            ..Default::default()
        };
        // 被明确标记为对话的模型不能当作嵌入模型
        assert!(is_definitely_not_embedding_in(
            &cfg,
            "deepseek",
            "deepseek-v4-flash"
        ));
        // model_meta 缺失（未打标）时无法判定，放行
        assert!(!is_definitely_not_embedding_in(
            &cfg,
            "deepseek",
            "unknown-model"
        ));
        // 未配置嵌入模型时，默认嵌入不得回退到对话模型
        assert_eq!(default_embedding_model_from_cfg(&cfg), None);
    }

    #[test]
    fn accepts_embedding_model() {
        let cfg = LlmConfig {
            providers: vec![provider("sf", &["bge-m3"], "bge-m3", &[("bge-m3", "嵌入")])],
            ..Default::default()
        };
        assert!(!is_definitely_not_embedding_in(&cfg, "sf", "bge-m3"));
        assert_eq!(
            default_embedding_model_from_cfg(&cfg),
            Some(("sf".to_string(), "bge-m3".to_string()))
        );
    }

    #[test]
    fn default_embedding_never_falls_back_to_chat_default() {
        // 只有对话模型的提供方：即便设置了默认模型，也不能用作嵌入
        let cfg = LlmConfig {
            default_provider_id: Some("deepseek".to_string()),
            providers: vec![provider(
                "deepseek",
                &["deepseek-v4-flash", "deepseek-v4-pro"],
                "deepseek-v4-flash",
                &[("deepseek-v4-flash", "对话"), ("deepseek-v4-pro", "对话")],
            )],
            ..Default::default()
        };
        assert_eq!(default_embedding_model_from_cfg(&cfg), None);
    }
}
