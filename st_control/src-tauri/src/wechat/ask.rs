//! 微信数据 AI 问答模块（「问我的微信」）
//!
//! `ask_wechat` 命令：把自然语言问题解析成检索计划（关键词/目标会话/时间范围/数据源），
//! 在本地解密数据上检索证据，再交给 LLM 生成带引用标注的回答。
//!
//! 设计原则：
//! - 检索与回答解耦：未配置模型时仍返回检索结果，前端可展示"证据列表"。
//! - 检索只读本地解密副本，不触碰微信源库。
//! - LLM 只负责"规划"与"组织回答"，所有事实都必须来自检索到的证据。

use crate::wechat::handlers::helpers;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Instant;
use tauri::Emitter;

mod plan;
pub(crate) use plan::*;
mod search;
pub(crate) use search::*;
mod llm;
pub(crate) use llm::*;

/// 检索计划（LLM 生成或启发式推导）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskPlan {
    /// 检索关键词
    #[serde(default)]
    pub keywords: Vec<String>,
    /// 目标会话/联系人 username（可为空）
    #[serde(default)]
    pub target: Option<String>,
    /// 时间范围起点（YYYY-MM-DD）
    #[serde(default)]
    pub time_from: Option<String>,
    /// 时间范围终点（YYYY-MM-DD）
    #[serde(default)]
    pub time_to: Option<String>,
    /// 数据源：messages / transfers / redpackets / contacts / moments / favorites
    #[serde(default)]
    pub data_sources: Vec<String>,
    /// 统计/聚合子任务（可选；例如「一共多少条」「哪个群最活跃」）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregation: Option<AggregationSpec>,
    /// 最多返回的引用条数
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// 检索依据说明
    #[serde(default)]
    pub rationale: String,
}

fn default_limit() -> usize {
    24
}

impl Default for AskPlan {
    fn default() -> Self {
        Self {
            keywords: Vec::new(),
            target: None,
            time_from: None,
            time_to: None,
            data_sources: vec!["messages".to_string()],
            aggregation: None,
            limit: default_limit(),
            rationale: String::new(),
        }
    }
}

/// 统计/聚合子任务（由 LLM 规划生成，也可启发式推导）
///
/// kind 取值：
/// - `count_messages`：按会话/时间/关键词统计文本消息条数（无 target 时给出 Top 会话分布）
/// - `top_sessions`：最活跃的会话排行（支持 group_only 只看群聊）
/// - `message_trend`：按月份的消息量趋势
/// - `count_transfers`：转账笔数（按会话/时间过滤）
/// - `count_redpackets`：红包个数（按会话/时间过滤）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationSpec {
    pub kind: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub time_from: Option<String>,
    #[serde(default)]
    pub time_to: Option<String>,
    #[serde(default)]
    pub keyword: Option<String>,
    #[serde(default)]
    pub group_only: bool,
    #[serde(default = "default_agg_limit")]
    pub limit: usize,
}

fn default_agg_limit() -> usize {
    10
}

/// 一条统计结果表（前端渲染为小表格，LLM 作为证据参与回答）
#[derive(Debug, Clone, Serialize)]
pub struct StatsTable {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub summary: String,
}

/// 对话历史（供追问消解：如「那去年呢？」）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskHistoryItem {
    pub question: String,
    #[serde(default)]
    pub answer: String,
}

/// 一条检索证据（前端引用卡片）
#[derive(Debug, Clone)]
pub(crate) struct Citation {
    kind: &'static str,
    username: String,
    name: String,
    local_id: i64,
    ts: i64,
    time: String,
    snippet: String,
}

impl Citation {
    fn kind_label(&self) -> &'static str {
        match self.kind {
            "message" => "消息",
            "transfer" => "转账",
            "redpacket" => "红包",
            "contact" => "联系人",
            "moment" => "朋友圈",
            "favorite" => "收藏",
            _ => "记录",
        }
    }
}

fn citation_to_json(c: &Citation) -> serde_json::Value {
    serde_json::json!({
        "kind": c.kind,
        "kind_label": c.kind_label(),
        "username": c.username,
        "name": c.name,
        "local_id": c.local_id,
        "ts": c.ts,
        "time": c.time,
        "snippet": c.snippet,
    })
}

// ============ IPC 命令 ============

/// 问答进度事件（前端实时展示检索/统计/生成过程）
const PROGRESS_EVENT: &str = "ask-wechat-progress";

