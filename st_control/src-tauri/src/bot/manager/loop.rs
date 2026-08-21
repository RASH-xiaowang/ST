// ============================================================
// 消息通道 — 账号生命周期主循环
// 自 manager.rs 拆分：启动全部账号、每账号长轮询任务、
// token 持久化与状态/错误/事件上报。
// ============================================================

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use super::{AccountRuntime, BotManager};
use crate::bot::db;
use crate::bot::ilink::client::HttpApiClient;
use crate::bot::ilink::poller::{self, PollError};

impl BotManager {
    pub fn start_all(self: &Arc<Self>) {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(e) => {
                log::error!("[bot] 恢复账号失败: {e}");
                return;
            }
        };
        let accounts = match db::list_accounts(&conn) {
            Ok(a) => a,
            Err(e) => {
                log::error!("[bot] 读取账号失败: {e}");
                return;
            }
        };
        for acc in accounts {
            // 仅微信（ilink）平台账号走长轮询：qqbot 是官方推送型通道
            // （WebSocket 网关），没有 ilink 会话可轮询——把它们塞进轮询循环
            // 会用空 base_url 构造 ilink 客户端导致异常状态
            if acc.platform != "wechat" {
                continue;
            }
            if matches!(
                acc.status.as_str(),
                "online" | "connecting" | "expiring" | "error"
            ) {
                self.spawn_account_loop(acc.id);
            }
        }
        self.spawn_responder();
        // QQ 官方机器人走 WebSocket 网关（官方推送型），顺便自动收集 openid
        crate::bot::qqbot_gateway::spawn_qqbot_gateways(Arc::clone(self));
        log::info!(
            "[bot] 消息通道启动：恢复 {} 个账号，应答器已就绪",
            self.accounts
                .read()
                .unwrap_or_else(|p| p.into_inner())
                .len()
        );
    }

    pub(crate) fn spawn_account_loop(self: &Arc<Self>, account_id: i64) {
        {
            let mut map = self.accounts.write().unwrap_or_else(|p| p.into_inner());
            if map.contains_key(&account_id) {
                return;
            }
            let runtime = Arc::new(AccountRuntime {
                cancel: CancellationToken::new(),
                sync_buf: Mutex::new(String::new()),
                context_tokens: Mutex::new(HashMap::new()),
                status: Mutex::new("connecting".to_owned()),
                last_error: Mutex::new(String::new()),
                expiring_notified: AtomicBool::new(false),
            });
            if let Ok(conn) = self.conn() {
                if let Ok(Some(acc)) = db::get_account(&conn, account_id) {
                    *runtime.sync_buf.lock().unwrap_or_else(|p| p.into_inner()) = acc.sync_buf;
                    if let Ok(tokens) =
                        serde_json::from_str::<HashMap<String, String>>(&acc.context_tokens_json)
                    {
                        *runtime
                            .context_tokens
                            .lock()
                            .unwrap_or_else(|p| p.into_inner()) = tokens;
                    }
                }
            }
            map.insert(account_id, runtime);
        }
        let me = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            me.run_account_loop(account_id).await;
        });
    }

    async fn run_account_loop(self: &Arc<Self>, account_id: i64) {
        let runtime = {
            let map = self.accounts.read().unwrap_or_else(|p| p.into_inner());
            match map.get(&account_id) {
                Some(r) => Arc::clone(r),
                None => return,
            }
        };
        let mut backoff: u64 = 0;
        log::info!("[bot] 账号 {account_id} 轮询启动");

        loop {
            if runtime.cancel.is_cancelled() {
                break;
            }

            // 账号信息每次从 DB 读取（token 可能被重扫更新）
            let (token, base_url, _cdn_base_url, expires_at) = {
                let conn = match self.conn() {
                    Ok(c) => c,
                    Err(e) => {
                        self.set_error(account_id, &format!("数据库异常: {e}"))
                            .await;
                        tokio::time::sleep(Duration::from_secs(10)).await;
                        continue;
                    }
                };
                match db::get_account(&conn, account_id) {
                    Ok(Some(acc)) => {
                        let token = match self.cipher.decrypt(&acc.token_enc) {
                            Ok(t) => t,
                            Err(e) => {
                                self.set_error(account_id, &format!("token 解密失败: {e}"))
                                    .await;
                                tokio::time::sleep(Duration::from_secs(10)).await;
                                continue;
                            }
                        };
                        (token, acc.base_url, acc.cdn_base_url, acc.expires_at)
                    }
                    Ok(None) => {
                        log::info!("[bot] 账号 {account_id} 已被删除，轮询退出");
                        break;
                    }
                    Err(e) => {
                        self.set_error(account_id, &format!("读取账号失败: {e}"))
                            .await;
                        tokio::time::sleep(Duration::from_secs(10)).await;
                        continue;
                    }
                }
            };

            // 到期检查：新绑定不写 expires_at（长期有效，凭服务端 SessionExpired
            // 信号自动下线）；此处仅兼容历史数据里遗留的 24h 过期时间
            if let Some(exp) = expires_at {
                if let Ok(exp_dt) = chrono::NaiveDateTime::parse_from_str(&exp, "%Y-%m-%d %H:%M:%S")
                {
                    let now = chrono::Local::now().naive_local();
                    if now >= exp_dt {
                        self.set_status(account_id, "expired", "连接已过期，请重新扫码")
                            .await;
                        log::warn!("[bot] 账号 {account_id} 已过期，轮询退出");
                        break;
                    }
                    if exp_dt - now < chrono::Duration::minutes(30)
                        && !runtime.expiring_notified.swap(true, Ordering::SeqCst)
                    {
                        let minutes = (exp_dt - now).num_minutes();
                        self.emit(
                            "bot://expiring",
                            &serde_json::json!({
                                "accountId": account_id,
                                "minutesLeft": minutes,
                            }),
                        );
                        log::info!("[bot] 账号 {account_id} 即将过期（剩余 {minutes} 分钟）");
                    }
                }
            }

            let client = HttpApiClient::new(&base_url, &token);
            let sync_buf = runtime
                .sync_buf
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            match poller::poll_once(&client, &sync_buf).await {
                Ok((new_buf, msgs)) => {
                    backoff = 0;
                    *runtime.sync_buf.lock().unwrap_or_else(|p| p.into_inner()) = new_buf.clone();
                    if let Ok(conn) = self.conn() {
                        let _ = db::patch_account(
                            &conn,
                            account_id,
                            "online",
                            "",
                            Some(&new_buf),
                            Some(&chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()),
                            None,
                        );
                    }
                    let prev = runtime
                        .status
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .clone();
                    if prev != "online" && prev != "expiring" {
                        *runtime.status.lock().unwrap_or_else(|p| p.into_inner()) =
                            "online".to_owned();
                        self.emit_status(account_id).await;
                    }

                    // 更新 context token 并逐条异步处理
                    for m in msgs {
                        if let (Some(token), false) = (m.context_token.clone(), m.from.is_empty()) {
                            runtime
                                .context_tokens
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .insert(m.from.clone(), token);
                            self.persist_tokens(account_id, &runtime).await;
                        }
                        let me = Arc::clone(self);
                        tauri::async_runtime::spawn(async move {
                            crate::bot::bridge::handle_inbound(me, account_id, m).await;
                        });
                    }
                }
                Err(PollError::SessionExpired) => {
                    self.set_status(account_id, "expired", "会话已过期，请重新扫码")
                        .await;
                    log::warn!("[bot] 账号 {account_id} 会话过期");
                    break;
                }
                Err(e) => {
                    backoff = (backoff + 1).min(5);
                    let delay = [3u64, 6, 12, 20, 30][(backoff - 1) as usize];
                    self.set_error(account_id, &e.to_string()).await;
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                }
            }
        }
        log::info!("[bot] 账号 {account_id} 轮询结束");
    }

    async fn persist_tokens(&self, account_id: i64, runtime: &AccountRuntime) {
        if let Ok(conn) = self.conn() {
            let tokens = runtime
                .context_tokens
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            let json = serde_json::to_string(&tokens).unwrap_or_else(|_| "{}".to_string());
            let _ = db::patch_account(&conn, account_id, "", "", None, None, Some(&json));
        }
    }

    async fn set_status(&self, account_id: i64, status: &str, error: &str) {
        {
            let map = self.accounts.read().unwrap_or_else(|p| p.into_inner());
            if let Some(r) = map.get(&account_id) {
                *r.status.lock().unwrap_or_else(|p| p.into_inner()) = status.to_owned();
                *r.last_error.lock().unwrap_or_else(|p| p.into_inner()) = error.to_owned();
            }
        }
        if let Ok(conn) = self.conn() {
            let _ = db::patch_account(&conn, account_id, status, error, None, None, None);
        }
        self.emit_status(account_id).await;
    }

    async fn set_error(&self, account_id: i64, error: &str) {
        {
            let map = self.accounts.read().unwrap_or_else(|p| p.into_inner());
            if let Some(r) = map.get(&account_id) {
                *r.status.lock().unwrap_or_else(|p| p.into_inner()) = "error".to_owned();
                *r.last_error.lock().unwrap_or_else(|p| p.into_inner()) = error.to_owned();
            }
        }
        if let Ok(conn) = self.conn() {
            let _ = db::patch_account(&conn, account_id, "error", error, None, None, None);
        }
        self.emit_status(account_id).await;
    }

    pub(crate) async fn emit_status(&self, account_id: i64) {
        let (status, error, expires_at) = {
            let conn = match self.conn() {
                Ok(c) => c,
                Err(_) => return,
            };
            match db::get_account(&conn, account_id) {
                Ok(Some(acc)) => (acc.status, acc.last_error, acc.expires_at),
                _ => return,
            }
        };
        self.emit(
            "bot://status",
            &serde_json::json!({
                "accountId": account_id,
                "status": status,
                "error": error,
                "expiresAt": expires_at,
            }),
        );
    }
}
