//! 二维码登录（获取二维码 / 轮询扫码状态）

use std::time::Duration;

use super::client::HttpApiClient;
use super::types::{QrCodeResponse, QrStatusResponse, DEFAULT_BOT_TYPE};
use crate::common::truncate;

/// 二维码会话
#[derive(Debug, Clone)]
pub struct QrSession {
    pub qrcode: String,
    pub img_url: String,
}

/// 扫码状态
#[derive(Debug, Clone)]
pub enum QrStatus {
    Wait,
    Scanned,
    ScannedButRedirect {
        _redirect_host: String,
    },
    Confirmed {
        bot_token: String,
        ilink_bot_id: String,
        base_url: String,
        ilink_user_id: String,
    },
    NeedVerify,
    VerifyBlocked,
    Expired,
    Unknown(String),
}

/// 获取登录二维码（POST 优先，失败回退 GET）
pub async fn create_qr() -> Result<QrSession, String> {
    let client = HttpApiClient::new(super::types::DEFAULT_BASE_URL, "");
    let endpoint = format!("ilink/bot/get_bot_qrcode?bot_type={DEFAULT_BOT_TYPE}");

    // POST 优先，失败重试 3 次（指数退避），再回退 GET
    let mut last_post_err: Option<String> = None;
    for attempt in 1..=3 {
        match client
            .post_json::<QrCodeResponse>(
                &endpoint,
                &serde_json::json!({ "local_token_list": [] }),
                Duration::from_secs(20),
            )
            .await
        {
            Ok(r) => return Ok(parse_qr(r)),
            Err(e) => {
                last_post_err = Some(e.clone());
                if attempt < 3 {
                    tokio::time::sleep(Duration::from_millis(500 * attempt)).await;
                }
            }
        }
    }
    log::warn!(
        "[ilink] 获取二维码 POST 失败，回退 GET: {}",
        last_post_err.unwrap_or_default()
    );
    let raw = client
        .anonymous_get(&endpoint, Duration::from_secs(20))
        .await?;

    let resp: QrCodeResponse = serde_json::from_str(&raw)
        .map_err(|e| format!("二维码响应解析失败: {e} → {}", truncate(&raw, 120)))?;
    Ok(parse_qr(resp))
}

fn parse_qr(resp: QrCodeResponse) -> QrSession {
    QrSession {
        qrcode: resp.qrcode,
        img_url: resp.qrcode_img_content,
    }
}

/// 轮询扫码状态
pub async fn poll_status(qrcode: &str) -> Result<QrStatus, String> {
    let client = HttpApiClient::new(super::types::DEFAULT_BASE_URL, "");
    let endpoint = format!(
        "ilink/bot/get_qrcode_status?qrcode={}",
        urlencoding::encode(qrcode)
    );
    // 状态接口为长轮询：服务端 hold 约 30s，超时放宽到 45s；
    // 若仍超时则视为「仍在等待」，由前端继续轮询（服务端状态按 qrcode 保留）
    let raw = match client
        .anonymous_get(&endpoint, Duration::from_secs(45))
        .await
    {
        Ok(r) => r,
        Err(e) if e.contains("timed out") || e.contains("operation timed out") => {
            return Ok(QrStatus::Wait);
        }
        Err(e) => return Err(e),
    };
    let resp: QrStatusResponse = serde_json::from_str(&raw)
        .map_err(|e| format!("二维码状态解析失败: {e} → {}", truncate(&raw, 120)))?;

    Ok(match resp.status.as_str() {
        "scaned" => QrStatus::Scanned,
        "scaned_but_redirect" => QrStatus::ScannedButRedirect {
            _redirect_host: resp.redirect_host.unwrap_or_default(),
        },
        "wait" => QrStatus::Wait,
        "confirmed" => QrStatus::Confirmed {
            bot_token: resp.bot_token.unwrap_or_default(),
            ilink_bot_id: resp.ilink_bot_id.unwrap_or_default(),
            base_url: resp.baseurl.unwrap_or_default(),
            ilink_user_id: resp.ilink_user_id.unwrap_or_default(),
        },
        "need_verifycode" => QrStatus::NeedVerify,
        "verify_code_blocked" => QrStatus::VerifyBlocked,
        "expired" => QrStatus::Expired,
        other => QrStatus::Unknown(other.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "真实网络测试：诊断二维码接口连通性用（cargo test -- --ignored）"]
    async fn live_create_qr_debug() {
        match create_qr().await {
            Ok(s) => println!("LIVE_OK url={}", s.img_url),
            Err(e) => println!("LIVE_ERR {e:?}"),
        }
    }

    #[tokio::test]
    #[ignore = "真实网络测试：验证状态长轮询不超时（cargo test -- --ignored）"]
    async fn live_poll_status_no_timeout() {
        let session = match create_qr().await {
            Ok(s) => s,
            Err(e) => {
                println!("LIVE_QR_ERR {e:?}");
                return;
            }
        };
        let started = std::time::Instant::now();
        match poll_status(&session.qrcode).await {
            Ok(QrStatus::Wait) => {
                println!(
                    "LIVE_POLL_OK wait after {}ms",
                    started.elapsed().as_millis()
                )
            }
            Ok(other) => println!("LIVE_POLL_OTHER {other:?}"),
            Err(e) => println!("LIVE_POLL_ERR {e:?}"),
        }
    }
}
