//! 头像模块 - 对应 PC 微信用户头像显示
//!
//! 头像来源（按优先级）：
//! 1. `head_image/head_image.db` 的 `head_image` 表（image_buffer 原始图片字节）
//! 2. 通讯录 `small_head_url` / `big_head_url`（远程 URL）
//!
//! 返回 JSON：`{ "kind": "data" | "url" | "none", "data": "data:image/jpeg;base64,..." | "https://..." }`

use super::common;
use std::path::Path;

// ============ 轻量 Base64 编码（避免额外依赖）============

const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 编码（标准字母表，带填充）
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_CHARS[(n >> 18) as usize & 63] as char);
        out.push(B64_CHARS[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64_CHARS[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_CHARS[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// 探测图片格式
fn sniff_image_format(data: &[u8]) -> &'static str {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpeg"
    } else if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "png"
    } else if data.starts_with(b"GIF8") {
        "gif"
    } else if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WEBP" {
        "webp"
    } else {
        "jpeg"
    }
}

/// 从 head_image.db 读取头像（与 PC 微信头像库一致）
fn avatar_from_head_image_db(decrypted_dir: &Path, username: &str) -> Option<String> {
    let db_path = decrypted_dir.join("head_image").join("head_image.db");
    if !db_path.exists() {
        return None;
    }
    let conn = common::open_readonly_db(&db_path).ok()?;
    if !common::table_exists(&conn, "head_image") {
        return None;
    }
    let blob: Option<Vec<u8>> = conn
        .query_row(
            "SELECT image_buffer FROM head_image WHERE username = ?1 \
             ORDER BY update_time DESC LIMIT 1",
            rusqlite::params![username],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    let data = blob?;
    if data.len() < 16 {
        return None;
    }
    let fmt = sniff_image_format(&data);
    Some(format!(
        "data:image/{};base64,{}",
        fmt,
        base64_encode(&data)
    ))
}

/// 从通讯录获取头像 URL
fn avatar_url_from_contact(decrypted_dir: &Path, username: &str) -> Option<String> {
    let db_path = decrypted_dir.join("contact").join("contact.db");
    if !db_path.exists() {
        return None;
    }
    let conn = common::open_readonly_db(&db_path).ok()?;
    if !common::table_exists(&conn, "contact") {
        return None;
    }
    let cols = common::table_columns(&conn, "contact");
    let has = |c: &str| cols.iter().any(|x| x == c);
    if !has("username") {
        return None;
    }
    let small = if has("small_head_url") {
        "small_head_url"
    } else {
        "NULL"
    };
    let big = if has("big_head_url") {
        "big_head_url"
    } else {
        "NULL"
    };
    let sql = format!(
        "SELECT COALESCE(NULLIF({}, ''), {}) FROM contact WHERE username = ?1 LIMIT 1",
        small, big
    );
    conn.query_row(&sql, rusqlite::params![username], |row| {
        row.get::<_, Option<String>>(0)
    })
    .ok()
    .flatten()
    .filter(|s| !s.is_empty())
}

/// 获取用户头像
///
/// * `decrypted_dir` 解密数据库目录
/// * `_wechat_base_dir` 微信账号目录（保留参数，未来可扩展本地缓存解密）
pub fn get_user_avatar(
    decrypted_dir: &Path,
    _wechat_base_dir: &Path,
    username: &str,
    _aes_key: Option<&[u8]>,
    _xor_key: u8,
) -> serde_json::Value {
    // 1. head_image.db（与 PC 微信头像库一致）
    if let Some(data) = avatar_from_head_image_db(decrypted_dir, username) {
        return serde_json::json!({ "kind": "data", "data": data });
    }
    // 2. 远程 URL
    if let Some(url) = avatar_url_from_contact(decrypted_dir, username) {
        return serde_json::json!({ "kind": "url", "data": url });
    }
    serde_json::json!({ "kind": "none", "data": "" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(&[0xFF, 0xD8, 0xFF]), "/9j/");
    }

    #[test]
    fn test_sniff_image_format() {
        assert_eq!(sniff_image_format(&[0xFF, 0xD8, 0xFF, 0xE0]), "jpeg");
        assert_eq!(sniff_image_format(&[0x89, 0x50, 0x4E, 0x47]), "png");
        assert_eq!(sniff_image_format(b"GIF89a"), "gif");
    }
}
