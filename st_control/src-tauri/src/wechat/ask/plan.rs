// ============================================================
// 微信数据 AI 问答 — 启发式规划（无 LLM 时的兜底）
// 自 ask.rs 拆分：关键词提取、时间/目标解析、数据源检测与聚合判定。
// ============================================================

use chrono::{Datelike, Local, NaiveDate, TimeZone};
use std::collections::HashSet;
use std::path::Path;

use crate::wechat::modules::{contacts, sessions};

use super::{default_limit, fmt_ts, truncate, AggregationSpec, AskPlan, Citation};

// ============ 启发式规划（无 LLM 时的兜底） ============

const STOPWORDS: &[&str] = &[
    "我", "我们", "我的", "的", "了", "吗", "呢", "啊", "吧", "和", "与", "在", "有", "什么",
    "哪些", "多少", "怎么", "如何", "帮我", "看看", "找", "查", "一下", "请", "问", "是", "都",
    "也", "就", "给", "把", "被", "关于", "最近", "今年", "去年", "本月", "上月", "上周", "昨天",
    "今天", "年", "月", "日", "聊天", "消息", "记录", "内容", "时候", "期间", "聊", "说过",
];

fn tokenize(q: &str) -> Vec<String> {
    q.split(|c: char| c.is_whitespace() || "，。？！、；：,.?!;:".contains(c))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub(crate) fn extract_keywords(q: &str) -> Vec<String> {
    // 先剥离常见时间/数据源短语，避免它们被当成搜索关键词
    let mut text = q.to_string();
    for phrase in [
        "哪些群",
        "什么群",
        "哪个群",
        "那些",
        "这些",
        "里面",
        "聊过天",
        "聊过",
        "说过话",
        "上个星期",
        "上礼拜",
        "上个月",
        "这个月",
        "本月",
        "上月",
        "上周",
        "前天",
        "昨天",
        "今天",
        "今年",
        "去年",
        "最近",
        "近",
        "以内",
        "内",
    ] {
        text = text.replace(phrase, " ");
    }
    // 功能词/助词作为切分边界，把连续中文短语拆成有意义的词
    let seps: Vec<char> =
        "，。？！、；：,.?!;: 我你他她它我们你们他们咱们的得地了和与或及在有没有是都是也就要给把被对着向从为以于关于请问帮我看看找查一下什么哪些多少怎么如何吗呢吧啊呀哦嗯还".chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<String>, seen: &mut HashSet<String>| {
        let t = cur.trim().to_string();
        cur.clear();
        if t.is_empty() {
            return;
        }
        // 中文单字也可能是有效关键词（群/钱/车），仅排除单个英数字/符号
        if t.chars().count() == 1 && !is_cjk(&t) {
            return;
        }
        if STOPWORDS.contains(&t.as_str()) {
            return;
        }
        // 纯年份/日期数字不作为关键词
        if t.chars().all(|c| c.is_ascii_digit()) && (t.len() == 4 || t.len() == 8) {
            return;
        }
        if seen.insert(t.clone()) {
            out.push(t);
        }
    };
    for ch in text.chars() {
        if seps.contains(&ch) {
            flush(&mut cur, &mut out, &mut seen);
        } else {
            cur.push(ch);
        }
    }
    flush(&mut cur, &mut out, &mut seen);
    out
}

/// 是否为纯 CJK 文本（用于放行中文单字关键词）
pub(crate) fn is_cjk(s: &str) -> bool {
    s.chars().all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

/// 群聊活跃度类问题（“在哪些群聊过天”）：直接检索会话列表比关键词搜消息更准
pub(crate) fn is_group_activity_question(q: &str) -> bool {
    let group_hint = [
        "哪些群",
        "什么群",
        "哪个群",
        "群里面",
        "群里",
        "群聊",
        "群组",
        "群活跃",
        "在群里",
        "在哪些群",
    ];
    let activity_hint = [
        "聊过",
        "聊了",
        "聊天",
        "说过话",
        "活跃",
        "消息",
        "最近",
        "发言",
        "聊过天",
    ];
    group_hint.iter().any(|s| q.contains(s)) && activity_hint.iter().any(|s| q.contains(s))
}

/// 最近活跃的群会话（按最后消息时间倒序），作为群活跃类问题的证据
pub(crate) fn retrieve_recent_group_sessions(
    cfg: &crate::wechat::config::WeChatConfig,
    q: &str,
    limit: usize,
) -> Vec<Citation> {
    let Ok(list) = sessions::get_session_list(&cfg.decrypted_dir) else {
        return Vec::new();
    };
    let (tf, tt) = parse_time_hints(q)
        .map(|(a, b)| {
            (
                a.as_deref().and_then(date_to_epoch),
                b.as_deref().and_then(date_to_epoch),
            )
        })
        .unwrap_or((None, None));
    let mut out: Vec<Citation> = list
        .into_iter()
        .filter(|s| s.username.contains("@chatroom"))
        .filter(|s| s.ts > 0)
        .filter(|s| match (tf, tt) {
            (Some(a), Some(b)) => s.ts >= a && s.ts <= b + 86399,
            _ => true,
        })
        .map(|s| Citation {
            kind: "message",
            username: s.username.clone(),
            name: s.name.clone(),
            local_id: 0,
            ts: s.ts,
            time: fmt_ts(s.ts),
            snippet: if s.summary.is_empty() {
                "群消息".to_string()
            } else {
                truncate(&s.summary, 120)
            },
        })
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.ts));
    out.truncate(limit);
    out
}

