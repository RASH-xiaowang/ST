//! 朋友圈模块 - 对应 PC 微信「朋友圈」
//!
//! 数据来源：`sns/db_sns/sns.db`
//! - `SnsTimeLine`  朋友圈时间线（tid / user_name / content XML / pack_info_buf）
//!
//! 与 PC 微信一致的逻辑：
//! - 按 tid（服务端 ID，与时间正相关）降序展示
//! - 解析 content XML：正文文字、图片/视频数量、位置、链接标题、发布时间
//! - 作者名经通讯录解析（备注 > 昵称）

use super::common;
use super::contacts;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// 一条朋友圈图片媒体
///
/// 微信朋友圈 XML 中每张图带独立的解密参数：
/// - `thumb`：150px 缩略图（列表网格使用），配 `thumb_token`
/// - `url`：`/0` 原图（查看大图使用），配 `url_token`
/// - `key`：ISAAC-64 解密种子（媒体级，thumb 与 url 共用）
#[derive(Debug, Clone, Serialize)]
pub struct MomentMedia {
    /// 缩略图 URL（150px，列表网格使用）
    pub thumb: String,
    /// 缩略图下载 token
    pub thumb_token: String,
    /// 原图 URL（/0，查看大图使用）
    pub url: String,
    /// 原图下载 token
    pub url_token: String,
    /// 解密 key（XML key 属性，数字字符串；空 = 直链无需解密）
    pub key: String,
    /// 图片内容 MD5（XML url 的 md5 属性）
    pub md5: String,
}

/// 一条朋友圈视频媒体
///
/// 视频动态的 XML 与图片不同：
/// - `<url>` 是视频文件（`snsvideodownload?...dotrans=1/9/11`），token 已内嵌在 URL
/// - `<thumb>` 可能是封面图（`vweixinthumb` 域）或视频本体（video.qq.com 域）
/// - 解密 key 在 `<enc key="...">` 里（不是 media 的 key 属性）
/// - 视频只加密前 128KB；封面若是 vweixinthumb 则整体加密（同为 ISAAC-64）
#[derive(Debug, Clone, Serialize)]
pub struct MomentVideo {
    /// 视频文件 URL（已含 token 参数）
    pub url: String,
    /// 封面 URL（vweixinthumb 图片 或 video.qq.com 视频本体）
    pub thumb: String,
    /// 封面是否为图片（vweixinthumb 域 → 可直接解出封面 JPEG）
    pub thumb_is_image: bool,
    /// 视频解密 key（`<enc key>`）
    pub key: String,
    /// 视频文件 MD5
    pub md5: String,
    /// 视频时长（秒）
    pub duration: f64,
    /// 视频宽
    pub width: u32,
    /// 视频高
    pub height: u32,
}

/// 朋友圈点赞人
#[derive(Debug, Clone, Serialize)]
pub struct MomentLike {
    /// 点赞者 username
    pub username: String,
    /// 点赞者显示名
    pub nickname: String,
}

/// 朋友圈评论
#[derive(Debug, Clone, Serialize)]
pub struct MomentComment {
    /// 评论者 username
    pub username: String,
    /// 评论者显示名
    pub nickname: String,
    /// 被回复者 username（回复他人评论时）
    pub to_username: String,
    /// 被回复者显示名
    pub to_nickname: String,
    /// 评论内容
    pub content: String,
    /// 评论时间（Unix 秒）
    pub ts: i64,
    /// 时间显示
    pub time: String,
}

/// 一条朋友圈
#[derive(Debug, Clone, Serialize)]
pub struct MomentEntry {
    /// 朋友圈 ID（tid）
    pub tid: String,
    /// 作者 username
    pub username: String,
    /// 作者显示名
    pub author: String,
    /// 正文文字
    pub text: String,
    /// 发布时间（Unix 秒，来自 XML createTime；0 表示未知）
    pub ts: i64,
    /// 时间显示
    pub time: String,
    /// 媒体数量
    pub media_count: usize,
    /// 媒体类型描述（如 "图片×3" / "视频"）
    pub media_desc: String,
    /// 图片媒体列表（从 XML 提取，含解密所需的 key/token）
    pub images: Vec<MomentMedia>,
    /// 视频媒体列表（视频动态）
    pub videos: Vec<MomentVideo>,
    /// 位置名
    pub location: String,
    /// 链接标题（分享链接时）
    pub link_title: String,
    /// 是否我自己发的
    pub is_self: bool,
    /// 点赞人列表
    pub likes: Vec<MomentLike>,
    /// 评论列表
    pub comments: Vec<MomentComment>,
}