fn emit_progress(app: &tauri::AppHandle, phase: &str, message: &str) {
    let _ = app.emit(
        PROGRESS_EVENT,
        serde_json::json!({ "phase": phase, "message": message }),
    );
}

/// 微信数据 AI 问答（Agent 化）：
/// 问题 + 对话历史 → 检索计划 → 本地检索/统计 → LLM 自评证据 → 不足则补检（最多 3 轮）→ 生成带引用回答
#[tauri::command]
pub async fn ask_wechat(
    app: tauri::AppHandle,
    question: String,
    limit: Option<usize>,
    history: Option<Vec<AskHistoryItem>>,
) -> Result<serde_json::Value, String> {
    let q = question.trim().to_string();
    if q.is_empty() {
        return Err("问题不能为空".to_string());
    }
    let t0 = Instant::now();
    // 取最近 6 轮对话作为追问上下文
    let history: Vec<AskHistoryItem> = history
        .unwrap_or_default()
        .into_iter()
        .filter(|h| !h.question.trim().is_empty())
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    // 1) 解析检索计划（LLM 优先，失败回退启发式）
    emit_progress(&app, "plan", "正在分析问题、生成检索计划…");
    let plan = resolve_plan(&q, &history).await;
    let cfg =
        crate::wechat::config::WeChatConfig::load().map_err(|e| format!("读取配置失败: {}", e))?;
    let limit = limit.unwrap_or(24).clamp(1, 60);
    emit_progress(
        &app,
        "plan",
        &format!("检索计划已确定：{}", truncate(&plan.rationale, 120)),
    );

    // 2) Agent 循环：检索/统计 → LLM 自评 → 必要时补检
    let mut plan_exec = plan.clone();
    let mut merged: Vec<Citation> = Vec::new();
    let mut stats: Vec<StatsTable> = Vec::new();
    let mut relevant: HashSet<usize> = HashSet::new();
    let mut steps: Vec<String> = Vec::new();
    let mut rounds = 0usize;
    let max_rounds = 3usize;
    let mut last_reflect_len: usize;

    loop {
        rounds += 1;
        // 2a) 本地检索（CPU/IO 密集，放阻塞线程池）
        let plan_round = plan_exec.clone();
        let q_round = q.clone();
        let cfg_round = cfg.clone();
        emit_progress(
            &app,
            "search",
            &format!("第 {} 轮：正在检索本地聊天数据…", rounds),
        );
        let cits = helpers::run_blocking(move || {
            Ok::<_, String>(execute_plan(&q_round, &plan_round, &cfg_round, limit))
        })
        .await?;
        let new_count = merge_citations(&mut merged, cits);
        emit_progress(
            &app,
            "search",
            &format!(
                "第 {} 轮：检索到 {} 条新证据（累计 {} 条）",
                rounds,
                new_count,
                merged.len()
            ),
        );

        // 2b) 统计/聚合（如有）
        if let Some(agg) = &plan_exec.aggregation {
            let agg = agg.clone();
            let decrypted = cfg.decrypted_dir.clone();
            emit_progress(
                &app,
                "stats",
                &format!("第 {} 轮：正在计算统计数据…", rounds),
            );
            let r = helpers::run_blocking(move || execute_aggregation(&decrypted, &agg)).await;
            match r {
                Ok(table) => {
                    if !stats.iter().any(|s| s.title == table.title) {
                        stats.push(table.clone());
                    }
                    steps.push(format!("第 {} 轮：统计完成 —— {}", rounds, table.summary));
                    emit_progress(
                        &app,
                        "stats",
                        &format!("第 {} 轮：{}", rounds, table.summary),
                    );
                }
                Err(e) => {
                    steps.push(format!("第 {} 轮：统计不可用（{}）", rounds, e));
                    emit_progress(
                        &app,
                        "stats",
                        &format!("第 {} 轮：统计不可用（{}）", rounds, e),
                    );
                }
            }
        }
        steps.push(format!(
            "第 {} 轮：新增 {} 条证据（共 {} 条）。计划：{}",
            rounds,
            new_count,
            merged.len(),
            plan_exec.rationale
        ));

        if rounds >= max_rounds {
            last_reflect_len = merged.len();
            break;
        }

        // 2c) LLM 自评：证据是否足够；不足则按 refinement 补检
        emit_progress(
            &app,
            "reflect",
            &format!("第 {} 轮：正在自评证据质量…", rounds),
        );
        let refl = reflect_evidence(&q, &history, &plan_exec, &merged, &stats).await;
        last_reflect_len = merged.len();
        if !refl.relevant.is_empty() {
            relevant = refl.relevant.into_iter().collect();
        }
        if refl.sufficient {
            emit_progress(&app, "reflect", &format!("第 {} 轮：证据已足够", rounds));
            break;
        }
        match refl.refinement {
            Some(rp) => {
                plan_exec = rp;
                steps.push(format!(
                    "第 {} 轮证据不足，自动补检：{}",
                    rounds, plan_exec.rationale
                ));
                emit_progress(
                    &app,
                    "reflect",
                    &format!(
                        "第 {} 轮证据不足，自动补检：{}",
                        rounds,
                        truncate(&plan_exec.rationale, 100)
                    ),
                );
            }
            None => break,
        }
    }

    // 3) 按自评的相关序号过滤噪音引用（保留最后一次自评后新增的证据；
    //    检索结果按时间倒序，头 3 条最新证据始终保留，防止自评误删最新关键证据）
    let mut final_citations: Vec<Citation> = if relevant.is_empty() {
        merged
    } else {
        merged
            .into_iter()
            .enumerate()
            .filter(|(i, _)| relevant.contains(i) || *i < 3 || *i >= last_reflect_len)
            .map(|(_, c)| c)
            .collect()
    };
    final_citations.truncate(limit);

    // 4) LLM 生成回答（不阻塞检索线程）
    emit_progress(&app, "answer", "正在根据证据生成回答…");
    let (answer, error) = generate_answer(&q, &history, &plan_exec, &final_citations, &stats).await;

    let citations_json: Vec<serde_json::Value> =
        final_citations.iter().map(citation_to_json).collect();
    Ok(serde_json::json!({
        "question": q,
        "answer": answer,
        "error": error,
        "citations": citations_json,
        "stats": stats,
        "steps": steps,
        "rounds": rounds,
        "plan": serde_json::to_value(&plan_exec).unwrap_or_default(),
        "llm_used": answer.is_some(),
        "elapsed_ms": t0.elapsed().as_millis() as i64,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 启发式规划：关键词/时间范围/数据源应被正确识别
    #[test]
    fn heuristic_plan_smoke() {
        let p = heuristic_plan("我和张三上周聊了什么项目？");
        assert!(!p.keywords.is_empty(), "应提取出关键词");
        assert!(p.time_from.is_some(), "「上周」应解析出时间范围");
        assert!(p.time_to.is_some());
        assert!(p.data_sources.contains(&"messages".to_string()));

        let p2 = heuristic_plan("我上个月的转账记录");
        assert!(p2.data_sources.contains(&"transfers".to_string()));
        assert!(p2.time_from.is_some());
        assert!(p2.time_to.is_some());

        let p3 = heuristic_plan("最近7天聊了什么");
        assert!(p3.time_from.is_some());
        assert!(p3.time_to.is_some());
    }

    #[test]
    fn record_type_stopwords_are_stripped() {
        assert_eq!(non_type_keyword("红包"), "");
        assert_eq!(non_type_keyword("转账记录"), "");
        assert_eq!(non_type_keyword("TRANSFER"), "");
        assert_eq!(non_type_keyword("张三"), "张三");
        assert_eq!(non_type_keyword(""), "");
    }

    #[test]
    fn record_keyword_matching() {
        let matched = vec!["wxid_abc".to_string()];
        assert!(record_matches("张三", &matched, &["wxid_abc"]));
        assert!(record_matches("张三", &[], &["wxid_张三"]));
        assert!(!record_matches("张三", &[], &["wxid_other"]));
        // 空关键词恒匹配（不限制）
        assert!(record_matches("", &[], &["anything"]));
    }

    #[test]
    fn keywords_are_slimmed_not_verbatim_phrases() {
        let kws = extract_keywords("我最近在那些群里面聊过天");
        assert!(kws.iter().any(|k| k == "群"), "应提取出「群」: {kws:?}");
        assert!(
            kws.iter().all(|k| k != "群里面聊过天"),
            "不应把整句口语当关键词: {kws:?}"
        );
        // 中文单字关键词应被保留
        assert!(is_cjk("群"));
        assert!(!is_cjk("a"));
    }

    #[test]
    fn group_activity_question_detection() {
        assert!(is_group_activity_question("我最近在哪些群里聊过天"));
        assert!(is_group_activity_question("最近在哪些群聊过天"));
        assert!(is_group_activity_question("我在群里面聊过什么"));
        assert!(!is_group_activity_question("我和张三聊了什么项目"));
    }

    /// 启发式应能识别统计类问题并生成聚合子任务
    #[test]
    fn heuristic_aggregation_detection() {
        let a = heuristic_plan("我上个月和谁聊得最多？");
        assert_eq!(
            a.aggregation.as_ref().map(|x| x.kind.as_str()),
            Some("top_sessions")
        );

        let b = heuristic_plan("我和张三上周聊了多少条消息？");
        let agg = b.aggregation.expect("应识别统计任务");
        assert_eq!(agg.kind, "count_messages");
        assert_eq!(
            agg.keyword.as_deref(),
            Some("张三"),
            "统计关键词应为会话对象名"
        );

        let c = heuristic_plan("我去年转了几笔账？");
        assert_eq!(
            c.aggregation.as_ref().map(|x| x.kind.as_str()),
            Some("count_transfers")
        );

        let d = heuristic_plan("哪些群最近最活跃？");
        let agg = d.aggregation.expect("应识别群活跃统计");
        assert_eq!(agg.kind, "top_sessions");
        assert!(agg.group_only);
    }

    /// 「聊的最多」（的/得混用）+ 裸「最近」：应识别为近 30 天会话排行，且不留关键词
    #[test]
    fn heuristic_rank_variants_and_bare_recent() {
        let e = heuristic_plan("我最近和谁聊的最多？");
        let agg = e.aggregation.expect("「聊的最多」应识别为会话排行");
        assert_eq!(agg.kind, "top_sessions");
        assert!(
            agg.time_from.is_some() && agg.time_to.is_some(),
            "裸「最近」应默认近 30 天范围: {:?} {:?}",
            agg.time_from,
            agg.time_to
        );
        assert!(
            e.keywords.is_empty(),
            "排行类问题不应保留关键词（谁聊/最 等碎片会污染答案）: {:?}",
            e.keywords
        );

        let f = heuristic_plan("我和谁联系最多");
        assert_eq!(
            f.aggregation.as_ref().map(|x| x.kind.as_str()),
            Some("top_sessions")
        );
    }

    /// 多轮检索合并应去重
    #[test]
    fn merge_citations_dedup() {
        let mk = |i: i64| Citation {
            kind: "message",
            username: "u".to_string(),
            name: "n".to_string(),
            local_id: i,
            ts: i,
            time: String::new(),
            snippet: String::new(),
        };
        let mut dst = vec![mk(1)];
        let added = merge_citations(&mut dst, vec![mk(1), mk(2)]);
        assert_eq!(added, 1);
        assert_eq!(dst.len(), 2);
    }

    /// LLM 输出被截断时，answer 字段兜底提取应还原正文而非 JSON 原文
    #[test]
    fn truncated_answer_json_is_recovered() {
        // 完整 JSON 截断（没有右括号）
        let text = r#"{"answer": "上月共 5309 条\n与「张三」聊得最多【1】", "citation_"#;
        let out = extract_answer_field(text).expect("应提取出 answer");
        assert!(out.contains("5309"), "应还原正文: {out}");
        assert!(!out.contains('{'), "不应含 JSON 外壳: {out}");
        assert!(out.contains('\n'), "\\n 应转义为换行");
        // 无 answer 字段时返回 None
        assert!(extract_answer_field("随便一段话").is_none());
    }

    /// 真实数据检索冒烟：解密库存在时应能取回消息引用
    #[test]
    #[cfg(target_os = "windows")]
    fn execute_plan_smoke_real_data() {
        let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        if !cfg
            .decrypted_dir
            .join("session")
            .join("session.db")
            .exists()
        {
            eprintln!("解密库不存在，跳过");
            return;
        }
        let plan = AskPlan {
            keywords: vec!["项目".to_string()],
            target: None,
            time_from: None,
            time_to: None,
            data_sources: vec!["messages".to_string()],
            aggregation: None,
            limit: 10,
            rationale: String::new(),
        };
        let cits = execute_plan("项目", &plan, &cfg, 10);
        eprintln!("检索到 {} 条消息引用", cits.len());
        for c in cits.iter().take(3) {
            eprintln!(
                "- [{}] {} {} {}",
                c.kind,
                c.name,
                c.time,
                c.snippet.chars().take(40).collect::<String>()
            );
        }
        // 真实数据中「项目」一词常见，但为避免数据差异导致测试不稳定，
        // 只在取回结果时校验结构。
        if let Some(first) = cits.first() {
            assert_eq!(first.kind, "message");
            assert!(first.local_id > 0);
            assert!(!first.snippet.is_empty());
        }
    }
}
