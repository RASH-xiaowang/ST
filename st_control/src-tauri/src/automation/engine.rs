// ============================================================
// 自动化管理中心 — 规则匹配 / AI 分析 / 派发引擎
// ============================================================

use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use super::db::{
    self, bump_rule_hit, insert_rule, list_rules, update_rule, AutomationRule, RuleCondition,
    WechatTask,
};

/// 正则预编译缓存：避免每条消息 × 每条规则都重新编译正则（上限 256，超限整体清空）
static REGEX_CACHE: LazyLock<Mutex<HashMap<String, regex::Regex>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 条件匹配：field + op + value（AND 组合）
pub fn condition_match(cond: &RuleCondition, msg: &Value) -> bool {
    let field_val = match cond.field.as_str() {
        "content" => msg.get("content").and_then(|v| v.as_str()).unwrap_or(""),
        "sender" => msg
            .get("sender_username")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        "session" => msg.get("username").and_then(|v| v.as_str()).unwrap_or(""),
        "media_type" => msg.get("media_type").and_then(|v| v.as_str()).unwrap_or(""),
        "is_send" => {
            let v = msg
                .get("is_send")
                .and_then(|x| x.as_bool())
                .unwrap_or(false);
            return match cond.op.as_str() {
                "equals" => v == (cond.value == "true" || cond.value == "1"),
                "not_contains" => v != (cond.value == "true" || cond.value == "1"),
                _ => v,
            };
        }
        _ => "",
    };
    match cond.op.as_str() {
        "contains" => field_val.contains(&cond.value),
        "not_contains" => !field_val.contains(&cond.value),
        "equals" => field_val == cond.value,
        "regex" => regex_match(field_val, &cond.value),
        _ => false,
    }
}

fn regex_match(text: &str, pattern: &str) -> bool {
    let mut cache = REGEX_CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(re) = cache.get(pattern) {
        return re.is_match(text);
    }
    if cache.len() >= 256 {
        cache.clear();
    }
    match regex::Regex::new(pattern) {
        Ok(re) => {
            let matched = re.is_match(text);
            cache.insert(pattern.to_string(), re);
            matched
        }
        Err(_) => false,
    }
}

/// 判断消息是否命中规则（全部条件 AND）
pub fn rule_hits(rule: &AutomationRule, msg: &Value) -> bool {
    rule.enabled
        && !rule.conditions.is_empty()
        && rule.conditions.iter().all(|c| condition_match(c, msg))
}

/// 按优先级取第一个命中规则
pub fn first_hit<'a>(rules: &'a [AutomationRule], msg: &Value) -> Option<&'a AutomationRule> {
    let mut sorted: Vec<&AutomationRule> = rules.iter().filter(|r| r.enabled).collect();
    sorted.sort_by_key(|r| r.priority);
    sorted.into_iter().find(|r| rule_hits(r, msg))
}

/// 生成 AI 分析提示词（表单字段 → 提示词，允许手动覆盖）
///
/// dispatch_mode=ai 时额外要求模型输出 candidates（推断的处理方名称），
/// 供后续 AI 决策派发阶段作为候选参考（避免决策阶段无候选瞎猜）。
pub fn build_analyze_prompt(rule: &AutomationRule, msg: &Value) -> String {
    if !rule.prompt_override.trim().is_empty() {
        return rule.prompt_override.clone();
    }
    let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let sender = msg
        .get("sender_username")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let session = msg.get("username").and_then(|v| v.as_str()).unwrap_or("");
    let fields: Vec<String> = rule
        .analyze_fields
        .iter()
        .map(|f| {
            format!(
                "{}：{}",
                f.name,
                if f.desc.is_empty() {
                    "（提取字段）"
                } else {
                    &f.desc
                }
            )
        })
        .collect();
    let ai_dispatch = rule.dispatch_mode == "ai";
    let candidates_schema = if ai_dispatch {
        ", \"candidates\":[\"候选处理方1\", \"候选处理方2\"]"
    } else {
        ""
    };
    let candidates_hint = if ai_dispatch {
        "\n另外，请判断这条消息适合交给哪个处理方处理，并在返回 JSON 中给出 candidates 数组（2-4 个候选名称，如 [\"销售顾问\", \"售后客服\"]），供后续派发决策参考。"
    } else {
        ""
    };
    format!(
        "你是消息自动化分析助手。请分析下面这条消息，提取业务字段并以 JSON 返回。\n\
         需要提取的字段：\n{}\n\
         消息内容：{}\n\
         发送人：{}\n\
         会话：{}\n\
         返回格式：{{\"task\":\"任务名或意图\", \"fields\":{{...提取的字段...}}{}}}\n\
         只返回 JSON，不要多余文字。{}",
        if fields.is_empty() {
            "- task（任务/意图）".to_string()
        } else {
            fields.join("\n")
        },
        content,
        sender,
        session,
        candidates_schema,
        candidates_hint
    )
}

