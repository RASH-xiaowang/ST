//! 收藏模块 - 对应 PC 微信「收藏」
//!
//! 数据来源：`favorite/favorite.db`
//! - `fav_db_item`      收藏条目（type / update_time / content XML / fromusr / realchatname）
//! - `fav_tag_db_item`  收藏标签
//!
//! 收藏类型（微信 fav type）：
//!   1=文本  2=图片  3=语音  4=视频  5=链接  6=位置
//!   8=文件  14=笔记  16=聊天记录  18=笔记(新)
//!
//! 与 PC 微信一致的逻辑：
//! - 按 update_time 降序
//! - 解析 content XML 得到标题/描述
//! - 来源（fromusr / realchatname）经通讯录解析

use super::common;
use super::contacts;
use rusqlite::OptionalExtension;
use serde::Serialize;
use std::path::Path;

/// 收藏条目行（SELECT：local_id, type, update_time, content, fromusr, realchatname）
struct FavoriteRow(
    #[allow(dead_code)] // local_id 列随 SELECT 保留，查询按它定位但结果不需要
    i64,
    i64,
    i64,
    Option<Vec<u8>>,
    String,
    String,
);

/// 收藏条目
#[derive(Debug, Clone, Serialize)]
pub struct FavoriteEntry {
    pub local_id: i64,
    /// 类型码
    #[serde(rename = "type")]
    pub fav_type: i64,
    /// 类型中文名
    pub type_label: String,
    /// 标题
    pub title: String,
    /// 描述/正文
    pub desc: String,
    /// 链接 URL（链接类）
    pub url: String,
    /// 收藏时间（Unix 秒）
    pub ts: i64,
    /// 时间显示
    pub time: String,
    /// 来源（谁/哪个群）
    pub source: String,
    /// 同步状态
    pub sync_status: i64,
    /// 语音收藏的 server_id（type=3 时有效，用于播放）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_server_id: Option<i64>,
    /// 图片收藏的图片 md5（type=2 时有效，用于显示）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_md5: Option<String>,
}

/// 收藏类型标签
fn fav_type_label(t: i64) -> &'static str {
    match t {
        1 => "文本",
        2 => "图片",
        3 => "语音",
        4 => "视频",
        5 => "链接",
        6 => "位置",
        7 => "音乐",
        8 => "文件",
        14 => "聊天记录",
        16 => "商品",
        18 => "笔记",
        19 => "小程序",
        20 => "视频号",
        _ => "其他",
    }
}

