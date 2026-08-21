// ============================================================
// 微信密钥获取 — 图片密钥（kvcomm 读取 + AES/XOR 派生校验）
// 自 auto_key.rs 拆分：GetImageKey 缓存读取、扫描根目录定位、
// wxid 清理与密钥派生/模板校验。
// ============================================================

use crate::wechat::image;
use md5::{Digest, Md5};
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::{c_int, CStr};
use std::path::{Path, PathBuf};

use super::{emit_progress, get_dll, IMAGE_KEY_BUF};

// ============ 图片密钥自动获取 ============

/// GetImageKey 返回的 JSON：`{"accounts":[{"wxid":..., "keys":[{"code":...}]}]}`
#[derive(Debug, Deserialize)]
pub(crate) struct ImageKeyResponse {
    #[serde(default)]
    pub(crate) accounts: Vec<ImageKeyAccount>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ImageKeyAccount {
    #[serde(default)]
    pub(crate) wxid: Option<String>,
    #[serde(default)]
    pub(crate) keys: Vec<ImageKeyItem>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ImageKeyItem {
    #[serde(deserialize_with = "de_u64_flex")]
    pub(crate) code: u64,
}

fn de_u64_flex<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum V {
        Num(u64),
        Str(String),
    }
    match V::deserialize(d)? {
        V::Num(n) => Ok(n),
        V::Str(s) => s.trim().parse::<u64>().map_err(serde::de::Error::custom),
    }
}

pub fn auto_get_image_key(
    app: &tauri::AppHandle,
    op: &str,
    base_dir: Option<String>,
    wxid: Option<String>,
) -> Result<serde_json::Value, String> {
    #[cfg(target_os = "windows")]
    {
        auto_get_image_key_windows(app, op, base_dir, wxid)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, op, base_dir, wxid);
        Err("仅支持 Windows 微信 4.x".to_string())
    }
}

#[cfg(target_os = "windows")]
fn auto_get_image_key_windows(
    app: &tauri::AppHandle,
    op: &str,
    base_dir: Option<String>,
    wxid: Option<String>,
) -> Result<serde_json::Value, String> {
    emit_progress(app, op, 0, 0, "正在读取微信图片密钥缓存（kvcomm）…");
    let mut dll = get_dll(Some(app))?;
    let d = dll
        .as_mut()
        .ok_or_else(|| "wx_key.dll 未加载".to_string())?;
    let mut buf = vec![0i8; IMAGE_KEY_BUF];
    let Some(get_image_key) = d.get_image_key else {
        return Err(
            "当前 wx_key.dll 不提供 GetImageKey（图片密钥请使用 kvcomm 缓存读取）".to_string(),
        );
    };
    if !unsafe { get_image_key(buf.as_mut_ptr(), buf.len() as c_int) } {
        return Err(d.last_error_string());
    }
    let text = unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .to_string();
    let resp: ImageKeyResponse =
        serde_json::from_str(&text).map_err(|e| format!("解析图片密钥缓存失败: {}", e))?;
    let codes: Vec<u64> = resp
        .accounts
        .iter()
        .flat_map(|a| a.keys.iter().map(|k| k.code))
        .collect();
    if codes.is_empty() {
        return Err(
            "未找到有效的图片密钥码（kvcomm 缓存为空，请先在微信中打开几张图片大图后重试）"
                .to_string(),
        );
    }

    // 确定扫描根目录与 wxid 候选
    let (scan_root, wxid_hint) = resolve_scan_root(app, base_dir, wxid)?;
    let wxid_candidates = collect_wxid_candidates(&scan_root, wxid_hint.as_deref());
    emit_progress(
        app,
        op,
        0,
        0,
        &format!(
            "已获取 {} 个密钥码，正在扫描 *_t.dat 模板文件…",
            codes.len()
        ),
    );

    let (ciphertext, _xor_from_template) = find_template_data(&scan_root, 32);
    if let Some(ct) = ciphertext {
        emit_progress(
            app,
            op,
            0,
            0,
            &format!(
                "正在校验候选账号（{} 个 wxid × {} 个 code）…",
                wxid_candidates.len(),
                codes.len()
            ),
        );
        for wx in &wxid_candidates {
            for &code in &codes {
                let (xor_key, aes_key) = derive_image_keys(code, wx);
                if verify_derived_aes_key(&aes_key, &ct) {
                    let _ = crate::wechat::config::patch_config(
                        crate::wechat::config::KeyConfigPatch {
                            db_dir: None,
                            db_enc_key: None,
                            image_aes_key: Some(&aes_key),
                            image_xor_key: Some(xor_key),
                        },
                    );
                    emit_progress(app, op, 1, 1, "图片密钥获取成功（已通过模板校验）");
                    return Ok(serde_json::json!({
                        "success": true,
                        "xor_key": xor_key,
                        "aes_key": aes_key,
                        "verified": true,
                        "account": wx,
                        "code": code,
                        "source": "template_verified",
                    }));
                }
            }
        }
        emit_progress(app, op, 0, 0, "模板校验未命中，回退使用缓存 code 直接派生…");
    }

    // 回退：第一个 code + 第一个 wxid（未校验）
    let wx = wxid_candidates
        .first()
        .cloned()
        .or_else(|| resp.accounts.first().and_then(|a| a.wxid.clone()))
        .unwrap_or_else(|| "unknown".to_string());
    let code = codes[0];
    let (xor_key, aes_key) = derive_image_keys(code, &wx);
    let _ = crate::wechat::config::patch_config(crate::wechat::config::KeyConfigPatch {
        db_dir: None,
        db_enc_key: None,
        image_aes_key: Some(&aes_key),
        image_xor_key: Some(xor_key),
    });
    emit_progress(app, op, 1, 1, "图片密钥已保存（未通过模板校验）");
    Ok(serde_json::json!({
        "success": true,
        "xor_key": xor_key,
        "aes_key": aes_key,
        "verified": false,
        "account": wx,
        "code": code,
        "source": "kvcomm_fallback",
        "note": "未通过模板校验，建议在微信中打开几张图片后重新获取",
    }))
}

