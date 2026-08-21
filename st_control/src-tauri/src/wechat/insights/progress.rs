// ============================================================
// 微信关系图谱 — 进度/分块事件发射
// 自 insights.rs 拆分：图谱构建过程中的进度、分块与完成事件。
// ============================================================

use std::collections::HashMap;

use super::stats::CountMap;

pub(crate) fn emit_progress(
    app: Option<&tauri::AppHandle>,
    phase: &str,
    done: usize,
    total: usize,
    message: &str,
) {
    if let Some(app) = app {
        use tauri::Emitter;
        let percent = if total == 0 {
            0
        } else {
            (done as f64 * 100.0 / total as f64).round() as u32
        };
        let _ = app.emit(
            "wechat-graph-progress",
            serde_json::json!({
                "phase": phase,
                "done": done,
                "total": total,
                "percent": percent.min(100),
                "message": message,
            }),
        );
    }
}

/// 增量推送上下文：冷构建时把已就绪的节点/边尽早推给前端（懒加载组装）
pub(crate) struct GraphEmitCtx {
    pub(crate) app: tauri::AppHandle,
    pub(crate) self_username: String,
    pub(crate) name_map: HashMap<String, String>,
    pub(crate) is_group_map: HashMap<String, bool>,
    pub(crate) is_official_map: HashMap<String, bool>,
}

/// 扫描阶段：每扫完一个分库，把该分库中已知会话的节点与「我→会话」边推给前端
pub(crate) fn emit_graph_chunk(ctx: &GraphEmitCtx, counts: &CountMap, done: usize, total: usize) {
    use tauri::Emitter;
    if counts.is_empty() {
        return;
    }
    let mut nodes: Vec<serde_json::Value> = Vec::with_capacity(counts.len());
    let mut edges: Vec<serde_json::Value> = Vec::with_capacity(counts.len());
    for (username, (cnt, last_ts)) in counts {
        let label = ctx
            .name_map
            .get(username)
            .cloned()
            .unwrap_or_else(|| username.clone());
        let kind = if ctx.is_official_map.get(username).copied().unwrap_or(false) {
            "official"
        } else if ctx.is_group_map.get(username).copied().unwrap_or(false)
            || username.ends_with("@chatroom")
        {
            "group"
        } else {
            "contact"
        };
        nodes.push(serde_json::json!({
            "id": username,
            "label": label,
            "kind": kind,
            "msg_count": cnt,
            "active_days": 0,
            "last_ts": last_ts,
            "member_count": 0,
        }));
        edges.push(serde_json::json!({
            "source": ctx.self_username,
            "target": username,
            "weight": (*cnt).max(1),
            "msg_count": *cnt,
            "active_days": 0,
            "last_ts": *last_ts,
            "kinds": ["message"],
        }));
    }
    let percent = if total == 0 {
        0
    } else {
        (done as f64 * 100.0 / total as f64).round() as u32
    };
    let _ = ctx.app.emit(
        "wechat-graph-progress",
        serde_json::json!({
            "phase": "chunk",
            "done": done,
            "total": total,
            "percent": percent.min(100),
            "message": format!("扫描分库 {}/{}…", done, total),
            "nodes": nodes,
            "edges": edges,
        }),
    );
}

/// 活跃天数阶段：每算完一批会话，把增量更新推给前端
pub(crate) fn emit_days_chunk(
    app: &tauri::AppHandle,
    updates: &[serde_json::Value],
    done: usize,
    total: usize,
) {
    use tauri::Emitter;
    if updates.is_empty() {
        return;
    }
    let percent = if total == 0 {
        0
    } else {
        (done as f64 * 100.0 / total as f64).round() as u32
    };
    let _ = app.emit(
        "wechat-graph-progress",
        serde_json::json!({
            "phase": "days_chunk",
            "done": done,
            "total": total,
            "percent": percent.min(100),
            "message": format!("计算活跃天数 {}/{}…", done, total),
            "updates": updates,
        }),
    );
}

/// 组装完成：推送完整图谱数据，前端以 finalData 覆盖增量结果
pub(crate) fn emit_graph_final(app: &tauri::AppHandle, data: &serde_json::Value) {
    use tauri::Emitter;
    let _ = app.emit(
        "wechat-graph-progress",
        serde_json::json!({
            "phase": "final",
            "done": 1,
            "total": 1,
            "percent": 100,
            "message": "图谱组装完成",
            "finalData": data,
        }),
    );
}
