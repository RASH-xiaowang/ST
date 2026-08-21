// ============================================================
// 消息通道 — 二维码绑定 / 重扫
// 自 manager.rs 拆分：扫码会话创建、状态轮询、绑定落库。
// ============================================================

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{default_account_name, qr_svg_data_url, BotManager, QrRecord, QrView};
use crate::bot::db::{self, BotAccount, DEFAULT_CDN_BASE_URL};
use crate::bot::ilink::auth::{self, QrStatus};

impl BotManager {
    // ─── 二维码绑定 / 重扫 ───

    pub async fn start_qr(self: &Arc<Self>, account_id: Option<i64>) -> Result<QrView, String> {
        let session = auth::create_qr().await?;
        let image_data_url = qr_svg_data_url(&session.img_url)?;
        let session_id = uuid::Uuid::new_v4().simple().to_string();
        self.qr_sessions
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                session_id.clone(),
                QrRecord {
                    qrcode: session.qrcode,
                    account_id,
                    created_at: Instant::now(),
                },
            );
        Ok(QrView {
            session_id,
            image_data_url,
            raw_url: session.img_url,
        })
    }

    pub async fn poll_qr(self: &Arc<Self>, session_id: &str) -> Result<serde_json::Value, String> {
        let record = {
            let map = self.qr_sessions.read().unwrap_or_else(|p| p.into_inner());
            match map.get(session_id) {
                Some(r) => (r.qrcode.clone(), r.account_id, r.created_at),
                None => return Err("二维码会话不存在或已过期".to_string()),
            }
        };
        if record.2.elapsed() > Duration::from_secs(180) {
            self.qr_sessions
                .write()
                .unwrap_or_else(|p| p.into_inner())
                .remove(session_id);
            return Err("二维码已过期，请重新生成".to_string());
        }

        match auth::poll_status(&record.0).await? {
            QrStatus::Wait => Ok(serde_json::json!({ "status": "wait" })),
            QrStatus::Scanned => Ok(serde_json::json!({ "status": "scaned" })),
            QrStatus::ScannedButRedirect { .. } => {
                Ok(serde_json::json!({ "status": "scaned_but_redirect" }))
            }
            QrStatus::NeedVerify => Ok(serde_json::json!({ "status": "need_verifycode" })),
            QrStatus::VerifyBlocked => Ok(serde_json::json!({ "status": "verify_code_blocked" })),
            QrStatus::Expired => Ok(serde_json::json!({ "status": "expired" })),
            QrStatus::Unknown(s) => Ok(serde_json::json!({ "status": s })),
            QrStatus::Confirmed {
                bot_token,
                ilink_bot_id,
                base_url,
                ilink_user_id,
            } => {
                if bot_token.is_empty() || base_url.is_empty() {
                    return Err("扫码成功但未返回有效 token".to_string());
                }
                let token_enc = self.cipher.encrypt(&bot_token)?;
                let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                // 【长期绑定】ilink token 的实际有效期由服务端决定（失效时轮询会
                // 收到 SessionExpired 信号自动下线），不再人为写死 24 小时——
                // 此前 expires_at = now + 24h 导致账号每天被强制过期、需要重扫。
                let expires: Option<String> = None;

                let conn = self.conn()?;
                let account_id = match record.1 {
                    Some(id) => {
                        let mut acc = db::get_account(&conn, id)
                            .map_err(|e| e.to_string())?
                            .ok_or_else(|| "账号不存在".to_string())?;
                        acc.bot_id = ilink_bot_id.clone();
                        acc.name = if acc.name.is_empty() {
                            default_account_name(&ilink_bot_id, &ilink_user_id)
                        } else {
                            acc.name.clone()
                        };
                        acc.owner_id = ilink_user_id.clone();
                        acc.token_enc = token_enc;
                        acc.base_url = base_url.clone();
                        acc.status = "online".to_owned();
                        acc.connected_at = Some(now.clone());
                        acc.expires_at = expires.clone();
                        acc.last_error = String::new();
                        acc.sync_buf = String::new();
                        db::update_account(&conn, &acc).map_err(|e| e.to_string())?;
                        // 旧轮询任务停止
                        if let Some(old) = self
                            .accounts
                            .read()
                            .unwrap_or_else(|p| p.into_inner())
                            .get(&id)
                        {
                            old.cancel.cancel();
                        }
                        self.accounts
                            .write()
                            .unwrap_or_else(|p| p.into_inner())
                            .remove(&id);
                        id
                    }
                    None => {
                        let acc = BotAccount {
                            id: 0,
                            bot_id: ilink_bot_id.clone(),
                            name: default_account_name(&ilink_bot_id, &ilink_user_id),
                            owner_id: ilink_user_id.clone(),
                            token_enc,
                            base_url: base_url.clone(),
                            cdn_base_url: DEFAULT_CDN_BASE_URL.to_owned(),
                            platform: "wechat".to_owned(),
                            target_id: String::new(),
                            status: "online".to_owned(),
                            connected_at: Some(now.clone()),
                            expires_at: expires.clone(),
                            last_active_at: Some(now.clone()),
                            last_error: String::new(),
                            sync_buf: String::new(),
                            context_tokens_json: "{}".to_owned(),
                            created_at: now.clone(),
                            updated_at: now.clone(),
                        };
                        db::insert_account(&conn, &acc).map_err(|e| e.to_string())?
                    }
                };

                self.qr_sessions
                    .write()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(session_id);
                self.spawn_account_loop(account_id);
                self.emit_status(account_id).await;
                log::info!(
                    "[bot] 账号 {account_id} 绑定成功（bot_id={ilink_bot_id}），长期有效（凭服务端会话状态自动维持）"
                );

                // 绑定成功：向绑定微信本人发送欢迎消息（稍等会话建立，失败不影响绑定）
                {
                    let me = Arc::clone(self);
                    let owner = ilink_user_id.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        match me.send_text(account_id, &owner, "欢迎使用ST").await {
                            Ok(_) => {
                                log::info!("[bot] 账号 {account_id} 欢迎消息已发送");
                            }
                            Err(e) => {
                                log::warn!("[bot] 账号 {account_id} 欢迎消息发送失败: {e}");
                            }
                        }
                    });
                }
                Ok(serde_json::json!({
                    "status": "confirmed",
                    "accountId": account_id,
                    "expiresAt": expires,
                }))
            }
        }
    }

    pub fn cancel_qr(&self, session_id: &str) {
        self.qr_sessions
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .remove(session_id);
    }
}
