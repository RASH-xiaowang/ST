use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

/// 消息类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    Command,
    Response,
    Event,
    Heartbeat,
    Error,
}

/// 通信协议消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    #[serde(rename = "type")]
    pub msg_type: MessageType,
    pub id: String,
    pub timestamp: i64,
    pub source: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<MessageError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageError {
    pub code: String,
    pub message: String,
}

impl ProtocolMessage {
    pub fn new(
        msg_type: MessageType,
        source: &str,
        target: &str,
        method: Option<&str>,
        payload: Option<serde_json::Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().timestamp_millis(),
            msg_type,
            source: source.to_string(),
            target: target.to_string(),
            method: method.map(|m| m.to_string()),
            payload,
            correlation_id: None,
            error: None,
        }
    }

    pub fn success_response(original: &ProtocolMessage, data: Option<serde_json::Value>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().timestamp_millis(),
            msg_type: MessageType::Response,
            source: original.target.clone(),
            target: original.source.clone(),
            method: original.method.clone(),
            payload: data,
            correlation_id: Some(original.id.clone()),
            error: None,
        }
    }

    pub fn heartbeat_response(original_source: &str) -> Self {
        Self::new(
            MessageType::Heartbeat,
            "st_control",
            original_source,
            None,
            Some(serde_json::json!({
                "time": Utc::now().to_rfc3339(),
                "status": "alive",
                "server_time": Utc::now().timestamp_millis()
            })),
        )
    }
}

/// 已连接的 Agent 客户端信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedClient {
    pub id: String,
    pub name: String,
    pub connected_at: String,
    pub last_heartbeat: String,
    pub remote_addr: String,
}

/// 服务器状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ServerStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerState {
    pub status: ServerStatus,
    pub port: u16,
    pub clients: Vec<ConnectedClient>,
    pub started_at: Option<String>,
    pub message_count: u64,
}

/// 服务器
pub struct WsServer {
    pub status: Arc<RwLock<ServerStatus>>,
    pub clients: Arc<RwLock<HashMap<String, ConnectedClient>>>,
    pub message_count: Arc<RwLock<u64>>,
    pub started_at: Arc<RwLock<Option<String>>>,
    pub shutdown_tx: Arc<Mutex<Option<broadcast::Sender<()>>>>,
    /// 事件通道：WebSocket 事件 → Tauri 前端
    pub event_tx: broadcast::Sender<String>,
    /// 广播通道：Tauri 前端 → 所有 WebSocket 客户端
    pub broadcast_tx: broadcast::Sender<String>,
    /// 定向通道：(target_client_id, message_json)
    pub direct_tx: broadcast::Sender<(String, String)>,
    pub port: Arc<RwLock<u16>>,
}

