// ============================================================
// 微信 IPC — 账号归档导出（ZIP）
// ============================================================

use crate::wechat::handlers::helpers;

/// 将解密数据库与本地资源打包为 ZIP 归档
#[tauri::command]
pub async fn export_wechat_archive(
    app: tauri::AppHandle,
    output_dir: Option<String>,
    include_resources: Option<bool>,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        crate::wechat::archive::export_archive(
            &app,
            &cfg.decrypted_dir,
            output_dir,
            include_resources.unwrap_or(true),
        )
    })
    .await
}

/// 导入微信备份（账号归档 ZIP 或已解密目录）到本地解密数据区
#[tauri::command]
pub async fn import_wechat_backup(source: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::wechat::import_backup::import_wechat_backup(std::path::Path::new(&source))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}
