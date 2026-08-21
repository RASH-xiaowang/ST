// ============================================================
// 自动化管理中心 — Tauri 命令（前端界面调用）
// ============================================================

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use super::db::{
    self, delete_task, list_rules, AutomationRule, AutomationStats, RuleCondition, WechatTask,
};
use super::engine::{rule_crud_sync, task_to_json};
use super::AutomationState;

/// 规则保存入参
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInput {
    pub id: Option<i64>,
    pub name: String,
    pub enabled: bool,
    pub priority: i64,
    pub conditions: Vec<RuleCondition>,
    pub analyze_fields: Vec<serde_json::Value>,
    pub prompt_override: String,
    pub provider_id: String,
    pub model: String,
    pub dispatch_mode: String,
    pub target_type: String,
    pub target_id: String,
    /// 绑定的 AI 角色 id（内置 Worker 执行时注入角色提示词）
    #[serde(default)]
    pub role_id: String,
}

fn rule_from_input(i: &RuleInput) -> AutomationRule {
    AutomationRule {
        id: i.id.unwrap_or(0),
        name: i.name.clone(),
        enabled: i.enabled,
        priority: i.priority,
        conditions: i.conditions.clone(),
        analyze_fields: i
            .analyze_fields
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect(),
        prompt_override: i.prompt_override.clone(),
        provider_id: i.provider_id.clone(),
        model: i.model.clone(),
        dispatch_mode: i.dispatch_mode.clone(),
        target_type: i.target_type.clone(),
        target_id: i.target_id.clone(),
        role_id: i.role_id.clone(),
        hit_count: 0,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

#[tauri::command]
pub fn automation_list_rules(state: State<'_, AutomationState>) -> Result<Vec<Value>, String> {
    let conn = state.conn();
    let rules = list_rules(&conn).map_err(|e| e.to_string())?;
    Ok(rules
        .iter()
        .map(|r| {
            json!({
                "id": r.id, "name": r.name, "enabled": r.enabled, "priority": r.priority,
                "conditions": r.conditions, "analyzeFields": r.analyze_fields,
                "promptOverride": r.prompt_override, "providerId": r.provider_id, "model": r.model,
                "dispatchMode": r.dispatch_mode, "targetType": r.target_type, "targetId": r.target_id,
                "hitCount": r.hit_count, "createdAt": r.created_at, "updatedAt": r.updated_at,
            })
        })
        .collect())
}

#[tauri::command]
pub fn automation_save_rule(
    state: State<'_, AutomationState>,
    input: RuleInput,
) -> Result<i64, String> {
    let conn = state.conn();
    let rule = rule_from_input(&input);
    rule_crud_sync(&conn, &rule, input.id.is_none())
}

#[tauri::command]
pub fn automation_delete_rule(state: State<'_, AutomationState>, id: i64) -> Result<(), String> {
    let conn = state.conn();
    super::engine::delete_rule_sync(&conn, id)
}

#[tauri::command]
pub fn automation_toggle_rule(
    state: State<'_, AutomationState>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    let conn = state.conn();
    conn.execute(
        "UPDATE automation_rules SET enabled=?1, updated_at=datetime('now','localtime') WHERE id=?2",
        rusqlite::params![enabled as i64, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn automation_list_tasks(
    state: State<'_, AutomationState>,
    status: Option<String>,
    keyword: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<Value, String> {
    let conn = state.conn();
    let limit = page_size.unwrap_or(50).clamp(1, 200);
    let page = page.unwrap_or(1).max(1);
    let offset = (page - 1) * limit;
    let tasks = db::list_tasks(&conn, status.as_deref(), keyword.as_deref(), limit, offset)
        .map_err(|e| e.to_string())?;
    let total =
        db::count_tasks(&conn, status.as_deref(), keyword.as_deref()).map_err(|e| e.to_string())?;
    Ok(json!({
        "items": tasks.iter().map(task_to_json).collect::<Vec<_>>(),
        "total": total,
        "page": page,
        "pageSize": limit,
    }))
}

#[tauri::command]
pub fn automation_get_task(state: State<'_, AutomationState>, id: i64) -> Result<Value, String> {
    let conn = state.conn();
    let t = db::get_task(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "任务不存在".to_string())?;
    Ok(task_to_json(&t))
}

#[tauri::command]
pub fn automation_set_task_status(
    state: State<'_, AutomationState>,
    id: i64,
    status: String,
) -> Result<(), String> {
    let conn = state.conn();
    db::update_task_status(&conn, id, &status, "").map_err(|e| e.to_string())
}

#[tauri::command]
pub fn automation_set_task_target(
    state: State<'_, AutomationState>,
    id: i64,
    target_type: String,
    target_id: String,
) -> Result<(), String> {
    let conn = state.conn();
    db::update_task_target(&conn, id, &target_type, &target_id).map_err(|e| e.to_string())?;
    db::update_task_status(&conn, id, "claimed", "").map_err(|e| e.to_string())
}

#[tauri::command]
pub fn automation_edit_task_reply(
    state: State<'_, AutomationState>,
    id: i64,
    reply_text: String,
    status: String,
) -> Result<(), String> {
    let conn = state.conn();
    db::update_task_reply(
        &conn,
        id,
        &reply_text,
        if status.is_empty() {
            "to_reply"
        } else {
            &status
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn automation_edit_ai_extract(
    state: State<'_, AutomationState>,
    id: i64,
    ai_extract: String,
) -> Result<(), String> {
    let conn = state.conn();
    conn.execute(
        "UPDATE task_wechat_info SET ai_extract=?1, updated_at=datetime('now','localtime') WHERE id=?2",
        rusqlite::params![ai_extract, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn automation_delete_task(state: State<'_, AutomationState>, id: i64) -> Result<(), String> {
    let conn = state.conn();
    delete_task(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn automation_stats(state: State<'_, AutomationState>) -> Result<AutomationStats, String> {
    let conn = state.conn();
    db::stats(&conn).map_err(|e| e.to_string())
}

/// 模拟推送一条消息（调试/验证 SSE 消费与规则引擎链路）
#[tauri::command]
pub async fn automation_simulate_push(
    app: AppHandle,
    state: State<'_, AutomationState>,
    content: Option<String>,
    sender_username: Option<String>,
    username: Option<String>,
) -> Result<i64, String> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let now_us = now_ms * 1000;
    let msg = json!({
        "ack_id": format!("sim_{now_ms}"),
        "channel": "event",
        "chat": "",
        "content": content.unwrap_or_else(|| "新丰田预审 购车价格:100000 融资金额:80000 婚姻状况:离异".to_string()),
        "decrypt_ms": 1.0,
        "is_group": true,
        "is_send": false,
        "local_id": Value::Null,
        "media_type": Value::Null,
        "msg_type": 1,
        "pages": 1,
        "sender": "",
        "sender_username": sender_username.unwrap_or_else(|| "wxid_sim_test".to_string()),
        "session_type": "group",
        "sort_seq": Value::Null,
        "time": "12:00:00",
        "timestamp": now_us,
        "ts_backend": now_ms,
        "username": username.unwrap_or_else(|| "sim_chatroom@chatroom".to_string()),
    });
    // 实时推送给前端概览（与 SSE 消费同一事件名）
    let _ = app.emit("automation://message", &msg);
    // 走规则引擎（同步阶段，不含 AI）
    let conn = state.conn();
    match super::engine::process_sync(&conn, &msg) {
        Ok(Some(outcome)) => {
            if let Some(rule) = outcome.rule {
                if let Some(prompt) = outcome.analyze_prompt {
                    // 有 AI 分析规则：异步分析后写回
                    let conn_path = crate::automation::control_db_path();
                    tauri::async_runtime::spawn(async move {
                        let result = super::engine::finish_ai(&rule, &prompt).await;
                        if let Ok(c) = rusqlite::Connection::open(&conn_path) {
                            match result {
                                Ok((extract, ttype, tid)) => {
                                    let _ = super::engine::apply_ai_result(
                                        &c,
                                        outcome.task_id,
                                        &extract,
                                        &ttype,
                                        &tid,
                                        "",
                                    );
                                }
                                Err(e) => {
                                    let _ = super::engine::apply_ai_result(
                                        &c,
                                        outcome.task_id,
                                        &Value::Null,
                                        "",
                                        "",
                                        &e,
                                    );
                                }
                            }
                        }
                    });
                }
            }
            Ok(outcome.task_id)
        }
        Ok(None) => Ok(0),
        Err(e) => Err(e),
    }
}

/// 调试：直接触发微信监控 router 广播（验证真实 SSE 消费链路）
#[tauri::command]
pub async fn automation_debug_broadcast(
    monitor: tauri::State<'_, std::sync::Arc<crate::wechat::handlers::WeChatMonitorState>>,
    content: Option<String>,
) -> Result<String, String> {
    match monitor.router() {
        Some(router) => {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            router
                .broadcast(serde_json::json!({
                    "ack_id": format!("dbg_{now_ms}"),
                    "channel": "event",
                    "chat": "",
                    "content": content.unwrap_or_else(|| "调试广播：验证 SSE 消费链路".to_string()),
                    "decrypt_ms": 1.0,
                    "is_group": true,
                    "is_send": false,
                    "local_id": Value::Null,
                    "media_type": Value::Null,
                    "msg_type": 1,
                    "pages": 1,
                    "sender": "",
                    "sender_username": "wxid_dbg_test",
                    "session_type": "group",
                    "sort_seq": Value::Null,
                    "time": "12:00:00",
                    "timestamp": now_ms * 1000,
                    "ts_backend": now_ms,
                    "username": "dbg_chatroom@chatroom",
                }))
                .await;
            Ok(format!("已通过 router 广播 (ack=dbg_{now_ms})"))
        }
        None => Err("微信监控未运行，无 router".to_string()),
    }
}

/// 查询 SSE 消费状态（连接/收到计数/最后消息时间）
#[tauri::command]
pub fn automation_conn_status(state: State<'_, AutomationState>) -> Result<Value, String> {
    use std::sync::atomic::Ordering;
    let last = state.sse_last_at.lock().map(|g| g.clone()).unwrap_or(None);
    Ok(json!({
        "connected": state.sse_connected.load(Ordering::Relaxed),
        "received": state.sse_received.load(Ordering::Relaxed),
        "lastAt": last,
        "url": "http://127.0.0.1:5032/api/v1/push/messages",
    }))
}

/// 手动重启 SSE 消费（重连）：先取消旧消费任务再启动，避免双消费者
#[tauri::command]
pub fn automation_reconnect(
    app: AppHandle,
    state: State<'_, AutomationState>,
) -> Result<(), String> {
    state.restart_sse(
        app,
        "http://127.0.0.1:5032/api/v1/push/messages".to_string(),
    );
    Ok(())
}

#[tauri::command]
pub fn automation_update_reply_by_key(
    state: State<'_, AutomationState>,
    sender_username: String,
    timestamp: i64,
    username: String,
    reply_text: String,
    status: String,
) -> Result<bool, String> {
    let conn = state.conn();
    db::update_reply_by_key(
        &conn,
        &sender_username,
        timestamp,
        &username,
        &reply_text,
        if status.is_empty() {
            "replied"
        } else {
            &status
        },
    )
    .map_err(|e| e.to_string())
}

/// 提供给其他模块使用的任务查询（智能体领任务）
pub fn query_tasks_by_agent(
    conn: &rusqlite::Connection,
    agent_id: &str,
    status: &str,
) -> Result<Vec<WechatTask>, String> {
    let sql = format!(
        "SELECT {} FROM task_wechat_info WHERE target_id=?1 AND (?2='' OR status=?2) ORDER BY id ASC",
        "id,ack_id,channel,chat,content,decrypt_ms,is_group,is_send,local_id,media_type,msg_type,pages,sender,sender_username,session_type,sort_seq,time,timestamp,ts_backend,username,rule_id,rule_name,ai_extract,full_json,target_type,target_id,reply_text,status,error,retry_count,created_at,updated_at"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![agent_id, status], |r| {
            Ok(WechatTask {
                id: r.get(0)?,
                ack_id: r.get(1)?,
                content: r.get(4)?,
                sender_username: r.get(13)?,
                session_type: r.get(14)?,
                is_group: r.get::<_, i64>(6)? != 0,
                is_send: r.get::<_, i64>(7)? != 0,
                media_type: r.get(9)?,
                msg_type: r.get(10)?,
                timestamp: r.get(17)?,
                username: r.get(19)?,
                rule_id: r.get(20)?,
                rule_name: r.get(21)?,
                ai_extract: r.get(22)?,
                full_json: r.get(23)?,
                target_type: r.get(24)?,
                target_id: r.get(25)?,
                reply_text: r.get(26)?,
                status: r.get(27)?,
                error: r.get(28)?,
                retry_count: r.get(29)?,
                created_at: r.get(30)?,
                updated_at: r.get(31)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
