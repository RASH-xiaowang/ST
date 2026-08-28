// ============================================================
// Harness — 上下文压缩（DSH compaction 迁移）
//
// 消息序列超出 token 预算时，把较早轮次压缩为一段摘要：
// - 预算/开关来自用户设置（context_budget_tokens，默认 24000；
//   enable_compaction，默认 true；token 估算 = 字符数 / 2 的保守值）
// - 摘要由模型一次性生成（无工具），替换被压缩的轮次；
//   摘要内容以 Compaction 会话事件落日志（模型可见 ⟺ 落日志）
// ============================================================

use serde::{Deserialize, Serialize};

/// 裁剪尾部未闭合的工具轮（DSH toolPairingBalanced 语义）：摘要请求重放
/// 被压缩消息前，去掉末尾无对应 tool 结果的 assistant tool_calls 及其后
/// 的孤儿 tool 结果，保证消息序对 OpenAI 兼容/DeepSeek 适配器合法。
fn trim_unclosed_tool_round(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut out = messages.to_vec();
    // 定位最后一条 assistant tool_calls；若其调用 id 未被其后 tool 结果
    // 全部闭合，则视为崩溃/中断残留：从投影剥离该调用及其后的孤儿结果，
    // 保证重放消息序对 OpenAI 兼容/DeepSeek 适配器合法（DSH 工具配对平衡）
    let mut last_calls: Option<usize> = None;
    for (i, m) in out.iter().enumerate() {
        let is_calls = m.get("role").and_then(|r| r.as_str()) == Some("assistant")
            && m.get("tool_calls").is_some();
        if is_calls {
            last_calls = Some(i);
        }
    }
    if let Some(k) = last_calls {
        let ids: std::collections::HashSet<String> = out[k]
            .get("tool_calls")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.get("id").and_then(|i| i.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let after = &out[k + 1..];
        let all_resolved = !ids.is_empty()
            && ids.iter().all(|id| {
                after
                    .iter()
                    .any(|m| m.get("tool_call_id").and_then(|i| i.as_str()) == Some(id.as_str()))
            });
        if !all_resolved {
            out.truncate(k);
        }
    }
    out
}

/// 压缩结果（供落日志与诊断）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CompactionSummary {
    pub removed_messages: usize,
    pub summary: String,
}

/// 溢写（spill）：压缩前把完整转录写盘，返回文件路径
fn spill_transcript(session_id: &str, transcript: &str) -> Option<String> {
    let dir = crate::common::st_data_dir().join("harness").join("spill");
    std::fs::create_dir_all(&dir).ok()?;
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("{}-{}.md", session_id, ts));
    std::fs::write(&path, transcript).ok()?;
    Some(path.display().to_string())
}

/// 某会话的溢写文件列表（spill）
pub fn list_spills(session_id: &str) -> Result<Vec<String>, String> {
    let dir = crate::common::st_data_dir().join("harness").join("spill");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).map_err(|e| format!("读取溢写目录失败: {}", e))? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with(session_id) {
            out.push(e.path().display().to_string());
        }
    }
    out.sort();
    Ok(out)
}

fn estimate_tokens(messages: &[serde_json::Value]) -> u64 {
    let chars: usize = messages
        .iter()
        .map(|m| {
            m.get("content")
                .and_then(|c| c.as_str())
                .map(|s| s.chars().count())
                .unwrap_or(0)
        })
        .sum();
    // 中文约占 1 token/字；保守按字符数/2 估算
    (chars / 2) as u64
}

/// 上下文占用投影（DSH ContextMeter 迁移）：当前会话模型上下文的 token
/// 估算（消息 / 系统提示词 / 工具 schema 三分）与预算占比，供输入区环形仪表
#[derive(serde::Serialize, Clone, Debug)]
pub struct ContextMeterView {
    pub used_tokens: u64,
    pub budget_tokens: u64,
    /// 0~1 占用率（预算 0 时为 0）
    pub percent: f64,
    pub system_tokens: u64,
    pub tools_tokens: u64,
    pub messages_tokens: u64,
}