/// 提取标签文本，兼容带属性的开标签 `<tag attr="...">text</tag>`
///
/// 微信朋友圈 XML 中 `<thumb type="1" key="..." token="...">URL</thumb>`
/// 这类标签普遍带属性，现有精确匹配 `<thumb>` 会失败，导致图片 URL 提取不到。
fn xml_tag_text_loose(xml: &str, tag: &str) -> Option<String> {
    let open_exact = format!("<{}>", tag);
    let open_attr = format!("<{} ", tag);
    let start = xml.find(&open_exact).or_else(|| xml.find(&open_attr))?;
    let content_start = if xml[start..].starts_with(&open_exact) {
        start + open_exact.len()
    } else {
        let tag_end = xml[start..].find('>')?;
        start + tag_end + 1
    };
    let close = format!("</{}>", tag);
    let end = xml[content_start..].find(&close)?;
    Some(xml[content_start..content_start + end].to_string())
}

/// XML 实体反转义（URL 中的 `&amp;` → `&` 等）
fn unescape_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
}

/// 提取开标签 `<tag ...>` 的属性值（如 `<thumb type="1" key="..." token="...">`）
fn xml_open_attr(inner: &str, tag: &str, attr: &str) -> Option<String> {
    let open = format!("<{} ", tag);
    let start = inner.find(&open)?;
    let tag_end = inner[start..].find('>')?;
    let tag_str = &inner[start..start + tag_end];
    let search = format!("{}=\"", attr);
    let a = tag_str.find(&search)?;
    let v = a + search.len();
    let e = tag_str[v..].find('"')?;
    Some(tag_str[v..v + e].to_string())
}

/// 按标签名优先级提取文本与 token（首个非空标签生效）
fn media_text_and_token(inner: &str, tags: &[&str]) -> (String, String) {
    for t in tags {
        if let Some(text) = xml_tag_text_loose(inner, t) {
            let text = unescape_xml_entities(&text).trim().to_string();
            if !text.is_empty() {
                let token = xml_open_attr(inner, t, "token").unwrap_or_default();
                return (text, token);
            }
        }
    }
    (String::new(), String::new())
}

/// 判断是否为视频 URL（视频媒体不放入图片列表）
fn is_video_url(u: &str) -> bool {
    let l = u.to_ascii_lowercase();
    l.contains("snsvideodownload") || l.contains("video.qq.com") || l.contains(".mp4")
}

/// 从 XML 中提取视频媒体（snsvideodownload / mp4）
fn extract_videos(xml: &str) -> Vec<MomentVideo> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < xml.len() {
        let rest = &xml[pos..];
        let tag_start = match rest.find("<media>").or_else(|| rest.find("<media ")) {
            Some(i) => i,
            None => break,
        };
        let tag_start = pos + tag_start;
        let tag_close = match xml[tag_start..].find('>') {
            Some(i) => tag_start + i,
            None => break,
        };
        let media_close = match xml[tag_close..].find("</media>") {
            Some(i) => tag_close + i,
            None => break,
        };
        let inner = &xml[tag_close + 1..media_close];

        let (url, _) = media_text_and_token(inner, &["url", "Url", "cdnUrl", "cdnurl"]);
        if !is_video_url(&url) {
            pos = media_close + 8;
            continue;
        }
        let (thumb, _) = media_text_and_token(inner, &["thumb", "Thumb", "thumbUrl", "thumburl"]);
        let key = xml_open_attr(inner, "enc", "key")
            .or_else(|| xml_open_attr(inner, "thumb", "key"))
            .or_else(|| xml_open_attr(inner, "url", "key"))
            .unwrap_or_default();
        let md5 = xml_open_attr(inner, "url", "md5").unwrap_or_default();
        let duration = xml_tag_text_loose(inner, "videoDuration")
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
        let width = xml_open_attr(inner, "size", "width")
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(0);
        let height = xml_open_attr(inner, "size", "height")
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(0);
        let thumb_is_image = thumb.to_ascii_lowercase().contains("vweixinthumb");

        if !url.is_empty() {
            out.push(MomentVideo {
                url,
                thumb,
                thumb_is_image,
                key,
                md5,
                duration,
                width,
                height,
            });
        }
        pos = media_close + 8;
    }
    out
}

/// 从 XML 中提取每个 `<media>` 块的图片媒体（含解密 key/token）
///
/// 只精确匹配 `<media>` / `<media ...>`，避免把 `<mediaList>` 误当作媒体块；
/// 标签名兼容大小写与带属性形式；finderFeed（视频号）使用 thumbUrl / coverUrl。
/// 视频媒体（snsvideodownload / mp4）不进入图片列表。
fn extract_media(xml: &str) -> Vec<MomentMedia> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < xml.len() {
        let rest = &xml[pos..];
        // 精确匹配 `<media>` 或 `<media `（带属性），避免误匹配 `<mediaList>`
        let tag_start = match rest.find("<media>").or_else(|| rest.find("<media ")) {
            Some(i) => i,
            None => break,
        };
        let tag_start = pos + tag_start;
        // 找到 > 结束标签开头
        let tag_close = match xml[tag_start..].find('>') {
            Some(i) => tag_start + i,
            None => break,
        };
        // 找到 </media>
        let media_close = match xml[tag_close..].find("</media>") {
            Some(i) => tag_close + i,
            None => break,
        };
        // media 内部内容
        let inner = &xml[tag_close + 1..media_close];

        // 缩略图：thumb → cdnThumbUrl → thumbUrl（视频号封面）→ coverUrl
        let (thumb, thumb_token) = media_text_and_token(
            inner,
            &[
                "thumb",
                "Thumb",
                "cdnThumbUrl",
                "cdnthumburl",
                "thumbUrl",
                "thumburl",
                "coverUrl",
                "coverurl",
            ],
        );
        // 原图：url → cdnUrl；缺失时用缩略图 URL 兜底
        let (mut url, mut url_token) =
            media_text_and_token(inner, &["url", "Url", "cdnUrl", "cdnurl"]);
        if url.is_empty() {
            url = thumb.clone();
            url_token = thumb_token.clone();
        }
        // 解密参数：媒体级 key（thumb/url 共用）、url md5
        let key = xml_open_attr(inner, "thumb", "key")
            .or_else(|| xml_open_attr(inner, "url", "key"))
            .unwrap_or_default();
        let md5 = xml_open_attr(inner, "url", "md5").unwrap_or_default();

        if !url.is_empty() && !is_video_url(&url) {
            out.push(MomentMedia {
                thumb,
                thumb_token,
                url,
                url_token,
                key,
                md5,
            });
        }
        // 跳过 </media>
        pos = media_close + 8;
    }
    out
}