/// 确定图片模板扫描根：显式 base_dir > 配置 db_dir 的父目录 > 最活跃账号目录
fn resolve_scan_root(
    app: &tauri::AppHandle,
    base_dir: Option<String>,
    wxid: Option<String>,
) -> Result<(PathBuf, Option<String>), String> {
    if let Some(b) = base_dir {
        let p = PathBuf::from(&b);
        if p.is_dir() {
            return Ok((p, wxid));
        }
    }
    if let Ok(cfg) = crate::wechat::config::WeChatConfig::load() {
        let db = cfg.db_dir.clone();
        if let Some(parent) = db.parent() {
            if parent.is_dir() {
                let hint = wxid.or_else(|| {
                    parent
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(String::from)
                });
                return Ok((parent.to_path_buf(), hint));
            }
        }
    }
    let accounts = crate::wechat::config::detect_accounts();
    if let Some(a) = accounts.first() {
        let root = PathBuf::from(&a.base_dir);
        let hint = wxid.or_else(|| Some(a.wxid.clone()));
        return Ok((root, hint));
    }
    let _ = app;
    Err("未找到微信账号目录，请先在配置中检测账号或选择数据库目录".to_string())
}

/// 收集 wxid 候选：传入 wxid、目录名、xwechat_files 下全部账号目录，最后兜底 unknown
fn collect_wxid_candidates(root: &Path, wxid: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        let t = s.trim().to_string();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    };
    if let Some(w) = wxid {
        push(w);
    }
    if let Some(name) = root.file_name().and_then(|n| n.to_str()) {
        if name.starts_with("wxid_") {
            push(name);
        }
    }
    // 向上最多找 3 层，定位 xwechat_files / WeChat Files 根后枚举全部账号目录
    let mut cur = root.to_path_buf();
    for _ in 0..3 {
        let Some(parent) = cur.parent() else { break };
        let is_root = parent
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| {
                n.eq_ignore_ascii_case("xwechat_files") || n.eq_ignore_ascii_case("WeChat Files")
            })
            .unwrap_or(false);
        if is_root {
            if let Ok(entries) = std::fs::read_dir(parent) {
                for e in entries.flatten() {
                    if let Some(n) = e.file_name().to_str() {
                        if n.starts_with("wxid_") && e.path().is_dir() {
                            push(n);
                        }
                    }
                }
            }
            break;
        }
        cur = parent.to_path_buf();
    }
    push("unknown");
    out
}

