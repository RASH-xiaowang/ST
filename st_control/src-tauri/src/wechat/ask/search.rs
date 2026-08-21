// ============================================================
// 微信数据 AI 问答 — 检索执行与统计聚合
// 自 ask.rs 拆分：证据检索（消息/转账/红包/收藏/朋友圈）与
// 统计聚合（计数/会话/趋势）。
// ============================================================

use rusqlite::{Connection, OpenFlags};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::wechat::modules::{common, contacts, favorites, messages, moments, sessions};
use crate::wechat::{chat_search_index, general_records};

// truncate 已收敛至共享 crate::common（T-288），re-export 保持
// 本模块与 plan.rs/llm.rs 的 `super::truncate` 引用零改动
pub(crate) use crate::common::truncate;

use super::{
    date_to_epoch, is_group_activity_question, retrieve_recent_group_sessions, AggregationSpec,
    AskPlan, Citation, StatsTable,
};

// ============ 检索执行 ============

fn name_matches(hit: &serde_json::Value, target: &str) -> bool {
    hit.get("name")
        .and_then(|v| v.as_str())
        .map(|n| n == target || n.contains(target))
        .unwrap_or(false)
}

/// 计划关键词若是数据源类型词（红包/转账等），不作为过滤条件。
/// 这类词在记录表字段里不存在，直接过滤会把全部记录误杀。
pub(crate) fn non_type_keyword(kw: &str) -> String {
    let k = kw.trim().to_lowercase();
    if matches!(
        k.as_str(),
        "红包"
            | "转账"
            | "转帐"
            | "收款"
            | "付款"
            | "收红包"
            | "发红包"
            | "红包记录"
            | "转账记录"
            | "redpacket"
            | "red_packet"
            | "transfer"
    ) {
        String::new()
    } else {
        kw.trim().to_owned()
    }
}