fn detect_sources(q: &str) -> Vec<String> {
    let mut srcs = vec!["messages".to_string()];
    for (kw, src) in [
        ("转账", "transfers"),
        ("打钱", "transfers"),
        ("红包", "redpackets"),
        ("收藏", "favorites"),
        ("朋友圈", "moments"),
        ("动态", "moments"),
        ("联系人", "contacts"),
        ("通讯录", "contacts"),
        ("好友", "contacts"),
        ("公众号", "contacts"),
    ] {
        if q.contains(kw) && !srcs.contains(&src.to_string()) {
            srcs.push(src.to_string());
        }
    }
    srcs
}

fn naive_to_epoch(d: NaiveDate) -> i64 {
    Local
        .from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap_or_default())
        .single()
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

/// 某年某月的最后一天
fn month_end(y: i32, m: u32) -> Option<NaiveDate> {
    let next =
        NaiveDate::from_ymd_opt(y, m + 1, 1).or_else(|| NaiveDate::from_ymd_opt(y + 1, 1, 1))?;
    next.checked_sub_days(chrono::Days::new(1))
}

pub(crate) fn date_to_epoch(s: &str) -> Option<i64> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .ok()
        .map(naive_to_epoch)
}

/// 从问题中解析时间范围（Unix 秒，含当天），识别：具体日期(范围)、YYYY年M月、YYYY年、
/// 今年/去年、本月/上月、上周、昨天/今天、最近N天
fn parse_time_hints(q: &str) -> Option<(Option<String>, Option<String>)> {
    let now = Local::now();
    let today = now.date_naive();

    // 1) 两个具体日期之间的范围（至/到/~）
    let date_re = regex::Regex::new(r"(20\d{2})[-/年](\d{1,2})[-/月](\d{1,2})日?").ok()?;
    let dates: Vec<(i32, u32, u32)> = date_re
        .captures_iter(q)
        .filter_map(|c| {
            Some((
                c[1].parse::<i32>().ok()?,
                c[2].parse::<u32>().ok()?,
                c[3].parse::<u32>().ok()?,
            ))
        })
        .collect();
    if dates.len() >= 2 {
        let a = NaiveDate::from_ymd_opt(dates[0].0, dates[0].1, dates[0].2)?;
        let b = NaiveDate::from_ymd_opt(dates[1].0, dates[1].1, dates[1].2)?;
        return Some((
            Some(a.format("%Y-%m-%d").to_string()),
            Some(b.format("%Y-%m-%d").to_string()),
        ));
    }
    if dates.len() == 1 {
        let a = NaiveDate::from_ymd_opt(dates[0].0, dates[0].1, dates[0].2)?;
        return Some((
            Some(a.format("%Y-%m-%d").to_string()),
            Some(a.format("%Y-%m-%d").to_string()),
        ));
    }

    // 2) YYYY年M月
    let ym_re = regex::Regex::new(r"(20\d{2})年(\d{1,2})月").ok()?;
    if let Some(c) = ym_re.captures(q) {
        let y = c[1].parse::<i32>().ok()?;
        let m = c[2].parse::<u32>().ok()?;
        if let Some(start) = NaiveDate::from_ymd_opt(y, m, 1) {
            let end = month_end(y, m)?;
            return Some((
                Some(start.format("%Y-%m-%d").to_string()),
                Some(end.format("%Y-%m-%d").to_string()),
            ));
        }
    }

    // 3) YYYY年
    let y_re = regex::Regex::new(r"(20\d{2})年").ok()?;
    if let Some(c) = y_re.captures(q) {
        if let Ok(y) = c[1].parse::<i32>() {
            if let Some(start) = NaiveDate::from_ymd_opt(y, 1, 1) {
                return Some((
                    Some(start.format("%Y-%m-%d").to_string()),
                    Some(format!("{}-12-31", y)),
                ));
            }
        }
    }

    // 4) 相对时间词
    if q.contains("今年") {
        let y = today.year();
        return Some((Some(format!("{}-01-01", y)), Some(format!("{}-12-31", y))));
    }
    if q.contains("去年") {
        let y = today.year() - 1;
        return Some((Some(format!("{}-01-01", y)), Some(format!("{}-12-31", y))));
    }
    if q.contains("这个月") || q.contains("本月") {
        let start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)?;
        let end = month_end(today.year(), today.month())?;
        return Some((
            Some(start.format("%Y-%m-%d").to_string()),
            Some(end.format("%Y-%m-%d").to_string()),
        ));
    }
    if q.contains("上个月") || q.contains("上月") {
        let (y, m) = if today.month() == 1 {
            (today.year() - 1, 12)
        } else {
            (today.year(), today.month() - 1)
        };
        let start = NaiveDate::from_ymd_opt(y, m, 1)?;
        let end = month_end(y, m)?;
        return Some((
            Some(start.format("%Y-%m-%d").to_string()),
            Some(end.format("%Y-%m-%d").to_string()),
        ));
    }
    if q.contains("上个星期") || q.contains("上周") || q.contains("上礼拜") {
        let weekday = today.weekday().num_days_from_monday();
        let monday = today.checked_sub_days(chrono::Days::new(weekday as u64 + 7))?;
        let sunday = monday.checked_add_days(chrono::Days::new(6))?;
        return Some((
            Some(monday.format("%Y-%m-%d").to_string()),
            Some(sunday.format("%Y-%m-%d").to_string()),
        ));
    }
    if q.contains("前天") {
        let d = today.checked_sub_days(chrono::Days::new(2))?;
        return Some((
            Some(d.format("%Y-%m-%d").to_string()),
            Some(d.format("%Y-%m-%d").to_string()),
        ));
    }
    if q.contains("昨天") {
        let d = today.checked_sub_days(chrono::Days::new(1))?;
        return Some((
            Some(d.format("%Y-%m-%d").to_string()),
            Some(d.format("%Y-%m-%d").to_string()),
        ));
    }
    if q.contains("今天") {
        return Some((
            Some(today.format("%Y-%m-%d").to_string()),
            Some(today.format("%Y-%m-%d").to_string()),
        ));
    }
    // 5) 最近N天 / 近N天 / N天内
    let n_re = regex::Regex::new(r"(?:最近|近)\s*(\d{1,3})\s*天|(\d{1,3})\s*天(?:内|以内)").ok()?;
    if let Some(c) = n_re.captures(q) {
        let n = c
            .get(1)
            .or_else(|| c.get(2))
            .and_then(|m| m.as_str().parse::<u64>().ok())?;
        let start = today.checked_sub_days(chrono::Days::new(n))?;
        return Some((
            Some(start.format("%Y-%m-%d").to_string()),
            Some(today.format("%Y-%m-%d").to_string()),
        ));
    }
    // 6) 裸「最近/近期」（无具体天数）：默认最近 30 天
    //（如「我最近和谁聊的最多」——时间限定近期，否则全时段排行会答非所问）
    if q.contains("最近") || q.contains("近期") {
        let start = today.checked_sub_days(chrono::Days::new(29))?;
        return Some((
            Some(start.format("%Y-%m-%d").to_string()),
            Some(today.format("%Y-%m-%d").to_string()),
        ));
    }
    None
}

