// ============================================================
// 年度总结 — 工具函数
// 自 annual.rs 拆分：时间格式、纯文本/短语/表情判定、类型标签。
// ============================================================

use crate::wechat::modules::common;

// ─── 时间工具 ───

pub(crate) fn year_range(year: i32) -> (i64, i64) {
    use chrono::{Local, TimeZone};
    let start = Local
        .with_ymd_and_hms(year, 1, 1, 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp())
        .unwrap_or(0);
    let end = Local
        .with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp())
        .unwrap_or(0);
    (start, end)
}

pub(crate) fn local_datetime(ts: i64) -> Option<chrono::NaiveDateTime> {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.naive_local())
}

pub(crate) fn fmt_time(ts: i64) -> String {
    local_datetime(ts)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

pub(crate) fn fmt_date(ts: i64) -> String {
    local_datetime(ts)
        .map(|dt| dt.format("%m-%d").to_string())
        .unwrap_or_default()
}

/// 提取纯文本（去除 XML 标签；消息内容为空返回空串）
pub(crate) fn plain_text(content: &str) -> String {
    let c = content.trim_start();
    if c.starts_with('<') || c.starts_with("<?xml") {
        let s = common::strip_xml_tags(c);
        s.trim().to_string()
    } else {
        c.to_string()
    }
}

pub(crate) fn is_valid_phrase(s: &str) -> bool {
    if s.is_empty() || s.len() > 12 {
        return false;
    }
    let lower = s.to_lowercase();
    if lower.contains("http://") || lower.contains("https://") {
        return false;
    }
    if s.starts_with('<') {
        return false;
    }
    // 必须包含中文或字母
    s.chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c) || c.is_ascii_alphabetic())
}

pub(crate) fn is_emoji_char(c: char) -> bool {
    matches!(c as u32,
        0x1F300..=0x1FAFF
        | 0x2600..=0x27BF
        | 0x2300..=0x23FF
        | 0x2B00..=0x2BFF
        | 0x1F000..=0x1F0FF
    )
}

pub(crate) fn kind_label(t: i64) -> &'static str {
    let t = common::normalize_msg_type(t);
    match t {
        1 => "text",
        3 => "image",
        34 => "voice",
        43 => "video",
        47 => "emoji",
        49 => "link",
        50 => "voip",
        10000 => "system",
        _ => "other",
    }
}

pub(crate) fn kind_label_zh(k: &str) -> &'static str {
    match k {
        "text" => "文字",
        "emoji" => "表情",
        "voice" => "语音",
        "image" => "图片",
        "video" => "视频",
        "link" => "链接",
        "file" => "文件",
        "system" => "系统",
        "other" => "其他",
        _ => "其他",
    }
}
