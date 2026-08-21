//! 微信 general.db 记录类 IPC（撤回 / 转账 / 红包 / 视频号 / 小程序）

/// 撤回消息缓存记录
#[tauri::command]
pub async fn list_wechat_revokes(
    limit: Option<i64>,
    offset: Option<i64>,
    q: Option<String>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::wechat::general_records::list_revokes(limit, offset, q)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 转账记录
#[tauri::command]
pub async fn list_wechat_transfers(
    limit: Option<i64>,
    offset: Option<i64>,
    q: Option<String>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::wechat::general_records::list_transfers(limit, offset, q)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 红包记录
#[tauri::command]
pub async fn list_wechat_red_envelopes(
    limit: Option<i64>,
    offset: Option<i64>,
    q: Option<String>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::wechat::general_records::list_red_envelopes(limit, offset, q)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 视频号直播记录
#[tauri::command]
pub async fn list_wechat_finder(
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::wechat::general_records::list_finder(limit, offset)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 小程序记录
#[tauri::command]
pub async fn list_wechat_mini_programs(
    limit: Option<i64>,
    offset: Option<i64>,
    q: Option<String>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::wechat::general_records::list_mini_programs(limit, offset, q)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 导出记录为 CSV（返回 CSV 文本，前端保存）
#[tauri::command]
pub async fn export_wechat_records_csv(kind: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let csv = crate::wechat::general_records::export_records_csv(&kind)?;
        Ok(serde_json::json!({ "csv": csv }))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 好友验证 / 新朋友记录
#[tauri::command]
pub async fn list_wechat_friend_verifications(
    limit: Option<i64>,
    offset: Option<i64>,
    q: Option<String>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::wechat::general_records::list_friend_verifications(limit, offset, q)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// CDN 自动获取原图开关状态
#[tauri::command]
pub async fn get_cdn_image_status() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "enabled": crate::wechat::cdn_image::is_cdn_enabled(),
        "localDecrypt": crate::wechat::cdn_image::is_cdn_local_decrypt(),
    }))
}

/// 设置 CDN 自动获取原图开关
#[tauri::command]
pub async fn set_cdn_image_enabled(enabled: bool) -> Result<serde_json::Value, String> {
    crate::wechat::cdn_image::set_cdn_enabled(enabled);
    Ok(serde_json::json!({ "enabled": enabled }))
}

/// 设置 CDN 原图解密方式（true=本地 AES-ECB，aeskey 不出本机；false=服务端解密）
#[tauri::command]
pub async fn set_cdn_image_local_decrypt(local_decrypt: bool) -> Result<serde_json::Value, String> {
    crate::wechat::cdn_image::set_cdn_local_decrypt(local_decrypt);
    Ok(serde_json::json!({ "localDecrypt": local_decrypt }))
}
