// ============================================================
// QQ 官方机器人 WebSocket 网关（openid 自动收集）
//
// 解决「openid 去哪拿」：官方后台没有 openid 检索界面，但只要
// 连接官方 gateway（wss://api.bot.qq.com/websocket/），所有给
// 机器人发过消息的用户（C2C_MESSAGE_CREATE）与群
// （GROUP_AT_MESSAGE_CREATE）都会带 openid 到达——自动记录到
// qqbot_contacts 表，前端发送框直接选择，无需人工查找。
//
// 协议（官方 v2 WebSocket 方式）：
//   连接 → HELLO(op10, heartbeat_interval) → IDENTIFY(op2,
//   token+intents+shard) → READY / DISPATCH(op0, s=seq, t=事件)
//   每 heartbeat_interval 发 HEARTBEAT(op1, 最近 seq)
// ============================================================

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

use super::channels::{self, QqbotConfig};
use super::manager::BotManager;

const OP_DISPATCH: i64 = 0;
const OP_HEARTBEAT: i64 = 1;
const OP_IDENTIFY: i64 = 2;
const OP_RECONNECT: i64 = 7;
const OP_INVALID_SESSION: i64 = 9;
const OP_HELLO: i64 = 10;
const OP_HEARTBEAT_ACK: i64 = 11;

/// C2C 消息与群 @ 消息事件（官方后台需在「消息配置」启用对应事件）
const INTENTS: i64 = (1 << 25) | (1 << 30);
const GATEWAY_URL: &str = "wss://api.bot.qq.com/websocket/";

