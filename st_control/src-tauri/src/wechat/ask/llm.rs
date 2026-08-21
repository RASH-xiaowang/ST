// ============================================================
// 微信数据 AI 问答 — LLM 规划 / 反思 / 回答
// 自 ask.rs 拆分：模型选择、检索计划生成、证据自评与引用回答。
// ============================================================

use crate::llm::types::ChatMessage;
use std::collections::HashSet;

use super::{heuristic_plan, truncate, AskHistoryItem, AskPlan, Citation, StatsTable};

// ============ LLM 提供方解析 ============

/// 选择用于问答的模型：优先恢复上次聊天所用，其次默认提供方，最后第一个启用提供方
fn llm_provider() -> Option<(crate::llm::types::ProviderConfig, String)> {
    let cfg = crate::llm::config::load_config();
    let pid = cfg
        .last_chat_provider_id
        .clone()
        .or_else(|| cfg.default_provider_id.clone());
    if let Some(pid) = pid {
        if let Some(p) = crate::llm::config::find_provider(&cfg, &pid) {
            if p.enabled {
                let model = cfg
                    .last_chat_model
                    .clone()
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| p.default_model.clone());
                return Some((p.clone(), model));
            }
        }
    }
    cfg.providers.iter().find(|p| p.enabled).map(|p| {
        let model = if p.default_model.is_empty() {
            p.models.first().cloned().unwrap_or_default()
        } else {
            p.default_model.clone()
        };
        (p.clone(), model)
    })
}

/// 解析 LLM 返回中的 JSON 对象（容忍 ```json 围栏与前后杂讯）
fn parse_json_object(text: &str) -> Option<serde_json::Value> {
    let t = text.trim();
    let start = t.find('{')?;
    let end = t.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&t[start..=end]).ok()
}

/// JSON 整体解析失败（max_tokens 截断/引号干扰）时的兜底：
/// 直接扫描 `"answer":"...` 字段值并按 JSON 转义规则解码。
/// 保证即使模型输出被截断，用户也能看到干净的回答正文，
/// 而不是整段残缺 JSON 原文。
pub(crate) fn extract_answer_field(text: &str) -> Option<String> {
    let t = text.trim();
    let idx = t.find("\"answer\"")?;
    let rest = t[idx + "\"answer\"".len()..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let inner = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('u') => {
                    // \uXXXX：近似处理——跳过 4 位十六进制（中文转义极少见）
                    let _: Vec<char> = chars.by_ref().take(4).collect();
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => break,
            },
            '"' => break,
            other => out.push(other),
        }
    }
    if out.trim().is_empty() {
        None
    } else {
        Some(out)
    }
}

// ============ LLM 规划与回答 ============

const PLAN_SYSTEM_PROMPT: &str = r#"你是微信聊天数据检索规划助手。用户会问关于自己本地微信数据的问题。
请把问题拆解为检索计划，只输出 JSON（不要多余文字、不要 Markdown）：
{
  "keywords": ["关键词1", "关键词2"],
  "target": "目标会话/联系人（username 或显示名，没有则为 null）",
  "time_from": "起始日期 YYYY-MM-DD 或 null",
  "time_to": "结束日期 YYYY-MM-DD 或 null",
  "data_sources": ["messages"],
  "aggregation": null,
  "limit": 24,
  "rationale": "一句话说明检索依据"
}
说明：
- 提示词会给出「今天」的具体日期；上个月/上周/最近N天等相对时间必须换算成具体的 YYYY-MM-DD 填入 time_from/time_to，不允许留空。
- data_sources 可选：messages（聊天消息）、transfers（转账）、redpackets（红包）、contacts（联系人）、moments（朋友圈）、favorites（收藏）；一般默认 ["messages"]。
- 关键词要具体可检索，去掉口语和疑问词；目标会话要精确到用户名或显示名。
- 没有时间线索时 time_from/time_to 为 null。
- aggregation 用于「数量 / 排行 / 趋势」类统计问题，普通内容查询为 null。格式：
  {"kind":"count_messages|top_sessions|message_trend|count_transfers|count_redpackets","target":null,"time_from":null,"time_to":null,"keyword":null,"group_only":false,"limit":10}
  适用场景：
  * 「我和张三上周聊了多少条消息」→ count_messages（target=张三，时间范围=上周；keyword 仅当用户指定了消息内容主题如「提到项目」时才填，否则 null）
  * 「我上个月和谁聊得最多」「哪些群最活跃」→ top_sessions（群聊用 group_only=true；keyword 必须为 null——排行的维度是会话不是内容）
  * 「今年的消息趋势 / 按月的分布」→ message_trend（keyword 必须为 null）
  * 「我去年转了几笔账」「红包有多少个」→ count_transfers / count_redpackets
  统计问题同时保留 keywords/target 供普通检索交叉验证，但 aggregation 是主要答案来源。
