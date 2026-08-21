// ============================================================
// 微信消息实时推送路由器 —— EventRouter
// ============================================================
// 架构文档章节：3.2 通信方案（推荐 Tauri Event + WebSocket 双通道）
// 职责：
//   1. 默认通过 Tauri Event 向前端推送消息
//   2. Tauri Event 失败或队列积压时回退到 WebSocket
//   3. 为每条消息生成 ack_id，跟踪前端 ACK，实现至少一次语义
//   4. 背压保护：未确认消息超过阈值时暂停推送
//   5. 批量压缩：单条消息进入 50ms/32 条微批窗口，降低 IPC 频率
//   6. WebSocket 精确重传：超时未确认的消息自动重发
// 边界条件：
//   - 前端未连接时消息进入广播通道，由前端订阅后补发
//   - WebSocket 端口占用时自动尝试下一个端口
//   - ack_id 仅用于去重与可靠性，不影响业务排序
//   - 重传次数超过 MAX_RETRY_COUNT 后丢弃，避免死循环
// ============================================================

mod batch;
mod retry;
mod types;
pub use types::Metrics;
use types::{MessageItem, PendingAck};
mod ws;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tauri::Emitter;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};

use crate::wechat::handlers::helpers;

/// 背压阈值：未确认消息超过该值时暂停产生新消息
const BACKPRESSURE_LIMIT: usize = 1024;

/// 断线补推缓冲容量：保留最近 N 条已广播消息，供前端重连/页面恢复后按 ack_id 补拉
const REPLAY_CAPACITY: usize = 1000;

/// 事件路由器
pub struct EventRouter {
    app_handle: tauri::AppHandle,
    /// 内部广播通道，容量 8192，供未来扩展或遗留任务消费
    broadcast_tx: broadcast::Sender<String>,
    /// WebSocket 客户端发送端集合
    ws_clients: Arc<RwLock<Vec<mpsc::Sender<String>>>>,
    /// ack_id -> 未确认消息元数据，用于去重、超时清理与重传
    pending_acks: Arc<Mutex<HashMap<u64, PendingAck>>>,
    /// ack_id 序列号
    seq: AtomicU64,
    /// WebSocket 监听端口（0 表示未启动）
    ws_port: AtomicU64,
    /// 批量发送入口
    batch_tx: mpsc::Sender<MessageItem>,
    /// 监控指标
    metrics: Arc<Mutex<Metrics>>,
    /// 断线补推环形缓冲：(ack_id, 单条消息文本)，按 ack_id 单调递增
    replay: Arc<Mutex<std::collections::VecDeque<(u64, String)>>>,
}

impl EventRouter {
    pub fn new(app_handle: tauri::AppHandle, broadcast_tx: broadcast::Sender<String>) -> Arc<Self> {
        let (batch_tx, batch_rx) = mpsc::channel::<MessageItem>(4096);
        let router = Arc::new(Self {
            app_handle,
            broadcast_tx,
            ws_clients: Arc::new(RwLock::new(Vec::new())),
            pending_acks: Arc::new(Mutex::new(HashMap::new())),
            seq: AtomicU64::new(1),
            ws_port: AtomicU64::new(0),
            batch_tx,
            metrics: Arc::new(Mutex::new(Metrics::default())),
            replay: Arc::new(Mutex::new(std::collections::VecDeque::with_capacity(
                REPLAY_CAPACITY,
            ))),
        });

        // 启动批量聚合任务
        let batch_router = router.clone();
        tokio::spawn(async move {
            batch_router.batch_loop(batch_rx).await;
        });

        // 启动超时清理与重传任务
        let retry_router = router.clone();
        tokio::spawn(async move {
            retry_router.retry_loop().await;
        });

        router
    }

