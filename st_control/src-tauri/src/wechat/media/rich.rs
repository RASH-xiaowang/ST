// ============================================================
// 微信富媒体消息解析 — 富媒体域
// 自 media.rs 拆分：RichMedia 类型与通用/mmreader 图文解析。
// ============================================================

use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Serialize;

use super::*;

// ============ 富媒体类型枚举 ============

#[derive(Debug, Clone, Serialize)]
pub struct ChatLogItem {
    pub name: String,
    pub text: String,
}

/// 图文推送条目（mmreader item/newitem）
#[derive(Debug, Clone, Serialize)]
pub struct NewsFeedItem {
    pub title: String,
    pub url: String,
    pub cover: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum RichMedia {
    #[serde(rename = "emoji")]
    Emoji {
        emoji_url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    #[serde(rename = "file")]
    File {
        title: String,
        file_ext: String,
        file_size: i64,
    },
    #[serde(rename = "link")]
    Link {
        title: String,
        des: String,
        url: String,
        source: String,
        /// 文章封面图（<thumburl>，公众号/服务号文章消息普遍携带）
        #[serde(skip_serializing_if = "Option::is_none")]
        thumb: Option<String>,
        /// 多图文消息的子文章列表（appmsg 内嵌 <mmreader> 的 item[1..]）
        #[serde(skip_serializing_if = "Vec::is_empty")]
        articles: Vec<NewsFeedItem>,
    },
    #[serde(rename = "quote")]
    Quote {
        title: String,
        ref_name: String,
        ref_content: String,
    },
    #[serde(rename = "miniapp")]
    MiniApp {
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        des: Option<String>,
        source: String,
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        /// 小程序页面路径（部分分享消息没有 <url>，但 pagepath 里带真实网页链接）
        #[serde(skip_serializing_if = "Option::is_none")]
        pagepath: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        appid: Option<String>,
    },
    #[serde(rename = "channels")]
    Channels {
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        nickname: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        desc: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cover: Option<String>,
    },
    #[serde(rename = "chatlog")]
    ChatLog {
        title: String,
        des: String,
        items: Vec<ChatLogItem>,
    },
    #[serde(rename = "transfer")]
    Transfer {
        title: String,
        direction: String,
        paysubtype: String,
        fee_desc: String,
        amount: String,
        pay_memo: String,
        transfer_id: String,
    },
    #[serde(rename = "redpacket")]
    RedPacket {
        title: String,
        fee_desc: String,
        amount: String,
        paysubtype: String,
    },
    #[serde(rename = "location")]
    Location {
        label: String,
        poiname: String,
        url: String,
    },
    #[serde(rename = "contact")]
    Contact { nickname: String, username: String },
    #[serde(rename = "video")]
    Video { duration: i32 },
    #[serde(rename = "voice")]
    Voice { duration: f64 },
    #[serde(rename = "newsfeed")]
    NewsFeed {
        name: String,
        top_cover: Option<String>,
        items: Vec<NewsFeedItem>,
    },
}

// ============ 通用富媒体解析 ============

/// 解析富媒体消息 XML, 返回 RichMedia 枚举
pub fn parse_rich_content(xml: &str, msg_type: i32) -> Option<RichMedia> {
    match msg_type {
        47 => parse_emoji(xml),
        49 => parse_appmsg(xml),
        43 => parse_video(xml),
        34 => parse_voice(xml),
        42 => parse_contact(xml),
        48 => parse_location(xml),
        _ => None,
    }
}

/// 解析名片 (type=42)：`<msg><contact username="..." nickname="..." .../></msg>`
pub(crate) fn parse_contact(xml: &str) -> Option<RichMedia> {
    let nickname = find_attr(xml, "contact", "nickname").unwrap_or_default();
    let username = find_attr(xml, "contact", "username").unwrap_or_default();
    if nickname.is_empty() && username.is_empty() {
        return None;
    }
    Some(RichMedia::Contact { nickname, username })
}

/// 解析位置 (type=48)：`<msg><location x=".." y=".." label=".." poiname=".." infourl=".." /></msg>`
pub(crate) fn parse_location(xml: &str) -> Option<RichMedia> {
    let poiname = find_attr(xml, "location", "poiname").unwrap_or_default();
    let label = find_attr(xml, "location", "label").unwrap_or_default();
    let url = find_attr(xml, "location", "infourl")
        .or_else(|| find_attr(xml, "location", "url"))
        .unwrap_or_default();
    let poiname = if poiname.is_empty() {
        label.clone()
    } else {
        poiname
    };
    if poiname.is_empty() {
        return None;
    }
    Some(RichMedia::Location {
        label,
        poiname,
        url,
    })
}

/// 解析表情 (type=47)
pub(crate) fn parse_emoji(xml: &str) -> Option<RichMedia> {
    let emoji_url = find_attr(xml, "emoji", "md5")
        .or_else(|| find_attr(xml, "emoji", "thumburl"))
        .or_else(|| find_attr(xml, "emoji", "cdnurl"))?;
    let description = find_attr(xml, "emoji", "attachedtext")
        .filter(|s| !s.is_empty())
        .or_else(|| find_attr(xml, "emoji", "desc").filter(|s| !s.is_empty()));
    Some(RichMedia::Emoji {
        emoji_url,
        description,
    })
}

/// 解析 appmsg (type=49, 子类型丰富)
pub(crate) fn parse_appmsg(xml: &str) -> Option<RichMedia> {
    // 先找到 appmsg 节
    let app_start = xml.find("<appmsg")?;
    let app_end = xml[app_start..].find("</appmsg>")?;
    let app_str = &xml[app_start..app_start + app_end + 9];

    // 提取 app type
    let app_type = get_tag_int(app_str, "type").unwrap_or(0);
    let title = get_tag_text(app_str, "title").unwrap_or_default();
    let des = get_tag_text(app_str, "des").unwrap_or_default();
    let url = get_tag_text(app_str, "url")
        .unwrap_or_default()
        .replace("&amp;", "&");

    match app_type {
        57 => {
            // 引用回复
            let ref_name = extract_nested(app_str, "refermsg", "displayname").unwrap_or_default();
            let ref_content = extract_nested(app_str, "refermsg", "content")
                .map(|s| {
                    // 安全截断：用 floor_char_boundary 避免在 UTF-8 字符中间切开
                    let end = s.floor_char_boundary(s.len().min(100));
                    collapse_text(&s[..end])
                })
                .unwrap_or_default();
            Some(RichMedia::Quote {
                title,
                ref_name,
                ref_content,
            })
        }
        6 => {
            // 文件
            let ext = extract_nested(app_str, "appattach", "fileext").unwrap_or_default();
            let size = parse_nested_int(app_str, "appattach", "totallen").unwrap_or(0);
            Some(RichMedia::File {
                title,
                file_ext: ext,
                file_size: size,
            })
        }
        5 | 0 => {
            // 链接
            let source = get_tag_text(app_str, "sourcedisplayname").unwrap_or_default();
            let thumb = get_tag_text(app_str, "thumburl")
                .map(clean_cdata)
                .filter(|s| !s.is_empty());
            // 多图文消息：appmsg 内嵌 <mmreader>，item[0] 即头条（与 appmsg 标题一致），
            // item[1..] 是下方展示的子文章列表
            let articles = extract_appmsg_sub_articles(app_str);
            Some(RichMedia::Link {
                title,
                des: des.chars().take(200).collect(),
                url,
                source,
                thumb,
                articles,
            })
        }
        33 | 36 => {
            // 小程序
            let source = get_tag_text(app_str, "sourcedisplayname").unwrap_or_default();
            let icon = extract_nested(app_str, "weappinfo", "weappiconurl")
                .or_else(|| get_tag_text(app_str, "weappiconurl"))
                .map(clean_cdata)
                .filter(|s| !s.is_empty());
            let pagepath = extract_nested(app_str, "weappinfo", "pagepath")
                .map(clean_cdata)
                .filter(|s| !s.is_empty());
            let appid = extract_nested(app_str, "weappinfo", "appid")
                .map(clean_cdata)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    get_tag_text(app_str, "appid")
                        .map(clean_cdata)
                        .filter(|s| !s.is_empty())
                });
            let miniapp_des = if des.is_empty() {
                None
            } else {
                Some(des.clone())
            };
            Some(RichMedia::MiniApp {
                title,
                des: miniapp_des,
                source,
                url,
                icon,
                pagepath,
                appid,
            })
        }
        51 => {
            // 视频号
            let nickname = extract_nested(app_str, "finderFeed", "nickname")
                .map(clean_cdata)
                .unwrap_or_default();
            let desc2 = extract_nested(app_str, "finderFeed", "desc")
                .map(clean_cdata)
                .unwrap_or_default();
            let cover = get_tag_text(app_str, "thumbUrl")
                .or_else(|| extract_nested(app_str, "mediaList", "thumbUrl"))
                .map(clean_cdata)
                .filter(|s| !s.is_empty());
            Some(RichMedia::Channels {
                title: if title.is_empty() {
                    "视频号内容".to_string()
                } else {
                    title
                },
                nickname: if nickname.is_empty() {
                    None
                } else {
                    Some(nickname)
                },
                desc: if desc2.is_empty() { None } else { Some(desc2) },
                cover,
            })
        }
        19 => {
            // 聊天记录转发
            let items = parse_recorditem(app_str);
            Some(RichMedia::ChatLog {
                title,
                des: des.chars().take(200).collect(),
                items,
            })
        }
        2000 => {
            // 转账
            let info = extract_wcpayinfo(xml).unwrap_or_default();
            let paysubtype = info.get("paysubtype").cloned().unwrap_or_default();
            let direction = if !paysubtype.is_empty() {
                let label = transfer_label(&paysubtype);
                if label.is_empty() {
                    String::new()
                } else {
                    label.to_string()
                }
            } else {
                String::new()
            };
            Some(RichMedia::Transfer {
                title: if title.is_empty() {
                    "微信转账".to_string()
                } else {
                    title
                },
                direction,
                paysubtype,
                fee_desc: info.get("feedesc").cloned().unwrap_or_default(),
                amount: clean_amount(info.get("feedesc").unwrap_or(&String::new())),
                pay_memo: info.get("pay_memo").cloned().unwrap_or_default(),
                transfer_id: info.get("transferid").cloned().unwrap_or_default(),
            })
        }
        2001 => {
            // 红包
            let info = extract_wcpayinfo(xml).unwrap_or_default();
            let paysubtype = info.get("paysubtype").cloned().unwrap_or_default();
            let fee_desc = info.get("feedesc").cloned().unwrap_or_default();
            Some(RichMedia::RedPacket {
                title: if title.is_empty() {
                    "微信红包".to_string()
                } else {
                    title
                },
                fee_desc,
                amount: clean_amount(info.get("feedesc").unwrap_or(&String::new())),
                paysubtype,
            })
        }
        _ => {
            // 其他类型: 用 title 显示
            if !title.is_empty() {
                Some(RichMedia::Link {
                    title,
                    des: des.chars().take(200).collect(),
                    url,
                    source: String::new(),
                    thumb: None,
                    articles: Vec::new(),
                })
            } else {
                None
            }
        }
    }
}