impl WsServer {
    pub fn new(port: u16) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        let (broadcast_tx, _) = broadcast::channel(256);
        let (direct_tx, _) = broadcast::channel(256);
        Self {
            status: Arc::new(RwLock::new(ServerStatus::Stopped)),
            clients: Arc::new(RwLock::new(HashMap::new())),
            message_count: Arc::new(RwLock::new(0)),
            started_at: Arc::new(RwLock::new(None)),
            shutdown_tx: Arc::new(Mutex::new(None)),
            event_tx,
            broadcast_tx,
            direct_tx,
            port: Arc::new(RwLock::new(port)),
        }
    }

    pub async fn get_state(&self) -> ServerState {
        let status = *self.status.read().await;
        let clients = self.clients.read().await;
        let client_list: Vec<ConnectedClient> = clients.values().cloned().collect();
        let message_count = *self.message_count.read().await;
        let started_at = self.started_at.read().await.clone();
        let port = *self.port.read().await;
        ServerState {
            status,
            port,
            clients: client_list,
            started_at,
            message_count,
        }
    }

    /// 启动服务器（自动调用，不需要手动点击）
    pub async fn start(&self) -> Result<(), String> {
        let mut status = self.status.write().await;
        if *status == ServerStatus::Running {
            return Ok(()); // 已在运行则静默成功
        }
        *status = ServerStatus::Starting;
        drop(status);

        let port = *self.port.read().await;
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| format!("无法绑定端口 {}: {}", port, e))?;

        let (shutdown_tx, _) = broadcast::channel(1);
        let mut shutdown_handle = self.shutdown_tx.lock().await;
        *shutdown_handle = Some(shutdown_tx.clone());
        drop(shutdown_handle);

        *self.status.write().await = ServerStatus::Running;
        *self.started_at.write().await = Some(Utc::now().to_rfc3339());

        let shutdown_for_heartbeat = shutdown_tx.subscribe();

        let server = Arc::new(ServerInner {
            shutdown_tx,
            status: self.status.clone(),
            clients: self.clients.clone(),
            message_count: self.message_count.clone(),
            event_tx: self.event_tx.clone(),
            broadcast_tx: self.broadcast_tx.clone(),
            direct_tx: self.direct_tx.clone(),
        });

        let hb_clients = self.clients.clone();
        let hb_status = self.status.clone();
        tokio::spawn(async move {
            heartbeat_loop(hb_clients, hb_status, shutdown_for_heartbeat).await;
        });

        tokio::spawn(async move {
            server.accept_loop(listener).await;
        });

        Ok(())
    }

    /// 向指定 Agent 定向发送消息
    pub fn send_to_agent(&self, target_client_id: &str, msg: &ProtocolMessage) {
        if let Ok(json) = serde_json::to_string(msg) {
            let _ = self.direct_tx.send((target_client_id.to_string(), json));
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<String> {
        self.event_tx.subscribe()
    }
}

struct ServerInner {
    shutdown_tx: broadcast::Sender<()>,
    status: Arc<RwLock<ServerStatus>>,
    clients: Arc<RwLock<HashMap<String, ConnectedClient>>>,
    message_count: Arc<RwLock<u64>>,
    event_tx: broadcast::Sender<String>,
    broadcast_tx: broadcast::Sender<String>,
    direct_tx: broadcast::Sender<(String, String)>,
}

impl ServerInner {
    async fn accept_loop(&self, listener: TcpListener) {
        log::info!("Control 服务器正在监听 Agent 连接...");

        loop {
            {
                let status = self.status.read().await;
                if *status != ServerStatus::Running {
                    break;
                }
            }

            let accept_result =
                tokio::time::timeout(std::time::Duration::from_secs(1), listener.accept()).await;

            match accept_result {
                Ok(Ok((stream, peer_addr))) => {
                    let ip = peer_addr.ip().to_string(); // 纯 IP，不含端口
                    log::info!("Agent 连接来自: {} (IP: {})", peer_addr, ip);
                    let clients = self.clients.clone();
                    let message_count = self.message_count.clone();
                    let event_tx = self.event_tx.clone();
                    let shutdown_rx = self.shutdown_tx.subscribe();
                    let broadcast_rx = self.broadcast_tx.subscribe();
                    let direct_rx = self.direct_tx.subscribe();

                    tokio::spawn(async move {
                        handle_connection(
                            stream,
                            ip,
                            ConnCtx {
                                clients,
                                message_count,
                                event_tx,
                                shutdown_rx,
                                broadcast_rx,
                                direct_rx,
                            },
                        )
                        .await;
                    });
                }
                Ok(Err(e)) => log::error!("接受连接错误: {}", e),
                Err(_) => {}
            }
        }

        log::info!("服务器接受连接循环已结束");
    }
}

/// WebSocket 连接共享上下文（clients/计数/各广播订阅）
struct ConnCtx {
    clients: Arc<RwLock<HashMap<String, ConnectedClient>>>,
    message_count: Arc<RwLock<u64>>,
    event_tx: broadcast::Sender<String>,
    shutdown_rx: broadcast::Receiver<()>,
    broadcast_rx: broadcast::Receiver<String>,
    direct_rx: broadcast::Receiver<(String, String)>,
}

async fn handle_connection(stream: tokio::net::TcpStream, ip: String, ctx: ConnCtx) {
    // 纯 IP 地址（不含端口）
    let ConnCtx {
        clients,
        message_count,
        event_tx,
        mut shutdown_rx,
        mut broadcast_rx,
        mut direct_rx,
    } = ctx;
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            log::error!("WebSocket 握手失败 ({}): {}", ip, e);
            return;
        }
    };

    let client_id = Uuid::new_v4().to_string();
    let connected_at = Utc::now();
    let client_info = ConnectedClient {
        id: client_id.clone(),
        name: "等待名称...".to_string(), // 首次心跳时更新为真实名称
        connected_at: connected_at.to_rfc3339(),
        last_heartbeat: connected_at.to_rfc3339(),
        remote_addr: ip, // 纯 IP
    };

    clients
        .write()
        .await
        .insert(client_id.clone(), client_info.clone());

    let _ = event_tx.send(
        serde_json::json!({
            "event": "agent_connected", "client": client_info
        })
        .to_string(),
    );

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // 最后收到应用层 heartbeat 的时间
    let mut last_heartbeat = std::time::Instant::now();
    // 最后收到任何消息（含 PONG）的时间
    let mut last_any_activity = std::time::Instant::now();

    // 定时器：每 15s 发送 WebSocket PING 帧
    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(10));

    // 定时器：每 1s 检查各类超时
    let mut check_interval = tokio::time::interval(std::time::Duration::from_secs(1));

    let conn_tag = format!(
        "[{}] {}@{}",
        &client_id[..6],
        client_info.name,
        client_info.remote_addr
    );

    log::info!("{} 接入", conn_tag);

    loop {
        tokio::select! {
            // ============================================================
            // 来自 WebSocket 客户端的消息
            // ============================================================
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        last_any_activity = std::time::Instant::now();
                        if let Ok(protocol_msg) = serde_json::from_str::<ProtocolMessage>(&text) {
                            *message_count.write().await += 1;
                            match protocol_msg.msg_type {
                                MessageType::Heartbeat => {
                                    last_heartbeat = std::time::Instant::now();
                                    if let Some(client) = clients.write().await.get_mut(&client_id) {
                                        client.last_heartbeat = Utc::now().to_rfc3339();
                                        // 从心跳 payload 中提取 agentName，确保名称唯一映射
                                        if let Some(payload) = &protocol_msg.payload {
                                            if let Some(name) = payload.get("agentName").and_then(|v| v.as_str()) {
                                                if !name.is_empty() && client.name != name {
                                                    log::info!("{} 名称同步: {} -> {}", conn_tag, client.name, name);
                                                    client.name = name.to_string();
                                                }
                                            }
                                        }
                                    }
                                    let pong = ProtocolMessage::heartbeat_response(&protocol_msg.source);
                                    let _ = ws_sender.send(Message::Text(
                                        serde_json::to_string(&pong).unwrap()
                                    )).await;
                                }
                                MessageType::Command => {
                                    log::info!("{} 命令: {:?}", conn_tag, protocol_msg.method);

                                    // 处理握手命令：提取 Agent 名称
                                    if protocol_msg.method.as_deref() == Some("agent.handshake") {
                                        if let Some(payload) = &protocol_msg.payload {
                                            if let Some(name) = payload.get("agentName").and_then(|v| v.as_str()) {
                                                if !name.is_empty() {
                                                    if let Some(client) = clients.write().await.get_mut(&client_id) {
                                                        if client.name != name {
                                                            log::info!("{} 名称同步（握手）: {} -> {}", conn_tag, client.name, name);
                                                            client.name = name.to_string();
                                                            let _ = event_tx.send(serde_json::json!({
                                                                "event":"agent_name_updated",
                                                                "client_id": client_id,
                                                                "name": name
                                                            }).to_string());
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    let response = ProtocolMessage::success_response(
                                        &protocol_msg,
                                        Some(serde_json::json!({"status":"received"}))
                                    );
                                    let _ = ws_sender.send(Message::Text(
                                        serde_json::to_string(&response).unwrap()
                                    )).await;
                                    let _ = event_tx.send(serde_json::json!({
                                        "event":"message_received", "message":protocol_msg
                                    }).to_string());
                                }
                                _ => {
                                    let _ = event_tx.send(serde_json::json!({
                                        "event":"message_received", "message":protocol_msg
                                    }).to_string());
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(frame))) => {
                        let code_str = frame.as_ref().map(|f| format!("{}", f.code)).unwrap_or_else(|| "1005".into());
                        log::info!("{} 断开 code={}", conn_tag, code_str);
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        last_any_activity = std::time::Instant::now();
                        let _ = ws_sender.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_any_activity = std::time::Instant::now();
                    }
                    Some(Err(e)) => {
                        log::warn!("{} 连接错误: {}", conn_tag, e);
                        break;
                    }
                    None => {
                        // stream 结束等同于连接断开
                        log::info!("{} 连接流结束", conn_tag);
                        break;
                    }
                    _ => {}
                }
            }

            // ============================================================
            // 来自 Tauri 前端的广播消息
            // ============================================================
            result = broadcast_rx.recv() => {
                match result {
                    Ok(json) => {
                        if let Ok(parsed) = serde_json::from_str::<ProtocolMessage>(&json) {
                            if parsed.msg_type != MessageType::Heartbeat {
                                let _ = ws_sender.send(Message::Text(json)).await;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        log::info!("{} 广播通道关闭", conn_tag);
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("{} 广播滞后 {} 条", conn_tag, n);
                    }
                }
            }

            // ============================================================
            // 定向消息（仅发给自己）
            // ============================================================
            result = direct_rx.recv() => {
                match result {
                    Ok((target_id, json)) => {
                        if target_id == client_id {
                            last_any_activity = std::time::Instant::now();
                            let _ = ws_sender.send(Message::Text(json)).await;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        log::info!("{} 定向通道关闭", conn_tag);
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("{} 定向滞后 {} 条", conn_tag, n);
                    }
                }
            }

            // ============================================================
            // 服务器关闭信号
            // ============================================================
            _ = shutdown_rx.recv() => {
                log::info!("{} 服务器关闭", conn_tag);
                let _ = ws_sender.send(Message::Close(None)).await;
                break;
            }

            // ============================================================
            // 每 10s 发送 WebSocket PING 保活
            // ============================================================
            _ = ping_interval.tick() => {
                if ws_sender.send(Message::Ping(b"K".to_vec())).await.is_err() {
                    log::warn!("{} PING 发送失败", conn_tag);
                    break;
                }
            }

            // ============================================================
            // 每秒超时检查
            // ============================================================
            _ = check_interval.tick() => {
                let now = std::time::Instant::now();

                // 应用层心跳超时 45s（从 Agent 收到 heartbeat 算起）
                if now.duration_since(last_heartbeat) > std::time::Duration::from_secs(45) {
                    log::warn!("{} 心跳超时 ({}s)，断开", conn_tag, now.duration_since(last_heartbeat).as_secs());
                    break;
                }

                // 任意活动超时 60s（含 PONG）
                if now.duration_since(last_any_activity) > std::time::Duration::from_secs(60) {
                    log::warn!("{} 活动超时 ({}s)，断开", conn_tag, now.duration_since(last_any_activity).as_secs());
                    break;
                }
            }
        }
    }

    clients.write().await.remove(&client_id);
    let _ = event_tx.send(
        serde_json::json!({
            "event": "agent_disconnected", "client_id": client_id
        })
        .to_string(),
    );
    log::info!("{} 已清理", conn_tag);
}

async fn heartbeat_loop(
    clients: Arc<RwLock<HashMap<String, ConnectedClient>>>,
    status: Arc<RwLock<ServerStatus>>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let current_status = *status.read().await;
                if current_status != ServerStatus::Running {
                    break;
                }
                let count = clients.read().await.len();
                log::debug!("心跳检测: {} 个 Agent 在线", count);
            }
            _ = shutdown_rx.recv() => {
                break;
            }
        }
    }
}

pub fn create_server(port: u16) -> Arc<WsServer> {
    Arc::new(WsServer::new(port))
}
