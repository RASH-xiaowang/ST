//! iLink HTTP 客户端：统一请求头 + 各接口封装

use base64::Engine;
use rand::Rng;
use reqwest::header::HeaderMap;
use std::error::Error;
use std::sync::Mutex;
use std::time::Duration;

use super::types::{
    GetConfigRequest, GetConfigResponse, GetUpdatesRequest, GetUpdatesResponse,
    GetUploadUrlRequest, GetUploadUrlResponse, SendMessageRequest, SendTypingRequest,
    CHANNEL_VERSION, ILINK_APP_ID,
};
use crate::common::truncate;

/// 长轮询 hold 时长（服务端约 35s，客户端超时放宽到 45s）
pub const LONG_POLL_TIMEOUT: Duration = Duration::from_secs(45);
/// 普通 API 超时
pub const API_TIMEOUT: Duration = Duration::from_secs(15);
/// 配置/输入状态超时
pub const CONFIG_TIMEOUT: Duration = Duration::from_secs(10);

/// 代理探测结果缓存：避免每次新建客户端都做 1.2s TCP 探测（轮询会高频新建客户端）
static PROXY_CACHE: Mutex<Option<(Option<String>, std::time::Instant)>> = Mutex::new(None);
const PROXY_CACHE_TTL: Duration = Duration::from_secs(300);

/// 编码版本号：2.4.3 → (2<<16)|(4<<8)|3 = 132099
fn build_client_version(version: &str) -> u32 {
    let parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    let major = parts.first().copied().unwrap_or(0) & 0xff;
    let minor = parts.get(1).copied().unwrap_or(0) & 0xff;
    let patch = parts.get(2).copied().unwrap_or(0) & 0xff;
    (major << 16) | (minor << 8) | patch
}

/// 每次请求随机生成 X-WECHAT-UIN（base64(随机 uint32 十进制字符串)）
fn random_wechat_uin() -> String {
    let n: u32 = rand::thread_rng().gen();
    base64::engine::general_purpose::STANDARD.encode(n.to_string().as_bytes())
}

fn ensure_trailing_slash(url: &str) -> String {
    if url.ends_with('/') {
        url.to_owned()
    } else {
        format!("{url}/")
    }
}

pub struct HttpApiClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
    http_direct: Option<reqwest::Client>,
}

impl HttpApiClient {
    pub fn new(base_url: &str, token: &str) -> Self {
        // 显式禁用 reqwest 自动读取环境代理，代理策略完全由 detect_system_proxy 控制，
        // 避免终端里残留的 HTTPS_PROXY 指向不可达代理导致 tunnel 错误
        let mut builder = reqwest::Client::builder().timeout(API_TIMEOUT).no_proxy();
        let mut http_direct = None;
        if let Some(proxy_url) = detect_system_proxy() {
            if let Ok(proxy) = reqwest::Proxy::https(&proxy_url) {
                log::info!("[ilink] 使用系统代理: {proxy_url}");
                builder = builder.proxy(proxy);
                http_direct = Some(
                    reqwest::Client::builder()
                        .timeout(API_TIMEOUT)
                        .no_proxy()
                        .build()
                        .unwrap_or_default(),
                );
            }
        }
        Self {
            base_url: ensure_trailing_slash(base_url),
            token: token.trim().to_owned(),
            http: builder.build().unwrap_or_default(),
            http_direct,
        }
    }

    /// POST 发送；代理失败（tunnel/connect）自动回退直连一次
    async fn send_post(
        &self,
        url: &str,
        timeout: Duration,
        body_str: &str,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let result = self
            .http
            .post(url)
            .timeout(timeout)
            .headers(self.post_headers())
            .body(body_str.to_owned())
            .send()
            .await;
        match result {
            Err(e) if self.http_direct.is_some() && e.is_connect() => {
                log::warn!("[ilink] 代理请求失败，回退直连: {e}");
                self.http_direct
                    .as_ref()
                    .unwrap()
                    .post(url)
                    .timeout(timeout)
                    .headers(self.post_headers())
                    .body(body_str.to_owned())
                    .send()
                    .await
            }
            other => other,
        }
    }

    /// GET 发送；代理失败自动回退直连一次
    async fn send_get(
        &self,
        url: &str,
        timeout: Duration,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let result = self
            .http
            .get(url)
            .timeout(timeout)
            .headers(self.get_headers())
            .send()
            .await;
        match result {
            Err(e) if self.http_direct.is_some() && e.is_connect() => {
                log::warn!("[ilink] 代理请求失败，回退直连: {e}");
                self.http_direct
                    .as_ref()
                    .unwrap()
                    .get(url)
                    .timeout(timeout)
                    .headers(self.get_headers())
                    .send()
                    .await
            }
            other => other,
        }
    }

