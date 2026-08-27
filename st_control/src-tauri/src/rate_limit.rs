// Copyright (c) 2026 ST Team - MIT License
// See LICENSE file in the project root for full license information.

// ============================================================
// HTTP API 速率限制（滑动窗口算法）
//
// 设计目标：
//  - 防止 API 被暴力调用（爬虫/脚本滥用）
//  - 按 IP 地址独立计数，支持白名单豁免
//  - 轻量级实现，无外部依赖，适合桌面/私有化部署
//  - 自动清理过期条目，避免内存泄漏
//
// 默认策略：
//  - 60 次/分钟（普通接口）
//  - 10 次/分钟（写入接口，如 SSE 推送）
//  - /health 健康检查豁免限流
// ============================================================

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 滑动窗口速率限制器
pub struct RateLimiter {
    /// IP → 请求时间戳列表
    windows: Mutex<HashMap<String, Vec<Instant>>>,
    /// 窗口大小
    window: Duration,
    /// 每窗口最大请求数
    max_requests: usize,
    /// 上次清理时间
    last_cleanup: Mutex<Instant>,
    /// 清理间隔
    cleanup_interval: Duration,
}

impl RateLimiter {
    /// 创建速率限制器
    ///
    /// - `window_secs`: 滑动窗口大小（秒）
    /// - `max_requests`: 窗口内最大请求数
    pub fn new(window_secs: u64, max_requests: usize) -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            window: Duration::from_secs(window_secs),
            max_requests,
            last_cleanup: Mutex::new(Instant::now()),
            cleanup_interval: Duration::from_secs(60),
        }
    }

    /// 默认 API 限制器：60 次/分钟
    pub fn default_api() -> Self {
        Self::new(60, 60)
    }

    /// 严格限制器：10 次/分钟（用于写入/推送接口）
    pub fn strict() -> Self {
        Self::new(60, 10)
    }

    /// 检查请求是否允许通过
    ///
    /// 返回 `Ok(remaining)` 表示允许（remaining = 剩余配额），
    /// `Err(retry_after)` 表示被拒绝（retry_after = 建议重试秒数）。
    pub fn check(&self, ip: &str) -> Result<usize, u64> {
        let now = Instant::now();
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());

        // 定期清理过期条目，避免内存泄漏
        self.maybe_cleanup(&mut windows, now);

        let timestamps = windows.entry(ip.to_string()).or_default();

        // 移除窗口外的过期时间戳
        let cutoff = now - self.window;
        timestamps.retain(|&t| t > cutoff);

        if timestamps.len() >= self.max_requests {
            // 超限：计算最早过期时间
            let oldest = timestamps.first().copied().unwrap_or(now);
            let retry_after = (self.window.as_secs())
                .saturating_sub(now.duration_since(oldest).as_secs())
                .max(1);
            return Err(retry_after);
        }

        timestamps.push(now);
        Ok(self.max_requests - timestamps.len())
    }

    /// 清理过期 IP 条目（超过 2 个窗口无活动的 IP 移除）
    fn maybe_cleanup(&self, windows: &mut HashMap<String, Vec<Instant>>, now: Instant) {
        let mut last = self.last_cleanup.lock().unwrap_or_else(|e| e.into_inner());
        if now.duration_since(*last) < self.cleanup_interval {
            return;
        }
        *last = now;

        let stale_cutoff = now - self.window * 2;
        windows.retain(|_, timestamps| {
            timestamps.retain(|&t| t > stale_cutoff);
            !timestamps.is_empty()
        });
    }

    /// 当前跟踪的 IP 数量（监控用）
    pub fn tracked_ips(&self) -> usize {
        let windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        windows.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_basic() {
        let limiter = RateLimiter::new(60, 3);
        assert!(limiter.check("127.0.0.1").is_ok());
        assert!(limiter.check("127.0.0.1").is_ok());
        assert!(limiter.check("127.0.0.1").is_ok());
        // 第 4 次应被拒绝
        assert!(limiter.check("127.0.0.1").is_err());
    }

    #[test]
    fn test_rate_limiter_per_ip() {
        let limiter = RateLimiter::new(60, 2);
        assert!(limiter.check("10.0.0.1").is_ok());
        assert!(limiter.check("10.0.0.2").is_ok());
        assert!(limiter.check("10.0.0.1").is_ok());
        assert!(limiter.check("10.0.0.2").is_ok());
        // 两个 IP 各自独立
        assert!(limiter.check("10.0.0.1").is_err());
        assert!(limiter.check("10.0.0.2").is_err());
    }

    #[test]
    fn test_rate_limiter_remaining() {
        let limiter = RateLimiter::new(60, 5);
        let r1 = limiter.check("192.168.1.1").unwrap();
        assert_eq!(r1, 4);
        let r2 = limiter.check("192.168.1.1").unwrap();
        assert_eq!(r2, 3);
    }

    #[test]
    fn test_tracked_ips() {
        let limiter = RateLimiter::new(60, 10);
        limiter.check("1.1.1.1").ok();
        limiter.check("2.2.2.2").ok();
        assert_eq!(limiter.tracked_ips(), 2);
    }
}
