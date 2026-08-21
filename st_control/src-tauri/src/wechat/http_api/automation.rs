// ============================================================
// HTTP API — 自动化管理中心接口（供智能体 / st_agent 调用）
// 自 http_api.rs 拆分：任务查询 / 认领 / 开始 / 完成。
// ============================================================

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use std::collections::HashMap;
use std::sync::Arc;

use super::{check_auth, ApiError, ApiResult, ApiServerState};

// ============ 自动化管理中心接口（供智能体 / st_agent 调用） ============

fn open_automation_conn() -> ApiResult<rusqlite::Connection> {
    rusqlite::Connection::open(crate::automation::control_db_path()).map_err(|e| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            format!("打开数据库失败: {e}"),
        )
    })
}

fn db_err(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "DB_ERROR",
        format!("数据库错误: {e}"),
    )
}

/// GET /api/v1/automation/tasks?agent_id=&status=
/// 查询派发给指定智能体/Agent 的任务
pub(crate) async fn automation_tasks(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<serde_json::Value>> {
    check_auth(&state, &headers, &query, None)?;
    let agent_id = query.get("agent_id").cloned().unwrap_or_default();
    let status = query.get("status").cloned().unwrap_or_default();
    let conn = open_automation_conn()?;
    let tasks = crate::automation::handlers::query_tasks_by_agent(&conn, &agent_id, &status)
        .map_err(db_err)?;
    let items: Vec<serde_json::Value> = tasks
        .iter()
        .map(crate::automation::engine::task_to_json)
        .collect();
    Ok(Json(
        serde_json::json!({ "success": true, "count": items.len(), "items": items }),
    ))
}

/// POST /api/v1/automation/tasks/claim  { task_id, agent_id }
/// 领取任务：pending → claimed
pub(crate) async fn automation_task_claim(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    check_auth(&state, &headers, &HashMap::new(), Some(&body))?;
    let task_id = body
        .get("task_id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError::bad_request("缺少 task_id"))?;
    let agent_id = body
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let conn = open_automation_conn()?;
    let task = crate::automation::db::get_task(&conn, task_id)
        .map_err(db_err)?
        .ok_or_else(|| ApiError::not_found("任务不存在"))?;
    if !task.target_id.is_empty() && task.target_id != agent_id {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "任务不属于该 Agent",
        ));
    }
    if task.status != "pending" {
        return Err(ApiError::bad_request(format!(
            "任务当前状态为 {}，无法领取",
            task.status
        )));
    }
    crate::automation::db::update_task_status(&conn, task_id, "claimed", "").map_err(db_err)?;
    Ok(Json(
        serde_json::json!({ "success": true, "id": task_id, "status": "claimed" }),
    ))
}

/// POST /api/v1/automation/tasks/start  { task_id }
/// 开始执行：claimed → processing
pub(crate) async fn automation_task_start(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    check_auth(&state, &headers, &HashMap::new(), Some(&body))?;
    let task_id = body
        .get("task_id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError::bad_request("缺少 task_id"))?;
    let conn = open_automation_conn()?;
    let task = crate::automation::db::get_task(&conn, task_id)
        .map_err(db_err)?
        .ok_or_else(|| ApiError::not_found("任务不存在"))?;
    if task.status != "claimed" {
        return Err(ApiError::bad_request(format!(
            "任务当前状态为 {}，无法开始执行",
            task.status
        )));
    }
    crate::automation::db::update_task_status(&conn, task_id, "processing", "").map_err(db_err)?;
    Ok(Json(
        serde_json::json!({ "success": true, "id": task_id, "status": "processing" }),
    ))
}

/// POST /api/v1/automation/tasks/complete
/// { sender_username, timestamp, username, reply_text, status? }
/// 提交结果：按三字段更新回复文本与状态（默认 to_reply）
pub(crate) async fn automation_task_complete(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    check_auth(&state, &headers, &HashMap::new(), Some(&body))?;
    let sender = body
        .get("sender_username")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("缺少 sender_username"))?;
    let timestamp = body
        .get("timestamp")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError::bad_request("缺少 timestamp"))?;
    let username = body
        .get("username")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("缺少 username"))?;
    let reply = body
        .get("reply_text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let status = body
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("to_reply");
    let conn = open_automation_conn()?;
    let ok = crate::automation::db::update_reply_by_key(
        &conn, sender, timestamp, username, reply, status,
    )
    .map_err(db_err)?;
    if !ok {
        return Err(ApiError::not_found(
            "未找到匹配任务（请检查 sender_username / timestamp / username）",
        ));
    }
    Ok(Json(
        serde_json::json!({ "success": true, "status": status }),
    ))
}