/// 解析朋友圈 content XML
fn parse_sns_xml(
    xml: &str,
) -> (
    String,
    i64,
    usize,
    String,
    Vec<MomentMedia>,
    Vec<MomentVideo>,
    String,
    String,
) {
    let text = common::xml_tag_text(xml, "contentDesc")
        .or_else(|| common::xml_tag_text(xml, "ContentDesc"))
        .unwrap_or_default();
    let create_time = common::xml_tag_text(xml, "createTime")
        .or_else(|| common::xml_tag_text(xml, "CreateTime"))
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(0);
    // 媒体数量：<media> 标签个数
    let media_count = xml.matches("<media>").count() + xml.matches("<media ").count();
    let has_video = xml.contains("<type>6</type>")
        || xml.contains("<type>4</type>")
        || xml.contains("<type>15</type>");
    let media_desc = if media_count == 0 {
        String::new()
    } else if has_video {
        "视频".to_string()
    } else {
        format!("图片×{}", media_count)
    };
    // 提取图片媒体
    let images = extract_media(xml);
    // 提取视频媒体
    let videos = extract_videos(xml);
    let location = common::xml_tag_attr(xml, "location", "poiName")
        .or_else(|| common::xml_tag_attr(xml, "location", "poiname"))
        .unwrap_or_default();
    let link_title = common::xml_nested_text(xml, "ContentObject", "title")
        .or_else(|| common::xml_nested_text(xml, "contentObject", "title"))
        .unwrap_or_default();
    (
        text,
        create_time,
        media_count,
        media_desc,
        images,
        videos,
        location,
        link_title,
    )
}

/// 朋友圈分页结果
#[derive(Debug, Clone, Serialize)]
pub struct MomentsPage {
    /// 当前页条目
    pub items: Vec<MomentEntry>,
    /// 该分类下联系人总数
    pub total: usize,
    /// 是否还有更多数据可加载
    #[serde(rename = "has_more")]
    pub has_more: bool,
}

