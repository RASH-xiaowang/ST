// ============================================================
// HTTP API — 监控状态 / SSE 推送 / OpenAPI 文档
// 自 http_api.rs 拆分：监控指标、实时消息推送、事件名解析。
// ============================================================

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use super::{check_auth, ApiError, ApiResult, ApiServerState};

// ============ 7. 监控状态 ============

pub(crate) async fn monitor_status(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<serde_json::Value>> {
    check_auth(&state, &headers, &q, None)?;
    let running = state.monitor.is_running();
    let mut body = serde_json::json!({
        "success": true,
        "running": running,
        "uptimeSeconds": state.started.elapsed().as_secs(),
    });
    if let Some(router) = state.monitor.router() {
        let m = router.metrics().await;
        body["wsPort"] = serde_json::json!(router.ws_port());
        body["metrics"] = serde_json::json!({
            "pendingAcks": m.pending_acks,
            "sentTotal": m.sent_total,
            "sentBatchCount": m.sent_batch_count,
            "sentWsCount": m.sent_ws_count,
            "latency": {
                "buckets": m.latency_buckets,
                "sumMs": m.latency_ms_sum,
                "count": m.latency_ms_count,
            },
        });
    }
    Ok(Json(body))
}

// ============ 8. SSE 实时推送 ============

pub(crate) async fn push_messages(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>> {
    check_auth(&state, &headers, &q, None)?;
    let router = state.monitor.router().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "MONITOR_NOT_RUNNING",
            "消息监控未运行，无法订阅实时推送",
        )
    })?;

    // 断线补推：Last-Event-ID 头或 since_ack 参数
    let since: u64 = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .or_else(|| q.get("since_ack").and_then(|s| s.parse().ok()))
        .unwrap_or(0);

    let replay = router.replay_since(since).await;
    let rx = router.subscribe();

    // 先补推遗漏，再转发实时流
    let replay_stream = futures_util::stream::iter(replay);
    let live_stream =
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|r| async move { r.ok() });
    let combined = replay_stream.chain(live_stream).map(|text| {
        let (id, event_name) = parse_event_meta(&text);
        let mut ev = Event::default().event(event_name).data(text);
        if let Some(i) = id {
            ev = ev.id(i);
        }
        Ok::<Event, Infallible>(ev)
    });

    Ok(Sse::new(combined).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keepalive"),
    ))
}

/// 从消息 JSON 提取 (ack_id, 事件名)
pub(crate) fn parse_event_meta(text: &str) -> (Option<String>, &'static str) {
    let v: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return (None, "message.new"),
    };
    let id = v.get("ack_id").and_then(|x| {
        x.as_str()
            .map(|s| s.to_string())
            .or_else(|| x.as_u64().map(|n| n.to_string()))
    });
    let name = if v.get("batch").is_some() {
        "message.batch"
    } else if v.get("type").and_then(|t| t.as_i64()) == Some(10000) {
        "message.revoke"
    } else {
        "message.new"
    };
    (id, name)
}

// ============ 9. OpenAPI 描述（供"API 文档"界面动态渲染）============

pub(crate) async fn openapi_json(
    State(state): State<Arc<ApiServerState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "openapi": "3.0.0",
        "info": {
            "title": "ST 控制台 · 微信数据 HTTP API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "本机微信数据只读接口 + SSE 实时推送。仅监听 127.0.0.1。",
        },
        "servers": [{ "url": format!("http://127.0.0.1:{}", state.current_port()) }],
        "paths": {
            "/health": { "get": { "summary": "健康检查（含监控/数据库状态）" } },
            "/api/v1/sessions": { "get": { "summary": "会话列表（keyword/limit/offset）" } },
            "/api/v1/messages": { "get": { "summary": "会话消息（talker 必填，cursor 分页/时间/关键词过滤）" } },
            "/api/v1/sessions/{id}/messages": { "get": { "summary": "增量拉取（since 时间戳 + sync 分页块）" } },
            "/api/v1/contacts": { "get": { "summary": "联系人列表（category/keyword 过滤）" } },
            "/api/v1/group-members": { "get": { "summary": "群成员列表（chatroomId 必填）" } },
            "/api/v1/media/{username}/{local_id}": { "get": { "summary": "图片按需即时解密（含 wxgf 转码）" } },
            "/api/v1/sns/video/{file_key}": { "get": { "summary": "朋友圈视频播放（本地解密 MP4，支持 Range）" } },
            "/api/v1/monitor/status": { "get": { "summary": "监控运行状态与推送指标" } },
            "/api/v1/push/messages": { "get": { "summary": "SSE 实时消息推送（支持 Last-Event-ID 补推）" } },
        },
    }))
}
