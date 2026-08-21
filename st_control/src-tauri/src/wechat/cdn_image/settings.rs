// ============================================================
// 微信 CDN 原图下载 — 配置域
// 自 cdn_image.rs 拆分：.cdn_settings.json 读写与开关。
// ============================================================

/// 读取 decoded_dir/.cdn_settings.json（enabled / localDecrypt）
fn read_cdn_settings() -> serde_json::Value {
    let Some(cfg) = crate::wechat::config::WeChatConfig::load().ok() else {
        return serde_json::json!({});
    };
    let p = cfg.decoded_image_dir.join(".cdn_settings.json");
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn write_cdn_settings(v: &serde_json::Value) {
    if let Ok(cfg) = crate::wechat::config::WeChatConfig::load() {
        let _ = std::fs::create_dir_all(cfg.decoded_image_dir.as_path());
        let _ = std::fs::write(
            cfg.decoded_image_dir.join(".cdn_settings.json"),
            serde_json::to_string(v).unwrap_or_default(),
        );
    }
}

/// CDN 自动获取原图开关（默认开启，写 decoded_dir/.cdn_settings.json）
pub fn is_cdn_enabled() -> bool {
    read_cdn_settings()
        .get("enabled")
        .and_then(|e| e.as_bool())
        .unwrap_or(true)
}

pub fn set_cdn_enabled(enabled: bool) {
    let mut v = read_cdn_settings();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("enabled".to_string(), enabled.into());
    }
    write_cdn_settings(&v);
}

/// CDN 原图解密方式：true = 本地 AES-ECB 解密（aeskey 不出本机，默认）；
/// false = 服务端解密（把 aeskey 发给 CDN 服务，由对方解密后返回原图）。
pub fn is_cdn_local_decrypt() -> bool {
    read_cdn_settings()
        .get("localDecrypt")
        .and_then(|e| e.as_bool())
        .unwrap_or(true)
}

pub fn set_cdn_local_decrypt(local: bool) {
    let mut v = read_cdn_settings();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("localDecrypt".to_string(), local.into());
    }
    write_cdn_settings(&v);
}
