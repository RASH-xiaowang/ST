// ============================================================
// 消息通道适配器（v2：QQ 官方机器人）
//   qqbot   QQ 官方机器人（AppID + Secret → 官方开放平台 API）
// 微信通道走 ilink 模块；企业微信 / 钉钉 / OneBot 已移除（J-23）
// 所有请求显式 no_proxy 直连（与 CDN 修复一致），错误带完整 cause 链
// ============================================================

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::common::{describe_reqwest_error, truncate};

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

fn direct_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .no_proxy()
        .build()
        .unwrap_or_default()
}

/// 读取请求响应文本；非 2xx 报错并附服务端正文
async fn read_text(resp: reqwest::Response) -> Result<String, String> {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", truncate(&body, 200)));
    }
    Ok(body)
}

// ───────────────────────── QQ 官方机器人 ─────────────────────────
//
// QQ 开放平台官方机器人：只需 AppID + ClientSecret，经官方 API 发消息。
// - 取 token：POST https://bots.qq.com/app/getAppAccessToken
//   （{appId, clientSecret} → access_token，约 2 小时有效，本地缓存）
// - 发消息：Authorization: QQBot {token}
//   C2C: POST https://api.sgroup.qq.com/v2/users/{openid}/messages
//   群 : POST https://api.sgroup.qq.com/v2/groups/{group_openid}/messages
// 注意：目标填 openid（不是 QQ 号）；官方「主动消息」要求对方 24 小时内
// 与机器人互动过（错误码 11255），被动回复需带 event_id/msg_id。

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct QqbotConfig {
    pub app_id: String,
    pub app_secret: String,
    /// private | group
    pub target_type: String,
    /// 用户 openid / 群 group_openid
    pub target_id: String,
}

impl Default for QqbotConfig {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            app_secret: String::new(),
            target_type: "private".to_owned(),
            target_id: String::new(),
        }
    }
}

impl QqbotConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.app_id.trim().is_empty() {
            return Err("缺少机器人 AppID".to_string());
        }
        if self.app_secret.trim().is_empty() {
            return Err("缺少机器人 Secret".to_string());
        }
        if !self.target_type.eq_ignore_ascii_case("private")
            && !self.target_type.eq_ignore_ascii_case("group")
        {
            return Err("目标类型必须是 private 或 group".to_string());
        }
        Ok(())
    }

    pub fn resolve_target(&self, to: &str) -> Result<(String, String), String> {
        self.validate()?;
        // 支持 "private:openid" / "group:group_openid" 前缀覆盖（发送台临时指定目标）
        let mut target_type = self.target_type.to_ascii_lowercase();
        let mut target = to.trim().to_owned();
        if let Some((prefix, rest)) = target.split_once(':') {
            let p = prefix.trim().to_ascii_lowercase();
            if p == "private" || p == "group" {
                target_type = p;
                target = rest.trim().to_owned();
            }
        }
        if target.is_empty() {
            target = self.target_id.trim().to_owned();
        }
        if target.is_empty() {
            return Err("缺少推送目标（用户 openid / 群 openid）".to_string());
        }
        // 纯数字目标是 QQ 号/群号：官方接口只接受 openid，必失败——
        // 直接给出收集指引，避免用户困惑于 501003 之类的原始错误
        if target.len() >= 5 && target.bytes().all(|b| b.is_ascii_digit()) {
            return Err(
                "目标看起来是 QQ 号/群号：QQ 官方机器人要求填 openid（不是 QQ 号）——\
                 在群里 @机器人 发消息后，群 openid 会自动收集到发送台列表，点击选择即可"
                    .to_string(),
            );
        }
        Ok((target_type, target))
    }
}

/// access_token 缓存：(app_id → (token, 过期时刻))
static QQ_TOKEN_CACHE: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();

