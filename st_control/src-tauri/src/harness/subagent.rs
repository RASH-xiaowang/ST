// ============================================================
// Harness — 子代理（DSH subagent 迁移）
//
// task 工具：把子任务委派给全新上下文的子代理。
// 子代理 = 独立消息序列（系统提示词 + 任务描述），可调用工具，
// 最多 4 轮，返回最终结论文本；子代理不继承当前会话上下文。
// subagent 工具（fork 语义）：基于会话分叉的子代理——分叉当前会话
// 为子会话（继承父上下文），在子会话中运行任务；支持后台运行
// （run_in_background）、跟进（send_message）、中断（interrupt_agent）、
// 枚举（subagent_list）与结论读取（subagent_output）。
// ============================================================

use serde_json::json;

/// 子代理单轮工具循环上限
const SUBAGENT_MAX_ROUNDS: usize = 4;

/// 运行子代理：返回 (是否成功, 结论文本)
pub async fn run_subagent(
    app: &tauri::AppHandle,
    provider: &crate::llm::types::ProviderConfig,
    model: &str,
    task: &str,
    scope: &crate::harness::preset::SessionScope,
) -> Result<String, String> {
    let system_prompt = format!(
        "{}\n\n你是主代理派出的子代理。只处理分配给你的子任务，\
         完成后直接输出结论，不要提问。",
        crate::harness::tools::assemble_system_prompt_scoped(scope)
    );
    let mut messages: Vec<serde_json::Value> = vec![
        serde_json::json!({ "role": "system", "content": system_prompt }),
        serde_json::json!({ "role": "user", "content": task }),
    ];
    // one-shot 子代理非分叉可继续子代理：移除 report（DSH 仅在
    // continuable 子代理作用域注册 report；one-shot 子代理调用会报错）
    let tools_json =
        crate::harness::tools::strip_report_tool(crate::harness::tools::tools_json_scoped(scope));
    let mut final_content = String::new();

    for _round in 1..=SUBAGENT_MAX_ROUNDS {
        let comp = crate::llm::client::chat_completion_with_tools_raw(
            provider,
            model,
            &messages,
            None,
            None,
            None,
            None,
            None,
            &tools_json,
            "auto",
        )
        .await
        .map_err(|e| format!("子代理模型调用失败: {}", e))?;
        let content_out = comp.content;
        let tool_calls = comp.tool_calls;
        match tool_calls {
            Some(calls) if !calls.is_empty() => {
                messages.push(json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": calls,
                }));
                for call in &calls {
                    let args: serde_json::Value =
                        serde_json::from_str(&call.function.arguments).unwrap_or(json!({}));
                    let timeout = scope.tool_timeout(
                        &call.function.name,
                        crate::harness::settings::current().effective_timeout_secs(),
                    );
                    // 子代理无审批上下文：需审批的危险工具直接拒绝
                    let (ok, result, _duration) = if crate::harness::tools::requires_approval_scoped(
                        &call.function.name,
                        scope,
                    ) {
                        (
                            false,
                            "子代理无权执行需审批的工具（请由主会话执行）".to_string(),
                            0u64,
                        )
                    } else {
                        run_subagent_tool(app, &call.function.name, &args, timeout).await
                    };
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call.id,
                        "content": if ok { result } else { format!("工具失败: {}", result) },
                    }));
                }
            }
            _ => {
                final_content = content_out;
                break;
            }
        }
    }
    if final_content.trim().is_empty() {
        return Err("子代理在多轮工具调用后未给出结论".to_string());
    }
    Ok(final_content)
}

async fn run_subagent_tool(
    app: &tauri::AppHandle,
    name: &str,
    args: &serde_json::Value,
    timeout_secs: u64,
) -> (bool, String, u64) {
    let out = crate::harness::tools::execute_tool_guarded(app, name, args, timeout_secs).await;
    (out.ok, out.result, out.duration_ms)
}

// ─── fork 子代理（会话分叉语义） ───

fn session_store() -> Result<std::sync::Arc<crate::harness::session::SessionStore>, String> {
    crate::harness::registry::get::<crate::harness::session::SessionStore>("harness.sessions")
        .ok_or_else(|| "Harness 运行时未初始化".to_string())
}

