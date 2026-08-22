// ============================================================
// 大模型管理 — IPC 命令：接入配置 / 提供方 / 模型管理
// 自 handlers.rs 拆分：配置读取、提供方 CRUD、连接测试、
// 模型列表与元数据管理、提供方类型枚举。
// ============================================================

use crate::llm::client;
use crate::llm::config;
use crate::llm::types::{LlmConfig, ModelMeta, ProviderConfig, ProviderType, TestResult};

use super::notify_llm_config_changed;

// ─── 配置读取 ───

#[tauri::command]
pub async fn get_llm_config() -> Result<LlmConfig, String> {
    let cfg = config::load_config();
    if let Some(err) = config::load_error() {
        return Err(err);
    }
    Ok(cfg)
}

#[tauri::command]
pub async fn get_llm_config_path() -> Result<String, String> {
    Ok(config::config_path_string())
}

/// 记住最后一次全局调用的 提供方/模型，重启后自动恢复到该会话
#[tauri::command]
pub async fn set_last_chat(provider_id: String, model: String) -> Result<(), String> {
    config::set_last_chat(&provider_id, &model)
}

// ─── 接入配置 CRUD ───

/// 新增或更新一个提供方配置（按 id 判断）
#[tauri::command]
pub async fn upsert_llm_provider<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    provider: ProviderConfig,
) -> Result<ProviderConfig, String> {
    let mut cfg = config::load_config();
    let now = config::now_iso();

    // 校验必填字段
    if provider.name.trim().is_empty() {
        return Err("请提供配置名称".to_string());
    }
    if provider.base_url.trim().is_empty() {
        return Err("请提供 API Base URL".to_string());
    }

    let pid = provider.id.clone();
    let final_id;
    if let Some(existing) = cfg.providers.iter_mut().find(|p| p.id == pid) {
        let mut updated = provider;
        updated.created_at = existing.created_at.clone();
        if updated.updated_at.is_empty() {
            updated.updated_at = now;
        }
        *existing = updated;
        final_id = existing.id.clone();
    } else {
        let mut new_provider = provider;
        if new_provider.id.is_empty() {
            new_provider.id = config::new_id();
        }
        new_provider.created_at = now.clone();
        new_provider.updated_at = now;
        final_id = new_provider.id.clone();
        cfg.providers.push(new_provider);
    }

    config::save_config(&cfg)?;
    notify_llm_config_changed(&app);
    cfg.providers
        .iter()
        .find(|p| p.id == final_id)
        .cloned()
        .ok_or_else(|| "保存提供方配置失败".to_string())
}

/// 删除一个提供方配置
#[tauri::command]
pub async fn delete_llm_provider<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
) -> Result<(), String> {
    let mut cfg = config::load_config();
    // 目标提供方不存在（例如并发下读到了空配置）：直接返回、不落盘，
    // 避免用空配置覆盖磁盘上已有的提供方列表。
    if !cfg.providers.iter().any(|p| p.id == id) {
        return Ok(());
    }
    cfg.providers.retain(|p| p.id != id);
    if cfg.default_provider_id.as_deref() == Some(&id) {
        cfg.default_provider_id = cfg.providers.first().map(|p| p.id.clone());
    }
    // 删除最后一个提供方会得到空列表，属用户明确操作，使用允许空列表的写盘
    config::save_config_allow_empty(&cfg)?;
    notify_llm_config_changed(&app);
    Ok(())
}

/// 设置全局默认提供方
#[tauri::command]
pub async fn set_llm_default_provider<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
) -> Result<(), String> {
    let mut cfg = config::load_config();
    if !cfg.providers.iter().any(|p| p.id == id) {
        return Err("指定的提供方不存在".to_string());
    }
    cfg.default_provider_id = Some(id);
    config::save_config(&cfg)?;
    notify_llm_config_changed(&app);
    Ok(())
}

// ─── 连接测试 ───

#[tauri::command]
pub async fn test_llm_connection(id: String) -> Result<TestResult, String> {
    let cfg = config::load_config();
    let provider = config::find_provider(&cfg, &id)
        .ok_or_else(|| "指定的提供方不存在".to_string())?
        .clone();
    let (ok, latency_ms, model, error) = client::test_connection(&provider).await;
    Ok(TestResult {
        ok,
        latency_ms,
        model,
        error,
    })
}

