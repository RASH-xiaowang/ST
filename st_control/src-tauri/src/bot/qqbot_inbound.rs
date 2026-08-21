// ============================================================
// QQ 官方机器人 → 自动化引擎桥接
// 网关收到 C2C / 群 @ 消息后：写日志 → 事件推送 → process_sync
// （与微信 ilink 共用同一套规则与 AI 分析流水线）；
// 命中规则的自动回复由待回复应答器经 qqbot 通道发出。
//
// 与微信桥接的差异：
//   - 无媒体下载（QQ 官方网关事件目前只处理文本内容）
//   - 回复目标记在 full_json 的 qq_reply_to（"private:openid" /
//     "group:group_openid"），应答器据此发送
//   - local_id 存官方事件 id：官方要求被动回复（消息到达后短暂
//     窗口内）带原消息 msg_id，应答器优先被动回复、失败退化为
//     主动消息（24h 互动窗口）
// ============================================================

use rusqlite::Connection;
use serde_json::{json, Value};
use std::sync::Arc;

use super::channel::CHANNEL_QQBOT;
use super::manager::BotManager;

#[allow(clippy::too_many_arguments)]
pub async fn handle_qq_message(
    manager: Arc<BotManager>,
    account_id: i64,
    is_group: bool,
    peer: String,    // 群 openid 或用户 openid
    display: String, // QQ 昵称（可能为空）
    content: String,
    event_id: String,
    event_ts: Option<i64>, // 官方事件时间（毫秒），缺失时用当前时间
) {
    let sender = peer.clone();
    let chat = peer.clone();
    let now = chrono::Local::now();
    let ts = event_ts.unwrap_or_else(|| now.timestamp_millis());
    let time_str = chrono::DateTime::from_timestamp_millis(ts)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| now.format("%Y-%m-%d %H:%M:%S").to_string());
    let reply_to = if is_group {
        format!("group:{peer}")
    } else {
        format!("private:{peer}")
    };

    let full = json!({
        "channel": CHANNEL_QQBOT,
        "account_id": account_id,
        "sender_username": sender,
        "sender": sender,
        "username": display,
        "chat": chat,
        "content": content,
        "media_type": "text",
        "is_send": false,
        "session_type": if is_group { "group" } else { "friend" },
        "is_group": is_group,
        "msg_type": 1,
        "timestamp": ts,
        "time": time_str,
        "local_id": event_id,
        "qq_reply_to": reply_to,
        "context_token": "",
        "local_path": "",
        "orig_name": "",
    });

    // 消息日志 + 前端实时事件（日志视图、自动化概览共用）
    let log_id = if let Ok(conn) = manager.conn() {
        super::db::insert_log(
            &conn,
            &super::db::LogEntry {
                account_id,
                direction: "in",
                msg_type: "text",
                peer: &peer,
                content: full["content"].as_str().unwrap_or(""),
                local_path: "",
                status: "ok",
                error: "",
            },
        )
        .ok()
    } else {
        None
    };
    manager.emit(
        "bot://message",
        &json!({
            "id": log_id,
            "accountId": account_id,
            "direction": "in",
            "msgType": "text",
            "peer": peer,
            "content": full["content"],
            "localPath": "",
            "createdAt": time_str,
        }),
    );

    // ─── 自动化引擎（与微信同一条流水线）───
    let db_path = manager.db_path().to_path_buf();
    let conn = match Connection::open(&db_path) {
        Ok(c) => {
            c.execute_batch("PRAGMA busy_timeout=5000;").ok();
            c
        }
        Err(e) => {
            log::error!("[qqbot] 打开数据库失败: {e}");
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
                                    "[qqbot] 消息任务 {task_id} AI 分析完成",
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
                                log::warn!("[qqbot] 消息任务 AI 分析失败: {e}");
                            }
                        }
                    }
                } else {
                    log::info!("[qqbot] 消息入库并派发: task_id={}", outcome.task_id);
                }
            }
        }
        Ok(None) => {
            emit_message(&manager, &full);
        }
        Err(e) => {
            log::warn!("[qqbot] 消息处理失败: {e}");
            emit_message(&manager, &full);
        }
    }
}

fn emit_message(manager: &BotManager, payload: &Value) {
    manager.emit("automation://message", payload);
}