/// 干净分叉边界：从最新事件回退，跳过尾部未闭合的 assistant_tool_calls
/// （其后无对应 tool_result 的在途工具调用——模型回合中途 fork 时父日志
/// 含未配对调用，复制给子会话会得到「tool_calls 无 tool 结果」的非法消息
/// 序列，模型 API 返回 400）。返回最后一个完整事件的 seq。
pub(crate) fn clean_boundary(
    store: &std::sync::Arc<crate::harness::session::SessionStore>,
    parent_id: &str,
) -> Result<i64, String> {
    let events = store.events(parent_id, 0)?;
    let mut boundary = events.last().map(|(seq, _)| *seq).unwrap_or(0);
    for (seq, ev) in events.iter().rev() {
        match ev {
            crate::harness::session::HarnessEvent::AssistantToolCalls { .. } => {
                // 尾部未闭合调用：边界回退到其前
                boundary = seq - 1;
            }
            _ => break, // 遇到结果/消息等完整事件即停止回退
        }
    }
    Ok(boundary)
}

/// 分叉当前会话为子代理会话（边界 = 干净边界：排除在途未闭合工具调用），
/// 返回子会话 id
pub(crate) fn fork_child(parent_id: &str) -> Result<String, String> {
    let store = session_store()?;
    let boundary = clean_boundary(&store, parent_id)?;
    let child = store.fork(parent_id, boundary)?;
    Ok(child.id)
}

/// 校验目标会话是本会话的子代理（SessionForked 溯源 = 本会话）
pub(crate) fn check_child(parent_id: &str, child_id: &str) -> Result<(), String> {
    let store = session_store()?;
    let events = store.events(child_id, 0)?;
    let forked = events.iter().any(|(_, e)| {
        matches!(
            e,
            crate::harness::session::HarnessEvent::SessionForked { source, .. }
                if source == parent_id
        )
    });
    if forked {
        Ok(())
    } else {
        Err(format!("子代理不存在: {child_id}"))
    }
}

/// 子代理结论 = 最后一条**非空**助手消息（DSH 2026-08-10
/// subagent-empty-terminal-message-output：max-tokens 截断产生的空内容
/// usage-only 消息不计入输出，避免把真实部分答案挤掉）。
pub(crate) fn conclusion(child_id: &str) -> Result<String, String> {
    let store = session_store()?;
    let msgs = store.derive_display_messages(child_id)?;
    for m in msgs.iter().rev() {
        if let crate::harness::session::DisplayMessage::Assistant { content, .. } = m {
            if !content.trim().is_empty() {
                return Ok(content.clone());
            }
        }
    }
    Ok("（子代理尚无结论）".to_string())
}

/// 本会话的子代理会话 id 列表
pub(crate) fn list_children(parent_id: &str) -> Vec<String> {
    let Ok(store) = session_store() else {
        return Vec::new();
    };
    let Ok(sessions) = store.list() else {
        return Vec::new();
    };
    sessions
        .into_iter()
        .filter(|m| {
            // 事件日志中存在分叉溯源（fork 复制事件后追加 SessionForked，
            // 不一定是首事件——全量扫描）
            store
                .events(&m.id, 0)
                .ok()
                .map(|evs| {
                    evs.iter().any(|(_, e)| {
                        matches!(
                            e,
                            crate::harness::session::HarnessEvent::SessionForked { source, .. }
                                if source == parent_id
                        )
                    })
                })
                .unwrap_or(false)
        })
        .map(|m| m.id)
        .collect()
}

/// 查找子代理会话的直接父会话 id（fork 溯源；DSH 语义：report 仅直达
/// 直接父代理）。非子代理（无 SessionForked 溯源）返回 Err。
pub(crate) fn parent_of(child_id: &str) -> Result<String, String> {
    let store = session_store()?;
    let events = store.events(child_id, 0)?;
    // 取最近一次分叉溯源（孙代理 fork 出曾孙后，最近来源即直接父）
    let parent = events.iter().rev().find_map(|(_, e)| match e {
        crate::harness::session::HarnessEvent::SessionForked { source, .. } => Some(source.clone()),
        _ => None,
    });
    parent.ok_or_else(|| "report 仅子代理可调用：当前会话不是任何会话的子代理".to_string())
}

/// 子代理 report 工具执行：把内容作为 SubagentReported 事件写入直接父会话
/// 日志（模型可见 ⟺ 落日志；父代理下一回合可读到），返回父会话内事件 seq
/// 作为 messageId（DSH tool-subagent-report 输出同构）。
pub(crate) fn report(child_id: &str, content: &str) -> Result<i64, String> {
    let content = content.trim();
    if content.is_empty() {
        return Err("report 内容不能为空".to_string());
    }
    let parent = parent_of(child_id)?;
    let store = session_store()?;
    store.append(
        &parent,
        &crate::harness::session::HarnessEvent::SubagentReported {
            child: child_id.to_string(),
            content: content.to_string(),
        },
    )
}

