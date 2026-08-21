// ============================================================
// 消息通道 — 发送（文本 / 媒体）
// 自 manager.rs 拆分：文本/媒体发送（微信与非微信双轨）、
// Sender 构造与结果记录。
// ============================================================

use std::path::Path;
use std::sync::Arc;

use super::BotManager;
use crate::bot::channels::{self, QqbotConfig};
use crate::bot::db::{self, BotAccount};
use crate::bot::ilink::client::HttpApiClient;
use crate::bot::ilink::sender::Sender;

impl BotManager {
    pub async fn send_text(&self, account_id: i64, to: &str, text: &str) -> Result<String, String> {
        let acc = self.require_account(account_id).await?;
        match acc.platform.as_str() {
            "wechat" => self.send_wechat_text(account_id, &acc, to, text).await,
            "qqbot" => {
                let cfg: QqbotConfig = self.channel_config(&acc)?;
                // 发送台可传 "private:openid" / "group:group_openid" 覆盖配置目标
                let (target_type, target_id) = cfg.resolve_target(to)?;
                // 群目标：官方对「群主动消息」有独立权限（40034105），很多机器人
                // 未开通。优先用最近 @ 事件的 id 被动回复（官方 5 分钟窗口），
                // 窗口已过或失败再退化为主动消息
                let mut passive_error = String::new();
                if target_type == "group" {
                    if let Some(event_id) = self.latest_group_reply_event(account_id, &target_id) {
                        match channels::qqbot_send_text_with_id(&cfg, to, text, Some(&event_id))
                            .await
                        {
                            Ok(()) => {
                                let logged = Ok::<String, String>(event_id.clone());
                                self.log_outcome(account_id, "text", &target_id, text, "", &logged)
                                    .await;
                                log::info!("[bot] QQ 群消息被动回复成功 → {target_id}");
                                return logged;
                            }
                            Err(e) => {
                                passive_error = e;
                                log::warn!(
                                    "[bot] QQ 群被动回复失败，退化为主动消息: {passive_error}"
                                );
                            }
                        }
                    }
                }
                let result = channels::qqbot_send_text(&cfg, to, text).await;
                let msg_id = uuid::Uuid::new_v4().simple().to_string();
                let logged = match result {
                    Ok(()) => Ok(msg_id),
                    Err(e) => {
                        let mut msg = if passive_error.is_empty() {
                            e
                        } else {
                            format!("被动回复失败（{passive_error}）；主动发送也失败: {e}")
                        };
                        if msg.contains("40034105") {
                            msg = format!(
                                "{msg}。群主动消息需在 QQ 开放平台机器人控制台开通权限；\
                                 未开通时，让群里的人 @机器人 后 5 分钟内发送，系统会自动改为被动回复"
                            );
                        }
                        Err(msg)
                    }
                };
                self.log_outcome(account_id, "text", &target_id, text, "", &logged)
                    .await;
                if target_type == "group" && logged.is_ok() {
                    log::info!("[bot] QQ 官方机器人向群 {target_id} 发送文本（主动）");
                }
                logged
            }
            other => Err(format!("不支持的通道平台: {other}")),
        }
    }

    /// 群被动回复用：取该群最近一次 @ 事件 id（官方 5 分钟窗口内有效）
    fn latest_group_reply_event(&self, account_id: i64, group_openid: &str) -> Option<String> {
        let conn = self.conn().ok()?;
        let contact = db::get_qqbot_contact(&conn, account_id, "group", group_openid)
            .ok()
            .flatten()?;
        if contact.last_event_id.is_empty() {
            return None;
        }
        let seen =
            chrono::NaiveDateTime::parse_from_str(&contact.last_seen_at, "%Y-%m-%d %H:%M:%S")
                .ok()?;
        let now = chrono::Local::now().naive_local();
        if now.signed_duration_since(seen).num_minutes() >= 5 {
            log::info!("[bot] 群 {group_openid} 最近 @ 事件已超过 5 分钟，被动回复窗口已过");
            return None;
        }
        Some(contact.last_event_id)
    }

