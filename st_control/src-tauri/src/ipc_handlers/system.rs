// ============================================================
// 系统 IPC — 服务端状态 / 应用信息 / 任务下发
// 依赖：ws_server / serde / Arc（完全限定 + 顶层导入）
// ============================================================

use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ─────────────────────────────────────────────
// 服务端 / 系统 IPC
// ─────────────────────────────────────────────

/// 服务器状态响应
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
pub struct ServerStatusResponse {
    pub status: String,
    pub port: u16,
    pub agentCount: u32,
    pub messageCount: u64,
}

impl From<crate::ws_server::ServerState> for ServerStatusResponse {
    fn from(state: crate::ws_server::ServerState) -> Self {
        Self {
            status: match state.status {
                crate::ws_server::ServerStatus::Stopped => "stopped",
                crate::ws_server::ServerStatus::Starting => "starting",
                crate::ws_server::ServerStatus::Running => "running",
                crate::ws_server::ServerStatus::Stopping => "stopping",
                crate::ws_server::ServerStatus::Error => "error",
            }
            .to_string(),
            port: state.port,
            agentCount: state.clients.len() as u32,
            messageCount: state.message_count,
        }
    }
}

/// IPC：获取服务器状态（含已连接的 Agent 数量）
#[tauri::command]
pub async fn get_server_status(
    server: tauri::State<'_, Arc<crate::ws_server::WsServer>>,
) -> Result<ServerStatusResponse, String> {
    let state = server.get_state().await;
    Ok(ServerStatusResponse::from(state))
}

/// IPC：获取应用信息
#[tauri::command]
pub async fn get_app_info() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "name": "ST 控制台",
        "version": "1.0.0",
        "description": "本地数据管理 · AI 工作台 · 微信数据控制端"
    }))
}

#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
}

#[tauri::command]
pub async fn get_system_info() -> Result<SystemInfo, String> {
    Ok(SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    })
}

/// 下发任务的参数
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct SendCommandArgs {
    pub agentId: String,
    pub method: String,
    pub payload: Option<serde_json::Value>,
}

/// IPC：向指定 Agent 下发任务
#[tauri::command]
pub async fn send_command_to_agent(
    server: tauri::State<'_, Arc<crate::ws_server::WsServer>>,
    args: SendCommandArgs,
) -> Result<String, String> {
    let status = *server.status.read().await;
    if status != crate::ws_server::ServerStatus::Running {
        return Err("服务器未运行".to_string());
    }
    let clients = server.clients.read().await;
    if !clients.contains_key(&args.agentId) {
        return Err(format!("Agent {} 不在线", args.agentId));
    }
    drop(clients);
    let method_name = args.method.clone();
    let agent_id = args.agentId.clone();
    let msg = crate::ws_server::ProtocolMessage {
        msg_type: crate::ws_server::MessageType::Command,
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        source: "st_control".to_string(),
        target: "st_agent".to_string(),
        method: Some(args.method),
        payload: Some(serde_json::json!({
            "targetAgentId": args.agentId,
            "task": args.payload
        })),
        correlation_id: None,
        error: None,
    };
    server.send_to_agent(&args.agentId, &msg);
    log::info!("已向 {} 下发任务: {:?}", agent_id, method_name);
    Ok(msg.id)
}
