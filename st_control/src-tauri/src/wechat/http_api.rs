//! 微信数据 HTTP API 服务（127.0.0.1 本地监听）
//!
//! 参考 WeFlow HTTP API 设计并做了如下增强：
//! - 健康检查携带监控/数据库/版本状态
//! - 媒体接口**按需即时解密**图片（含 wxgf/HEVC 转码），无需预先导出
//! - SSE 推送支持 `Last-Event-ID` / `since_ack` 断线补推（基于 replay 缓冲）
//! - 统一错误格式 `{success:false,error:{code,message}}` + 正确 HTTP 状态码
//! - 监控状态接口暴露 ACK 积压、推送计数等运行指标
//!
//! 鉴权：config.json 配置 `api_token` 后生效；支持三种传参方式：
//!   1. Header: `Authorization: Bearer <token>`（推荐）
//!   2. Query:  `?access_token=<token>`（SSE 推荐）
//!   3. Body:   `{"access_token": "<token>"}`（POST）
//!
//! 未配置 token 时免鉴权（仅建议本机使用）。

use axum::{
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::wechat::config::WeChatConfig;
use crate::wechat::handlers::WeChatMonitorState;

mod automation;
pub(crate) use automation::*;
mod status;
pub(crate) use status::*;
mod query;
pub(crate) use query::*;
mod media;
pub(crate) use media::*;

// ============ 服务状态 ============

/// 响应缓存条目
struct ApiCacheEntry {
    at: Instant,
    body: serde_json::Value,
}

/// 缓存容量上限（超出后整体清空，简单 LRU 兜底）
const API_CACHE_MAX_ENTRIES: usize = 256;

pub struct ApiServerState {
    pub monitor: Arc<WeChatMonitorState>,
    /// 访问令牌（运行时可热更新）
    token: std::sync::RwLock<Option<String>>,
    /// 是否启用（false 时所有 /api/* 返回 503）
    enabled: std::sync::atomic::AtomicBool,
    /// 当前监听端口
    port: std::sync::atomic::AtomicU16,
    pub started: Instant,
    /// 优雅关闭信号（端口变更时触发重启）
    shutdown_tx: std::sync::Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    /// 数据查询响应缓存（会话/消息/通讯录/群成员，TTL 内命中免重复查询）
    response_cache: std::sync::Mutex<HashMap<String, ApiCacheEntry>>,
}

impl ApiServerState {
    pub fn new(
        monitor: Arc<WeChatMonitorState>,
        token: Option<String>,
        port: u16,
        enabled: bool,
    ) -> Self {
        Self {
            monitor,
            token: std::sync::RwLock::new(token),
            enabled: std::sync::atomic::AtomicBool::new(enabled),
            port: std::sync::atomic::AtomicU16::new(port),
            started: Instant::now(),
            shutdown_tx: std::sync::Mutex::new(None),
            response_cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    fn lock_token<'a>(&'a self) -> std::sync::RwLockReadGuard<'a, Option<String>> {
        self.token.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn current_token(&self) -> Option<String> {
        self.lock_token().clone()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn current_port(&self) -> u16 {
        self.port.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 应用新设置；端口变化时优雅重启监听。
    /// 返回是否需要重启监听。
    pub fn apply_settings(&self, enabled: bool, port: u16, token: Option<String>) {
        *self.token.write().unwrap_or_else(|e| e.into_inner()) = token;
        self.enabled
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
        let old_port = self.port.swap(port, std::sync::atomic::Ordering::Relaxed);
        if old_port != port {
            // 触发优雅关闭，由外部重新拉起 serve
            if let Some(tx) = self
                .shutdown_tx
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
            {
                let _ = tx.send(true);
            }
        }
    }

    fn install_shutdown(&self, tx: tokio::sync::watch::Sender<bool>) {
        *self.shutdown_tx.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    }

    /// 读取未过期的缓存响应；未命中/已过期返回 None
    fn cached(&self, key: &str, ttl: Duration) -> Option<Json<serde_json::Value>> {
        let cache = self
            .response_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let entry = cache.get(key)?;
        if entry.at.elapsed() > ttl {
            None
        } else {
            Some(Json(entry.body.clone()))
        }
    }

    /// 写入响应缓存（容量上限兜底）
    fn store(&self, key: &str, body: serde_json::Value) {
        let mut cache = self
            .response_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if cache.len() >= API_CACHE_MAX_ENTRIES {
            cache.clear();
        }
        cache.insert(
            key.to_string(),
            ApiCacheEntry {
                at: Instant::now(),
                body,
            },
        );
    }
}

/// 生成规范化缓存 key：path + 排序后的 query 参数
fn cache_key(path: &str, params: &HashMap<String, String>) -> String {
    let mut parts: Vec<(&String, &String)> = params.iter().collect();
    parts.sort_by(|a, b| a.0.cmp(b.0));
    let mut key = String::with_capacity(path.len() + parts.len() * 16);
    key.push_str(path);
    for (k, v) in parts {
        key.push('&');
        key.push_str(k);
        key.push('=');
        key.push_str(v);
    }
    key
}

// ============ 统一错误 ============

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
    fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "BAD_REQUEST", msg)
    }
    fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "缺少或无效的 access_token",
        )
    }
    fn not_found(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "NOT_FOUND", msg)
    }
    fn internal(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg)
    }
    fn config(e: std::io::Error) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CONFIG_NOT_FOUND",
            e.to_string(),
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "success": false,
            "error": { "code": self.code, "message": self.message },
        });
        (self.status, Json(body)).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