// ─── 模型管理 ───

/// 从提供方接口探测模型列表
#[tauri::command]
pub async fn list_llm_models(id: String) -> Result<Vec<String>, String> {
    let cfg = config::load_config();
    let provider = config::find_provider(&cfg, &id)
        .ok_or_else(|| "指定的提供方不存在".to_string())?
        .clone();
    client::fetch_models(&provider).await
}

/// 向提供方添加自定义模型 id（用于接口不返回模型列表的网关）
#[tauri::command]
pub async fn add_llm_model<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    model: String,
) -> Result<ProviderConfig, String> {
    let mut cfg = config::load_config();
    let provider = cfg
        .providers
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| "指定的提供方不存在".to_string())?;
    let m = model.trim().to_string();
    if m.is_empty() {
        return Err("模型 id 不能为空".to_string());
    }
    if !provider.models.contains(&m) {
        provider.models.push(m);
    }
    let result = provider.clone();
    config::save_config(&cfg)?;
    notify_llm_config_changed(&app);
    Ok(result)
}

/// 从提供方移除一个模型 id
#[tauri::command]
pub async fn remove_llm_model<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    model: String,
) -> Result<ProviderConfig, String> {
    let mut cfg = config::load_config();
    let provider = cfg
        .providers
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| "指定的提供方不存在".to_string())?;
    provider.models.retain(|m| m != &model);
    if provider.default_model == model {
        provider.default_model = provider.models.first().cloned().unwrap_or_default();
    }
    let result = provider.clone();
    config::save_config(&cfg)?;
    notify_llm_config_changed(&app);
    Ok(result)
}

/// 批量移除提供方下的多个模型 id
#[tauri::command]
pub async fn remove_llm_models<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    models: Vec<String>,
) -> Result<ProviderConfig, String> {
    if models.is_empty() {
        return Err("未选择任何要移除的模型".to_string());
    }
    let mut cfg = config::load_config();
    let provider = cfg
        .providers
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| "指定的提供方不存在".to_string())?;
    for m in &models {
        provider.models.retain(|x| x != m);
    }
    if models.iter().any(|m| &provider.default_model == m) {
        provider.default_model = provider.models.first().cloned().unwrap_or_default();
    }
    let result = provider.clone();
    config::save_config(&cfg)?;
    notify_llm_config_changed(&app);
    Ok(result)
}

/// 设置提供方默认模型
#[tauri::command]
pub async fn set_llm_default_model<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    model: String,
) -> Result<ProviderConfig, String> {
    let mut cfg = config::load_config();
    let provider = cfg
        .providers
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| "指定的提供方不存在".to_string())?;
    provider.default_model = model;
    let result = provider.clone();
    config::save_config(&cfg)?;
    notify_llm_config_changed(&app);
    Ok(result)
}

/// 设置单个模型的能力元数据（类型 / 标签）。类型与标签均为空时删除该条目。
#[tauri::command]
pub async fn set_llm_model_meta<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    model: String,
    #[allow(non_snake_case)] modelType: Option<String>,
    tags: Vec<String>,
) -> Result<ProviderConfig, String> {
    let mut cfg = config::load_config();
    let provider = cfg
        .providers
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| "指定的提供方不存在".to_string())?;
    let m = model.trim().to_string();
    if m.is_empty() {
        return Err("模型 id 不能为空".to_string());
    }
    let cleaned_tags: Vec<String> = tags
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    let model_type_clean = modelType
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    if model_type_clean.is_none() && cleaned_tags.is_empty() {
        provider.model_meta.remove(&m);
    } else {
        provider.model_meta.insert(
            m,
            ModelMeta {
                model_type: model_type_clean,
                tags: cleaned_tags,
                reasoning_efforts: Vec::new(),
                context_window: None,
            },
        );
    }
    let result = provider.clone();
    config::save_config(&cfg)?;
    notify_llm_config_changed(&app);
    Ok(result)
}

/// 供测试或内部调用：返回受支持的提供方类型
#[tauri::command]
pub async fn get_llm_provider_types() -> Result<Vec<String>, String> {
    Ok(vec![
        ProviderType::OpenAI.as_str().to_string(),
        ProviderType::Azure.as_str().to_string(),
        ProviderType::Ollama.as_str().to_string(),
        ProviderType::Custom.as_str().to_string(),
    ])
}
