// ============================================================
// 微信富媒体消息解析 — XML 工具层
// 自 media.rs 拆分：轻量 XML 标签/属性/嵌套提取与文本清洗。
// ============================================================

// ============ 辅助函数 ============

/// 压缩空白字符
pub(crate) fn collapse_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 去掉 CDATA 包装（`<![CDATA[...]]>` → `...`）
pub(crate) fn clean_cdata(s: String) -> String {
    let s = s.trim().to_string();
    if let Some(inner) = s
        .strip_prefix("<![CDATA[")
        .and_then(|x| x.strip_suffix("]]>"))
    {
        inner.trim().to_string()
    } else {
        s
    }
}

/// 从 XML 中查找标签的文本内容（自动去除 `<![CDATA[...]]>` 包装）。
///
/// 微信 appmsg 的 title/des/url 等字段普遍用 CDATA 包裹，若保留标记，
/// 前端拿到的 url 会变成 `<![CDATA[http://...]]>` 导致链接打不开。
pub(crate) fn get_tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let s = xml.find(&open)?;
    let content_start = s + open.len();
    let e = xml[content_start..].find(&close)?;
    Some(clean_cdata(
        xml[content_start..content_start + e].to_string(),
    ))
}

/// 从 XML 中查找标签的整数值
pub(crate) fn get_tag_int(xml: &str, tag: &str) -> Option<i64> {
    get_tag_text(xml, tag).and_then(|s| s.trim().parse::<i64>().ok())
}

/// 从 XML 中查找属性的值
pub(crate) fn find_attr(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let tag_start = xml.find(&format!("<{} ", tag))?;
    let tag_end = xml[tag_start..].find('>')?;
    let tag_str = &xml[tag_start..tag_start + tag_end];

    let search = format!("{}=\"", attr);
    let attr_start = tag_str.find(&search)?;
    let value_start = attr_start + search.len();
    let value_end = tag_str[value_start..].find('"')?;
    Some(tag_str[value_start..value_start + value_end].to_string())
}

/// 提取嵌套标签的文本
pub(crate) fn extract_nested(xml: &str, outer: &str, inner: &str) -> Option<String> {
    let outer_start = xml.find(&format!("<{}", outer))?;
    let outer_close = format!("</{}>", outer);
    let outer_end = xml[outer_start..].find(&outer_close)?;
    let outer_str = &xml[outer_start..outer_start + outer_end];
    get_tag_text(outer_str, inner)
}

/// 解析嵌套标签的整数值
pub(crate) fn parse_nested_int(xml: &str, outer: &str, inner: &str) -> Option<i64> {
    extract_nested(xml, outer, inner).and_then(|s| s.trim().parse::<i64>().ok())
}