/// 获取 QQ 官方机器人 access_token（带缓存；失效自动重取）
pub(crate) async fn qqbot_access_token(app_id: &str, secret: &str) -> Result<String, String> {
    let cache = QQ_TOKEN_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let guard = cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((token, exp)) = guard.get(app_id) {
            if exp.saturating_duration_since(Instant::now()) > Duration::from_secs(60) {
                return Ok(token.clone());
            }
        }
    }
    let resp = direct_client()
        .post("https://bots.qq.com/app/getAppAccessToken")
        .json(&serde_json::json!({
            "appId": app_id.trim(),
            "clientSecret": secret.trim(),
        }))
        .send()
        .await
        .map_err(|e| format!("QQ 机器人获取 token 失败: {}", describe_reqwest_error(&e)))?;
    let text = read_text(resp).await?;
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|_| format!("QQ 机器人 token 响应解析失败: {}", truncate(&text, 200)))?;
    let token = v
        .get("access_token")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            format!(
                "QQ 机器人 token 获取失败（AppID/Secret 无效？）: {}",
                truncate(&text, 200)
            )
        })?
        .to_string();
    let expires = v
        .get("expires_in")
        .and_then(|x| x.as_i64())
        .unwrap_or(7200)
        .max(60);
    cache.lock().unwrap_or_else(|p| p.into_inner()).insert(
        app_id.to_string(),
        (
            token.clone(),
            Instant::now() + Duration::from_secs(expires as u64),
        ),
    );
    Ok(token)
}

/// QQ 官方 API 错误码 → 用户可读提示
fn qqbot_error_message(v: &serde_json::Value) -> String {
    let code = v
        .get("code")
        .and_then(|x| x.as_i64())
        .or_else(|| v.get("retcode").and_then(|x| x.as_i64()));
    let msg = v
        .get("message")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("msg").and_then(|x| x.as_str()))
        .unwrap_or("")
        .to_string();
    match code {
        Some(11255) => format!(
            "错误码 11255：对方 24 小时内未与机器人互动，官方限制无法主动发送（{msg}）"
        ),
        Some(22009) => format!("错误码 22009：发送过于频繁，被官方频控（{msg}）"),
        Some(304023) | Some(304024) | Some(304025) => {
            format!("错误码 {}：access_token 失效或权限不足（{msg}）", code.unwrap())
        }
        Some(501003) | Some(501002) | Some(501005) => format!(
            "错误码 {}：目标 openid 无效或不存在——openid 需从机器人收到的消息事件中获取，不是 QQ 号（{msg}）",
            code.unwrap()
        ),
        Some(40034005) => "错误码 40034005：被动回复的 msg_id 已过期（收到消息后 5 分钟内回复）".to_string(),
        Some(40034024) => {
            format!("错误码 40034024：msg_id 无效或越权——主动消息不应携带 msg_id（{msg}）")
        }
        Some(40034025) | Some(40034026) => {
            format!("错误码 {}：回复事件无效或已过期（{msg}）", code.unwrap())
        }
        Some(40034105) => "错误码 40034105：主动消息发送无权限——群主动消息需在 QQ 开放平台机器人控制台开通权限；未开通时，群里 @机器人 后 5 分钟内可由系统被动回复".to_string(),
        Some(40034128) => "错误码 40034128：被动回复时间或次数超限（收到消息后尽快回复）".to_string(),
        Some(40054005) => format!("错误码 40054005：消息被去重（{msg}）"),
        Some(40054013) => "错误码 40054013：用户拒收消息（对方设置了拒绝接收）".to_string(),
        Some(c) => format!("错误码 {c}: {msg}"),
        None => format!("响应异常: {}", truncate(&msg, 160)),
    }
}

/// 发送文本消息（QQ 官方机器人）
pub async fn qqbot_send_text(cfg: &QqbotConfig, to: &str, text: &str) -> Result<(), String> {
    qqbot_send_text_with_id(cfg, to, text, None).await
}