/// 解析目标会话/联系人（最长显示名匹配；username 直接命中优先）
fn resolve_target(q: &str, decrypted_dir: &Path) -> Option<String> {
    let contact_db = decrypted_dir.join("contact").join("contact.db");
    let names = contacts::load_display_names(&contact_db);
    // username 直接命中（含 @chatroom / gh_ / wxid_）
    for tok in tokenize(q) {
        if names.contains_key(&tok) {
            return Some(tok);
        }
    }
    let mut best: Option<(usize, String)> = None;
    for (username, display) in &names {
        let display = display.trim();
        if display.is_empty() || display.chars().count() < 2 {
            continue;
        }
        if q.contains(display) {
            let len = display.chars().count();
            if best.as_ref().is_none_or(|(l, _)| len > *l) {
                best = Some((len, username.clone()));
            }
        }
    }
    best.map(|(_, u)| u)
}

pub(crate) fn heuristic_plan(q: &str) -> AskPlan {
    let mut plan = AskPlan {
        keywords: extract_keywords(q),
        target: None,
        time_from: None,
        time_to: None,
        data_sources: detect_sources(q),
        aggregation: heuristic_aggregation(q),
        limit: default_limit(),
        rationale: "未配置模型，使用关键词/时间/数据源启发式规划".to_string(),
    };
    if let Some((tf, tt)) = parse_time_hints(q) {
        plan.time_from = tf;
        plan.time_to = tt;
    }
    // 统计类问题的关键词处理：
    // - 排行/趋势类维度是会话不是内容 → 清空关键词，避免「谁聊/最」等
    //   疑问词碎片触发无关消息检索，污染答案
    // - 计数类只保留实质性内容词，避免把「月/群/最」等口语碎片当搜索词
    if let Some(agg) = &plan.aggregation {
        if agg.kind == "top_sessions" || agg.kind == "message_trend" {
            plan.keywords.clear();
        } else {
            plan.keywords
                .retain(|k| k.chars().count() >= 2 && !is_timeish_keyword(k));
        }
    }
    if let Ok(cfg) = crate::wechat::config::WeChatConfig::load() {
        let t = resolve_target(q, &cfg.decrypted_dir);
        if plan.target.is_none() {
            plan.target = t.clone();
        }
        if let Some(agg) = &mut plan.aggregation {
            if agg.target.is_none() {
                agg.target = t.clone();
            }
        }
    }
    plan
}