/// 投影上下文占用（渲染与回放同源：全部从会话日志派生）
pub fn context_meter(session_id: &str) -> Result<ContextMeterView, String> {
    use crate::harness::registry;
    let store = registry::get::<crate::harness::session::SessionStore>("harness.sessions")
        .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    let settings = crate::harness::settings::current();
    // 预算优先级（DSH：上下文容量 = 模型 contextWindow）：
    // 当前会话模型元数据声明的 context_window > 用户显式设置 > 默认 24K
    let budget = resolve_context_budget(&settings);
    // 消息（模型历史投影）
    let msgs = store.derive_model_messages(session_id)?;
    let messages_tokens = estimate_tokens(&msgs);
    // 工具 schema（会话作用域）
    let scope = crate::harness::preset::scope_for_session_id(session_id);
    let tools_json = crate::harness::tools::tools_json_scoped(&scope);
    let tools_tokens = estimate_tokens(std::slice::from_ref(&tools_json));
    // 系统提示词（分区组装）
    let prompt = crate::harness::tools::assemble_system_prompt_scoped(&scope);
    let system_tokens = estimate_tokens(std::slice::from_ref(&serde_json::json!({
        "content": prompt
    })));
    let used_tokens = messages_tokens + tools_tokens + system_tokens;
    let percent = if budget > 0 {
        (used_tokens as f64 / budget as f64).min(1.0)
    } else {
        0.0
    };
    Ok(ContextMeterView {
        used_tokens,
        budget_tokens: budget,
        percent,
        system_tokens,
        tools_tokens,
        messages_tokens,
    })
}

/// IPC：上下文占用（DSH ContextMeter）
#[tauri::command]
pub async fn harness_context_meter(session_id: String) -> Result<ContextMeterView, String> {
    context_meter(&session_id)
}

/// 上下文预算解析（DSH：容量 = 模型 contextWindow）：
/// 用户显式设置（context_budget_tokens，压缩阈值语义）> 当前会话模型
/// 元数据声明的 context_window（真实容量）> deepseek 官方模型 catalog
/// 兜底 1M（DSH llm-deepseek DEFAULT_CONTEXT_WINDOW）> 24K 兜底。
fn resolve_context_budget(settings: &crate::harness::settings::HarnessSettings) -> u64 {
    if let Some(configured) = settings.context_budget_tokens {
        if configured > 0 {
            return configured;
        }
    }
    if let Some(meta_window) = current_model_context_window() {
        if meta_window > 0 {
            return meta_window;
        }
    }
    // 模型元数据缺失时的 DSH catalog 兜底：deepseek 官方模型默认 1M 窗口
    if settings.last_model.starts_with("deepseek-v4") {
        return 1_000_000;
    }
    24_000
}

/// 当前会话模型元数据声明的上下文窗口（provider.model_meta[model].context_window）
fn current_model_context_window() -> Option<u64> {
    let (provider, model) = crate::harness::agent::resolve_provider_model(None, None).ok()?;
    provider
        .model_meta
        .get(&model)
        .and_then(|m| m.context_window)
        .filter(|w| *w > 0)
}

/// 工具结果剪枝（DSH compaction-tool-result-pruner 语义）：
/// 超长 tool 消息重写为 head / 截断标记 / tail，防止巨型工具结果
/// 霸占上下文（模型无关的纯文本改写）。返回剪枝条数。
pub fn prune_tool_results(messages: &mut [serde_json::Value]) -> usize {
    const MAX_INLINE: usize = 8 * 1024;
    const KEEP_HEAD: usize = 1500;
    const KEEP_TAIL: usize = 1500;
    let mut count = 0;
    for m in messages.iter_mut() {
        if m.get("role").and_then(|r| r.as_str()) != Some("tool") {
            continue;
        }
        let Some(content) = m.get("content").and_then(|c| c.as_str()) else {
            continue;
        };
        if content.chars().count() <= MAX_INLINE {
            continue;
        }
        let head: String = content.chars().take(KEEP_HEAD).collect();
        let tail: String = content
            .chars()
            .rev()
            .take(KEEP_TAIL)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let total = content.chars().count();
        *m = serde_json::json!({
            "role": "tool",
            "tool_call_id": m.get("tool_call_id").cloned().unwrap_or(serde_json::Value::Null),
            "content": format!(
                "{head}\n…（工具结果已剪枝，原 {total} 字符；完整值见工具输出日志/溢写）…\n{tail}"
            ),
        });
        count += 1;
    }
    count
}

