// ============================================================
// Harness — 会话核心（DSH core/session 纯原生迁移）
//
// 语义对齐 DSH：
// - 会话日志是追加式的 SessionEvent 流（SQLite 持久化，seq 单调递增）
// - 模型可见 ⟺ 落日志：模型上下文从日志投影（derive），不另行存储
// - UI / 回放 / 标题 / 消息数全部从事件流派生
// - 事件类型（tag 判别，前端增量恢复用 after_seq）：
//   user_message / assistant_chunk（流式增量，末块 done=true）/
//   assistant_message（回复边界，供投影与标题）/
//   assistant_tool_calls（模型发出的工具调用）/ tool_result（执行结果）/
//   session_title（改名记录）
// ============================================================

use serde::{Deserialize, Serialize};

/// 工具调用视图（模型可见的调用事实）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolCallView {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// 会话事件（追加式日志条目；type 判别联合）
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HarnessEvent {
    UserMessage {
        id: String,
        content: String,
    },
    AssistantChunk {
        id: String,
        delta: String,
        done: bool,
    },
    AssistantMessage {
        id: String,
        content: String,
        /// 推理过程全文（reasoning_content；DSH Think 推理行展示。
        /// 模型可见 ⟺ 落日志；旧日志无此字段，serde default 向后兼容）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
    },
    /// 助手回合发出的工具调用（模型可见 ⟺ 落日志）
    AssistantToolCalls {
        id: String,
        calls: Vec<ToolCallView>,
    },
    /// 单个工具调用结果
    ToolResult {
        id: String,
        ok: bool,
        result: String,
        duration_ms: u64,
    },
    /// 待办列表更新（todo_write）
    TodoUpdate {
        items: Vec<TodoItem>,
    },
    /// 进入计划模式（plan：logged state）
    PlanEnter {
        plan: String,
    },
    /// 退出计划模式
    PlanExit,
    /// 设置/更新同会话目标（goal）
    GoalSet {
        objective: String,
    },
    /// 目标生命周期更新（goal 状态机：status/revision/阻塞原因/轮次预算）
    GoalUpdate {
        objective: String,
        status: String,
        blocked_reason: String,
        max_goal_rounds: Option<u64>,
    },
    /// 工作流阶段执行记录（workflow：每阶段一条）
    WorkflowRun {
        workflow_id: String,
        name: String,
        stage: usize,
        total: usize,
        output: String,
    },
    /// 附件添加（attachment：列表/回放同源）
    AttachmentAdded {
        meta: crate::harness::attachment::AttachmentMeta,
    },
    /// 上下文压缩（compaction：摘要落日志）
    Compaction {
        removed_messages: usize,
        summary: String,
    },
    /// 会话分叉（fork：来源与边界落日志，可溯源回放）
    SessionForked {
        source: String,
        boundary_seq: i64,
    },
    SessionTitle {
        title: String,
    },
    /// 会话级 AI 角色注入（原「AI 聊天」角色功能迁移：name/prompt 落日志，
    /// 回合开始时投影进系统提示词；模型可见 ⟺ 落日志）
    RoleSet {
        name: String,
        prompt: String,
    },
    /// 会话聊天记录被清空（维护会话能力：清空后日志以本事件为新的起点）
    SessionCleared,
    /// 代理指令注入（DSH context/agent-instructions + ContextInjectionRow 迁移）：
    /// 回合开始时扫描到的 AGENTS.md/CLAUDE.md 文件列表随事件落日志，
    /// UI 投影为可展开的「上下文注入」行（渲染与回放同源）
    ContextInjected {
        files: Vec<String>,
    },
    /// 用户手势加载的技能注入（/skill <id>：内容随 <system-reminder> 注入
    /// 下一回合系统提示词）。注入来源随事件落日志（模型可见 ⟺ 落日志，
    /// 回放可重建本轮模型输入；UI 投影为可展开的「技能注入」行）
    SkillInjected {
        skills: Vec<String>,
    },
    /// 子代理回传（DSH tool-subagent-report 迁移）：分叉子代理通过 report 工具
    /// 把内容回传给直接父会话，落父会话日志（模型可见 ⟺ 落日志；
    /// UI 投影为「子代理报告」元信息行；仅直接父代理可见）。
    SubagentReported {
        /// 来源子代理会话 id（fork 溯源）
        child: String,
        /// 回传内容（自包含结论/进度）
        content: String,
    },
}

/// 待办条目
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: String,
}

/// 事件关系追踪结果（DSH session_event_trace 输出同构）：
/// target 面 + 替换面（追加式日志恒空）+ 来源/派生关系面。
#[derive(Serialize, Clone, Debug)]
pub struct EventTrace {
    pub target_seq: i64,
    /// 事件类型名（serde tag，如 user_message / forked）
    pub target_type: String,
    pub target_time: String,
    /// 直接替换目标的事件 seq（追加式日志无替换语义，恒 None）
    pub replaced_by: Option<i64>,
    /// 替换链（追加式日志恒空）
    pub replacement_chain: Vec<i64>,
    /// 被目标替换的事件 seq（追加式日志恒空）
    pub replaced_event_seqs: Vec<i64>,
    /// 目标事件直接引用的来源事件 seq
    pub source_event_seqs: Vec<i64>,
    /// 直接引用目标事件 seq 的派生事件（如分叉边界指向目标）
    pub derived_event_seqs: Vec<i64>,
}

/// 事件类型名（serde tag 语义；诊断与追踪输出共用）
pub(crate) fn event_type_name(ev: &HarnessEvent) -> &'static str {
    match ev {
        HarnessEvent::UserMessage { .. } => "user_message",
        HarnessEvent::AssistantChunk { .. } => "assistant_chunk",
        HarnessEvent::AssistantMessage { .. } => "assistant_message",
        HarnessEvent::AssistantToolCalls { .. } => "assistant_tool_calls",
        HarnessEvent::ToolResult { .. } => "tool_result",
        HarnessEvent::TodoUpdate { .. } => "todo_update",
        HarnessEvent::PlanEnter { .. } => "plan_enter",
        HarnessEvent::PlanExit => "plan_exit",
        HarnessEvent::GoalSet { .. } => "goal_set",
        HarnessEvent::GoalUpdate { .. } => "goal_update",
        HarnessEvent::WorkflowRun { .. } => "workflow_run",
        HarnessEvent::AttachmentAdded { .. } => "attachment_added",
        HarnessEvent::Compaction { .. } => "compaction",
        HarnessEvent::SessionForked { .. } => "session_forked",
        HarnessEvent::SessionTitle { .. } => "session_title",
        HarnessEvent::RoleSet { .. } => "role_set",
        HarnessEvent::SessionCleared => "session_cleared",
        HarnessEvent::ContextInjected { .. } => "context_injected",
        HarnessEvent::SkillInjected { .. } => "skill_injected",
        HarnessEvent::SubagentReported { .. } => "subagent_reported",
    }
}

/// 会话元信息（消息数 = 用户消息数；preset_id = 每会话预设作用域，
/// 空 = 跟随全局默认；workspace_id = 会话归属工作区，空 = 默认工作区；
/// archived = 会话归档标记，归档后从常规列表隐去）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HarnessSessionMeta {
    pub id: String,
    pub title: String,
    pub preset_id: String,
    pub workspace_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub archived: bool,
}

/// 展示投影：日志 → UI 消息列表
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum DisplayMessage {
    User {
        content: String,
        /// 日志 seq（分叉边界定位）
        seq: i64,
    },
    Assistant {
        content: String,
        /// 日志 seq（分叉边界定位）
        seq: i64,
        /// 该回复之前发生的工具步骤（从日志投影，渲染与回放同源）
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tools: Vec<ToolStepView>,
        /// 推理过程全文（Think 折叠行；DSH ReasoningRow 迁移）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
    },
    /// 会话元信息行（压缩 / 角色注入；渲染与回放同源，参考 DSH 的
    /// compaction / context-injection 节点）
    #[serde(rename = "meta")]
    MetaLine {
        /// 行类型：compaction（上下文压缩）| role（角色注入）| context（指令注入）| workflow（工作流阶段）
        kind: String,
        title: String,
        detail: String,
        /// 工作流阶段结构化视图（DSH WorkflowRunPanel：阶段进度点 + 状态文案；
        /// 旧日志无此字段，serde default 向后兼容）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workflow: Option<WorkflowStageView>,
    },
}

/// 工作流阶段视图（DSH WorkflowRunPanel RunHeader/PhaseSection 等价：
/// 一次运行的单个阶段记录；前端聚合连续阶段行为运行面板）
#[derive(Serialize, Clone, Debug)]
pub struct WorkflowStageView {
    pub workflow_id: String,
    pub name: String,
    pub stage: usize,
    pub total: usize,
}

/// 工具步骤视图（UI 展示用投影）
#[derive(Serialize, Clone, Debug)]
pub struct ToolStepView {
    pub id: String,
    pub name: String,
    pub args: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// 会话查询命中（session-query）
#[derive(Serialize, Clone, Debug)]
pub struct SearchHit {
    pub session_id: String,
    pub event_type: String,
    pub snippet: String,
}

/// 会话血缘（traceSession）
#[derive(Serialize, Clone, Debug)]
pub struct SessionTrace {
    pub ancestors: Vec<String>,
    pub descendants: Vec<String>,
}

/// 轨迹台账条目（DSH Trajectory 迁移：对话|轨迹 标签页数据源，日志投影。
/// 与 DSH 的 SYSTEM/USER/ASSISTANT/TOOL 台账类型对应，渲染与回放同源）
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrajectoryEntry {
    /// 用户消息
    User {
        seq: i64,
        /// 事件原始时间戳（DB created_at，格式 %Y-%m-%dT%H:%M:%S）
        time: String,
        content: String,
    },
    /// 助手回合（正文 + 该回合工具统计；权威边界 = assistant_message）
    Assistant {
        seq: i64,
        time: String,
        content: String,
        turn: u64,
        steps: usize,
        tool_calls: usize,
    },
    /// 工具调用（含结果与耗时；未闭合调用以 ok=false 收尾）
    Tool {
        seq: i64,
        time: String,
        id: String,
        name: String,
        args: String,
        ok: bool,
        result: String,
        duration_ms: u64,
    },
    /// 系统更新（todo/plan/goal/workflow/attachment/compaction/role/fork/title/cleared）
    System {
        seq: i64,
        time: String,
        event: String,
        summary: String,
        detail: String,
    },
}

/// 轨迹台账（entries + 汇总计数，供「轨迹」标签页渲染）
#[derive(Serialize, Clone, Debug)]
pub struct HarnessTrajectory {
    pub entries: Vec<TrajectoryEntry>,
    pub turn_count: u64,
    pub tool_call_count: u64,
}

/// 回合产物文件（DSH ProducedFiles 迁移：变更类工具的路径，日志投影）
#[derive(Serialize, Clone, Debug)]
pub struct TurnFileView {
    pub path: String,
    pub seq: i64,
}

/// 会话用量聚合（telemetry；含 DSH 统计条字段）
#[derive(Serialize, Clone, Debug)]
pub struct HarnessUsageSummary {
    pub session_id: String,
    /// 模型回合数（轮）
    pub turns: usize,
    /// 工具调用步数（日志投影）
    pub steps: usize,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cost: f64,
    /// LLM 请求墙钟合计（毫秒）
    pub llm_wall_ms: u64,
    /// 工具调用墙钟合计（毫秒，日志投影）
    pub tool_wall_ms: u64,
    /// 首 token / 首字节延迟平均（毫秒）
    pub first_token_avg_ms: f64,
    /// 输出 tokens / 秒
    pub tokens_per_sec: f64,
    /// 缓存命中率（0~1；无缓存字段时为 0）
    pub cache_hit_rate: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// 会话运行状态（从日志投影：plan 模式 / 目标 / 待办列表）
#[derive(Serialize, Clone, Debug, Default)]
pub struct SessionState {
    pub plan_mode: bool,
    pub plan_text: String,
    pub goal: String,
    /// 目标状态机：active / paused / blocked / complete
    pub goal_status: String,
    /// 目标修订号（GoalSet/GoalUpdate 事件计数）
    pub goal_revision: u64,
    /// 阻塞原因（blocked 状态时）
    pub goal_blocked_reason: String,
    /// 最大自动续跑轮次（DSH max_goal_rounds）
    pub goal_max_rounds: Option<u64>,
    pub todos: Vec<TodoItem>,
}

/// 会话存储服务（注册进 Cordis-lite 注册表，键 "harness.sessions"）
pub struct SessionStore {
    db: crate::db::Database,
}

impl SessionStore {
    pub fn new(db: crate::db::Database) -> Self {
        SessionStore { db }
    }

    pub fn create(&self) -> Result<HarnessSessionMeta, String> {
        self.create_in_workspace("")
    }

    /// 在指定工作区创建会话（workspace_id = "" 表示默认工作区；
    /// DSH 工作区浏览器：会话按工作区组织）
    pub fn create_in_workspace(&self, workspace_id: &str) -> Result<HarnessSessionMeta, String> {
        let id = format!("h-{}", uuid::Uuid::new_v4().simple());
        let now = now_iso();
        self.db
            .create_harness_session(&id, &now, workspace_id)
            .map_err(|e| format!("创建会话失败: {}", e))?;
        Ok(HarnessSessionMeta {
            id,
            title: String::new(),
            preset_id: String::new(),
            workspace_id: workspace_id.to_string(),
            created_at: now.clone(),
            updated_at: now,
            message_count: 0,
            archived: false,
        })
    }

    /// 设置会话归属工作区（会话在工作区间移动）
    pub fn set_workspace(&self, id: &str, workspace_id: &str) -> Result<(), String> {
        self.db
            .set_harness_session_workspace(id, workspace_id, &now_iso())
            .map_err(|e| format!("设置会话工作区失败: {}", e))
    }

