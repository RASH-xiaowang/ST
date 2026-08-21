// ============================================================
// 知识库管理 — 指标配置命令
// 自 handlers.rs 拆分：指标显示名 / 可见性读写。
// ============================================================

use crate::kb::db::KbDatabase;
use serde::Deserialize;
use tauri::State;

use super::{analytics_settings_map, ANALYTICS_METRIC_DEFAULTS};

/// 指标配置（8 项内置指标：显示名 + 可见性）
#[tauri::command]
pub async fn kb_get_analytics_settings(
    db: State<'_, KbDatabase>,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = db.conn_lock();
    let map = analytics_settings_map(&conn);
    Ok(ANALYTICS_METRIC_DEFAULTS
        .iter()
        .map(|(key, dflt)| {
            let (label, visible) = map
                .get(*key)
                .cloned()
                .unwrap_or_else(|| (dflt.to_string(), true));
            serde_json::json!({ "key": key, "label": label, "visible": visible })
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsSettingInput {
    pub key: String,
    pub label: String,
    pub visible: bool,
}

/// 保存指标配置（显示名 / 是否展示）
#[tauri::command]
pub async fn kb_set_analytics_settings(
    db: State<'_, KbDatabase>,
    input: AnalyticsSettingInput,
) -> Result<(), String> {
    if !ANALYTICS_METRIC_DEFAULTS
        .iter()
        .any(|(k, _)| *k == input.key)
    {
        return Err("未知的指标标识".to_string());
    }
    let label = input.label.trim().to_string();
    if label.is_empty() {
        return Err("指标显示名不能为空".to_string());
    }
    let conn = db.conn_lock();
    conn.execute(
        "INSERT INTO kb_analytics_settings (key, label, visible, updated_at) VALUES (?1,?2,?3,datetime('now'))
         ON CONFLICT(key) DO UPDATE SET label=excluded.label, visible=excluded.visible, updated_at=datetime('now')",
        rusqlite::params![input.key, label, input.visible as i64],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