// ============ 图片密钥派生与校验（与 WeFlow 完全一致） ============

/// `wxid_xxx_f312` → `wxid_xxx`（截取第二个下划线之前）
pub fn clean_wxid(s: &str) -> String {
    if let Some((idx, _)) = s.match_indices('_').nth(1) {
        s[..idx].to_string()
    } else {
        s.to_string()
    }
}

/// 派生图片密钥：`aes = md5(code + clean_wxid)` hex 前 16 字符（ASCII 字节直接当 AES key），
/// `xor = code & 0xFF`
pub(crate) fn derive_image_keys(code: u64, wxid: &str) -> (u8, String) {
    let clean = clean_wxid(wxid);
    let input = format!("{}{}", code, clean);
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{:x}", digest);
    (code as u8, hex[..16].to_string())
}

/// 用模板密文校验派生 key：AES-128-ECB 解密后前几个字节须为常见图片魔数
pub(crate) fn verify_derived_aes_key(aes_hex: &str, ciphertext: &[u8]) -> bool {
    if aes_hex.len() < 16 || ciphertext.len() != 16 {
        return false;
    }
    let dec = image::aes128_ecb_decrypt(&aes_hex.as_bytes()[..16], ciphertext);
    dec.starts_with(&[0xFF, 0xD8, 0xFF]) // jpg
        || dec.starts_with(&[0x89, 0x50, 0x4E, 0x47]) // png
        || dec.starts_with(&[0x52, 0x49, 0x46, 0x46]) // webp (RIFF)
        || dec.starts_with(&[0x77, 0x78, 0x67, 0x66]) // wxgf
        || dec.starts_with(&[0x47, 0x49, 0x46]) // gif
}

/// `*_t.dat` 模板：头 6 字节 `07 08 56 32 08 07`，密文 = 偏移 15..31，
/// XOR key 由末两字节反推 `(b0^255) == (b1^217)`
pub(crate) fn find_template_data(root: &Path, max_files: usize) -> (Option<Vec<u8>>, Option<u8>) {
    let mut files = Vec::new();
    collect_template_files(root, &mut files, max_files);
    files.sort_by(|a, b| {
        let mt = |p: &Path| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0)
        };
        mt(b)
            .partial_cmp(&mt(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut ciphertext: Option<Vec<u8>> = None;
    let mut pair_counts: HashMap<(u8, u8), usize> = HashMap::new();
    for f in files.iter().take(32) {
        let Ok(data) = std::fs::read(f) else { continue };
        if data.len() < 8 || data[..6] != *b"\x07\x08V2\x08\x07" {
            continue;
        }
        *pair_counts
            .entry((data[data.len() - 2], data[data.len() - 1]))
            .or_insert(0) += 1;
        if ciphertext.is_none() && data.len() >= 31 {
            ciphertext = Some(data[15..31].to_vec());
        }
    }

    let mut xor_key = None;
    let mut best = 0usize;
    for ((b0, b1), count) in pair_counts {
        if (b0 ^ 255) == (b1 ^ 217) && count > best {
            best = count;
            xor_key = Some(b0 ^ 255);
        }
    }
    (ciphertext, xor_key)
}

fn collect_template_files(dir: &Path, out: &mut Vec<PathBuf>, limit: usize) {
    if out.len() >= limit {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= limit {
            break;
        }
        let p = entry.path();
        if p.is_dir() {
            collect_template_files(&p, out, limit);
        } else if p
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("_t.dat"))
        {
            out.push(p);
        }
    }
}