/// 解析视频 (type=43)
pub(crate) fn parse_video(xml: &str) -> Option<RichMedia> {
    let duration = find_attr(xml, "videomsg", "playlength")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0);
    Some(RichMedia::Video { duration })
}

/// 解析语音 (type=34)
pub(crate) fn parse_voice(xml: &str) -> Option<RichMedia> {
    let ms = find_attr(xml, "voicemsg", "voicelength")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    Some(RichMedia::Voice {
        duration: (ms / 1000.0 * 10.0).round() / 10.0,
    })
}

// ============ mmreader 图文推送（腾讯新闻/公众号每日精选）============

/// 解析 mmreader 图文推送 XML。
///
/// 腾讯新闻等公众号的图文卡片 local_type=1（伪装成文本消息），内容为：
/// ```xml
/// <mmreader><category type="20" count="5">
///   <name>腾讯新闻</name>
///   <topnew><cover>头条大图URL</cover></topnew>
///   <item><title>头条标题</title><url>…</url><cover>…</cover><digest>…</digest></item>
///   <newitem>…子条目…</newitem>   <!-- 可能多个 -->
/// </category></mmreader>
/// ```
/// 注意：首条 <item> 常与第一个 <newitem> 重复，需要去重。
pub fn parse_mmreader(xml: &str) -> Option<RichMedia> {
    if !xml.contains("<mmreader>") {
        return None;
    }

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut name = String::new();
    let mut top_cover: Option<String> = None;
    let mut items: Vec<NewsFeedItem> = Vec::new();

    // 路径栈：category / topnew / item|newitem / sources / source
    let mut in_topnew = false;
    let mut in_sources = false;
    let mut cur_item: Option<NewsFeedItem> = None;
    let mut cur_tag = String::new();
    let mut buf = Vec::new();

    macro_rules! handle_text {
        ($text:expr) => {{
            let text = $text.trim();
            if !text.is_empty() {
                if let Some(ref mut item) = cur_item {
                    match cur_tag.as_str() {
                        "title" => item.title = text.to_string(),
                        "url" | "shorturl" if item.url.is_empty() => item.url = text.to_string(),
                        "cover" if item.cover.is_empty() => item.cover = text.to_string(),
                        "digest" => item.digest = text.to_string(),
                        _ => {}
                    }
                } else if in_topnew && cur_tag == "cover" && top_cover.is_none() {
                    top_cover = Some(text.to_string());
                } else if !in_topnew && !in_sources && cur_tag == "name" && name.is_empty() {
                    name = text.to_string();
                }
            }
        }};
    }

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag.as_str() {
                    "topnew" => in_topnew = true,
                    "sources" => in_sources = true,
                    "item" | "newitem" => {
                        cur_item = Some(NewsFeedItem {
                            title: String::new(),
                            url: String::new(),
                            cover: String::new(),
                            digest: String::new(),
                        });
                    }
                    _ => cur_tag = tag,
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().to_string();
                handle_text!(text);
            }
            Ok(Event::CData(ref e)) => {
                // CDATA 内容不做转义还原（BytesCData deref 为 [u8]）
                let text = String::from_utf8_lossy(e).to_string();
                handle_text!(text);
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag.as_str() {
                    "topnew" => in_topnew = false,
                    "sources" => in_sources = false,
                    "item" | "newitem" => {
                        if let Some(item) = cur_item.take() {
                            if !item.title.is_empty() {
                                items.push(item);
                            }
                        }
                    }
                    _ => {}
                }
                cur_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    if items.is_empty() {
        return None;
    }

    // 去重：首条 item 常与第一个 newitem 完全相同
    let mut deduped: Vec<NewsFeedItem> = Vec::with_capacity(items.len());
    for it in items {
        let dup = deduped
            .last()
            .is_some_and(|p| p.title == it.title && (p.url == it.url || it.url.is_empty()));
        if !dup {
            deduped.push(it);
        }
    }

    Some(RichMedia::NewsFeed {
        name,
        top_cover,
        items: deduped,
    })
}

/// 从 appmsg（type=5）中提取多图文的子文章列表。
///
/// 多图文消息的 appmsg 内嵌 `<mmreader>`：`item[0]` 即头条（与 appmsg 的
/// title/url 一致），`item[1..]` 是主卡片下方展示的子文章（第二条、第三条…）。
fn extract_appmsg_sub_articles(app_str: &str) -> Vec<NewsFeedItem> {
    let Some(start) = app_str.find("<mmreader>") else {
        return Vec::new();
    };
    let mm = &app_str[start..];
    let end = mm
        .find("</mmreader>")
        .map(|e| e + "</mmreader>".len())
        .unwrap_or(mm.len());
    match parse_mmreader(&mm[..end]) {
        Some(RichMedia::NewsFeed { items, .. }) if items.len() > 1 => {
            items.into_iter().skip(1).collect()
        }
        _ => Vec::new(),
    }
}

/// 解析 recorditem (聊天记录转发)
fn parse_recorditem(xml: &str) -> Vec<ChatLogItem> {
    let mut items = Vec::new();
    let ri = get_tag_text(xml, "recorditem")
        .map(clean_cdata)
        .unwrap_or_default();
    if ri.is_empty() {
        return items;
    }

    let mut reader = Reader::from_str(&ri);
    reader.config_mut().trim_text(true);
    let mut in_dataitem = false;
    let mut current_item = (String::new(), String::new());
    let mut current_tag = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "dataitem" {
                    in_dataitem = true;
                    current_item = (String::new(), String::new());
                } else if in_dataitem {
                    current_tag = name;
                }
            }
            Ok(Event::Text(ref e)) if in_dataitem && !current_tag.is_empty() => {
                let text = e.unescape().unwrap_or_default().to_string();
                match current_tag.as_str() {
                    "sourcename" => current_item.0 = text,
                    "datadesc" => current_item.1 = text.chars().take(100).collect(),
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "dataitem" && !current_item.0.is_empty() && !current_item.1.is_empty() {
                    items.push(ChatLogItem {
                        name: current_item.0.clone(),
                        text: current_item.1.clone(),
                    });
                    if items.len() >= 20 {
                        break;
                    }
                    in_dataitem = false;
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    items
}