    /// 广播一条消息到前端
    ///
    /// 流程：
    ///   1. 注入 ack_id / channel 元数据
    ///   2. 写入本地环形日志
    ///   3. 进入批量发送队列，由 batch_loop 统一聚合/分发
    pub async fn broadcast(&self, mut payload: serde_json::Value) {
        let ack_id = self.next_ack_id();
        payload["ack_id"] = serde_json::Value::String(ack_id.to_string());
        payload["channel"] = serde_json::Value::String("event".to_string());
        payload["ts_backend"] = serde_json::Value::Number(serde_json::Number::from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        ));

        let text = match serde_json::to_string(&payload) {
            Ok(s) => s,
            Err(e) => {
                log::error!("[router] 序列化消息失败: {}", e);
                return;
            }
        };

        // 内部广播通道：即使前端未连接也保留，便于重连后补消费
        let _ = self.broadcast_tx.send(text.clone());

        // 写入断线补推缓冲：无论是否被背压丢弃，消息都保留在缓冲中，
        // 前端可通过 resync_wechat_messages 按 ack_id 补拉遗漏部分。
        {
            let mut rb = self.replay.lock().await;
            rb.push_back((ack_id, text.clone()));
            while rb.len() > REPLAY_CAPACITY {
                rb.pop_front();
            }
        }

        // 写入全局日志
        helpers::push_wechat_message(payload.clone());

        // 背压检查：超过阈值时直接丢弃，避免压垮前端
        let pending_count = self.pending_acks.lock().await.len();
        if pending_count > BACKPRESSURE_LIMIT {
            log::warn!(
                "[router] 未确认消息 {} 超过阈值 {}，本次消息仅入广播通道，避免压垮前端",
                pending_count,
                BACKPRESSURE_LIMIT
            );
            return;
        }

        let item = MessageItem {
            ack_id,
            text,
            payload,
            retries: 0,
        };
        if let Err(e) = self.batch_tx.send(item).await {
            log::warn!("[router] 批量队列已关闭，消息无法发送: {}", e);
        }
    }

    /// 前端确认收到消息
    pub async fn ack(&self, ack_id: u64) {
        self.pending_acks.lock().await.remove(&ack_id);
    }

    /// 获取当前未确认消息数（用于状态上报）
    pub async fn pending_count(&self) -> usize {
        self.pending_acks.lock().await.len()
    }

    /// 获取监控指标快照
    pub async fn metrics(&self) -> Metrics {
        let mut m = self.metrics.lock().await.clone();
        m.pending_acks = self.pending_acks.lock().await.len();
        m
    }

    /// 返回 ack_id 大于 `since` 的全部缓冲消息文本（用于断线重连补推）
    ///
    /// ack_id 由单调递增序列生成；缓冲外的过旧消息已被淘汰，调用方得到空 Vec。
    pub async fn replay_since(&self, since: u64) -> Vec<String> {
        self.replay
            .lock()
            .await
            .iter()
            .filter(|(id, _)| *id > since)
            .map(|(_, text)| text.clone())
            .collect()
    }

    /// 订阅实时消息广播（HTTP API SSE 推送通道）
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.broadcast_tx.subscribe()
    }

    /// 推送监控运行状态到前端（wechat-status 事件）
    ///
    /// 用于心跳上报、背压告警、任务退出通知等场景；
    /// 前端看门狗据此判断监控任务存活并在异常时自动重启。
    pub async fn emit_status(&self, status: &str) {
        let m = self.metrics().await;
        let payload = serde_json::json!({
            "running": true,
            "status": status,
            "ws_port": self.ws_port(),
            "pending_acks": m.pending_acks,
            "sent_total": m.sent_total,
            "sent_batch_count": m.sent_batch_count,
            "sent_ws_count": m.sent_ws_count,
            "latency": {
                "buckets": m.latency_buckets,
                "sum_ms": m.latency_ms_sum,
                "count": m.latency_ms_count,
            },
        });
        if let Err(e) = self.app_handle.emit("wechat-status", payload) {
            log::debug!("[router] 状态上报失败 ({}): {}", status, e);
        }
    }

    fn next_ack_id(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }
}