    /// QQ 官方机器人自动回复：优先被动回复（带原事件 msg_id），
    /// 窗口过期（官方约 5 分钟）失败后退化为主动消息（24h 互动窗口）
    pub async fn send_qqbot_reply(
        &self,
        account_id: i64,
        to: &str,
        text: &str,
        reply_msg_id: &str,
    ) -> Result<String, String> {
        let acc = self.require_account(account_id).await?;
        if acc.platform != "qqbot" {
            return Err(format!("账号 {account_id} 不是 QQ 官方机器人通道"));
        }
        let cfg: QqbotConfig = self.channel_config(&acc)?;
        let target_id = cfg.resolve_target(to)?.1;
        let mut passive_error = String::new();
        if !reply_msg_id.is_empty() {
            match channels::qqbot_send_text_with_id(&cfg, to, text, Some(reply_msg_id)).await {
                Ok(()) => {
                    self.log_outcome(
                        account_id,
                        "text",
                        &target_id,
                        text,
                        "",
                        &Ok(reply_msg_id.to_string()),
                    )
                    .await;
                    return Ok(reply_msg_id.to_string());
                }
                Err(e) => {
                    passive_error = e;
                    log::warn!(
                        "[bot] QQ 被动回复失败（窗口已过？），退化为主动消息: {passive_error}"
                    );
                }
            }
        }
        match channels::qqbot_send_text(&cfg, to, text).await {
            Ok(()) => {
                let msg_id = uuid::Uuid::new_v4().simple().to_string();
                self.log_outcome(
                    account_id,
                    "text",
                    &target_id,
                    text,
                    "",
                    &Ok(msg_id.clone()),
                )
                .await;
                Ok(msg_id)
            }
            Err(e) => {
                let combined = if passive_error.is_empty() {
                    e.clone()
                } else {
                    format!("被动回复失败（{passive_error}）；主动发送也失败: {e}")
                };
                self.log_outcome(
                    account_id,
                    "text",
                    &target_id,
                    text,
                    "",
                    &Err(combined.clone()),
                )
                .await;
                Err(combined)
            }
        }
    }

    /// 微信通道发送（含目标解析与 @im.wechat 补全重试）
    async fn send_wechat_text(
        &self,
        account_id: i64,
        acc: &BotAccount,
        to: &str,
        text: &str,
    ) -> Result<String, String> {
        let final_to = if to.trim().is_empty() {
            if acc.owner_id.trim().is_empty() {
                return Err("该账号未记录绑定微信 ID，无法推送".to_string());
            }
            acc.owner_id.trim().to_owned()
        } else {
            to.trim().to_owned()
        };
        match self.send_text_inner(account_id, &final_to, text).await {
            Ok(v) => Ok(v),
            Err(e) if !final_to.contains('@') => {
                // 微信数据页传入的是本地 wxid（无 @im.wechat 后缀）：
                // 首次失败后自动补全为 ClawBot 完整 ID 重试一次
                let to2 = format!("{final_to}@im.wechat");
                log::warn!("[bot] 推送 {final_to} 失败（{e}），补全 @im.wechat 后重试: {to2}");
                self.send_text_inner(account_id, &to2, text).await
            }
            Err(e) => Err(e),
        }
    }

    async fn send_text_inner(
        &self,
        account_id: i64,
        to: &str,
        text: &str,
    ) -> Result<String, String> {
        let sender = self.make_sender(account_id).await?;
        let context_token = self
            .accounts
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(&account_id)
            .and_then(|r| {
                r.context_tokens
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .get(to)
                    .cloned()
            });
        let result = sender.send_text(to, text, context_token.as_deref()).await;
        self.log_outcome(account_id, "text", to, text, "", &result)
            .await;
        result
    }