/// 加载朋友圈互动数据（点赞 type=1 / 评论 type=2）
///
/// 数据来源：`SnsMessage_tmp3`（微信 4.x 互动消息表），旧版微信为 `SnsComment`。
/// 返回 feed_id(tid) → (点赞列表, 评论列表)。
/// 单条朋友圈互动（点赞 + 评论）
#[derive(Clone, Default, serde::Serialize)]
pub struct MomentInteractions {
    pub likes: Vec<MomentLike>,
    pub comments: Vec<MomentComment>,
}
pub type MomentInteractionsMap = HashMap<i64, MomentInteractions>;
fn load_interactions(
    conn: &rusqlite::Connection,
    contact_names: &HashMap<String, String>,
) -> MomentInteractionsMap {
    let mut result: MomentInteractionsMap = HashMap::new();
    let table = ["SnsMessage_tmp3", "SnsComment"]
        .iter()
        .find(|t| common::table_exists(conn, t))
        .copied();
    let Some(table) = table else {
        return result;
    };

    let cols = common::table_columns(conn, table);
    let has = |c: &str| cols.iter().any(|x| x == c);
    if !has("feed_id") {
        return result;
    }
    let sel = |c: &str, dft: &str| {
        if has(c) {
            format!("\"{}\"", c)
        } else {
            dft.to_string()
        }
    };
    let del_filter = if has("del_status") {
        " WHERE \"del_status\" = 0"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {feed}, {itype}, {fuser}, {fnick}, {tuser}, {tnick}, {content}, {time} \
         FROM \"{table}\"{del_filter} ORDER BY {time} ASC",
        feed = sel("feed_id", "0"),
        itype = sel("type", "0"),
        fuser = sel("from_username", "''"),
        fnick = sel("from_nickname", "''"),
        tuser = sel("to_username", "''"),
        tnick = sel("to_nickname", "''"),
        content = sel("content", "''"),
        time = sel("create_time", "0"),
    );

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[moments] 查询互动表 {} 失败: {}", table, e);
            return result;
        }
    };
    let rows = match stmt.query_map([], |row| {
        use rusqlite::types::ValueRef;
        let feed_id = match row.get_ref(0)? {
            ValueRef::Integer(i) => i,
            ValueRef::Text(t) => String::from_utf8_lossy(t).parse::<i64>().unwrap_or(0),
            _ => 0,
        };
        Ok::<_, rusqlite::Error>((
            feed_id,
            row.get::<_, i64>(1).unwrap_or(0),
            row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            row.get::<_, Option<String>>(6)?.unwrap_or_default(),
            row.get::<_, i64>(7).unwrap_or(0),
        ))
    }) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("[moments] 读取互动表 {} 失败: {}", table, e);
            return result;
        }
    };

    let resolve_name = |username: &str, nickname: &str| -> String {
        if !nickname.is_empty() {
            nickname.to_string()
        } else {
            contact_names
                .get(username)
                .cloned()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| username.to_string())
        }
    };

    let mut seen_likes: std::collections::HashSet<(i64, String)> = std::collections::HashSet::new();
    for r in rows.flatten() {
        let (feed_id, itype, fuser, fnick, tuser, tnick, content, ts) = r;
        if feed_id == 0 {
            continue;
        }
        let entry = result.entry(feed_id).or_default();
        if itype == 1 {
            // 点赞
            if !seen_likes.insert((feed_id, fuser.clone())) {
                continue; // 同一用户对同一动态去重（点赞时间变化只保留最早一条）
            }
            entry.likes.push(MomentLike {
                username: fuser.clone(),
                nickname: resolve_name(&fuser, &fnick),
            });
        } else if itype == 2 {
            // 评论
            let to_nickname = resolve_name(&tuser, &tnick);
            entry.comments.push(MomentComment {
                username: fuser.clone(),
                nickname: resolve_name(&fuser, &fnick),
                to_username: tuser,
                to_nickname,
                content,
                ts,
                time: if ts > 0 {
                    common::format_date_time(ts)
                } else {
                    String::new()
                },
            });
        }
    }
    result
}

/// 读取朋友圈全量互动（点赞/评论），按 feed_id 分组。
///
/// 供前端轮询合并：即使动态本身未变，评论/点赞的新增也能实时更新到已加载条目。
pub fn get_all_interactions(decrypted_dir: &Path) -> Result<MomentInteractionsMap, String> {
    let candidates = [
        decrypted_dir.join("sns").join("db_sns").join("sns.db"),
        decrypted_dir.join("sns").join("sns.db"),
    ];
    let db_path = candidates
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| "朋友圈数据库未解密 (sns.db)".to_string())?;
    let conn = common::open_readonly_db(db_path).map_err(|e| format!("打开失败: {}", e))?;
    let contact_db = decrypted_dir.join("contact").join("contact.db");
    let contact_names = contacts::load_display_names(&contact_db);
    Ok(load_interactions(&conn, &contact_names))
}

