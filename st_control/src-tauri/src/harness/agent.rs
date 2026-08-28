// ============================================================
// Harness — 代理循环（DSH core/agent-loop 迁移）
//
// 会话内工具循环：一次用户消息 = 一个日志事件序列：
//   user_message → assistant_tool_calls → tool_result*（循环 ≤6 轮）→
//   assistant_chunk / assistant_message（最终回答边界）
// 模型上下文由日志投影 + 系统提示词分区组装（tools.rs）。
// 模型可见 ⟺ 落日志：工具调用与结果全部进入会话日志。
// ============================================================

use serde_json::json;
use tauri::ipc::Channel;

use super::session::{HarnessEvent, SessionStore, ToolCallView};
use super::tools;

fn emit(ch: Option<&Channel<String>>, v: serde_json::Value) {
    if let Some(c) = ch {
        let _ = c.send(v.to_string());
    }
}

/// 解析提供方与模型（回退链：显式参数 → Harness 设置记忆 → 全局默认 →
/// 首个启用的提供方；模型缺省回退提供方默认模型）
pub(crate) fn resolve_provider_model(
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<(crate::llm::types::ProviderConfig, String), String> {
    let cfg = crate::llm::config::load_config();
    let pid = provider_id
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            let s = super::settings::current();
            if !s.last_provider_id.is_empty()
                && cfg.providers.iter().any(|p| p.id == s.last_provider_id)
            {
                Some(s.last_provider_id)
            } else {
                None
            }
        })
        .or_else(|| cfg.default_provider_id.clone())
        .or_else(|| {
            cfg.providers
                .iter()
                .find(|p| p.enabled)
                .map(|p| p.id.clone())
        })
        .ok_or_else(|| "未指定提供方，且未配置全局默认提供方".to_string())?;
    let provider = cfg
        .providers
        .iter()
        .find(|p| p.id == pid)
        .ok_or_else(|| "指定的提供方不存在".to_string())?
        .clone();
    if !provider.enabled {
        return Err("该提供方已被禁用".to_string());
    }
    let model = model
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            let s = super::settings::current();
            if !s.last_model.is_empty() && provider.models.contains(&s.last_model) {
                Some(s.last_model)
            } else {
                None
            }
        })
        .unwrap_or_else(|| provider.default_model.clone());
    if model.is_empty() {
        return Err("未指定模型，且提供方未配置默认模型".to_string());
    }
    Ok((provider, model))
}

/// 重复工具调用提醒（DSH guard repeat-tool-reminder 语义）：
/// 会话内连续调用同一工具（名称+参数相同）达阈值 [3,5,8] 时返回提醒文本，
/// 由工具循环注入模型上下文；换工具即重置计数。
fn repeat_reminder(session_id: &str, name: &str, args: &serde_json::Value) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static MAP: OnceLock<Mutex<HashMap<String, (String, usize)>>> = OnceLock::new();
    let key = format!("{name}:{args}");
    let mut map = MAP
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    let entry = map
        .entry(session_id.to_string())
        .or_insert_with(|| (String::new(), 0));
    if entry.0 == key {
        entry.1 += 1;
    } else {
        entry.0 = key;
        entry.1 = 1;
    }
    match entry.1 {
        3 | 5 | 8 => Some(format!(
            "[系统提醒] 已连续 {} 次调用工具「{name}」且参数相同。请检查是否陷入重复循环：尝试换一种方法，或向用户说明当前进展与下一步。",
            entry.1
        )),
        _ => None,
    }
}

/// 回合取消标志（interrupt_agent 用）：run_turn 工具循环每轮开始前检查
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
static TURN_CANCEL: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
/// 当前回合是否由直接人类消息发起（DSH 2026-07-19 model-facing-goal-tools：
/// 目标变更工具仅允许在含直接人类消息的 live root-agent 回合内执行；
/// 自动续跑/子代理/定时任务回合不得静默改写人类目标）。回合级临时标记，
/// 非持久状态。
static TURN_HUMAN: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();

fn turn_human_flags() -> &'static Mutex<HashMap<String, bool>> {
    TURN_HUMAN.get_or_init(|| Mutex::new(HashMap::new()))
}

fn mark_human_turn(session_id: &str, human: bool) {
    let mut m = turn_human_flags().lock().unwrap();
    if human {
        m.insert(session_id.to_string(), true);
    } else {
        m.remove(session_id);
    }
}

fn is_human_turn(session_id: &str) -> bool {
    turn_human_flags()
        .lock()
        .unwrap()
        .get(session_id)
        .copied()
        .unwrap_or(false)
}

/// 运行中回合注册表（子代理目录「正在运行」标记；harness_chat_stream
/// 入口标记、出口清除——含 goal 自动续跑整段序列）
static RUNNING_TURNS: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();

pub(crate) fn mark_turn_running(session_id: &str) {
    RUNNING_TURNS
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
        .lock()
        .unwrap()
        .insert(session_id.to_string());
}

pub(crate) fn mark_turn_idle(session_id: &str) {
    RUNNING_TURNS
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
        .lock()
        .unwrap()
        .remove(session_id);
}

/// 指定会话是否有进行中的回合（子代理目录 StateDot ongoing）
pub(crate) fn is_turn_running(session_id: &str) -> bool {
    RUNNING_TURNS
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
        .lock()
        .unwrap()
        .contains(session_id)
}

/// 请求中断指定会话的进行中回合
pub(crate) fn request_cancel(session_id: &str) {
    let mut map = TURN_CANCEL
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    map.entry(session_id.to_string())
        .or_insert_with(|| Arc::new(AtomicBool::new(false)))
        .store(true, Ordering::SeqCst);
}

pub(crate) fn is_cancelled(session_id: &str) -> bool {
    TURN_CANCEL
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .get(session_id)
        .map(|f| f.load(Ordering::SeqCst))
        .unwrap_or(false)
}

fn clear_cancel(session_id: &str) {
    // L10：移除条目而非置 false——释放 Arc 内存，且语义等价
    // （is_cancelled 对缺失条目返回 false）
    if let Some(map) = TURN_CANCEL.get() {
        map.lock().unwrap().remove(session_id);
    }
}

/// slash 命令分派（DSH interaction/commands 语义）：
/// 识别则以命令效果作为助手回复返回；未识别返回 None 走模型回合
async fn handle_slash_command(
    app: &tauri::AppHandle,
    store: &std::sync::Arc<SessionStore>,
    session_id: &str,
    provider: &crate::llm::types::ProviderConfig,
    model: &str,
    messages: &mut Vec<serde_json::Value>,
    content: &str,
) -> Option<String> {
    let rest = content.trim().strip_prefix('/')?;
    let (cmd, arg) = match rest.split_once(char::is_whitespace) {
        Some((c, a)) => (c, a.trim()),
        None => (rest, ""),
    };
    match cmd {
        "plan" => {
            // /plan off = 退出计划模式（DSH /plan 通道语义）；其余 = 进入
            if arg.eq_ignore_ascii_case("off") {
                store.append(session_id, &HarnessEvent::PlanExit).ok();
                return Some("已退出计划模式，全部工具恢复可用。".to_string());
            }
            store
                .append(
                    session_id,
                    &HarnessEvent::PlanEnter {
                        plan: arg.to_string(),
                    },
                )
                .ok();
            Some(
                "已进入计划模式：仅只读工具可用。提交方案后经 plan_exit(plan=…) 评审退出。"
                    .to_string(),
            )
        }
        "exit" => {
            store.append(session_id, &HarnessEvent::PlanExit).ok();
            Some("已退出计划模式，全部工具恢复可用。".to_string())
        }
        "goal" => {
            if arg.is_empty() {
                return Some("用法：/goal <目标文本>".to_string());
            }
            store
                .append(
                    session_id,
                    &HarnessEvent::GoalSet {
                        objective: arg.to_string(),
                    },
                )
                .ok();
            store
                .append(
                    session_id,
                    &HarnessEvent::GoalUpdate {
                        objective: arg.to_string(),
                        status: "active".to_string(),
                        blocked_reason: String::new(),
                        max_goal_rounds: None,
                    },
                )
                .ok();
            Some(format!("目标已设置（active）：{arg}"))
        }
        "feedback" => {
            if arg.is_empty() {
                return Some("用法：/feedback <反馈内容>".to_string());
            }
            match super::feedback::submit(session_id, "", arg, None) {
                Ok(()) => Some("反馈已记录，感谢！".to_string()),
                Err(e) => Some(format!("反馈记录失败：{e}")),
            }
        }
        "compact" => {
            match super::compaction::maybe_compact(session_id, provider, model, messages, 0).await {
                Ok(Some(sum)) => {
                    store
                        .append(
                            session_id,
                            &HarnessEvent::Compaction {
                                removed_messages: sum.removed_messages,
                                summary: sum.summary,
                            },
                        )
                        .ok();
                    Some(format!(
                        "已压缩上下文：移除 {} 条消息并生成摘要。",
                        sum.removed_messages
                    ))
                }
                Ok(None) => Some("当前上下文无需压缩。".to_string()),
                Err(e) => Some(format!("压缩失败：{e}")),
            }
        }
        "skill" => {
            if arg.is_empty() {
                return Some("用法：/skill <技能id>（内容注入下一回合上下文）".to_string());
            }
            match super::skill::inject_next(session_id, arg) {
                Ok(()) => Some(format!("技能「{arg}」已加载，将在下一回合生效。")),
                Err(e) => Some(e),
            }
        }
        "help" => Some(
            "可用命令：/plan [方案文本] 进入计划模式；/exit 退出计划模式；\
             /goal <目标> 设置目标；/feedback <内容> 提交反馈；/compact 立即压缩上下文；\
             /skill <技能id> 加载技能到下一回合；/help 显示本帮助"
                .to_string(),
        ),
        _ => {
            // 未识别命令：提示即可，不消耗模型回合
            let _ = app;
            None
        }
    }
}

/// 同步 spawn 助手：内部 async 块为独立类型，斩断
/// handle_subagent_tool 未来与 run_turn_internal 未来的类型递归
fn spawn_subagent_background(
    app: tauri::AppHandle,
    child_id: String,
    provider_id: String,
    model: String,
    task: String,
) {
    tauri::async_runtime::spawn(async move {
        // 子代理回合运行标记（M9：子代理目录「正在运行」状态点亮）
        mark_turn_running(&child_id);
        let _ = crate::harness::agent::run_turn_internal(
            &app,
            &child_id,
            Some(&provider_id),
            Some(&model),
            &task,
        )
        .await;
        mark_turn_idle(&child_id);
        // DSH 2026-08-11 background-job-completion-wakes：后台子代理
        // 结束后向直接父会话送达完成通知（user-role 消息），父模型
        // 下一回合可见并可经 subagent_output 读取结论。
        if let Ok(parent) = super::subagent::parent_of(&child_id) {
            if let Some(store) = super::registry::get::<SessionStore>("harness.sessions") {
                let _ = store.append(
                    &parent,
                    &HarnessEvent::UserMessage {
                        id: format!("sys-sub-{}", uuid::Uuid::new_v4().simple()),
                        content: format!(
                            "（后台子代理 {child_id} 已结束；结论可用 subagent_output {child_id} 读取，如需继续可 send_message 跟进）"
                        ),
                    },
                );
            }
        }
    });
}

/// Harness 会话对话流（工具循环）：写入用户消息 → 投影上下文 →
/// 模型调用/工具执行循环 → 最终回答落日志。
#[tauri::command]
pub async fn harness_chat_stream(
    app: tauri::AppHandle,
    session_id: String,
    provider_id: Option<String>,
    model: Option<String>,
    content: String,
    on_event: Channel<String>,
) -> Result<(), String> {
    // 运行中标记：子代理目录「正在运行」状态；无论成功/失败/停止都清除
    mark_turn_running(&session_id);
    let out = harness_chat_stream_inner(
        app,
        session_id.clone(),
        provider_id,
        model,
        content,
        on_event,
    )
    .await;
    mark_turn_idle(&session_id);
    out
}

