// ============================================================
// 微信 CDN 原图下载 — 消息 XML 解析域
// 自 cdn_image.rs 拆分：图片消息查询与原图字段解析。
// ============================================================

use std::path::{Path, PathBuf};

use crate::wechat::modules::common;

/// 图片消息行（SELECT：local_type, message_content, compress_content）
struct CdnMediaRow(i64, Option<Vec<u8>>, Option<Vec<u8>>);

/// 从图片消息 XML 中抽取 CDN 下载所需信息。
/// 返回 (fileid, aeskey, 是否含 cdnbigimgurl)。
/// 仅含 cdnmidimgurl 的消息在 c3o.re 上不响应（实测全部超时），
/// 由调用方据此跳过 CDN，避免每张失效图拖慢加载。
fn extract_cdn_info_from_xml(xml: &str) -> Option<(String, String, bool)> {
    let big = extract_xml_value(xml, "cdnbigimgurl")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let fileid = big
        .clone()
        .or_else(|| extract_xml_value(xml, "cdnmidimgurl"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;
    let aeskey = extract_xml_value(xml, "aeskey")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    Some((fileid, aeskey, big.is_some()))
}

/// 轻量 XML 取值：优先属性 `name="..."`，其次标签 `<name>...</name>`（含 CDATA）
pub(crate) fn extract_xml_value(xml: &str, name: &str) -> Option<String> {
    // 属性形式：name="value" 或 name='value'
    for (open, close) in [("\"", "\""), ("'", "'")] {
        let pat = format!("{}={}", name, open);
        if let Some(i) = xml.find(&pat) {
            let rest = &xml[i + pat.len()..];
            if let Some(end) = rest.find(close) {
                let v = rest[..end].trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    // 标签形式：<name><![CDATA[v]]></name> 或 <name>v</name>
    let open_tag = format!("<{}>", name);
    let close_tag = format!("</{}>", name);
    if let Some(i) = xml.find(&open_tag) {
        let rest = &xml[i + open_tag.len()..];
        if let Some(end) = rest.find(&close_tag) {
            let v = rest[..end].trim();
            if !v.is_empty() {
                let v = v
                    .strip_prefix("<![CDATA[")
                    .and_then(|s| s.strip_suffix("]]>"))
                    .unwrap_or(v)
                    .trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// 从解密消息库查图片消息（local_type=3）的 XML 文本
fn find_image_message_xml(decrypted_dir: &Path, username: &str, local_id: i64) -> Option<String> {
    let table = common::msg_table_name(username);
    let msg_dir = decrypted_dir.join("message");
    let Ok(entries) = std::fs::read_dir(&msg_dir) else {
        return None;
    };
    let mut dbs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e == "db")
                .unwrap_or(false)
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| {
                        n.starts_with("message_") && !n.contains("fts") && !n.contains("resource")
                    })
                    .unwrap_or(false)
        })
        .collect();
    dbs.sort();
    for db in dbs {
        let Ok(conn) =
            rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        // 表可能不存在，先确认
        let has_table = conn
            .prepare(&format!("SELECT 1 FROM \"{}\" LIMIT 1", table))
            .is_ok();
        if !has_table {
            drop(conn);
            continue;
        }
        let sql = format!(
            "SELECT local_type, message_content, compress_content FROM \"{}\" \
             WHERE local_id = ?1 ORDER BY create_time DESC LIMIT 1",
            table
        );
        let row: Option<CdnMediaRow> = conn.prepare(&sql).ok().and_then(|mut stmt| {
            stmt.query_row(rusqlite::params![local_id], |r| {
                Ok(CdnMediaRow(
                    r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                    common::get_bytes(r, 1),
                    common::get_bytes(r, 2),
                ))
            })
            .ok()
        });
        drop(conn);
        let Some(CdnMediaRow(local_type, content, compressed)) = row else {
            continue;
        };
        if local_type != 3 {
            continue;
        }
        let xml: String = content
            .or(compressed)
            .map(|b| common::decode_blob_text(&b))
            .unwrap_or_default();
        if !xml.is_empty() {
            return Some(xml);
        }
    }
    None
}

/// 按 (username, local_id) 从解密消息库查图片消息 XML，
/// 返回 (fileid, aeskey, 是否含 cdnbigimgurl)
pub fn lookup_image_cdn_info(
    decrypted_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<(String, String, bool)> {
    let xml = find_image_message_xml(decrypted_dir, username, local_id)?;
    extract_cdn_info_from_xml(&xml)
}

/// 图片消息 XML 中的全部 md5 变体（md5 / originsourcemd5 / hdmd5），去重保序。
/// 本地 .dat 可能按其中任意一个命名，主 md5 未命中时依次补查。
pub fn lookup_image_md5_variants(
    decrypted_dir: &Path,
    username: &str,
    local_id: i64,
) -> Vec<String> {
    let mut out = Vec::new();
    let Some(xml) = find_image_message_xml(decrypted_dir, username, local_id) else {
        return out;
    };
    for attr in ["md5", "originsourcemd5", "hdmd5"] {
        if let Some(v) = extract_xml_value(&xml, attr) {
            let v = v.trim().to_lowercase();
            if v.len() == 32 && !out.contains(&v) {
                out.push(v);
            }
        }
    }
    out
}
