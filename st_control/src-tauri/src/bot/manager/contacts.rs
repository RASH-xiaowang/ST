// ============================================================
// 消息通道 — 联系人 / 日志 / 入站媒体 / 待回复应答器
// 自 manager.rs 拆分：会话联系人聚合、发送日志、媒体落盘、
// 待回复任务循环应答。
// ============================================================

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{sniff_ext, AccountContact, BotManager};
use crate::bot::db;

impl BotManager {
    pub fn list_contacts(&self, account_id: i64) -> Vec<AccountContact> {
        let runtime = self
            .accounts
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(&account_id)
            .cloned();
        let tokens = runtime
            .as_ref()
            .map(|r| {
                r.context_tokens
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .clone()
            })
            .unwrap_or_default();
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<AccountContact> = tokens
            .keys()
            .map(|peer| AccountContact {
                peer: peer.clone(),
                last_text: String::new(),
                last_ts: 0,
            })
            .collect();
        if let Ok((logs, _)) = db::list_logs(&conn, account_id, 1, 200) {
            for log in logs {
                if let Some(c) = out.iter_mut().find(|c| c.peer == log.peer) {
                    c.last_text = log.content;
                    if let Ok(dt) =
                        chrono::NaiveDateTime::parse_from_str(&log.created_at, "%Y-%m-%d %H:%M:%S")
                    {
                        c.last_ts = dt.and_utc().timestamp_millis();
                    }
                }
            }
        }
        out.sort_by_key(|a| std::cmp::Reverse(a.last_ts));
        out
    }

    pub fn list_logs(
        &self,
        account_id: i64,
        page: i64,
        page_size: i64,
    ) -> Result<(Vec<db::BotLog>, i64), String> {
        let conn = self.conn()?;
        db::list_logs(&conn, account_id, page, page_size).map_err(|e| e.to_string())
    }

    // ─── 入站媒体保存 ───

    pub async fn save_inbound_media(
        &self,
        account_id: i64,
        cdn_base_url: &str,
        media: &crate::bot::ilink::poller::PolledMedia,
    ) -> Result<Option<(String, String, String)>, String> {
        let m = media.clone();
        let dir = self.data_dir.join("bot_media").join(account_id.to_string());
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建媒体目录失败: {e}"))?;

        let data = if let Some(aes_key) = &m.aes_key {
            crate::bot::ilink::cdn::download_and_decrypt(cdn_base_url, &m.cdn_media, aes_key)
                .await?
        } else {
            crate::bot::ilink::cdn::download_plain(cdn_base_url, &m.cdn_media).await?
        };
        let ext = sniff_ext(&m.kind, &data, m.file_name.as_deref());
        let fname = format!("{}.{}", uuid::Uuid::new_v4().simple(), ext);
        let path = dir.join(&fname);
        tokio::fs::write(&path, &data)
            .await
            .map_err(|e| format!("写入媒体失败: {e}"))?;
        Ok(Some((
            m.kind.clone(),
            path.display().to_string(),
            m.file_name.unwrap_or_default(),
        )))
    }

    // ─── 待回复任务应答器 ───

    pub(crate) fn spawn_responder(self: &Arc<Self>) {
        if self.responder_running.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            log::info!("[bot] 待回复应答器启动（ilink + 本机微信监控任务）");
            // 同 peer 回复频控：60 秒内同一对象最多自动回复一次，
            // 防止规则批量误触发造成刷屏
            let mut last_reply_at: std::collections::HashMap<String, std::time::Instant> =
                std::collections::HashMap::new();
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let conn = match me.conn() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let pending = match crate::bot::reply_tasks::list_pending_reply(&conn, 5) {
                    Ok(p) => p,
                    Err(e) => {
                        log::warn!("[bot] 读取待回复任务失败: {e}");
                        continue;
                    }
                };
                for task in pending {
                    let task_id = task.task_id;
                    let peer = task.peer;
                    let reply_text = task.reply_text;
                    // 本机群聊任务暂不自动发送（ilink 群回复需 context_token/@ 语义），
                    // 保留 to_reply 供人工处理
                    if task.channel.is_empty() && task.is_group {
                        log::info!(
                            "[bot] 任务 {task_id} 为群聊消息，暂不自动回复（保留待人工处理）"
                        );
                        continue;
                    }
                    // 频控：同一对象 60 秒内只自动回复一次
                    let now = std::time::Instant::now();
                    if let Some(last) = last_reply_at.get(&peer) {
                        if now.duration_since(*last).as_secs() < 60 {
                            continue;
                        }
                    }
                    // 本机任务无 account_id：使用第一个绑定的微信（ilink）账号
                    let account_id = if task.account_id > 0 {
                        task.account_id
                    } else {
                        let fallback = me.conn().ok().and_then(|c| {
                            super::super::db::list_accounts(&c).ok().and_then(|list| {
                                list.into_iter()
                                    .find(|a| a.platform == "wechat")
                                    .map(|a| a.id)
                            })
                        });
                        match fallback {
                            Some(id) => id,
                            None => {
                                log::warn!(
                                    "[bot] 本机任务 {task_id} 无可用微信账号，跳过（保留待回复）"
                                );
                                continue;
                            }
                        }
                    };
                    // QQ 官方机器人：优先被动回复（带原事件 id），失败退化为主动消息
                    let send_result = if task.channel == "qqbot" {
                        me.send_qqbot_reply(
                            account_id,
                            &task.qq_reply_to,
                            &reply_text,
                            &task.qq_reply_msg_id,
                        )
                        .await
                    } else {
                        me.send_text(account_id, &peer, &reply_text).await
                    };
                    match send_result {
                        Ok(msg_id) => {
                            if let Ok(conn) = me.conn() {
                                let _ =
                                    crate::bot::reply_tasks::mark_replied(&conn, task_id, &msg_id);
                            }
                            last_reply_at.insert(peer.clone(), Instant::now());
                            log::info!("[bot] 已回复任务 {task_id} → {peer}（账号 {account_id}）");
                        }
                        Err(e) => {
                            if let Ok(conn) = me.conn() {
                                let _ =
                                    crate::bot::reply_tasks::mark_reply_failed(&conn, task_id, &e);
                            }
                            log::warn!("[bot] 回复任务 {task_id} 失败: {e}");
                        }
                    }
                }
            }
        });
    }
}
