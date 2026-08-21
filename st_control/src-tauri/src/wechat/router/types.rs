// ============================================================
// 微信消息实时推送路由器 — 数据类型
// 自 router.rs 拆分：待发送消息/未确认元数据/监控指标。
// ============================================================

use tokio::time::Instant;

/// 单条待发送消息
#[derive(Clone)]
pub(crate) struct MessageItem {
    pub(crate) ack_id: u64,
    pub(crate) text: String,
    pub(crate) payload: serde_json::Value,
    /// 当前重传次数（首次发送为 0）
    pub(crate) retries: u8,
}

/// 未确认消息元数据
pub(crate) struct PendingAck {
    pub(crate) ts: Instant,
    pub(crate) text: String,
    pub(crate) retries: u8,
}

/// 监控指标
#[derive(Default)]
pub struct Metrics {
    pub pending_acks: usize,
    pub sent_total: u64,
    pub sent_batch_count: u64,
    pub sent_ws_count: u64,
    pub latency_ms_sum: u64,
    pub latency_ms_count: u64,
    /// 延迟分桶：[<50ms, <200ms, <500ms, <1000ms, >=1000ms]
    pub latency_buckets: [u64; 5],
}

impl Metrics {
    pub(crate) fn record_latency(&mut self, ms: u64) {
        self.latency_ms_sum += ms;
        self.latency_ms_count += 1;
        let bucket = match ms {
            0..50 => 0,
            50..200 => 1,
            200..500 => 2,
            500..1000 => 3,
            _ => 4,
        };
        self.latency_buckets[bucket] += 1;
    }
}

impl Clone for Metrics {
    fn clone(&self) -> Self {
        Self {
            pending_acks: self.pending_acks,
            sent_total: self.sent_total,
            sent_batch_count: self.sent_batch_count,
            sent_ws_count: self.sent_ws_count,
            latency_ms_sum: self.latency_ms_sum,
            latency_ms_count: self.latency_ms_count,
            latency_buckets: self.latency_buckets,
        }
    }
}