// ============ 鉴权 ============

fn check_auth(
    state: &ApiServerState,
    headers: &HeaderMap,
    query: &HashMap<String, String>,
    body: Option<&serde_json::Value>,
) -> ApiResult<()> {
    // API 总开关（/health 不经过此检查，始终可用）
    if !state.is_enabled() {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "API_DISABLED",
            "HTTP API 未启用，请在设置中开启",
        ));
    }
    let binding = state.lock_token();
    let expected = match binding.as_ref() {
        Some(t) if !t.is_empty() => t,
        _ => return Ok(()), // 未配置 token → 免鉴权
    };
    // 1. Authorization: Bearer
    if let Some(v) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(tok) = v.strip_prefix("Bearer ") {
            if tok == expected {
                return Ok(());
            }
        }
    }
    // 2. Query access_token
    if query.get("access_token") == Some(expected) {
        return Ok(());
    }
    // 3. Body access_token
    if let Some(b) = body {
        if b.get("access_token").and_then(|v| v.as_str()) == Some(expected.as_str()) {
            return Ok(());
        }
    }
    Err(ApiError::unauthorized())
}

// ============ 参数工具 ============

fn parse_i64(q: &HashMap<String, String>, key: &str, default: i64) -> i64 {
    q.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn parse_usize(q: &HashMap<String, String>, key: &str, default: usize, max: usize) -> usize {
    q.get(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
        .min(max)
}

/// start/end 时间参数：支持秒级时间戳、毫秒时间戳、YYYYMMDD
fn parse_time(q: &HashMap<String, String>, key: &str) -> Option<i64> {
    let v = q.get(key)?.trim().to_string();
    if v.is_empty() {
        return None;
    }
    if v.len() == 8 && v.chars().all(|c| c.is_ascii_digit()) {
        let y: i64 = v[0..4].parse().ok()?;
        let m: i64 = v[4..6].parse().ok()?;
        let d: i64 = v[6..8].parse().ok()?;
        let days = days_from_civil(y, m as u32, d as u32);
        let base = days * 86400;
        // end 的 YYYYMMDD 扩展到当天 23:59:59
        return Some(if key == "end" { base + 86399 } else { base });
    }
    let mut ts: i64 = v.parse().ok()?;
    if ts > 10_000_000_000 {
        ts /= 1000; // 毫秒时间戳
    }
    Some(ts)
}

/// 公历转 days since epoch（Howard Hinnant 算法）
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn load_cfg() -> ApiResult<WeChatConfig> {
    WeChatConfig::load().map_err(ApiError::config)
}

// ============ 路由 ============

pub fn build_router(state: Arc<ApiServerState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/health", get(health))
        .route("/api/v1/sessions", get(get_sessions).post(get_sessions))
        .route("/api/v1/messages", get(get_messages).post(get_messages))
        .route("/api/v1/sessions/{id}/messages", get(get_session_messages))
        .route("/api/v1/contacts", get(get_contacts).post(get_contacts))
        .route(
            "/api/v1/group-members",
            get(get_group_members).post(get_group_members),
        )
        .route("/api/v1/media/{username}/{local_id}", get(get_media))
        .route(
            "/api/v1/media/video/{username}/{local_id}",
            get(get_media_video),
        )
        .route(
            "/api/v1/media/video/thumb/{username}/{local_id}",
            get(get_media_video_thumb),
        )
        .route("/api/v1/sns/video/{file_key}", get(get_sns_video))
        .route("/api/v1/emoticon/{md5}", get(get_emoticon_image))
        .route("/api/v1/file/image/{md5}", get(get_file_image))
        .route("/api/v1/file/video/{md5}", get(get_file_video))
        .route("/api/v1/file/video/thumb/{md5}", get(get_file_video_thumb))
        .route("/api/v1/monitor/status", get(monitor_status))
        .route("/api/v1/push/messages", get(push_messages))
        .route("/api/v1/automation/tasks", get(automation_tasks))
        .route(
            "/api/v1/automation/tasks/claim",
            post(automation_task_claim),
        )
        .route(
            "/api/v1/automation/tasks/start",
            post(automation_task_start),
        )
        .route(
            "/api/v1/automation/tasks/complete",
            post(automation_task_complete),
        )
        .route("/api/v1/openapi.json", get(openapi_json))
        // 仅放行应用自身来源（WebView2 dev/prod），拒绝任意站点跨域读取本地数据
        .layer(app_cors_layer())
        .with_state(state)
}

/// 受限 CORS：仅允许应用自身 Origin，其余跨域请求不返回 CORS 头
fn app_cors_layer() -> tower_http::cors::CorsLayer {
    use tower_http::cors::AllowOrigin;
    let app_origins = [
        "http://localhost:1420",
        "http://127.0.0.1:1420",
        "tauri://localhost",
        "http://tauri.localhost",
    ];
    tower_http::cors::CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _req| {
            let o = origin.as_bytes();
            o.is_empty() || app_origins.iter().any(|a| o == a.as_bytes())
        }))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
        ])
}