/// 发送文本消息（QQ 官方机器人；msg_id 传入原事件 id 即被动回复）
///
/// 官方规则（v2 发消息接口）：
///   - msg_id 字段是「被动回复的消息 ID」（取自消息事件的 d.id，
///     5 分钟内有效）；主动消息不得携带 msg_id，否则报 40034024
///   - 被动回复时 msg_seq 与 msg_id 联合去重；主动消息两者都不带
///   - 主动消息要求对方近期与机器人互动过（否则 11255/40034105）
pub async fn qqbot_send_text_with_id(
    cfg: &QqbotConfig,
    to: &str,
    text: &str,
    msg_id: Option<&str>,
) -> Result<(), String> {
    let (target_type, target_id) = cfg.resolve_target(to)?;
    if text.is_empty() {
        return Err("发送内容为空".to_string());
    }
    let token = qqbot_access_token(&cfg.app_id, &cfg.app_secret).await?;
    let url = if target_type == "group" {
        format!("https://api.sgroup.qq.com/v2/groups/{target_id}/messages")
    } else {
        format!("https://api.sgroup.qq.com/v2/users/{target_id}/messages")
    };
    // 主动消息：只有 content + msg_type；被动回复：加 msg_id + msg_seq
    let mut body = serde_json::json!({
        "content": text,
        "msg_type": 0,
    });
    if let Some(id) = msg_id {
        body["msg_id"] = serde_json::json!(id);
        body["msg_seq"] = serde_json::json!(1);
    }
    let resp = direct_client()
        .post(&url)
        .header("Authorization", format!("QQBot {token}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("QQ 机器人发送失败: {}", describe_reqwest_error(&e)))?;
    let status_code = resp.status().as_u16();
    let resp_text = resp.text().await.unwrap_or_default();
    if status_code != 200 {
        let v: serde_json::Value = serde_json::from_str(&resp_text).unwrap_or_default();
        return Err(format!(
            "QQ 机器人发送失败（HTTP {status_code}）：{}",
            qqbot_error_message(&v)
        ));
    }
    let v: serde_json::Value = serde_json::from_str(&resp_text).unwrap_or_default();
    // 官方接口成功响应带 id 字段；部分实现返回 code=0
    if let Some(code) = v.get("code").and_then(|x| x.as_i64()) {
        if code != 0 {
            return Err(format!("QQ 机器人发送失败：{}", qqbot_error_message(&v)));
        }
    }
    Ok(())
}

// ───────────────────── QQ 官方机器人：富媒体（分片上传） ─────────────────────
// 官方没有「本地文件直传」：媒体走三步流程——
//   1. POST /v2/{users|groups}/{id}/upload_prepare（md5/sha1/md5_10m）
//      → upload_id + block_size + 每片预签名 COS URL
//   2. 每片 PUT 到 COS，再 POST upload_part_finish 确认
//      （40093001 可重试，直到 retry_timeout；40093002 日配额耗尽）
//   3. POST /v2/{users|groups}/{id}/files {upload_id} → file_info
//   4. 发消息 msg_type=7 + media.file_info（主动消息，占用互动窗口）
// 参考官方 wiki + WideLee qqbot-agent-sdk media_loader（社区实现）。

/// 单文件上限（官方分片上传约 100MB；超出直接报错不浪费配额）
const QQ_MEDIA_MAX_BYTES: u64 = 100 * 1024 * 1024;
/// md5_10m 取文件前 10_002_432 字节（官方规格）
const QQ_MD5_10M_BYTES: u64 = 10_002_432;
/// 分片确认失败（40093001）默认重试窗口
const QQ_PART_FINISH_DEFAULT_TIMEOUT_SECS: f64 = 120.0;

/// 上传/大包请求专用客户端：分片 PUT 可能很慢，超时放宽到 5 分钟
fn upload_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .no_proxy()
        .build()
        .unwrap_or_default()
}

/// 本地文件扩展名 → 官方 file_type（1图片 2视频 3语音 4文件）
fn qqbot_media_file_type(path: &Path) -> Result<i64, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Ok(match ext.as_str() {
        "png" | "jpg" | "jpeg" => 1,
        "mp4" => 2,
        "silk" => 3,
        _ => 4,
    })
}