/// 解码收藏正文中的 XML/HTML 实体（保留换行：`&#x0A;` / `&#10;` → `\n`）
fn decode_fav_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let end = after.find(';').map(|e| e + 1).unwrap_or(after.len());
        let ent = &after[..end];
        let ch = match ent {
            "&amp;" => Some('&'),
            "&lt;" => Some('<'),
            "&gt;" => Some('>'),
            "&quot;" => Some('"'),
            "&apos;" => Some('\''),
            "&#x0A;" | "&#10;" | "&#xa;" | "&#xA;" => Some('\n'),
            "&#x0D;" | "&#13;" | "&#xd;" | "&#xD;" => Some('\r'),
            "&#x09;" | "&#9;" | "&#x9;" => Some('\t'),
            _ => None,
        };
        match ch {
            Some(c) => out.push(c),
            None => out.push_str(ent),
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// 解析收藏 content XML 为详情 JSON（供收藏详情查看）
///
/// 结构：`<favitem type="N"><desc>…</desc><datalist><dataitem datatype="M" dataid="…">…`
/// - 文本/笔记：完整正文（保留换行）
/// - 图片：全部 dataitem 的 md5（dataid / fullmd5）
/// - 语音：server_id（sourceid/msgid）用于播放
/// - 视频：时长
/// - 链接：标题 + URL
/// - 位置：名称 + 标签
/// - 文件：名称 + 扩展名 + 大小
/// - 聊天记录（type=14）：合并的各条 dataitem 内容
pub fn parse_fav_detail(fav_type: i64, xml: &str) -> serde_json::Value {
    let mut detail = serde_json::Map::new();
    detail.insert("type".into(), serde_json::json!(fav_type));
    detail.insert(
        "type_label".into(),
        serde_json::json!(fav_type_label(fav_type)),
    );

    // 完整正文（文本/笔记等，保留换行）
    let desc_raw = common::xml_tag_text(xml, "desc").unwrap_or_default();
    let title = common::xml_tag_text(xml, "title").unwrap_or_default();
    let text = decode_fav_text(&desc_raw);
    detail.insert("text".into(), serde_json::json!(text));
    detail.insert("title".into(), serde_json::json!(title));

    // 逐个 dataitem 解析
    let mut images: Vec<String> = Vec::new();
    let mut voice_server_id: Option<i64> = None;
    let mut video_duration: f64 = 0.0;
    let mut video_md5 = String::new();
    let mut link_url = String::new();
    let mut link_title = String::new();
    let mut location_name = String::new();
    let mut location_label = String::new();
    let mut file_name = String::new();
    let mut file_ext = String::new();
    let mut file_size: i64 = 0;
    let mut chat_items: Vec<serde_json::Value> = Vec::new();

    let mut pos = 0;
    while let Some(start_rel) = xml[pos..].find("<dataitem") {
        let body = &xml[pos + start_rel..];
        // 绝对位置推进：无论是否有闭合标签都必须单调前进，避免死循环
        let (item_str, consumed) = match body.find("</dataitem>") {
            Some(e) => (&body[..e + "</dataitem>".len()], e + "</dataitem>".len()),
            None => (body, body.len()),
        };
        let datatype = common::xml_tag_attr(item_str, "dataitem", "datatype")
            .and_then(|s| s.trim().parse::<i64>().ok())
            .unwrap_or(0);
        let dataid = common::xml_tag_attr(item_str, "dataitem", "dataid")
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_default();
        let md5 = common::xml_tag_text(item_str, "fullmd5")
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| dataid.clone());

        match datatype {
            1 => {
                // 文本（聊天记录内的文本条目）
                let t = common::xml_tag_text(item_str, "datadesc")
                    .or_else(|| common::xml_tag_text(item_str, "datatitle"))
                    .unwrap_or_default();
                let t = decode_fav_text(&t);
                if !t.is_empty() {
                    chat_items.push(serde_json::json!({ "type": "text", "text": t }));
                }
            }
            2 => {
                if !md5.is_empty() && !images.contains(&md5) {
                    images.push(md5.clone());
                }
                let t = common::xml_tag_text(item_str, "datadesc")
                    .or_else(|| common::xml_tag_text(item_str, "datatitle"))
                    .unwrap_or_default();
                if !t.is_empty() {
                    chat_items
                        .push(serde_json::json!({ "type": "text", "text": decode_fav_text(&t) }));
                }
            }
            3 => {
                // 语音
                if voice_server_id.is_none() {
                    voice_server_id = common::xml_tag_text(xml, "sourceid")
                        .or_else(|| common::xml_tag_text(xml, "msgid"))
                        .and_then(|s| s.trim().parse::<i64>().ok());
                }
            }
            4 => {
                video_duration = common::xml_tag_text(item_str, "duration")
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .unwrap_or(0.0);
                video_md5 = md5.clone();
            }
            5 => {
                link_title = common::xml_tag_text(item_str, "datatitle")
                    .or_else(|| common::xml_tag_text(item_str, "pagetitle"))
                    .unwrap_or_default();
                link_url = common::xml_tag_text(item_str, "stream_weburl")
                    .or_else(|| common::xml_tag_text(item_str, "url"))
                    .map(|u| u.replace("&amp;", "&"))
                    .unwrap_or_default();
            }
            8 => {
                file_name = common::xml_tag_text(item_str, "datatitle").unwrap_or_default();
                file_ext = common::xml_tag_text(item_str, "datafmt").unwrap_or_default();
                file_size = common::xml_tag_text(item_str, "fullsize")
                    .and_then(|s| s.trim().parse::<i64>().ok())
                    .unwrap_or(0);
            }
            19 | 36 => {
                let t = common::xml_tag_text(item_str, "datatitle").unwrap_or_default();
                let d = common::xml_tag_text(item_str, "datadesc").unwrap_or_default();
                if !t.is_empty() || !d.is_empty() {
                    chat_items.push(serde_json::json!({
                        "type": "link",
                        "text": decode_fav_text(&t),
                        "des": decode_fav_text(&d),
                    }));
                }
            }
            _ => {}
        }
        pos += start_rel + consumed;
    }

    // 位置信息在 <locitem> 内（<poiname>/<label>），不受 dataitem 约束
    if location_name.is_empty() {
        location_name = common::xml_nested_text(xml, "locitem", "poiname")
            .or_else(|| common::xml_tag_text(xml, "poiname"))
            .unwrap_or_default();
    }
    if location_label.is_empty() {
        location_label = common::xml_nested_text(xml, "locitem", "label")
            .or_else(|| common::xml_tag_text(xml, "label"))
            .unwrap_or_default();
    }

    detail.insert("images".into(), serde_json::json!(images));
    if let Some(svr) = voice_server_id {
        detail.insert("voice_server_id".into(), serde_json::json!(svr));
    }
    if video_duration > 0.0 || !video_md5.is_empty() {
        detail.insert(
            "video".into(),
            serde_json::json!({ "duration": video_duration, "md5": video_md5 }),
        );
    }
    if !link_url.is_empty() || !link_title.is_empty() {
        detail.insert(
            "link".into(),
            serde_json::json!({ "url": link_url, "title": link_title }),
        );
    }
    if !location_name.is_empty() || !location_label.is_empty() {
        detail.insert(
            "location".into(),
            serde_json::json!({ "name": location_name, "label": location_label }),
        );
    }
    if !file_name.is_empty() || !file_ext.is_empty() || file_size > 0 {
        detail.insert(
            "file".into(),
            serde_json::json!({ "name": file_name, "ext": file_ext, "size": file_size }),
        );
    }
    if !chat_items.is_empty() {
        detail.insert("items".into(), serde_json::json!(chat_items));
    }
    serde_json::Value::Object(detail)
}