/// 实际回合流（goal 自动续跑外层；标记注册表由 harness_chat_stream 包裹）
async fn harness_chat_stream_inner(
    app: tauri::AppHandle,
    session_id: String,
    provider_id: Option<String>,
    model: Option<String>,
    content: String,
    on_event: Channel<String>,
) -> Result<(), String> {
    // goal 自动续跑（DSH goal-round-driver）：首回合正常执行；回合结束后
    // 若目标 active 且未达 max_goal_rounds，自动发起下一回合（每轮落
    // GoalUpdate revision+1，模型可见 ⟺ 落日志），直到目标完成/阻塞/
    // 轮次用尽或用户停止。
    // 会话级互斥：整段自动续跑序列独占该会话（与定时任务/SDK 串行化，
    // 防止并发写日志造成上下文交错损坏）
    let turn_lock = acquire_turn_lock(&session_id).await;
    let _turn_guard = turn_lock.lock().await;
    let mut round_content = content;
    let mut auto_rounds = 0u64;
    loop {
        if is_cancelled(&session_id) {
            return Ok(());
        }
        // 首轮 = 用户直接消息（可改目标）；自动续跑轮 = 非人类（不可改写）
        mark_human_turn(&session_id, auto_rounds == 0);
        run_turn(
            &app,
            session_id.clone(),
            provider_id.clone(),
            model.clone(),
            round_content,
            Some(&on_event),
        )
        .await?;
        auto_rounds += 1;
        // 回合结束：判断是否继续自动续跑
        let store = super::registry::get::<SessionStore>("harness.sessions")
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
        let state = store.session_state(&session_id)?;
        let max_rounds = state.goal_max_rounds.unwrap_or(0);
        let should_continue = goal_auto_round_should_continue(
            &state.goal,
            &state.goal_status,
            state.goal_revision,
            max_rounds,
            auto_rounds,
        );
        if !should_continue {
            break;
        }
        // 记录续跑轮次（revision 递增，持久化；前端目标条同步）
        store
            .append(
                &session_id,
                &HarnessEvent::GoalUpdate {
                    objective: state.goal.clone(),
                    status: "active".to_string(),
                    blocked_reason: String::new(),
                    max_goal_rounds: state.goal_max_rounds,
                },
            )
            .ok();
        emit(
            Some(&on_event),
            json!({ "type": "goal_auto_round", "round": auto_rounds, "max": max_rounds }),
        );
        round_content = format!(
            "（自动续跑 {}/{}）继续执行当前目标。若目标已完成或无法推进，请调用 goal_update 更新状态（complete/blocked）。",
            auto_rounds + 1,
            max_rounds
        );
    }
    // 回合序列结束：清除人类标记（防止泄漏到后续非人类回合）
    mark_human_turn(&session_id, false);
    Ok(())
}

/// goal 自动续跑判断（DSH goal-round-driver）：目标 active 且未达轮次上限
/// 且本轮不是最后一轮时继续。revision 从 1 起（goal_create 的 GoalUpdate），
/// 每续跑一轮 +1；续跑轮数上限 = max_goal_rounds（0 = 不自动续跑）。
/// rounds_done 含首回合（首回合后值为 1）；`rounds_done <= max_rounds`
/// 保证「最大续跑 max_rounds 轮」语义（首回合不算续跑）。
pub(crate) fn goal_auto_round_should_continue(
    goal: &str,
    status: &str,
    revision: u64,
    max_rounds: u64,
    rounds_done: u64,
) -> bool {
    !goal.is_empty()
        && status == "active"
        && max_rounds > 0
        && revision <= max_rounds
        && rounds_done <= max_rounds
}

/// 请求中断指定会话的进行中回合（UI「停止」；工具循环每轮开始前检查，
/// 下一轮迭代即停止，已完成的工具结果保留落日志）
#[tauri::command]
pub fn harness_cancel_turn(session_id: String) -> Result<(), String> {
    request_cancel(&session_id);
    Ok(())
}