    fn post_headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("Content-Type", "application/json".parse().unwrap());
        h.insert("AuthorizationType", "ilink_bot_token".parse().unwrap());
        h.insert("X-WECHAT-UIN", random_wechat_uin().parse().unwrap());
        if !self.token.is_empty() {
            h.insert(
                "Authorization",
                format!("Bearer {}", self.token).parse().unwrap(),
            );
        }
        h.insert("iLink-App-Id", ILINK_APP_ID.parse().unwrap());
        h.insert(
            "iLink-App-ClientVersion",
            build_client_version(CHANNEL_VERSION)
                .to_string()
                .parse()
                .unwrap(),
        );
        h
    }

    fn get_headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("iLink-App-Id", ILINK_APP_ID.parse().unwrap());
        h.insert(
            "iLink-App-ClientVersion",
            build_client_version(CHANNEL_VERSION)
                .to_string()
                .parse()
                .unwrap(),
        );
        h
    }

    pub(crate) async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &impl serde::Serialize,
        timeout: Duration,
    ) -> Result<T, String> {
        let url = format!("{}{endpoint}", self.base_url);
        let body_str = serde_json::to_string(body).map_err(|e| format!("序列化失败: {e}"))?;
        let resp = self
            .send_post(&url, timeout, &body_str)
            .await
            .map_err(|e| describe_error(&e))?;
        let status = resp.status();
        let raw = resp
            .text()
            .await
            .map_err(|e| format!("读取响应失败: {e}"))?;
        if !status.is_success() {
            return Err(format!("HTTP {status}: {}", truncate(&raw, 200)));
        }
        serde_json::from_str(&raw)
            .map_err(|e| format!("响应解析失败: {e} → {}", truncate(&raw, 120)))
    }

    async fn get_raw(&self, endpoint: &str, timeout: Duration) -> Result<String, String> {
        let url = format!("{}{endpoint}", self.base_url);
        let resp = self
            .send_get(&url, timeout)
            .await
            .map_err(|e| describe_error(&e))?;
        let status = resp.status();
        let raw = resp
            .text()
            .await
            .map_err(|e| format!("读取响应失败: {e}"))?;
        if !status.is_success() {
            return Err(format!("HTTP {status}: {}", truncate(&raw, 200)));
        }
        Ok(raw)
    }

    /// 长轮询收消息；客户端超时按空响应处理（服务端 hold）
    pub async fn get_updates(
        &self,
        request: &GetUpdatesRequest,
    ) -> Result<GetUpdatesResponse, String> {
        let url = format!("{}ilink/bot/getupdates", self.base_url);
        let body_str = serde_json::to_string(request).map_err(|e| format!("序列化失败: {e}"))?;
        match self.send_post(&url, LONG_POLL_TIMEOUT, &body_str).await {
            Ok(resp) => {
                let status = resp.status();
                let raw = resp
                    .text()
                    .await
                    .map_err(|e| format!("读取响应失败: {e}"))?;
                if !status.is_success() {
                    return Err(format!("HTTP {status}: {}", truncate(&raw, 200)));
                }
                serde_json::from_str(&raw)
                    .map_err(|e| format!("响应解析失败: {e} → {}", truncate(&raw, 120)))
            }
            Err(e) if e.is_timeout() => Ok(GetUpdatesResponse {
                ret: Some(0),
                msgs: Some(Vec::new()),
                get_updates_buf: Some(request.get_updates_buf.clone()),
                ..Default::default()
            }),
            Err(e) => Err(describe_error(&e)),
        }
    }

    pub async fn send_message(&self, request: &SendMessageRequest) -> Result<(), String> {
        let v = self
            .post_json::<serde_json::Value>("ilink/bot/sendmessage", request, API_TIMEOUT)
            .await?;
        check_ret(&v)
    }

    pub async fn get_upload_url(
        &self,
        request: &GetUploadUrlRequest,
    ) -> Result<GetUploadUrlResponse, String> {
        let v = self
            .post_json::<serde_json::Value>("ilink/bot/getuploadurl", request, API_TIMEOUT)
            .await?;
        check_ret(&v)?;
        serde_json::from_value(v).map_err(|e| format!("getuploadurl 响应解析失败: {e}"))
    }

    pub async fn get_config(
        &self,
        user_id: &str,
        context_token: Option<&str>,
    ) -> Result<GetConfigResponse, String> {
        let body = GetConfigRequest {
            ilink_user_id: user_id.to_owned(),
            context_token: context_token.map(String::from),
            base_info: super::types::build_base_info(),
        };
        let v = self
            .post_json::<serde_json::Value>("ilink/bot/getconfig", &body, CONFIG_TIMEOUT)
            .await?;
        check_ret(&v)?;
        serde_json::from_value(v).map_err(|e| format!("getconfig 响应解析失败: {e}"))
    }

    pub async fn send_typing(&self, request: &SendTypingRequest) -> Result<(), String> {
        let v = self
            .post_json::<serde_json::Value>("ilink/bot/sendtyping", request, CONFIG_TIMEOUT)
            .await?;
        check_ret(&v)
    }

    /// 匿名 GET（二维码相关接口）
    pub async fn anonymous_get(&self, endpoint: &str, timeout: Duration) -> Result<String, String> {
        self.get_raw(endpoint, timeout).await
    }
}