/// 读取单条收藏的详情（含完整正文/图片/语音/链接/位置/文件/聊天记录）
pub fn get_favorite_detail(
    decrypted_dir: &Path,
    local_id: i64,
) -> Result<serde_json::Value, String> {
    let db_path = decrypted_dir.join("favorite").join("favorite.db");
    if !db_path.exists() {
        return Err(format!("收藏数据库未解密: {}", db_path.display()));
    }
    let conn = common::open_readonly_db(&db_path).map_err(|e| format!("打开失败: {}", e))?;
    if !common::table_exists(&conn, "fav_db_item") {
        return Err("fav_db_item 表不存在".to_string());
    }
    let cols = common::table_columns(&conn, "fav_db_item");
    let has = |c: &str| cols.iter().any(|x| x == c);
    if !has("local_id") || !has("content") {
        return Err("fav_db_item 缺少必要列".to_string());
    }
    let sel = |c: &str, dft: &str| {
        if has(c) {
            format!("\"{}\"", c)
        } else {
            dft.to_string()
        }
    };
    let sql = format!(
        "SELECT {lid}, {typ}, {ut}, {content}, {fromusr}, {chatname} FROM fav_db_item WHERE {lid} = ?1 LIMIT 1",
        lid = sel("local_id", "rowid"),
        typ = sel("type", "0"),
        ut = sel("update_time", "0"),
        content = sel("content", "NULL"),
        fromusr = sel("fromusr", "NULL"),
        chatname = sel("realchatname", "NULL"),
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("查询失败: {}", e))?;
    let row: Option<FavoriteRow> = stmt
        .query_row(rusqlite::params![local_id], |r| {
            Ok(FavoriteRow(
                r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                common::get_bytes(r, 3),
                r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                r.get::<_, Option<String>>(5)?.unwrap_or_default(),
            ))
        })
        .optional()
        .ok()
        .flatten();
    let Some(FavoriteRow(_, fav_type, ts, content, fromusr, chatname)) = row else {
        return Err(format!("收藏不存在: local_id={}", local_id));
    };
    let xml = content
        .as_deref()
        .map(common::decode_blob_text)
        .unwrap_or_default();
    let mut detail = parse_fav_detail(fav_type, &xml);
    let contact_db = decrypted_dir.join("contact").join("contact.db");
    let contact_names = contacts::load_display_names(&contact_db);
    let source = if !chatname.is_empty() {
        contact_names.get(&chatname).cloned().unwrap_or(chatname)
    } else if !fromusr.is_empty() {
        contact_names.get(&fromusr).cloned().unwrap_or(fromusr)
    } else {
        String::new()
    };
    if let Some(obj) = detail.as_object_mut() {
        obj.insert("local_id".into(), serde_json::json!(local_id));
        obj.insert("ts".into(), serde_json::json!(ts));
        obj.insert(
            "time".into(),
            serde_json::json!(common::format_date_time(ts)),
        );
        obj.insert("source".into(), serde_json::json!(source));
    }
    Ok(detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实数据：收藏详情应能解析出正文/图片/语音/链接/文件等内容
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_favorite_detail() {
        let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let list = get_favorites(&cfg.decrypted_dir, 200).expect("读取收藏列表失败");
        let items = list
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if items.is_empty() {
            eprintln!("无收藏数据，跳过");
            return;
        }
        let mut any_content = false;
        let mut seen_types: std::collections::HashSet<i64> = std::collections::HashSet::new();
        for it in items.iter() {
            let fav_type = it.get("type").and_then(|v| v.as_i64()).unwrap_or(0);
            // 每种类型只看一条，避免重复输出
            if !seen_types.insert(fav_type) {
                continue;
            }
            let lid = it.get("local_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let d = get_favorite_detail(&cfg.decrypted_dir, lid).expect("读取详情失败");
            let text = d
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let images = d
                .get("images")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let has_voice = d.get("voice_server_id").is_some();
            let has_link = d.get("link").is_some();
            let has_file = d.get("file").is_some();
            let has_loc = d.get("location").is_some();
            let items_n = d
                .get("items")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            eprintln!(
                "lid={} {} title={:?} text={} imgs={} voice={} link={} file={} loc={} items={}",
                lid,
                d.get("type_label").and_then(|v| v.as_str()).unwrap_or(""),
                d.get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .chars()
                    .take(24)
                    .collect::<String>(),
                text.chars().count(),
                images,
                has_voice,
                has_link,
                has_file,
                has_loc,
                items_n
            );
            if !text.is_empty()
                || images > 0
                || has_voice
                || has_link
                || has_file
                || has_loc
                || items_n > 0
            {
                any_content = true;
            }
        }
        if !any_content {
            eprintln!("收藏详情未解析出内容，跳过");
            return;
        }
        eprintln!("覆盖收藏类型: {:?}", seen_types.iter().collect::<Vec<_>>());
    }
}

/// 解析收藏 content XML → (title, desc, url)
fn parse_fav_content(fav_type: i64, xml: &str) -> (String, String, String) {
    if xml.is_empty() {
        return (String::new(), String::new(), String::new());
    }
    match fav_type {
        1 => {
            // 文本收藏：内容即正文（可能直接是文本，也可能包在 XML 里）
            if xml.contains('<') {
                let desc = common::xml_tag_text(xml, "desc")
                    .or_else(|| common::xml_tag_text(xml, "title"))
                    .unwrap_or_else(|| common::strip_xml_tags(xml).trim().to_string());
                (String::new(), desc, String::new())
            } else {
                (String::new(), xml.to_string(), String::new())
            }
        }
        5 => {
            // 链接
            let title = common::xml_tag_text(xml, "title").unwrap_or_default();
            let desc = common::xml_tag_text(xml, "desc").unwrap_or_default();
            let url = common::xml_tag_text(xml, "url")
                .map(|u| u.replace("&amp;", "&"))
                .unwrap_or_default();
            (title, desc, url)
        }
        14 | 18 => {
            // 笔记：<recordinfo><title>/<desc>
            let title = common::xml_nested_text(xml, "recordinfo", "title")
                .or_else(|| common::xml_tag_text(xml, "title"))
                .unwrap_or_default();
            let desc = common::xml_nested_text(xml, "recordinfo", "desc")
                .or_else(|| common::xml_tag_text(xml, "desc"))
                .unwrap_or_default();
            (title, desc, String::new())
        }
        6 => {
            // 位置
            let title = common::xml_tag_attr(xml, "location", "poiname")
                .or_else(|| common::xml_tag_text(xml, "title"))
                .unwrap_or_default();
            let desc = common::xml_tag_attr(xml, "location", "label")
                .or_else(|| common::xml_tag_text(xml, "desc"))
                .unwrap_or_default();
            (title, desc, String::new())
        }
        _ => {
            let title = common::xml_tag_text(xml, "title").unwrap_or_default();
            let desc = common::xml_tag_text(xml, "desc").unwrap_or_default();
            (title, desc, String::new())
        }
    }
}

/// 读取收藏列表
pub fn get_favorites(decrypted_dir: &Path, limit: usize) -> Result<serde_json::Value, String> {
    let db_path = decrypted_dir.join("favorite").join("favorite.db");
    if !db_path.exists() {
        return Err(format!("收藏数据库未解密: {}", db_path.display()));
    }
    let conn = common::open_readonly_db(&db_path).map_err(|e| format!("打开失败: {}", e))?;
    if !common::table_exists(&conn, "fav_db_item") {
        return Err("fav_db_item 表不存在".to_string());
    }

    let contact_db = decrypted_dir.join("contact").join("contact.db");
    let contact_names = contacts::load_display_names(&contact_db);

    let cols = common::table_columns(&conn, "fav_db_item");
    let has = |c: &str| cols.iter().any(|x| x == c);
    let sel = |c: &str, dft: &str| {
        if has(c) {
            format!("\"{}\"", c)
        } else {
            dft.to_string()
        }
    };
    let order_col = if has("update_time") {
        "update_time"
    } else {
        "local_id"
    };
    let sql = format!(
        "SELECT {lid}, {typ}, {ut}, {content}, {fromusr}, {chatname}, {sync} \
         FROM fav_db_item ORDER BY {ord} DESC LIMIT ?1",
        lid = sel("local_id", "rowid"),
        typ = sel("type", "0"),
        ut = sel("update_time", "0"),
        content = sel("content", "NULL"),
        fromusr = sel("fromusr", "NULL"),
        chatname = sel("realchatname", "NULL"),
        sync = sel("sync_status", "0"),
        ord = order_col,
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("查询失败: {}", e))?;
    let rows = stmt
        .query_map(rusqlite::params![limit as i64], |row| {
            // 全部按 Option 读取：任何字段为 NULL 都不应丢弃整条收藏
            // content 字段在收藏数据中可能为 TEXT 或 BLOB，
            // 直接 Vec<u8> 读取会在 TEXT 行上报错导致整行被丢弃
            Ok::<_, rusqlite::Error>((
                row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                common::get_bytes(row, 3),
                row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                row.get::<_, Option<i64>>(6)?.unwrap_or(0),
            ))
        })
        .map_err(|e| format!("读取失败: {}", e))?;

    let mut favorites = Vec::new();
    for r in rows.flatten() {
        let fav_type = r.1;
        let xml =
            r.3.as_deref()
                .map(common::decode_blob_text)
                .unwrap_or_default();
        let (title, desc, url) = parse_fav_content(fav_type, &xml);

        // 来源：群名优先，其次发送者
        let source = if !r.5.is_empty() {
            contact_names.get(&r.5).cloned().unwrap_or(r.5.clone())
        } else if !r.4.is_empty() {
            contact_names.get(&r.4).cloned().unwrap_or(r.4.clone())
        } else {
            String::new()
        };

        favorites.push(FavoriteEntry {
            local_id: r.0,
            fav_type,
            type_label: fav_type_label(fav_type).to_string(),
            title,
            desc,
            url,
            ts: r.2,
            time: common::format_date_time(r.2),
            source,
            sync_status: r.6,
            voice_server_id: if fav_type == 3 {
                common::xml_tag_text(&xml, "sourceid")
                    .and_then(|s| s.trim().parse::<i64>().ok())
                    .or_else(|| {
                        common::xml_tag_text(&xml, "msgid")
                            .and_then(|s| s.trim().parse::<i64>().ok())
                    })
            } else {
                None
            },
            image_md5: if fav_type == 2 {
                common::xml_tag_attr(&xml, "dataitem", "dataid")
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
            } else {
                None
            },
        });
    }

    // 收藏标签
    let mut tags = Vec::new();
    if common::table_exists(&conn, "fav_tag_db_item") {
        if let Some((cols, rows)) = common::dump_table(&conn, "fav_tag_db_item", None, 200) {
            for row in rows {
                let mut obj = serde_json::Map::new();
                for (i, c) in cols.iter().enumerate() {
                    obj.insert(
                        c.clone(),
                        row.get(i).cloned().unwrap_or(serde_json::Value::Null),
                    );
                }
                tags.push(serde_json::Value::Object(obj));
            }
        }
    }

    Ok(serde_json::json!({
        "items": favorites,
        "tags": tags,
    }))
}