- 若问题依赖上一轮对话（如「那去年呢」「具体是谁」），依据对话历史补全 target 与时间，而不是当作全新问题。"#;

const REFLECT_SYSTEM_PROMPT: &str = r#"你是微信问答的检索质量检查员。根据问题、已检索到的证据与统计结果，判断证据是否足够回答用户问题。
只输出 JSON（不要 Markdown）：
{
  "sufficient": true 或 false,
  "relevant_indices": [与问题相关的证据序号，如 [1,3,5]；空数组表示全部保留],
  "refinement": null 或检索计划对象 {keywords,target,time_from,time_to,data_sources,aggregation,limit,rationale},
  "reason": "一句话说明"
}
规则：
- 证据必须能明确支撑答案才可 sufficient=true；统计结果（总数/排行/趋势）本身就是证据。
- 证据不足、关键词不对、时间/会话缺失或需要更完整历史时，sufficient=false 并给出 refinement（换关键词 / 扩时间范围 / 换数据源 / 补 aggregation）。
- refinement 不要重复已经检索过且无效的条件；保持 data_sources 非空。
- relevant_indices 用于过滤噪音引用；空数组表示全部保留。"#;

const ANSWER_SYSTEM_PROMPT: &str = r#"你是微信聊天数据问答助手。用户的问题只能依据下面提供的证据回答。
要求：
1. 用中文简洁回答，先给结论再给细节；
2. 引用证据时在句子末尾标注【序号】，例如【1】【2】；
3. 统计类问题优先给出准确数字（X 条 / X 笔 / Top 排行），统计结果无需编造引用；
4. 证据不足时明确说明"根据现有记录无法确认"，不要编造；
5. 涉及金额/时间/人物时以证据原文为准；
6. 用户提示词里会给出「今天」的日期，相对时间（最近/上个月）以此为准换算，不要自己猜测日期。
只输出 JSON（不要 Markdown）：
{"answer": "回答内容（可含【1】标注）", "citation_indices": [1, 2, 3]}"#;

fn chat_messages(system: &str, user: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".to_string(),
            content: system.to_string(),
            parts: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: user.to_string(),
            parts: None,
        },
    ]
}

/// 把最近几轮对话历史格式化为提示词片段（供追问消解）
fn format_history(history: &[AskHistoryItem]) -> String {
    if history.is_empty() {
        return String::new();
    }
    let mut lines = vec!["对话历史：".to_string()];
    for (i, h) in history.iter().enumerate() {
        let a = if h.answer.trim().is_empty() {
            "（无回答）".to_string()
        } else {
            truncate(h.answer.trim(), 240)
        };
        lines.push(format!("{}. 问：{}\n   答：{}", i + 1, h.question, a));
    }
    lines.join("\n")
}