/// 为所有 qqbot 账号启动网关连接（幂等；每个账号一个连接）
pub fn spawn_qqbot_gateways(manager: Arc<BotManager>) {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        // 每 30 秒重扫账号：新账号自动接入、解绑账号自动断开
        let mut handles: std::collections::HashMap<i64, tauri::async_runtime::JoinHandle<()>> =
            std::collections::HashMap::new();
        loop {
            let accounts: Vec<(i64, QqbotConfig)> = {
                let Ok(conn) = manager.conn() else {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    continue;
                };
                let Ok(list) = super::db::list_accounts(&conn) else {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    continue;
                };
                list.into_iter()
                    .filter(|a| a.platform == "qqbot")
                    .filter_map(|a| {
                        manager
                            .channel_config::<QqbotConfig>(&a)
                            .ok()
                            .map(|cfg| (a.id, cfg))
                    })
                    .collect()
            };
            // 停掉已解绑/失效账号的连接
            let ids: std::collections::HashSet<i64> = accounts.iter().map(|(id, _)| *id).collect();
            handles.retain(|id, h| {
                if ids.contains(id) {
                    true
                } else {
                    h.abort();
                    log::info!("[qqbot] 账号 {id} 已移除，网关连接断开");
                    false
                }
            });
            // 为尚未连接的账号启动网关
            for (account_id, cfg) in accounts {
                if handles.contains_key(&account_id) {
                    continue;
                }
                if cfg.app_id.trim().is_empty() || cfg.app_secret.trim().is_empty() {
                    continue;
                }
                let mgr = Arc::clone(&manager);
                handles.insert(
                    account_id,
                    tauri::async_runtime::spawn(async move {
                        run_gateway_loop(mgr, account_id, cfg).await;
                    }),
                );
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

/// 单账号网关循环：连接 → HELLO → IDENTIFY → 事件处理（断线重连）
async fn run_gateway_loop(manager: Arc<BotManager>, account_id: i64, cfg: QqbotConfig) {
    loop {
        match connect_and_serve(&manager, account_id, &cfg).await {
            Ok(()) => {
                log::info!("[qqbot] 账号 {account_id} 网关会话正常结束，稍后重连");
            }
            Err(e) => {
                log::warn!("[qqbot] 账号 {account_id} 网关连接失败: {e}");
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn connect_and_serve(
    manager: &Arc<BotManager>,
    account_id: i64,
    cfg: &QqbotConfig,
) -> Result<(), String> {
    let token = channels::qqbot_access_token(&cfg.app_id, &cfg.app_secret).await?;
    let (mut ws, _) = tokio_tungstenite::connect_async(GATEWAY_URL)
        .await
        .map_err(|e| format!("连接官方网关失败: {e}"))?;

    // 1) HELLO：拿到心跳间隔
    let mut heartbeat_ms: u64 = 41250;
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(20), ws.next())
            .await
            .map_err(|_| "等待 HELLO 超时".to_string())?
            .ok_or_else(|| "等待 HELLO 时连接被关闭".to_string())?
            .map_err(|e| format!("读取失败: {e}"))?;
        let Message::Text(text) = msg else {
            continue;
        };
        let v: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("op").and_then(|x| x.as_i64()) == Some(OP_HELLO) {
            if let Some(hb) = v
                .get("d")
                .and_then(|d| d.get("heartbeat_interval"))
                .and_then(|x| x.as_u64())
            {
                heartbeat_ms = hb.max(10_000);
            }
            break;
        }
    }

    // 2) IDENTIFY
    let identify = json!({
        "op": OP_IDENTIFY,
        "d": {
            "token": format!("QQBot {token}"),
            "intents": INTENTS,
            "shard": [0, 1],
            "properties": { "$os": "windows", "$browser": "st-control", "$device": "st-control" },
        }
    });
    ws.send(Message::Text(identify.to_string()))
        .await
        .map_err(|e| format!("IDENTIFY 发送失败: {e}"))?;
    log::info!("[qqbot] 账号 {account_id} 官方网关已连接（等待消息事件）");

    // 3) 事件循环 + 心跳
    let mut last_seq: i64 = 0;
    let mut hb_tick = tokio::time::interval(Duration::from_millis(heartbeat_ms));
    hb_tick.tick().await; // 首个 tick 立即消耗
    loop {
        tokio::select! {
            _ = hb_tick.tick() => {
                let hb = json!({ "op": OP_HEARTBEAT, "d": last_seq });
                if ws.send(Message::Text(hb.to_string())).await.is_err() {
                    return Err("心跳发送失败".to_string());
                }
            }
            msg = ws.next() => {
                let Some(msg) = msg else { return Err("连接被服务端关闭".to_string()) };
                let msg = msg.map_err(|e| format!("连接断开: {e}"))?;
                let Message::Text(text) = msg else { continue };
                let v: Value = match serde_json::from_str(&text) { Ok(v) => v, Err(_) => continue };
                let op = v.get("op").and_then(|x| x.as_i64());
                if let Some(s) = v.get("s").and_then(|x| x.as_i64()) {
                    last_seq = s;
                }
                match op {
                    Some(OP_RECONNECT) => {
                        log::info!("[qqbot] 账号 {account_id} 收到 RECONNECT，重连中");
                        return Ok(());
                    }
                    Some(OP_INVALID_SESSION) => {
                        log::warn!("[qqbot] 账号 {account_id} 会话失效，重连中");
                        return Ok(());
                    }
                    Some(OP_HEARTBEAT_ACK) => {}
                    Some(OP_DISPATCH) => {
                        if let Some(t) = v.get("t").and_then(|x| x.as_str()) {
                            // 记录所有到达的事件类型：群事件（GROUP_AT_MESSAGE_CREATE）
                            // 未到达时便于确认是网关未收到还是控制台未启用群消息事件
                            log::info!("[qqbot] 账号 {account_id} 收到事件 {t}");
                            if let Some(d) = v.get("d") {
                                handle_event(manager, account_id, t, d);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// 处理消息事件：记录 openid + 接入自动化流水线（不阻塞 WS 读取循环）
fn handle_event(manager: &Arc<BotManager>, account_id: i64, event_type: &str, d: &Value) {
    // 官方事件时间（ISO 8601 字符串），用于流水线去重；缺失时用当前时间
    let ts = d
        .get("timestamp")
        .and_then(|x| x.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp_millis());
    match event_type {
        "C2C_MESSAGE_CREATE" => {
            let openid = d
                .get("author")
                .and_then(|a| a.get("user_openid"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            // 官方事件带 QQ 昵称（username），一并记录便于辨认
            let display = d
                .get("author")
                .and_then(|a| a.get("username"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let content = d.get("content").and_then(|x| x.as_str()).unwrap_or("");
            let event_id = d.get("id").and_then(|x| x.as_str()).unwrap_or("");
            if !openid.is_empty() {
                // C2C 事件 id 可用于私聊被动回复
                save_contact(
                    manager, account_id, "private", openid, display, content, event_id,
                );
                queue_inbound(
                    manager, account_id, false, openid, display, content, event_id, ts,
                );
            }
        }
        "GROUP_AT_MESSAGE_CREATE" => {
            let group_openid = d.get("group_openid").and_then(|x| x.as_str()).unwrap_or("");
            let author = d
                .get("author")
                .and_then(|a| a.get("user_openid"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let display = d
                .get("author")
                .and_then(|a| a.get("username"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let content = d.get("content").and_then(|x| x.as_str()).unwrap_or("");
            let event_id = d.get("id").and_then(|x| x.as_str()).unwrap_or("");
            if !group_openid.is_empty() {
                // 群条目记录事件 id：群无主动消息权限（40034105）时，
                // 5 分钟窗口内用它被动回复
                save_contact(
                    manager,
                    account_id,
                    "group",
                    group_openid,
                    author,
                    content,
                    event_id,
                );
                queue_inbound(
                    manager,
                    account_id,
                    true,
                    group_openid,
                    display,
                    content,
                    event_id,
                    ts,
                );
            }
            // 群内发言者本人也可以作为私聊目标（群事件 id 不能用于私聊被动回复）
            if !author.is_empty() {
                save_contact(manager, account_id, "private", author, display, content, "");
            }
        }
        _ => {}
    }
}

/// 异步投递到自动化流水线（规则匹配 / AI 分析 / 自动回复应答）
#[allow(clippy::too_many_arguments)]
fn queue_inbound(
    manager: &Arc<BotManager>,
    account_id: i64,
    is_group: bool,
    peer: &str,
    display: &str,
    content: &str,
    event_id: &str,
    ts: Option<i64>,
) {
    let mgr = Arc::clone(manager);
    let peer = peer.to_string();
    let display = display.to_string();
    let content = content.to_string();
    let event_id = event_id.to_string();
    tauri::async_runtime::spawn(async move {
        super::qqbot_inbound::handle_qq_message(
            mgr, account_id, is_group, peer, display, content, event_id, ts,
        )
        .await;
    });
}

#[allow(clippy::too_many_arguments)]
fn save_contact(
    manager: &Arc<BotManager>,
    account_id: i64,
    kind: &str,
    openid: &str,
    display: &str,
    content: &str,
    event_id: &str,
) {
    let Ok(conn) = manager.conn() else { return };
    let changed = super::db::upsert_qqbot_contact(
        &conn, account_id, kind, openid, display, content, event_id,
    );
    match changed {
        Ok(true) => log::info!(
            "[qqbot] 账号 {account_id} 新{}目标: {}（openid 已自动收集）",
            if kind == "group" { "群" } else { "用户" },
            if display.is_empty() { openid } else { display }
        ),
        Ok(false) => {}
        Err(e) => log::warn!("[qqbot] 保存 openid 失败: {e}"),
    }
}
