// ============================================================
// SQLite 表浏览 — 值转换
// 自 sql_browse.rs 拆分：单元格读取、JSON ↔ SQL 值、BLOB 预览。
// ============================================================

use rusqlite::{params, Connection};

use super::{friendly_db_error, safe_name};

/// 读取某行某列的原始值（用于查看完整 BLOB / 文本内容）
pub fn read_cell(
    conn: &Connection,
    table: &str,
    rowid: i64,
    column: &str,
) -> Result<rusqlite::types::Value, String> {
    let sql = format!(
        "SELECT {} FROM {} WHERE rowid = ?1",
        safe_name(column),
        safe_name(table)
    );
    conn.query_row(&sql, params![rowid], |r| {
        r.get::<_, rusqlite::types::Value>(0)
    })
    .map_err(|e| format!("读取单元格失败: {}", friendly_db_error(&e)))
}

/// 单元格值 → JSON；BLOB 返回 base64（完整）+ hex 预览（前 256 字节）+ mime
pub fn cell_value_to_json(value: &rusqlite::types::Value) -> serde_json::Value {
    match value {
        rusqlite::types::Value::Null => serde_json::json!({ "kind": "null" }),
        rusqlite::types::Value::Integer(n) => {
            serde_json::json!({ "kind": "text", "text": n.to_string() })
        }
        rusqlite::types::Value::Real(f) => {
            let s = if f.fract() == 0.0 && f.is_finite() {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            };
            serde_json::json!({ "kind": "text", "text": s })
        }
        rusqlite::types::Value::Text(t) => serde_json::json!({ "kind": "text", "text": t }),
        rusqlite::types::Value::Blob(b) => {
            use base64::Engine as _;
            let mime = guess_mime(b);
            let preview_n = b.len().min(256);
            let hex_preview: Vec<String> = b[..preview_n]
                .iter()
                .map(|x| format!("{:02X}", x))
                .collect();
            serde_json::json!({
                "kind": "blob",
                "length": b.len(),
                "base64": base64::engine::general_purpose::STANDARD.encode(b),
                "mime": mime,
                "is_image": mime.starts_with("image/"),
                "hex_preview": hex_preview.join(" "),
            })
        }
    }
}

/// JSON 值 → SQLite 绑定值（供 CRUD 使用，保留数值/布尔/空类型语义）
pub fn json_to_sql_value(v: &serde_json::Value) -> rusqlite::types::Value {
    match v {
        serde_json::Value::Null => rusqlite::types::Value::Null,
        serde_json::Value::Bool(b) => rusqlite::types::Value::Integer(if *b { 1 } else { 0 }),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                rusqlite::types::Value::Integer(i)
            } else if let Some(u) = n.as_u64() {
                i64::try_from(u)
                    .map(rusqlite::types::Value::Integer)
                    .unwrap_or_else(|_| rusqlite::types::Value::Text(u.to_string()))
            } else {
                rusqlite::types::Value::Real(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        // 数组/对象等复合值以 JSON 文本形式保存
        other => rusqlite::types::Value::Text(other.to_string()),
    }
}

/// BLOB 转 hex；超大 BLOB 截断预览，避免撑爆 IPC 消息
pub fn blob_to_preview(b: &[u8]) -> String {
    const PREVIEW_BYTES: usize = 128;
    if b.len() > PREVIEW_BYTES {
        format!("{}…[{} bytes]", hex::encode(&b[..PREVIEW_BYTES]), b.len())
    } else {
        hex::encode(b)
    }
}

/// 根据魔数推断 MIME 类型
fn guess_mime(bytes: &[u8]) -> String {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return "image/png".into();
    }
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return "image/jpeg".into();
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return "image/gif".into();
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(&b"WEBP"[..]) {
        return "image/webp".into();
    }
    if bytes.starts_with(b"BM") {
        return "image/bmp".into();
    }
    if bytes.starts_with(b"%PDF") {
        return "application/pdf".into();
    }
    if bytes.len() >= 12 && bytes.get(4..8) == Some(&b"ftyp"[..]) {
        return "video/mp4".into();
    }
    "application/octet-stream".into()
}
