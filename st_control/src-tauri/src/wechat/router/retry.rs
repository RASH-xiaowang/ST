// ============================================================
// 微信消息实时推送路由器 — 超时重传域
// 自 router.rs 拆分：未确认消息清理与重传循环。
// ============================================================

use std::sync::Arc;

use tokio::time::{Duration, Instant};

use super::types::MessageItem;
use super::EventRouter;

/// 未确认消息保留超时
const ACK_TIMEOUT: Duration = Duration::from_secs(30);
/// 最大重传次数
const MAX_RETRY_COUNT: u8 = 3;

impl EventRouter {
    /// 超时清理与重传循环
    ///
    /// 每 ACK_TIMEOUT 检查一次 pending_acks：
    ///   - 未超时：保留
    ///   - 已超时且 retries < MAX_RETRY_COUNT：重新放入 batch_tx 重传
    ///   - 已超时且 retries >= MAX_RETRY_COUNT：丢弃
    pub(crate) async fn retry_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(ACK_TIMEOUT);
        loop {
            interval.tick().await;

            let now = Instant::now();
            let mut pending = self.pending_acks.lock().await;
            let mut to_retry: Vec<MessageItem> = Vec::new();
            let mut dropped = 0usize;

            pending.retain(|ack_id, meta| {
                if now.duration_since(meta.ts) < ACK_TIMEOUT {
                    return true;
                }
                if meta.retries >= MAX_RETRY_COUNT {
                    dropped += 1;
                    return false;
                }
                // 重传：保留 ack_id，复用原 payload（text 中 channel 为 event，重传时不变）
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&meta.text) {
                    let retries = meta.retries.saturating_add(1);
                    to_retry.push(MessageItem {
                        ack_id: *ack_id,
                        text: meta.text.clone(),
                        payload,
                        retries,
                    });
                }
                false
            });

            // 记录重传次数（重传消息重新进入 pending 时 retries 会被重置为 0，
            // 因此这里先把原 retries 透传，避免被立即再次判定超时）
            drop(pending);
            for item in &mut to_retry {
                if let Err(e) = self.batch_tx.send(item.clone()).await {
                    log::warn!("[router] 重传消息进入批量队列失败: {}", e);
                }
            }

            if !to_retry.is_empty() {
                log::warn!("[router] {} 条消息超时未确认，触发重传", to_retry.len());
            }
            if dropped > 0 {
                log::warn!("[router] {} 条消息超过最大重传次数，已丢弃", dropped);
            }
        }
    }
}
