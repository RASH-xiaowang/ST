// ============================================================
// 微信消息实时推送路由器 — 批量聚合与分发域
// 自 router.rs 拆分：微批窗口/事件分发/WebSocket 回退/ACK 跟踪。
// ============================================================

use std::sync::Arc;

use tauri::Emitter;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

use super::types::{MessageItem, PendingAck};
use super::EventRouter;

/// 批量聚合尾沿窗口：每条消息到达都重置该窗口；
/// 单条消息最多等待该时长即发出（趋近实时），连续突发则合并至停顿或达上限才发出。
const BATCH_MAX_WAIT_MS: u64 = 5;
/// 批量窗口：最多累积 32 条消息
const BATCH_MAX_SIZE: usize = 32;
/// 批量刷新：空闲 10ms 且累积 >0 时立即发送，降低单条消息延迟
const BATCH_FLUSH_IDLE_MS: u64 = 10;

impl EventRouter {
    async fn track_ack(&self, ack_id: u64, text: String, retries: u8) {
        self.pending_acks.lock().await.insert(
            ack_id,
            PendingAck {
                ts: Instant::now(),
                text,
                retries,
            },
        );
    }

    async fn track_acks(&self, items: &[MessageItem]) {
        let mut pending = self.pending_acks.lock().await;
        for item in items {
            pending.insert(
                item.ack_id,
                PendingAck {
                    ts: Instant::now(),
                    text: item.text.clone(),
                    retries: item.retries,
                },
            );
        }
    }

    /// 批量聚合发送循环
    ///
    /// 策略：
    ///   - 累积 BATCH_MAX_SIZE 条立即 flush
    ///   - 或等待 BATCH_MAX_WAIT_MS 后 flush
    ///   - 空闲 BATCH_FLUSH_IDLE_MS 且累积 >0 时立即 flush，降低单条延迟
    pub(crate) async fn batch_loop(self: Arc<Self>, mut rx: mpsc::Receiver<MessageItem>) {
        let mut buffer: Vec<MessageItem> = Vec::with_capacity(BATCH_MAX_SIZE);
        let mut deadline: Option<Instant> = None;

        loop {
            let timeout = if buffer.is_empty() {
                None
            } else if let Some(d) = deadline {
                Some(tokio::time::sleep_until(d))
            } else {
                Some(tokio::time::sleep(Duration::from_millis(
                    BATCH_FLUSH_IDLE_MS,
                )))
            };

            tokio::select! {
                biased;
                Some(item) = rx.recv() => {
                    buffer.push(item);
                    if buffer.len() >= BATCH_MAX_SIZE {
                        let batch = std::mem::replace(&mut buffer, Vec::with_capacity(BATCH_MAX_SIZE));
                        self.dispatch_batch(batch).await;
                        deadline = None;
                    } else {
                        // 尾沿聚合：每条到达都重置极短窗口。
                        // 单条消息 ≤ BATCH_MAX_WAIT_MS 即发出；连续突发则合并至
                        // 出现 ≥BATCH_MAX_WAIT_MS 的停顿或累积达上限才批量发出。
                        deadline = Some(Instant::now() + Duration::from_millis(BATCH_MAX_WAIT_MS));
                    }
                }
                _ = async {
                    if let Some(t) = timeout {
                        t.await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                }, if !buffer.is_empty() => {
                    let batch = std::mem::replace(&mut buffer, Vec::with_capacity(BATCH_MAX_SIZE));
                    self.dispatch_batch(batch).await;
                    deadline = None;
                }
                else => {
                    // 通道关闭且缓冲区为空时退出
                    if buffer.is_empty() {
                        break;
                    }
                    let batch = std::mem::replace(&mut buffer, Vec::with_capacity(BATCH_MAX_SIZE));
                    self.dispatch_batch(batch).await;
                    break;
                }
            }
        }

        log::info!("[router] 批量发送循环已退出");
    }

    /// 发送批量消息
    ///
    /// 优先使用 Tauri Event；失败时回退 WebSocket。
    /// 单条消息时保持原有格式，多条时打包为 batch 数组。
    async fn dispatch_batch(&self, items: Vec<MessageItem>) {
        if items.is_empty() {
            return;
        }

        // 单条消息保持向后兼容的单对象格式
        if items.len() == 1 {
            self.dispatch_single(items.into_iter().next().unwrap())
                .await;
            return;
        }

        let ack_ids: Vec<u64> = items.iter().map(|i| i.ack_id).collect();
        let messages: Vec<serde_json::Value> = items.iter().map(|i| i.payload.clone()).collect();
        let batch_payload = serde_json::json!({
            "batch": true,
            "messages": messages,
            "ack_ids": ack_ids,
        });

        let text = match serde_json::to_string(&batch_payload) {
            Ok(s) => s,
            Err(e) => {
                log::error!("[router] 批量序列化失败: {}", e);
                return;
            }
        };

        // 记录发送指标
        self.record_batch_latency(&items).await;

        let delivered = match self.app_handle.emit("wechat-message", &text) {
            Ok(_) => true,
            Err(e) => {
                log::warn!("[router] Tauri Event 批量推送失败，回退 WebSocket: {}", e);
                self.broadcast_ws(text.clone()).await;
                false
            }
        };

        if delivered {
            self.track_acks(&items).await;
            let mut m = self.metrics.lock().await;
            m.sent_total += items.len() as u64;
            m.sent_batch_count += 1;
        } else {
            // WebSocket 回退不等待 ACK，不加入 pending（避免重复计数）
            let mut m = self.metrics.lock().await;
            m.sent_ws_count += items.len() as u64;
        }
    }

    /// 发送单条消息
    async fn dispatch_single(&self, item: MessageItem) {
        let ack_id = item.ack_id;
        self.record_single_latency(&item).await;

        let delivered = match self.app_handle.emit("wechat-message", &item.text) {
            Ok(_) => true,
            Err(e) => {
                log::warn!("[router] Tauri Event 推送失败，回退 WebSocket: {}", e);
                self.broadcast_ws(item.text.clone()).await;
                false
            }
        };

        if delivered {
            self.track_ack(ack_id, item.text, item.retries).await;
            let mut m = self.metrics.lock().await;
            m.sent_total += 1;
        } else {
            let mut m = self.metrics.lock().await;
            m.sent_ws_count += 1;
        }
    }

    async fn record_single_latency(&self, item: &MessageItem) {
        if let Some(ts_backend) = item.payload.get("ts_backend").and_then(|v| v.as_i64()) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let delay = now.saturating_sub(ts_backend).max(0) as u64;
            if delay > 500 {
                log::warn!(
                    "[router] 消息端到端延迟 {}ms (ack_id={})",
                    delay,
                    item.ack_id
                );
            } else {
                log::debug!(
                    "[router] 消息端到端延迟 {}ms (ack_id={})",
                    delay,
                    item.ack_id
                );
            }
        }
    }

    async fn record_batch_latency(&self, items: &[MessageItem]) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let mut m = self.metrics.lock().await;
        for item in items {
            if let Some(ts_backend) = item.payload.get("ts_backend").and_then(|v| v.as_i64()) {
                let delay = now.saturating_sub(ts_backend).max(0) as u64;
                m.record_latency(delay);
            }
        }
    }
}