/// 加载本地智能体候选（id, name），供 AI 决策派发提示词注入真实目标列表
pub fn load_agent_candidates() -> Vec<(i64, String)> {
    let Ok(conn) = rusqlite::Connection::open(super::control_db_path()) else {
        return Vec::new();
    };
    let mut stmt = match conn.prepare("SELECT id, name FROM agents ORDER BY id DESC") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)));
    match rows {
        Ok(rs) => rs.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

/// AI 决策派发提示词（决定是否派发 + 派给谁）：
/// 注入本地智能体列表（id: 名称），约束 target_id 必须是列表中的真实 id，
/// 避免模型凭空编造不存在的目标。
pub fn build_dispatch_prompt(
    _rule: &AutomationRule,
    extract: &Value,
    agents: &[(i64, String)],
) -> String {
    let candidates = extract
        .get("candidates")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|c| c.as_str())
                .map(|s| format!("- {s}"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let agent_list = if agents.is_empty() {
        "- （暂无本地智能体，可返回 agent_instance 或 should_dispatch=false）".to_string()
    } else {
        agents
            .iter()
            .map(|(id, name)| format!("- {id}: {name}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "你是任务派发决策助手。根据消息分析结果决定派发给哪个智能体/Agent。\n\
         分析结果：{}\n\
         模型推断的处理方候选：\n{}\n\
         可派发的本地智能体（格式：id: 名称）：\n{}\n\
         返回 JSON：{{\"should_dispatch\":true/false, \"target_type\":\"agent\"|\"agent_instance\", \"target_id\":\"...\"}}\n\
         - target_type=\"agent\" 时 target_id 必须取上面本地智能体列表中的 id；\n\
         - target_type=\"agent_instance\" 时 target_id 填已接入 Agent 的 id（没有合适的本地智能体时）；\n\
         - 消息不适合自动处理时 should_dispatch=false。\n\
         只返回 JSON。",
        extract,
        if candidates.is_empty() {
            "- （模型未给出，可从智能体列表中选择最合适的）".to_string()
        } else {
            candidates
        },
        agent_list,
    )
}

/// 调用大模型分析（复用 llm client）
pub async fn llm_analyze(rule: &AutomationRule, prompt: &str) -> Result<Value, String> {
    let cfg = crate::llm::config::load_config();
    let provider = cfg
        .providers
        .iter()
        .find(|p| p.id == rule.provider_id && p.enabled)
        .cloned()
        .ok_or_else(|| "规则未配置可用模型，或提供方已停用".to_string())?;
    let model = if rule.model.trim().is_empty() {
        provider.default_model.clone()
    } else {
        rule.model.clone()
    };
    if model.is_empty() {
        return Err("规则未指定模型".to_string());
    }
    let messages = vec![
        crate::llm::types::ChatMessage {
            role: "system".into(),
            content: prompt.to_string(),
            parts: None,
        },
        crate::llm::types::ChatMessage {
            role: "user".into(),
            content: "请分析。".into(),
            parts: None,
        },
    ];
    let (text, ..) = crate::llm::client::chat_completion(
        &provider,
        &crate::llm::client::CompletionParams {
            model: &model,
            messages: &messages,
            max_tokens: Some(1024),
            temperature: Some(0.1),
            top_p: None,
            presence_penalty: None,
            frequency_penalty: None,
            tools: None,
            tool_choice: None,
        },
    )
    .await
    .map_err(|e| format!("AI 分析失败: {e}"))?;
    parse_json_response(&text)
}

/// 解析模型返回文本中的 JSON（容错：去掉代码块围栏）
pub fn parse_json_response(text: &str) -> Result<Value, String> {
    let t = text.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t)
        .trim();
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    serde_json::from_str(t).map_err(|_| {
        format!(
            "模型返回无法解析为 JSON: {}",
            t.chars().take(120).collect::<String>()
        )
    })
}

/// 用规则绑定的提供方/模型做一次自定义对话（内置 Worker 执行器用：
/// system = 角色提示词 + 任务说明，user = 消息内容 + 知识库上下文）
pub async fn llm_chat(rule: &AutomationRule, system: &str, user: &str) -> Result<String, String> {
    let cfg = crate::llm::config::load_config();
    let provider = cfg
        .providers
        .iter()
        .find(|p| p.id == rule.provider_id && p.enabled)
        .cloned()
        .ok_or_else(|| "规则未配置可用模型，或提供方已停用".to_string())?;
    let model = if rule.model.trim().is_empty() {
        provider.default_model.clone()
    } else {
        rule.model.clone()
    };
    if model.is_empty() {
        return Err("规则未指定模型".to_string());
    }
    let messages = vec![
        crate::llm::types::ChatMessage {
            role: "system".into(),
            content: system.to_string(),
            parts: None,
        },
        crate::llm::types::ChatMessage {
            role: "user".into(),
            content: user.to_string(),
            parts: None,
        },
    ];
    let (text, ..) = crate::llm::client::chat_completion(
        &provider,
        &crate::llm::client::CompletionParams {
            model: &model,
            messages: &messages,
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
    .map_err(|e| format!("AI 调用失败: {e}"))?;
    Ok(text)
}

/// 同步阶段结果：已入库任务 + 需要 AI 分析的规则（若有）
pub struct ProcessOutcome {
    pub task_id: i64,
    pub rule: Option<AutomationRule>,
    pub analyze_prompt: Option<String>,
}

/// 同步阶段：去重 → 规则匹配 → 入库（AI 分析留待异步完成）
pub fn process_sync(conn: &Connection, msg: &Value) -> Result<Option<ProcessOutcome>, String> {
    // 排除自己发送的消息
    if msg
        .get("is_send")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Ok(None);
    }
    let sender = msg
        .get("sender_username")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let username = msg
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let timestamp = msg.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
    // 三字段唯一约束去重
    if db::find_task_by_key(conn, &sender, timestamp, &username)
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Ok(None);
    }

    let rules = list_rules(conn).map_err(|e| e.to_string())?;
    let hit = first_hit(&rules, msg);
    let rule = match hit {
        Some(r) => r.clone(),
        None => return Ok(None), // 未命中规则：不入库（概览消息流由 SSE 实时推送展示）
    };

    bump_rule_hit(conn, rule.id).map_err(|e| e.to_string())?;
    let needs_ai = !rule.provider_id.is_empty();
    let analyze_prompt = if needs_ai {
        Some(build_analyze_prompt(&rule, msg))
    } else {
        None
    };

    // 无需 AI：按固定目标直接入库派发
    if !needs_ai {
        let full = serde_json::to_string(msg).unwrap_or_default();
        let status = if !rule.target_id.is_empty() {
            "claimed"
        } else {
            "pending"
        };
        let id = insert_task(
            conn,
            msg,
            &TaskInsert {
                rule_id: rule.id,
                rule_name: &rule.name,
                ai_extract: "",
                full_json: &full,
                target_type: &rule.target_type,
                target_id: &rule.target_id,
                status,
                err: "",
            },
        )
        .map_err(|e| e.to_string())?;
        return Ok(Some(ProcessOutcome {
            task_id: id,
            rule: None,
            analyze_prompt: None,
        }));
    }

    // 需要 AI：先入库待处理
    let full = serde_json::to_string(msg).unwrap_or_default();
    let id = insert_task(
        conn,
        msg,
        &TaskInsert {
            rule_id: rule.id,
            rule_name: &rule.name,
            ai_extract: "",
            full_json: &full,
            target_type: &rule.target_type,
            target_id: &rule.target_id,
            status: "pending",
            err: "",
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(Some(ProcessOutcome {
        task_id: id,
        rule: Some(rule),
        analyze_prompt,
    }))
}

/// 异步 AI 阶段：分析 + （可选）决策派发，然后写回
pub async fn finish_ai(
    rule: &AutomationRule,
    analyze_prompt: &str,
) -> Result<(Value, String, String), String> {
    // 1. 提取分析
    let extract = match llm_analyze(rule, analyze_prompt).await {
        Ok(v) => v,
        Err(e) => return Err(e),
    };
    // 2. AI 决策派发（仅 dispatch_mode=ai）：注入本地智能体列表作为真实候选
    if rule.dispatch_mode == "ai" {
        let agents = load_agent_candidates();
        let dprompt = build_dispatch_prompt(rule, &extract, &agents);
        match llm_analyze(rule, &dprompt).await {
            Ok(v) => {
                let should = v
                    .get("should_dispatch")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false);
                if !should {
                    return Ok((extract, String::new(), "AI 决策：无需派发".to_string()));
                }
                let t = v
                    .get("target_type")
                    .and_then(|x| x.as_str())
                    .unwrap_or(&rule.target_type)
                    .to_string();
                let id = v
                    .get("target_id")
                    .and_then(|x| x.as_str())
                    .unwrap_or(&rule.target_id)
                    .to_string();
                Ok((extract, t, id))
            }
            Err(e) => Err(format!("AI 决策失败：{e}")),
        }
    } else {
        Ok((extract, rule.target_type.clone(), rule.target_id.clone()))
    }
}

/// 将 AI 结果写回任务（新连接，避免跨 await 持有 Connection）
pub fn apply_ai_result(
    conn: &Connection,
    task_id: i64,
    extract: &Value,
    target_type: &str,
    target_id: &str,
    err: &str,
) -> Result<(), String> {
    let ai_str = serde_json::to_string(extract).unwrap_or_default();
    let status = if err.is_empty() && !target_id.is_empty() {
        "claimed"
    } else {
        "pending"
    };
    conn.execute(
        "UPDATE task_wechat_info SET ai_extract=?1, target_type=?2, target_id=?3, status=?4, error=?5, updated_at=datetime('now','localtime') WHERE id=?6",
        rusqlite::params![ai_str, target_type, target_id, status, err, task_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 从 AI 提取结果中解析自动回复文本（reply / reply_text / replyText，含嵌套 fields）。
///
/// 供本机监控路径与 ilink 渠道路径共用：模型输出里带回复内容时，
/// 任务进入 to_reply 状态，由待回复应答器统一发送。
pub fn extract_reply(extract: &Value) -> Option<String> {
    for key in ["reply", "reply_text", "replyText"] {
        if let Some(v) = extract.get(key).and_then(|v| v.as_str()) {
            let v = v.trim();
            if !v.is_empty() && v != "null" {
                return Some(v.to_owned());
            }
        }
    }
    // 嵌套 fields
    if let Some(fields) = extract.get("fields") {
        for key in ["reply", "reply_text", "replyText"] {
            if let Some(v) = fields.get(key).and_then(|v| v.as_str()) {
                let v = v.trim();
                if !v.is_empty() && v != "null" {
                    return Some(v.to_owned());
                }
            }
        }
    }
    None
}

/// 任务插入参数（消息 + 规则/任务字段）
struct TaskInsert<'a> {
    rule_id: i64,
    rule_name: &'a str,
    ai_extract: &'a str,
    full_json: &'a str,
    target_type: &'a str,
    target_id: &'a str,
    status: &'a str,
    err: &'a str,
}

fn insert_task(conn: &Connection, msg: &Value, task: &TaskInsert) -> rusqlite::Result<i64> {
    let rule_id = task.rule_id;
    let rule_name = task.rule_name;
    let ai_extract = task.ai_extract;
    let full_json = task.full_json;
    let target_type = task.target_type;
    let target_id = task.target_id;
    let status = task.status;
    let err = task.err;
    conn.execute(
        "INSERT OR IGNORE INTO task_wechat_info
         (ack_id, channel, chat, content, decrypt_ms, is_group, is_send, local_id, media_type, msg_type, pages,
          sender, sender_username, session_type, sort_seq, time, timestamp, ts_backend, username,
          rule_id, rule_name, ai_extract, full_json, target_type, target_id, status, error)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27)",
        rusqlite::params![
            msg.get("ack_id").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("channel").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("chat").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("content").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("decrypt_ms").and_then(|v| v.as_f64()).unwrap_or(0.0),
            msg.get("is_group").and_then(|v| v.as_bool()).unwrap_or(false) as i64,
            msg.get("is_send").and_then(|v| v.as_bool()).unwrap_or(false) as i64,
            msg.get("local_id").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("media_type").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("msg_type").and_then(|v| v.as_i64()).unwrap_or(0),
            msg.get("pages").and_then(|v| v.as_i64()).unwrap_or(0),
            msg.get("sender").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("sender_username").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("session_type").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("sort_seq").map(|v| v.to_string()).unwrap_or_default(),
            msg.get("time").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0),
            msg.get("ts_backend").and_then(|v| v.as_i64()).unwrap_or(0),
            msg.get("username").and_then(|v| v.as_str()).unwrap_or(""),
            rule_id,
            rule_name,
            ai_extract,
            full_json,
            target_type,
            target_id,
            status,
            err,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 提供给测试/手动入口：按规则插入任务并返回任务（复用 process 逻辑的同步壳）
pub fn rule_crud_sync(
    conn: &Connection,
    rule: &AutomationRule,
    is_new: bool,
) -> Result<i64, String> {
    if is_new {
        insert_rule(conn, rule).map_err(|e| e.to_string())
    } else {
        update_rule(conn, rule.id, rule).map_err(|e| e.to_string())?;
        Ok(rule.id)
    }
}

pub fn delete_rule_sync(conn: &Connection, id: i64) -> Result<(), String> {
    db::delete_rule(conn, id).map_err(|e| e.to_string())
}

pub fn task_to_json(t: &WechatTask) -> Value {
    json!({
        "id": t.id,
        "ackId": t.ack_id,
        "content": t.content,
        "senderUsername": t.sender_username,
        "sessionType": t.session_type,
        "isGroup": t.is_group,
        "isSend": t.is_send,
        "mediaType": t.media_type,
        "msgType": t.msg_type,
        "timestamp": t.timestamp,
        "username": t.username,
        "ruleId": t.rule_id,
        "ruleName": t.rule_name,
        "aiExtract": serde_json::from_str::<Value>(&t.ai_extract).unwrap_or(Value::Null),
        "fullJson": serde_json::from_str::<Value>(&t.full_json).unwrap_or(Value::Null),
        "targetType": t.target_type,
        "targetId": t.target_id,
        "replyText": t.reply_text,
        "status": t.status,
        "error": t.error,
        "retryCount": t.retry_count,
        "createdAt": t.created_at,
        "updatedAt": t.updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::db::{AnalyzeField, RuleCondition};

    fn test_rule(dispatch_mode: &str) -> AutomationRule {
        AutomationRule {
            id: 1,
            name: "测试规则".into(),
            enabled: true,
            priority: 0,
            conditions: vec![RuleCondition {
                field: "content".into(),
                op: "contains".into(),
                value: "预审".into(),
            }],
            analyze_fields: vec![AnalyzeField {
                name: "购车价格".into(),
                desc: "万元".into(),
            }],
            prompt_override: String::new(),
            provider_id: "deepseek".into(),
            model: "chat".into(),
            dispatch_mode: dispatch_mode.into(),
            target_type: "agent".into(),
            target_id: String::new(),
            role_id: String::new(),
            hit_count: 0,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    /// AI 决策模式：分析提示词必须要求模型输出 candidates（决策阶段候选来源）
    #[test]
    fn analyze_prompt_ai_mode_asks_candidates() {
        let rule = test_rule("ai");
        let msg = json!({"content": "新丰田预审", "sender_username": "u", "username": "c"});
        let p = build_analyze_prompt(&rule, &msg);
        assert!(
            p.contains("candidates"),
            "AI 决策模式应要求输出 candidates: {p}"
        );
        assert!(p.contains("候选处理方"), "应说明 candidates 的语义: {p}");
        assert!(p.contains("购车价格"), "应包含表单字段: {p}");
    }

    /// 固定派发模式：不需要 candidates，避免模型输出多余内容
    #[test]
    fn analyze_prompt_fixed_mode_no_candidates() {
        let rule = test_rule("fixed");
        let msg = json!({"content": "你好", "sender_username": "u", "username": "c"});
        let p = build_analyze_prompt(&rule, &msg);
        assert!(!p.contains("candidates"), "固定派发无需 candidates: {p}");
    }

    /// 手动覆盖提示词时原样返回（不做任何增强）
    #[test]
    fn analyze_prompt_respects_override() {
        let mut rule = test_rule("ai");
        rule.prompt_override = "自定义提示词".into();
        let msg = json!({"content": "x"});
        assert_eq!(build_analyze_prompt(&rule, &msg), "自定义提示词");
    }

    /// 决策提示词注入本地智能体列表（id: 名称），并约束 target_id 取真实 id
    #[test]
    fn dispatch_prompt_injects_agent_list() {
        let rule = test_rule("ai");
        let extract = json!({"task": "预审", "candidates": ["销售顾问"]});
        let agents = vec![(1, "销售顾问".to_string()), (2, "售后客服".to_string())];
        let p = build_dispatch_prompt(&rule, &extract, &agents);
        assert!(p.contains("- 1: 销售顾问"), "应注入智能体 id 与名称: {p}");
        assert!(p.contains("- 2: 售后客服"));
        assert!(
            p.contains("target_id 必须取上面本地智能体列表中的 id"),
            "应约束 target_id 必须是真实 id: {p}"
        );
        assert!(p.contains("- 销售顾问"), "应保留模型推断的候选名称");
    }

    /// 无本地智能体时给出降级说明（允许 agent_instance / 不派发）
    #[test]
    fn dispatch_prompt_empty_agents_fallback() {
        let rule = test_rule("ai");
        let extract = json!({"task": "x"});
        let p = build_dispatch_prompt(&rule, &extract, &[]);
        assert!(
            p.contains("暂无本地智能体"),
            "无智能体时应给出降级说明: {p}"
        );
    }

    /// 模型返回文本解析：容错代码块围栏
    #[test]
    fn parse_json_response_strips_fences() {
        assert_eq!(
            parse_json_response("```json\n{\"a\":1}\n```").unwrap(),
            json!({"a": 1})
        );
        assert_eq!(parse_json_response("{\"a\":1}").unwrap(), json!({"a": 1}));
        assert!(parse_json_response("not json").is_err());
    }

    /// 回复提取：顶层与嵌套 fields 均支持
    #[test]
    fn extract_reply_finds_top_and_nested() {
        assert_eq!(
            extract_reply(&json!({"reply": "你好"})),
            Some("你好".to_string())
        );
        assert_eq!(
            extract_reply(&json!({"fields": {"reply_text": "好的"}})),
            Some("好的".to_string())
        );
        assert_eq!(extract_reply(&json!({"reply": "null"})), None);
        assert_eq!(extract_reply(&json!({"task": "x"})), None);
    }
}
