// ============================================================
// 自动化管理中心 — 消息消费
// 直接订阅微信监控 EventRouter 的广播通道（同进程，不依赖 HTTP/SSE），
// router 被替换（监控重启）时 30 秒内自动重新订阅。
// ============================================================

use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// 后台常驻消费：断线自动重连
pub async fn run_consumer(app: AppHandle, _url: String) {
    log::info!("[automation] 消息消费者启动（订阅微信监控 router 广播）");
    let mut fail_count: u32 = 0;
    // 启动宽限期：监控在应用启动后异步注册 router，先静默等待（2s × 30 ≈ 60s），
    // 避免启动期刷「订阅中断/暂无 router」噪音。
    let mut grace_remaining: Option<u32> = Some(30);
    loop {
        match consume_router(&app).await {
            Ok(()) => {
                // 正常退出（router 变更/通道关闭）：短暂等待后重新订阅
                fail_count = 0;
                grace_remaining = None;
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
            Err(e) => {
                if let Some(remaining) = grace_remaining {
                    if remaining > 0 {
                        grace_remaining = Some(remaining - 1);
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                    grace_remaining = None;
                    log::info!(
                        "[automation] 微信监控长时间未就绪（router 不可用），转入告警重试: {}",
                        e
                    );
                }
                fail_count += 1;
                if fail_count <= 3 {
                    log::warn!("[automation] 订阅中断（第 {fail_count} 次）: {e}");
                } else if fail_count == 4 {
                    log::warn!("[automation] 订阅持续不可用，转入静默重试（每 30 秒）");
                }
                // 指数退避：3s → 6s → 12s → 20s → 30s 上限
                let backoff = [3u64, 6, 12, 20, 30]
                    .get(fail_count.saturating_sub(1) as usize)
                    .copied()
                    .unwrap_or(30);
                tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
            }
        }
    }
}

async fn consume_router(app: &AppHandle) -> Result<(), String> {
    let st = app.state::<super::AutomationState>();
    let monitor = st
        .monitor
        .clone()
        .ok_or_else(|| "自动化未挂载微信监控状态".to_string())?;
    let router = monitor
        .router()
        .ok_or_else(|| "微信监控未运行，暂无 router".to_string())?;
    let last_ptr = Arc::as_ptr(&router) as usize;
    let mut rx = router.subscribe();
    st.mark_connected();
    log::info!("[automation] 已订阅微信监控 router（消息消费就绪）");

    loop {
        tokio::select! {
            r = rx.recv() => {
                match r {
                    Ok(text) => {
                        st.mark_received();
                        if let Ok(msg) = serde_json::from_str::<Value>(&text) {
                            handle_message(app, msg).await;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // 积压跳过，继续消费最新
                        continue;
                    }
                    Err(_) => {
                        log::warn!("[automation] 广播通道关闭，重新订阅");
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(30)) => {
                // 30 秒巡检：router 被替换（监控重启）则重新订阅
                let cur = st.router_ptr();
                if cur != Some(last_ptr) {
                    log::warn!("[automation] 微信监控 router 已变更，重新订阅");
                    break;
                }
            }
        }
    }
    st.mark_disconnected();
    Ok(())
}

async fn handle_message(app: &AppHandle, msg: Value) {
    let db_path = control_db_path();
    match rusqlite::Connection::open(&db_path) {
        Ok(conn) => {
            conn.execute_batch("PRAGMA busy_timeout=5000;").ok();
            match super::engine::process_sync(&conn, &msg) {
                Ok(Some(outcome)) => {
                    // 命中规则：emit 时附加标记，前端视觉突出
                    let mut payload = msg.clone();
                    payload["automationHit"] = serde_json::json!(true);
                    payload["ruleName"] = serde_json::json!(outcome
                        .rule
                        .as_ref()
                        .map(|r| r.name.clone())
                        .unwrap_or_default());
                    let _ = app.emit("automation://message", &payload);
                    if let Some(rule) = outcome.rule {
                        if let Some(prompt) = outcome.analyze_prompt {
                            // 异步 AI 分析 + 决策
                            let result = super::engine::finish_ai(&rule, &prompt).await;
                            if let Ok(conn2) = rusqlite::Connection::open(&db_path) {
                                match result {
                                    Ok((extract, ttype, tid)) => {
                                        let _ = super::engine::apply_ai_result(
                                            &conn2,
                                            outcome.task_id,
                                            &extract,
                                            &ttype,
                                            &tid,
                                            "",
                                        );
                                        // AI 返回 reply 字段 → 自动进入待回复队列
                                        // （本机路径与 ilink 路径行为对齐）
                                        if let Some(reply) = super::engine::extract_reply(&extract)
                                        {
                                            let _ = crate::automation::db::update_task_reply(
                                                &conn2,
                                                outcome.task_id,
                                                &reply,
                                                "to_reply",
                                            );
                                        }
                                        log::info!(
                                            "[automation] AI 分析完成: task_id={}",
                                            outcome.task_id
                                        );
                                    }
                                    Err(e) => {
                                        let _ = super::engine::apply_ai_result(
                                            &conn2,
                                            outcome.task_id,
                                            &serde_json::Value::Null,
                                            "",
                                            "",
                                            &e,
                                        );
                                        log::warn!("[automation] AI 分析失败: {e}");
                                    }
                                }
                            }
                        }
                    } else {
                        log::info!("[automation] 消息入库并派发: task_id={}", outcome.task_id);
                    }
                }
                Ok(None) => {
                    let _ = app.emit("automation://message", &msg);
                }
                Err(e) => {
                    log::warn!("[automation] 消息处理失败: {e}");
                    let _ = app.emit("automation://message", &msg);
                }
            }
        }
        Err(e) => {
            log::error!("[automation] 打开数据库失败: {e}");
            let _ = app.emit("automation://message", &msg);
        }
    }
}

fn control_db_path() -> PathBuf {
    super::control_db_path()
}