/// 单趟读取计算 md5 / sha1 / md5_10m（阻塞 IO，调用方放入 spawn_blocking）
fn compute_media_hashes(path: &Path) -> std::io::Result<(String, String, String, u64)> {
    use md5::Digest; // md5/sha1 的 Digest 是同一 digest 特质再导出，引入一次即可
    let mut md5h = md5::Md5::new();
    let mut sha1h = sha1::Sha1::new();
    let mut md5_10m = md5::Md5::new();
    let mut remaining_10m: u64 = QQ_MD5_10M_BYTES;
    let mut total: u64 = 0;
    let mut f = std::fs::File::open(path)?;
    let mut buf = [0u8; 65536];
    loop {
        let n = std::io::Read::read(&mut f, &mut buf)?;
        if n == 0 {
            break;
        }
        md5h.update(&buf[..n]);
        sha1h.update(&buf[..n]);
        if remaining_10m > 0 {
            let take = (n as u64).min(remaining_10m) as usize;
            md5_10m.update(&buf[..take]);
            remaining_10m -= take as u64;
        }
        total += n as u64;
    }
    let full_md5 = hex::encode(md5h.finalize());
    // 小文件 md5_10m 即完整 md5
    let md5_10m_hex = if total > QQ_MD5_10M_BYTES {
        hex::encode(md5_10m.finalize())
    } else {
        full_md5.clone()
    };
    Ok((full_md5, hex::encode(sha1h.finalize()), md5_10m_hex, total))
}

/// 官方 API POST（JSON），统一错误码翻译；响应可能包在 data 里
async fn qqbot_api_post(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let resp = client
        .post(url)
        .header("Authorization", format!("QQBot {token}"))
        .json(body)
        .send()
        .await
        .map_err(|e| format!("QQ 机器人接口请求失败: {}", describe_reqwest_error(&e)))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    if status != 200 {
        return Err(format!(
            "QQ 机器人接口失败（HTTP {status}）：{}",
            qqbot_error_message(&v)
        ));
    }
    if let Some(code) = v.get("code").and_then(|x| x.as_i64()) {
        if code != 0 {
            return Err(format!("QQ 机器人接口失败：{}", qqbot_error_message(&v)));
        }
    }
    Ok(v)
}

/// 官方部分响应把数值字段编码成字符串（如 block_size:"70"），统一容错解析
fn json_u64(v: &serde_json::Value) -> Option<u64> {
    if let Some(n) = v.as_u64() {
        return Some(n);
    }
    v.as_str().and_then(|s| s.parse::<u64>().ok())
}

/// 同上：i64 版本（分片序号）
fn json_i64(v: &serde_json::Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    v.as_str().and_then(|s| s.parse::<i64>().ok())
}

/// 同上：f64 版本（retry_timeout）
fn json_f64(v: &serde_json::Value) -> Option<f64> {
    if let Some(n) = v.as_f64() {
        return Some(n);
    }
    v.as_str().and_then(|s| s.parse::<f64>().ok())
}

