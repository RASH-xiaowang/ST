// ============================================================
// 消息通道 → 自动化引擎桥接
// 入站消息：保存媒体 → 写日志 → 事件推送 → process_sync
// 规则命中：异步 AI 分析 → 写回任务 → 自动回复（reply 字段）
// ============================================================

use rusqlite::Connection;
use serde_json::{json, Value};
use std::sync::Arc;

use super::ilink::poller::PolledMessage;
use super::manager::BotManager;

pub async fn handle_inbound(manager: Arc<BotManager>, account_id: i64, msg: PolledMessage) {
    // 读取账号 CDN base（媒体下载用）
    let cdn_base_url = match manager.conn().ok().and_then(|c| {
        super::db::get_account(&c, account_id)
            .ok()
            .flatten()
            .map(|a| a.cdn_base_url)
    }) {
        Some(url) => url,
        None => super::db::DEFAULT_CDN_BASE_URL.to_owned(),
    };

    // 保存媒体（图片/语音/文件/视频）
    let media_saved = if let Some(media) = &msg.media {
        match manager
            .save_inbound_media(account_id, &cdn_base_url, media)
            .await
        {
            Ok(saved) => saved,
            Err(e) => {
                log::warn!("[bot] 账号 {account_id} 媒体保存失败: {e}");
                None
            }
        }
    } else {
        None
    };

    let (media_kind, local_path, orig_name) = match &media_saved {
        Some((kind, path, name)) => (Some(kind.clone()), Some(path.clone()), Some(name.clone())),
        None => (None, None, None),
    };
    let media_type = media_kind.clone().unwrap_or_else(|| {
        if msg.body.is_some() {
            "text".to_owned()
        } else {
            String::new()
        }
    });
    let content = msg.body.clone().unwrap_or_else(|| {
        let label = match media_kind.as_deref() {
            Some("image") => "图片",
            Some("voice") => "语音",
            Some("file") => "文件",
            Some("video") => "视频",
            _ => "消息",
        };
        if let Some(name) = &orig_name {
            if !name.is_empty() {
                format!("[{label}] {name}")
            } else {
                format!("[{label}]")
            }
        } else {
            format!("[{label}]")
        }
    });
    let ts = msg.ts;
    let time_str = chrono::DateTime::from_timestamp_millis(ts)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

    let full = json!({
        "channel": super::channel::CHANNEL_ILINK,
        "account_id": account_id,
        "sender_username": msg.from,
        "sender": msg.from,
        "username": msg.from,
        "chat": msg.from,
        "content": content,
        "media_type": media_type,
        "is_send": false,
        "session_type": "friend",
        "is_group": false,
        "msg_type": 1,
        "timestamp": ts,
        "time": time_str,
        "local_id": msg.msg_id.map(|v| v.to_string()).unwrap_or_default(),
        "context_token": msg.context_token,
        "local_path": local_path.unwrap_or_default(),
        "orig_name": orig_name.unwrap_or_default(),
    });

    // 消息日志 + 前端实时事件
    let log_id = if let Ok(conn) = manager.conn() {
        super::db::insert_log(
            &conn,
            &super::db::LogEntry {
                account_id,
                direction: "in",
                msg_type: full["media_type"].as_str().unwrap_or("text"),
                peer: full["sender_username"].as_str().unwrap_or(""),
                content: full["content"].as_str().unwrap_or(""),
                local_path: full["local_path"].as_str().unwrap_or(""),
                status: "ok",
                error: "",
            },
        )
        .ok()
    } else {
        None
    };
    let _ = log_id;
    manager.emit(
        "bot://message",
        &json!({
            "id": log_id,
            "accountId": account_id,
            "direction": "in",
            "msgType": full["media_type"],
            "peer": full["sender_username"],
            "content": full["content"],
            "localPath": full["local_path"],
            "createdAt": time_str,
        }),
    );

    // ─── 自动化引擎 ───
    let db_path = manager.db_path().to_path_buf();
    let conn = match Connection::open(&db_path) {
        Ok(c) => {
            c.execute_batch("PRAGMA busy_timeout=5000;").ok();
            c
        }
        Err(e) => {
            log::error!("[bot] 打开数据库失败: {e}");
            return;
        }
    };
    match crate::automation::engine::process_sync(&conn, &full) {
        Ok(Some(outcome)) => {
            let mut payload = full.clone();
            payload["automationHit"] = json!(true);
            payload["ruleName"] = json!(outcome
                .rule
                .as_ref()
                .map(|r| r.name.clone())
                .unwrap_or_default());
            emit_message(&manager, &payload);

            if let Some(rule) = outcome.rule {
                if let Some(prompt) = outcome.analyze_prompt {
                    let result = crate::automation::engine::finish_ai(&rule, &prompt).await;
                    if let Ok(conn2) = Connection::open(&db_path) {
                        match result {
                            Ok((extract, ttype, tid)) => {
                                let _ = crate::automation::engine::apply_ai_result(
                                    &conn2,
                                    outcome.task_id,
                                    &extract,
                                    &ttype,
                                    &tid,
                                    "",
                                );
                                // AI 返回 reply/reply_text 字段 → 自动进入待回复队列
                                if let Some(reply) =
                                    crate::automation::engine::extract_reply(&extract)
                                {
                                    let _ = crate::automation::db::update_task_reply(
                                        &conn2,
                                        outcome.task_id,
                                        &reply,
                                        "to_reply",
                                    );
                                }
                                log::info!(
                                    "[bot] 消息任务 {task_id} AI 分析完成",
                                    task_id = outcome.task_id
                                );
                            }
                            Err(e) => {
                                let _ = crate::automation::engine::apply_ai_result(
                                    &conn2,
                                    outcome.task_id,
                                    &Value::Null,
                                    "",
                                    "",
                                    &e,
                                );
                                log::warn!("[bot] 消息任务 AI 分析失败: {e}");
                            }
                        }
                    }
                } else {
                    log::info!("[bot] 消息入库并派发: task_id={}", outcome.task_id);
                }
            }
        }
        Ok(None) => {
            emit_message(&manager, &full);
        }
        Err(e) => {
            log::warn!("[bot] 消息处理失败: {e}");
            emit_message(&manager, &full);
        }
    }
}

fn emit_message(manager: &BotManager, payload: &Value) {
    manager.emit("automation://message", payload);
}
