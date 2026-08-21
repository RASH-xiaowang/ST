// ============================================================
// 微信消息实时推送路由器 — WebSocket 服务器域
// 自 router.rs 拆分：回退通道监听/客户端管理/文本广播。
// ============================================================

use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message;

use super::EventRouter;

/// 默认 WebSocket 起始端口
const DEFAULT_WS_PORT: u16 = 56789;
/// WebSocket 最大客户端数
const MAX_WS_CLIENTS: usize = 4;

impl EventRouter {
    /// 启动 WebSocket 服务器
    ///
    /// 会尝试绑定 DEFAULT_WS_PORT，若被占用则尝试后续 10 个端口
    pub async fn start_ws_server(self: &Arc<Self>) -> Result<u16, String> {
        if self.ws_port.load(Ordering::SeqCst) != 0 {
            return Ok(self.ws_port.load(Ordering::SeqCst) as u16);
        }

        let (port, listener) = self.try_bind(DEFAULT_WS_PORT).await?;
        self.ws_port.store(port as u64, Ordering::SeqCst);
        log::info!("[router] WebSocket 服务器监听 127.0.0.1:{}", port);

        let router = self.clone();
        tokio::spawn(async move {
            router.accept_ws_loop(listener).await;
        });

        Ok(port)
    }

    /// 停止 WebSocket 服务器（通过关闭底层 listener 在 drop 时自动完成）
    pub fn stop_ws_server(&self) {
        self.ws_port.store(0, Ordering::SeqCst);
        // 注：tokio::net::TcpListener 无显式 close，listener 随任务句柄被 abort 后 drop
    }

    /// 获取 WebSocket 端口，0 表示未启动
    pub fn ws_port(&self) -> u16 {
        self.ws_port.load(Ordering::SeqCst) as u16
    }

    pub(crate) async fn broadcast_ws(&self, text: String) {
        // 克隆发送端后尽快释放读锁，避免发送阻塞时长时间占用客户端列表
        let clients: Vec<mpsc::Sender<String>> =
            self.ws_clients.read().await.iter().cloned().collect();
        if clients.is_empty() {
            log::debug!("[router] 无 WebSocket 客户端，消息仅保留在广播通道");
            return;
        }
        for tx in clients {
            let _ = tx.send(text.clone()).await;
        }
    }

    async fn try_bind(&self, start_port: u16) -> Result<(u16, TcpListener), String> {
        for port in start_port..start_port.saturating_add(10) {
            let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
            match TcpListener::bind(addr).await {
                Ok(listener) => return Ok((port, listener)),
                Err(e) => {
                    log::debug!("[router] 端口 {} 绑定失败: {}", port, e);
                }
            }
        }
        Err("无法绑定 WebSocket 端口".to_string())
    }

    async fn accept_ws_loop(self: Arc<Self>, listener: TcpListener) {
        while self.ws_port.load(Ordering::SeqCst) != 0 {
            tokio::select! {
                accept = listener.accept() => {
                    match accept {
                        Ok((stream, peer)) => {
                            let clients = self.ws_clients.read().await;
                            if clients.len() >= MAX_WS_CLIENTS {
                                log::warn!("[router] WebSocket 客户端数达上限，拒绝 {}", peer);
                                drop(clients);
                                continue;
                            }
                            drop(clients);
                            let router = self.clone();
                            tokio::spawn(async move {
                                router.handle_ws_client(stream, peer).await;
                            });
                        }
                        Err(e) => {
                            log::error!("[router] WebSocket accept 失败: {}", e);
                        }
                    }
                }
            }
        }

        log::info!("[router] WebSocket 服务器已停止");
    }

    async fn handle_ws_client(self: Arc<Self>, stream: TcpStream, peer: SocketAddr) {
        let ws = match tokio_tungstenite::accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                log::warn!("[router] WebSocket 握手失败 {}: {}", peer, e);
                return;
            }
        };

        let (mut ws_tx, mut ws_rx) = ws.split();
        let (text_tx, mut text_rx) = mpsc::channel::<String>(256);

        // 注册客户端
        self.ws_clients.write().await.push(text_tx.clone());
        log::info!("[router] WebSocket 客户端已连接: {}", peer);

        // 发送任务：将本地 mpsc 中的文本转发到 WebSocket
        let forward_handle = tokio::spawn(async move {
            while let Some(text) = text_rx.recv().await {
                if ws_tx.send(Message::Text(text)).await.is_err() {
                    break;
                }
            }
        });

        // 接收任务：处理前端 ACK
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(ack_id) = val.get("ack_id").and_then(|v| v.as_str()) {
                            if let Ok(id) = ack_id.parse::<u64>() {
                                self.ack(id).await;
                            }
                        }
                        // 支持批量 ACK：{ "ack_ids": ["1","2"] }
                        if let Some(ids) = val.get("ack_ids").and_then(|v| v.as_array()) {
                            for id_val in ids {
                                if let Some(ack_id) = id_val.as_str() {
                                    if let Ok(id) = ack_id.parse::<u64>() {
                                        self.ack(id).await;
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }

        // 注销客户端
        forward_handle.abort();
        let mut clients = self.ws_clients.write().await;
        clients.retain(|tx| !tx.same_channel(&text_tx));
        log::info!("[router] WebSocket 客户端已断开: {}", peer);
    }
}