/// 发送本地文件（QQ 官方机器人）：分片上传 + 富媒体消息
pub async fn qqbot_send_media(cfg: &QqbotConfig, to: &str, path: &Path) -> Result<(), String> {
    let (target_type, target_id) = cfg.resolve_target(to)?;
    if !path.is_file() {
        return Err("文件不存在或不是普通文件".to_string());
    }
    let file_type = qqbot_media_file_type(path)?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    let file_size = std::fs::metadata(path)
        .map_err(|e| format!("读取文件信息失败: {e}"))?
        .len();
    if file_size == 0 {
        return Err("文件为空".to_string());
    }
    if file_size > QQ_MEDIA_MAX_BYTES {
        return Err("文件超过 QQ 官方 100MB 上限，请压缩后重试".to_string());
    }
    let token = qqbot_access_token(&cfg.app_id, &cfg.app_secret).await?;
    let client = upload_client();
    let api_base = if target_type == "group" {
        format!("https://api.sgroup.qq.com/v2/groups/{target_id}")
    } else {
        format!("https://api.sgroup.qq.com/v2/users/{target_id}")
    };

    // 1) 哈希（阻塞 IO → 后台线程）
    let path_owned = path.to_path_buf();
    let (md5_hex, sha1_hex, md5_10m, _total) =
        tauri::async_runtime::spawn_blocking(move || compute_media_hashes(&path_owned))
            .await
            .map_err(|e| format!("计算文件哈希任务失败: {e}"))?
            .map_err(|e| format!("计算文件哈希失败: {e}"))?;

    // 2) upload_prepare
    let prepare_body = serde_json::json!({
        "file_type": file_type,
        "file_name": file_name,
        "file_size": file_size,
        "md5": md5_hex,
        "sha1": sha1_hex,
        "md5_10m": md5_10m,
    });
    let prepare = qqbot_api_post(
        &client,
        &format!("{api_base}/upload_prepare"),
        &token,
        &prepare_body,
    )
    .await
    .map_err(|e| format!("上传准备失败：{e}"))?;
    let src = prepare
        .get("data")
        .and_then(|d| d.as_object())
        .map(|o| serde_json::Value::Object(o.clone()))
        .unwrap_or(prepare);
    let upload_id = src
        .get("upload_id")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            format!(
                "上传准备响应缺少 upload_id: {}",
                truncate(&src.to_string(), 200)
            )
        })?
        .to_string();
    let rsp_block_size = src.get("block_size").and_then(json_u64).unwrap_or(0);
    let retry_timeout = src
        .get("retry_timeout")
        .and_then(json_f64)
        .filter(|t| *t > 0.0)
        .unwrap_or(QQ_PART_FINISH_DEFAULT_TIMEOUT_SECS)
        .min(600.0);
    let parts: Vec<(i64, String, u64)> = src
        .get("parts")
        .or_else(|| src.get("part_list"))
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let idx = p
                        .get("part_index")
                        .or_else(|| p.get("index"))
                        .and_then(json_i64);
                    let url = p
                        .get("presigned_url")
                        .or_else(|| p.get("url"))
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string());
                    match (idx, url) {
                        (Some(i), Some(u)) if !u.is_empty() => {
                            Some((i, u, p.get("block_size").and_then(json_u64).unwrap_or(0)))
                        }
                        _ => None,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    if upload_id.is_empty() || parts.is_empty() || rsp_block_size == 0 {
        return Err(format!(
            "上传准备响应无效（缺少分片信息）: {}",
            truncate(&src.to_string(), 200)
        ));
    }
    log::info!(
        "[bot] QQ 富媒体上传准备完成: upload_id={} 分片 {} 片 块 {rsp_block_size}B",
        truncate(&upload_id, 24),
        parts.len()
    );

    // 3) 逐片 PUT + 确认
    for (part_index, part_url, part_block) in &parts {
        let actual_block = if *part_block > 0 {
            *part_block
        } else {
            rsp_block_size
        };
        let offset = (*part_index - 1) as u64 * rsp_block_size;
        if offset >= file_size {
            continue;
        }
        let length = actual_block.min(file_size - offset);
        let path_owned = path.to_path_buf();
        let data = tauri::async_runtime::spawn_blocking(move || {
            let mut f = std::fs::File::open(&path_owned).map_err(|e| e.to_string())?;
            use std::io::{Read, Seek, SeekFrom};
            f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; length as usize];
            f.read_exact(&mut buf).map_err(|e| e.to_string())?;
            Ok::<Vec<u8>, String>(buf)
        })
        .await
        .map_err(|e| format!("读取分片任务失败: {e}"))??;
        let part_md5 = {
            use md5::Digest;
            hex::encode(md5::Md5::digest(&data))
        };
        put_part_with_retry(&client, part_url, &data, *part_index, parts.len()).await?;
        finish_part_with_retry(
            &client,
            &api_base,
            &token,
            &upload_id,
            *part_index,
            length,
            &part_md5,
            retry_timeout,
        )
        .await?;
    }

    // 4) 合并取 file_info
    let complete_body = serde_json::json!({ "upload_id": upload_id });
    let mut complete_err = String::new();
    for attempt in 0..3u32 {
        match qqbot_api_post(
            &client,
            &format!("{api_base}/files"),
            &token,
            &complete_body,
        )
        .await
        {
            Ok(v) => {
                let src = v
                    .get("data")
                    .and_then(|d| d.as_object())
                    .map(|o| serde_json::Value::Object(o.clone()))
                    .unwrap_or(v);
                let file_info = src
                    .get("file_info")
                    .and_then(|x| x.as_str())
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        format!(
                            "上传合并响应缺少 file_info: {}",
                            truncate(&src.to_string(), 200)
                        )
                    })?
                    .to_string();
                // 5) 发富媒体消息（主动消息）
                return send_qq_media_msg(&client, &api_base, &token, &file_info, None).await;
            }
            Err(e) => {
                complete_err = e;
                if attempt < 2 {
                    tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                }
            }
        }
    }
    Err(format!("上传合并失败（重试后仍失败）: {complete_err}"))
}

