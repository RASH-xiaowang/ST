// ============================================================
// 微信 IPC — 每日总结（任务 / 记录 / 群成员 / 格式）
// ============================================================

use crate::wechat::handlers::helpers;

#[tauri::command]
pub async fn list_daily_summary_tasks() -> Result<serde_json::Value, String> {
    helpers::run_blocking(|| {
        let tasks = crate::wechat::daily_summary::list_tasks()?;
        serde_json::to_value(tasks).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn save_daily_summary_task(task: serde_json::Value) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let task = crate::wechat::daily_summary::save_task(task)?;
        serde_json::to_value(task).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_daily_summary_task(id: i64) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        crate::wechat::daily_summary::delete_task(id)?;
        Ok(serde_json::json!({ "ok": true }))
    })
    .await
}

#[tauri::command]
pub async fn toggle_daily_summary_task(
    id: i64,
    enabled: bool,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        crate::wechat::daily_summary::toggle_task(id, enabled)?;
        Ok(serde_json::json!({ "ok": true }))
    })
    .await
}

/// 立即执行一次总结任务（会真实调用所选模型）
#[tauri::command]
pub async fn run_daily_summary_task(id: i64) -> Result<serde_json::Value, String> {
    let rec = crate::wechat::daily_summary::execute_task(id).await?;
    serde_json::to_value(rec).map_err(|e| e.to_string())
}

/// 按自定义日期范围执行总结（总结历史聊天内容）
#[tauri::command]
pub async fn run_daily_summary_range(
    task_id: i64,
    start_date: String,
    end_date: String,
) -> Result<serde_json::Value, String> {
    let rec =
        crate::wechat::daily_summary::execute_task_range(task_id, start_date, end_date).await?;
    serde_json::to_value(rec).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_daily_summary_records(task_id: Option<i64>) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let records = crate::wechat::daily_summary::list_records(task_id)?;
        serde_json::to_value(records).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_daily_summary_record(id: i64) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        crate::wechat::daily_summary::delete_record(id)?;
        Ok(serde_json::json!({ "ok": true }))
    })
    .await
}

#[tauri::command]
pub async fn get_daily_summary_formats() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "formats": crate::wechat::daily_summary::summary_formats()
    }))
}

/// 获取指定群聊的成员列表（供“关注成员”选择）
#[tauri::command]
pub async fn get_group_members(group_username: String) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let members =
            crate::wechat::daily_summary::get_group_members(&cfg.decrypted_dir, &group_username)?;
        Ok(serde_json::json!({ "members": members }))
    })
    .await
}