    pub async fn send_media(
        &self,
        account_id: i64,
        to: &str,
        path: &Path,
    ) -> Result<String, String> {
        let acc = self.require_account(account_id).await?;
        match acc.platform.as_str() {
            "wechat" => self.send_wechat_media(account_id, &acc, to, path).await,
            "qqbot" => {
                let cfg: QqbotConfig = self.channel_config(&acc)?;
                // 发送台可传 "private:openid" / "group:group_openid" 覆盖配置目标
                let (_, target_id) = cfg.resolve_target(to)?;
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_owned();
                let result = channels::qqbot_send_media(&cfg, to, path).await;
                let msg_id = uuid::Uuid::new_v4().simple().to_string();
                let logged = result.as_ref().map(|_| msg_id).map_err(|e| e.clone());
                self.log_outcome(
                    account_id,
                    "media",
                    &target_id,
                    &file_name,
                    &path.display().to_string(),
                    &logged,
                )
                .await;
                logged
            }
            other => Err(format!("不支持的通道平台: {other}")),
        }
    }

    /// 微信通道媒体发送（含 @im.wechat 补全重试）
    async fn send_wechat_media(
        &self,
        account_id: i64,
        acc: &BotAccount,
        to: &str,
        path: &Path,
    ) -> Result<String, String> {
        let final_to = if to.trim().is_empty() {
            if acc.owner_id.trim().is_empty() {
                return Err("该账号未记录绑定微信 ID，无法发送文件".to_string());
            }
            acc.owner_id.trim().to_owned()
        } else {
            to.trim().to_owned()
        };
        match self.send_media_inner(account_id, &final_to, path).await {
            Ok(v) => Ok(v),
            Err(e) if !final_to.contains('@') => {
                let to2 = format!("{final_to}@im.wechat");
                log::warn!(
                    "[bot] 发送文件到 {final_to} 失败（{e}），补全 @im.wechat 后重试: {to2}"
                );
                self.send_media_inner(account_id, &to2, path).await
            }
            Err(e) => Err(e),
        }
    }

    async fn send_media_inner(
        &self,
        account_id: i64,
        to: &str,
        path: &Path,
    ) -> Result<String, String> {
        let sender = self.make_sender(account_id).await?;
        let context_token = self
            .accounts
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(&account_id)
            .and_then(|r| {
                r.context_tokens
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .get(to)
                    .cloned()
            });
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_owned();
        let result = sender.send_media(to, path, context_token.as_deref()).await;
        self.log_outcome(
            account_id,
            "media",
            to,
            &file_name,
            &path.display().to_string(),
            &result,
        )
        .await;
        result
    }

    async fn make_sender(&self, account_id: i64) -> Result<Sender, String> {
        let conn = self.conn()?;
        let acc = db::get_account(&conn, account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "账号不存在".to_string())?;
        let token = self.cipher.decrypt(&acc.token_enc)?;
        Ok(Sender::new(
            Arc::new(HttpApiClient::new(&acc.base_url, &token)),
            acc.cdn_base_url,
        ))
    }

    async fn log_outcome(
        &self,
        account_id: i64,
        msg_type: &str,
        peer: &str,
        content: &str,
        local_path: &str,
        result: &Result<String, String>,
    ) {
        let (status, error) = match result {
            Ok(_) => ("ok", String::new()),
            Err(e) => ("failed", e.clone()),
        };
        if let Ok(conn) = self.conn() {
            let id = db::insert_log(
                &conn,
                &db::LogEntry {
                    account_id,
                    direction: "out",
                    msg_type,
                    peer,
                    content,
                    local_path,
                    status,
                    error: &error,
                },
            )
            .ok();
            self.emit(
                "bot://log",
                &serde_json::json!({
                    "id": id,
                    "accountId": account_id,
                    "direction": "out",
                    "msgType": msg_type,
                    "peer": peer,
                    "content": content,
                    "localPath": local_path,
                    "status": status,
                    "error": error,
                    "createdAt": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                }),
            );
        }
    }
}