/// 发送富媒体消息（msg_type=7；msg_id 传入时是被动回复）
async fn send_qq_media_msg(
    client: &reqwest::Client,
    api_base: &str,
    token: &str,
    file_info: &str,
    msg_id: Option<&str>,
) -> Result<(), String> {
    let mut body = serde_json::json!({
        "msg_type": 7,
        "media": { "file_info": file_info },
    });
    if let Some(id) = msg_id {
        body["msg_id"] = serde_json::json!(id);
        body["msg_seq"] = serde_json::json!(1);
    }
    qqbot_api_post(client, &format!("{api_base}/messages"), token, &body)
        .await
        .map_err(|e| format!("发送富媒体消息失败：{e}"))?;
    Ok(())
}

/// 分片 PUT 到预签名 COS URL（重试 3 次）
async fn put_part_with_retry(
    client: &reqwest::Client,
    url: &str,
    data: &[u8],
    part_index: i64,
    total_parts: usize,
) -> Result<(), String> {
    let mut last_err = String::new();
    for attempt in 0..3u32 {
        let body = data.to_vec();
        match client
            .put(url)
            .header("Content-Length", body.len().to_string())
            .body(body)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) => last_err = format!("COS PUT 返回 HTTP {}", resp.status().as_u16()),
            Err(e) => last_err = describe_reqwest_error(&e),
        }
        if attempt < 2 {
            tokio::time::sleep(Duration::from_secs(1u64 << attempt)).await;
        }
    }
    Err(format!(
        "分片 {part_index}/{total_parts} 上传失败: {last_err}"
    ))
}

