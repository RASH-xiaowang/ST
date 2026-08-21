// ============================================================
// 微信 CDN 原图下载 — token 域
// 自 cdn_image.rs 拆分：c3o.re token 换取与 45 分钟缓存。
// ============================================================

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const TOKEN_URL: &str = "https://view.free.c3o.re/api/token";
const TOKEN_TTL: Duration = Duration::from_secs(45 * 60);

static TOKEN_CACHE: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();
/// 请求账号 → token 实际可用的账号目录名（服务端要求等于当前登录微信的目录）
static TOKEN_WXID_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn token_cache() -> &'static Mutex<HashMap<String, (String, Instant)>> {
    TOKEN_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn token_wxid_cache() -> &'static Mutex<HashMap<String, String>> {
    TOKEN_WXID_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 定位账号目录下的 global_config / global_config.crc（微信内部文件）
fn global_config_paths(wxid_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let all_users_config = wxid_dir.parent()?.join("all_users").join("config");
    let g1 = all_users_config.join("global_config");
    let g2 = all_users_config.join("global_config.crc");
    if g1.is_file() && g2.is_file() {
        Some((g1, g2))
    } else {
        None
    }
}

/// 用指定账号目录名向 c3o.re 换取 token（一次请求）
fn try_fetch_token(wxid: &str, wxid_dir: &Path) -> Result<String, String> {
    let (g1, g2) = global_config_paths(wxid_dir).ok_or_else(|| {
        format!(
            "缺少微信内部配置文件（{}），无法换取 CDN token",
            wxid_dir
                .parent()
                .map(|p| p.join("all_users").join("config").display().to_string())
                .unwrap_or_default()
        )
    })?;
    // 用 curl.exe 子进程（Windows 10+ 自带，实测对 c3o.re 稳定；
    // reqwest blocking 在本机对 wxcdn 的 GET 会间歇性挂起且超时不生效）
    let output = Command::new("curl.exe")
        .args([
            "-s",
            "-f",
            "--max-time",
            "30",
            "-X",
            "POST",
            TOKEN_URL,
            "-F",
            &format!("weixinIDFolder={}", wxid),
            "-F",
            &format!("fileBytes=@{}", g1.display()),
            "-F",
            &format!("crcBytes=@{}", g2.display()),
        ])
        .output()
        .map_err(|e| format!("调用 curl 获取 token 失败: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "CDN token 请求失败: curl exit={} {}",
            output.status,
            stderr.chars().take(120).collect::<String>()
        ));
    }
    let token: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|e| {
        format!(
            "解析 CDN token 响应失败: {} body={}",
            e,
            stdout.chars().take(80).collect::<String>()
        )
    })?;
    let token = token
        .get("token")
        .and_then(|t| t.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "CDN token 返回为空".to_string())?;

    Ok(token)
}

/// 换取（并缓存）该账号的 CDN Bearer token。
///
/// 服务端要求 `weixinIDFolder` 必须等于**当前登录微信**的账号目录
/// （`all_users/config/global_config` 属于登录账号），而应用分析的可能
/// 是历史账号；因此先试请求账号，失败后自动枚举同级 `wxid_*` 目录逐个尝试，
/// 并把成功的目录名缓存下来，避免每次重复探测。
pub fn fetch_cdn_token(wxid_dir: &Path) -> Result<String, String> {
    let wxid = wxid_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("wxid")
        .to_string();
    let now = Instant::now();
    if let Ok(cache) = token_cache().lock() {
        if let Some((tok, exp)) = cache.get(&wxid) {
            if *exp > now {
                return Ok(tok.clone());
            }
        }
    }

    // 候选账号：先请求账号本身，再是此前探测成功的目录，最后是同级的其它 wxid_* 目录
    let mut candidates = vec![wxid.clone()];
    if let Ok(cache) = token_wxid_cache().lock() {
        if let Some(actual) = cache.get(&wxid) {
            if actual != &wxid && !candidates.contains(actual) {
                candidates.push(actual.clone());
            }
        }
    }
    if let Some(parent) = wxid_dir.parent() {
        if let Ok(entries) = std::fs::read_dir(parent) {
            let mut siblings: Vec<String> = entries
                .flatten()
                .filter(|e| {
                    e.path().is_dir() && e.file_name().to_string_lossy().starts_with("wxid_")
                })
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            siblings.sort();
            for s in siblings {
                if !candidates.contains(&s) {
                    candidates.push(s);
                }
            }
        }
    }

    let mut last_err = format!("CDN token 请求失败（{}）", wxid);
    for cand in &candidates {
        match try_fetch_token(cand, wxid_dir) {
            Ok(token) => {
                if let Ok(mut cache) = token_wxid_cache().lock() {
                    cache.insert(wxid.clone(), cand.clone());
                }
                if let Ok(mut cache) = token_cache().lock() {
                    cache.insert(wxid.clone(), (token.clone(), now + TOKEN_TTL));
                }
                return Ok(token);
            }
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}