/// 预算内无需压缩返回 None；否则溢写完整转录（spill）并压缩较早轮次
pub async fn maybe_compact(
    session_id: &str,
    provider: &crate::llm::types::ProviderConfig,
    model: &str,
    messages: &mut Vec<serde_json::Value>,
    budget_tokens: u64,
) -> Result<Option<CompactionSummary>, String> {
    if estimate_tokens(messages) <= budget_tokens {
        return Ok(None);
    }
    // 定位系统消息（保留在头部），其余为历史轮次
    let sys_count = messages
        .iter()
        .take_while(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"))
        .count();
    let history = &messages[sys_count..];
    if history.len() < 5 {
        return Ok(None); // 少于 5 条（至少两轮半）不必压
    }
    // 保留最近 4 条（约两轮），压缩更早部分
    let keep_from = history.len() - 4;
    let to_compress: Vec<serde_json::Value> = history[..keep_from].to_vec();
    let keep: Vec<serde_json::Value> = history[keep_from..].to_vec();
    let transcript = to_compress
        .iter()
        .map(|m| {
            let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("");
            let content = m
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("（工具调用/结果）");
            format!("{}: {}", role, content)
        })
        .collect::<Vec<_>>()
        .join("\n");
    // spill：压缩前把完整转录写盘（上下文溢写，可审计可恢复）
    let _spill_path = spill_transcript(session_id, &transcript);
    // DSH 2026-07-21 compaction-summary-prefix-cache-reuse：摘要调用重放
    // 被压缩区域的真实消息（非扁平转录）+ 尾部指令，作为上一个请求的
    // 前缀扩展以复用提供方 KV 缓存；尾部 user 消息保证消息序合法。
    let mut summary_messages = trim_unclosed_tool_round(&to_compress);
    summary_messages.push(serde_json::json!({
        "role": "user",
        "content": "请把以上对话压缩为一段简洁摘要（保留关键事实/结论/用户目标，不超过 300 字）。\
                    不要提及本次压缩请求，只输出摘要文本。",
    }));
    // 摘要调用无工具
    let content = crate::llm::client::chat_completion_with_tools_raw(
        provider,
        model,
        &summary_messages,
        None,
        None,
        None,
        None,
        None,
        &serde_json::json!([]),
        "none",
    )
    .await
    .map_err(|e| format!("压缩摘要生成失败: {}", e))?
    .content;
    let summary = content.trim().to_string();
    if summary.is_empty() {
        return Err("压缩摘要为空".to_string());
    }
    let removed = to_compress.len();
    // 重建消息序列：系统 + 摘要占位 + 保留部分
    let mut rebuilt: Vec<serde_json::Value> = messages[..sys_count].to_vec();
    rebuilt.push(serde_json::json!({
        "role": "user",
        "content": format!("[较早对话摘要]\n{}", summary),
    }));
    rebuilt.extend(keep);
    *messages = rebuilt;
    Ok(Some(CompactionSummary {
        removed_messages: removed,
        summary,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn trim_unclosed_tool_round_keeps_valid_prefix() {
        // DSH 2026-07-21 compaction-summary-prefix-cache-reuse：
        // 摘要重放前剥离尾部未闭合工具轮，保证消息序合法
        let closed = vec![
            json!({"role": "assistant", "content": null, "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "a", "arguments": "{}"}}]}),
            json!({"role": "tool", "tool_call_id": "c1", "content": "ok"}),
        ];
        let trimmed = trim_unclosed_tool_round(&closed);
        assert_eq!(trimmed.len(), 2, "已闭合工具轮应保留: {trimmed:?}");
        let unclosed = vec![
            json!({"role": "assistant", "content": null, "tool_calls": [{"id": "c2", "type": "function", "function": {"name": "b", "arguments": "{}"}}]}),
        ];
        let trimmed2 = trim_unclosed_tool_round(&unclosed);
        assert!(trimmed2.is_empty(), "未闭合工具轮应剥离: {trimmed2:?}");
        let partial = vec![
            json!({"role": "user", "content": "hi"}),
            json!({"role": "assistant", "content": null, "tool_calls": [{"id": "c3", "type": "function", "function": {"name": "c", "arguments": "{}"}}]}),
            json!({"role": "tool", "tool_call_id": "c3", "content": "ok"}),
            json!({"role": "assistant", "content": null, "tool_calls": [{"id": "c4", "type": "function", "function": {"name": "d", "arguments": "{}"}}]}),
        ];
        let trimmed3 = trim_unclosed_tool_round(&partial);
        assert_eq!(
            trimmed3.len(),
            3,
            "已闭合轮保留、尾部未闭合剥离: {trimmed3:?}"
        );
    }

    #[test]
    fn estimate_and_threshold() {
        let msgs = vec![
            serde_json::json!({"role": "user", "content": "你好"}),
            serde_json::json!({"role": "assistant", "content": "你好呀"}),
        ];
        assert!(estimate_tokens(&msgs) < 100);
        assert!(estimate_tokens(&[]) == 0);
    }

    #[test]
    fn deepseek_model_window_falls_back_to_1m() {
        // 回归：上下文预算不得再硬编码 24K 兜底——deepseek 官方模型
        // （DSH llm-deepseek catalog 默认 1M 窗口）在元数据缺失时按 1M 计
        crate::harness::init(None, crate::db::Database::new().unwrap());
        let settings = crate::harness::settings::HarnessSettings {
            last_provider_id: "deepseek".into(),
            last_model: "deepseek-v4-flash".into(),
            ..Default::default()
        };
        // 解析结果不得是 24K 兜底（真实配置元数据 1M 或 deepseek 前缀兜底 1M）
        let budget = resolve_context_budget(&settings);
        assert_eq!(
            budget, 1_000_000,
            "deepseek 模型默认窗口应为 1M，实际 {budget}"
        );
        // 用户显式设置仍优先（压缩阈值语义）
        let settings2 = crate::harness::settings::HarnessSettings {
            context_budget_tokens: Some(64_000),
            last_provider_id: "deepseek".into(),
            last_model: "deepseek-v4-flash".into(),
            ..Default::default()
        };
        assert_eq!(resolve_context_budget(&settings2), 64_000);
    }

    #[test]
    fn spills_listed_per_session() {
        let sid = format!("spill-test-{}", uuid::Uuid::new_v4().simple());
        let p = spill_transcript(&sid, "转录内容").unwrap();
        assert!(p.contains(&sid));
        let list = list_spills(&sid).unwrap();
        assert!(list.iter().any(|x| x == &p));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn context_meter_projects_usage_from_log() {
        // 运行时引导（注册 SessionStore；settings 无文件时返回默认）
        crate::harness::init(None, crate::db::Database::new().unwrap());
        let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
            "harness.sessions",
        )
        .expect("运行时已引导");
        let meta = store.create().unwrap();
        use crate::harness::session::HarnessEvent;
        store
            .append(
                &meta.id,
                &HarnessEvent::UserMessage {
                    id: "u1".into(),
                    content: "请分析这段代码并给出优化建议".into(),
                },
            )
            .unwrap();
        let view = context_meter(&meta.id).unwrap();
        // 预算来自设置（测试环境可能被真实设置文件覆盖），只要求 > 0
        assert!(view.budget_tokens > 0);
        // 消息 + 工具 schema + 系统提示词合计应大于消息本身
        assert!(view.used_tokens >= view.messages_tokens);
        assert!(view.messages_tokens > 0);
        assert!(view.percent >= 0.0 && view.percent <= 1.0);
        // 无会话时优雅返回空投影（消息 0，仅系统提示词/工具 schema 计费）
        let empty = context_meter("no-such-session").unwrap();
        assert_eq!(empty.messages_tokens, 0);
        assert!(empty.percent >= 0.0 && empty.percent <= 1.0);
        let _ = store.delete(&meta.id);
    }

    #[test]
    fn prune_tool_results_rewrites_oversized_only() {
        // B5 工具结果剪枝：超长 tool 消息 → head/标记/tail；短消息不动
        let small = json!({
            "role": "tool",
            "tool_call_id": "t1",
            "content": "短结果",
        });
        let big_content = "长".repeat(9 * 1024); // > 8K 上限
        let big = json!({
            "role": "tool",
            "tool_call_id": "t2",
            "content": big_content,
        });
        let assistant = json!({ "role": "assistant", "content": "回复" });
        let mut msgs = vec![small.clone(), big.clone(), assistant.clone()];
        let n = prune_tool_results(&mut msgs);
        assert_eq!(n, 1, "仅超长 tool 消息被剪枝");
        // 短消息原样保留
        assert_eq!(msgs[0], small);
        // 超长消息被重写：head 保留 + 剪枝标记 + tail
        let pruned = msgs[1]
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap()
            .to_string();
        assert!(
            pruned.contains("工具结果已剪枝"),
            "应含剪枝标记: {pruned:?}"
        );
        assert!(pruned.starts_with("长长长"), "应保留头部");
        assert!(pruned.ends_with("长"), "应保留尾部");
        // tool_call_id 保留
        assert_eq!(msgs[1]["tool_call_id"], "t2");
        // 助手消息不动
        assert_eq!(msgs[2], assistant);
    }
}

/// 会话溢写文件列表（spill）
#[tauri::command]
pub async fn harness_list_spills(session_id: String) -> Result<Vec<String>, String> {
    list_spills(&session_id)
}