/// 子代理目录节点（DSH ui-subagent SubagentCatalog 迁移：
/// 会话头树目录；ST 子代理 = 分叉会话，全部可继续（continuable））
#[derive(serde::Serialize, Clone, Debug)]
pub struct SubagentNode {
    pub id: String,
    pub title: String,
    /// 模式：continuable（分叉会话可继续聊）
    pub mode: String,
    /// 活动状态：running（有进行中回合）| inactive
    pub activity: String,
    pub has_children: bool,
    pub children: Vec<SubagentNode>,
}

/// 递归收集子代理目录树（深度上限 8，防环）
pub(crate) fn catalog(parent_id: &str, depth: usize) -> Vec<SubagentNode> {
    if depth >= 8 {
        return Vec::new();
    }
    let Ok(store) = session_store() else {
        return Vec::new();
    };
    let Ok(sessions) = store.list() else {
        return Vec::new();
    };
    let mut nodes = Vec::new();
    for m in sessions {
        // 事件日志中存在分叉溯源（fork 复制事件后追加 SessionForked，
        // 不限于首事件——全量扫描）
        let is_child = store
            .events(&m.id, 0)
            .ok()
            .map(|evs| {
                evs.iter().any(|(_, e)| {
                    matches!(
                        e,
                        crate::harness::session::HarnessEvent::SessionForked { source, .. }
                            if source == parent_id
                    )
                })
            })
            .unwrap_or(false);
        if !is_child {
            continue;
        }
        let children = catalog(&m.id, depth + 1);
        nodes.push(SubagentNode {
            id: m.id.clone(),
            title: m.title.clone(),
            mode: "continuable".to_string(),
            activity: if crate::harness::agent::is_turn_running(&m.id) {
                "running".to_string()
            } else {
                "inactive".to_string()
            },
            has_children: !children.is_empty(),
            children,
        });
    }
    nodes
}