/// 启动 HTTP API 服务（在 Tauri setup 中 spawn）
///
/// 支持端口热更新：apply_settings 检测到端口变化时发送优雅关闭信号，
/// 本循环收到信号后按新端口重新绑定，连接平滑迁移。
pub async fn serve(state: Arc<ApiServerState>) {
    loop {
        let port = state.current_port();
        let addr = format!("127.0.0.1:{}", port);
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                log::error!("[http-api] 绑定 {} 失败: {}", addr, e);
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }
        };
        log::info!("[http-api] HTTP API 服务已启动: http://{}", addr);

        let (tx, mut rx) = tokio::sync::watch::channel(false);
        state.install_shutdown(tx);
        let result = axum::serve(listener, build_router(state.clone()))
            .with_graceful_shutdown(async move {
                let _ = rx.changed().await;
            })
            .await;
        if let Err(e) = result {
            log::error!("[http-api] 服务异常退出: {}", e);
            return;
        }
        // 优雅关闭完成（端口变更触发）：按最新端口重新绑定
        log::info!("[http-api] 监听重启，新端口: {}", state.current_port());
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_days_from_civil() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 1, 1), 10957);
        // 相邻日期差
        assert_eq!(
            days_from_civil(2026, 7, 28) - days_from_civil(2026, 7, 1),
            27
        );
        // 跨年差
        assert_eq!(
            days_from_civil(2026, 1, 1) - days_from_civil(2025, 1, 1),
            365
        );
    }

    #[test]
    fn test_parse_time() {
        let mut q = HashMap::new();
        q.insert("start".into(), "1753718400".into());
        assert_eq!(parse_time(&q, "start"), Some(1753718400));
        // 毫秒时间戳自动转秒
        q.insert("start".into(), "1753718400000".into());
        assert_eq!(parse_time(&q, "start"), Some(1753718400));
        // YYYYMMDD：start 取当天 00:00:00
        q.insert("start".into(), "20260728".into());
        let day0 = days_from_civil(2026, 7, 28) * 86400;
        assert_eq!(parse_time(&q, "start"), Some(day0));
        // YYYYMMDD：end 取当天 23:59:59
        q.insert("end".into(), "20260728".into());
        assert_eq!(parse_time(&q, "end"), Some(day0 + 86399));
        // 非法输入
        q.insert("start".into(), "abc".into());
        assert_eq!(parse_time(&q, "start"), None);
        // 缺失参数
        assert_eq!(parse_time(&q, "nope"), None);
    }

    #[test]
    fn test_parse_event_meta() {
        let (id, ev) = parse_event_meta(r#"{"ack_id":"42","type":3}"#);
        assert_eq!(id.as_deref(), Some("42"));
        assert_eq!(ev, "message.new");
        let (_, ev2) = parse_event_meta(r#"{"batch":[]}"#);
        assert_eq!(ev2, "message.batch");
        let (_, ev3) = parse_event_meta(r#"{"type":10000}"#);
        assert_eq!(ev3, "message.revoke");
        let (id4, ev4) = parse_event_meta("not json");
        assert!(id4.is_none());
        assert_eq!(ev4, "message.new");
    }
}