/// 分页读取朋友圈（每次 6 条懒加载）。
///
/// `author_username` 非空时只返回该作者的动态（「专门看某位好友的
/// 朋友圈」，按 SnsTimeLine.user_name 精确过滤，分页/总数同步生效）。
pub fn get_moments_page(
    decrypted_dir: &Path,
    self_username: &str,
    offset: usize,
    limit: usize,
    author_username: Option<&str>,
) -> Result<MomentsPage, String> {
    // sns.db 可能在 sns/db_sns/sns.db 或 sns/sns.db
    let candidates = [
        decrypted_dir.join("sns").join("db_sns").join("sns.db"),
        decrypted_dir.join("sns").join("sns.db"),
    ];
    let db_path = candidates
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| "朋友圈数据库未解密 (sns.db)".to_string())?;

    let conn = common::open_readonly_db(db_path).map_err(|e| format!("打开失败: {}", e))?;
    if !common::table_exists(&conn, "SnsTimeLine") {
        return Err("SnsTimeLine 表不存在".to_string());
    }

    // 按作者 username 过滤（「专门看某位好友的朋友圈」）：
    // 仅在 user_name 列存在且传入非空作者时启用
    let author = author_username
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let cols = common::table_columns(&conn, "SnsTimeLine");
    let has_user_name = cols.iter().any(|c| c == "user_name");
    let author_where = match (&author, has_user_name) {
        (Some(_), true) => Some(" WHERE \"user_name\" = ?1".to_string()),
        _ => None,
    };

    // 先算总数
    let total: usize = match &author_where {
        Some(w) => conn
            .query_row(
                &format!("SELECT COUNT(*) FROM SnsTimeLine{}", w),
                rusqlite::params![author.as_deref().unwrap_or("")],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as usize,
        None => conn
            .query_row("SELECT COUNT(*) FROM SnsTimeLine", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize,
    };

    let contact_db = decrypted_dir.join("contact").join("contact.db");
    let contact_names = contacts::load_display_names(&contact_db);
    // 点赞/评论：同一连接上一次查完，按 feed_id(tid) 分组
    let interactions = load_interactions(&conn, &contact_names);

    let has = |c: &str| cols.iter().any(|x| x == c);
    let sel = |c: &str, dft: &str| {
        if has(c) {
            format!("\"{}\"", c)
        } else {
            dft.to_string()
        }
    };
    let order_col = if has("tid") { "tid" } else { "rowid" };
    let sql = match &author_where {
        Some(w) => format!(
            "SELECT {tid}, {uname}, {content} FROM SnsTimeLine{w} ORDER BY {ord} DESC LIMIT ?2 OFFSET ?3",
            tid = sel("tid", "rowid"),
            uname = sel("user_name", "''"),
            content = sel("content", "NULL"),
            ord = order_col,
        ),
        None => format!(
            "SELECT {tid}, {uname}, {content} FROM SnsTimeLine ORDER BY {ord} DESC LIMIT ?1 OFFSET ?2",
            tid = sel("tid", "rowid"),
            uname = sel("user_name", "''"),
            content = sel("content", "NULL"),
            ord = order_col,
        ),
    };

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("查询失败: {}", e))?;
    let params: Vec<rusqlite::types::Value> = match (&author, &author_where) {
        (Some(a), Some(_)) => vec![
            rusqlite::types::Value::Text(a.clone()),
            rusqlite::types::Value::Integer(limit as i64),
            rusqlite::types::Value::Integer(offset as i64),
        ],
        _ => vec![
            rusqlite::types::Value::Integer(limit as i64),
            rusqlite::types::Value::Integer(offset as i64),
        ],
    };
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            let tid = match row.get_ref(0)? {
                rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).to_string(),
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                _ => String::new(),
            };
            Ok::<_, rusqlite::Error>((
                tid,
                row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                common::get_bytes(row, 2),
            ))
        })
        .map_err(|e| format!("读取失败: {}", e))?;

    let mut items = Vec::new();
    for r in rows.flatten() {
        let tid = r.0;
        let username = r.1;
        let xml =
            r.2.as_deref()
                .map(common::decode_blob_text)
                .unwrap_or_default();
        let (text, create_time, media_count, media_desc, images, videos, location, link_title) =
            parse_sns_xml(&xml);

        let is_self = !self_username.is_empty() && username == self_username;
        let author = if is_self {
            "我".to_string()
        } else {
            contact_names
                .get(&username)
                .cloned()
                .filter(|s| !s.is_empty())
                .or_else(|| common::system_account_name(&username).map(|s| s.to_string()))
                .unwrap_or_else(|| username.clone())
        };
        // 互动数据按 tid 关联（SnsTimeLine.tid 与互动表 feed_id 同源）
        let MomentInteractions { likes, comments } = tid
            .parse::<i64>()
            .ok()
            .and_then(|t| interactions.get(&t))
            .cloned()
            .unwrap_or_default();

        items.push(MomentEntry {
            tid,
            username,
            author,
            text,
            ts: create_time,
            time: if create_time > 0 {
                common::format_date_time(create_time)
            } else {
                String::new()
            },
            media_count,
            media_desc,
            images,
            videos,
            location,
            link_title,
            is_self,
            likes,
            comments,
        });
    }

    let has_more = offset + items.len() < total;
    Ok(MomentsPage {
        items,
        total,
        has_more,
    })
}

// ─── 朋友圈洞察（作者活跃榜 / 月度热力 / 媒体构成）───

/// 作者活跃统计
#[derive(Debug, Clone, Serialize)]
pub struct MomentsAuthorStat {
    /// 作者 username
    pub username: String,
    /// 作者显示名（备注 > 昵称 > username；本人为「我」）
    pub name: String,
    /// 发圈数
    pub posts: usize,
    /// 最近一条发圈时间（Unix 秒）
    pub last_ts: i64,
}

/// 月度发圈分布
#[derive(Debug, Clone, Serialize)]
pub struct MomentsMonthStat {
    /// 月份 "2025-08"
    pub month: String,
    /// 该月发圈数
    pub posts: usize,
}

/// 朋友圈洞察（一次全量扫描聚合，供洞察面板直接展示）
#[derive(Debug, Clone, Serialize)]
pub struct MomentsInsight {
    /// 动态总数
    pub total: usize,
    /// 含图片的动态数
    pub with_images: usize,
    /// 含视频的动态数
    pub with_videos: usize,
    /// 带位置的动态数
    pub with_location: usize,
    /// 分享链接的动态数
    pub with_link: usize,
    /// 自己发布的动态数
    pub self_posts: usize,
    /// 活跃作者 Top 15（发圈数降序，并列按最近发圈时间降序）
    pub top_authors: Vec<MomentsAuthorStat>,
    /// 最近 12 个月发圈分布（升序；锚定最新一条动态所在月，无时间数据时锚定当前月）
    pub monthly: Vec<MomentsMonthStat>,
}