pub(crate) async fn resolve_plan(q: &str, history: &[AskHistoryItem]) -> AskPlan {
    let heuristic = heuristic_plan(q);
    let Some((provider, model)) = llm_provider() else {
        return heuristic;
    };
    // 「今天」的具体日期：LLM 无法自行得知当前时间，相对时间（上个月/最近N天）
    // 全靠这行换算成具体日期，缺失会导致时间范围留空或乱猜
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let user = if history.is_empty() {
        format!("今天是 {}。\n问题：{}", today, q)
    } else {
        format!(
            "今天是 {}。\n{}\n\n当前问题：{}",
            today,
            format_history(history),
            q
        )
    };
    match crate::llm::client::chat_completion(
        &provider,
        &crate::llm::client::CompletionParams {
            model: &model,
            messages: &chat_messages(PLAN_SYSTEM_PROMPT, &user),
            max_tokens: Some(800),
            temperature: Some(0.1),
            top_p: None,
            presence_penalty: None,
            frequency_penalty: None,
            tools: None,
            tool_choice: None,
        },
    )
    .await
    {
        Ok((text, _, _, _)) => {
            if let Some(v) = parse_json_object(&text) {
                if let Ok(plan) = serde_json::from_value::<AskPlan>(v) {
                    if !plan.data_sources.is_empty() {
                        let mut plan = plan;
                        // LLM 未识别出统计需求时，回退启发式识别结果（避免统计问题走普通检索）
                        if plan.aggregation.is_none() {
                            plan.aggregation = heuristic.aggregation.clone();
                        }
                        // 相对时间常被 LLM 留空 → 用启发式解析结果补齐
                        if plan.time_from.is_none() && plan.time_to.is_none() {
                            plan.time_from = heuristic.time_from.clone();
                            plan.time_to = heuristic.time_to.clone();
                        }
                        // LLM 的 target 常是显示名而非 username：显示名无法直接
                        // 查消息库，需换成启发式解析出的 username
                        let t = plan.target.as_deref().unwrap_or("").trim().to_string();
                        let is_username_form = t.contains('@')
                            || t.starts_with("wxid_")
                            || t.starts_with("gh_")
                            || t.starts_with("v3_")
                            || t.starts_with("wc_")
                            || t.starts_with("filehelper");
                        if !is_username_form && heuristic.target.is_some() {
                            plan.target = heuristic.target.clone();
                        }
                        // 聚合子任务同样补齐时间/目标（「上个月和谁聊得最多」类问题
                        // 的时间范围在 aggregation 里，plan 级时间常为 null）
                        if let Some(agg) = &mut plan.aggregation {
                            if let Some(h) = &heuristic.aggregation {
                                if agg.target.is_none() {
                                    agg.target = h.target.clone();
                                }
                                if agg.time_from.is_none() && agg.time_to.is_none() {
                                    agg.time_from = h.time_from.clone();
                                    agg.time_to = h.time_to.clone();
                                }
                                if agg.keyword.is_none() {
                                    agg.keyword = h.keyword.clone();
                                }
                                if h.group_only {
                                    agg.group_only = true;
                                }
                            }
                        }
                        return plan;
                    }
                }
            }
            heuristic
        }
        Err(_) => heuristic,
    }
}

pub(crate) struct ReflectResult {
    pub(crate) sufficient: bool,
    pub(crate) relevant: Vec<usize>,
    pub(crate) refinement: Option<AskPlan>,
}

/// 自评：已检索证据是否足够回答；不足时返回补检计划
pub(crate) async fn reflect_evidence(
    q: &str,
    history: &[AskHistoryItem],
    plan: &AskPlan,
    citations: &[Citation],
    stats: &[StatsTable],
) -> ReflectResult {
    let default = ReflectResult {
        sufficient: true,
        relevant: Vec::new(),
        refinement: None,
    };
    let Some((provider, model)) = llm_provider() else {
        return default;
    };
    let evidence: Vec<String> = citations
        .iter()
        .enumerate()
        .take(60)
        .map(|(i, c)| {
            format!(
                "[{}] 会话「{}」({}) {}：{}",
                i + 1,
                c.name,
                c.kind_label(),
                c.time,
                truncate(&c.snippet, 100)
            )
        })
        .collect();
    let stats_text: Vec<String> = stats
        .iter()
        .map(|t| format!("【{}】{}", t.title, t.summary))
        .collect();
    let user = format!(
        "{}\n\n检索计划：{}\n\n检索到 {} 条证据：\n{}\n\n统计数据：\n{}\n\n当前问题：{}",
        format_history(history),
        plan.rationale,
        citations.len(),
        if evidence.is_empty() {
            "（无）".to_string()
        } else {
            evidence.join("\n")
        },
        if stats_text.is_empty() {
            "（无）".to_string()
        } else {
            stats_text.join("\n")
        },
        q,
    );
    match crate::llm::client::chat_completion(
        &provider,
        &crate::llm::client::CompletionParams {
            model: &model,
            messages: &chat_messages(REFLECT_SYSTEM_PROMPT, &user),
            max_tokens: Some(700),
            temperature: Some(0.1),
            top_p: None,
            presence_penalty: None,
            frequency_penalty: None,
            tools: None,
            tool_choice: None,
        },
    )
    .await
    {
        Ok((text, _, _, _)) => {
            if let Some(v) = parse_json_object(&text) {
                let sufficient = v
                    .get("sufficient")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(true);
                let relevant = v
                    .get("relevant_indices")
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|i| i.as_u64())
                            // 证据在提示词里按 1 基编号（[1][2]…），转回 0 基下标
                            .filter_map(|i| i.checked_sub(1).map(|x| x as usize))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let refinement = v
                    .get("refinement")
                    .filter(|r| !r.is_null())
                    .and_then(|r| serde_json::from_value::<AskPlan>(r.clone()).ok());
                return ReflectResult {
                    sufficient,
                    relevant,
                    refinement,
                };
            }
            default
        }
        Err(_) => default,
    }
}

