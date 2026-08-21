// ============================================================
// 微信 IPC — 年度总结
// ============================================================

use crate::wechat::handlers::helpers;

/// 有消息数据的年份列表（降序）
#[tauri::command]
pub async fn get_annual_available_years() -> Result<serde_json::Value, String> {
    helpers::run_blocking(|| {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let years = crate::wechat::annual::available_years(&cfg.decrypted_dir);
        Ok(serde_json::json!({ "years": years }))
    })
    .await
}

/// 指定年份的年度总结
#[tauri::command]
pub async fn get_annual_summary(year: i32) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let summary = crate::wechat::annual::annual_summary(&cfg.decrypted_dir, year)?;
        serde_json::to_value(summary).map_err(|e| e.to_string())
    })
    .await
}