/// Unix 秒 → "YYYY-MM"
fn month_key(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m").to_string())
        .unwrap_or_default()
}

/// "YYYY-MM" 的上一月
fn prev_month_key(key: &str) -> String {
    let (y, m) = key
        .split_once('-')
        .and_then(|(y, m)| Some((y.parse::<i32>().ok()?, m.parse::<u32>().ok()?)))
        .unwrap_or((1970, 1));
    if m == 1 {
        format!("{:04}-12", y - 1)
    } else {
        format!("{:04}-{:02}", y, m - 1)
    }
}

/// 全量扫描 SnsTimeLine 聚合朋友圈洞察（作者活跃榜 / 月度热力 / 媒体构成）。
///
/// 逐行解析 content XML 统计媒体类型与时间分布；1271 条规模的
/// 全量扫描毫秒级完成，无需分页。
pub fn get_moments_insights(
    decrypted_dir: &Path,
    self_username: &str,
) -> Result<MomentsInsight, String> {
    let candidates = [
        decrypted_dir.join("sns").join("db_sns").join("sns.db"),
        decrypted_dir.join("sns").join("sns.db"),
    ];
    let db_path = candidates
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| "朋友圈数据库未解密 (sns.db)".to_string())?;
    let conn = common::open_readonly_db(db_path).map_err(|e| format!("打开失败: {}", e))?;
    if !common::table_exists(&conn, "SnsTimeLine") {
        return Err("SnsTimeLine 表不存在".to_string());
    }
    let contact_db = decrypted_dir.join("contact").join("contact.db");
    let contact_names = contacts::load_display_names(&contact_db);
    let cols = common::table_columns(&conn, "SnsTimeLine");
    let has = |c: &str| cols.iter().any(|x| x == c);
    let sel_tid = if has("tid") { "\"tid\"" } else { "rowid" };
    let sel_uname = if has("user_name") {
        "\"user_name\""
    } else {
        "''"
    };
    let sel_content = if has("content") {
        "\"content\""
    } else {
        "NULL"
    };
    let sql = format!(
        "SELECT {tid}, {uname}, {content} FROM SnsTimeLine",
        tid = sel_tid,
        uname = sel_uname,
        content = sel_content,
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("查询失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            let tid = match row.get_ref(0)? {
                rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).to_string(),
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                _ => String::new(),
            };
            Ok::<_, rusqlite::Error>((
                tid,
                row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                common::get_bytes(row, 2),
            ))
        })
        .map_err(|e| format!("读取失败: {}", e))?;

    let mut total = 0usize;
    let mut with_images = 0usize;
    let mut with_videos = 0usize;
    let mut with_location = 0usize;
    let mut with_link = 0usize;
    let mut self_posts = 0usize;
    let mut max_ts: i64 = 0;
    // username → (发圈数, 最近发圈时间)
    let mut authors: HashMap<String, (usize, i64)> = HashMap::new();
    let mut months: HashMap<String, usize> = HashMap::new();

    for r in rows.flatten() {
        let username = r.1;
        let xml =
            r.2.as_deref()
                .map(common::decode_blob_text)
                .unwrap_or_default();
        let (_, create_time, _, _, images, videos, location, link_title) = parse_sns_xml(&xml);
        total += 1;
        if !images.is_empty() {
            with_images += 1;
        }
        if !videos.is_empty() {
            with_videos += 1;
        }
        if !location.is_empty() {
            with_location += 1;
        }
        if !link_title.is_empty() {
            with_link += 1;
        }
        if !self_username.is_empty() && username == self_username {
            self_posts += 1;
        }
        let entry = authors.entry(username.clone()).or_insert((0, 0));
        entry.0 += 1;
        if create_time > entry.1 {
            entry.1 = create_time;
        }
        if create_time > 0 {
            if create_time > max_ts {
                max_ts = create_time;
            }
            *months.entry(month_key(create_time)).or_insert(0) += 1;
        }
    }

    let mut top_authors: Vec<MomentsAuthorStat> = authors
        .into_iter()
        .map(|(username, (posts, last_ts))| {
            let name = if !self_username.is_empty() && username == self_username {
                "我".to_string()
            } else {
                contact_names
                    .get(&username)
                    .cloned()
                    .filter(|s| !s.is_empty())
                    .or_else(|| common::system_account_name(&username).map(|s| s.to_string()))
                    .unwrap_or_else(|| username.clone())
            };
            MomentsAuthorStat {
                username,
                name,
                posts,
                last_ts,
            }
        })
        .collect();
    top_authors.sort_by(|a, b| b.posts.cmp(&a.posts).then(b.last_ts.cmp(&a.last_ts)));
    top_authors.truncate(15);

    // 最近 12 个月：从锚定月往前推 11 个月，再升序返回
    let anchor = if max_ts > 0 {
        month_key(max_ts)
    } else {
        chrono::Local::now().format("%Y-%m").to_string()
    };
    let mut keys = Vec::with_capacity(12);
    let mut cur = anchor;
    for _ in 0..12 {
        keys.push(cur.clone());
        cur = prev_month_key(&cur);
    }
    keys.reverse();
    let monthly = keys
        .into_iter()
        .map(|m| {
            let posts = months.get(&m).copied().unwrap_or(0);
            MomentsMonthStat { month: m, posts }
        })
        .collect();

    Ok(MomentsInsight {
        total,
        with_images,
        with_videos,
        with_location,
        with_link,
        self_posts,
        top_authors,
        monthly,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实数据：朋友圈洞察聚合（作者榜 / 月度 12 个月 / 媒体构成）
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_insights_real_data() {
        let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let insight = get_moments_insights(&cfg.decrypted_dir, &cfg.wxid().unwrap_or_default())
            .expect("读取朋友圈洞察失败");
        eprintln!(
            "洞察: total={}, 含图={}, 含视频={}, 带位置={}, 链接={}, 作者Top={}",
            insight.total,
            insight.with_images,
            insight.with_videos,
            insight.with_location,
            insight.with_link,
            insight.top_authors.len(),
        );
        if insight.total == 0 {
            eprintln!("朋友圈无动态数据，跳过");
            return;
        }
        assert_eq!(insight.monthly.len(), 12, "月度分布应为最近 12 个月");
        assert!(!insight.top_authors.is_empty(), "应存在活跃作者");
        assert!(
            insight.with_images + insight.with_videos <= insight.total,
            "媒体计数不应超过总数"
        );
        // 月度按升序返回
        assert!(
            insight.monthly.windows(2).all(|w| w[0].month < w[1].month),
            "月度分布应升序"
        );
    }

    /// 真实数据：朋友圈互动（点赞/评论）应能读取，且评论按时间升序排列
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_interactions_sorted() {
        let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let interactions = get_all_interactions(&cfg.decrypted_dir).expect("读取互动失败");
        eprintln!("互动 feed 数: {}", interactions.len());
        if interactions.is_empty() {
            eprintln!("朋友圈无点赞/评论数据，跳过");
            return;
        }
        let mut checked_pairs = 0usize;
        for (feed_id, MomentInteractions { likes, comments }) in &interactions {
            eprintln!(
                "feed {}: 点赞 {}，评论 {}",
                feed_id,
                likes.len(),
                comments.len()
            );
            for w in comments.windows(2) {
                assert!(w[0].ts <= w[1].ts, "评论应按时间升序");
                checked_pairs += 1;
            }
        }
        eprintln!("已校验评论时间对: {}", checked_pairs);
        // 互动数据较稀疏时（仅 1 条评论）无法校验时间对，但读取链路必须正常
        assert!(interactions.values().all(|v| v.comments.len() <= 1) || checked_pairs > 0);
    }

    /// 真实朋友圈 XML：`<thumb>` / `<url>` 带属性，应能提取图片 URL
    #[test]
    fn extract_urls_from_attr_tags() {
        let xml = r#"<SnsDataItem><TimelineObject><ContentObject><mediaList>
            <media><id>1</id><type>2</type>
                <thumb type="1" key="123" enc_idx="1" token="abc">http://szmmsns.qpic.cn/mmsns/A/150</thumb>
                <url type="1" md5="abc" key="123" enc_idx="1">http://szmmsns.qpic.cn/mmsns/A/0</url>
            </media>
            <media><id>2</id><type>2</type>
                <thumb>http://szmmsns.qpic.cn/mmsns/B/150</thumb>
            </media>
        </mediaList></ContentObject></TimelineObject></SnsDataItem>"#;
        let media = extract_media(xml);
        assert_eq!(media.len(), 2);
        assert_eq!(media[0].thumb, "http://szmmsns.qpic.cn/mmsns/A/150");
        assert_eq!(media[0].url, "http://szmmsns.qpic.cn/mmsns/A/0");
        assert_eq!(media[0].key, "123");
        assert_eq!(media[0].thumb_token, "abc");
        assert_eq!(media[0].md5, "abc");
        // 无 url 时用 thumb 兜底
        assert_eq!(media[1].url, "http://szmmsns.qpic.cn/mmsns/B/150");
    }

    /// `<mediaList>` 不应被误当作媒体块
    #[test]
    fn media_list_not_treated_as_media() {
        let xml = r#"<ContentObject><mediaList><media><thumb>http://x/1</thumb></media></mediaList></ContentObject>"#;
        let media = extract_media(xml);
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].thumb, "http://x/1");
    }

    /// 视频号（finderFeed）使用 thumbUrl / coverUrl
    #[test]
    fn extract_finder_thumb_url() {
        let xml = r#"<ContentObject><finderFeed><mediaList><media>
            <mediaType>4</mediaType>
            <thumbUrl>https://wxapp.tc.qq.com/thumb?a=1&amp;b=2</thumbUrl>
            <coverUrl>https://wxapp.tc.qq.com/cover</coverUrl>
        </media></mediaList></finderFeed></ContentObject>"#;
        let media = extract_media(xml);
        assert_eq!(media.len(), 1);
        // XML 实体应反转义：&amp; → &
        assert_eq!(media[0].thumb, "https://wxapp.tc.qq.com/thumb?a=1&b=2");
        assert_eq!(media[0].url, "https://wxapp.tc.qq.com/thumb?a=1&b=2");
    }

    /// 视频媒体（snsvideodownload）不应进入图片列表
    #[test]
    fn video_media_excluded_from_images() {
        let xml = r#"<ContentObject><mediaList><media><id>1</id><type>4</type>
            <thumb type="1" key="0">http://shzjwxsns.video.qq.com/102/20250/snsvideodownload?dotrans=0</thumb>
            <url type="1" md5="abc">http://shzjwxsns.video.qq.com/102/20202/snsvideodownload?dotrans=1</url>
        </media></mediaList></ContentObject>"#;
        let media = extract_media(xml);
        assert!(media.is_empty());
    }

    /// 视频媒体提取：url/key/封面/时长/宽高
    #[test]
    fn extract_video_media() {
        let xml = r#"<ContentObject><mediaList><media><id>1</id><type>6</type>
            <thumb type="1" key="0">http://vweixinthumb.tc.qq.com/150/20250/snsvideodownload?token=t1&amp;idx=1</thumb>
            <url type="1" md5="abc" key="0" videomd5="def">http://shzjwxsns.video.qq.com/102/20202/snsvideodownload?dotrans=9</url>
            <size width="288" height="512" totalSize="18190"/>
            <videoDuration>2.53299999</videoDuration>
            <enc key="4168408197">1</enc>
        </media></mediaList></ContentObject>"#;
        let videos = extract_videos(xml);
        assert_eq!(videos.len(), 1);
        assert_eq!(
            videos[0].url,
            "http://shzjwxsns.video.qq.com/102/20202/snsvideodownload?dotrans=9"
        );
        assert_eq!(videos[0].key, "4168408197");
        assert_eq!(videos[0].md5, "abc");
        assert!(videos[0].thumb_is_image);
        assert!((videos[0].duration - 2.533).abs() < 0.01);
        assert_eq!(videos[0].width, 288);
        assert_eq!(videos[0].height, 512);
        // 非视频媒体不应被当作视频
        let img_xml = r#"<ContentObject><mediaList><media><type>2</type>
            <thumb>http://szmmsns.qpic.cn/mmsns/A/150</thumb>
            <url>http://szmmsns.qpic.cn/mmsns/A/0</url>
        </media></mediaList></ContentObject>"#;
        assert!(extract_videos(img_xml).is_empty());
    }

    /// 无媒体纯文字动态：不产生图片
    #[test]
    fn no_media_no_images() {
        let xml = r#"<SnsDataItem><TimelineObject><contentDesc>hello</contentDesc></TimelineObject></SnsDataItem>"#;
        assert!(extract_media(xml).is_empty());
    }

    /// 互动表加载：点赞/评论分组、昵称解析、删除过滤
    #[test]
    fn load_interactions_groups_by_feed() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE SnsMessage_tmp3 (
                local_id INTEGER PRIMARY KEY,
                create_time INTEGER,
                type INTEGER,
                feed_id INTEGER,
                from_username TEXT,
                from_nickname TEXT,
                to_username TEXT,
                to_nickname TEXT,
                content TEXT,
                comment_flag INTEGER,
                del_status INTEGER
            );
            INSERT INTO SnsMessage_tmp3 VALUES (1, 1768055665, 1, 1001, 'u_like', '点赞人', '', '', '', 0, 0);
            INSERT INTO SnsMessage_tmp3 VALUES (2, 1768055666, 2, 1001, 'u_cmt', '评论人', 'u_like', '点赞人', '赞一个', 0, 0);
            INSERT INTO SnsMessage_tmp3 VALUES (3, 1768055667, 2, 1002, 'u_del', '', '', '', '已删除', 0, 1);
            "#,
        )
        .unwrap();

        let mut names = HashMap::new();
        names.insert("u_unk".to_string(), "通讯录名".to_string());
        // 昵称缺失时回退通讯录 / username
        conn.execute_batch(
            "INSERT INTO SnsMessage_tmp3 VALUES (4, 1768055668, 1, 1002, 'u_unk', '', '', '', '', 0, 0);",
        )
        .unwrap();

        let map = load_interactions(&conn, &names);
        let MomentInteractions {
            likes: likes1,
            comments: comments1,
        } = map.get(&1001).expect("feed 1001 应有互动");
        assert_eq!(likes1.len(), 1);
        assert_eq!(likes1[0].nickname, "点赞人");
        assert_eq!(comments1.len(), 1);
        assert_eq!(comments1[0].content, "赞一个");
        assert_eq!(comments1[0].to_nickname, "点赞人");
        // del_status=1 的行被过滤
        let MomentInteractions {
            likes: likes2,
            comments: comments2,
        } = map.get(&1002).expect("feed 1002 应有互动");
        assert!(comments2.is_empty());
        assert_eq!(likes2.len(), 1);
        assert_eq!(likes2[0].nickname, "通讯录名");
    }
}
