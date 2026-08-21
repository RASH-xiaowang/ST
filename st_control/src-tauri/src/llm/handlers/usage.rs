// ============================================================
// 大模型管理 — IPC 命令：流量与成本管控
// 自 handlers.rs 拆分：用量读取 / 重置 / 月度汇总（含配额进度）。
// ============================================================

use crate::llm::config;
use crate::llm::types::ProviderUsage;
use serde_json::json;

// ─── 流量与成本管控 ───

#[tauri::command]
pub async fn get_llm_usage() -> Result<crate::llm::types::LlmUsage, String> {
    Ok(config::load_usage())
}

#[tauri::command]
pub async fn reset_llm_usage() -> Result<(), String> {
    config::reset_usage()
}

/// 返回当前月份每个提供方的用量 + 配额上限，便于前端直接渲染进度条
#[tauri::command]
pub async fn get_llm_usage_summary() -> Result<Vec<serde_json::Value>, String> {
    let cfg = config::load_config();
    let usage = config::load_usage();
    let month = config::current_month();
    let month_map = usage.months.get(&month).cloned().unwrap_or_default();

    let mut summary = Vec::new();
    for p in &cfg.providers {
        let u: ProviderUsage = month_map.get(&p.id).cloned().unwrap_or_default();
        summary.push(json!({
            "id": p.id,
            "name": p.name,
            "enabled": p.enabled,
            "usage": u,
            "monthly_token_limit": p.monthly_token_limit,
            "monthly_cost_limit": p.monthly_cost_limit,
            "token_ratio": if let Some(lim) = p.monthly_token_limit {
                if lim > 0 { (u.total_tokens as f64 / lim as f64 * 100.0).min(100.0) } else { 0.0 }
            } else { 0.0 },
            "cost_ratio": if let Some(lim) = p.monthly_cost_limit {
                if lim > 0.0 { (u.cost / lim * 100.0).min(100.0) } else { 0.0 }
            } else { 0.0 },
        }));
    }
    Ok(summary)
}