    pub fn list(&self) -> Result<Vec<HarnessSessionMeta>, String> {
        let rows = self
            .db
            .list_harness_sessions()
            .map_err(|e| format!("读取会话列表失败: {}", e))?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    title,
                    preset_id,
                    workspace_id,
                    created_at,
                    updated_at,
                    message_count,
                    archived,
                )| {
                    HarnessSessionMeta {
                        id,
                        title,
                        preset_id,
                        workspace_id,
                        created_at,
                        updated_at,
                        message_count,
                        archived,
                    }
                },
            )
            .collect())
    }

    /// 设置会话归档标记（DSH workspace.archiveSession 语义；
    /// 归档不删除日志，可随时恢复）
    pub fn set_archived(&self, id: &str, archived: bool) -> Result<(), String> {
        self.db
            .set_harness_session_archived(id, archived)
            .map_err(|e| format!("设置会话归档失败: {}", e))
    }

    /// 设置会话手动排序序号（DSH 拖拽排序：交换双方各写一次）
    pub fn set_order(&self, id: &str, order_index: i64) -> Result<(), String> {
        self.db
            .set_harness_session_order(id, order_index)
            .map_err(|e| format!("设置会话排序失败: {}", e))
    }

    /// 交换两个会话的手动排序序号（DSH 拖拽排序：前端拖放即交换）
    pub fn swap_order(&self, a: &str, b: &str) -> Result<(), String> {
        self.db
            .swap_harness_session_order(a, b)
            .map_err(|e| format!("交换会话排序失败: {}", e))
    }

    /// 设置会话预设（每会话预设作用域）
    pub fn set_preset(&self, id: &str, preset_id: &str) -> Result<(), String> {
        self.db
            .set_harness_session_preset(id, preset_id)
            .map_err(|e| format!("设置会话预设失败: {}", e))
    }

    /// 读取会话预设
    pub fn preset_id(&self, id: &str) -> Result<Option<String>, String> {
        self.db
            .get_harness_session_preset(id)
            .map_err(|e| format!("读取会话预设失败: {}", e))
    }

    /// 会话分叉：复制源会话 seq <= boundary 的事件到新会话
    pub fn fork(&self, source: &str, boundary_seq: i64) -> Result<HarnessSessionMeta, String> {
        let child = format!("h-{}", uuid::Uuid::new_v4().simple());
        let now = now_iso();
        let copied = self
            .db
            .fork_harness_session(source, &child, boundary_seq, &now)
            .map_err(|e| format!("分叉失败: {}", e))?;
        // 分叉来源事件（回放可溯源）
        self.append(
            &child,
            &HarnessEvent::SessionForked {
                source: source.to_string(),
                boundary_seq,
            },
        )
        .ok();
        let preset = self
            .db
            .get_harness_session_preset(&child)
            .unwrap_or_default();
        // 分叉继承源会话的工作区归属（DSH fork 语义）
        let workspace_id = self
            .db
            .get_harness_session_workspace(&child)
            .unwrap_or_default()
            .unwrap_or_default();
        Ok(HarnessSessionMeta {
            id: child,
            title: {
                let list = self.list()?;
                list.iter()
                    .find(|m| m.id == source)
                    .map(|m| format!("{}（分叉）", m.title))
                    .unwrap_or_else(|| "分叉会话".to_string())
            },
            preset_id: preset.unwrap_or_default(),
            workspace_id,
            created_at: now.clone(),
            updated_at: now,
            message_count: copied,
            archived: false,
        })
    }

    /// 转写导出（回放）：日志投影为 Markdown
    pub fn export_markdown(&self, id: &str) -> Result<String, String> {
        let events = self.events(id, 0)?;
        let mut out = String::new();
        let meta = self
            .list()?
            .into_iter()
            .find(|m| m.id == id)
            .ok_or("会话不存在")?;
        out.push_str(&format!("# {}（{}）\n\n", meta.title, meta.id));
        let mut cur_chunks: Option<(String, String)> = None;
        for (_seq, ev) in events {
            match ev {
                HarnessEvent::UserMessage { content, .. } => {
                    flush_md(&mut out, &mut cur_chunks);
                    out.push_str(&format!("## 用户\n\n{}\n\n", content));
                }
                HarnessEvent::AssistantChunk { id, delta, done } => {
                    let entry = cur_chunks.get_or_insert_with(|| (id.clone(), String::new()));
                    entry.1.push_str(&delta);
                    if done {
                        flush_md(&mut out, &mut cur_chunks);
                    }
                }
                HarnessEvent::AssistantMessage { content, .. } => {
                    cur_chunks = None;
                    out.push_str(&format!("## 助手\n\n{}\n\n", content));
                }
                HarnessEvent::AssistantToolCalls { calls, .. } => {
                    flush_md(&mut out, &mut cur_chunks);
                    for c in calls {
                        out.push_str(&format!(
                            "> 🔧 工具调用 `{}`：`{}`\n\n",
                            c.name, c.arguments
                        ));
                    }
                }
                HarnessEvent::ToolResult {
                    id: _,
                    ok,
                    result,
                    duration_ms,
                } => {
                    out.push_str(&format!(
                        "> 结果（{}，{}ms）：{}\n\n",
                        if ok { "成功" } else { "失败" },
                        duration_ms,
                        result
                    ));
                }
                HarnessEvent::SessionForked {
                    source,
                    boundary_seq,
                } => {
                    out.push_str(&format!(
                        "> ⑂ 分叉自 {}（边界 seq {}）\n\n",
                        source, boundary_seq
                    ));
                }
                HarnessEvent::TodoUpdate { items } => {
                    flush_md(&mut out, &mut cur_chunks);
                    out.push_str("## 待办\n\n");
                    for t in items {
                        out.push_str(&format!("- [{}] {}\n", t.status, t.content));
                    }
                    out.push('\n');
                }
                HarnessEvent::PlanEnter { plan } => {
                    out.push_str(&format!("> 📋 进入计划模式：{}\n\n", plan));
                }
                HarnessEvent::PlanExit => out.push_str("> 📋 退出计划模式\n\n"),
                HarnessEvent::GoalSet { objective } => {
                    out.push_str(&format!("> 🎯 目标：{}\n\n", objective));
                }
                HarnessEvent::GoalUpdate {
                    objective,
                    status,
                    blocked_reason,
                    ..
                } => {
                    out.push_str(&format!(
                        "> 🎯 目标（{}）：{}{}\n\n",
                        status,
                        objective,
                        if blocked_reason.is_empty() {
                            String::new()
                        } else {
                            format!("（阻塞原因：{}）", blocked_reason)
                        }
                    ));
                }
                _ => {}
            }
        }
        flush_md(&mut out, &mut cur_chunks);
        Ok(out)
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<(), String> {
        if title.trim().is_empty() {
            return Err("标题不能为空".to_string());
        }
        self.db
            .rename_harness_session(id, title.trim())
            .map_err(|e| format!("重命名失败: {}", e))?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<usize, String> {
        self.db
            .delete_harness_session(id)
            .map_err(|e| format!("删除会话失败: {}", e))
    }

    /// 追加事件，返回 seq；session_title / 首条 user_message 联动标题投影与更新时间
    pub fn append(&self, session_id: &str, event: &HarnessEvent) -> Result<i64, String> {
        let now = now_iso();
        let (event_type, payload) = match event {
            HarnessEvent::UserMessage { .. } => ("user_message", serde_json::to_string(event)),
            HarnessEvent::AssistantChunk { .. } => {
                ("assistant_chunk", serde_json::to_string(event))
            }
            HarnessEvent::AssistantMessage { .. } => {
                ("assistant_message", serde_json::to_string(event))
            }
            HarnessEvent::AssistantToolCalls { .. } => {
                ("assistant_tool_calls", serde_json::to_string(event))
            }
            HarnessEvent::ToolResult { .. } => ("tool_result", serde_json::to_string(event)),
            HarnessEvent::TodoUpdate { .. } => ("todo_update", serde_json::to_string(event)),
            HarnessEvent::PlanEnter { .. } => ("plan_enter", serde_json::to_string(event)),
            HarnessEvent::PlanExit => ("plan_exit", serde_json::to_string(event)),
            HarnessEvent::GoalSet { .. } => ("goal_set", serde_json::to_string(event)),
            HarnessEvent::GoalUpdate { .. } => ("goal_update", serde_json::to_string(event)),
            HarnessEvent::WorkflowRun { .. } => ("workflow_run", serde_json::to_string(event)),
            HarnessEvent::AttachmentAdded { .. } => {
                ("attachment_added", serde_json::to_string(event))
            }
            HarnessEvent::Compaction { .. } => ("compaction", serde_json::to_string(event)),
            HarnessEvent::SessionForked { .. } => ("session_forked", serde_json::to_string(event)),
            HarnessEvent::SessionTitle { .. } => ("session_title", serde_json::to_string(event)),
            HarnessEvent::RoleSet { .. } => ("role_set", serde_json::to_string(event)),
            HarnessEvent::SessionCleared => ("session_cleared", serde_json::to_string(event)),
            HarnessEvent::ContextInjected { .. } => {
                ("context_injected", serde_json::to_string(event))
            }
            HarnessEvent::SkillInjected { .. } => ("skill_injected", serde_json::to_string(event)),
            HarnessEvent::SubagentReported { .. } => {
                ("subagent_reported", serde_json::to_string(event))
            }
        };
        let payload = payload.map_err(|e| format!("事件序列化失败: {}", e))?;
        let seq = self
            .db
            .append_harness_event(session_id, event_type, &payload, &now)
            .map_err(|e| format!("写入会话日志失败: {}", e))?;
        match event {
            // 首条用户消息投影会话标题（截断 40 字符）。
            // L4：判定 = seq==1 或标题为空（清空会话后新首条消息重新投影——
            // SessionCleared 占用 seq 1，清空后的首条消息 seq 不为 1）
            HarnessEvent::UserMessage { content, .. } => {
                let title_empty = self
                    .db
                    .get_harness_session_title(session_id)
                    .map(|t| t.is_empty())
                    .unwrap_or(true);
                let is_first = seq == 1 || title_empty;
                if is_first {
                    let title = truncate_chars(content.trim(), 40);
                    self.db
                        .set_harness_session_title(session_id, &title, &now)
                        .map_err(|e| format!("更新标题失败: {}", e))?;
                } else {
                    self.db
                        .touch_harness_session(session_id, &now)
                        .map_err(|e| format!("更新会话时间失败: {}", e))?;
                }
            }
            HarnessEvent::SessionTitle { title } => {
                let title = truncate_chars(title.trim(), 40);
                self.db
                    .rename_harness_session(session_id, &title)
                    .map_err(|e| format!("更新标题失败: {}", e))?;
            }
            _ => {}
        }
        Ok(seq)
    }

    /// 读取 after_seq 之后的事件（前端增量恢复 / 回放）
    pub fn events(
        &self,
        session_id: &str,
        after_seq: i64,
    ) -> Result<Vec<(i64, HarnessEvent)>, String> {
        let rows = self
            .db
            .get_harness_events(session_id, after_seq)
            .map_err(|e| format!("读取会话日志失败: {}", e))?;
        rows.into_iter()
            .map(|(seq, _type, payload, _ts)| {
                let ev = serde_json::from_str(&payload)
                    .map_err(|e| format!("解析会话日志失败: {}", e))?;
                Ok((seq, ev))
            })
            .collect()
    }

    /// 设置会话级 AI 角色（name/prompt 落日志；空 prompt = 清除角色）
    pub fn set_role(&self, session_id: &str, name: &str, prompt: &str) -> Result<(), String> {
        self.append(
            session_id,
            &HarnessEvent::RoleSet {
                name: name.to_string(),
                prompt: prompt.to_string(),
            },
        )
        .map(|_| ())
    }

    /// 清空会话聊天记录（维护会话能力）：删除全部事件与用量行，
    /// 随后以 SessionCleared 事件作为日志新起点（模型可见 ⟺ 落日志）。
    /// by 记录发起方（user / model），供日志叙事。
    pub fn clear_messages(&self, session_id: &str, by: &str) -> Result<(), String> {
        self.db
            .clear_harness_session(session_id)
            .map_err(|e| format!("清空会话失败: {}", e))?;
        // L4：清空后重置标题——新对话的首条用户消息重新投影标题
        // （否则标题停留在清空前，且 SessionCleared 占用 seq 1 使
        //  seq==1 判定失效）
        let now = now_iso();
        self.db
            .set_harness_session_title(session_id, "", &now)
            .map_err(|e| format!("重置标题失败: {}", e))?;
        // 清空动作自身落日志（先于任何新消息成为 seq 1）
        self.append(session_id, &HarnessEvent::SessionCleared)
            .map(|_| ())
            .map_err(|e| format!("记录清空事件失败: {}", e))?;
        log::info!("[harness] 会话 {} 聊天记录已清空（by={}）", session_id, by);
        Ok(())
    }

    /// 投影当前会话角色：日志中最后一条 RoleSet（渲染与回放同源）
    pub fn role(&self, session_id: &str) -> Result<(String, String), String> {
        let mut role: (String, String) = (String::new(), String::new());
        for (_seq, ev) in self.events(session_id, 0)? {
            if let HarnessEvent::RoleSet { name, prompt } = ev {
                role = (name, prompt);
            }
        }
        Ok(role)
    }

    /// 从事件流投影角色（agent 循环组装系统提示词时使用，避免重复查询）
    pub fn role_from_events(events: &[(i64, HarnessEvent)]) -> (String, String) {
        let mut role: (String, String) = (String::new(), String::new());
        for (_seq, ev) in events {
            if let HarnessEvent::RoleSet { name, prompt } = ev {
                role = (name.clone(), prompt.clone());
            }
        }
        role
    }

    /// 记录一轮对话的用量（telemetry：会话用量统计；含 DSH 统计条遥测）
    pub fn record_usage(&self, record: &mut crate::db::HarnessUsageRecord) -> Result<(), String> {
        record.created_at = now_iso();
        self.db
            .append_harness_usage(record)
            .map_err(|e| format!("记录会话用量失败: {}", e))
    }

    /// 会话用量聚合（telemetry 查询；DSH 统计条数据源）
    pub fn usage_summary(&self, session_id: &str) -> Result<HarnessUsageSummary, String> {
        let (turns, prompt, completion, cost, llm_wall, first_token, requests, cached) = self
            .db
            .harness_usage_summary(session_id)
            .map_err(|e| format!("读取会话用量失败: {}", e))?;
        // 步数与工具墙钟从事件日志投影（渲染与回放同源）
        let mut steps = 0usize;
        let mut tool_wall_ms = 0u64;
        for (_seq, ev) in self.events(session_id, 0)? {
            if let HarnessEvent::ToolResult { duration_ms, .. } = ev {
                steps += 1;
                tool_wall_ms += duration_ms;
            }
        }
        Ok(HarnessUsageSummary {
            session_id: session_id.to_string(),
            turns,
            steps,
            prompt_tokens: prompt,
            completion_tokens: completion,
            cost,
            llm_wall_ms: llm_wall,
            tool_wall_ms,
            // 首 token 平均 = Σ首 token / 请求数；tok/s = Σ输出 / LLM 墙钟
            first_token_avg_ms: if requests > 0 {
                first_token as f64 / requests as f64
            } else {
                0.0
            },
            tokens_per_sec: if llm_wall > 0 {
                completion as f64 / (llm_wall as f64 / 1000.0)
            } else {
                0.0
            },
            // 缓存命中率 = Σ缓存 / Σ输入（OpenAI/DeepSeek 兼容 usage 字段）
            cache_hit_rate: if prompt > 0 {
                (cached as f64 / prompt as f64).min(1.0)
            } else {
                0.0
            },
            input_tokens: prompt,
            output_tokens: completion,
        })
    }

    /// 提交会话反馈（feedback；message_seq = 助手消息序号，可选）
    pub fn submit_feedback(
        &self,
        session_id: &str,
        rating: &str,
        comment: &str,
        message_seq: Option<i64>,
    ) -> Result<(), String> {
        self.db
            .append_harness_feedback(session_id, rating, comment, message_seq, &now_iso())
            .map_err(|e| format!("保存反馈失败: {}", e))
    }

    /// 反馈列表（feedback）
    pub fn list_feedback(&self) -> Result<Vec<crate::harness::feedback::FeedbackRecord>, String> {
        let rows = self
            .db
            .list_harness_feedback()
            .map_err(|e| format!("读取反馈失败: {}", e))?;
        Ok(rows
            .into_iter()
            .map(
                |(id, session_id, rating, comment, message_seq, created_at)| {
                    crate::harness::feedback::FeedbackRecord {
                        id,
                        session_id,
                        rating,
                        comment,
                        message_seq,
                        created_at,
                    }
                },
            )
            .collect())
    }

    /// 会话查询（session-query：按关键词搜索事件载荷）
    pub fn search(&self, query: &str) -> Result<Vec<SearchHit>, String> {
        let rows = self
            .db
            .search_harness_sessions(query)
            .map_err(|e| format!("会话查询失败: {}", e))?;
        Ok(rows
            .into_iter()
            .map(|(session_id, event_type, snippet)| SearchHit {
                session_id,
                event_type,
                snippet,
            })
            .collect())
    }

    /// 读取单个完整事件（DSH session_event_read 语义：只读、工作区授权）
    pub fn event_read(
        &self,
        session_id: &str,
        seq: i64,
    ) -> Result<Option<(i64, HarnessEvent)>, String> {
        let events = self.events(session_id, 0)?;
        Ok(events.into_iter().find(|(s, _)| *s == seq))
    }

    /// 事件关系追踪（DSH session_event_trace 语义适配）：
    /// ST 日志为追加式事件溯源，无替换/shadow 语义——surface 恒为 current、
    /// 替换链恒空；关系面 = 目标事件引用的来源 seq（SessionForked.boundary_seq）
    /// 与引用目标 seq 的派生事件（分叉边界指向目标）。
    pub fn event_trace(&self, session_id: &str, seq: i64) -> Result<EventTrace, String> {
        let events = self.events_with_time(session_id)?;
        let target = events
            .iter()
            .find(|(s, _, _)| *s == seq)
            .ok_or_else(|| format!("会话 {session_id} 无序号 {seq} 的事件"))?;
        let (_, ts, ev) = target;
        let mut source_event_seqs = Vec::new();
        if let HarnessEvent::SessionForked { boundary_seq, .. } = ev {
            source_event_seqs.push(*boundary_seq);
        }
        let mut derived_event_seqs = Vec::new();
        for (s, _t, e) in &events {
            if *s <= seq {
                continue;
            }
            if let HarnessEvent::SessionForked { boundary_seq, .. } = e {
                if *boundary_seq == seq {
                    derived_event_seqs.push(*s);
                }
            }
        }
        Ok(EventTrace {
            target_seq: seq,
            target_type: event_type_name(ev).to_string(),
            target_time: ts.clone(),
            replaced_by: None,
            replacement_chain: Vec::new(),
            replaced_event_seqs: Vec::new(),
            source_event_seqs,
            derived_event_seqs,
        })
    }

    /// 单会话事件搜索（DSH session_event_search 语义）：
    /// 在指定会话的日志载荷内做关键词匹配，返回 (seq, 事件类型, 片段)
    pub fn event_search(
        &self,
        session_id: &str,
        query: &str,
    ) -> Result<Vec<(i64, String, String)>, String> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let rows = self
            .db
            .get_harness_events(session_id, 0)
            .map_err(|e| format!("读取会话日志失败: {}", e))?;
        let mut hits = Vec::new();
        for (seq, etype, payload, _ts) in rows {
            let lower = payload.to_lowercase();
            if !lower.contains(&q) {
                continue;
            }
            // 片段：命中位置前后各 60 字符（L1：全字符偏移，中文载荷
            // 不再因字节/字符混用错位）
            let idx = lower.find(&q).unwrap_or(0);
            let char_idx = lower[..idx].chars().count();
            let q_chars = q.chars().count();
            let total_chars = payload.chars().count();
            let start = char_idx.saturating_sub(60);
            let end = (char_idx + q_chars + 60).min(total_chars);
            let snippet: String = payload.chars().skip(start).take(end - start).collect();
            hits.push((seq, etype, snippet));
        }
        Ok(hits)
    }

    /// 会话血缘（DSH session-query traceSession 语义）：
    /// 祖先链 = 沿 SessionForked 溯源逐级向上（fork 复制事件后追加
    /// SessionForked，不限于首事件——全量扫描）；后代 = 日志中存在
    /// 溯源到本会话的分叉
    pub fn trace(&self, session_id: &str) -> Result<SessionTrace, String> {
        let mut ancestors = Vec::new();
        let mut current = session_id.to_string();
        for _ in 0..16 {
            // 深度上限防环；全量扫描找分叉溯源事件
            let source = self
                .events(&current, 0)?
                .into_iter()
                .find_map(|(_, e)| match e {
                    HarnessEvent::SessionForked { source, .. } => Some(source),
                    _ => None,
                });
            match source {
                Some(src) => {
                    ancestors.push(src.clone());
                    current = src;
                }
                None => break,
            }
        }
        let sessions = self.list()?;
        let mut descendants = Vec::new();
        for m in sessions {
            // 事件日志中存在分叉溯源（fork 复制事件后追加 SessionForked，
            // 不限于首事件——全量扫描）
            let forked_from = self.events(&m.id, 0).ok().and_then(|evs| {
                evs.iter().find_map(|(_, e)| match e {
                    HarnessEvent::SessionForked { source, .. } => Some(source.clone()),
                    _ => None,
                })
            });
            if forked_from.as_deref() == Some(session_id) {
                descendants.push(m.id);
            }
        }
        Ok(SessionTrace {
            ancestors,
            descendants,
        })
    }

    /// KV 存储（storage 能力后端）
    pub fn kv_put(&self, key: &str, value: &str) -> Result<(), String> {
        self.db
            .harness_kv_put(key, value)
            .map_err(|e| format!("KV 写入失败: {}", e))
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<String>, String> {
        self.db
            .harness_kv_get(key)
            .map_err(|e| format!("KV 读取失败: {}", e))
    }

    pub fn kv_delete(&self, key: &str) -> Result<(), String> {
        self.db
            .harness_kv_delete(key)
            .map_err(|e| format!("KV 删除失败: {}", e))
    }

    /// 模型上下文投影：从日志派生 OpenAI 消息序列（含 assistant tool_calls
    /// 与 role=tool 结果——模型可见 ⟺ 落日志，投影与回放同源）。
    /// 压缩持久化（H4）：最近一次 Compaction 事件之前的历史被摘要占位替换，
    /// 摘要只生成一次、跨回合生效——后续回合从日志投影即得，不再每回合
    /// 重复全量压缩（消除重复摘要 LLM 调用）。
    pub fn derive_model_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<serde_json::Value>, String> {
        let events = self.events(session_id, 0)?;
        // 最近一次压缩边界（seq）：其前事件折叠为摘要
        let mut compacted_until: Option<(i64, String)> = None;
        for (seq, ev) in events.iter() {
            if let HarnessEvent::Compaction { summary, .. } = ev {
                compacted_until = Some((*seq, summary.clone()));
            }
        }
        let mut out: Vec<serde_json::Value> = Vec::new();
        for (seq, ev) in events {
            // 压缩边界之前的历史：跳过（已折叠进摘要占位）
            if let Some((cseq, _)) = &compacted_until {
                if seq < *cseq {
                    continue;
                }
            }
            match ev {
                HarnessEvent::Compaction { summary, .. } => {
                    out.push(serde_json::json!({
                        "role": "user",
                        "content": format!("[较早对话摘要]\n{}", summary),
                    }));
                }
                HarnessEvent::UserMessage { content, .. } => {
                    out.push(serde_json::json!({ "role": "user", "content": content }));
                }
                // 子代理报告投影为用户消息（父代理模型可见；DSH user/message 语义）
                HarnessEvent::SubagentReported { child, content } => {
                    out.push(serde_json::json!({
                        "role": "user",
                        "content": format!("[子代理 {child} 报告]\n{}", content),
                    }));
                }
                HarnessEvent::AssistantMessage {
                    content, reasoning, ..
                } => {
                    // DSH deepseek-reasoning-passback（2026-08-19）：每个含推理的
                    // 助手轮次都回传 reasoning_content（与是否带工具调用无关），
                    // 否则经 OpenAI 兼容网关转发时对话无法按推理签名重建。
                    let mut m = serde_json::json!({ "role": "assistant", "content": content });
                    if let Some(r) = reasoning {
                        if !r.trim().is_empty() {
                            m["reasoning_content"] = serde_json::json!(r);
                        }
                    }
                    out.push(m);
                }
                HarnessEvent::AssistantToolCalls { calls, .. } => {
                    let calls_json: Vec<serde_json::Value> = calls
                        .iter()
                        .map(|c| {
                            serde_json::json!({
                                "id": c.id,
                                "type": "function",
                                "function": { "name": c.name, "arguments": c.arguments },
                            })
                        })
                        .collect();
                    out.push(serde_json::json!({
                        "role": "assistant",
                        "content": null,
                        "tool_calls": calls_json,
                    }));
                }
                HarnessEvent::ToolResult { id, result, .. } => {
                    out.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": id,
                        "content": result,
                    }));
                }
                _ => {}
            }
        }
        // 防御性清理（重放有效性）：进程崩溃/回合中断可能在日志尾部残留
        // 未闭合 tool_calls（有调用无结果），直接投影会给模型 API 非法序列
        Self::sanitize_model_messages(&mut out);
        Ok(out)
    }

    /// 防御性清理模型消息序列（DSH 重放有效性不变式）：
    /// 追加式日志在进程崩溃/回合中断时可能残留「尾部未闭合 tool_calls」
    /// （有调用无结果），直接投影会让模型 API 400（与 fork 的
    /// clean_boundary 同理）。投影层剥离最后一条未闭合的 assistant
    /// tool_calls 及其后的孤儿 tool 结果；只影响投影、不改日志
    /// （模型可见 ⟺ 落日志不变式保持：投影内容仍全部来自日志）。
    fn sanitize_model_messages(out: &mut Vec<serde_json::Value>) {
        // 先剥离孤儿 tool 结果：tool_call_id 未被任何前置 assistant tool_calls
        // 引用的 tool 消息对模型 API 恒为非法（防御：正常流程不会产生，崩溃
        // 日志可能残留）。
        let mut referenced: std::collections::HashSet<String> = std::collections::HashSet::new();
        out.retain(|m| match m.get("role").and_then(|r| r.as_str()) {
            Some("assistant") => {
                if let Some(calls) = m.get("tool_calls").and_then(|c| c.as_array()) {
                    for c in calls {
                        if let Some(id) = c.get("id").and_then(|i| i.as_str()) {
                            referenced.insert(id.to_string());
                        }
                    }
                }
                true
            }
            Some("tool") => m
                .get("tool_call_id")
                .and_then(|i| i.as_str())
                .map(|id| referenced.contains(id))
                .unwrap_or(false),
            _ => true,
        });
        // 定位最后一条 assistant tool_calls（非空调用集）
        let mut last_calls: Option<usize> = None;
        for (i, m) in out.iter().enumerate() {
            let is_calls = m.get("role").and_then(|r| r.as_str()) == Some("assistant")
                && m.get("tool_calls")
                    .and_then(|c| c.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false);
            if is_calls {
                last_calls = Some(i);
            }
        }
        let Some(k) = last_calls else {
            return;
        };
        let ids: std::collections::HashSet<String> = out[k]
            .get("tool_calls")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.get("id").and_then(|i| i.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        // 该调用之后必须由匹配的 tool 结果全部闭合（否则为尾部未闭合）
        let after = &out[k + 1..];
        let all_resolved = !ids.is_empty()
            && ids.iter().all(|id| {
                after
                    .iter()
                    .any(|m| m.get("tool_call_id").and_then(|i| i.as_str()) == Some(id.as_str()))
            });
        if !all_resolved {
            // 未闭合：剥离该调用及其后的孤儿 tool 结果（投影层，日志保留可审计）
            out.truncate(k);
        }
    }

    /// UI 投影：日志 → 展示消息（assistant_chunk 按 id 归组；assistant_message
    /// 为权威边界；工具步骤挂到其后的助手回复上；seq 供分叉边界定位）
    pub fn derive_display_messages(&self, session_id: &str) -> Result<Vec<DisplayMessage>, String> {
        let events = self.events(session_id, 0)?;
        let mut out: Vec<DisplayMessage> = Vec::new();
        let mut cur_chunks: Option<(String, String, i64)> = None; // (assistant id, 累计文本, 末块 seq)
        let mut pending_tools: Vec<ToolStepView> = Vec::new();
        for (seq, ev) in events {
            match ev {
                HarnessEvent::UserMessage { content, .. } => {
                    // 中断回合的工具步骤（M5）：无助手回复时也呈现，与轨迹/
                    // 模型上下文投影一致（正常回合由随后的 assistant_message 消费）
                    if cur_chunks.is_none() && !pending_tools.is_empty() {
                        out.push(DisplayMessage::Assistant {
                            content: "（回合中断，工具已执行，未产生回复）".to_string(),
                            seq,
                            tools: std::mem::take(&mut pending_tools),
                            reasoning: None,
                        });
                    }
                    flush_chunks(&mut out, &mut cur_chunks, &mut pending_tools);
                    pending_tools = Vec::new();
                    out.push(DisplayMessage::User { content, seq });
                }
                HarnessEvent::AssistantChunk { id, delta, .. } => {
                    // L9：不同 assistant id 的分块不得合并（异常日志回放时
                    // 各自成段，避免把不同回复的流式文本拼成一条）
                    if let Some((cur_id, _, _)) = &cur_chunks {
                        if cur_id != &id {
                            flush_chunks(&mut out, &mut cur_chunks, &mut pending_tools);
                        }
                    }
                    let entry = cur_chunks.get_or_insert_with(|| (id.clone(), String::new(), seq));
                    entry.1.push_str(&delta);
                    entry.2 = seq;
                    // done 不在此冲刷：权威边界是随后的 assistant_message，
                    // 若在这里出消息会导致同一回复投影出两条（修复重复显示）。
                    // 未闭合的分块（回合中断/无权威边界）由后续 UserMessage
                    // 或收尾 flush_chunks 兜底呈现。
                }
                HarnessEvent::AssistantMessage {
                    content, reasoning, ..
                } => {
                    // 权威边界：丢弃分块累计（避免重复），直接以完整内容呈现；
                    // 其前发生的工具步骤随该回复一并展示；推理全文随 Think 行展示
                    cur_chunks = None;
                    out.push(DisplayMessage::Assistant {
                        content,
                        seq,
                        tools: std::mem::take(&mut pending_tools),
                        reasoning,
                    });
                }
                HarnessEvent::AssistantToolCalls { calls, .. } => {
                    for c in calls {
                        pending_tools.push(ToolStepView {
                            id: c.id,
                            name: c.name,
                            args: c.arguments,
                            status: "running".to_string(),
                            result: None,
                            duration_ms: None,
                        });
                    }
                }
                HarnessEvent::ToolResult {
                    id,
                    ok,
                    result,
                    duration_ms,
                } => {
                    if let Some(step) = pending_tools.iter_mut().find(|s| s.id == id) {
                        step.status = if ok {
                            "ok".to_string()
                        } else {
                            "err".to_string()
                        };
                        step.result = Some(result);
                        step.duration_ms = Some(duration_ms);
                    }
                }
                HarnessEvent::TodoUpdate { .. }
                | HarnessEvent::PlanEnter { .. }
                | HarnessEvent::PlanExit
                | HarnessEvent::GoalSet { .. }
                | HarnessEvent::GoalUpdate { .. }
                | HarnessEvent::AttachmentAdded { .. }
                | HarnessEvent::SessionForked { .. } => {}
                HarnessEvent::SessionTitle { .. } => {}
                HarnessEvent::Compaction {
                    removed_messages,
                    summary,
                } => {
                    // 压缩标记行：位置即事件发生处（渲染与回放同源）
                    out.push(DisplayMessage::MetaLine {
                        kind: "compaction".to_string(),
                        title: format!("上下文已压缩 · 移除 {} 条消息", removed_messages),
                        detail: summary,
                        workflow: None,
                    });
                }
                HarnessEvent::RoleSet { name, prompt } => {
                    out.push(DisplayMessage::MetaLine {
                        kind: "role".to_string(),
                        title: format!("注入 AI 角色：{}", name),
                        detail: prompt,
                        workflow: None,
                    });
                }
                HarnessEvent::SessionCleared => {} // 清空后无残留展示
                HarnessEvent::ContextInjected { files } => {
                    // 上下文注入行（DSH ContextInjectionRow 迁移）：文件列表可展开
                    out.push(DisplayMessage::MetaLine {
                        kind: "context".to_string(),
                        title: format!("上下文注入 · {} 个指令文件", files.len()),
                        detail: files.join("\n"),
                        workflow: None,
                    });
                }
                HarnessEvent::SkillInjected { skills } => {
                    // 技能注入行（/skill 手势）：注入的技能 id 列表可展开
                    out.push(DisplayMessage::MetaLine {
                        kind: "skill".to_string(),
                        title: format!("注入技能 · {} 个", skills.len()),
                        detail: skills.join("\n"),
                        workflow: None,
                    });
                }
                HarnessEvent::SubagentReported { child, content } => {
                    // 子代理报告行（DSH tool-subagent-report 迁移）：来源 + 内容可展开
                    out.push(DisplayMessage::MetaLine {
                        kind: "subagent".to_string(),
                        title: format!("子代理 {child} 报告"),
                        detail: content,
                        workflow: None,
                    });
                }
                HarnessEvent::WorkflowRun {
                    workflow_id,
                    name,
                    stage,
                    total,
                    output,
                } => {
                    // 工作流阶段行（DSH WorkflowRunPanel 等价投影：阶段进度点 + 输出；
                    // stage 从 0 起存储，展示时 +1 对齐用户可读序号）
                    out.push(DisplayMessage::MetaLine {
                        kind: "workflow".to_string(),
                        title: format!("工作流「{}」阶段 {}/{}", name, stage + 1, total),
                        detail: output,
                        workflow: Some(WorkflowStageView {
                            workflow_id: workflow_id.clone(),
                            name: name.clone(),
                            stage,
                            total,
                        }),
                    });
                }
            }
        }
        flush_chunks(&mut out, &mut cur_chunks, &mut pending_tools);
        Ok(out)
    }

    /// 读取事件 + 原始时间戳（轨迹台账需要时间轴；DB created_at 为
    /// %Y-%m-%dT%H:%M:%S 字符串，前端直接展示/格式化）
    fn events_with_time(
        &self,
        session_id: &str,
    ) -> Result<Vec<(i64, String, HarnessEvent)>, String> {
        let rows = self
            .db
            .get_harness_events(session_id, 0)
            .map_err(|e| format!("读取会话日志失败: {}", e))?;
        rows.into_iter()
            .map(|(seq, _type, payload, ts)| {
                let ev = serde_json::from_str(&payload)
                    .map_err(|e| format!("解析会话日志失败: {}", e))?;
                Ok((seq, ts, ev))
            })
            .collect()
    }

    /// 轨迹台账投影：日志 → (用户/助手/工具/系统) 条目序列。
    /// 与 DSH Trajectory 台账语义一致：轮次以用户消息为边界，工具条目
    /// 在所属助手回复之前出现（先执行后作答的时序），系统更新就地入账。
    pub fn trajectory(&self, session_id: &str) -> Result<HarnessTrajectory, String> {
        let events = self.events_with_time(session_id)?;
        let mut entries: Vec<TrajectoryEntry> = Vec::new();
        let mut turn: u64 = 0;
        let mut tool_call_count: u64 = 0;
        // 助手回合组装缓冲：(seq, time, 累计正文, turn, steps, calls)
        let mut cur_assistant: Option<(i64, String, String, u64, usize, usize)> = None;
        // 待闭合工具调用：call id → (seq, time, name, args)
        let mut pending_tools: Vec<(String, i64, String, String, String)> = Vec::new();
        // 工具结果表：call id → (ok, result, duration_ms)
        let mut tool_results: std::collections::HashMap<String, (bool, String, u64)> =
            std::collections::HashMap::new();

        // 局部闭包：把待闭合工具 flush 为 Tool 条目（未闭合以 ok=false 收尾），
        // 返回 flush 条数（助手条目的 steps/tool_calls 统计来源）
        let flush_tools = |entries: &mut Vec<TrajectoryEntry>,
                           pending: &mut Vec<(String, i64, String, String, String)>,
                           results: &mut std::collections::HashMap<String, (bool, String, u64)>,
                           count: &mut u64|
         -> usize {
            let n = pending.len();
            for (id, seq, time, name, args) in pending.drain(..) {
                let (ok, result, duration_ms) =
                    results
                        .remove(&id)
                        .unwrap_or((false, "（未记录结果）".to_string(), 0));
                // L8：工具调用次数统计全部调用（成功与失败/未闭合），
                // 与 TrajectoryEntry::Tool 全量入账一致
                *count += 1;
                entries.push(TrajectoryEntry::Tool {
                    seq,
                    time,
                    id,
                    name,
                    args,
                    ok,
                    result,
                    duration_ms,
                });
            }
            n
        };
        // 局部闭包：闭合助手回合缓冲为 Assistant 条目
        let flush_assistant =
            |entries: &mut Vec<TrajectoryEntry>,
             cur: &mut Option<(i64, String, String, u64, usize, usize)>| {
                if let Some((seq, time, content, t, steps, calls)) = cur.take() {
                    entries.push(TrajectoryEntry::Assistant {
                        seq,
                        time,
                        content,
                        turn: t,
                        steps,
                        tool_calls: calls,
                    });
                }
            };

        for (seq, time, ev) in events {
            match ev {
                HarnessEvent::UserMessage { content, .. } => {
                    flush_tools(
                        &mut entries,
                        &mut pending_tools,
                        &mut tool_results,
                        &mut tool_call_count,
                    );
                    flush_assistant(&mut entries, &mut cur_assistant);
                    turn += 1;
                    entries.push(TrajectoryEntry::User { seq, time, content });
                }
                HarnessEvent::AssistantChunk { id, delta, .. } => {
                    let entry = cur_assistant
                        .get_or_insert_with(|| (seq, time.clone(), String::new(), turn, 0, 0));
                    // chunk 的 time 以首个 chunk 为准（回合开始时刻）
                    entry.2.push_str(&delta);
                    let _ = id;
                }
                HarnessEvent::AssistantMessage { content, .. } => {
                    // 权威边界：工具先行 flush（其条数即本回合步骤/调用统计，
                    // 与 chunk 缓冲解耦——纯 assistant_message 无 chunk 也正确），
                    // 再以完整正文闭合回合
                    let flushed = flush_tools(
                        &mut entries,
                        &mut pending_tools,
                        &mut tool_results,
                        &mut tool_call_count,
                    );
                    let t = match cur_assistant.take() {
                        Some((_, _, _, t, _, _)) => t,
                        None => turn,
                    };
                    entries.push(TrajectoryEntry::Assistant {
                        seq,
                        time,
                        content,
                        turn: t,
                        steps: flushed,
                        tool_calls: flushed,
                    });
                }
                HarnessEvent::AssistantToolCalls { calls, .. } => {
                    for c in calls {
                        pending_tools.push((c.id, seq, time.clone(), c.name, c.arguments));
                    }
                }
                HarnessEvent::ToolResult {
                    id,
                    ok,
                    result,
                    duration_ms,
                } => {
                    tool_results.insert(id, (ok, result, duration_ms));
                }
                HarnessEvent::TodoUpdate { items } => {
                    let summary = format!("待办更新（{} 项）", items.len());
                    let detail = items
                        .iter()
                        .map(|t| format!("{} {}", t.status, t.content))
                        .collect::<Vec<_>>()
                        .join("\n");
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "todo".to_string(),
                        summary,
                        detail,
                    });
                }
                HarnessEvent::PlanEnter { plan } => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "plan".to_string(),
                        summary: "进入计划模式".to_string(),
                        detail: plan,
                    });
                }
                HarnessEvent::PlanExit => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "plan".to_string(),
                        summary: "退出计划模式".to_string(),
                        detail: String::new(),
                    });
                }
                HarnessEvent::GoalSet { objective } => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "goal".to_string(),
                        summary: "设置目标".to_string(),
                        detail: objective,
                    });
                }
                HarnessEvent::GoalUpdate {
                    objective,
                    status,
                    blocked_reason,
                    ..
                } => {
                    let mut detail = objective;
                    if !blocked_reason.is_empty() {
                        detail = format!("{}\n阻塞原因：{}", detail, blocked_reason);
                    }
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "goal".to_string(),
                        summary: format!("目标状态：{}", status),
                        detail,
                    });
                }
                HarnessEvent::WorkflowRun {
                    name,
                    stage,
                    total,
                    output,
                    ..
                } => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "workflow".to_string(),
                        summary: format!("工作流「{}」阶段 {}/{}", name, stage + 1, total),
                        detail: output,
                    });
                }
                HarnessEvent::AttachmentAdded { meta } => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "attachment".to_string(),
                        summary: format!("添加附件：{}", meta.name),
                        detail: meta.path,
                    });
                }
                HarnessEvent::Compaction {
                    removed_messages,
                    summary,
                } => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "compaction".to_string(),
                        summary: format!("上下文已压缩 · 移除 {} 条消息", removed_messages),
                        detail: summary,
                    });
                }
                HarnessEvent::SessionForked {
                    source,
                    boundary_seq,
                } => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "fork".to_string(),
                        summary: format!("分叉自 {}（边界 seq {}）", source, boundary_seq),
                        detail: String::new(),
                    });
                }
                HarnessEvent::SessionTitle { title } => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "title".to_string(),
                        summary: format!("会话标题：{}", title),
                        detail: String::new(),
                    });
                }
                HarnessEvent::RoleSet { name, prompt } => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "role".to_string(),
                        summary: format!("注入 AI 角色：{}", name),
                        detail: prompt,
                    });
                }
                HarnessEvent::SessionCleared => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "cleared".to_string(),
                        summary: "聊天记录已清空".to_string(),
                        detail: String::new(),
                    });
                }
                HarnessEvent::ContextInjected { files } => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "context".to_string(),
                        summary: format!("上下文注入 · {} 个指令文件", files.len()),
                        detail: files.join("\n"),
                    });
                }
                HarnessEvent::SkillInjected { skills } => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "skill".to_string(),
                        summary: format!("注入技能 · {} 个", skills.len()),
                        detail: skills.join("\n"),
                    });
                }
                HarnessEvent::SubagentReported { child, content } => {
                    entries.push(TrajectoryEntry::System {
                        seq,
                        time,
                        event: "subagent_report".to_string(),
                        summary: format!("子代理 {child} 报告"),
                        detail: content,
                    });
                }
            }
        }
        // 收尾：未闭合的助手回合/工具调用如实入账（中断回合可回放）
        flush_tools(
            &mut entries,
            &mut pending_tools,
            &mut tool_results,
            &mut tool_call_count,
        );
        flush_assistant(&mut entries, &mut cur_assistant);
        Ok(HarnessTrajectory {
            entries,
            turn_count: turn,
            tool_call_count,
        })
    }

    /// 回合产物文件（DSH ProducedFiles 迁移）：从工具日志提取变更类工具
    /// （edit_file / write_file / str_replace_editor 的变更命令）成功时的
    /// path 参数，按首次出现顺序去重。str_replace_editor 仅 view（只读）
    /// 不产生产物（DSH 渲染意图语义：变更命令 create/str_replace/insert
    /// 才识别）。
    pub fn turn_files(&self, session_id: &str) -> Result<Vec<TurnFileView>, String> {
        let events = self.events(session_id, 0)?;
        // call id → (name, args JSON 字符串)
        let mut calls: std::collections::HashMap<String, (String, String)> =
            std::collections::HashMap::new();
        let mut files: Vec<TurnFileView> = Vec::new();
        for (seq, ev) in events {
            match ev {
                HarnessEvent::AssistantToolCalls { calls: cs, .. } => {
                    for c in cs {
                        calls.insert(c.id.clone(), (c.name, c.arguments));
                    }
                }
                HarnessEvent::ToolResult { id, ok, .. } => {
                    if !ok {
                        continue;
                    }
                    let Some((name, args)) = calls.get(&id) else {
                        continue;
                    };
                    let mut produces = matches!(name.as_str(), "edit_file" | "write_file");
                    if name == "str_replace_editor" {
                        // 变更命令才产生产物（view 只读）
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                            let cmd = v.get("command").and_then(|c| c.as_str()).unwrap_or("");
                            produces = matches!(cmd, "create" | "str_replace" | "insert");
                        }
                    }
                    if !produces {
                        continue;
                    }
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                        if let Some(p) = v.get("path").and_then(|x| x.as_str()) {
                            if !p.is_empty() {
                                files.push(TurnFileView {
                                    path: p.to_string(),
                                    seq,
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        // 按路径去重，保留首次出现顺序（同一文件多轮编辑只列一次）
        let mut seen = std::collections::HashSet::new();
        files.retain(|f| seen.insert(f.path.clone()));
        Ok(files)
    }

    /// 会话运行状态（plan 模式 / 目标 / 待办）——从日志投影，重建即恢复
    pub fn session_state(&self, session_id: &str) -> Result<SessionState, String> {
        let events = self.events(session_id, 0)?;
        let mut state = SessionState::default();
        for (_seq, ev) in events {
            match ev {
                HarnessEvent::PlanEnter { plan } => {
                    state.plan_mode = true;
                    state.plan_text = plan;
                }
                HarnessEvent::PlanExit => {
                    state.plan_mode = false;
                    state.plan_text = String::new();
                }
                HarnessEvent::GoalSet { objective } => {
                    // 仅设定目标文本与激活状态；revision 由 GoalUpdate 计数
                    // （goal_create 会追加 GoalSet+GoalUpdate 各一条，若 GoalSet
                    // 也递增会造成轮次预算双计数，max_goal_rounds 实际减半）
                    state.goal = objective;
                    state.goal_status = "active".to_string();
                }
                HarnessEvent::GoalUpdate {
                    objective,
                    status,
                    blocked_reason,
                    max_goal_rounds,
                } => {
                    state.goal = objective;
                    state.goal_status = status;
                    state.goal_blocked_reason = blocked_reason;
                    state.goal_max_rounds = max_goal_rounds;
                    state.goal_revision += 1;
                }
                HarnessEvent::TodoUpdate { items } => state.todos = items,
                _ => {}
            }
        }
        Ok(state)
    }
}

fn flush_chunks(
    out: &mut Vec<DisplayMessage>,
    cur: &mut Option<(String, String, i64)>,
    pending_tools: &mut Vec<ToolStepView>,
) {
    if let Some((_id, text, seq)) = cur.take() {
        out.push(DisplayMessage::Assistant {
            content: text,
            seq,
            tools: std::mem::take(pending_tools),
            reasoning: None,
        });
    }
}

/// 转写导出用的分块冲洗
fn flush_md(out: &mut String, cur: &mut Option<(String, String)>) {
    if let Some((_id, text)) = cur.take() {
        out.push_str(&format!("## 助手\n\n{}\n\n", text));
    }
}

fn truncate_chars(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        s.to_string()
    } else {
        format!("{}…", chars[..n].iter().collect::<String>())
    }
}

fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

// ─── IPC ───

/// 从运行时注册表取会话存储服务（lib.rs 启动时经 harness::init 注册）
fn store() -> Result<std::sync::Arc<SessionStore>, String> {
    crate::harness::registry::get::<SessionStore>("harness.sessions")
        .ok_or_else(|| "Harness 运行时未初始化".to_string())
}

#[tauri::command]
pub async fn harness_list_sessions() -> Result<Vec<HarnessSessionMeta>, String> {
    store()?.list()
}

/// 创建会话；workspace_id 为空 = 默认工作区（DSH 工作区浏览器）
#[tauri::command]
pub async fn harness_create_session(
    workspace_id: Option<String>,
) -> Result<HarnessSessionMeta, String> {
    let ws = workspace_id.unwrap_or_default();
    if ws.is_empty() {
        store()?.create()
    } else {
        store()?.create_in_workspace(&ws)
    }
}

/// 设置会话归属工作区（会话在工作区间移动）
#[tauri::command]
pub async fn harness_set_session_workspace(id: String, workspace_id: String) -> Result<(), String> {
    store()?.set_workspace(&id, &workspace_id)
}

/// 设置会话归档标记（DSH workspace.archiveSession：归档/恢复会话）
#[tauri::command]
pub async fn harness_set_session_archived(id: String, archived: bool) -> Result<(), String> {
    store()?.set_archived(&id, archived)
}

/// 设置会话手动排序序号（DSH 拖拽排序：交换双方各写一次）
#[tauri::command]
pub async fn harness_set_session_order(id: String, order_index: i64) -> Result<(), String> {
    store()?.set_order(&id, order_index)
}

/// 交换两个会话的手动排序序号（DSH 拖拽排序：前端拖放即交换）
#[tauri::command]
pub async fn harness_swap_session_order(a: String, b: String) -> Result<(), String> {
    store()?.swap_order(&a, &b)
}

/// B19：LLM 生成会话标题（IPC 与 SDK 共用）。取最近约一轮对话调模型生成
/// 简洁中文标题（≤12 字）并重命名会话；无提供方/模型时优雅报错。
pub async fn generate_title_for(session_id: &str) -> Result<String, String> {
    let s = store()?;
    let msgs = s.derive_model_messages(session_id)?;
    if msgs.is_empty() {
        return Err("会话暂无消息，无法生成标题".to_string());
    }
    // 取最近 6 条（约 1-2 轮）做输入
    let recent: Vec<&serde_json::Value> = msgs
        .iter()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let transcript = recent
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
    let (provider, model) = crate::harness::agent::resolve_provider_model(None, None)?;
    let prompt = format!(
        "为以下对话生成一个简洁的中文会话标题（不超过 12 字，直接输出标题本身，不要引号、不要解释）：\n{}",
        truncate_chars(&transcript, 800)
    );
    let content = crate::llm::client::chat_completion_with_tools_raw(
        &provider,
        &model,
        &[serde_json::json!({ "role": "user", "content": prompt })],
        None,
        None,
        None,
        None,
        None,
        &serde_json::json!([]),
        "none",
    )
    .await
    .map_err(|e| format!("标题生成失败: {}", e))?
    .content;
    let title = truncate_chars(content.trim().trim_matches('"').trim(), 40);
    if title.is_empty() {
        return Err("标题生成为空".to_string());
    }
    s.db.rename_harness_session(session_id, &title)
        .map_err(|e| format!("更新标题失败: {}", e))?;
    Ok(title)
}

/// B19：LLM 生成会话标题（手动触发）。取最近约一轮对话调模型生成
/// 简洁中文标题（≤12 字）并重命名会话；无提供方/模型时优雅报错。
/// 仅手动触发消耗额度（首条消息投影仍为默认标题来源）。
#[tauri::command]
pub async fn harness_generate_title(session_id: String) -> Result<String, String> {
    generate_title_for(&session_id).await
}

/// 会话祖先链（DSH 会话头面包屑：SessionForked 溯源逐级向上，近→远；
/// 返回 (id, title) 列表，当前会话由前端提供）
#[tauri::command]
pub async fn harness_session_lineage(id: String) -> Result<Vec<(String, String)>, String> {
    let s = store()?;
    let mut chain = Vec::new();
    let mut current = id;
    for _ in 0..16 {
        // fork 复制事件后追加 SessionForked（非首事件）——全量扫描
        let source = s
            .events(&current, 0)?
            .into_iter()
            .find_map(|(_, e)| match e {
                HarnessEvent::SessionForked { source, .. } => Some(source),
                _ => None,
            });
        match source {
            Some(src) => {
                let title = s
                    .list()?
                    .into_iter()
                    .find(|m| m.id == src)
                    .map(|m| m.title)
                    .unwrap_or_default();
                chain.push((src.clone(), title));
                current = src;
            }
            None => break,
        }
    }
    Ok(chain)
}

#[tauri::command]
pub async fn harness_rename_session(id: String, title: String) -> Result<(), String> {
    let s = store()?;
    s.rename(&id, &title)?;
    s.append(&id, &HarnessEvent::SessionTitle { title })
        .map(|_| ())
}

#[tauri::command]
pub async fn harness_delete_session(id: String) -> Result<usize, String> {
    // 联动清理该会话的审批信任（interaction 生命周期）
    crate::harness::approval::clear_trust_for_session(&id);
    store()?.delete(&id)
}

/// 增量读取会话事件（前端用 after_seq 续传，重启后完整回放）
#[tauri::command]
pub async fn harness_session_events(
    id: String,
    after_seq: i64,
) -> Result<Vec<(i64, HarnessEvent)>, String> {
    store()?.events(&id, after_seq)
}

/// UI 投影：日志 → 展示消息（渲染与回放同源，DSH「render from the log」）
#[tauri::command]
pub async fn harness_display_messages(id: String) -> Result<Vec<DisplayMessage>, String> {
    store()?.derive_display_messages(&id)
}

/// 轨迹台账（DSH Trajectory 迁移：「轨迹」标签页数据源，日志投影）
#[tauri::command]
pub async fn harness_trajectory(id: String) -> Result<HarnessTrajectory, String> {
    let started = std::time::Instant::now();
    let out = store()?.trajectory(&id);
    log::info!(
        "[harness] harness_trajectory {} -> {:?}（{}ms）",
        id,
        out.as_ref().map(|t| t.entries.len()).map_err(|_| ()),
        started.elapsed().as_millis()
    );
    out
}

/// 回合产物文件（DSH ProducedFiles 迁移：变更类工具路径，日志投影）
#[tauri::command]
pub async fn harness_turn_files(id: String) -> Result<Vec<TurnFileView>, String> {
    store()?.turn_files(&id)
}

/// 会话用量聚合（telemetry）
#[tauri::command]
pub async fn harness_usage_summary(id: String) -> Result<HarnessUsageSummary, String> {
    store()?.usage_summary(&id)
}

/// 会话运行状态（plan / goal / todo，日志投影）
#[tauri::command]
pub async fn harness_session_state(id: String) -> Result<SessionState, String> {
    store()?.session_state(&id)
}

/// 会话查询（session-query）
#[tauri::command]
pub async fn harness_search_sessions(query: String) -> Result<Vec<SearchHit>, String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    store()?.search(&query)
}

/// 会话分叉：复制源会话 seq <= boundary 的事件到新会话
#[tauri::command]
pub async fn harness_fork_session(
    id: String,
    boundary_seq: i64,
) -> Result<HarnessSessionMeta, String> {
    store()?.fork(&id, boundary_seq)
}

/// 设置会话预设（每会话预设作用域）
#[tauri::command]
pub async fn harness_set_session_preset(id: String, preset_id: String) -> Result<(), String> {
    store()?.set_preset(&id, &preset_id)
}

/// 设置会话级 AI 角色（原「AI 聊天」角色注入迁移；空 prompt = 清除）
#[tauri::command]
pub async fn harness_set_session_role(
    id: String,
    name: String,
    prompt: String,
) -> Result<(), String> {
    store()?.set_role(&id, &name, &prompt)
}

/// 读取会话当前 AI 角色（日志投影）
#[tauri::command]
pub async fn harness_get_session_role(id: String) -> Result<HarnessRoleView, String> {
    let (name, prompt) = store()?.role(&id)?;
    Ok(HarnessRoleView { name, prompt })
}

/// 清空会话聊天记录（维护会话能力：删除事件与用量，保留会话元信息）
#[tauri::command]
pub async fn harness_clear_session(id: String) -> Result<(), String> {
    store()?.clear_messages(&id, "user")
}

/// 会话角色投影视图
#[derive(serde::Serialize, Clone, Debug)]
pub struct HarnessRoleView {
    pub name: String,
    pub prompt: String,
}

/// 转写导出（回放）：Markdown 文本；给定 path 时写入文件并返回路径
#[tauri::command]
pub async fn harness_export_session(id: String, path: Option<String>) -> Result<String, String> {
    let md = store()?.export_markdown(&id)?;
    match path {
        Some(p) if !p.trim().is_empty() => {
            std::fs::write(&p, md).map_err(|e| format!("写入导出文件失败: {e}"))?;
            Ok(p)
        }
        _ => Ok(md),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_store() -> SessionStore {
        // 用内存不可行（db 固定文件），单测直接构造事件流验证投影纯逻辑
        let _ = now_iso();
        SessionStore {
            db: crate::db::Database::new().unwrap(),
        }
    }

    #[test]
    fn event_serde_roundtrip_tagged() {
        let ev = HarnessEvent::AssistantChunk {
            id: "a1".into(),
            delta: "你好".into(),
            done: false,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"assistant_chunk\""));
        let back: HarnessEvent = serde_json::from_str(&json).unwrap();
        match back {
            HarnessEvent::AssistantChunk { delta, .. } => assert_eq!(delta, "你好"),
            _ => panic!("类型判别失败"),
        }
    }

    #[test]
    fn session_create_list_delete_roundtrip() {
        let s = test_store();
        let meta = s.create().unwrap();
        assert!(meta.id.starts_with("h-"));
        assert!(meta.title.is_empty());
        let before = s.list().unwrap().len();
        assert!(s.delete(&meta.id).unwrap() == 1);
        assert!(s.list().unwrap().len() == before - 1);
    }

    #[test]
    fn session_workspace_ownership_roundtrip() {
        let s = test_store();
        // 指定工作区创建
        let meta = s.create_in_workspace("ws-1").unwrap();
        assert_eq!(meta.workspace_id, "ws-1");
        // 列表携带 workspace_id
        let listed = s.list().unwrap();
        let row = listed.iter().find(|m| m.id == meta.id).unwrap();
        assert_eq!(row.workspace_id, "ws-1");
        // 移动到另一工作区
        s.set_workspace(&meta.id, "ws-2").unwrap();
        let listed = s.list().unwrap();
        let row = listed.iter().find(|m| m.id == meta.id).unwrap();
        assert_eq!(row.workspace_id, "ws-2");
        // 默认工作区创建
        let def = s.create().unwrap();
        assert_eq!(def.workspace_id, "");
        // 分叉继承源工作区
        let fork = s.fork(&meta.id, 0).unwrap();
        assert_eq!(fork.workspace_id, "ws-2");
        let _ = s.delete(&meta.id);
        let _ = s.delete(&def.id);
        let _ = s.delete(&fork.id);
    }

    #[test]
    fn session_archive_order_preset_roundtrip() {
        // 会话管理：归档（不删日志可恢复）/ 手动排序（交换）/ 每会话预设
        let s = test_store();
        let a = s.create().unwrap();
        let b = s.create().unwrap();
        // 归档：标记后列表仍含该会话（归档仅隐去，不删除）
        s.set_archived(&a.id, true).unwrap();
        let listed = s.list().unwrap();
        let ra = listed.iter().find(|m| m.id == a.id).unwrap();
        assert!(ra.archived, "归档标记应落库: {ra:?}");
        // 恢复
        s.set_archived(&a.id, false).unwrap();
        let listed = s.list().unwrap();
        let ra = listed.iter().find(|m| m.id == a.id).unwrap();
        assert!(!ra.archived);
        // 手动排序：a 靠前
        s.set_order(&a.id, 1).unwrap();
        s.set_order(&b.id, 2).unwrap();
        let listed = s.list().unwrap();
        let pa = listed.iter().position(|m| m.id == a.id).unwrap();
        let pb = listed.iter().position(|m| m.id == b.id).unwrap();
        assert!(pa < pb, "a 应排在 b 前: {listed:?}");
        // 交换：b 靠前
        s.swap_order(&a.id, &b.id).unwrap();
        let listed = s.list().unwrap();
        let pa = listed.iter().position(|m| m.id == a.id).unwrap();
        let pb = listed.iter().position(|m| m.id == b.id).unwrap();
        assert!(pb < pa, "交换后 b 应排在 a 前: {listed:?}");
        // 每会话预设（空字符串 = 未设置，跟随全局默认）
        assert_eq!(s.preset_id(&a.id).unwrap().as_deref(), Some(""));
        s.set_preset(&a.id, "preset-example-readonly").unwrap();
        assert_eq!(
            s.preset_id(&a.id).unwrap().as_deref(),
            Some("preset-example-readonly")
        );
        // 预设不串会话
        assert_eq!(s.preset_id(&b.id).unwrap().as_deref(), Some(""));
        let _ = s.delete(&a.id);
        let _ = s.delete(&b.id);
    }

    #[test]
    fn derive_display_projects_chunks_and_boundaries() {
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "你好".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantChunk {
                id: "a1".into(),
                delta: "你".into(),
                done: false,
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantChunk {
                id: "a1".into(),
                delta: "好呀".into(),
                done: true,
            },
        )
        .unwrap();
        let msgs = s.derive_display_messages(&meta.id).unwrap();
        assert_eq!(msgs.len(), 2);
        match &msgs[1] {
            DisplayMessage::Assistant { content, .. } => assert_eq!(content, "你好呀"),
            _ => panic!("投影失败"),
        }
        // 标题投影：首条用户消息
        let list = s.list().unwrap();
        assert_eq!(list.iter().find(|m| m.id == meta.id).unwrap().title, "你好");
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn derive_display_attaches_tool_steps_to_reply() {
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "现在几点".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantToolCalls {
                id: "a1".into(),
                calls: vec![ToolCallView {
                    id: "call-1".into(),
                    name: "get_current_time".into(),
                    arguments: "{}".into(),
                }],
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::ToolResult {
                id: "call-1".into(),
                ok: true,
                result: "2026-08-17".into(),
                duration_ms: 5,
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantMessage {
                id: "a1".into(),
                content: "现在是 2026-08-17".into(),
                reasoning: None,
            },
        )
        .unwrap();
        let msgs = s.derive_display_messages(&meta.id).unwrap();
        match &msgs[1] {
            DisplayMessage::Assistant { tools, .. } => {
                assert_eq!(tools.len(), 1);
                assert_eq!(tools[0].name, "get_current_time");
                assert_eq!(tools[0].status, "ok");
                assert_eq!(tools[0].duration_ms, Some(5));
            }
            _ => panic!("助手回复应携带工具步骤"),
        }
        // 模型上下文投影：user + assistant(tool_calls) + tool + assistant
        let model = s.derive_model_messages(&meta.id).unwrap();
        assert_eq!(model.len(), 4);
        assert_eq!(model[1]["role"], "assistant");
        assert!(model[1]["tool_calls"].is_array());
        assert_eq!(model[2]["role"], "tool");
        assert_eq!(model[2]["tool_call_id"], "call-1");
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn registry_service_provided_once() {
        let s = Arc::new(test_store());
        let _d = crate::harness::registry::provide("harness.sessions.test", s);
        assert!(crate::harness::registry::get::<SessionStore>("harness.sessions.test").is_some());
    }

    #[test]
    fn fork_copies_boundary_and_logs_provenance() {
        let s = test_store();
        let meta = s.create().unwrap();
        let user_seq = s
            .append(
                &meta.id,
                &HarnessEvent::UserMessage {
                    id: "u1".into(),
                    content: "问题一".into(),
                },
            )
            .unwrap();
        let asst_seq = s
            .append(
                &meta.id,
                &HarnessEvent::AssistantMessage {
                    id: "a1".into(),
                    content: "回答一".into(),
                    reasoning: None,
                },
            )
            .unwrap();
        // 在用户消息边界分叉：只复制用户消息
        let child = s.fork(&meta.id, user_seq).unwrap();
        assert_ne!(child.id, meta.id);
        assert!(child.title.contains("分叉"));
        let events = s.events(&child.id, 0).unwrap();
        assert_eq!(events.len(), 2); // 用户消息 + SessionForked 溯源事件
        assert!(matches!(events[0].1, HarnessEvent::UserMessage { .. }));
        match &events[1].1 {
            HarnessEvent::SessionForked {
                source,
                boundary_seq,
            } => {
                assert_eq!(source, &meta.id);
                assert_eq!(*boundary_seq, user_seq);
            }
            _ => panic!("分叉溯源事件缺失"),
        }
        // 展示投影：仅一条用户消息且携带 seq
        let msgs = s.derive_display_messages(&child.id).unwrap();
        assert_eq!(msgs.len(), 1);
        match &msgs[0] {
            DisplayMessage::User { seq, .. } => assert_eq!(*seq, user_seq),
            _ => panic!("投影失败"),
        }
        // 完整边界分叉：用户 + 助手均复制；Markdown 导出含对话
        let child2 = s.fork(&meta.id, asst_seq).unwrap();
        let msgs2 = s.derive_display_messages(&child2.id).unwrap();
        assert_eq!(msgs2.len(), 2);
        let md = s.export_markdown(&child2.id).unwrap();
        assert!(md.contains("问题一") && md.contains("回答一"));
        let _ = s.delete(&meta.id);
        let _ = s.delete(&child.id);
        let _ = s.delete(&child2.id);
    }

    #[test]
    fn fork_descendants_found_via_full_log_scan() {
        // 回归：fork 复制事件后追加 SessionForked（不一定是首事件），
        // 后代判定必须全量扫描事件日志（catalog/trace 同源）
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "问题一".into(),
            },
        )
        .unwrap();
        let child = s.fork(&meta.id, 1).unwrap();
        // 子会话首事件是复制的 user_message，SessionForked 在末尾
        let events = s.events(&child.id, 0).unwrap();
        assert!(matches!(events[0].1, HarnessEvent::UserMessage { .. }));
        assert!(matches!(
            events.last().unwrap().1,
            HarnessEvent::SessionForked { .. }
        ));
        // trace 后代：全量扫描能找到子会话
        let t = s.trace(&meta.id).unwrap();
        assert!(
            t.descendants.contains(&child.id),
            "descendants: {:?}",
            t.descendants
        );
        // 祖先链：子会话能溯源到父（fork 后 SessionForked 非首事件）
        let t2 = s.trace(&child.id).unwrap();
        assert_eq!(t2.ancestors, vec![meta.id.clone()]);
        let _ = s.delete(&child.id);
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn derive_display_projects_meta_lines_for_compaction_and_role() {
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "问题一".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::RoleSet {
                name: "测试角色".into(),
                prompt: "你是测试角色".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::Compaction {
                removed_messages: 12,
                summary: "早期对话已压缩为摘要".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u2".into(),
                content: "问题二".into(),
            },
        )
        .unwrap();
        let msgs = s.derive_display_messages(&meta.id).unwrap();
        // 用户1 + 角色行 + 压缩行 + 用户2 = 4 行
        assert_eq!(msgs.len(), 4);
        match &msgs[1] {
            DisplayMessage::MetaLine {
                kind,
                title,
                detail,
                workflow,
            } => {
                assert_eq!(kind, "role");
                assert!(title.contains("测试角色"));
                assert!(detail.contains("测试角色"));
                assert!(workflow.is_none());
            }
            _ => panic!("角色注入行缺失"),
        }
        match &msgs[2] {
            DisplayMessage::MetaLine {
                kind,
                title,
                detail,
                ..
            } => {
                assert_eq!(kind, "compaction");
                assert!(title.contains("12"));
                assert!(detail.contains("摘要"));
            }
            _ => panic!("压缩行缺失"),
        }
        // 序列化：role 判别为 "meta"，UI 类型可对齐
        let json = serde_json::to_value(&msgs[1]).unwrap();
        assert_eq!(json["role"], "meta");
        assert_eq!(json["kind"], "role");
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn trajectory_projects_turns_tools_and_system() {
        let s = test_store();
        let meta = s.create().unwrap();
        // 轮 1：用户 → 工具（成功）→ 助手
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "读取文件".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantToolCalls {
                id: "a1".into(),
                calls: vec![ToolCallView {
                    id: "call-1".into(),
                    name: "read_file".into(),
                    arguments: r#"{"path":"a.txt"}"#.into(),
                }],
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::ToolResult {
                id: "call-1".into(),
                ok: true,
                result: "内容".into(),
                duration_ms: 3,
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantMessage {
                id: "a1".into(),
                content: "已读取".into(),
                reasoning: None,
            },
        )
        .unwrap();
        // 轮 2 前：目标与压缩系统事件
        s.append(
            &meta.id,
            &HarnessEvent::GoalSet {
                objective: "完成迁移".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::Compaction {
                removed_messages: 5,
                summary: "摘要".into(),
            },
        )
        .unwrap();
        // 轮 2：用户 → 中断的工具调用（无结果，未闭合）
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u2".into(),
                content: "运行命令".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantToolCalls {
                id: "a2".into(),
                calls: vec![ToolCallView {
                    id: "call-2".into(),
                    name: "exec_command".into(),
                    arguments: r#"{"command":"dir"}"#.into(),
                }],
            },
        )
        .unwrap();
        let t = s.trajectory(&meta.id).unwrap();
        // 轮1(用户+工具+助手) + 系统2(目标+压缩) + 轮2(用户+未闭合工具) = 7 条
        assert_eq!(t.entries.len(), 7, "entries: {:?}", t.entries);
        assert_eq!(t.turn_count, 2, "entries: {:?}", t.entries);
        // L8：工具调用次数统计全部调用（含未闭合/失败）——2 次
        assert_eq!(t.tool_call_count, 2, "entries: {:?}", t.entries);
        // 顺序断言：工具条目先于其助手回复
        let kinds: Vec<&str> = t
            .entries
            .iter()
            .map(|e| match e {
                TrajectoryEntry::User { .. } => "user",
                TrajectoryEntry::Assistant { .. } => "assistant",
                TrajectoryEntry::Tool { .. } => "tool",
                TrajectoryEntry::System { .. } => "system",
            })
            .collect();
        assert_eq!(&kinds[0..3], &["user", "tool", "assistant"]);
        // 未闭合工具：ok=false
        match &t.entries[6] {
            TrajectoryEntry::Tool { ok, name, .. } => {
                assert!(!ok);
                assert_eq!(name, "exec_command");
            }
            other => panic!("期望工具条目，得到 {:?}", other),
        }
        // 助手条目携带轮次与工具统计（轮号从 1 起：第一条用户消息所在轮 = 1）
        match &t.entries[2] {
            TrajectoryEntry::Assistant {
                turn,
                steps,
                tool_calls,
                ..
            } => {
                assert_eq!(*turn, 1, "entries: {:?}", t.entries);
                assert_eq!(*steps, 1, "entries: {:?}", t.entries);
                assert_eq!(*tool_calls, 1, "entries: {:?}", t.entries);
            }
            other => panic!("期望助手条目，得到 {:?}", other),
        }
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn derive_display_projects_reasoning_with_reply() {
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "推理题".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantMessage {
                id: "a1".into(),
                content: "答案是 42".into(),
                reasoning: Some("逐步思考：1+1=2…".into()),
            },
        )
        .unwrap();
        let msgs = s.derive_display_messages(&meta.id).unwrap();
        match &msgs[1] {
            DisplayMessage::Assistant {
                content, reasoning, ..
            } => {
                assert_eq!(content, "答案是 42");
                assert_eq!(reasoning.as_deref(), Some("逐步思考：1+1=2…"));
            }
            _ => panic!("投影失败"),
        }
        // 事件序列化：reasoning 字段随 payload 持久化（回放同源）
        let events = s.events(&meta.id, 0).unwrap();
        let json = serde_json::to_value(&events[1].1).unwrap();
        assert_eq!(json["reasoning"], "逐步思考：1+1=2…");
        // 旧日志（无 reasoning 字段）反序列化兼容
        let legacy = serde_json::json!({
            "type": "assistant_message",
            "id": "a2",
            "content": "旧消息"
        });
        let ev: HarnessEvent = serde_json::from_value(legacy).unwrap();
        match ev {
            HarnessEvent::AssistantMessage { reasoning, .. } => assert!(reasoning.is_none()),
            _ => panic!("判别失败"),
        }
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn derive_display_projects_context_injection_row() {
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::ContextInjected {
                files: vec!["AGENTS.md".into(), "sub/CLAUDE.md".into()],
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "问题".into(),
            },
        )
        .unwrap();
        let msgs = s.derive_display_messages(&meta.id).unwrap();
        match &msgs[0] {
            DisplayMessage::MetaLine {
                kind,
                title,
                detail,
                ..
            } => {
                assert_eq!(kind, "context");
                assert!(title.contains("2"));
                assert!(detail.contains("AGENTS.md"));
            }
            _ => panic!("上下文注入行缺失"),
        }
        // 轨迹台账同步入账
        let t = s.trajectory(&meta.id).unwrap();
        match &t.entries[0] {
            TrajectoryEntry::System { event, summary, .. } => {
                assert_eq!(event, "context");
                assert!(summary.contains("2"));
            }
            _ => panic!("轨迹上下文条目缺失"),
        }
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn event_read_and_event_search_work() {
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "今天天气怎么样".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantMessage {
                id: "a1".into(),
                content: "今天晴天".into(),
                reasoning: None,
            },
        )
        .unwrap();
        // event_read：按 seq 定位完整事件
        let ev = s.event_read(&meta.id, 1).unwrap().unwrap();
        assert_eq!(ev.0, 1);
        assert!(matches!(ev.1, HarnessEvent::UserMessage { .. }));
        assert!(s.event_read(&meta.id, 99).unwrap().is_none());
        // event_search：关键词命中 + 片段
        let hits = s.event_search(&meta.id, "天气").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, 1);
        assert_eq!(hits[0].1, "user_message");
        assert!(hits[0].2.contains("天气"));
        assert!(s.event_search(&meta.id, "").unwrap().is_empty());
        assert!(s.event_search(&meta.id, "不存在的词").unwrap().is_empty());
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn event_trace_reports_sources_and_derived() {
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "开始".into(),
            },
        )
        .unwrap();
        // 目标事件：分叉边界指向 seq 1（来源引用）
        s.append(
            &meta.id,
            &HarnessEvent::SessionForked {
                source: "parent-s".into(),
                boundary_seq: 1,
            },
        )
        .unwrap();
        // 派生事件：另一条分叉以 seq 2 为边界（引用目标）
        s.append(
            &meta.id,
            &HarnessEvent::SessionForked {
                source: "child-s".into(),
                boundary_seq: 2,
            },
        )
        .unwrap();
        let t = s.event_trace(&meta.id, 2).unwrap();
        assert_eq!(t.target_seq, 2);
        assert_eq!(t.target_type, "session_forked");
        // 追加式日志：替换面恒空
        assert!(t.replaced_by.is_none());
        assert!(t.replacement_chain.is_empty());
        assert!(t.replaced_event_seqs.is_empty());
        // 关系面：目标引用 seq 1，被 seq 3 的分叉引用
        assert_eq!(t.source_event_seqs, vec![1]);
        assert_eq!(t.derived_event_seqs, vec![3]);
        // 不存在的 seq 报错
        assert!(s.event_trace(&meta.id, 99).is_err());
        let _ = s.delete(&meta.id);
    }

    #[test]
    #[ignore = "真实数据库诊断用（需应用数据目录存在）"]
    fn real_db_trajectory_smoke() {
        let s = SessionStore {
            db: crate::db::Database::new().unwrap(),
        };
        let list = s.list().unwrap();
        eprintln!("real sessions: {}", list.len());
        for m in list.iter().take(5) {
            let t = s.trajectory(&m.id).unwrap();
            eprintln!(
                "session {} title={:?} entries {}",
                m.id,
                m.title,
                t.entries.len()
            );
            let events = s.events(&m.id, 0).unwrap();
            for (seq, ev) in events.iter().take(40) {
                let kind = match ev {
                    HarnessEvent::UserMessage { .. } => "user",
                    HarnessEvent::AssistantChunk { .. } => "chunk",
                    HarnessEvent::AssistantMessage { .. } => "assistant",
                    HarnessEvent::AssistantToolCalls { .. } => "tool_calls",
                    HarnessEvent::ToolResult { .. } => "tool_result",
                    HarnessEvent::TodoUpdate { .. } => "todo",
                    HarnessEvent::PlanEnter { .. } => "plan_enter",
                    HarnessEvent::PlanExit => "plan_exit",
                    HarnessEvent::GoalSet { .. } => "goal_set",
                    HarnessEvent::GoalUpdate { .. } => "goal_update",
                    HarnessEvent::WorkflowRun { .. } => "workflow",
                    HarnessEvent::AttachmentAdded { .. } => "attachment",
                    HarnessEvent::Compaction { .. } => "compaction",
                    HarnessEvent::SessionForked { .. } => "forked",
                    HarnessEvent::SessionTitle { .. } => "title",
                    HarnessEvent::RoleSet { .. } => "role",
                    HarnessEvent::SessionCleared => "cleared",
                    HarnessEvent::ContextInjected { .. } => "context",
                    HarnessEvent::SkillInjected { .. } => "skill",
                    HarnessEvent::SubagentReported { .. } => "subagent_report",
                };
                eprintln!("  #{seq} {kind}");
            }
        }
    }

    #[test]
    fn turn_files_projects_mutated_paths_deduped() {
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "改文件".into(),
            },
        )
        .unwrap();
        // 第一次 edit_file 成功 → 产物
        s.append(
            &meta.id,
            &HarnessEvent::AssistantToolCalls {
                id: "a1".into(),
                calls: vec![ToolCallView {
                    id: "c1".into(),
                    name: "edit_file".into(),
                    arguments: r#"{"path":"src/lib/a.ts","old_string":"x","new_string":"y"}"#
                        .into(),
                }],
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::ToolResult {
                id: "c1".into(),
                ok: true,
                result: "已替换".into(),
                duration_ms: 1,
            },
        )
        .unwrap();
        // 同一文件第二次编辑 → 去重
        s.append(
            &meta.id,
            &HarnessEvent::AssistantToolCalls {
                id: "a2".into(),
                calls: vec![ToolCallView {
                    id: "c2".into(),
                    name: "edit_file".into(),
                    arguments: r#"{"path":"src/lib/a.ts","old_string":"y","new_string":"z"}"#
                        .into(),
                }],
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::ToolResult {
                id: "c2".into(),
                ok: true,
                result: "已替换".into(),
                duration_ms: 1,
            },
        )
        .unwrap();
        // 失败的工具不产生产物
        s.append(
            &meta.id,
            &HarnessEvent::AssistantToolCalls {
                id: "a3".into(),
                calls: vec![ToolCallView {
                    id: "c3".into(),
                    name: "write_file".into(),
                    arguments: r#"{"path":"b.txt","content":"x"}"#.into(),
                }],
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::ToolResult {
                id: "c3".into(),
                ok: false,
                result: "拒绝".into(),
                duration_ms: 1,
            },
        )
        .unwrap();
        // 只读工具不产生产物
        s.append(
            &meta.id,
            &HarnessEvent::AssistantToolCalls {
                id: "a4".into(),
                calls: vec![ToolCallView {
                    id: "c4".into(),
                    name: "read_file".into(),
                    arguments: r#"{"path":"c.txt"}"#.into(),
                }],
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::ToolResult {
                id: "c4".into(),
                ok: true,
                result: "x".into(),
                duration_ms: 1,
            },
        )
        .unwrap();
        let files = s.turn_files(&meta.id).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/lib/a.ts");
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn turn_files_recognizes_str_replace_editor_mutations() {
        // DSH 渲染意图语义：str_replace_editor 的变更命令（create/
        // str_replace/insert）识别为产物；view（只读）不识别
        let s = test_store();
        let meta = s.create().unwrap();
        // 一次性追加 3 个变更 + 1 个只读 view
        let mk = |id: &str, cmd: &str, path: &str| {
            (
                HarnessEvent::AssistantToolCalls {
                    id: format!("a-{id}").into(),
                    calls: vec![ToolCallView {
                        id: format!("c-{id}").into(),
                        name: "str_replace_editor".into(),
                        arguments: format!(r#"{{"command":"{cmd}","path":"{path}"}}"#),
                    }],
                },
                HarnessEvent::ToolResult {
                    id: format!("c-{id}").into(),
                    ok: true,
                    result: "ok".into(),
                    duration_ms: 1,
                },
            )
        };
        for (call, result) in [
            mk("1", "create", "new.md"),
            mk("2", "str_replace", "edit.md"),
            mk("3", "insert", "ins.md"),
            mk("4", "view", "readonly.md"), // 只读不产生产物
        ] {
            s.append(&meta.id, &call).unwrap();
            s.append(&meta.id, &result).unwrap();
        }
        let files = s.turn_files(&meta.id).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["new.md", "edit.md", "ins.md"],
            "变更命令识别、view 排除: {paths:?}"
        );
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn skill_injected_event_roundtrips_and_projects_meta_line() {
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::SkillInjected {
                skills: vec!["sk-1".into(), "sk-2".into()],
            },
        )
        .unwrap();
        // 事件持久化可读回（模型可见 ⟺ 落日志）
        let evs = s.events(&meta.id, 0).unwrap();
        assert!(matches!(
            &evs[0].1,
            HarnessEvent::SkillInjected { skills }
                if skills == &vec!["sk-1".to_string(), "sk-2".to_string()]
        ));
        // UI 投影为 meta 行（渲染与回放同源）
        let msgs = s.derive_display_messages(&meta.id).unwrap();
        assert!(matches!(
            &msgs[0],
            DisplayMessage::MetaLine { kind, title, detail, .. }
                if kind == "skill" && title.contains('2') && detail.contains("sk-1")
        ));
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn subagent_report_event_roundtrips_and_projects() {
        // DSH tool-subagent-report 迁移：SubagentReported 落父会话日志，
        // 模型投影为用户消息（父代理可见），UI 投影为「子代理报告」meta 行
        let s = test_store();
        let meta = s.create().unwrap();
        let seq = s
            .append(
                &meta.id,
                &HarnessEvent::SubagentReported {
                    child: "child-1".into(),
                    content: "任务完成：输出已写入 src/out.rs".into(),
                },
            )
            .unwrap();
        assert!(seq > 0);
        // 事件持久化可读回
        let evs = s.events(&meta.id, 0).unwrap();
        assert!(matches!(
            &evs[0].1,
            HarnessEvent::SubagentReported { child, content }
                if child == "child-1" && content.contains("src/out.rs")
        ));
        // 模型投影：user 消息，内容含来源与正文（模型可见 ⟺ 落日志）
        let model = s.derive_model_messages(&meta.id).unwrap();
        assert_eq!(model.len(), 1);
        assert_eq!(model[0]["role"], "user");
        let text = model[0]["content"].as_str().unwrap();
        assert!(text.contains("child-1") && text.contains("src/out.rs"));
        // UI 投影：meta 行 kind=subagent
        let msgs = s.derive_display_messages(&meta.id).unwrap();
        assert!(matches!(
            &msgs[0],
            DisplayMessage::MetaLine { kind, title, detail, .. }
                if kind == "subagent" && title.contains("child-1") && detail.contains("src/out.rs")
        ));
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn derive_model_messages_strips_trailing_unclosed_tool_calls() {
        // 崩溃/中断残留：assistant tool_calls 无对应 tool 结果 → 投影必须剥离，
        // 否则模型 API 400（与 fork clean_boundary 同理；投影层剥离、日志保留）
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "跑一下".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantToolCalls {
                id: "as1".into(),
                calls: vec![ToolCallView {
                    id: "c1".into(),
                    name: "exec_command".into(),
                    arguments: "{}".into(),
                }],
            },
        )
        .unwrap();
        let model = s.derive_model_messages(&meta.id).unwrap();
        assert_eq!(model.len(), 1, "未闭合 tool_calls 应从投影剥离: {model:?}");
        assert_eq!(model[0]["role"], "user");
        // 日志保留（模型可见 ⟺ 落日志：审计可恢复）
        let evs = s.events(&meta.id, 0).unwrap();
        assert!(evs
            .iter()
            .any(|(_, e)| matches!(e, HarnessEvent::AssistantToolCalls { .. })));
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn derive_model_messages_strips_partially_resolved_tool_round() {
        // 部分结果（多调用中仅一个落结果）也视为未闭合：整轮从投影剥离
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "跑一下".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantToolCalls {
                id: "as1".into(),
                calls: vec![
                    ToolCallView {
                        id: "c1".into(),
                        name: "a".into(),
                        arguments: "{}".into(),
                    },
                    ToolCallView {
                        id: "c2".into(),
                        name: "b".into(),
                        arguments: "{}".into(),
                    },
                ],
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::ToolResult {
                id: "c1".into(),
                ok: true,
                result: "r1".into(),
                duration_ms: 5,
            },
        )
        .unwrap();
        let model = s.derive_model_messages(&meta.id).unwrap();
        assert_eq!(model.len(), 1, "部分闭合的工具轮应从投影剥离: {model:?}");
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn derive_model_messages_strips_orphan_tool_results() {
        // 孤儿 tool 结果（tool_call_id 无前置引用）对模型 API 恒为非法：
        // 投影剥离、日志保留
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "跑一下".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantToolCalls {
                id: "as1".into(),
                calls: vec![ToolCallView {
                    id: "c1".into(),
                    name: "exec_command".into(),
                    arguments: "{}".into(),
                }],
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::ToolResult {
                id: "c1".into(),
                ok: true,
                result: "ok".into(),
                duration_ms: 5,
            },
        )
        .unwrap();
        // 孤儿结果：id 无对应调用
        s.append(
            &meta.id,
            &HarnessEvent::ToolResult {
                id: "ghost".into(),
                ok: true,
                result: "ghost-result".into(),
                duration_ms: 1,
            },
        )
        .unwrap();
        let model = s.derive_model_messages(&meta.id).unwrap();
        assert_eq!(model.len(), 3, "孤儿 tool 结果应从投影剥离: {model:?}");
        assert!(!model
            .iter()
            .any(|m| m.get("tool_call_id").and_then(|i| i.as_str()) == Some("ghost")));
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn derive_model_messages_keeps_closed_tool_rounds() {
        // 正常闭合工具轮（调用 + 全部结果 + 回复）不被剥离
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "跑一下".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantToolCalls {
                id: "as1".into(),
                calls: vec![ToolCallView {
                    id: "c1".into(),
                    name: "exec_command".into(),
                    arguments: "{}".into(),
                }],
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::ToolResult {
                id: "c1".into(),
                ok: true,
                result: "ok".into(),
                duration_ms: 5,
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantMessage {
                id: "as2".into(),
                content: "完成".into(),
                reasoning: None,
            },
        )
        .unwrap();
        let model = s.derive_model_messages(&meta.id).unwrap();
        assert_eq!(model.len(), 4, "闭合工具轮应完整保留: {model:?}");
        assert_eq!(model[3]["role"], "assistant");
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn derive_model_messages_passes_back_reasoning_content() {
        // DSH 2026-08-19 deepseek-reasoning-passback：每个含推理的助手轮次
        // 都回传 reasoning_content（含无工具调用的纯作答轮次）
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "分析问题".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantMessage {
                id: "as1".into(),
                content: "结论".into(),
                reasoning: Some("推理过程".into()),
            },
        )
        .unwrap();
        let model = s.derive_model_messages(&meta.id).unwrap();
        assert_eq!(model[1]["role"], "assistant");
        assert_eq!(
            model[1]["reasoning_content"].as_str(),
            Some("推理过程"),
            "推理应回传: {}",
            model[1]
        );
        // 无推理的助手消息不携带该字段
        s.append(
            &meta.id,
            &HarnessEvent::AssistantMessage {
                id: "as2".into(),
                content: "普通回复".into(),
                reasoning: None,
            },
        )
        .unwrap();
        let model2 = s.derive_model_messages(&meta.id).unwrap();
        assert!(model2[2].get("reasoning_content").is_none());
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn compaction_folds_history_in_model_messages() {
        // H4 回归：压缩事件之前的全部历史被摘要占位替换，且只生成一次
        // （后续回合从日志投影即得，不再每回合重复全量压缩）
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "问题一".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantMessage {
                id: "a1".into(),
                content: "回答一".into(),
                reasoning: None,
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::Compaction {
                removed_messages: 2,
                summary: "用户问了第一个问题，助手给出了回答。".into(),
            },
        )
        .unwrap();
        // 压缩后新增的一轮
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u2".into(),
                content: "问题二".into(),
            },
        )
        .unwrap();
        let msgs = s.derive_model_messages(&meta.id).unwrap();
        // 摘要占位 + 新用户消息（旧历史被折叠，不重复出现）
        let contents: Vec<String> = msgs
            .iter()
            .map(|m| {
                m.get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        assert_eq!(msgs.len(), 2, "旧历史应折叠为摘要占位: {contents:?}");
        assert!(contents[0].contains("[较早对话摘要]"));
        assert!(contents[0].contains("第一个问题"));
        assert_eq!(contents[1], "问题二");
        // 被压缩的原始消息不再作为独立消息出现（内容只存在于摘要中）
        assert!(
            !contents.iter().any(|c| c == "回答一" || c == "问题一"),
            "原始消息应被折叠: {contents:?}"
        );
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn chunk_groups_separate_by_assistant_id() {
        // L9 回归：不同 assistant id 的流式分块不得合并成一条文本
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantChunk {
                id: "a1".into(),
                delta: "第一段".into(),
                done: false,
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantChunk {
                id: "a1".into(),
                delta: "续".into(),
                done: false,
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantChunk {
                id: "a2".into(),
                delta: "第二段".into(),
                done: false,
            },
        )
        .unwrap();
        let msgs = s.derive_display_messages(&meta.id).unwrap();
        let contents: Vec<String> = msgs
            .iter()
            .filter_map(|m| match m {
                DisplayMessage::Assistant { content, .. } => Some(content.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            contents.len(),
            2,
            "两个 assistant id 应各自成段: {contents:?}"
        );
        assert_eq!(contents[0], "第一段续");
        assert_eq!(contents[1], "第二段");
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn interrupted_turn_tool_steps_survive_display_projection() {
        // M5 回归：用户 → 工具调用 → 工具结果（无助手回复，回合被中断）
        // 的工具步骤必须在对话投影中呈现（与轨迹/模型上下文一致），
        // 不得在下一个 user_message 处被丢弃
        let s = test_store();
        let meta = s.create().unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "查一下".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::AssistantToolCalls {
                id: "as1".into(),
                calls: vec![ToolCallView {
                    id: "c1".into(),
                    name: "exec_command".into(),
                    arguments: "{}".into(),
                }],
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::ToolResult {
                id: "c1".into(),
                ok: true,
                result: "结果".into(),
                duration_ms: 10,
            },
        )
        .unwrap();
        // 中断：无 assistant_message，直接来下一条用户消息
        s.append(
            &meta.id,
            &HarnessEvent::UserMessage {
                id: "u2".into(),
                content: "继续".into(),
            },
        )
        .unwrap();
        let msgs = s.derive_display_messages(&meta.id).unwrap();
        // user / 中断回合(带工具) / user
        assert_eq!(msgs.len(), 3, "中断回合应保留工具步骤: {msgs:?}");
        match &msgs[1] {
            DisplayMessage::Assistant { tools, content, .. } => {
                assert_eq!(tools.len(), 1, "中断回合工具应挂载");
                assert_eq!(tools[0].name, "exec_command");
                assert!(content.contains("中断"), "应有中断说明: {content}");
            }
            other => panic!("第 2 条应为中断回合助手行: {other:?}"),
        }
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn feedback_submit_and_list_roundtrip() {
        // feedback 能力：会话级 + 消息级（message_seq）反馈往返
        let s = test_store();
        let meta = s.create().unwrap();
        // 会话级 good + 消息级 bad（带评论）
        s.submit_feedback(&meta.id, "good", "", None).unwrap();
        s.submit_feedback(&meta.id, "bad", "回答不准确", Some(3))
            .unwrap();
        let list = s.list_feedback().unwrap();
        let mine: Vec<_> = list.iter().filter(|f| f.session_id == meta.id).collect();
        assert_eq!(mine.len(), 2, "应列出 2 条反馈: {list:?}");
        // 倒序：最新（bad）在前
        assert_eq!(mine[0].rating, "bad");
        assert_eq!(mine[0].comment, "回答不准确");
        assert_eq!(mine[0].message_seq, Some(3));
        assert_eq!(mine[1].rating, "good");
        assert_eq!(mine[1].message_seq, None);
        // 不同会话隔离
        let meta2 = s.create().unwrap();
        s.submit_feedback(&meta2.id, "good", "ok", None).unwrap();
        let list2 = s.list_feedback().unwrap();
        let mine2 = list2.iter().filter(|f| f.session_id == meta2.id).count();
        assert_eq!(mine2, 1, "反馈按会话隔离");
        let _ = s.delete(&meta.id);
        let _ = s.delete(&meta2.id);
    }

    #[test]
    fn session_search_finds_keyword_across_sessions() {
        // B4 session-query：按关键词搜索事件载荷（跨会话命中 + 无结果）
        let s = test_store();
        let a = s.create().unwrap();
        let b = s.create().unwrap();
        s.append(
            &a.id,
            &HarnessEvent::UserMessage {
                id: "u1".into(),
                content: "如何优化数据库查询性能".into(),
            },
        )
        .unwrap();
        s.append(
            &b.id,
            &HarnessEvent::UserMessage {
                id: "u2".into(),
                content: "今天天气如何".into(),
            },
        )
        .unwrap();
        // 命中会话 a（含「数据库」），不含 b
        let hits = s.search("数据库").unwrap();
        assert!(
            hits.iter().any(|h| h.session_id == a.id),
            "应命中会话 a: {hits:?}"
        );
        assert!(
            !hits.iter().any(|h| h.session_id == b.id),
            "不应命中会话 b: {hits:?}"
        );
        // 工具结果也可检索（DSH session-query：tool calls/results 贡献语义文本）
        s.append(
            &a.id,
            &HarnessEvent::ToolResult {
                id: "t1".into(),
                ok: true,
                result: "数据库索引优化建议：覆盖索引".into(),
                duration_ms: 3,
            },
        )
        .unwrap();
        let hits2 = s.search("覆盖索引").unwrap();
        assert!(
            hits2.iter().any(|h| h.session_id == a.id),
            "tool_result 应可检索: {hits2:?}"
        );
        // 片段应包含命中词上下文（长结果中段命中可见）
        assert!(
            hits2
                .iter()
                .find(|h| h.session_id == a.id)
                .map(|h| h.snippet.contains("覆盖索引"))
                .unwrap_or(false),
            "片段应含命中词: {:?}",
            hits2
        );
        // 子代理报告也可检索
        s.append(
            &b.id,
            &HarnessEvent::SubagentReported {
                child: "c1".into(),
                content: "已把数据库迁移脚本写入 migrate.sql".into(),
            },
        )
        .unwrap();
        let hits3 = s.search("migrate.sql").unwrap();
        assert!(
            hits3.iter().any(|h| h.session_id == b.id),
            "subagent_reported 应可检索: {hits3:?}"
        );
        // 无结果
        let none = s.search("不存在的词xyz").unwrap();
        assert!(none.is_empty());
        // 清理
        let _ = s.delete(&a.id);
        let _ = s.delete(&b.id);
    }

    #[test]
    fn usage_summary_aggregates_db_and_event_projection() {
        // usage_summary：db 用量聚合 + 事件日志投影步骤/工具墙钟 + 派生指标
        let s = test_store();
        let meta = s.create().unwrap();
        // 记录一次用量（2 次请求合并的聚合源）
        s.db.append_harness_usage(&crate::db::HarnessUsageRecord {
            session_id: meta.id.clone(),
            provider: "deepseek".into(),
            model: "m".into(),
            reasoning_effort: Some("high".into()),
            prompt_tokens: 1000,
            completion_tokens: 2000,
            cost: 0.5,
            llm_wall_ms: 2000,
            first_token_ms: 400, // 2 次请求 → 平均 200ms
            requests: 2,
            cached_tokens: 500,
            tool_wall_ms: 0,
            created_at: "t".into(),
        })
        .unwrap();
        // 事件：2 个工具结果（步骤 + 工具墙钟投影）
        for (i, dur) in [(1u64, 100u64), (2u64, 200u64)] {
            s.append(
                &meta.id,
                &HarnessEvent::ToolResult {
                    id: format!("c{}", i),
                    ok: true,
                    result: "r".into(),
                    duration_ms: dur,
                },
            )
            .unwrap();
        }
        let u = s.usage_summary(&meta.id).unwrap();
        assert_eq!(u.turns, 1, "1 条用量记录 = 1 轮");
        assert_eq!(u.steps, 2, "事件投影步骤数");
        assert_eq!(u.tool_wall_ms, 300, "事件投影工具墙钟");
        assert_eq!(u.prompt_tokens, 1000);
        assert_eq!(u.completion_tokens, 2000);
        assert_eq!(u.cost, 0.5);
        assert_eq!(u.first_token_avg_ms, 200.0, "400ms/2 请求");
        assert_eq!(u.tokens_per_sec, 1000.0, "2000 tokens / 2s");
        assert_eq!(u.cache_hit_rate, 0.5, "500/1000");
        let _ = s.delete(&meta.id);
    }

    #[test]
    fn session_state_projects_plan_goal_todo() {
        // GoalBar / 计划横幅数据源：plan/goal/todo 状态机投影
        let s = test_store();
        let meta = s.create().unwrap();
        // 目标：GoalSet + 2 次 GoalUpdate → revision=2（GoalSet 不递增）
        s.append(
            &meta.id,
            &HarnessEvent::GoalSet {
                objective: "目标A".into(),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::GoalUpdate {
                objective: "目标A".into(),
                status: "active".into(),
                blocked_reason: String::new(),
                max_goal_rounds: Some(3),
            },
        )
        .unwrap();
        s.append(
            &meta.id,
            &HarnessEvent::GoalUpdate {
                objective: "目标A".into(),
                status: "blocked".into(),
                blocked_reason: "缺密钥".into(),
                max_goal_rounds: Some(3),
            },
        )
        .unwrap();
        let st = s.session_state(&meta.id).unwrap();
        assert_eq!(st.goal, "目标A");
        assert_eq!(st.goal_status, "blocked");
        assert_eq!(st.goal_blocked_reason, "缺密钥");
        assert_eq!(
            st.goal_revision, 2,
            "GoalSet 不递增 revision（防轮次双计数）"
        );
        assert_eq!(st.goal_max_rounds, Some(3));
        // 计划模式进入/退出
        s.append(
            &meta.id,
            &HarnessEvent::PlanEnter {
                plan: "先读后写".into(),
            },
        )
        .unwrap();
        assert!(s.session_state(&meta.id).unwrap().plan_mode);
        assert_eq!(s.session_state(&meta.id).unwrap().plan_text, "先读后写");
        s.append(&meta.id, &HarnessEvent::PlanExit).unwrap();
        let st = s.session_state(&meta.id).unwrap();
        assert!(!st.plan_mode);
        assert!(st.plan_text.is_empty());
        // 待办
        s.append(
            &meta.id,
            &HarnessEvent::TodoUpdate {
                items: vec![TodoItem {
                    id: "t1".into(),
                    content: "事项".into(),
                    status: "in_progress".into(),
                }],
            },
        )
        .unwrap();
        let st = s.session_state(&meta.id).unwrap();
        assert_eq!(st.todos.len(), 1);
        assert_eq!(st.todos[0].status, "in_progress");
        let _ = s.delete(&meta.id);
    }
}