/// 检查接口响应中的 ret/errcode，非 0 视为失败（HTTP 200 也可能带错误码）
fn check_ret(body: &serde_json::Value) -> Result<(), String> {
    let ret = body.get("ret").and_then(|v| v.as_i64()).unwrap_or(0);
    let errcode = body.get("errcode").and_then(|v| v.as_i64()).unwrap_or(0);
    if ret != 0 || errcode != 0 {
        let msg = body
            .get("errmsg")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_owned();
        return Err(format!(
            "接口错误 ret={ret} errcode={errcode}{}",
            if msg.is_empty() {
                String::new()
            } else {
                format!(": {msg}")
            }
        ));
    }
    Ok(())
}

/// 完整错误链描述（reqwest 顶层信息 + 逐层 cause）
pub(crate) fn describe_error(e: &reqwest::Error) -> String {
    let mut msg = format!("{e}");
    let mut src = e.source();
    while let Some(s) = src {
        msg.push_str(&format!(" ← {s}"));
        src = s.source();
    }
    msg
}

/// 检测系统代理：环境变量优先，其次 Windows 注册表（HKCU Internet Settings）
fn detect_system_proxy() -> Option<String> {
    let mut cache = PROXY_CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((cached, at)) = cache.as_ref() {
        if at.elapsed() < PROXY_CACHE_TTL {
            return cached.clone();
        }
    }
    let result = detect_system_proxy_inner();
    *cache = Some((result.clone(), std::time::Instant::now()));
    result
}

fn detect_system_proxy_inner() -> Option<String> {
    for var in ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"] {
        if let Ok(v) = std::env::var(var) {
            let v = v.trim().to_owned();
            if !v.is_empty() && !v.eq_ignore_ascii_case("direct") {
                let url = normalize_proxy_url(&v);
                if proxy_alive(&url) {
                    return Some(url);
                }
                log::warn!("[ilink] 环境变量代理 {url} 不可达，跳过（回退直连）");
            }
        }
    }
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
        use winreg::RegKey;
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            KEY_READ,
        ) {
            let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
            if enabled == 1 {
                if let Ok(server) = key.get_value::<String, _>("ProxyServer") {
                    let server = server.trim().to_owned();
                    if !server.is_empty() {
                        let url = normalize_proxy_url(&server);
                        if proxy_alive(&url) {
                            return Some(url);
                        }
                        log::warn!("[ilink] 系统代理 {url} 不可达，跳过（回退直连）");
                    }
                }
            }
        }
    }
    None
}

/// 验证代理地址是否真的在监听（1.2s 超时 TCP 探测）
fn proxy_alive(raw: &str) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    let stripped = raw
        .strip_prefix("http://")
        .or_else(|| raw.strip_prefix("https://"))
        .unwrap_or(raw);
    let (host, port) = match stripped.rsplit_once(':') {
        Some((h, p)) => match p.parse::<u16>() {
            Ok(port) => (h, port),
            Err(_) => return true, // 无端口信息，跳过探测
        },
        None => return true,
    };
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return true; // 域名解析失败时交给 reqwest 处理
    };
    for addr in addrs {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(1200)).is_ok() {
            return true;
        }
    }
    false
}

/// 解析 "https=host:port;http=host:port" 或 "host:port"，补全 scheme
fn normalize_proxy_url(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(https_part) = raw
        .split(';')
        .find(|p| p.to_ascii_lowercase().starts_with("https="))
        .and_then(|p| p.split_once('=').map(|(_, v)| v.trim()))
    {
        if !https_part.is_empty() {
            return if https_part.contains("://") {
                https_part.to_owned()
            } else {
                format!("http://{https_part}")
            };
        }
    }
    if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("http://{raw}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_version() {
        assert_eq!(build_client_version("2.4.3"), 132_099);
        assert_eq!(build_client_version("2.1.1"), 131_329);
    }

    #[test]
    fn uin_format() {
        let uin = random_wechat_uin();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&uin)
            .unwrap();
        let s = std::str::from_utf8(&decoded).unwrap();
        assert!(s.parse::<u32>().is_ok());
    }

    #[test]
    fn proxy_parsing() {
        assert_eq!(
            normalize_proxy_url("https=127.0.0.1:7890;http=127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert_eq!(
            normalize_proxy_url("127.0.0.1:51081"),
            "http://127.0.0.1:51081"
        );
        assert_eq!(normalize_proxy_url("http://p:8080"), "http://p:8080");
    }
}
