// ============================================================
// 消息通道 — 非微信通道配置（QQ 官方机器人）
// 自 manager.rs 拆分：配置解密/回显、账号新增/更新、连通性测试。
// （企业微信 / 钉钉 / OneBot 已于 J-23 移除）
// ============================================================

use serde::de::DeserializeOwned;

use super::BotManager;
use crate::bot::channels::{self, QqbotConfig};
use crate::bot::db::{self, BotAccount, DEFAULT_CDN_BASE_URL};

impl BotManager {
    /// 解密通道配置（qqbot 的 config JSON 密文存在 token_enc）
    pub(crate) fn channel_config<T: DeserializeOwned>(
        &self,
        acc: &BotAccount,
    ) -> Result<T, String> {
        let json = self.cipher.decrypt(&acc.token_enc)?;
        serde_json::from_str(&json).map_err(|e| format!("通道配置解析失败: {e}"))
    }

    /// 返回通道配置明文（仅非微信通道，供前端编辑回显）
    pub fn channel_config_plain(&self, acc: &BotAccount) -> Result<String, String> {
        if acc.platform == "wechat" {
            return Ok(String::new());
        }
        self.cipher.decrypt(&acc.token_enc)
    }

    /// 新增非微信通道账号（QQ 官方机器人）
    pub fn add_channel_account(
        &self,
        platform: &str,
        name: String,
        config_json: String,
        target_id: String,
    ) -> Result<i64, String> {
        if platform != "qqbot" {
            return Err(format!("不支持的通道平台: {platform}"));
        }
        if name.trim().is_empty() {
            return Err("账号名称不能为空".to_string());
        }
        let token_enc = self.cipher.encrypt(config_json.trim())?;
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let acc = BotAccount {
            id: 0,
            bot_id: platform.to_owned(),
            name: name.trim().to_owned(),
            owner_id: String::new(),
            token_enc,
            base_url: String::new(),
            cdn_base_url: DEFAULT_CDN_BASE_URL.to_owned(),
            platform: platform.to_owned(),
            target_id: target_id.trim().to_owned(),
            status: "online".to_owned(),
            connected_at: Some(now.clone()),
            expires_at: None,
            last_active_at: Some(now.clone()),
            last_error: String::new(),
            sync_buf: String::new(),
            context_tokens_json: "{}".to_owned(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let conn = self.conn()?;
        let id = db::insert_account(&conn, &acc).map_err(|e| e.to_string())?;
        self.emit(
            "bot://status",
            &serde_json::json!({ "accountId": id, "status": "online" }),
        );
        log::info!("[bot] 新增通道账号：{platform}「{}」id={id}", acc.name);
        Ok(id)
    }

    /// 更新非微信通道账号的配置
    pub fn update_channel_account(
        &self,
        id: i64,
        name: String,
        config_json: String,
        target_id: String,
    ) -> Result<(), String> {
        let conn = self.conn()?;
        let mut acc = db::get_account(&conn, id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "账号不存在".to_string())?;
        if acc.platform == "wechat" {
            return Err("微信账号请使用扫码重绑更新".to_string());
        }
        if name.trim().is_empty() {
            return Err("账号名称不能为空".to_string());
        }
        acc.name = name.trim().to_owned();
        acc.token_enc = self.cipher.encrypt(config_json.trim())?;
        acc.target_id = target_id.trim().to_owned();
        acc.last_error = String::new();
        acc.status = "online".to_owned();
        db::update_account(&conn, &acc).map_err(|e| e.to_string())?;
        self.emit(
            "bot://status",
            &serde_json::json!({ "accountId": id, "status": "online" }),
        );
        log::info!("[bot] 更新通道账号 id={id}（{}）", acc.platform);
        Ok(())
    }

    /// 测试通道连通性：向默认目标发送一条测试消息
    pub async fn test_channel(&self, account_id: i64) -> Result<(), String> {
        log::info!("[bot] 通道测试开始 account_id={account_id}");
        let acc = self.require_account(account_id).await?;
        log::info!("[bot] 通道测试账号 platform={}", acc.platform);
        match acc.platform.as_str() {
            "qqbot" => {
                let cfg: QqbotConfig = self.channel_config(&acc)?;
                log::info!(
                    "[bot] 通道测试 qqbot app_id={} target={}",
                    cfg.app_id,
                    cfg.target_id
                );
                if cfg.target_id.trim().is_empty() {
                    // 未配置默认目标：退而验证凭证有效性，并给出收集指引
                    match channels::qqbot_access_token(&cfg.app_id, &cfg.app_secret).await {
                        Ok(_) => Err(
                            "凭证验证通过 ✓，但未配置默认推送目标：请在发送台选择已收集的 \
                             openid 后直接发送测试消息（私聊给机器人发消息、群里 @机器人 后 \
                             openid 会自动收集到列表）"
                                .to_string(),
                        ),
                        Err(e) => Err(e),
                    }
                } else {
                    channels::qqbot_send_text(
                        &cfg,
                        "",
                        "【ST 控制台】通道测试：QQ 官方机器人已连通 ✓",
                    )
                    .await
                }
            }
            _ => Err("微信通道请通过扫码绑定".to_string()),
        }
    }
}