/// 启发式识别统计类问题（“一共多少条 / 谁最活跃 / 几笔转账”等）。
/// 这些问题的答案来自聚合计算，而不是逐条引用，所以单独走统计工具。
fn heuristic_aggregation(q: &str) -> Option<AggregationSpec> {
    let (tf, tt) = parse_time_hints(q).unwrap_or((None, None));
    // 关键词候选：必须是有实质内容的名词（人名/项目名等），排除疑问词碎片。
    // 「我上个月和谁聊得最多」里的「谁聊」若被当作内容关键词，
    // 会把全部消息误杀（内容里根本没有「谁聊」两个字）。
    let kw = extract_keywords(q).into_iter().find(|k| {
        !k.is_empty()
            && ![
                "消息", "聊天", "记录", "群", "群聊", "条", "笔", "个", "多少", "几",
            ]
            .iter()
            .any(|s| k.contains(s))
            && !is_timeish_keyword(k)
            && ![
                "谁",
                "最",
                "多",
                "哪",
                "哪些",
                "几个",
                "最多",
                "最活跃",
                "活跃",
                "聊",
            ]
            .iter()
            .any(|s| k.contains(s))
    });
    let group_only =
        q.contains("群") && (q.contains("最活跃") || q.contains("活跃") || q.contains("聊得最多"));

    if q.contains("几笔")
        || q.contains("多少笔")
        || q.contains("转账") && (q.contains("几") || q.contains("多少"))
    {
        return Some(AggregationSpec {
            kind: "count_transfers".to_string(),
            target: None,
            time_from: tf,
            time_to: tt,
            keyword: kw,
            group_only: false,
            limit: 10,
        });
    }
    if q.contains("红包") && (q.contains("几个") || q.contains("多少个") || q.contains("多少"))
    {
        return Some(AggregationSpec {
            kind: "count_redpackets".to_string(),
            target: None,
            time_from: tf,
            time_to: tt,
            keyword: kw,
            group_only: false,
            limit: 10,
        });
    }
    if (q.contains("多少条")
        || q.contains("几条")
        || q.contains("多少消息")
        || q.contains("发过多少"))
        && q.contains("消息")
        || q.contains("聊了多少")
    {
        return Some(AggregationSpec {
            kind: "count_messages".to_string(),
            target: None,
            time_from: tf,
            time_to: tt,
            keyword: kw,
            group_only: false,
            limit: 10,
        });
    }
    // 排行类触发词：「聊得/聊的最多」「最活跃」「最常聊」等口语变体都覆盖；
    // 与「和谁/跟谁/哪个」连用的「最多」也视为会话排行
    let rank_hint = [
        "聊得最多",
        "聊的最多",
        "聊最多",
        "聊过最多",
        "聊得最频繁",
        "聊的最频繁",
        "联系最多",
        "联系最频繁",
        "最活跃",
        "最常聊",
        "最常联系",
        "消息最多",
        "发消息最多",
    ]
    .iter()
    .any(|s| q.contains(s))
        || (q.contains("最多")
            && (q.contains("和谁聊") || q.contains("跟谁聊") || q.contains("和哪个")));
    if rank_hint {
        // 排行/趋势类统计的维度是「会话」而非「内容」：keyword 恒为空，
        // 否则疑问词碎片会把整个排行误杀成空表
        return Some(AggregationSpec {
            kind: "top_sessions".to_string(),
            target: None,
            time_from: tf,
            time_to: tt,
            keyword: None,
            group_only,
            limit: 10,
        });
    }
    if q.contains("趋势") || q.contains("分布") || q.contains("每个月") || q.contains("按月")
    {
        return Some(AggregationSpec {
            kind: "message_trend".to_string(),
            target: None,
            time_from: tf,
            time_to: tt,
            keyword: None,
            group_only: false,
            limit: 12,
        });
    }
    None
}

/// 时间类短语不应作为统计关键词（时间已单独解析）
fn is_timeish_keyword(k: &str) -> bool {
    [
        "上周",
        "上个月",
        "这个月",
        "本月",
        "上月",
        "昨天",
        "今天",
        "前天",
        "最近",
        "去年",
        "今年",
        "上礼拜",
        "这个礼拜",
        "近",
    ]
    .iter()
    .any(|t| k.contains(t))
}