/// 分片确认 upload_part_finish（40093001 在 retry_timeout 内重试）
#[allow(clippy::too_many_arguments)]
async fn finish_part_with_retry(
    client: &reqwest::Client,
    api_base: &str,
    token: &str,
    upload_id: &str,
    part_index: i64,
    length: u64,
    part_md5: &str,
    retry_timeout: f64,
) -> Result<(), String> {
    let body = serde_json::json!({
        "upload_id": upload_id,
        "part_index": part_index,
        "block_size": length,
        "md5": part_md5,
    });
    let start = Instant::now();
    loop {
        let resp = client
            .post(format!("{api_base}/upload_part_finish"))
            .header("Authorization", format!("QQBot {token}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("分片确认请求失败: {}", describe_reqwest_error(&e)))?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let code = v.get("code").and_then(|x| x.as_i64());
        if status == 200 && code.unwrap_or(0) == 0 {
            return Ok(());
        }
        let is_retryable = status == 200 && code == Some(40093001);
        if is_retryable && start.elapsed().as_secs_f64() < retry_timeout {
            log::warn!("[bot] QQ 分片确认可重试错误（40093001），1s 后重试");
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }
        return Err(format!(
            "分片确认失败（HTTP {status}）：{}",
            qqbot_error_message(&v)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// QQ 官方机器人目标解析：支持 "private:openid" / "group:openid"
    /// 发送台临时覆盖，以及不带前缀回退到配置目标
    #[test]
    fn qqbot_target_resolution() {
        let cfg = QqbotConfig {
            app_id: "1".into(),
            app_secret: "2".into(),
            target_type: "private".into(),
            target_id: "CFG_OPENID".into(),
        };
        // 空 → 用配置目标
        let (ty, id) = cfg.resolve_target("").unwrap();
        assert_eq!((ty.as_str(), id.as_str()), ("private", "CFG_OPENID"));
        // 裸 openid → 沿用配置类型
        let (ty2, id2) = cfg.resolve_target("USER_ABC").unwrap();
        assert_eq!((ty2.as_str(), id2.as_str()), ("private", "USER_ABC"));
        // 群前缀 → 覆盖类型
        let (ty3, id3) = cfg.resolve_target("group:GROUP_XYZ").unwrap();
        assert_eq!((ty3.as_str(), id3.as_str()), ("group", "GROUP_XYZ"));
        // 私聊前缀（大小写不敏感）
        let (ty4, id4) = cfg.resolve_target("Private:USER_ABC").unwrap();
        assert_eq!((ty4.as_str(), id4.as_str()), ("private", "USER_ABC"));
        // 未配置目标且未传目标 → 报错
        let empty = QqbotConfig {
            target_id: String::new(),
            ..cfg.clone()
        };
        assert!(empty.resolve_target("").is_err());
        // 纯数字（QQ 号/群号）→ 明确报错并指引收集 openid
        let digits = cfg.resolve_target("group:123456789");
        assert!(digits.is_err());
        assert!(digits.unwrap_err().contains("openid"));
    }

    /// QQ 富媒体文件类型分类：图片/视频/语音/文件
    #[test]
    fn qqbot_media_file_type_classify() {
        assert_eq!(qqbot_media_file_type(Path::new("a.png")).unwrap(), 1);
        assert_eq!(qqbot_media_file_type(Path::new("b.JPG")).unwrap(), 1);
        assert_eq!(qqbot_media_file_type(Path::new("c.mp4")).unwrap(), 2);
        assert_eq!(qqbot_media_file_type(Path::new("d.silk")).unwrap(), 3);
        assert_eq!(qqbot_media_file_type(Path::new("e.zip")).unwrap(), 4);
        assert_eq!(qqbot_media_file_type(Path::new("f.pdf")).unwrap(), 4);
    }

    /// 官方上传接口数值字段可能是字符串（如 block_size:"70"），容错解析
    #[test]
    fn qqbot_json_numeric_tolerant() {
        let n = serde_json::json!(70);
        let s = serde_json::json!("70");
        assert_eq!(json_u64(&n), Some(70));
        assert_eq!(json_u64(&s), Some(70));
        assert_eq!(json_i64(&serde_json::json!("3")), Some(3));
        assert_eq!(json_f64(&serde_json::json!("120.5")), Some(120.5));
        assert_eq!(json_u64(&serde_json::json!("abc")), None);
        assert_eq!(json_u64(&serde_json::Value::Null), None);
    }

    /// QQ 富媒体哈希：小文件 md5_10m 等于完整 md5；sha1 与 md5 正确
    #[test]
    fn qqbot_media_hashes_small_file() {
        use md5::Digest;
        let dir = std::env::temp_dir().join(format!("st_qq_media_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hash.bin");
        let data = b"hello qq media";
        std::fs::write(&path, data).unwrap();
        let (md5_hex, sha1_hex, md5_10m, total) = compute_media_hashes(&path).unwrap();
        assert_eq!(total, data.len() as u64);
        assert_eq!(md5_hex, hex::encode(md5::Md5::digest(data)));
        assert_eq!(sha1_hex, hex::encode(sha1::Sha1::digest(data)));
        // 小文件：md5_10m 与完整 md5 一致
        assert_eq!(md5_10m, md5_hex);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
