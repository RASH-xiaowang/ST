//! 微信富媒体消息解析模块
//!
//! 解析表情(47)、链接/文件(49)、视频(43)、语音(34)、
//! 转账(2000)、引用回复、小程序、聊天记录转发等类型的 XML 内容。

mod transfer;
pub(crate) use transfer::*;
mod xml;
pub(crate) use xml::*;
mod rich;
pub(crate) use rich::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_transfer() {
        let xml = r#"<msg><appmsg type="2000"><title>转账</title><wcpayinfo><paysubtype>3</paysubtype><feedesc>¥10.00</feedesc></wcpayinfo></appmsg></msg>"#;
        let info = extract_transfer_info(xml).unwrap();
        assert_eq!(info["paysubtype"], "3");
        assert_eq!(info["paysubtype_label"], "已收款");
    }

    #[test]
    fn test_parse_voice() {
        let xml = r#"<msg><voicemsg voicelength="3500" /></msg>"#;
        let media = parse_voice(xml);
        assert!(matches!(media, Some(RichMedia::Voice { duration: 3.5 })));
    }

    #[test]
    fn test_parse_video() {
        let xml = r#"<msg><videomsg playlength="15" /></msg>"#;
        let media = parse_video(xml);
        assert!(matches!(media, Some(RichMedia::Video { duration: 15 })));
    }

    #[test]
    fn test_parse_emoji() {
        let xml = r#"<msg><emoji md5="abc123" type="1" /></msg>"#;
        let media = parse_emoji(xml);
        assert!(matches!(media, Some(RichMedia::Emoji { .. })));
    }

    #[test]
    fn test_parse_appmsg_link() {
        // 公众号文章：title/des/url 用 CDATA 包裹，解析后必须去掉 CDATA 标记，
        // 否则 url 变成 `<![CDATA[https://...]]>` 导致前端打不开文章
        let xml = r#"<msg><appmsg type="5"><title><![CDATA[文章]]></title><des><![CDATA[描述]]></des><url><![CDATA[https://mp.weixin.qq.com/s?__biz=abc&amp;mid=1]]></url><thumburl><![CDATA[https://mmbiz.qpic.cn/thumb1]]></thumburl><sourcedisplayname><![CDATA[公众号]]></sourcedisplayname></appmsg></msg>"#;
        let media = parse_appmsg(xml).expect("应解析链接");
        match media {
            RichMedia::Link {
                title,
                des,
                url,
                source,
                thumb,
                ..
            } => {
                assert_eq!(title, "文章");
                assert_eq!(des, "描述");
                assert_eq!(source, "公众号");
                assert_eq!(url, "https://mp.weixin.qq.com/s?__biz=abc&mid=1");
                assert!(!url.contains("CDATA"), "url 不应残留 CDATA 标记: {}", url);
                assert_eq!(
                    thumb.as_deref(),
                    Some("https://mmbiz.qpic.cn/thumb1"),
                    "应提取封面图"
                );
            }
            _ => panic!("应为链接"),
        }
    }

    #[test]
    fn test_parse_appmsg_multi_article() {
        // 多图文消息：appmsg 内嵌 <mmreader>，item[0] 是头条，item[1..] 为子文章
        let xml = r#"<msg><appmsg type="5">
            <title><![CDATA[头条文章]]></title>
            <url><![CDATA[https://mp.weixin.qq.com/s?__biz=1]]></url>
            <mmreader><category type="20" count="2">
                <name><![CDATA[账号]]></name>
                <topnew><cover><![CDATA[https://mmbiz.qpic.cn/0]]></cover></topnew>
                <item><title><![CDATA[头条文章]]></title><url><![CDATA[https://mp.weixin.qq.com/s?__biz=1]]></url><cover><![CDATA[https://mmbiz.qpic.cn/0]]></cover></item>
                <item><title><![CDATA[第二条文章]]></title><url><![CDATA[https://mp.weixin.qq.com/s?__biz=2]]></url><cover><![CDATA[https://mmbiz.qpic.cn/1]]></cover></item>
            </category></mmreader>
        </appmsg></msg>"#;
        let media = parse_appmsg(xml).expect("应解析链接");
        match media {
            RichMedia::Link {
                title, articles, ..
            } => {
                assert_eq!(title, "头条文章");
                assert_eq!(articles.len(), 1, "子文章应只含 item[1..]");
                assert_eq!(articles[0].title, "第二条文章");
                assert_eq!(articles[0].url, "https://mp.weixin.qq.com/s?__biz=2");
                assert_eq!(articles[0].cover, "https://mmbiz.qpic.cn/1");
            }
            _ => panic!("应为链接"),
        }
    }

    #[test]
    fn test_parse_transfer_with_amount() {
        let xml = r#"<msg><appmsg type="2000"><title>微信转账</title><type>2000</type><wcpayinfo><paysubtype>1</paysubtype><feedesc><![CDATA[￥150.00]]></feedesc><pay_memo><![CDATA[测试]]></pay_memo><transferid>1000050001202602071139845838187</transferid></wcpayinfo></appmsg></msg>"#;
        let info = extract_wcpayinfo(xml);
        eprintln!("wcpayinfo={:?}", info);
        let media = parse_appmsg(xml).expect("应解析转账");
        match media {
            RichMedia::Transfer {
                amount,
                direction,
                paysubtype,
                fee_desc,
                transfer_id,
                ..
            } => {
                assert_eq!(amount, "150.00");
                assert_eq!(direction, "待收款");
                assert_eq!(paysubtype, "1");
                assert_eq!(fee_desc, "￥150.00");
                assert_eq!(transfer_id, "1000050001202602071139845838187");
            }
            _ => panic!("应为转账"),
        }
    }

    #[test]
    fn test_transfer_status_label_direction() {
        assert_eq!(transfer_status_label(true, "1"), "等待对方领取");
        assert_eq!(transfer_status_label(false, "1"), "待收款");
        assert_eq!(transfer_status_label(true, "3"), "已被接收");
        assert_eq!(transfer_status_label(false, "3"), "已收款");
        assert_eq!(transfer_status_label(true, "4"), "已退还");
        assert_eq!(transfer_status_label(true, "5"), "已过期退回");
    }

    #[test]
    fn test_is_transfer_status_type() {
        // 发起行不是状态行
        assert!(!is_transfer_status_type("1"));
        assert!(!is_transfer_status_type(""));
        // 收款/退还/过期/待领取等状态行
        assert!(is_transfer_status_type("3"));
        assert!(is_transfer_status_type("4"));
        assert!(is_transfer_status_type("5"));
        assert!(is_transfer_status_type("7"));
        assert!(is_transfer_status_type("8"));
        assert!(is_transfer_status_type("9"));
        assert!(is_transfer_status_type("10"));
    }

    #[test]
    fn test_parse_redpacket() {
        let xml = r#"<msg><appmsg type="2001"><title>微信红包</title><type>2001</type><wcpayinfo><paysubtype>3</paysubtype><feedesc><![CDATA[￥8.88]]></feedesc></wcpayinfo></appmsg></msg>"#;
        let media = parse_appmsg(xml).expect("应解析红包");
        match media {
            RichMedia::RedPacket {
                amount, paysubtype, ..
            } => {
                assert_eq!(amount, "8.88");
                assert_eq!(paysubtype, "3");
            }
            _ => panic!("应为红包"),
        }
    }

    #[test]
    fn test_parse_location_and_contact() {
        let loc = parse_rich_content(r#"<msg><location x="1" y="2" label="深圳市" poiname="腾讯大厦" infourl="https://example.com/map" /></msg>"#, 48)
            .expect("应解析位置");
        match loc {
            RichMedia::Location { poiname, url, .. } => {
                assert_eq!(poiname, "腾讯大厦");
                assert_eq!(url, "https://example.com/map");
            }
            _ => panic!("应为位置"),
        }
        let contact = parse_rich_content(
            r#"<msg><contact username="wxid_abc" nickname="张三" /></msg>"#,
            42,
        )
        .expect("应解析名片");
        match contact {
            RichMedia::Contact { nickname, username } => {
                assert_eq!(nickname, "张三");
                assert_eq!(username, "wxid_abc");
            }
            _ => panic!("应为名片"),
        }
    }

    #[test]
    fn test_parse_miniapp_icon_and_channels() {
        let miniapp = parse_appmsg(
            r#"<msg><appmsg type="33"><title>小程序</title><type>33</type><des>描述</des><url>u</url><sourcedisplayname>来源</sourcedisplayname><weappinfo><weappiconurl><![CDATA[https://icon.png]]></weappiconurl></weappinfo></appmsg></msg>"#,
        )
        .expect("应解析小程序");
        match miniapp {
            RichMedia::MiniApp {
                icon, des, source, ..
            } => {
                assert_eq!(icon.as_deref(), Some("https://icon.png"));
                assert_eq!(des.as_deref(), Some("描述"));
                assert_eq!(source, "来源");
            }
            _ => panic!("应为小程序"),
        }
        let channels = parse_appmsg(
            r#"<msg><appmsg type="51"><title>视频号内容</title><type>51</type><finderFeed><nickname><![CDATA[博主]]></nickname><desc><![CDATA[简介]]></desc></finderFeed></appmsg></msg>"#,
        )
        .expect("应解析视频号");
        match channels {
            RichMedia::Channels { nickname, desc, .. } => {
                assert_eq!(nickname.as_deref(), Some("博主"));
                assert_eq!(desc.as_deref(), Some("简介"));
            }
            _ => panic!("应为视频号"),
        }
    }

    #[test]
    fn test_miniapp_pagepath_extract() {
        // 腾讯文档分享：<url> 为空，真实网页链接藏在 weappinfo.pagepath 的 url= 参数里
        let xml = r#"<msg>
            <appmsg appid="" sdkver="0">
                <title>项目跟进表</title><des>腾讯文档</des><type>36</type><url />
                <weappinfo>
                    <username><![CDATA[gh_252c5f06840b@app]]></username>
                    <pagepath><![CDATA[/pages/detail/detail.html?scene=51a16d27117b5ef4c80cf8a2r5aCw1&url=https%3A%2F%2Fdocs.qq.com%2Fsheet%2FDT3RtV3hCandTVmdi]]></pagepath>
                    <weappiconurl><![CDATA[http://mmbiz.qpic.cn/1]]></weappiconurl>
                </weappinfo>
            </appmsg>
        </msg>"#;
        let media = parse_appmsg(xml).expect("应解析小程序");
        match media {
            RichMedia::MiniApp { pagepath, url, .. } => {
                let pp = pagepath.unwrap_or_default();
                assert!(
                    pp.contains("url=https%3A%2F%2Fdocs.qq.com"),
                    "应提取 pagepath: {}",
                    pp
                );
                assert!(url.is_empty(), "该消息 <url> 为空");
            }
            _ => panic!("应为小程序"),
        }
    }

    #[test]
    fn test_parse_mmreader() {
        // 真实腾讯新闻消息结构（截取自数据库样本）
        let xml = r#"<?xml version="1.0"?>
<mmreader>
	<category type="20" sub_type="0" groupid="2026072800" count="2">
		<name><![CDATA[腾讯新闻]]></name>
		<topnew>
			<cover><![CDATA[https://inews.gtimg.com/news_ls/TOP/0]]></cover>
			<width>640</width>
			<height>320</height>
		</topnew>
		<item>
			<title><![CDATA[苏有朋发文悼念]]></title>
			<url><![CDATA[https://view.inews.qq.com/w/AAA]]></url>
			<pub_time>1785193145</pub_time>
			<cover><![CDATA[https://inews.gtimg.com/news_ls/TOP/0]]></cover>
			<digest><![CDATA[]]></digest>
			<play_length>0</play_length>
			<tweettype>1</tweettype>
		</item>
		<newitem>
			<title><![CDATA[苏有朋发文悼念]]></title>
			<url><![CDATA[https://view.inews.qq.com/w/AAA]]></url>
			<cover><![CDATA[https://inews.gtimg.com/news_ls/TOP/0]]></cover>
		</newitem>
		<newitem>
			<title><![CDATA[南宁商场女厕21点后改为男女混用]]></title>
			<url><![CDATA[https://view.inews.qq.com/w/BBB]]></url>
			<cover><![CDATA[https://inews.gtimg.com/news_ls/SUB/0]]></cover>
			<digest><![CDATA[商场：建筑条件受限]]></digest>
		</newitem>
	</category>
</mmreader>"#;
        let media = parse_mmreader(xml).expect("应解析成功");
        match media {
            RichMedia::NewsFeed {
                name,
                top_cover,
                items,
            } => {
                assert_eq!(name, "腾讯新闻");
                assert_eq!(
                    top_cover.as_deref(),
                    Some("https://inews.gtimg.com/news_ls/TOP/0")
                );
                // 首条 item 与第一个 newitem 重复，应去重为 2 条
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].title, "苏有朋发文悼念");
                assert_eq!(items[0].url, "https://view.inews.qq.com/w/AAA");
                assert_eq!(items[1].title, "南宁商场女厕21点后改为男女混用");
                assert_eq!(items[1].cover, "https://inews.gtimg.com/news_ls/SUB/0");
                assert_eq!(items[1].digest, "商场：建筑条件受限");
            }
            _ => panic!("应解析为 NewsFeed"),
        }
    }

    #[test]
    fn test_parse_mmreader_reject_non_mmreader() {
        assert!(parse_mmreader("<msg><appmsg type=\"5\"/></msg>").is_none());
        assert!(parse_mmreader("普通文本").is_none());
    }

    /// 真实数据冒烟：从解密消息库读真实的转账/小程序/视频号消息并验证解析字段
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_parse_real_rich_messages() {
        let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let msg_dir = cfg.decrypted_dir.join("message");
        let mut transfer_ok = 0usize;
        let mut miniapp_ok = 0usize;
        let mut channels_ok = 0usize;
        let mut chatlog_ok = 0usize;
        let Ok(entries) = std::fs::read_dir(&msg_dir) else {
            return;
        };
        let mut dbs: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension().and_then(|e| e.to_str()) == Some("db")
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| {
                            (n.starts_with("message_") || n.starts_with("biz_message_"))
                                && !n.contains("fts")
                                && !n.contains("resource")
                                && !n.contains("media")
                        })
                        .unwrap_or(false)
            })
            .collect();
        dbs.sort();
        for db in &dbs {
            let Ok(conn) = rusqlite::Connection::open_with_flags(
                db,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ) else {
                continue;
            };
            let tabs: Vec<String> = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'Msg\\_%' ESCAPE '\\'")
                .ok()
                .and_then(|mut st| {
                    st.query_map([], |r| r.get::<_, String>(0))
                        .ok()
                        .map(|rows| rows.flatten().collect())
                })
                .unwrap_or_default();
            let mask = 1i64 << 32;
            for t in tabs {
                let sql = format!(
                    "SELECT message_content, compress_content FROM \"{}\" WHERE local_type % {} = 49 LIMIT 200",
                    t, mask
                );
                let Ok(rows) = conn.prepare(&sql) else {
                    continue;
                };
                let mut stmt = rows;
                let Ok(iter) = stmt.query_map([], |r| {
                    Ok((
                        crate::wechat::modules::common::get_bytes(r, 0),
                        crate::wechat::modules::common::get_bytes(r, 1),
                    ))
                }) else {
                    continue;
                };
                for row in iter.flatten() {
                    let bytes = row.0.or(row.1);
                    let Some(bytes) = bytes else { continue };
                    let xml = crate::wechat::modules::common::decode_blob_text(&bytes);
                    if let Some(media) = parse_appmsg(&xml) {
                        match media {
                            RichMedia::Transfer { amount, .. } => {
                                if !amount.is_empty() {
                                    transfer_ok += 1;
                                }
                            }
                            RichMedia::MiniApp { icon, .. } => {
                                if icon.is_some() {
                                    miniapp_ok += 1;
                                }
                            }
                            RichMedia::Channels { nickname, .. } => {
                                if nickname.is_some() {
                                    channels_ok += 1;
                                }
                            }
                            RichMedia::ChatLog { items, .. } => {
                                if !items.is_empty() {
                                    chatlog_ok += 1;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            drop(conn);
        }
        eprintln!(
            "真实富媒体：转账含金额 {} 条，小程序含图标 {} 条，视频号含作者 {} 条，聊天记录含条目 {} 条",
            transfer_ok, miniapp_ok, channels_ok, chatlog_ok
        );
        if transfer_ok == 0 && miniapp_ok == 0 && channels_ok == 0 && chatlog_ok == 0 {
            eprintln!("未发现可解析的真实富媒体消息，跳过");
            return;
        }
    }
}