/// 人工目标操作（DSH GoalBar 语义：暂停/恢复/完成/清除/阻塞/编辑）：
/// 落 GoalUpdate 事件（模型可见 ⟺ 落日志，前端目标条同步）
#[tauri::command]
pub async fn harness_goal_action(
    session_id: String,
    action: String,
    blocked_reason: Option<String>,
    objective: Option<String>,
) -> Result<(), String> {
    let store = super::registry::get::<SessionStore>("harness.sessions")
        .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    let state = store.session_state(&session_id)?;
    if state.goal.is_empty() && action != "clear" && action != "edit" {
        return Err("本会话尚无目标".to_string());
    }
    let (status, reason, objective) = match action.as_str() {
        "pause" => ("paused".to_string(), String::new(), state.goal.clone()),
        "resume" => ("active".to_string(), String::new(), state.goal.clone()),
        "complete" => ("complete".to_string(), String::new(), state.goal.clone()),
        // 清除目标：objective 置空 + complete（投影后横幅消失）
        "clear" => ("complete".to_string(), String::new(), String::new()),
        "blocked" => (
            "blocked".to_string(),
            blocked_reason.unwrap_or_else(|| "用户标记阻塞".to_string()),
            state.goal.clone(),
        ),
        // 编辑目标文本（DSH GoalBar 内联编辑：objective 替换，状态保持）
        "edit" => {
            let new_obj = objective.unwrap_or_default().trim().to_string();
            if new_obj.is_empty() {
                return Err("目标文本不能为空".to_string());
            }
            (state.goal_status.clone(), String::new(), new_obj)
        }
        _ => return Err(format!("未知操作：{action}")),
    };
    store
        .append(
            &session_id,
            &HarnessEvent::GoalUpdate {
                objective,
                status: status.to_string(),
                blocked_reason: reason,
                max_goal_rounds: state.goal_max_rounds,
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 会话级推理等级覆盖（DSH reasoningEffort 迁移）：设置非空时克隆提供方
/// 并设置请求参数；空 = 跟随提供方部署默认
fn provider_with_effort(
    provider: &crate::llm::types::ProviderConfig,
) -> crate::llm::types::ProviderConfig {
    let mut p = provider.clone();
    if let Some(e) = super::settings::current().reasoning_effort {
        if !e.is_empty() {
            p.default_reasoning_effort = Some(e);
        }
    }
    p
}

/// 一轮对话核心（on_event 为 None 时无通道事件：schedule/workflow 等内部复用）
pub(crate) async fn run_turn(
    app: &tauri::AppHandle,
    session_id: String,
    provider_id: Option<String>,
    model: Option<String>,
    content: String,
    on_event: Option<&Channel<String>>,
) -> Result<(), String> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("消息内容不能为空".to_string());
    }
    // 回合开始：清除残留的取消标志
    clear_cancel(&session_id);
    let store = super::registry::get::<SessionStore>("harness.sessions")
        .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    let (provider, model) = resolve_provider_model(provider_id.as_deref(), model.as_deref())?;

    // 配额管控（与全局调用一致）
    let usage = crate::llm::config::current_month_usage(&provider.id);
    if let Some(limit) = provider.monthly_token_limit {
        if usage.total_tokens >= limit {
            return Err(format!("该提供方本月 token 配额已用尽（上限 {}）", limit));
        }
    }

    // 1. 用户消息落日志并推送
    let user_id = format!("u-{}", uuid::Uuid::new_v4().simple());
    let user_seq = store.append(
        &session_id,
        &HarnessEvent::UserMessage {
            id: user_id.clone(),
            content: content.clone(),
        },
    )?;
    emit(
        on_event,
        json!({ "type": "user_message", "seq": user_seq, "id": user_id, "content": content }),
    );
    // 钩子桥：turn_start
    super::hooks::fire(
        app,
        "turn_start",
        &session_id,
        json!({ "content_len": content.chars().count() }),
    );

    // 2. 上下文 = 系统提示词分区（全局 + 预设）+ 日志投影；
    //    工具目录按会话作用域（preset）过滤；轮次/超时取设置（guard 可配置）
    let settings = super::settings::current();
    let scope = super::preset::scope_for_session_id(&session_id);
    if !scope.preset_name.is_empty() {
        log::info!(
            "[harness] 会话 {} 应用预设「{}」（禁用 {} 个工具）",
            session_id,
            scope.preset_name,
            scope.disabled.len()
        );
    }
    let max_rounds = settings.effective_max_rounds();
    let timeout_secs = settings.effective_timeout_secs();
    // 上下文：日志投影消息 → （可选）工具结果剪枝 + 压缩 → 系统提示词
    let mut messages: Vec<serde_json::Value> = store.derive_model_messages(&session_id)?;
    if settings.enable_compaction {
        // 工具结果剪枝（DSH compaction-tool-result-pruner 语义）
        super::compaction::prune_tool_results(&mut messages);
        if let Some(sum) = super::compaction::maybe_compact(
            &session_id,
            &provider,
            &model,
            &mut messages,
            settings.effective_budget_tokens(),
        )
        .await?
        {
            store
                .append(
                    &session_id,
                    &HarnessEvent::Compaction {
                        removed_messages: sum.removed_messages,
                        summary: sum.summary,
                    },
                )
                .ok();
        }
    }
    let state = store.session_state(&session_id)?;
    let session_ctx = super::context::assemble(&state);
    let events = store.events(&session_id, 0)?;
    let attachments = super::attachment::attachments_from_events(&events);
    let attachment_ctx = super::attachment::context_block(&attachments);
    let mut system_prompt = tools::assemble_system_prompt_scoped(&scope);
    // 会话级 AI 角色注入（原「AI 聊天」角色功能迁移）：日志投影，
    // 用户显式选择的角色作为最高优先级提示词分区
    let (role_name, role_prompt) = super::session::SessionStore::role_from_events(&events);
    if !role_prompt.is_empty() {
        system_prompt = format!(
            "{}\n\n[AI 角色：{}]\n{}",
            system_prompt, role_name, role_prompt
        );
    }
    // 代理指令上下文（DSH agent-instructions）：回合开始重扫工作区
    // AGENTS.md / CLAUDE.md 并注入 <system-reminder> 分区；注入来源随
    // ContextInjected 事件落日志（模型可见 ⟺ 落日志，UI 上下文注入行）
    let instruction_files = super::instructions::rescan();
    let instructions_ctx = super::instructions::inject();
    if !instructions_ctx.is_empty() {
        system_prompt = format!("{}\n\n{}", system_prompt, instructions_ctx);
        if !instruction_files.is_empty() {
            store
                .append(
                    &session_id,
                    &HarnessEvent::ContextInjected {
                        files: instruction_files,
                    },
                )
                .ok();
        }
    }
    // 用户手势加载的技能（/skill <id>）：注入本回合上下文
    let (skill_ctx, skill_ids) = super::skill::drain_injections(&session_id);
    if !skill_ctx.is_empty() {
        system_prompt = format!("{}\n\n{}", system_prompt, skill_ctx);
        // 注入来源随事件落日志（模型可见 ⟺ 落日志；UI 技能注入行）
        store
            .append(
                &session_id,
                &HarnessEvent::SkillInjected { skills: skill_ids },
            )
            .ok();
    }
    if !session_ctx.is_empty() {
        system_prompt = format!("{}\n\n{}", system_prompt, session_ctx);
    }
    if !attachment_ctx.is_empty() {
        system_prompt = format!("{}\n\n[attachments]\n{}", system_prompt, attachment_ctx);
    }
    // DSH tool-subagent-report：仅子代理（有 fork 溯源）注入 report 工具与
    // 「tool:report」使用指引；非子代理从工具目录移除 report。
    let is_child = super::subagent::parent_of(&session_id).is_ok();
    if is_child {
        system_prompt = format!(
            "{}\n\n[子代理回传指引]\n用 report 工具把你的结论回传给启动你的父代理：\
             结束前调用一次、附上自包含的最终答案；中途有新发现影响父代理下一步行动时\
             也可提前回传（回传不会结束你的回合）。父代理共享你的工作区，但不会自动\
             收到你的转录、工具输出或推理过程，因此只输出「完成」对它没有可用信息。\
             只有直接父代理会收到你的报告。",
            system_prompt
        );
    }
    // DSH 2026-07-16 durable-per-step-time-context：每回合注入当前本地时间，
    // DSH 2026-07-28 web-agent-runtime-context：声明当前工作区目录，
    // 模型据此知道文件操作锚定在哪里
    let ws_dir =
        crate::harness::workspace::workspace_dir(&crate::harness::workspace::current().dir);
    system_prompt = format!("{}\n\n[工作区]\n{}", system_prompt, ws_dir.display());
    // 模型无需额外调用 get_current_time；时间敏感推理（日程/时效）据此判断。
    let now = chrono::Local::now();
    system_prompt = format!(
        "{}\n\n[当前时间]\n{}（UTC%:z）",
        system_prompt,
        now.format("%Y-%m-%d %H:%M:%S %A")
    );
    if !system_prompt.is_empty() {
        messages.insert(
            0,
            serde_json::json!({ "role": "system", "content": system_prompt }),
        );
    }
    // 图片附件直接注入模型请求（DSH attachment 图片 seam：ImageBlock 等价）。
    // 最后一条用户消息的 content 转为 [text, image_url…] 块数组（data URI base64）；
    // 无图片附件时保持纯文本不变（向后兼容）。
    let image_attachments: Vec<super::attachment::AttachmentMeta> = attachments
        .iter()
        .filter(|a| a.kind == "image" && !a.sha256.is_empty())
        .take(4)
        .cloned()
        .collect();
    if !image_attachments.is_empty() {
        if let Some(last_user) = messages
            .iter_mut()
            .rev()
            .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        {
            let text = last_user
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            let mut blocks = vec![serde_json::json!({ "type": "text", "text": text })];
            for att in &image_attachments {
                if let Ok(bytes) = std::fs::read(&att.path) {
                    let b64 =
                        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
                    let ext = std::path::Path::new(&att.name)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("png")
                        .to_lowercase();
                    let mime = match ext.as_str() {
                        "jpg" | "jpeg" => "image/jpeg",
                        "gif" => "image/gif",
                        "webp" => "image/webp",
                        "bmp" => "image/bmp",
                        _ => "image/png",
                    };
                    blocks.push(serde_json::json!({
                        "type": "image_url",
                        "image_url": { "url": format!("data:{mime};base64,{b64}") },
                    }));
                }
            }
            last_user["content"] = serde_json::json!(blocks);
        }
    }
    let tools_json = {
        let mut j = tools::tools_json_scoped(&scope);
        if !is_child {
            j = tools::strip_report_tool(j);
        }
        j
    };
    let assistant_id = format!("a-{}", uuid::Uuid::new_v4().simple());
    let mut final_content = String::new();
    let mut total_prompt = 0u64;
    let mut total_completion = 0u64;
    // 遥测累计（DSH 统计条）：LLM 墙钟 / 首 token 延迟 / 缓存命中 / 请求数 / 工具墙钟
    let mut llm_wall_ms = 0u64;
    let mut first_token_ms_sum = 0u64;
    let mut cached_tokens_sum = 0u64;
    let mut requests_count = 0u64;
    let mut tool_wall_ms = 0u64;
    // 本轮已流式下发的正文累计（收尾时前端已逐段渲染，chunk 事件不再重发全文）
    let mut streamed_turn = String::new();
    // 本轮推理全文累计（随最终 AssistantMessage 落日志，Think 行回放同源）
    let mut streamed_reasoning = String::new();
    // 本轮实际推理等级（DSH AssistantRequestConfig.reasoningEffort；循环内
    // 每次请求经 provider_with_effort 应用，记录最后一次生效值）
    let mut reasoning_effort_logged: Option<String> = None;

    // slash 命令（DSH interaction/commands 语义）：
    // /plan /exit /goal /feedback /compact /help——不消耗模型回合，
    // 效果落日志（渲染与回放同源）
    if let Some(reply) = handle_slash_command(
        app,
        &store,
        &session_id,
        &provider,
        &model,
        &mut messages,
        &content,
    )
    .await
    {
        emit(
            on_event,
            json!({ "type": "assistant_chunk", "id": assistant_id, "delta": reply.clone(), "done": true }),
        );
        store
            .append(
                &session_id,
                &HarnessEvent::AssistantChunk {
                    id: assistant_id.clone(),
                    delta: reply.clone(),
                    done: true,
                },
            )
            .ok();
        store
            .append(
                &session_id,
                &HarnessEvent::AssistantMessage {
                    id: assistant_id.clone(),
                    content: reply.clone(),
                    reasoning: None,
                },
            )
            .ok();
        // done 事件：前端据此把命令回复落消息列表（与常规回合一致）
        emit(
            on_event,
            json!({
                "type": "done",
                "content": reply,
                "seq": 0,
                "model": model,
                "prompt_tokens": 0,
                "completion_tokens": 0,
                "cost": 0.0,
            }),
        );
        super::hooks::fire(
            app,
            "turn_end",
            &session_id,
            json!({ "kind": "slash_command" }),
        );
        return Ok(());
    }

    // 3. 工具循环
    // 空响应有界重试计数（DSH 2026-07-24 empty-model-response-is-retryable：
    // 模型返回空正文/空推理/无工具调用时最多重试一次，避免空 assistant 消息与误报轮次用尽）
    let mut empty_response_retries = 0u32;
    for _round in 1..=max_rounds {
        // 取消检查（interrupt_agent 请求中断进行中回合）
        if is_cancelled(&session_id) {
            let msg = "回合已被请求中断".to_string();
            emit(on_event, json!({ "type": "error", "message": msg }));
            return Err(msg);
        }
        // 流式调用：正文增量逐段下发 assistant_chunk（done:false），
        // reasoning 增量经 chunk 事件携带（Think 推理行实时展示），累计后
        // 随最终 AssistantMessage 落日志（模型可见 ⟺ 落日志）；
        // 工具调用分片在客户端合并后返回
        let provider_effort = provider_with_effort(&provider);
        reasoning_effort_logged = provider_effort.default_reasoning_effort.clone();
        let mut streamed = String::new();
        let comp = crate::llm::client::chat_completion_with_tools_stream(
            &provider_effort,
            &model,
            &messages,
            None,
            None,
            None,
            None,
            None,
            &tools_json,
            "auto",
            |delta, reasoning_delta| {
                if let Some(r) = reasoning_delta {
                    if !r.is_empty() {
                        streamed_reasoning.push_str(r);
                        emit(
                            on_event,
                            json!({
                                "type": "assistant_chunk",
                                "id": assistant_id,
                                "delta": "",
                                "reasoning_delta": r,
                                "done": false,
                            }),
                        );
                    }
                    return;
                }
                streamed.push_str(delta);
                emit(
                    on_event,
                    json!({
                        "type": "assistant_chunk",
                        "id": assistant_id,
                        "delta": delta,
                        "done": false,
                    }),
                );
            },
        )
        .await
        .map_err(|e| {
            emit(
                on_event,
                json!({ "type": "error", "message": format!("模型调用失败: {}", e) }),
            );
            e
        })?;
        let content_out = comp.content;
        let tool_calls = comp.tool_calls;
        let prompt = comp.prompt_tokens;
        let completion = comp.completion_tokens;
        total_prompt += prompt;
        total_completion += completion;
        // 遥测累计（DSH 统计条：LLM 墙钟 / 首 token 延迟 / 缓存命中 / 请求数）
        llm_wall_ms += comp.wall_ms;
        first_token_ms_sum += comp.first_token_ms;
        cached_tokens_sum += comp.cached_tokens;
        requests_count += 1;
        // 已流式下发的正文：记录用于最终收尾（工具轮通常为空）
        streamed_turn.push_str(&streamed);

        match tool_calls {
            Some(calls) if !calls.is_empty() => {
                // 记录本轮伴随文本（工具轮通常为空；轮次用尽时作为收尾消息兜底）
                final_content = content_out;
                // 工具调用落日志（模型可见 ⟺ 落日志）
                let views: Vec<ToolCallView> = calls
                    .iter()
                    .map(|c| ToolCallView {
                        id: c.id.clone(),
                        name: c.function.name.clone(),
                        arguments: c.function.arguments.clone(),
                    })
                    .collect();
                store
                    .append(
                        &session_id,
                        &HarnessEvent::AssistantToolCalls {
                            id: assistant_id.clone(),
                            calls: views.clone(),
                        },
                    )
                    .ok();
                emit(
                    on_event,
                    json!({ "type": "assistant_tool_calls", "id": assistant_id, "calls": views }),
                );
                messages.push(json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": calls,
                }));

                // 重复调用提醒（guard）：本轮触发的提醒在工具结果后注入
                let mut reminder_msg: Option<String> = None;
                for call in &calls {
                    let args: serde_json::Value =
                        serde_json::from_str(&call.function.arguments).unwrap_or(json!({}));
                    if reminder_msg.is_none() {
                        reminder_msg = repeat_reminder(&session_id, &call.function.name, &args);
                    }
                    // 计划模式 + 沙箱只读模式守卫：仅只读工具放行。置于一切执行
                    // 之前（含会话编排工具与 exec_command 后台/升级分支），防止
                    // run_in_background / sandbox_permissions 参数绕过守卫。
                    let readonly_guard = session_plan_mode(&store, &session_id).unwrap_or(false)
                        || super::settings::current().effective_sandbox_mode() == "read-only";
                    if readonly_guard && !tools::is_readonly_tool(&call.function.name) {
                        let msg =
                            "当前处于计划模式：该工具被拦截，仅只读工具可用（用户未退出计划模式前，请勿再尝试执行类工具）"
                                .to_string();
                        store
                            .append(
                                &session_id,
                                &HarnessEvent::ToolResult {
                                    id: call.id.clone(),
                                    ok: false,
                                    result: msg.clone(),
                                    duration_ms: 0,
                                },
                            )
                            .ok();
                        emit(
                            on_event,
                            json!({
                                "type": "tool_result",
                                "id": call.id,
                                "name": call.function.name,
                                "ok": false,
                                "result": msg,
                                "duration_ms": 0,
                            }),
                        );
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": call.id,
                            "content": msg,
                        }));
                        continue;
                    }
                    // 会话编排工具（todo/plan/goal）与子代理：由运行时处理（需会话上下文）
                    let handled =
                        if matches!(call.function.name.as_str(), "subagent" | "send_message") {
                            handle_subagent_tool(
                                app,
                                &session_id,
                                Some(&provider),
                                Some(&model),
                                &call.function.name,
                                &args,
                            )
                            .await
                        } else {
                            handle_session_tool(
                                app,
                                &store,
                                &session_id,
                                Some(&provider),
                                Some(&model),
                                &call.function.name,
                                &args,
                            )
                            .await
                        };
                    if let Some((ok, result, duration_ms)) = handled {
                        // 遥测累计（DSH 统计条：工具调用墙钟）
                        tool_wall_ms += duration_ms;
                        // 溢写策略：超限工具结果落盘 + 预览替换（DSH spill；
                        // spill_read 取回结果不再次溢写，防递归）
                        let model_result = spill_result(&session_id, &call.function.name, &result);
                        let persisted = crate::llm::agent::truncate_str(&model_result, 4000);
                        store
                            .append(
                                &session_id,
                                &HarnessEvent::ToolResult {
                                    id: call.id.clone(),
                                    ok,
                                    result: persisted.clone(),
                                    duration_ms,
                                },
                            )
                            .ok();
                        emit(
                            on_event,
                            json!({
                                "type": "tool_result",
                                "id": call.id,
                                "name": call.function.name,
                                "ok": ok,
                                "result": persisted,
                                "duration_ms": duration_ms,
                            }),
                        );
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": call.id,
                            "content": model_result,
                        }));
                        continue;
                    }
                    // 决策钩子（CC/Codex 方言 PreToolUse）：deny 拦截 / ask 转审批
                    if let Some((decision, reason)) = super::hooks::fire_decision(
                        app,
                        "PreToolUse",
                        &session_id,
                        json!({ "tool": call.function.name, "arguments": args }),
                    )
                    .await
                    {
                        let block = match decision.as_str() {
                            "deny" => Some(format!("钩子拦截：{reason}")),
                            "ask" => Some(format!("钩子要求确认：{reason}")),
                            _ => None,
                        };
                        if let Some(msg) = block {
                            store
                                .append(
                                    &session_id,
                                    &HarnessEvent::ToolResult {
                                        id: call.id.clone(),
                                        ok: false,
                                        result: msg.clone(),
                                        duration_ms: 0,
                                    },
                                )
                                .ok();
                            emit(
                                on_event,
                                json!({
                                    "type": "tool_result",
                                    "id": call.id,
                                    "name": call.function.name,
                                    "ok": false,
                                    "result": msg,
                                    "duration_ms": 0,
                                }),
                            );
                            messages.push(json!({
                                "role": "tool",
                                "tool_call_id": call.id,
                                "content": msg,
                            }));
                            continue;
                        }
                    }
                    // 审批门控（interaction + preset 覆盖）：危险工具先过审批
                    let spec_approval =
                        tools::requires_approval_scoped(&call.function.name, &scope);
                    // 单工具超时：preset 覆盖优先，否则全局设置
                    let tool_timeout = scope.tool_timeout(&call.function.name, timeout_secs);
                    let (ok, result, duration_ms) = if spec_approval {
                        match super::approval::request_approval(
                            app,
                            &session_id,
                            &call.function.name,
                            &args,
                        )
                        .await
                        {
                            Ok(()) => run_tool(app, &call.function.name, &args, tool_timeout).await,
                            Err(e) => (false, e, 0u64),
                        }
                    } else {
                        run_tool(app, &call.function.name, &args, tool_timeout).await
                    };
                    // 遥测累计（DSH 统计条：工具调用墙钟）
                    tool_wall_ms += duration_ms;
                    // 溢写策略：超限工具结果落盘 + 预览替换（DSH spill；
                    // spill_read 取回结果不再次溢写，防递归）
                    let model_result = spill_result(&session_id, &call.function.name, &result);
                    let persisted = crate::llm::agent::truncate_str(&model_result, 4000);
                    store
                        .append(
                            &session_id,
                            &HarnessEvent::ToolResult {
                                id: call.id.clone(),
                                ok,
                                result: persisted.clone(),
                                duration_ms,
                            },
                        )
                        .ok();
                    emit(
                        on_event,
                        json!({
                            "type": "tool_result",
                            "id": call.id,
                            "name": call.function.name,
                            "ok": ok,
                            "result": persisted,
                            "duration_ms": duration_ms,
                        }),
                    );
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call.id,
                        "content": model_result,
                    }));
                    // 钩子桥：tool_executed
                    super::hooks::fire(
                        app,
                        "tool_executed",
                        &session_id,
                        json!({ "tool": call.function.name, "ok": ok, "duration_ms": duration_ms }),
                    );
                }
                // 重复调用提醒注入模型上下文（guard：本轮回合在工具结果之后）
                if let Some(reminder) = reminder_msg {
                    messages.push(json!({ "role": "system", "content": reminder }));
                }
            }
            _ => {
                // DSH 2026-07-24：空响应（无正文/无推理/无工具调用）视为可重试
                // EMPTY_RESPONSE——重试一次而不是当作成功/轮次用尽结束
                if final_content.trim().is_empty()
                    && streamed_reasoning.trim().is_empty()
                    && empty_response_retries < 1
                {
                    empty_response_retries += 1;
                    messages.push(serde_json::json!({
                        "role": "system",
                        "content": "（模型返回了空响应，正在重试一次…）",
                    }));
                    continue;
                }
                final_content = content_out;
                break;
            }
        }
    }

    if final_content.trim().is_empty() {
        // 轮次用尽：合成收尾消息而非裸错误（工具步骤挂到该消息上，
        // 前端 waitTurnDone 与日志投影都能看到完整回合；DSH guard 语义）
        final_content = format!(
            "已达到最大工具轮次（{}），已停止继续执行工具。上方工具步骤已落日志，其执行结果如上；如需继续，请发送新消息。",
            max_rounds
        );
    }

    // 4. 最终回答：正文已逐段流式下发（streamed_turn），chunk 收尾事件不再
    // 重发全文（避免前端 streamBuf 重复拼接）；日志落权威 chunk（回放完整）
    emit(
        on_event,
        json!({ "type": "assistant_chunk", "id": assistant_id, "delta": "", "done": true }),
    );
    store
        .append(
            &session_id,
            &HarnessEvent::AssistantChunk {
                id: assistant_id.clone(),
                delta: final_content.clone(),
                done: true,
            },
        )
        .ok();
    let done_seq = store
        .append(
            &session_id,
            &HarnessEvent::AssistantMessage {
                id: assistant_id.clone(),
                content: final_content.clone(),
                reasoning: if streamed_reasoning.trim().is_empty() {
                    None
                } else {
                    Some(streamed_reasoning)
                },
            },
        )
        .ok();

    let cost = crate::llm::client::estimate_cost(&provider, total_prompt, total_completion);
    // 钩子桥：turn_end；telemetry：本轮用量落库（含 DSH 统计条遥测）
    super::hooks::fire(
        app,
        "turn_end",
        &session_id,
        json!({ "content_len": final_content.chars().count() }),
    );
    if let Err(e) = store.record_usage(&mut crate::db::HarnessUsageRecord {
        session_id: session_id.clone(),
        provider: provider.id.clone(),
        model: model.to_string(),
        // 实际生效的推理等级（DSH AssistantRequestConfig.reasoningEffort；
        // provider_with_effort 应用链后的 default_reasoning_effort）
        reasoning_effort: reasoning_effort_logged.clone(),
        prompt_tokens: total_prompt,
        completion_tokens: total_completion,
        cost,
        llm_wall_ms,
        first_token_ms: first_token_ms_sum,
        requests: requests_count,
        cached_tokens: cached_tokens_sum,
        tool_wall_ms,
        created_at: String::new(),
    }) {
        log::warn!("[harness] 记录会话用量失败: {}", e);
    }
    emit(
        on_event,
        json!({
            "type": "done",
            "content": final_content,
            "seq": done_seq,
            "model": model,
            "prompt_tokens": total_prompt,
            "completion_tokens": total_completion,
            "cost": cost,
        }),
    );
    Ok(())
}