/// 合并多轮检索证据（按 类型+会话+消息ID+时间 去重），返回新增条数
pub(crate) fn merge_citations(dst: &mut Vec<Citation>, src: Vec<Citation>) -> usize {
    let mut seen: HashSet<(String, String, i64, i64)> = dst
        .iter()
        .map(|c| (c.kind.to_string(), c.username.clone(), c.local_id, c.ts))
        .collect();
    let mut added = 0;
    for c in src {
        let key = (c.kind.to_string(), c.username.clone(), c.local_id, c.ts);
        if seen.insert(key) {
            dst.push(c);
            added += 1;
        }
    }
    added
}

pub(crate) async fn generate_answer(
    q: &str,
    history: &[AskHistoryItem],
    plan: &AskPlan,
    citations: &[Citation],
    stats: &[StatsTable],
) -> (Option<String>, Option<String>) {
    let Some((provider, model)) = llm_provider() else {
        return (
            None,
            Some(
                "未配置可用的模型提供方，已展示检索结果。请在「设置 → 大模型」中配置默认模型后获得 AI 回答。"
                    .to_string(),
            ),
        );
    };
    if citations.is_empty() && stats.is_empty() {
        return (
            Some("没有检索到相关数据。可以换个说法重试；若关键词较宽泛，建议先在聊天搜索中构建消息搜索索引以提升检索质量。".to_string()),
            None,
        );
    }
    let evidence: Vec<String> = citations
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "[{}] 会话「{}」({}) {}：{}",
                i + 1,
                c.name,
                c.kind_label(),
                c.time,
                truncate(&c.snippet, 120)
            )
        })
        .collect();
    let stats_text: Vec<String> = stats
        .iter()
        .map(|t| {
            let rows: Vec<String> = t
                .rows
                .iter()
                .map(|r| r.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(" | "))
                .collect();
            format!("【{}】{}\n{}", t.title, t.summary, rows.join("\n"))
        })
        .collect();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let user = format!(
        "今天是 {}。\n{}\n\n问题：{}\n\n检索计划：{}\n\n证据：\n{}\n\n统计数据：\n{}",
        today,
        format_history(history),
        q,
        plan.rationale,
        if evidence.is_empty() {
            "（无）".to_string()
        } else {
            evidence.join("\n")
        },
        if stats_text.is_empty() {
            "（无）".to_string()
        } else {
            stats_text.join("\n")
        },
    );
    match crate::llm::client::chat_completion(
        &provider,
        &crate::llm::client::CompletionParams {
            model: &model,
            messages: &chat_messages(ANSWER_SYSTEM_PROMPT, &user),
            max_tokens: Some(1024),
            temperature: Some(0.2),
            top_p: None,
            presence_penalty: None,
            frequency_penalty: None,
            tools: None,
            tool_choice: None,
        },
    )
    .await
    {
        Ok((text, _, _, _)) => {
            if let Some(v) = parse_json_object(&text) {
                if let Some(answer) = v.get("answer").and_then(|a| a.as_str()) {
                    if !answer.trim().is_empty() {
                        return (Some(answer.trim().to_string()), None);
                    }
                }
            }
            // 完整 JSON 解析失败（截断/引号干扰）→ 直接提取 answer 字段，
            // 避免把残缺 JSON 原文当作答案展示
            if let Some(answer) = extract_answer_field(&text) {
                return (Some(answer), None);
            }
            (Some(text.trim().to_string()), None)
        }
        Err(e) => (None, Some(format!("AI 回答生成失败：{}", e))),
    }
}