/// 关键词命中的联系人 username 列表（显示名/备注/昵称/微信号匹配）
fn resolve_peer_usernames(decrypted: &Path, kw: &str) -> Vec<String> {
    if kw.is_empty() {
        return Vec::new();
    }
    contacts::get_contacts(decrypted)
        .map(|book| {
            book.contacts
                .iter()
                .filter(|c| {
                    c.display_name.contains(kw)
                        || c.remark.contains(kw)
                        || c.nick_name.contains(kw)
                        || c.username.contains(kw)
                })
                .map(|c| c.username.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// 记录是否匹配关键词：任一字段命中已解析的联系人 username，或包含关键词原文
pub(crate) fn record_matches(kw: &str, matched: &[String], fields: &[&str]) -> bool {
    for f in fields {
        if matched.iter().any(|u| u == f) {
            return true;
        }
        if f.contains(kw) {
            return true;
        }
    }
    false
}

pub(crate) fn fmt_ts(ts: i64) -> String {
    let secs = if ts > 10_000_000_000 { ts / 1000 } else { ts };
    if secs <= 0 {
        String::new()
    } else {
        common::format_full_time(secs)
    }
}

/// 取某会话的最近消息（时间倒序），供「我和X最近聊了什么」类问题直接取证。
/// 返回 (username, 会话显示名, local_id, ts, snippet)；可选时间/关键词过滤。
fn retrieve_session_recent_messages(
    decrypted: &Path,
    self_username: &str,
    username: &str,
    tf: Option<i64>,
    tt: Option<i64>,
    kw_filter: Option<&str>,
    limit: usize,
) -> Vec<(String, String, i64, i64, String)> {
    if limit == 0 {
        return Vec::new();
    }
    let page = match messages::get_conversation_messages(
        decrypted,
        username,
        self_username,
        None,
        limit.max(10),
    ) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    // 会话显示名：优先通讯录（备注 > 昵称），否则退回 chat_name/username
    let chat_name = contacts::load_display_names(&decrypted.join("contact").join("contact.db"))
        .get(username)
        .cloned()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| {
            if page.chat_name.trim().is_empty() {
                username.to_string()
            } else {
                page.chat_name.clone()
            }
        });
    let is_group = username.ends_with("@chatroom") || username.contains("@im.chatroom");
    let mut out: Vec<(String, String, i64, i64, String)> = Vec::new();
    for m in page.messages.into_iter().rev() {
        if let (Some(a), Some(b)) = (tf, tt) {
            if m.ts < a || m.ts > b {
                continue;
            }
        }
        let text = m.text.trim().to_string();
        let snippet = if text.is_empty() {
            format!("[{}]", m.type_label)
        } else if is_group {
            let sender = if m.is_self {
                "我".to_string()
            } else {
                m.sender_name.clone()
            };
            format!("{}: {}", sender, text)
        } else if m.is_self {
            format!("我: {}", text)
        } else {
            text.clone()
        };
        if let Some(kw) = kw_filter.filter(|k| !k.trim().is_empty()) {
            if !snippet.contains(kw) && !text.contains(kw) {
                continue;
            }
        }
        out.push((
            username.to_string(),
            chat_name.clone(),
            m.local_id,
            m.ts,
            snippet,
        ));
        if out.len() >= limit {
            break;
        }
    }
    out
}

pub(crate) fn execute_plan(
    q: &str,
    plan: &AskPlan,
    cfg: &crate::wechat::config::WeChatConfig,
    limit: usize,
) -> Vec<Citation> {
    let mut out: Vec<Citation> = Vec::new();
    let mut seen: HashSet<(String, i64)> = HashSet::new();
    let target = plan.target.clone().filter(|t| !t.is_empty());
    let tf = plan.time_from.as_deref().and_then(date_to_epoch);
    let tt = plan
        .time_to
        .as_deref()
        .and_then(date_to_epoch)
        .map(|e| e + 86399);

    let decrypted = cfg.decrypted_dir.clone();
    // 目标会话解析：target 可能是 username，也可能是显示名/备注。
    // 消息/朋友圈等检索都需要 username 形态，提前统一解析。
    let target_usernames: Vec<String> = match &target {
        Some(t) => {
            let mut v = resolve_peer_usernames(&decrypted, t);
            if v.is_empty() {
                v.push(t.clone());
            }
            v
        }
        None => Vec::new(),
    };
    let target_text = target.clone().unwrap_or_default();
    let self_username = cfg.wxid().unwrap_or_default();

    // 会话标识类关键词（目标显示名/备注）不是内容词：全文检索命中不了，
    // 还会在各数据源的内容过滤里把目标的结果误杀。先统一剔除，得到纯内容
    // 关键词（消息/朋友圈等所有「内容关键词过滤」都只能用 content_kws）。
    let target_names: Vec<String> = {
        let names = contacts::load_display_names(&decrypted.join("contact").join("contact.db"));
        target_usernames
            .iter()
            .filter_map(|u| names.get(u).cloned())
            .filter(|n| !n.trim().is_empty())
            .collect()
    };
    let content_kws: Vec<String> = plan
        .keywords
        .iter()
        .filter(|k| {
            let k = k.trim();
            if k.is_empty() {
                return false;
            }
            if !target_text.is_empty() && (target_text.contains(k) || k.contains(&target_text)) {
                return false;
            }
            !target_names
                .iter()
                .any(|n| n.contains(k) || k.contains(n.as_str()))
        })
        .cloned()
        .collect();

    // 1) 消息
    let skip_msg_for_stats =
        plan.aggregation.is_some() && plan.keywords.is_empty() && target.is_none();
    if plan.data_sources.iter().any(|s| s == "messages") && !skip_msg_for_stats {
        // 群活跃类问题（“在哪些群聊过天”）：直接按最近活跃群会话取证，比关键词搜消息更准
        if is_group_activity_question(q) {
            for c in retrieve_recent_group_sessions(cfg, q, limit) {
                if !seen.insert((c.username.clone(), c.local_id)) {
                    continue;
                }
                out.push(c);
                if out.len() >= limit {
                    break;
                }
            }
        } else if content_kws.is_empty() && target.is_some() {
            // 「我和X最近聊了什么 / X最近说了什么」：无内容关键词时，
            // 直接取目标会话的最近消息（比拿显示名当全文关键词搜更准）
            for u in target_usernames.iter().take(2) {
                let items = retrieve_session_recent_messages(
                    &decrypted,
                    &self_username,
                    u,
                    tf,
                    tt,
                    None,
                    limit.saturating_sub(out.len()),
                );
                for (username, name, local_id, ts, snippet) in items {
                    if !seen.insert((username.clone(), local_id)) {
                        continue;
                    }
                    out.push(Citation {
                        kind: "message",
                        username,
                        name,
                        local_id,
                        ts,
                        time: fmt_ts(ts),
                        snippet,
                    });
                    if out.len() >= limit {
                        break;
                    }
                }
                if out.len() >= limit {
                    break;
                }
            }
        } else {
            let mut kws = content_kws.clone();
            if kws.is_empty() {
                if let Some(t) = &target {
                    kws.push(t.clone());
                } else {
                    kws.push(q.to_string());
                }
            }
            let mut hits: Vec<serde_json::Value> = Vec::new();
            for kw in kws.iter().take(3) {
                match chat_search_index::search_indexed(kw, limit) {
                    Ok(indexed) => {
                        let indexed_ok = indexed
                            .get("indexed")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if indexed_ok {
                            if let Some(arr) = indexed.get("hits").and_then(|v| v.as_array()) {
                                // 索引命中但结果为 0 时，继续回退全表扫描（单字/口语短语 FTS 常搜不到）
                                if !arr.is_empty() {
                                    hits.extend(arr.clone());
                                } else if let Ok(scan) =
                                    crate::wechat::handlers::session::scan_search_messages(
                                        kw.clone(),
                                        Some(limit),
                                    )
                                {
                                    if let Some(arr2) = scan.get("hits").and_then(|v| v.as_array())
                                    {
                                        hits.extend(arr2.clone());
                                    }
                                }
                            }
                        } else if let Ok(scan) =
                            crate::wechat::handlers::session::scan_search_messages(
                                kw.clone(),
                                Some(limit),
                            )
                        {
                            if let Some(arr) = scan.get("hits").and_then(|v| v.as_array()) {
                                hits.extend(arr.clone());
                            }
                        }
                    }
                    Err(_) => {
                        if let Ok(scan) = crate::wechat::handlers::session::scan_search_messages(
                            kw.clone(),
                            Some(limit),
                        ) {
                            if let Some(arr) = scan.get("hits").and_then(|v| v.as_array()) {
                                hits.extend(arr.clone());
                            }
                        }
                    }
                }
            }
            // 命中按时间倒序：优先展示最近的消息（比 FTS 排序更符合「聊了什么」的直觉）
            hits.sort_by(|a, b| {
                let ta = a
                    .get("ts")
                    .and_then(|v| v.as_i64())
                    .or_else(|| a.get("create_time").and_then(|v| v.as_i64()))
                    .unwrap_or(0);
                let tb = b
                    .get("ts")
                    .and_then(|v| v.as_i64())
                    .or_else(|| b.get("create_time").and_then(|v| v.as_i64()))
                    .unwrap_or(0);
                tb.cmp(&ta)
            });
            let msg_before = out.len();
            for h in hits {
                let username = h
                    .get("username")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let local_id = h.get("local_id").and_then(|v| v.as_i64()).unwrap_or(0);
                let ts = h
                    .get("ts")
                    .and_then(|v| v.as_i64())
                    .or_else(|| h.get("create_time").and_then(|v| v.as_i64()))
                    .unwrap_or(0);
                if let (Some(a), Some(b)) = (tf, tt) {
                    if ts < a || ts > b {
                        continue;
                    }
                }
                if let Some(t) = &target {
                    if username != *t && !name_matches(&h, t) {
                        continue;
                    }
                }
                if !seen.insert((username.clone(), local_id)) {
                    continue;
                }
                out.push(Citation {
                    kind: "message",
                    username,
                    name: h
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    local_id,
                    ts,
                    time: h
                        .get("time")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    snippet: h
                        .get("snippet")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
                if out.len() >= limit {
                    break;
                }
            }
            // 空结果回退：有目标会话但关键词全文检索一无所获时，
            // 改为在目标会话的最近消息里按内容关键词过滤（比全文再扫一遍更准）
            if out.len() == msg_before && target.is_some() {
                let kw_filter = content_kws.first().map(|s| s.as_str());
                for u in target_usernames.iter().take(2) {
                    let items = retrieve_session_recent_messages(
                        &decrypted,
                        &self_username,
                        u,
                        tf,
                        tt,
                        kw_filter,
                        limit.saturating_sub(out.len()),
                    );
                    for (username, name, local_id, ts, snippet) in items {
                        if !seen.insert((username.clone(), local_id)) {
                            continue;
                        }
                        out.push(Citation {
                            kind: "message",
                            username,
                            name,
                            local_id,
                            ts,
                            time: fmt_ts(ts),
                            snippet,
                        });
                        if out.len() >= limit {
                            break;
                        }
                    }
                    if out.len() >= limit {
                        break;
                    }
                }
            }
        }
    }

    let kw = plan.keywords.first().cloned().unwrap_or_default();
    let room = limit.saturating_sub(out.len()).max(6);

    // 2) 转账（应用时间范围与目标会话过滤）
    if plan.data_sources.iter().any(|s| s == "transfers") {
        let kw_filter = non_type_keyword(&kw);
        let matched = resolve_peer_usernames(&decrypted, &kw_filter);
        if let Ok(r) = general_records::list_transfers(Some(room as i64), Some(0), None) {
            if let Some(items) = r.get("items").and_then(|v| v.as_array()) {
                for it in items.iter().take(room) {
                    let session = it
                        .get("session_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let ts = it
                        .get("begin_transfer_time")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let payer = it
                        .get("pay_payer")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let receiver = it
                        .get("pay_receiver")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    // 时间范围
                    if let (Some(a), Some(b)) = (tf, tt) {
                        if ts < a || ts > b {
                            continue;
                        }
                    }
                    // 目标会话：session 直接命中目标 username/显示名，或经由联系人解析命中
                    if !target_text.is_empty() {
                        let hit = target_usernames.contains(&session)
                            || session.contains(&target_text)
                            || payer.contains(&target_text)
                            || receiver.contains(&target_text);
                        if !hit {
                            continue;
                        }
                    }
                    if !kw_filter.is_empty()
                        && !record_matches(&kw_filter, &matched, &[&session, &payer, &receiver])
                    {
                        continue;
                    }
                    let sub = it
                        .get("pay_sub_type")
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    if !seen.insert(("transfer".to_string(), ts)) {
                        continue;
                    }
                    out.push(Citation {
                        kind: "transfer",
                        username: session.clone(),
                        name: if session.is_empty() {
                            "转账".to_string()
                        } else {
                            session
                        },
                        local_id: 0,
                        ts,
                        time: fmt_ts(ts),
                        snippet: format!(
                            "{} → {} · 类型 {}",
                            truncate(&payer, 24),
                            truncate(&receiver, 24),
                            sub
                        ),
                    });
                    if out.len() >= limit {
                        break;
                    }
                }
            }
        }
    }

    // 3) 红包（redEnvelopeTable 无真实时间戳列，按目标/关键词过滤；时间范围不支持）
    if plan.data_sources.iter().any(|s| s == "redpackets") {
        let kw_filter = non_type_keyword(&kw);
        let matched = resolve_peer_usernames(&decrypted, &kw_filter);
        if let Ok(r) = general_records::list_red_envelopes(Some(room as i64), Some(0), None) {
            if let Some(items) = r.get("items").and_then(|v| v.as_array()) {
                for it in items.iter().take(room) {
                    let session = it
                        .get("session_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let sender = it
                        .get("sender_user_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    // 目标会话过滤
                    if !target_text.is_empty() {
                        let hit = target_usernames.contains(&session)
                            || session.contains(&target_text)
                            || sender.contains(&target_text);
                        if !hit {
                            continue;
                        }
                    }
                    if !kw_filter.is_empty()
                        && !record_matches(&kw_filter, &matched, &[&session, &sender])
                    {
                        continue;
                    }
                    let ts = it
                        .get("message_server_id")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let status = it
                        .get("hb_status")
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    if !seen.insert(("redpacket".to_string(), ts)) {
                        continue;
                    }
                    out.push(Citation {
                        kind: "redpacket",
                        username: session.clone(),
                        name: if session.is_empty() {
                            "红包".to_string()
                        } else {
                            session
                        },
                        local_id: 0,
                        ts,
                        time: String::new(),
                        snippet: format!("发送人 {} · 状态 {}", truncate(&sender, 24), status),
                    });
                    if out.len() >= limit {
                        break;
                    }
                }
            }
        }
    }

    // 4) 联系人
    if plan.data_sources.iter().any(|s| s == "contacts") {
        if let Ok(book) = contacts::get_contacts(&decrypted) {
            for c in book.contacts {
                if out.len() >= limit {
                    break;
                }
                let display = c.display_name.clone();
                if !kw.is_empty()
                    && !display.contains(&kw)
                    && !c.username.contains(&kw)
                    && !c.remark.contains(&kw)
                    && !c.nick_name.contains(&kw)
                {
                    continue;
                }
                if let Some(t) = &target {
                    if c.username != *t && !display.contains(t) {
                        continue;
                    }
                }
                let snippet = if !c.remark.is_empty() {
                    format!("备注 {} · 微信号 {}", c.remark, c.alias)
                } else if !c.alias.is_empty() {
                    format!("微信号 {}", c.alias)
                } else {
                    c.category.clone()
                };
                out.push(Citation {
                    kind: "contact",
                    username: c.username.clone(),
                    name: display,
                    local_id: 0,
                    ts: 0,
                    time: String::new(),
                    snippet,
                });
            }
        }
    }

    // 5) 朋友圈（应用时间范围与目标作者过滤）
    if plan.data_sources.iter().any(|s| s == "moments") {
        let self_wxid = cfg.wxid().unwrap_or_default();
        // 指定作者时后端按 user_name 精确过滤取（不受全局最新 300 条窗口
        // 限制——活跃度低的作者会被 300 条窗口漏掉大部分动态）；
        // 作者解析失败/查询失败时回退全局窗口 + 逐条作者名匹配
        let items: Vec<crate::wechat::modules::moments::MomentEntry> = match target_usernames
            .first()
        {
            Some(a) => {
                match moments::get_moments_page(&decrypted, &self_wxid, 0, limit.max(24), Some(a)) {
                    Ok(p) if !p.items.is_empty() => p.items,
                    _ => moments::get_moments_page(&decrypted, &self_wxid, 0, 300, None)
                        .map(|p| p.items)
                        .unwrap_or_default(),
                }
            }
            None => moments::get_moments_page(&decrypted, &self_wxid, 0, 300, None)
                .map(|p| p.items)
                .unwrap_or_default(),
        };
        for m in items {
            if out.len() >= limit {
                break;
            }
            if let (Some(a), Some(b)) = (tf, tt) {
                if m.ts < a || m.ts > b {
                    continue;
                }
            }
            if !target_text.is_empty() {
                let hit = target_usernames.contains(&m.username) || m.author.contains(&target_text);
                if !hit {
                    continue;
                }
            }
            // 内容关键词：只用剔除会话标识后的 content_kws（「王勤」这类
            // 人名是作者标识，不是动态内容词）
            if !content_kws.is_empty() && !content_kws.iter().any(|k| m.text.contains(k)) {
                continue;
            }
            out.push(Citation {
                kind: "moment",
                username: m.username.clone(),
                name: m.author.clone(),
                local_id: 0,
                ts: m.ts,
                time: m.time,
                snippet: truncate(&m.text, 120),
            });
        }
    }

    // 6) 收藏（应用时间范围与来源过滤）
    if plan.data_sources.iter().any(|s| s == "favorites") {
        if let Ok(data) = favorites::get_favorites(&decrypted, 500) {
            if let Some(items) = data.get("items").and_then(|v| v.as_array()) {
                for it in items.iter() {
                    if out.len() >= limit {
                        break;
                    }
                    let title = it
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let desc = it
                        .get("desc")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let type_label = it
                        .get("type_label")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let source = it
                        .get("source")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let ts = it.get("ts").and_then(|v| v.as_i64()).unwrap_or(0);
                    if let (Some(a), Some(b)) = (tf, tt) {
                        if ts < a || ts > b {
                            continue;
                        }
                    }
                    if !target_text.is_empty() && !source.contains(&target_text) {
                        continue;
                    }
                    if !kw.is_empty() && !title.contains(&kw) && !desc.contains(&kw) {
                        continue;
                    }
                    let local_id = it.get("local_id").and_then(|v| v.as_i64()).unwrap_or(0);
                    out.push(Citation {
                        kind: "favorite",
                        username: source.clone(),
                        name: format!(
                            "{} · {}",
                            type_label,
                            if title.is_empty() { "收藏" } else { &title }
                        ),
                        local_id,
                        ts,
                        time: it
                            .get("time")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        snippet: truncate(if desc.is_empty() { &title } else { &desc }, 120),
                    });
                }
            }
        }
    }

    out.truncate(limit);
    out
}

// ============ 统计/聚合工具 ============

fn agg_time_bounds(tf: Option<&str>, tt: Option<&str>) -> (Option<i64>, Option<i64>) {
    let from = tf.and_then(date_to_epoch);
    let to = tt.and_then(date_to_epoch).map(|e| e + 86399);
    (from, to)
}

/// 把显示名/微信号解析为可用于 SQL 过滤的 username（username 形态直接放行）
fn resolve_agg_target(decrypted: &Path, target: &str) -> Option<String> {
    let t = target.trim();
    if t.is_empty() {
        return None;
    }
    if t.contains('@')
        || t.starts_with("wxid_")
        || t.starts_with("gh_")
        || t.starts_with("v3_")
        || t.starts_with("wc_")
    {
        return Some(t.to_string());
    }
    let contact_db = decrypted.join("contact").join("contact.db");
    for (u, d) in contacts::load_display_names(&contact_db) {
        if d == t || d.contains(t) {
            return Some(u);
        }
    }
    if let Ok(list) = sessions::get_session_list(decrypted) {
        for s in list {
            if s.name == t || s.name.contains(t) {
                return Some(s.username);
            }
        }
    }
    Some(t.to_string())
}

fn stats_index_conn() -> Result<Connection, String> {
    let p = crate::wechat::config::default_st_result_dir().join("wechat_search.db");
    if !p.is_file() {
        return Err("未找到消息搜索索引，请先在「聊天搜索」中构建消息索引后重试统计".to_string());
    }
    Connection::open_with_flags(
        &p,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("打开消息搜索索引失败: {}", e))
}

fn stats_index_empty(conn: &Connection) -> bool {
    conn.query_row("SELECT COUNT(*) FROM message_meta", [], |r| {
        r.get::<_, i64>(0)
    })
    .unwrap_or(0)
        == 0
}

/// 搜索索引是否新鲜（12 小时内构建/更新过）。
/// 索引是手动/一次性构建的快照，长时间不重建会漏掉新消息——
/// 统计必须优先用新鲜的索引，过期时改走分库直聚合（永远最新），
/// 否则「最近和谁聊得最多」类问题会给出低估的计数和错位的排行。
fn index_is_fresh() -> bool {
    let Ok(conn) = stats_index_conn() else {
        return false;
    };
    let Ok(max_ts) = conn.query_row(
        "SELECT COALESCE(MAX(create_time),0) FROM message_meta",
        [],
        |r| r.get::<_, i64>(0),
    ) else {
        return false;
    };
    if max_ts <= 0 {
        return false;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    now.saturating_sub(max_ts) < 12 * 3600
}

struct MessageWhere {
    sql: String,
    params: Vec<rusqlite::types::Value>,
}

/// 从聚合参数构造消息过滤条件（会话/时间/关键词）
fn build_message_where(decrypted: &Path, spec: &AggregationSpec) -> MessageWhere {
    let mut parts: Vec<String> = Vec::new();
    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(t) = spec.target.as_deref().filter(|t| !t.trim().is_empty()) {
        if let Some(u) = resolve_agg_target(decrypted, t) {
            parts.push("username = ?".to_string());
            params.push(rusqlite::types::Value::Text(u));
        }
    }
    let (tf, tt) = agg_time_bounds(spec.time_from.as_deref(), spec.time_to.as_deref());
    if let Some(f) = tf {
        parts.push("create_time >= ?".to_string());
        params.push(rusqlite::types::Value::Integer(f));
    }
    if let Some(t) = tt {
        parts.push("create_time <= ?".to_string());
        params.push(rusqlite::types::Value::Integer(t));
    }
    if let Some(k) = spec.keyword.as_deref().filter(|k| !k.trim().is_empty()) {
        // 内容关键词仅对「消息计数」有意义：排行/趋势的维度是会话，
        // 疑问词碎片（谁/最/多）会把整个统计误杀成空表
        if spec.kind == "count_messages" {
            parts.push("text LIKE ?".to_string());
            params.push(rusqlite::types::Value::Text(format!("%{}%", k.trim())));
        }
    }
    let sql = if parts.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", parts.join(" AND "))
    };
    MessageWhere { sql, params }
}

fn display_name_map(decrypted: &Path) -> std::collections::HashMap<String, String> {
    let usernames = crate::wechat::annual::load_session_usernames(decrypted);
    crate::wechat::annual::load_display_names(decrypted, &usernames)
}

fn agg_count_messages(decrypted: &Path, spec: &AggregationSpec) -> Result<StatsTable, String> {
    // 索引新鲜才可用：过期快照会漏掉新消息，计数偏低
    if index_is_fresh() {
        if let Ok(t) = agg_count_messages_indexed(decrypted, spec) {
            if !t.rows.is_empty() && !t.summary.starts_with("共 0 条") {
                return Ok(t);
            }
        }
    }
    agg_count_messages_shards(decrypted, spec)
}

fn agg_count_messages_indexed(
    decrypted: &Path,
    spec: &AggregationSpec,
) -> Result<StatsTable, String> {
    let conn = stats_index_conn()?;
    if stats_index_empty(&conn) {
        return Err("消息搜索索引为空，请先构建消息索引".to_string());
    }
    let w = build_message_where(decrypted, spec);
    let total: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM message_meta{}", w.sql),
            rusqlite::params_from_iter(w.params.iter()),
            |r| r.get(0),
        )
        .unwrap_or(0);
    let names = display_name_map(decrypted);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut columns = vec!["会话".to_string(), "消息数".to_string()];
    if spec
        .target
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .is_none()
    {
        let sql = format!(
            "SELECT username, COUNT(*) AS c FROM message_meta{} GROUP BY username ORDER BY c DESC LIMIT ?",
            w.sql
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let mut params = w.params.clone();
            params.push(rusqlite::types::Value::Integer(
                spec.limit.clamp(1, 20) as i64
            ));
            if let Ok(rs) = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
                Ok((
                    r.get::<_, String>(0).unwrap_or_default(),
                    r.get::<_, i64>(1).unwrap_or(0),
                ))
            }) {
                for (u, c) in rs.flatten() {
                    let name = names.get(&u).cloned().unwrap_or_else(|| u.clone());
                    rows.push(vec![name, format!("{}", c)]);
                }
            }
        }
    }
    if rows.is_empty() {
        columns = vec!["指标".to_string(), "数值".to_string()];
        rows.push(vec!["消息条数".to_string(), format!("{}", total)]);
    }
    Ok(StatsTable {
        title: "消息统计".to_string(),
        columns,
        rows,
        summary: format!("共 {} 条文本消息", total),
    })
}

fn agg_top_sessions(decrypted: &Path, spec: &AggregationSpec) -> Result<StatsTable, String> {
    // 索引新鲜才可用：过期快照会漏掉新消息，排行错位
    if index_is_fresh() {
        if let Ok(t) = agg_top_sessions_indexed(decrypted, spec) {
            if !t.rows.is_empty() {
                return Ok(t);
            }
        }
    }
    agg_top_sessions_shards(decrypted, spec)
}

fn agg_top_sessions_indexed(
    decrypted: &Path,
    spec: &AggregationSpec,
) -> Result<StatsTable, String> {
    let conn = stats_index_conn()?;
    if stats_index_empty(&conn) {
        return Err("消息搜索索引为空，请先构建消息索引".to_string());
    }
    let w = build_message_where(decrypted, spec);
    let mut sql = format!("SELECT username, COUNT(*) AS c FROM message_meta{}", w.sql);
    if spec.group_only {
        if w.sql.is_empty() {
            sql.push_str(" WHERE username LIKE '%@chatroom%'");
        } else {
            sql.push_str(" AND username LIKE '%@chatroom%'");
        }
    }
    sql.push_str(" GROUP BY username ORDER BY c DESC LIMIT ?");
    let names = display_name_map(decrypted);
    let mut rows: Vec<Vec<String>> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&sql) {
        let mut params = w.params.clone();
        params.push(rusqlite::types::Value::Integer(
            spec.limit.clamp(1, 20) as i64
        ));
        if let Ok(rs) = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
            Ok((
                r.get::<_, String>(0).unwrap_or_default(),
                r.get::<_, i64>(1).unwrap_or(0),
            ))
        }) {
            for (u, c) in rs.flatten() {
                let name = names.get(&u).cloned().unwrap_or_else(|| u.clone());
                rows.push(vec![name, format!("{}", c)]);
            }
        }
    }
    let title = if spec.group_only {
        "群聊活跃度排行".to_string()
    } else {
        "会话活跃度排行".to_string()
    };
    let top_n = rows.len();
    Ok(StatsTable {
        title,
        columns: vec!["会话".to_string(), "消息数".to_string()],
        rows,
        summary: format!("按文本消息量排序，Top {}", top_n),
    })
}

fn agg_message_trend(decrypted: &Path, spec: &AggregationSpec) -> Result<StatsTable, String> {
    // 索引新鲜才可用：过期快照会漏掉新消息，月度分布失真
    if index_is_fresh() {
        if let Ok(t) = agg_message_trend_indexed(decrypted, spec) {
            if !t.rows.is_empty() {
                return Ok(t);
            }
        }
    }
    agg_message_trend_shards(decrypted, spec)
}

fn agg_message_trend_indexed(
    decrypted: &Path,
    spec: &AggregationSpec,
) -> Result<StatsTable, String> {
    let conn = stats_index_conn()?;
    if stats_index_empty(&conn) {
        return Err("消息搜索索引为空，请先构建消息索引".to_string());
    }
    let w = build_message_where(decrypted, spec);
    let sql = format!(
        "SELECT strftime('%Y-%m', create_time, 'unixepoch', 'localtime') AS m, COUNT(*) AS c \
         FROM message_meta{} GROUP BY m ORDER BY m",
        w.sql
    );
    let mut rows: Vec<Vec<String>> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&sql) {
        if let Ok(rs) = stmt.query_map(rusqlite::params_from_iter(w.params.iter()), |r| {
            Ok((
                r.get::<_, String>(0).unwrap_or_default(),
                r.get::<_, i64>(1).unwrap_or(0),
            ))
        }) {
            for (m, c) in rs.flatten() {
                rows.push(vec![m, format!("{}", c)]);
            }
        }
    }
    Ok(StatsTable {
        title: "月度消息趋势".to_string(),
        columns: vec!["月份".to_string(), "消息数".to_string()],
        rows,
        summary: "按月份统计文本消息量".to_string(),
    })
}

fn agg_count_transfers(decrypted: &Path, spec: &AggregationSpec) -> Result<StatsTable, String> {
    let (tf, tt) = agg_time_bounds(spec.time_from.as_deref(), spec.time_to.as_deref());
    let target = spec
        .target
        .as_deref()
        .and_then(|t| resolve_agg_target(decrypted, t));
    let r = general_records::stats_transfers(target.as_deref(), tf, tt)?;
    let total = r.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
    let sessions = r
        .get("sessions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut rows: Vec<Vec<String>> = sessions
        .iter()
        .take(spec.limit.clamp(1, 20))
        .map(|s| {
            vec![
                s.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                s.get("count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
                    .to_string(),
            ]
        })
        .collect();
    let mut columns = vec!["会话".to_string(), "笔数".to_string()];
    if rows.is_empty() {
        columns = vec!["指标".to_string(), "数值".to_string()];
        rows.push(vec!["转账笔数".to_string(), format!("{}", total)]);
    }
    Ok(StatsTable {
        title: "转账统计".to_string(),
        columns,
        rows,
        summary: format!("共 {} 笔转账", total),
    })
}

fn agg_count_redpackets(decrypted: &Path, spec: &AggregationSpec) -> Result<StatsTable, String> {
    let (tf, tt) = agg_time_bounds(spec.time_from.as_deref(), spec.time_to.as_deref());
    let target = spec
        .target
        .as_deref()
        .and_then(|t| resolve_agg_target(decrypted, t));
    let r = general_records::stats_redpackets(target.as_deref(), tf, tt)?;
    let total = r.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
    let sessions = r
        .get("sessions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut rows: Vec<Vec<String>> = sessions
        .iter()
        .take(spec.limit.clamp(1, 20))
        .map(|s| {
            vec![
                s.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                s.get("count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
                    .to_string(),
            ]
        })
        .collect();
    let mut columns = vec!["会话".to_string(), "个数".to_string()];
    if rows.is_empty() {
        columns = vec!["指标".to_string(), "数值".to_string()];
        rows.push(vec!["红包个数".to_string(), format!("{}", total)]);
    }
    let range_note = match (spec.time_from.as_deref(), spec.time_to.as_deref()) {
        (Some(a), Some(b)) if a == b => format!("（{}）", a),
        (Some(a), Some(b)) => format!("（{} ~ {}）", a, b),
        _ => String::new(),
    };
    Ok(StatsTable {
        title: "红包统计".to_string(),
        columns,
        rows,
        summary: format!("共 {} 个红包{}", total, range_note),
    })
}

/// 执行统计/聚合子任务
pub(crate) fn execute_aggregation(
    decrypted: &Path,
    spec: &AggregationSpec,
) -> Result<StatsTable, String> {
    match spec.kind.as_str() {
        "count_messages" => agg_count_messages(decrypted, spec),
        "top_sessions" => agg_top_sessions(decrypted, spec),
        "message_trend" => agg_message_trend(decrypted, spec),
        "count_transfers" => agg_count_transfers(decrypted, spec),
        "count_redpackets" => agg_count_redpackets(decrypted, spec),
        other => Err(format!("未知统计类型: {}", other)),
    }
}

// ============ 分库直聚合（无搜索索引时的兜底） ============
//
// 消息统计原本依赖 wechat_search.db 搜索索引（用户需先手动构建）。
// 未构建索引时统计直接失败——「问我的微信」对统计类问题永远答不出。
// 兜底方案：直接从解密消息分库（message_*.db / biz_message_*.db）的
// Msg_<md5> 表按 create_time / 表名聚合，与索引结果等价且始终可用。

/// 分库消息表清单：(username, 所在分库路径, 表名)
fn shard_msg_tables(decrypted: &Path) -> Vec<(String, PathBuf, String)> {
    let mut dbs = common::find_db_files(decrypted, "message_");
    dbs.extend(common::find_db_files(decrypted, "biz_message_"));
    dbs.sort();
    dbs.dedup();
    dbs.retain(|p| !p.to_string_lossy().contains("monitor_cache"));
    dbs.retain(|p| common::is_message_shard_file(p));

    let mut out = Vec::new();
    for path in dbs {
        let Ok(conn) = common::open_readonly_db(&path) else {
            continue;
        };
        let Ok(mut stmt) = conn.prepare("SELECT user_name FROM Name2Id") else {
            continue;
        };
        let names: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map(|rs| rs.flatten().collect())
            .unwrap_or_default();
        for username in names {
            if username.is_empty() {
                continue;
            }
            let table = common::msg_table_name(&username);
            if common::table_exists(&conn, &table) {
                out.push((username, path.clone(), table));
            }
        }
    }
    out
}

/// 按 spec 构造分库 SQL 过滤片段（作用于单表；username 已在选表时确定）。
/// 统一只统计文本消息（local_type=1），与搜索索引 message_meta 的口径一致。
fn shard_where(spec: &AggregationSpec, params: &mut Vec<rusqlite::types::Value>) -> String {
    let mut parts: Vec<String> = vec!["local_type = 1".to_string()];
    let (tf, tt) = agg_time_bounds(spec.time_from.as_deref(), spec.time_to.as_deref());
    if let Some(f) = tf {
        parts.push("create_time >= ?".to_string());
        params.push(rusqlite::types::Value::Integer(f));
    }
    if let Some(t) = tt {
        parts.push("create_time <= ?".to_string());
        params.push(rusqlite::types::Value::Integer(t));
    }
    if let Some(k) = spec.keyword.as_deref().filter(|k| !k.trim().is_empty()) {
        // 同 build_message_where：内容关键词仅对 count_messages 有意义，
        // 排行/趋势的维度是会话，疑问词碎片会把统计误杀成空表
        if spec.kind == "count_messages" {
            parts.push("message_content LIKE ?".to_string());
            params.push(rusqlite::types::Value::Text(format!("%{}%", k.trim())));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", parts.join(" AND "))
    }
}

/// 执行分库聚合：f 接收 (连接, 表名, where_sql, params) 返回本表的部分结果；
/// 由调用方决定如何合并（计数求和 / 排行合并 / 趋势按月相加）。
fn run_shard_agg<T>(
    decrypted: &Path,
    spec: &AggregationSpec,
    f: impl Fn(&Connection, &str, &str, &[rusqlite::types::Value]) -> Option<T>,
) -> Vec<T> {
    let tables = shard_msg_tables(decrypted);
    // 目标会话过滤：表名本身已对应 username
    let target = spec
        .target
        .as_deref()
        .and_then(|t| resolve_agg_target(decrypted, t))
        .filter(|t| !t.trim().is_empty());
    let mut out = Vec::new();
    for (username, path, table) in tables {
        if let Some(t) = &target {
            if username != *t {
                continue;
            }
        }
        let Ok(conn) = common::open_readonly_db(&path) else {
            continue;
        };
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        let where_sql = shard_where(spec, &mut params);
        if let Some(v) = f(&conn, &table, &where_sql, &params) {
            out.push(v);
        }
    }
    out
}

fn agg_count_messages_shards(
    decrypted: &Path,
    spec: &AggregationSpec,
) -> Result<StatsTable, String> {
    let target = spec
        .target
        .as_deref()
        .and_then(|t| resolve_agg_target(decrypted, t))
        .filter(|t| !t.trim().is_empty());
    let names = display_name_map(decrypted);
    // 指定会话：直接求和
    if let Some(t) = &target {
        let total: i64 = run_shard_agg(decrypted, spec, |conn, table, w, params| {
            let sql = format!("SELECT COUNT(*) FROM \"{table}\"{w}");
            conn.query_row(&sql, rusqlite::params_from_iter(params.iter()), |r| {
                r.get::<_, i64>(0)
            })
            .ok()
        })
        .iter()
        .sum();
        let name = names.get(t).cloned().unwrap_or_else(|| t.clone());
        return Ok(StatsTable {
            title: "消息统计".to_string(),
            columns: vec!["会话".to_string(), "消息数".to_string()],
            rows: vec![vec![name.clone(), format!("{total}")]],
            summary: format!("与「{}」共 {} 条文本消息", name, total),
        });
    }
    // 无目标：逐会话计数后取 Top N
    let mut rows: Vec<(String, i64)> = Vec::new();
    for (username, path, table) in shard_msg_tables(decrypted) {
        let Ok(conn) = common::open_readonly_db(&path) else {
            continue;
        };
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        let w = shard_where(spec, &mut params);
        let sql = format!("SELECT COUNT(*) FROM \"{table}\"{w}");
        if let Ok(c) = conn.query_row(&sql, rusqlite::params_from_iter(params.iter()), |r| {
            r.get::<_, i64>(0)
        }) {
            if c > 0 {
                rows.push((username, c));
            }
        }
    }
    rows.sort_by_key(|a| std::cmp::Reverse(a.1));
    let total: i64 = rows.iter().map(|(_, c)| c).sum();
    let top_n = rows.len().min(spec.limit.clamp(1, 20));
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .take(top_n)
        .map(|(u, c)| {
            vec![
                names.get(u).cloned().unwrap_or_else(|| u.clone()),
                format!("{c}"),
            ]
        })
        .collect();
    Ok(StatsTable {
        title: "消息统计".to_string(),
        columns: vec!["会话".to_string(), "消息数".to_string()],
        rows: table_rows,
        summary: format!("共 {} 条文本消息，Top {top_n}", total),
    })
}

fn agg_top_sessions_shards(decrypted: &Path, spec: &AggregationSpec) -> Result<StatsTable, String> {
    let names = display_name_map(decrypted);
    let mut rows: Vec<(String, i64)> = Vec::new();
    for (username, path, table) in shard_msg_tables(decrypted) {
        if spec.group_only && !username.contains("@chatroom") {
            continue;
        }
        let Ok(conn) = common::open_readonly_db(&path) else {
            continue;
        };
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        let w = shard_where(spec, &mut params);
        let sql = format!("SELECT COUNT(*) FROM \"{table}\"{w}");
        if let Ok(c) = conn.query_row(&sql, rusqlite::params_from_iter(params.iter()), |r| {
            r.get::<_, i64>(0)
        }) {
            if c > 0 {
                rows.push((username, c));
            }
        }
    }
    rows.sort_by_key(|a| std::cmp::Reverse(a.1));
    let top_n = rows.len().min(spec.limit.clamp(1, 20));
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .take(top_n)
        .map(|(u, c)| {
            vec![
                names.get(u).cloned().unwrap_or_else(|| u.clone()),
                format!("{c}"),
            ]
        })
        .collect();
    let title = if spec.group_only {
        "群聊活跃度排行".to_string()
    } else {
        "会话活跃度排行".to_string()
    };
    Ok(StatsTable {
        title,
        columns: vec!["会话".to_string(), "消息数".to_string()],
        rows: table_rows,
        summary: format!("按文本消息量排序，Top {top_n}"),
    })
}

fn agg_message_trend_shards(
    decrypted: &Path,
    spec: &AggregationSpec,
) -> Result<StatsTable, String> {
    use std::collections::BTreeMap;
    let mut months: BTreeMap<String, i64> = BTreeMap::new();
    for (_, path, table) in shard_msg_tables(decrypted) {
        let Ok(conn) = common::open_readonly_db(&path) else {
            continue;
        };
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        let w = shard_where(spec, &mut params);
        let sql = format!(
            "SELECT strftime('%Y-%m', create_time, 'unixepoch', 'localtime') AS m, COUNT(*) \
             FROM \"{table}\"{w} GROUP BY m"
        );
        let query = conn.prepare(&sql);
        if let Ok(mut stmt) = query {
            let mapped = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
                Ok((
                    r.get::<_, String>(0).unwrap_or_default(),
                    r.get::<_, i64>(1).unwrap_or(0),
                ))
            });
            if let Ok(rs) = mapped {
                let part: Vec<(String, i64)> = rs.flatten().collect();
                for (m, c) in part {
                    if m.is_empty() {
                        continue;
                    }
                    *months.entry(m).or_insert(0) += c;
                }
            }
        }
    }
    let rows: Vec<Vec<String>> = months
        .iter()
        .map(|(m, c)| vec![m.clone(), format!("{c}")])
        .collect();
    Ok(StatsTable {
        title: "月度消息趋势".to_string(),
        columns: vec!["月份".to_string(), "消息数".to_string()],
        rows,
        summary: "按月份统计文本消息量（直接聚合解密分库）".to_string(),
    })
}