/// 会话头子代理目录（DSH ui-subagent：树目录弹层数据）
#[tauri::command]
pub async fn harness_subagent_catalog(session_id: String) -> Result<Vec<SubagentNode>, String> {
    Ok(catalog(&session_id, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() {
        // 引导运行时注册表（与 harness::tests::init_provides_session_store 一致）。
        // 进程内仅引导一次：避免并行测试互相覆盖注册表里的 SessionStore。
        static SEEDED: std::sync::Once = std::sync::Once::new();
        SEEDED.call_once(|| {
            crate::harness::init(None, crate::db::Database::new().unwrap());
        });
    }

    #[test]
    fn parent_of_and_report_deliver_to_direct_parent() {
        seed();
        let store = session_store().unwrap();
        let parent = store.create().unwrap();
        let child_id = fork_child(&parent.id).unwrap();
        // 直接父会话解析（fork 溯源）
        assert_eq!(parent_of(&child_id).unwrap(), parent.id);
        // 根会话（无分叉溯源）调用 report 应报错（DSH：仅子代理可见/可用）
        assert!(parent_of(&parent.id).is_err());
        // report：写入父会话日志并返回父会话内 seq（模型可见 ⟺ 落日志）
        let seq = report(&child_id, "子任务完成，输出写入 out.txt").unwrap();
        assert!(seq > 0);
        let evs = store.events(&parent.id, 0).unwrap();
        assert!(evs.iter().any(|(s, e)| {
            *s == seq
                && matches!(
                    e,
                    crate::harness::session::HarnessEvent::SubagentReported { child, content }
                        if child == &child_id && content.contains("out.txt")
                )
        }));
        // 父代理模型投影能看到报告内容
        let model = store.derive_model_messages(&parent.id).unwrap();
        assert!(model.iter().any(|m| m["role"] == "user"
            && m["content"]
                .as_str()
                .map(|t| t.contains("out.txt"))
                .unwrap_or(false)));
        // 空内容报错
        assert!(report(&child_id, "   ").is_err());
    }

    #[test]
    fn conclusion_selects_last_non_empty_assistant_message() {
        // DSH 2026-08-10 subagent-empty-terminal-message-output：
        // max-tokens 截断产生的空内容 usage-only 消息不计入输出
        seed();
        let store = session_store().unwrap();
        let meta = store.create().unwrap();
        store
            .append(
                &meta.id,
                &crate::harness::session::HarnessEvent::AssistantMessage {
                    id: "a1".into(),
                    content: "真实部分答案".into(),
                    reasoning: None,
                },
            )
            .unwrap();
        store
            .append(
                &meta.id,
                &crate::harness::session::HarnessEvent::AssistantMessage {
                    id: "a2".into(),
                    content: String::new(),
                    reasoning: None,
                },
            )
            .unwrap();
        let out = conclusion(&meta.id).unwrap();
        assert_eq!(out, "真实部分答案", "应选最后非空助手消息: {out}");
        let _ = store.delete(&meta.id);
    }

    #[test]
    fn fork_child_and_check_child_work() {
        seed();
        let store = session_store().unwrap();
        let parent = store.create().unwrap();
        store
            .append(
                &parent.id,
                &crate::harness::session::HarnessEvent::UserMessage {
                    id: "u1".into(),
                    content: "父任务".into(),
                },
            )
            .unwrap();
        let child_id = fork_child(&parent.id).unwrap();
        // 子会话继承父事件 + SessionForked 溯源
        let events = store.events(&child_id, 0).unwrap();
        assert!(events.iter().any(|(_, e)| matches!(
            e,
            crate::harness::session::HarnessEvent::SessionForked { source, .. }
                if source == &parent.id
        )));
        assert!(check_child(&parent.id, &child_id).is_ok());
        // 无关会话不是子代理
        let other = store.create().unwrap();
        assert!(check_child(&parent.id, &other.id).is_err());
        let _ = store.delete(&parent.id);
        let _ = store.delete(&child_id);
        let _ = store.delete(&other.id);
    }

    #[test]
    fn conclusion_reads_last_assistant_message() {
        seed();
        let store = session_store().unwrap();
        let child = store.create().unwrap();
        store
            .append(
                &child.id,
                &crate::harness::session::HarnessEvent::UserMessage {
                    id: "u1".into(),
                    content: "任务".into(),
                },
            )
            .unwrap();
        store
            .append(
                &child.id,
                &crate::harness::session::HarnessEvent::AssistantMessage {
                    id: "a1".into(),
                    content: "结论正文".into(),
                    reasoning: None,
                },
            )
            .unwrap();
        assert_eq!(conclusion(&child.id).unwrap(), "结论正文");
        let _ = store.delete(&child.id);
    }

    #[test]
    fn list_children_scans_fork_provenance() {
        seed();
        let store = session_store().unwrap();
        let parent = store.create().unwrap();
        let child_id = fork_child(&parent.id).unwrap();
        let kids = list_children(&parent.id);
        assert!(
            kids.contains(&child_id),
            "子代理列表应含 {child_id}: {kids:?}"
        );
        let _ = store.delete(&parent.id);
        let _ = store.delete(&child_id);
    }

    #[test]
    fn clean_boundary_excludes_trailing_unclosed_tool_calls() {
        // B2/子代理：模型回合中途 fork 时父日志含在途未闭合 tool_calls，
        // 干净边界必须回退到最后一个完整事件，避免子会话复制出
        // 「tool_calls 无 tool 结果」的非法消息序列（API 400）
        seed();
        let store = session_store().unwrap();
        let parent = store.create().unwrap();
        store
            .append(
                &parent.id,
                &crate::harness::session::HarnessEvent::UserMessage {
                    id: "u1".into(),
                    content: "任务".into(),
                },
            )
            .unwrap();
        let tc_seq = store
            .append(
                &parent.id,
                &crate::harness::session::HarnessEvent::AssistantToolCalls {
                    id: "a1".into(),
                    calls: vec![crate::harness::session::ToolCallView {
                        id: "c1".into(),
                        name: "workflow_run_js".into(),
                        arguments: "{}".into(),
                    }],
                },
            )
            .unwrap();
        // 尾部是未闭合调用：边界回退到其前（user 消息 seq）
        let boundary = clean_boundary(&store, &parent.id).unwrap();
        assert_eq!(boundary, tc_seq - 1);
        // 子会话不包含未闭合调用 → 派生消息无非法 tool_calls
        let child = store.fork(&parent.id, boundary).unwrap();
        let msgs = store.derive_model_messages(&child.id).unwrap();
        assert!(
            !msgs.iter().any(|m| m.get("tool_calls").is_some()),
            "子会话不应包含未闭合 tool_calls: {msgs:?}"
        );
        // 完整尾部（tool_result 跟随）：边界=最新
        store
            .append(
                &parent.id,
                &crate::harness::session::HarnessEvent::ToolResult {
                    id: "c1".into(),
                    ok: true,
                    result: "r".into(),
                    duration_ms: 1,
                },
            )
            .unwrap();
        let boundary2 = clean_boundary(&store, &parent.id).unwrap();
        assert_eq!(boundary2, tc_seq + 1);
        let _ = store.delete(&parent.id);
        let _ = store.delete(&child.id);
    }
}