/// 阻塞线程池中执行工具（守卫管道 + 超时）
async fn run_tool(
    app: &tauri::AppHandle,
    name: &str,
    args: &serde_json::Value,
    timeout_secs: u64,
) -> (bool, String, u64) {
    let out = tools::execute_tool_guarded(app, name, args, timeout_secs).await;
    (out.ok, out.result, out.duration_ms)
}

/// 会话级工具（todo/plan/goal）与子代理委派：由运行时处理并落日志。
/// 返回 None 表示非会话级工具，走常规执行管道。
/// provider/model 仅 task（子代理）需要；todo/plan/goal 可传 None。
async fn handle_session_tool(
    app: &tauri::AppHandle,
    store: &std::sync::Arc<SessionStore>,
    session_id: &str,
    provider: Option<&crate::llm::types::ProviderConfig>,
    model: Option<&str>,
    name: &str,
    args: &serde_json::Value,
) -> Option<(bool, String, u64)> {
    let started = std::time::Instant::now();
    match name {
        "job_list" => {
            let jobs = super::jobs::list(session_id);
            if jobs.is_empty() {
                return Some((true, "（当前会话无后台作业）".to_string(), 0));
            }
            let text = jobs
                .iter()
                .map(|j| {
                    format!(
                        "- {} [{}] {}（创建 {}）",
                        j.id, j.status, j.name, j.created_at
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            Some((true, text, started.elapsed().as_millis() as u64))
        }
        "job_output" => {
            let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 id 参数".to_string(), 0));
            };
            if let Err(e) = super::jobs::check_owner(id, session_id) {
                return Some((false, e, 0));
            }
            match super::jobs::output(id) {
                Ok(out) => Some((true, out, started.elapsed().as_millis() as u64)),
                Err(e) => Some((false, e, 0)),
            }
        }
        "job_kill" => {
            let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 id 参数".to_string(), 0));
            };
            if let Err(e) = super::jobs::check_owner(id, session_id) {
                return Some((false, e, 0));
            }
            match super::jobs::kill(id) {
                Ok(()) => Some((
                    true,
                    format!("已请求终止作业 {id}（进程回收中）"),
                    started.elapsed().as_millis() as u64,
                )),
                Err(e) => Some((false, e, 0)),
            }
        }
        "exec_command" => {
            // 后台作业模式：run_in_background=true 时不阻塞等待，立即返回作业 id
            // DSH 2026-08-11 background-first-continuable-delegation：
            // 可继续子代理默认后台执行（立即返回子代理 id），仅当下一步
            // 依赖其结果时才显式传 run_in_background=false 前台等待
            let bg = args
                .get("run_in_background")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let Some(command) = args.get("command").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 command 参数".to_string(), 0));
            };
            let command = command.trim();
            if command.is_empty() {
                return Some((false, "command 不能为空".to_string(), 0));
            }
            if bg {
                // 审批门控（与前台路径一致：exec_command 需审批，preset 覆盖优先）。
                // 计划模式/只读沙箱守卫已由调用方（工具循环）先行拦截。
                let scope = super::preset::scope_for_session_id(session_id);
                if super::tools::requires_approval_scoped("exec_command", &scope) {
                    match super::approval::request_approval(
                        app,
                        session_id,
                        "exec_command",
                        &json!({ "command": command, "run_in_background": true }),
                    )
                    .await
                    {
                        Ok(()) => {}
                        Err(e) => return Some((false, e, 0)),
                    }
                }
                match super::jobs::start(session_id, "后台命令", command) {
                    Ok(rec) => Some((
                        true,
                        format!(
                            "已启动后台作业 {}（{}）。用 job_list 查看、job_output {} 取输出、job_kill 终止。",
                            rec.id, rec.name, rec.id
                        ),
                        started.elapsed().as_millis() as u64,
                    )),
                    Err(e) => Some((false, e, 0)),
                }
            } else {
                // 逐调用升级（DSH sandbox_permissions 语义）：
                // 请求 danger-full-access 且当前模式更低 → 审批后越界执行
                let requested = args
                    .get("sandbox_permissions")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if requested == "danger-full-access"
                    && super::settings::current().effective_sandbox_mode() != "danger-full-access"
                {
                    let justification = args
                        .get("justification")
                        .and_then(|v| v.as_str())
                        .unwrap_or("（未提供理由）");
                    let approval_args = json!({
                        "command": command,
                        "sandbox_permissions": "danger-full-access",
                        "justification": justification,
                    });
                    match super::approval::request_approval(
                        app,
                        session_id,
                        "exec_command#danger-full-access",
                        &approval_args,
                    )
                    .await
                    {
                        Ok(()) => {}
                        Err(e) => return Some((false, e, 0)),
                    }
                    let svc = super::registry::get::<super::shell::ShellService>("harness.shell");
                    let Some(svc) = svc else {
                        return Some((false, "Harness 运行时未初始化".to_string(), 0));
                    };
                    let policy = super::shell::SandboxPolicy {
                        allow_workspace_escape: true,
                    };
                    let timeout = super::settings::current().effective_timeout_secs();
                    let r = svc.run_with_policy(command, None, timeout, &policy);
                    return Some((
                        r.ok,
                        if r.timed_out {
                            format!("命令超时已终止（{} 秒）\n{}", timeout, r.output)
                        } else {
                            r.output
                        },
                        r.duration_ms,
                    ));
                }
                None // 前台常规执行（受限世界：锚定当前工作区）
            }
        }
        "spill_read" => {
            let Some(locator) = args.get("locator").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 locator 参数".to_string(), 0));
            };
            match super::spill::SpillStore::read(locator) {
                Ok(text) => Some((true, text, started.elapsed().as_millis() as u64)),
                Err(e) => Some((false, e, 0)),
            }
        }
        "session_ref" => {
            let Some(target) = args.get("session_id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 session_id 参数".to_string(), 0));
            };
            let max_chars = args
                .get("max_chars")
                .and_then(|v| v.as_u64())
                .unwrap_or(4096) as usize;
            match super::instructions::session_ref(target, max_chars) {
                Ok(text) => Some((true, text, started.elapsed().as_millis() as u64)),
                Err(e) => Some((false, e, 0)),
            }
        }
        "workspace_list" => {
            let ws = super::workspace::list();
            let text = ws
                .iter()
                .map(|w| {
                    format!(
                        "- {}（{}）目录：{} 状态：{}",
                        w.id,
                        w.title,
                        if w.dir.is_empty() { "(根)" } else { &w.dir },
                        w.status
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            Some((true, text, started.elapsed().as_millis() as u64))
        }
        "workspace_create" => {
            let Some(title) = args.get("title").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 title 参数".to_string(), 0));
            };
            match super::workspace::create(title) {
                Ok(w) => Some((
                    true,
                    format!("已创建工作区 {}（{}）", w.id, w.title),
                    started.elapsed().as_millis() as u64,
                )),
                Err(e) => Some((false, e, 0)),
            }
        }
        "workspace_switch" => {
            let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 id 参数".to_string(), 0));
            };
            let exists = super::workspace::list().iter().any(|w| w.id == id);
            if !exists {
                return Some((false, format!("工作区不存在: {id}"), 0));
            }
            match super::settings::save_harness_settings(super::settings::HarnessSettings {
                workspace_id: id.to_string(),
                ..super::settings::current()
            })
            .await
            {
                Ok(_) => Some((
                    true,
                    format!("已切换到工作区 {id}"),
                    started.elapsed().as_millis() as u64,
                )),
                Err(e) => Some((false, e, 0)),
            }
        }
        "terminal_list" => {
            let list = super::terminal::list_harness_terminals()
                .await
                .unwrap_or_default();
            if list.is_empty() {
                return Some((true, "（无终端会话）".to_string(), 0));
            }
            let text = list
                .iter()
                .map(|t| format!("- {}（{}）cwd: {}", t.id, t.name, t.cwd))
                .collect::<Vec<_>>()
                .join("\n");
            Some((true, text, started.elapsed().as_millis() as u64))
        }
        "terminal_open" => {
            let name = args.get("name").and_then(|v| v.as_str());
            match super::terminal::create_harness_terminal(name.map(|s| s.to_string())).await {
                Ok(t) => Some((
                    true,
                    format!("已创建终端 {}（cwd: {}）", t.id, t.cwd),
                    started.elapsed().as_millis() as u64,
                )),
                Err(e) => Some((false, e, 0)),
            }
        }
        "terminal_send" => {
            let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 id 参数".to_string(), 0));
            };
            let Some(input) = args.get("input").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 input 参数".to_string(), 0));
            };
            if super::pty::is_running(id) {
                match super::pty::send(id, input) {
                    Ok(out) => {
                        super::terminal::push_log(id, input.trim().to_string(), out.clone());
                        Some((true, out, started.elapsed().as_millis() as u64))
                    }
                    Err(e) => Some((false, e, 0)),
                }
            } else {
                match super::terminal::send_regular(id, input) {
                    Ok(out) => Some((true, out, started.elapsed().as_millis() as u64)),
                    Err(e) => Some((false, e, 0)),
                }
            }
        }
        "terminal_read" => {
            let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 id 参数".to_string(), 0));
            };
            let logs = super::terminal::logs(id);
            if logs.is_empty() {
                return Some((true, "（终端暂无日志）".to_string(), 0));
            }
            let text = logs
                .iter()
                .rev()
                .take(20)
                .rev()
                .map(|l| format!("$ {}\n{}", l.input, l.output))
                .collect::<Vec<_>>()
                .join("\n");
            Some((true, text, started.elapsed().as_millis() as u64))
        }
        "terminal_signal" => {
            let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 id 参数".to_string(), 0));
            };
            let signal = args
                .get("signal")
                .and_then(|v| v.as_str())
                .unwrap_or("SIGINT");
            if signal != "SIGINT" {
                return Some((
                    false,
                    format!("Windows 终端仅支持 SIGINT（Ctrl+C），收到 {signal}"),
                    0,
                ));
            }
            if !super::pty::is_running(id) {
                return Some((
                    false,
                    "该终端未启动 PTY（信号仅对 PTY 有效）".to_string(),
                    0,
                ));
            }
            // \x03 = Ctrl+C：经输入管道投递到前台进程
            match super::pty::send_raw(id, "\x03") {
                Ok(()) => Some((true, "已发送 SIGINT（Ctrl+C）".to_string(), 0)),
                Err(e) => Some((false, e, 0)),
            }
        }
        "terminal_close" => {
            let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 id 参数".to_string(), 0));
            };
            super::pty::stop(id);
            match super::terminal::delete_harness_terminal(id.to_string()).await {
                Ok(()) => Some((true, format!("终端 {id} 已关闭"), 0)),
                Err(e) => Some((false, e, 0)),
            }
        }
        "schedule_list" => {
            let list = super::schedule::list_for_session(session_id);
            if list.is_empty() {
                return Some((true, "（本会话无定时任务）".to_string(), 0));
            }
            let text = list
                .iter()
                .map(|s| {
                    format!(
                        "- {}（{}）{} 每 {} 分钟{}，下次 {}",
                        s.id,
                        s.name,
                        if s.enabled { "启用" } else { "停用" },
                        s.interval_minutes,
                        if s.one_shot { "（一次性）" } else { "" },
                        s.next_run_at
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            Some((true, text, started.elapsed().as_millis() as u64))
        }
        "schedule_create" => {
            let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let every = args.get("every_minutes").and_then(|v| v.as_u64());
            let after = args.get("after_seconds").and_then(|v| v.as_u64());
            match super::schedule::create_for_session(session_id, name, prompt, every, after) {
                Ok(s) => Some((
                    true,
                    format!(
                        "已创建定时任务 {}（{}）{}",
                        s.id,
                        s.name,
                        if s.one_shot {
                            format!(
                                "{} 秒后一次性执行",
                                s.next_run_at - super::schedule::now_unix()
                            )
                        } else {
                            format!("每 {} 分钟", s.interval_minutes)
                        }
                    ),
                    started.elapsed().as_millis() as u64,
                )),
                Err(e) => Some((false, e, 0)),
            }
        }
        "schedule_delete" => {
            let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 id 参数".to_string(), 0));
            };
            match super::schedule::delete_for_session(id, session_id) {
                Ok(()) => Some((true, format!("已删除定时任务 {id}"), 0)),
                Err(e) => Some((false, e, 0)),
            }
        }
        "workflow_list" => match super::workflow::list() {
            Ok(list) if list.is_empty() => Some((true, "（暂无工作流）".to_string(), 0)),
            Ok(list) => {
                let text = list
                    .iter()
                    .map(|w| format!("- {}（{}）：{}", w.id, w.name, w.description))
                    .collect::<Vec<_>>()
                    .join("\n");
                Some((true, text, started.elapsed().as_millis() as u64))
            }
            Err(e) => Some((false, e, 0)),
        },
        "workflow_run" => {
            let Some(id) = args.get("workflow_id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 workflow_id 参数".to_string(), 0));
            };
            match super::workflow::run_workflow(app, id, session_id).await {
                Ok(r) => {
                    let text = r
                        .stages
                        .iter()
                        .map(|s| {
                            format!(
                                "[{}] {}：{}",
                                if s.ok { "完成" } else { "失败" },
                                s.name,
                                s.output
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    Some((true, text, started.elapsed().as_millis() as u64))
                }
                Err(e) => Some((false, e, 0)),
            }
        }
        "ralph" => {
            let Some(objective) = args.get("objective").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 objective 参数".to_string(), 0));
            };
            let (Some(prov), Some(model)) = (provider, model) else {
                return Some((
                    false,
                    "ralph 需要模型上下文（当前回合无提供方）".to_string(),
                    0,
                ));
            };
            let max_rounds = args.get("max_rounds").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
            match super::workflow::run_ralph(app, session_id, prov, model, objective, max_rounds)
                .await
            {
                Ok(text) => Some((true, text, started.elapsed().as_millis() as u64)),
                Err(e) => Some((false, e, 0)),
            }
        }
        "subagent" | "send_message" => {
            // 子代理派发在工具循环内提前处理（handle_subagent_tool），
            // 避免 handle_session_tool 未来类型与 run_turn_internal 相互递归
            None
        }
        "interrupt_agent" => {
            let Some(agent_id) = args.get("agent_id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 agent_id 参数".to_string(), 0));
            };
            if let Err(e) = super::subagent::check_child(session_id, agent_id) {
                return Some((false, e, 0));
            }
            request_cancel(agent_id);
            Some((true, format!("已请求中断子代理 {agent_id} 的进行中回合"), 0))
        }
        "list_agents" => {
            // 枚举全部代理运行状态（DSH list_agents 语义）：当前会话 + 子代理
            let mut lines = vec![format!(
                "- {session_id}（当前会话）{}",
                if is_turn_running(session_id) {
                    "运行中"
                } else {
                    "空闲"
                }
            )];
            for child in super::subagent::list_children(session_id) {
                lines.push(format!(
                    "- {child}（子代理）{}",
                    if is_turn_running(&child) {
                        "运行中"
                    } else {
                        "空闲"
                    }
                ));
            }
            Some((true, lines.join("\n"), started.elapsed().as_millis() as u64))
        }
        "subagent_list" => {
            let children = super::subagent::list_children(session_id);
            if children.is_empty() {
                return Some((true, "（本会话无子代理）".to_string(), 0));
            }
            let text = children
                .iter()
                .map(|id| {
                    let c = super::subagent::conclusion(id).unwrap_or_default();
                    let summary: String = c.chars().take(60).collect();
                    format!("- {id}：{summary}")
                })
                .collect::<Vec<_>>()
                .join("\n");
            Some((true, text, started.elapsed().as_millis() as u64))
        }
        "subagent_output" => {
            let Some(agent_id) = args.get("agent_id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 agent_id 参数".to_string(), 0));
            };
            if let Err(e) = super::subagent::check_child(session_id, agent_id) {
                return Some((false, e, 0));
            }
            match super::subagent::conclusion(agent_id) {
                Ok(c) => Some((true, c, started.elapsed().as_millis() as u64)),
                Err(e) => Some((false, e, 0)),
            }
        }
        "report" => {
            // 子代理 → 父会话回传（DSH tool-subagent-report 迁移）：
            // 仅分叉子代理可调用，内容写入直接父会话日志（模型可见 ⟺ 落日志），
            // 返回父会话内事件 seq 作为 messageId。
            let Some(output) = args.get("output").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 output 参数".to_string(), 0));
            };
            match super::subagent::report(session_id, output) {
                Ok(seq) => Some((
                    true,
                    format!("report 已送达启动你的父代理（消息 {seq}）"),
                    started.elapsed().as_millis() as u64,
                )),
                Err(e) => Some((false, e, 0)),
            }
        }
        "goal_create" => {
            // DSH 2026-07-19 model-facing-goal-tools：目标变更仅允许在含
            // 直接人类消息的回合内（自动续跑/子代理/定时任务回合不可改写）
            if !is_human_turn(session_id) {
                return Some((
                    false,
                    "目标变更需要直接的人类消息（当前回合非人类发起：自动续跑/子代理/定时任务不可改写目标）。请先让用户确认后再操作。".to_string(),
                    0,
                ));
            }
            let Some(objective) = args.get("objective").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 objective 参数".to_string(), 0));
            };
            let max_rounds = args.get("max_goal_rounds").and_then(|v| v.as_u64());
            store
                .append(
                    session_id,
                    &HarnessEvent::GoalSet {
                        objective: objective.to_string(),
                    },
                )
                .ok();
            store
                .append(
                    session_id,
                    &HarnessEvent::GoalUpdate {
                        objective: objective.to_string(),
                        status: "active".to_string(),
                        blocked_reason: String::new(),
                        max_goal_rounds: max_rounds,
                    },
                )
                .ok();
            Some((
                true,
                format!(
                    "目标已创建（active）：{}{}",
                    objective,
                    max_rounds
                        .map(|r| format!("；最大续跑轮次 {r}"))
                        .unwrap_or_default()
                ),
                started.elapsed().as_millis() as u64,
            ))
        }
        "goal_get" => {
            let state = match store.session_state(session_id) {
                Ok(s) => s,
                Err(e) => return Some((false, e, 0)),
            };
            if state.goal.is_empty() {
                return Some((true, "（本会话无目标）".to_string(), 0));
            }
            Some((
                true,
                format!(
                    "目标：{}\n状态：{}\n修订：{}{}{}",
                    state.goal,
                    state.goal_status,
                    state.goal_revision,
                    state
                        .goal_blocked_reason
                        .is_empty()
                        .then(String::new)
                        .unwrap_or_else(|| format!("\n阻塞原因：{}", state.goal_blocked_reason)),
                    state
                        .goal_max_rounds
                        .map(|r| format!("\n最大续跑轮次：{r}"))
                        .unwrap_or_default(),
                ),
                started.elapsed().as_millis() as u64,
            ))
        }
        "goal_update" => {
            // DSH 2026-07-19 model-facing-goal-tools：目标变更仅允许在含
            // 直接人类消息的回合内（自动续跑/子代理/定时任务回合不可改写）
            if !is_human_turn(session_id) {
                return Some((
                    false,
                    "目标变更需要直接的人类消息（当前回合非人类发起：自动续跑/子代理/定时任务不可改写目标）。请先让用户确认后再操作。".to_string(),
                    0,
                ));
            }
            let Some(action) = args.get("action").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 action 参数".to_string(), 0));
            };
            let mut state = match store.session_state(session_id) {
                Ok(s) => s,
                Err(e) => return Some((false, e, 0)),
            };
            if state.goal.is_empty() {
                return Some((false, "本会话尚无目标（先 goal_create）".to_string(), 0));
            }
            match action {
                "pause" | "resume" | "complete" | "blocked" => {
                    state.goal_status = action.to_string();
                }
                "edit" => {
                    let Some(objective) = args.get("objective").and_then(|v| v.as_str()) else {
                        return Some((false, "edit 需提供新 objective".to_string(), 0));
                    };
                    state.goal = objective.to_string();
                    state.goal_status = "active".to_string();
                }
                _ => {
                    return Some((false, format!("无效 action: {action}"), 0));
                }
            }
            if action == "blocked" {
                state.goal_blocked_reason = args
                    .get("blocked_reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("（未提供阻塞原因）")
                    .to_string();
            } else if action != "edit" {
                state.goal_blocked_reason = String::new();
            }
            store
                .append(
                    session_id,
                    &HarnessEvent::GoalUpdate {
                        objective: state.goal.clone(),
                        status: state.goal_status.clone(),
                        blocked_reason: state.goal_blocked_reason.clone(),
                        max_goal_rounds: state.goal_max_rounds,
                    },
                )
                .ok();
            Some((
                true,
                format!(
                    "目标已更新（{}）：{}{}",
                    state.goal_status,
                    state.goal,
                    if state.goal_blocked_reason.is_empty() {
                        String::new()
                    } else {
                        format!("；阻塞原因：{}", state.goal_blocked_reason)
                    }
                ),
                started.elapsed().as_millis() as u64,
            ))
        }
        "todo_write" => {
            let items_raw = args.get("items").and_then(|v| v.as_array()).cloned();
            let Some(items_raw) = items_raw else {
                return Some((false, "缺少 items 参数".to_string(), 0));
            };
            let items: Vec<super::session::TodoItem> = items_raw
                .iter()
                .enumerate()
                .map(|(i, it)| super::session::TodoItem {
                    id: format!("todo-{i}"),
                    content: it
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    status: it
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("pending")
                        .to_string(),
                })
                .collect();
            let summary = items
                .iter()
                .map(|t| format!("- [{}] {}", t.status, t.content))
                .collect::<Vec<_>>()
                .join("\n");
            let n = items.len();
            store
                .append(session_id, &HarnessEvent::TodoUpdate { items })
                .ok();
            Some((
                true,
                format!("待办列表已更新（{} 项）：\n{}", n, summary),
                started.elapsed().as_millis() as u64,
            ))
        }
        "plan_enter" => {
            let plan = args
                .get("plan")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            store
                .append(session_id, &HarnessEvent::PlanEnter { plan: plan.clone() })
                .ok();
            Some((
                true,
                "已进入计划模式：仅只读工具可用，执行 plan_exit 后恢复。".to_string(),
                started.elapsed().as_millis() as u64,
            ))
        }
        "plan_exit" => {
            // 方案评审流（DSH exit_plan_mode + PlanReviewPanel 语义）：
            // 携带 plan 文本时先交用户评审（确认执行 / 拒绝 / 去聊天里说），
            // 未确认执行则保持计划模式
            let plan = args.get("plan").and_then(|v| v.as_str());
            if let Some(plan) = plan.filter(|p| !p.trim().is_empty()) {
                let answer = match super::interaction::ask_user(
                    app,
                    session_id,
                    &format!("方案评审（计划模式退出）：\n{}", plan),
                    &[
                        "确认执行".to_string(),
                        "拒绝".to_string(),
                        "去聊天里说".to_string(),
                    ],
                    false,
                )
                .await
                {
                    Ok(a) => a,
                    Err(e) => return Some((false, e, 0)),
                };
                if !answer.contains("确认执行") {
                    let hint = if answer.contains("去聊天里说") {
                        "用户选择去聊天里说：评审已关闭，可在输入框继续对话（仍在计划模式）。"
                    } else {
                        "用户拒绝执行：仍在计划模式，请调整方案后再次 plan_exit 提交评审。"
                    };
                    return Some((true, hint.to_string(), started.elapsed().as_millis() as u64));
                }
            }
            store.append(session_id, &HarnessEvent::PlanExit).ok();
            Some((
                true,
                "已退出计划模式，全部工具恢复可用。".to_string(),
                started.elapsed().as_millis() as u64,
            ))
        }
        "ask_user_question" => {
            let Some(question) = args.get("question").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 question 参数".to_string(), 0));
            };
            let options = super::interaction::options_from_args(args);
            let multi_select = args
                .get("multi_select")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            match super::interaction::ask_user(app, session_id, question, &options, multi_select)
                .await
            {
                Ok(answer) => Some((
                    true,
                    format!("[用户回答] {}", answer),
                    started.elapsed().as_millis() as u64,
                )),
                Err(e) => Some((false, e, 0)),
            }
        }
        "session_search" => {
            let Some(query) = args.get("query").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 query 参数".to_string(), 0));
            };
            let query = query.trim();
            if query.is_empty() {
                return Some((false, "query 不能为空".to_string(), 0));
            }
            match store.search(query) {
                Ok(hits) if hits.is_empty() => Some((true, "（无匹配会话消息）".to_string(), 0)),
                Ok(hits) => {
                    let text = hits
                        .iter()
                        .map(|h| format!("- {} [{}] {}", h.session_id, h.event_type, h.snippet))
                        .collect::<Vec<_>>()
                        .join("\n");
                    Some((true, text, started.elapsed().as_millis() as u64))
                }
                Err(e) => Some((false, e, 0)),
            }
        }
        "session_trace" => {
            let target = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(session_id);
            match store.trace(target) {
                Ok(t) => Some((
                    true,
                    format!(
                        "会话 {} 血缘：\n祖先链（{}）：{}\n后代（{}）：{}",
                        target,
                        t.ancestors.len(),
                        if t.ancestors.is_empty() {
                            "（无，根会话）".to_string()
                        } else {
                            t.ancestors.join(" → ")
                        },
                        t.descendants.len(),
                        if t.descendants.is_empty() {
                            "（无）".to_string()
                        } else {
                            t.descendants.join(", ")
                        },
                    ),
                    started.elapsed().as_millis() as u64,
                )),
                Err(e) => Some((false, e, 0)),
            }
        }
        "session_event_read" => {
            let Some(seq) = args.get("seq").and_then(|v| v.as_i64()) else {
                return Some((false, "缺少 seq 参数".to_string(), 0));
            };
            let target = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(session_id);
            match store.event_read(target, seq) {
                Ok(Some((s, ev))) => {
                    let payload = serde_json::to_string_pretty(&ev).unwrap_or_default();
                    Some((
                        true,
                        format!("事件 #{s}：\n{payload}"),
                        started.elapsed().as_millis() as u64,
                    ))
                }
                Ok(None) => Some((true, format!("会话 {target} 无序号 {seq} 的事件"), 0)),
                Err(e) => Some((false, e, 0)),
            }
        }
        "session_event_search" => {
            let Some(query) = args.get("query").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 query 参数".to_string(), 0));
            };
            let target = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(session_id);
            match store.event_search(target, query) {
                Ok(hits) if hits.is_empty() => Some((true, "（无命中事件）".to_string(), 0)),
                Ok(hits) => {
                    let text = hits
                        .iter()
                        .take(20)
                        .map(|(seq, etype, snippet)| {
                            format!(
                                "#{seq} [{etype}] {}",
                                snippet.chars().take(120).collect::<String>()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    Some((
                        true,
                        format!("命中 {} 条：\n{}", hits.len(), text),
                        started.elapsed().as_millis() as u64,
                    ))
                }
                Err(e) => Some((false, e, 0)),
            }
        }
        "session_event_trace" => {
            let Some(seq) = args.get("seq").and_then(|v| v.as_i64()) else {
                return Some((false, "缺少 seq 参数".to_string(), 0));
            };
            let target = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(session_id);
            match store.event_trace(target, seq) {
                Ok(t) => {
                    let list = |v: &[i64]| -> String {
                        if v.is_empty() {
                            "none".to_string()
                        } else {
                            v.iter()
                                .map(|s| s.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        }
                    };
                    Some((
                        true,
                        format!(
                            "Session {}\nTarget: seq {} | {} | current | {}\nReplaced by: none\nReplacement chain: none\nEvents replaced by target: none\nEvents cited directly as sources: {}\nDirect derived events: {}",
                            target,
                            t.target_seq,
                            t.target_type,
                            t.target_time,
                            list(&t.source_event_seqs),
                            list(&t.derived_event_seqs),
                        ),
                        started.elapsed().as_millis() as u64,
                    ))
                }
                Err(e) => Some((false, e, 0)),
            }
        }
        // ─── 会话维护（session_list/create/rename/clear/delete：模型自维护会话） ───
        "session_list" => match store.list() {
            Ok(list) if list.is_empty() => Some((true, "（暂无会话）".to_string(), 0)),
            Ok(list) => {
                let text = list
                    .iter()
                    .map(|s| {
                        format!(
                            "- {}《{}》{} 条消息，更新 {}",
                            s.id, s.title, s.message_count, s.updated_at
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Some((true, text, started.elapsed().as_millis() as u64))
            }
            Err(e) => Some((false, e, 0)),
        },
        "session_create" => match store.create() {
            Ok(meta) => Some((
                true,
                format!("已创建新会话 {}（当前消息数 0）", meta.id),
                started.elapsed().as_millis() as u64,
            )),
            Err(e) => Some((false, e, 0)),
        },
        "session_rename" => {
            let Some(title) = args.get("title").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 title 参数".to_string(), 0));
            };
            let target = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(session_id);
            if let Err(e) = store.rename(target, title) {
                return Some((false, e, 0));
            }
            store
                .append(
                    target,
                    &HarnessEvent::SessionTitle {
                        title: title.to_string(),
                    },
                )
                .ok();
            Some((
                true,
                format!("会话 {target} 已重命名为「{title}」"),
                started.elapsed().as_millis() as u64,
            ))
        }
        "session_clear" => {
            let target = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(session_id);
            if let Err(e) = store.clear_messages(target, "model") {
                return Some((false, e, 0));
            }
            Some((
                true,
                format!("会话 {target} 的聊天记录已清空（会话保留，可重新开始对话）"),
                started.elapsed().as_millis() as u64,
            ))
        }
        "session_delete" => {
            let target = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(session_id);
            // 破坏性操作：先过用户审批（复用审批卡接缝）
            if let Err(e) =
                super::approval::request_approval(app, target, "session_delete", args).await
            {
                return Some((false, e, 0));
            }
            if let Err(e) = store.delete(target) {
                return Some((false, e, 0));
            }
            super::approval::clear_trust_for_session(target);
            Some((
                true,
                format!("会话 {target} 及其全部日志已删除"),
                started.elapsed().as_millis() as u64,
            ))
        }
        "attachment_list" => {
            let events = match store.events(session_id, 0) {
                Ok(e) => e,
                Err(e) => return Some((false, e, 0)),
            };
            let attachments = super::attachment::attachments_from_events(&events);
            if attachments.is_empty() {
                return Some((true, "（本会话无附件）".to_string(), 0));
            }
            let text = attachments
                .iter()
                .map(|a| {
                    format!(
                        "- {}（{}）{} {}",
                        a.name,
                        a.kind,
                        a.path,
                        if a.sha256.is_empty() {
                            String::new()
                        } else {
                            format!(" sha256:{}", a.sha256)
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            Some((true, text, started.elapsed().as_millis() as u64))
        }
        "goal_set" => {
            // DSH 2026-07-19 model-facing-goal-tools：目标变更仅允许在含
            // 直接人类消息的回合内（自动续跑/子代理/定时任务回合不可改写）
            if !is_human_turn(session_id) {
                return Some((
                    false,
                    "目标变更需要直接的人类消息（当前回合非人类发起：自动续跑/子代理/定时任务不可改写目标）。请先让用户确认后再操作。".to_string(),
                    0,
                ));
            }
            let Some(objective) = args.get("objective").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 objective 参数".to_string(), 0));
            };
            store
                .append(
                    session_id,
                    &HarnessEvent::GoalSet {
                        objective: objective.to_string(),
                    },
                )
                .ok();
            Some((
                true,
                format!("目标已设置：{}", objective),
                started.elapsed().as_millis() as u64,
            ))
        }
        "skill_list" => match super::skill::skill_list_result() {
            Ok(text) => Some((true, text, started.elapsed().as_millis() as u64)),
            Err(e) => Some((false, e, 0)),
        },
        "skill_load" => {
            let Some(name) = args.get("name").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 name 参数".to_string(), 0));
            };
            match super::skill::skill_load_result(name) {
                Ok(text) => Some((true, text, started.elapsed().as_millis() as u64)),
                Err(e) => Some((false, e, 0)),
            }
        }
        "lsp_hover" | "lsp_definition" | "lsp_references" | "lsp_implementation" => {
            // LSP 查询在阻塞线程池执行 + 15 秒硬超时（异常服务器不挂死会话）
            let args2 = args.clone();
            let op = match name {
                "lsp_definition" => super::lsp::LspOp::Definition,
                "lsp_references" => super::lsp::LspOp::References,
                "lsp_implementation" => super::lsp::LspOp::Implementation,
                _ => super::lsp::LspOp::Hover,
            };
            let out = tokio::time::timeout(
                std::time::Duration::from_secs(15),
                tauri::async_runtime::spawn_blocking(move || {
                    super::lsp::query_via_tool(op, &args2)
                }),
            )
            .await;
            match out {
                Ok(Ok(Ok(text))) => Some((true, text, started.elapsed().as_millis() as u64)),
                Ok(Ok(Err(e))) => Some((false, e, started.elapsed().as_millis() as u64)),
                Ok(Err(e)) => Some((
                    false,
                    format!("LSP 执行异常: {}", e),
                    started.elapsed().as_millis() as u64,
                )),
                Err(_) => Some((
                    false,
                    "LSP 查询超时（15 秒）".to_string(),
                    started.elapsed().as_millis() as u64,
                )),
            }
        }
        "task" => {
            let Some(task) = args.get("task").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 task 参数".to_string(), 0));
            };
            let (Some(provider), Some(model)) = (provider, model) else {
                return Some((false, "子代理无法解析提供方/模型".to_string(), 0));
            };
            let scope = super::preset::scope_for_session_id(session_id);
            match super::subagent::run_subagent(app, provider, model, task, &scope).await {
                Ok(out) => Some((
                    true,
                    format!("[子代理结论]\n{}", out),
                    started.elapsed().as_millis() as u64,
                )),
                Err(e) => Some((false, e, started.elapsed().as_millis() as u64)),
            }
        }
        "plugin_list" => {
            let plugins = crate::llm::agent_plugins::plugins_store()
                .lock()
                .unwrap()
                .clone();
            if plugins.is_empty() {
                return Some((true, "（未定义动态插件）".to_string(), 0));
            }
            let text = plugins
                .iter()
                .map(|p| {
                    let tools = p
                        .tools
                        .iter()
                        .map(|t| t.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "- {}《{}》[{}] 工具: {}{}",
                        p.id,
                        p.name,
                        if p.enabled { "启用" } else { "停用" },
                        tools,
                        if p.description.is_empty() {
                            String::new()
                        } else {
                            format!("（{}）", p.description)
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            Some((true, text, started.elapsed().as_millis() as u64))
        }
        "plugin_define" => {
            if session_plan_mode(store, session_id).unwrap_or(false)
                || super::settings::current().effective_sandbox_mode() == "read-only"
            {
                return Some((
                    false,
                    "当前处于计划模式：plugin_define 被拦截（仅只读工具可用）".to_string(),
                    0,
                ));
            }
            // 自修改（DSH extensions 语义）：先经用户审批
            if let Err(e) =
                super::approval::request_approval(app, session_id, "plugin_define", args).await
            {
                return Some((false, e, 0));
            }
            let plugin = plugin_from_args(args);
            match crate::llm::agent_plugins::define_plugin(plugin) {
                Ok(p) => Some((
                    true,
                    format!(
                        "插件已定义：{}《{}》（{} 个工具，{}）",
                        p.id,
                        p.name,
                        p.tools.len(),
                        if p.enabled { "启用" } else { "停用" }
                    ),
                    started.elapsed().as_millis() as u64,
                )),
                Err(e) => Some((false, e, 0)),
            }
        }
        "plugin_delete" => {
            if session_plan_mode(store, session_id).unwrap_or(false)
                || super::settings::current().effective_sandbox_mode() == "read-only"
            {
                return Some((
                    false,
                    "当前处于计划模式：plugin_delete 被拦截（仅只读工具可用）".to_string(),
                    0,
                ));
            }
            if let Err(e) =
                super::approval::request_approval(app, session_id, "plugin_delete", args).await
            {
                return Some((false, e, 0));
            }
            let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 id 参数".to_string(), 0));
            };
            match crate::llm::agent_plugins::delete_plugin(id) {
                Ok(()) => Some((true, format!("插件 {id} 已删除"), 0)),
                Err(e) => Some((false, e, 0)),
            }
        }
        "plugin_enable" | "plugin_disable" => {
            if session_plan_mode(store, session_id).unwrap_or(false)
                || super::settings::current().effective_sandbox_mode() == "read-only"
            {
                return Some((
                    false,
                    format!("当前处于计划模式：{name} 被拦截（仅只读工具可用）"),
                    0,
                ));
            }
            if let Err(e) = super::approval::request_approval(app, session_id, name, args).await {
                return Some((false, e, 0));
            }
            let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 id 参数".to_string(), 0));
            };
            let enabled = name == "plugin_enable";
            match crate::llm::agent_plugins::set_enabled(id, enabled) {
                Ok(p) => Some((
                    true,
                    format!(
                        "插件 {}《{}》已{}",
                        p.id,
                        p.name,
                        if enabled { "启用" } else { "停用" }
                    ),
                    started.elapsed().as_millis() as u64,
                )),
                Err(e) => Some((false, e, 0)),
            }
        }
        "run_code" => {
            if session_plan_mode(store, session_id).unwrap_or(false)
                || super::settings::current().effective_sandbox_mode() == "read-only"
            {
                return Some((
                    false,
                    "当前处于计划模式：run_code 被拦截（仅只读工具可用）".to_string(),
                    0,
                ));
            }
            // Code Mode（DSH code-runtime）：执行模型编写的程序，前端沙箱运行
            if let Err(e) =
                super::approval::request_approval(app, session_id, "run_code", args).await
            {
                return Some((false, e, 0));
            }
            let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
            if code.trim().is_empty() {
                return Some((false, "缺少 code 参数".to_string(), 0));
            }
            // 函数入参只取嵌套 args 字段（DSH run_code 语义：code 是 async
            // 函数体，参数经 args 传入），避免把 language/code 本身塞进上下文
            let code_args = args.get("args").cloned().unwrap_or_else(|| json!({}));
            let call_id = format!("harness-runcode-{}", uuid::Uuid::new_v4().simple());
            // B23：载荷携带 session_id——前端 ctx.tools 可调其它 Harness 工具
            let (ok, text) = crate::llm::agent_plugins::run_plugin_tool_on_ext(
                crate::llm::agent_plugins::PluginExecRequest {
                    app,
                    call_id: &call_id,
                    name: "run_code",
                    args: &code_args.to_string(),
                    code,
                    event: "harness-tool-exec-request",
                    timeout_secs: 60,
                    session_id,
                },
            )
            .await;
            Some((ok, text, started.elapsed().as_millis() as u64))
        }
        "workflow_run_js" => {
            // B2：workflow JS 编排（DSH workflow 组合子 agent/parallel/pipeline）。
            // 代码在前端 WebView 沙箱执行，ctx 提供 agent(prompt) 派生子代理
            // （经 harness_workflow_agent 原语）、parallel 并行、pipeline 流水线；
            // 返回脚本返回值。需审批；执行超时放宽到 300s（多轮子代理）。
            if let Err(e) =
                super::approval::request_approval(app, session_id, "workflow_run_js", args).await
            {
                return Some((false, e, 0));
            }
            let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
            if code.trim().is_empty() {
                return Some((false, "缺少 code 参数".to_string(), 0));
            }
            let code_args = args.get("args").cloned().unwrap_or_else(|| json!({}));
            let call_id = format!("harness-wf-{}", uuid::Uuid::new_v4().simple());
            let (ok, text) = crate::llm::agent_plugins::run_plugin_tool_on_ext(
                crate::llm::agent_plugins::PluginExecRequest {
                    app,
                    call_id: &call_id,
                    name: "workflow_run_js",
                    args: &code_args.to_string(),
                    code,
                    event: "harness-workflow-exec-request",
                    timeout_secs: 300,
                    session_id,
                },
            )
            .await;
            Some((ok, text, started.elapsed().as_millis() as u64))
        }
        _ => {
            // 启用插件的工具（DSH extensions）：定义代码在前端 WebView 执行
            if let Some((_pid, ptool)) = crate::llm::agent_plugins::find_plugin_tool(name) {
                if ptool.requires_approval {
                    if let Err(e) = super::approval::request_approval(
                        app,
                        session_id,
                        &format!("plugin:{}", name),
                        args,
                    )
                    .await
                    {
                        return Some((false, e, 0));
                    }
                }
                let call_id = format!("harness-plugin-{}", uuid::Uuid::new_v4().simple());
                // B23：载荷携带 session_id——插件代码 ctx.tools 可调其它工具
                let (ok, text) = crate::llm::agent_plugins::run_plugin_tool_on_ext(
                    crate::llm::agent_plugins::PluginExecRequest {
                        app,
                        call_id: &call_id,
                        name,
                        args: &args.to_string(),
                        code: &ptool.code,
                        event: "harness-tool-exec-request",
                        timeout_secs: 60,
                        session_id,
                    },
                )
                .await;
                return Some((ok, text, started.elapsed().as_millis() as u64));
            }
            None
        }
    }
}

/// 由 run_code / plugin_define 的 args 组装插件定义（工具参数 schema 透传）
fn plugin_from_args(args: &serde_json::Value) -> crate::llm::agent_plugins::AgentPlugin {
    use crate::llm::agent_plugins::{AgentPlugin, PluginToolDef};
    let tools: Vec<PluginToolDef> = args
        .get("tools")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|t| PluginToolDef {
            name: t
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            description: t
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            parameters: t
                .get("parameters")
                .cloned()
                .unwrap_or(json!({ "type": "object", "properties": {} })),
            requires_approval: t
                .get("requires_approval")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            code: t
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
        .collect();
    AgentPlugin {
        id: args
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        name: args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        description: args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        enabled: args
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        tools,
        versions: vec![],
        created_at: String::new(),
        updated_at: String::new(),
    }
}

/// 子代理派发（subagent / send_message）：独立于 handle_session_tool 处理，
/// 避免其未来类型与 run_turn_internal 相互递归导致 spawn 无法判 Send
pub(crate) async fn handle_subagent_tool(
    app: &tauri::AppHandle,
    session_id: &str,
    provider: Option<&crate::llm::types::ProviderConfig>,
    model: Option<&str>,
    name: &str,
    args: &serde_json::Value,
) -> Option<(bool, String, u64)> {
    let started = std::time::Instant::now();
    let (p, m) = match (provider, model) {
        (Some(p), Some(m)) => (p.clone(), m.to_string()),
        _ => {
            return Some((false, "子代理无法解析提供方/模型".to_string(), 0));
        }
    };
    match name {
        "subagent" => {
            let Some(task) = args.get("task").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 task 参数".to_string(), 0));
            };
            // DSH 2026-08-11 background-first-continuable-delegation：
            // 可继续子代理默认后台执行（立即返回子代理 id），仅当下一步
            // 依赖其结果时才显式传 run_in_background=false 前台等待
            let bg = args
                .get("run_in_background")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let child_id = match super::subagent::fork_child(session_id) {
                Ok(id) => id,
                Err(e) => return Some((false, e, 0)),
            };
            if bg {
                spawn_subagent_background(
                    app.clone(),
                    child_id.clone(),
                    p.id.clone(),
                    m.clone(),
                    task.to_string(),
                );
                Some((
                    true,
                    format!(
                        "已启动后台子代理 {child_id}。用 subagent_list 查看、subagent_output {child_id} 读结论、send_message 跟进、interrupt_agent 中断。"
                    ),
                    started.elapsed().as_millis() as u64,
                ))
            } else {
                mark_turn_running(&child_id);
                let r = run_turn_internal(app, &child_id, Some(&p.id), Some(&m), task).await;
                mark_turn_idle(&child_id);
                match r {
                    Ok(()) => match super::subagent::conclusion(&child_id) {
                        Ok(conclusion) => Some((
                            true,
                            format!("[子代理 {child_id} 结论]\n{conclusion}"),
                            started.elapsed().as_millis() as u64,
                        )),
                        Err(e) => Some((false, e, 0)),
                    },
                    Err(e) => Some((false, e, 0)),
                }
            }
        }
        "send_message" => {
            let Some(agent_id) = args.get("agent_id").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 agent_id 参数".to_string(), 0));
            };
            let Some(message) = args.get("message").and_then(|v| v.as_str()) else {
                return Some((false, "缺少 message 参数".to_string(), 0));
            };
            if let Err(e) = super::subagent::check_child(session_id, agent_id) {
                return Some((false, e, 0));
            }
            mark_turn_running(agent_id);
            let r = run_turn_internal(app, agent_id, Some(&p.id), Some(&m), message).await;
            mark_turn_idle(agent_id);
            match r {
                Ok(()) => match super::subagent::conclusion(agent_id) {
                    Ok(c) => Some((true, c, started.elapsed().as_millis() as u64)),
                    Err(e) => Some((false, e, 0)),
                },
                Err(e) => Some((false, e, 0)),
            }
        }
        _ => None,
    }
}

/// 当前会话是否处于计划模式（日志投影）
fn session_plan_mode(store: &SessionStore, session_id: &str) -> Result<bool, String> {
    Ok(store.session_state(session_id)?.plan_mode)
}

/// 工具结果溢写策略（DSH spill）：spill_read 的取回结果不再次溢写（防止
/// 「溢写 → 取回 → 再溢写」递归，截图实测卡死场景），其余超限结果落盘。
pub(crate) fn spill_result(session_id: &str, name: &str, result: &str) -> String {
    if name == "spill_read" {
        result.to_string()
    } else {
        super::spill::maybe_spill(session_id, result)
    }
}

/// B2：workflow JS 编排的子代理原语（前端 ctx.agent 调用）。
/// fork 子会话 + 一轮对话（子代理锁隔离，不阻塞父会话）+ 返回结论。
#[tauri::command]
pub async fn harness_workflow_agent(session_id: String, prompt: String) -> Result<String, String> {
    let app = crate::harness::runtime_app_handle()?;
    let child_id = super::subagent::fork_child(&session_id)?;
    let (provider, model) = resolve_provider_model(None, None)?;
    mark_turn_running(&child_id);
    let r = run_turn_internal(&app, &child_id, Some(&provider.id), Some(&model), &prompt).await;
    mark_turn_idle(&child_id);
    r?;
    super::subagent::conclusion(&child_id)
}

/// 内部执行一轮对话（无通道事件；schedule/workflow 复用）。
/// 返回 Box 化 future：工具循环内的子代理派发会嵌套再入本函数，
/// 构成真实递归——async fn 递归必须 box 化（E0733），
/// dyn 擦除同时斩断未来类型互递归。
pub(crate) fn run_turn_internal(
    app: &tauri::AppHandle,
    session_id: &str,
    provider_id: Option<&str>,
    model: Option<&str>,
    content: &str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'static>> {
    let app = app.clone();
    let session_id = session_id.to_string();
    let provider_id = provider_id.map(str::to_string);
    let model = model.map(str::to_string);
    let content = content.to_string();
    Box::pin(async move { run_turn(&app, session_id, provider_id, model, content, None).await })
}

/// 会话级回合互斥（H3：防日志交错）。同一会话的独立写入者——用户聊天
/// （harness_chat_stream）、定时任务（schedule run_due）、SDK/CLI 会话调用——
/// 必须串行化：会话日志是追加式唯一真相，并发追加会产出乱序上下文
/// （assistant 先于其 user、tool 结果错配）与展示/轨迹投影归组错乱。
///
/// 回合内嵌套调用（workflow_run 阶段、子代理推进同会话等）不取锁：
/// 它们与外部回合同属一个异步任务、天然串行，取锁会自锁死锁。
fn turn_locks() -> &'static Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>> {
    static L: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 获取指定会话的回合锁（Arc 克隆；条目按会话惰性创建，会话数有界）
pub(crate) async fn acquire_turn_lock(session_id: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut map = turn_locks().lock().unwrap();
    map.entry(session_id.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

/// 会话级互斥回合入口：独立写入者（定时任务 / SDK / CLI）用它串行化
/// 同一会话的回合，防止与用户聊天并发写日志。
pub(crate) async fn run_turn_locked(
    app: &tauri::AppHandle,
    session_id: &str,
    provider_id: Option<&str>,
    model: Option<&str>,
    content: &str,
) -> Result<(), String> {
    let lock = acquire_turn_lock(session_id).await;
    let _guard = lock.lock().await;
    run_turn_internal(app, session_id, provider_id, model, content).await
}

/// 人工命令：不经过模型，直接在会话中派发一次工具调用并落日志
/// （DSH ctx.commands 语义：命令派发不消耗模型回合）。
#[tauri::command]
pub async fn harness_execute_tool(
    app: tauri::AppHandle,
    session_id: String,
    name: String,
    arguments: String,
) -> Result<serde_json::Value, String> {
    execute_tool_command(&app, &session_id, &name, &arguments).await
}

/// 工具派发核心（SDK tool.execute 复用）。
/// H3：获取会话级锁串行化（与进行中回合/定时任务/工作流）。
pub(crate) async fn execute_tool_command(
    app: &tauri::AppHandle,
    session_id: &str,
    name: &str,
    arguments: &str,
) -> Result<serde_json::Value, String> {
    let lock = acquire_turn_lock(session_id).await;
    let _guard = lock.lock().await;
    execute_tool_command_inner(app, session_id, name, arguments).await
}

/// 无锁派发入口：**仅前端执行桥（ctx.tools）使用**——外层 run_code/插件/
/// workflow 派发已持有会话锁，嵌套调用再取锁会死锁；本路径不取锁，
/// 串行化由外层派发的锁保证。SDK/IPC 外部调用一律走带锁入口。
#[tauri::command]
pub async fn harness_execute_tool_nolock(
    app: tauri::AppHandle,
    session_id: String,
    name: String,
    arguments: String,
) -> Result<serde_json::Value, String> {
    execute_tool_command_inner(&app, &session_id, &name, &arguments).await
}

/// 派发主体（无锁；由 execute_tool_command / nolock 入口调用）
async fn execute_tool_command_inner(
    app: &tauri::AppHandle,
    session_id: &str,
    name: &str,
    arguments: &str,
) -> Result<serde_json::Value, String> {
    let store = super::registry::get::<SessionStore>("harness.sessions")
        .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    // 会话作用域禁用检查：预设禁用的工具在派发层拦截（模型目录过滤之外的双保险）
    let scope = super::preset::scope_for_session_id(session_id);
    if scope.is_disabled(name) {
        return Err(format!(
            "工具「{name}」已被会话预设「{}」禁用",
            scope.preset_name
        ));
    }
    // 沙箱只读模式守卫（人工派发路径与模型循环一致）：仅只读工具放行
    if super::settings::current().effective_sandbox_mode() == "read-only"
        && !super::tools::is_readonly_tool(name)
    {
        return Err(
            "当前处于沙箱只读模式：仅只读工具可用（可在治理→设置中切换沙箱模式）".to_string(),
        );
    }
    let args: serde_json::Value =
        serde_json::from_str(arguments).map_err(|e| format!("参数不是合法 JSON: {}", e))?;
    let call_id = format!("hcmd-{}", uuid::Uuid::new_v4().simple());
    let views = vec![ToolCallView {
        id: call_id.clone(),
        name: name.to_string(),
        arguments: arguments.to_string(),
    }];
    store
        .append(
            session_id,
            &HarnessEvent::AssistantToolCalls {
                id: format!("hcmd-a-{}", uuid::Uuid::new_v4().simple()),
                calls: views,
            },
        )
        .ok();
    let timeout = scope.tool_timeout(name, super::settings::current().effective_timeout_secs());
    // 手动派发（前端 CLI / ctx.tools 桥）由人类发起：标记人类回合，
    // 允许目标变更工具（DSH model-facing-goal-tools 权限边界）
    mark_human_turn(session_id, true);
    // 会话编排工具（todo/plan/goal/task）由运行时处理（需会话上下文）；
    // provider 解析失败不阻断 todo/plan/goal（仅 task 需要提供方）
    let provider_model = resolve_provider_model(None, None).ok();
    let handled = if matches!(name, "subagent" | "send_message") {
        handle_subagent_tool(
            app,
            session_id,
            provider_model.as_ref().map(|(p, _m)| p),
            provider_model.as_ref().map(|(_p, m)| m.as_str()),
            name,
            &args,
        )
        .await
    } else {
        handle_session_tool(
            app,
            &store,
            session_id,
            provider_model.as_ref().map(|(p, _m)| p),
            provider_model.as_ref().map(|(_p, m)| m.as_str()),
            name,
            &args,
        )
        .await
    };
    if let Some((ok, result, duration_ms)) = handled {
        // 溢写策略：超限工具结果落盘 + 预览替换（DSH spill；spill_read
        // 取回结果不再次溢写，防递归）
        let model_result = spill_result(session_id, name, &result);
        let persisted = crate::llm::agent::truncate_str(&model_result, 4000);
        store
            .append(
                session_id,
                &HarnessEvent::ToolResult {
                    id: call_id.clone(),
                    ok,
                    result: persisted.clone(),
                    duration_ms,
                },
            )
            .ok();
        super::hooks::fire(
            app,
            "tool_executed",
            session_id,
            json!({ "tool": name, "ok": ok, "duration_ms": duration_ms }),
        );
        mark_human_turn(session_id, false);
        return Ok(
            json!({ "id": call_id, "ok": ok, "result": persisted, "duration_ms": duration_ms }),
        );
    }
    let (ok, result, duration_ms) = if super::tools::requires_approval_scoped(name, &scope) {
        match super::approval::request_approval(app, session_id, name, &args).await {
            Ok(()) => run_tool(app, name, &args, timeout).await,
            Err(e) => (false, e, 0u64),
        }
    } else {
        run_tool(app, name, &args, timeout).await
    };
    // 溢写策略：超限工具结果落盘 + 预览替换（DSH spill；spill_read
    // 取回结果不再次溢写，防递归）
    let model_result = spill_result(session_id, name, &result);
    let persisted = crate::llm::agent::truncate_str(&model_result, 4000);
    store
        .append(
            session_id,
            &HarnessEvent::ToolResult {
                id: call_id.clone(),
                ok,
                result: persisted.clone(),
                duration_ms,
            },
        )
        .ok();
    super::hooks::fire(
        app,
        "tool_executed",
        session_id,
        json!({ "tool": name, "ok": ok, "duration_ms": duration_ms }),
    );
    mark_human_turn(session_id, false);
    Ok(json!({ "id": call_id, "ok": ok, "result": persisted, "duration_ms": duration_ms }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_turn_flag_gates_goal_mutations() {
        // DSH 2026-07-19 model-facing-goal-tools：目标变更仅允许在含直接
        // 人类消息的回合内；自动续跑/子代理回合不可改写
        let sid = format!("goal-gate-{}", uuid::Uuid::new_v4().simple());
        assert!(!is_human_turn(&sid), "默认非人类回合");
        mark_human_turn(&sid, true);
        assert!(is_human_turn(&sid), "人类回合应放行");
        mark_human_turn(&sid, false);
        assert!(!is_human_turn(&sid), "回合结束应清除标记");
    }

    #[test]
    fn goal_auto_round_continues_within_budget() {
        // max=2：首回合后（rounds_done=1）与第 1 次续跑后（rounds_done=2）
        // 都继续；rounds_done=3（第 2 次续跑完成后）停止——最大续跑 2 轮
        assert!(goal_auto_round_should_continue("目标", "active", 1, 2, 0));
        assert!(goal_auto_round_should_continue("目标", "active", 2, 2, 1));
        assert!(goal_auto_round_should_continue("目标", "active", 2, 2, 2));
        assert!(!goal_auto_round_should_continue("目标", "active", 3, 2, 3));
        // 无目标 / 非 active / max=0 / 轮次已满：不续跑
        assert!(!goal_auto_round_should_continue("", "active", 1, 2, 0));
        assert!(!goal_auto_round_should_continue(
            "目标", "complete", 1, 2, 0
        ));
        assert!(!goal_auto_round_should_continue("目标", "blocked", 1, 2, 0));
        assert!(!goal_auto_round_should_continue("目标", "active", 1, 0, 0));
        assert!(!goal_auto_round_should_continue("目标", "active", 1, 2, 3));
    }

    #[test]
    fn repeat_reminder_fires_at_thresholds_and_resets_on_tool_change() {
        // 阈值 [3,5,8]：同工具同参数连续调用到阈值才提醒；换工具即重置
        let sid = format!("rr-{}", uuid::Uuid::new_v4().simple());
        let args = json!({ "x": 1 });
        assert!(repeat_reminder(&sid, "tool_a", &args).is_none());
        assert!(repeat_reminder(&sid, "tool_a", &args).is_none());
        let r3 = repeat_reminder(&sid, "tool_a", &args).unwrap();
        assert!(r3.contains("3 次"), "阈值 3 应提醒: {r3}");
        // 第 4 次不提醒（阈值仅 3/5/8），第 5 次提醒
        assert!(repeat_reminder(&sid, "tool_a", &args).is_none());
        let r5 = repeat_reminder(&sid, "tool_a", &args).unwrap();
        assert!(r5.contains("5 次"), "阈值 5 应提醒: {r5}");
        // 换工具 → 计数重置（1 次不提醒）
        assert!(repeat_reminder(&sid, "tool_b", &args).is_none());
        // 参数不同 → 计数重置
        let args2 = json!({ "x": 2 });
        assert!(repeat_reminder(&sid, "tool_a", &args2).is_none());
    }

    #[test]
    fn cancel_flag_roundtrip() {
        let sid = format!("cc-{}", uuid::Uuid::new_v4().simple());
        assert!(!is_cancelled(&sid));
        request_cancel(&sid);
        assert!(is_cancelled(&sid));
        clear_cancel(&sid);
        assert!(!is_cancelled(&sid));
    }

    #[test]
    fn running_turn_flag_roundtrip() {
        let sid = format!("rt-{}", uuid::Uuid::new_v4().simple());
        assert!(!is_turn_running(&sid));
        mark_turn_running(&sid);
        assert!(is_turn_running(&sid));
        mark_turn_idle(&sid);
        assert!(!is_turn_running(&sid));
    }
}
