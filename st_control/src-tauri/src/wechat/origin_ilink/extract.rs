// ============================================================
// 消息原图官方通道回退 — 消息 XML 解析域
// 自 origin_ilink.rs 拆分：图片消息提取与原图字段解析。
// ============================================================

use std::path::Path;

use rusqlite::OpenFlags;

use crate::wechat::modules::common::{find_db_files, is_message_shard_file, msg_table_name};

use super::OriginSecret;

/// 从解密消息库提取图片消息 XML（message_content 为 zstd 压缩，含发送者前缀）
pub(crate) fn extract_image_xml(
    decrypted_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<String> {
    let table = msg_table_name(username);
    let mut dbs = find_db_files(decrypted_dir, "message_");
    dbs.extend(find_db_files(decrypted_dir, "biz_message_"));
    dbs.sort();
    dbs.dedup();
    dbs.retain(|p| is_message_shard_file(p));

    for db in dbs {
        let conn = rusqlite::Connection::open_with_flags(
            &db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()?;
        let sql = format!(
            "SELECT message_content, compress_content FROM \"{table}\" \
             WHERE local_id = ?1 AND (local_type = 3 OR local_type % 4294967296 = 3) LIMIT 1"
        );
        let Ok(row) = conn.query_row(&sql, rusqlite::params![local_id], |r| {
            Ok((
                r.get::<_, rusqlite::types::Value>(0).ok(),
                r.get::<_, rusqlite::types::Value>(1).ok(),
            ))
        }) else {
            continue;
        };
        let raw = match row.0.or(row.1) {
            Some(rusqlite::types::Value::Blob(b)) => b,
            Some(rusqlite::types::Value::Text(s)) => s.into_bytes(),
            _ => continue,
        };
        let decoded = zstd::stream::decode_all(std::io::Cursor::new(raw)).ok()?;
        if decoded.len() > 2_000_000 {
            return None;
        }
        let xml = String::from_utf8_lossy(&decoded);
        let start = xml.find("<msg")?;
        let end = xml.find("</msg>")? + "</msg>".len();
        if end <= start {
            return None;
        } // 畸形 XML 防护
        let msg = &xml[start..end];
        if msg.contains("cdnbigimgurl") {
            return Some(msg.to_string());
        }
    }
    None
}

/// 解析图片 XML 中的原图字段
pub(crate) fn parse_origin_secret(xml: &str) -> Option<OriginSecret> {
    let attr = |name: &str| -> Option<String> {
        let pat = format!("{name}=\"");
        let i = xml.find(&pat)?;
        let rest = &xml[i + pat.len()..];
        let end = rest.find('"')?;
        let v = &rest[..end];
        (!v.is_empty()).then(|| v.to_string())
    };
    Some(OriginSecret {
        file_id: attr("cdnbigimgurl")?,
        aes_key: attr("aeskey")?,
        md5: attr("md5").unwrap_or_default(),
        original_size: attr("hdlength")?.parse().ok()?,
    })
}
