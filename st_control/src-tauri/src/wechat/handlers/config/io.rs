// ============================================================
// 微信 IPC — 配置读写域
// 依赖：config（完全限定），零顶层导入
// ============================================================

// ─── 配置读写 ───

#[tauri::command]
pub async fn get_wechat_config() -> Result<serde_json::Value, String> {
    let raw = crate::wechat::config::load_raw_config_public();
    let config_path = crate::wechat::config::get_config_path();
    let resolved = crate::wechat::config::WeChatConfig::load().ok();
    Ok(serde_json::json!({
        "raw": raw,
        "configPath": config_path.to_string_lossy(),
        "resolved": resolved,
    }))
}

#[tauri::command]
pub async fn save_wechat_config(config: serde_json::Value) -> Result<(), String> {
    let mut raw: crate::wechat::config::RawConfig =
        serde_json::from_value(config).map_err(|e| format!("配置格式错误: {}", e))?;
    // 防密钥丢失：设置页表单不包含图片密钥字段，整量保存会把已提取的
    // image_aes_key/image_xor_key 覆盖成 null → 全部 V2 图片「解密失败」。
    // 未显式提供时保留磁盘上的现有值（手动置 null 需直接编辑 config.json）。
    if raw.image_aes_key.is_none() || raw.image_xor_key.is_none() {
        if let Some(existing) = crate::wechat::config::load_raw_config_public() {
            if raw.image_aes_key.is_none() {
                raw.image_aes_key = existing.image_aes_key;
            }
            if raw.image_xor_key.is_none() {
                raw.image_xor_key = existing.image_xor_key;
            }
        }
    }
    crate::wechat::config::save_config(&raw).map_err(|e| e.to_string())?;
    crate::wechat::config::WeChatConfig::refresh_cache();
    Ok(())
}

/// 获取 HTTP API 当前运行状态（启用/端口/令牌）
#[tauri::command]
pub async fn get_api_settings(
    state: tauri::State<'_, std::sync::Arc<crate::wechat::http_api::ApiServerState>>,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "enabled": state.is_enabled(),
        "port": state.current_port(),
        "token": state.current_token(),
    }))
}

/// 从 config.json 重新加载 API 设置并热应用到运行中的服务
/// （令牌即时生效；端口变化时优雅重启监听，无需重启应用）
#[tauri::command]
pub async fn apply_api_settings(
    state: tauri::State<'_, std::sync::Arc<crate::wechat::http_api::ApiServerState>>,
) -> Result<serde_json::Value, String> {
    let cfg =
        crate::wechat::config::WeChatConfig::load().map_err(|e| format!("读取配置失败: {}", e))?;
    let token_desc = if cfg.api_token.is_some() {
        "已配置"
    } else {
        "未配置"
    };
    state.apply_settings(cfg.api_enabled, cfg.api_port, cfg.api_token);
    log::info!(
        "[http-api] 设置已热应用: enabled={} port={} token={}",
        cfg.api_enabled,
        cfg.api_port,
        token_desc,
    );
    Ok(serde_json::json!({
        "enabled": state.is_enabled(),
        "port": state.current_port(),
        "token": state.current_token(),
    }))
}

#[tauri::command]
pub async fn detect_wechat_accounts() -> Result<Vec<crate::wechat::config::DetectedAccount>, String>
{
    Ok(crate::wechat::config::detect_accounts())
}

#[tauri::command]
pub async fn scan_wechat_accounts(
    base_dir: String,
) -> Result<Vec<crate::wechat::config::DetectedAccount>, String> {
    let path = std::path::Path::new(&base_dir);
    if !path.is_dir() {
        return Err("指定的目录不存在".to_string());
    }
    Ok(crate::wechat::config::scan_accounts(path))
}

#[tauri::command]
pub async fn get_wechat_keys_info() -> Result<serde_json::Value, String> {
    let cfg =
        crate::wechat::config::WeChatConfig::load().map_err(|e| format!("加载配置失败: {}", e))?;
    let keys_exists = cfg.has_keys();
    let (key_count, key_format) = if keys_exists {
        match crate::wechat::keys::Keys::from_file(&cfg.keys_file) {
            Ok(keys) => (keys.len(), keys.key_format.clone()),
            Err(_) => (0, None),
        }
    } else {
        (0, None)
    };
    Ok(serde_json::json!({
        "keysFile": cfg.keys_file.to_string_lossy(),
        "keysExists": keys_exists,
        "keyCount": key_count,
        "keyFormat": key_format,
    }))
}
