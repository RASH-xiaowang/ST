// ============================================================
// 社交关系图谱 — IPC API 与缓存路径
// 自 insights.rs 拆分：图谱获取命令与缓存文件定位。
// ============================================================

use crate::wechat::handlers::helpers;
use std::path::PathBuf;

use super::build_relationship_graph;

/// IPC：获取社交关系图谱
#[tauri::command]
pub async fn get_relationship_graph(
    app: tauri::AppHandle,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        // None = 全部关系；指定时联系人上限 ≈ limit，群上限 ≈ limit / 3
        let contact_limit = limit.map(|l| l.clamp(1, 10000));
        let group_limit = contact_limit.map(|l| (l / 3).max(8));
        build_relationship_graph(
            &cfg.decrypted_dir,
            &cfg.wechat_base_dir,
            &cfg.wxid().unwrap_or_default(),
            contact_limit,
            group_limit,
            Some(&app),
        )
    })
    .await
}

/// 读取上次成功构建的关系图谱缓存（无缓存返回 None）。
/// 用于进入图谱时「先秒开上次结果，再后台刷新」。
#[tauri::command]
pub async fn get_relationship_graph_cached() -> Result<Option<serde_json::Value>, String> {
    helpers::run_blocking(move || {
        let path = graph_cache_path();
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read(&path).map_err(|e| format!("读取图谱缓存失败: {}", e))?;
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("图谱缓存解析失败: {}", e))?;
        Ok(Some(v))
    })
    .await
}

/// 关系图谱缓存文件：`%APPDATA%\st-control\relationship_graph.json`
pub(crate) fn graph_cache_path() -> PathBuf {
    crate::common::st_data_dir().join("relationship_graph.json")
}
