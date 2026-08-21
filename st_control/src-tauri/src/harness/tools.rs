// ============================================================
// Harness — 工具能力（DSH core/tools + core/system-prompt 迁移）
//
// 对齐 DSH：
// - 作用域工具注册表：全局注册 + 每会话作用域过滤（本阶段：全局作用域；
//   会话作用域在 preset/isolate 阶段接入）
// - 守卫执行管道：tools/pre-execute → 执行 → tools/post-execute。
//   Rust 简化 waterfall：钩子返回 Some(msg) 即短路否决（等价
//   listener 不调 next()），返回 None 放行继续链。
// - prompt 分区组装：注册的 PromptSection 按 order 排序合并为系统提示词，
//   工具 schema 随分区注入模型请求。
// 工具实现复用「AI 聊天」代理模式的内置工具（llm::agent），避免重复；
// 审批门控由本模块的执行管道触发（见 approval.rs）。
// ============================================================

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub use crate::llm::agent::ToolSpec;

/// 工具执行结果
#[derive(Serialize, Clone, Debug)]
pub struct ToolExecOutcome {
    pub ok: bool,
    pub result: String,
    pub duration_ms: u64,
}

/// 守卫钩子：返回 Some(短消息) 短路管道，None 放行
type GuardHook = fn(&str, &Value) -> Option<String>;

/// 工具注册表：工具 + 守卫管道钩子
pub struct ToolRegistry {
    specs: HashMap<String, ToolSpec>,
    pre_hooks: Vec<GuardHook>,
    post_hooks: Vec<GuardHook>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        ToolRegistry {
            specs: HashMap::new(),
            pre_hooks: Vec::new(),
            post_hooks: Vec::new(),
        }
    }

    /// 注册/覆盖一个工具
    pub fn register(&mut self, spec: ToolSpec) {
        self.specs.insert(spec.name.clone(), spec);
    }

    /// 注册 pre-execute 钩子（可多链；返回 Some 即否决执行）。
    /// 扩展接入点：阶段 3+（guard/策略）启用。
    #[allow(dead_code)]
    pub fn add_pre_hook(&mut self, hook: GuardHook) {
        self.pre_hooks.push(hook);
    }

    /// 注册 post-execute 钩子（可多链；返回 Some 即覆盖结果为该消息）。
    /// 扩展接入点：阶段 3+（guard/策略）启用。
    #[allow(dead_code)]
    pub fn add_post_hook(&mut self, hook: GuardHook) {
        self.post_hooks.push(hook);
    }

    pub fn get(&self, name: &str) -> Option<&ToolSpec> {
        self.specs.get(name)
    }

    /// 该工具是否需要审批门控
    pub fn requires_approval(&self, name: &str) -> bool {
        self.get(name).map(|s| s.requires_approval).unwrap_or(false)
    }

    /// 已注册工具名（升序；测试与诊断使用）
    #[allow(dead_code)]
    pub fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.specs.keys().cloned().collect();
        v.sort();
        v
    }

    /// 守卫执行管道（同步；耗时工具由调用方包在阻塞线程池中运行）：
    /// pre 钩子 → 工具执行 → post 钩子
    pub fn execute(
        &self,
        app: Option<&tauri::AppHandle>,
        name: &str,
        args: &Value,
    ) -> ToolExecOutcome {
        let started = std::time::Instant::now();
        for hook in &self.pre_hooks {
            if let Some(reason) = hook(name, args) {
                return ToolExecOutcome {
                    ok: false,
                    result: reason,
                    duration_ms: started.elapsed().as_millis() as u64,
                };
            }
        }
        let (ok, mut result) = match self.specs.get(name) {
            Some(s) => match s.run.call(app.cloned(), args.clone()) {
                Ok(t) => (true, t),
                Err(e) => (false, e),
            },
            None => (false, format!("未知工具: {}", name)),
        };
        for hook in &self.post_hooks {
            if let Some(msg) = hook(name, args) {
                result = msg;
            }
        }
        let duration_ms = started.elapsed().as_millis() as u64;
        if !ok {
            log::warn!(
                "[harness] 工具 {name} 失败（{duration_ms}ms）: {}",
                truncate(&result, 200)
            );
        }
        ToolExecOutcome {
            ok,
            result,
            duration_ms,
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// prompt 分区：order 升序拼接为系统提示词
#[derive(Serialize, Clone, Debug)]
pub struct PromptSection {
    pub order: i32,
    pub title: String,
    pub content: String,
}

fn registry() -> &'static Mutex<ToolRegistry> {
    static REG: OnceLock<Mutex<ToolRegistry>> = OnceLock::new();
    REG.get_or_init(|| {
        let mut r = ToolRegistry::new();
        // 默认注册「AI 聊天」代理模式的全部内置工具（同一实现，不重复维护）
        for t in crate::llm::agent::builtin_tools() {
            r.register(t);
        }
        // 编排工具：由会话运行时（agent 循环）拦截处理，run 仅兜底报错
        for t in orchestration_tools() {
            r.register(t);
        }
        Mutex::new(r)
    })
}

/// 编排工具（todo/plan/goal/subagent）：schema 注册进模型，执行由 agent 循环
/// 拦截（需要会话上下文落日志），此处 run 为不可达兜底
fn orchestration_tools() -> Vec<ToolSpec> {
    let stub = |_app: Option<tauri::AppHandle>, _args: Value| -> Result<String, String> {
        Err("该工具由会话运行时处理，不应直接执行".to_string())
    };
    vec![
        ToolSpec {
            name: "job_list".to_string(),
            description: "列出当前会话的后台作业（id/名称/状态/时间）。后台作业由 exec_command 的 run_in_background=true 创建。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "job_output".to_string(),
            description: "读取指定后台作业的当前输出（运行中读最新输出，结束后读完整输出）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "作业 id" } },
                "required": ["id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "job_kill".to_string(),
            description: "终止指定后台作业（强制结束进程并回收资源）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "作业 id" } },
                "required": ["id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "spill_read".to_string(),
            description: "读取被溢写（spill）的完整工具输出：传入工具结果中给出的 locator。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "locator": { "type": "string", "description": "溢写结果中的 locator 值" } },
                "required": ["locator"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "session_ref".to_string(),
            description: "引用另一个会话的对话快照（跨会话引用）：返回该会话的用户/助手消息投影（截断至上限）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "目标会话 id" },
                    "max_chars": { "type": "integer", "description": "引用长度上限（512-8192，默认 4096）" },
                },
                "required": ["session_id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "workspace_list".to_string(),
            description: "列出全部工作区（id/名称/目录/状态），含默认工作区。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "workspace_create".to_string(),
            description: "创建新工作区（目录位于 agent_workspace 下，与工作区同名 id）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "title": { "type": "string", "description": "工作区名称" } },
                "required": ["title"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "workspace_switch".to_string(),
            description: "切换当前工作区（id 为 workspace_list 返回的 id，default = 默认工作区；影响终端/Shell 默认目录与 fs 相对路径锚点）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "工作区 id" } },
                "required": ["id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "terminal_list".to_string(),
            description: "列出全部终端会话（id/名称/工作目录）。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "terminal_open".to_string(),
            description: "新建终端会话（cwd = 当前工作区目录），返回终端 id。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "name": { "type": "string", "description": "终端名称（可空）" } },
                "required": [],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "terminal_send".to_string(),
            description: "向终端会话发送命令并返回输出（PTY 运行中则进真终端，否则独立进程执行；保持 cwd 状态）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "终端 id" },
                    "input": { "type": "string", "description": "要执行的命令" },
                },
                "required": ["id", "input"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "terminal_read".to_string(),
            description: "读取终端会话的输入/输出日志（只读快照）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "终端 id" } },
                "required": ["id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "terminal_signal".to_string(),
            description: "向 PTY 终端发送信号（当前支持 SIGINT=Ctrl+C 中断前台进程）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "终端 id" },
                    "signal": { "type": "string", "description": "SIGINT" },
                },
                "required": ["id", "signal"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "terminal_close".to_string(),
            description: "关闭终端会话（停止 PTY 并删除会话）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "终端 id" } },
                "required": ["id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "schedule_list".to_string(),
            description: "列出当前会话的定时任务（id/名称/提示词/间隔/下次运行）。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "schedule_create".to_string(),
            description: "创建当前会话的定时任务：every_minutes=周期执行（分钟）；after_seconds=一次性延时执行（秒）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "任务名称" },
                    "prompt": { "type": "string", "description": "到点执行的提示词" },
                    "every_minutes": { "type": "integer", "description": "周期分钟数（与 after_seconds 二选一）" },
                    "after_seconds": { "type": "integer", "description": "延时秒数（一次性）" },
                },
                "required": ["prompt"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "schedule_delete".to_string(),
            description: "删除指定定时任务。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "任务 id" } },
                "required": ["id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "workflow_list".to_string(),
            description: "列出已保存的工作流（id/名称/描述/阶段数）。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "workflow_run".to_string(),
            description: "运行指定工作流：按阶段顺序逐阶段执行一轮对话，前序输出注入后序提示词，结果落会话日志。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "workflow_id": { "type": "string", "description": "工作流 id" } },
                "required": ["workflow_id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "ralph".to_string(),
            description: "Ralph 迭代循环：固定轮次的全新子代理迭代（每轮全新上下文、共享工作区记忆），子代理以「已完成/已阻塞」汇报时提前结束；每轮报告落会话日志。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "objective": { "type": "string", "description": "不变的迭代目标" },
                    "max_rounds": { "type": "integer", "description": "轮次上限（1-16，默认 3）" },
                },
                "required": ["objective"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "subagent".to_string(),
            description: "派生子代理：分叉当前会话为子会话并运行任务（继承父上下文）。run_in_background=true 时后台运行、立即返回子代理 id，用 send_message 跟进、subagent_output 读结论、interrupt_agent 中断。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "子代理任务描述" },
                    "run_in_background": { "type": "boolean", "description": "true 时后台运行（默认 false，同步等待结论）" },
                },
                "required": ["task"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "send_message".to_string(),
            description: "给后台子代理发送跟进消息并执行一轮（返回其新结论）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "子代理会话 id" },
                    "message": { "type": "string", "description": "跟进消息" },
                },
                "required": ["agent_id", "message"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "interrupt_agent".to_string(),
            description: "请求中断指定子代理的进行中回合。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "agent_id": { "type": "string", "description": "子代理会话 id" } },
                "required": ["agent_id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "subagent_list".to_string(),
            description: "列出当前会话的子代理会话 id（含状态结论摘要）。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "subagent_output".to_string(),
            description: "读取指定子代理的当前结论（最后一条助手消息）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "agent_id": { "type": "string", "description": "子代理会话 id" } },
                "required": ["agent_id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "goal_create".to_string(),
            description: "创建会话目标（可指定最大自动续跑轮次）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "objective": { "type": "string", "description": "目标描述" },
                    "max_goal_rounds": { "type": "integer", "description": "最大自动续跑轮次（可空）" },
                },
                "required": ["objective"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "goal_get".to_string(),
            description: "读取当前会话目标状态（目标/状态/修订号/阻塞原因）。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "goal_update".to_string(),
            description: "更新会话目标：action=pause|resume|complete|blocked|edit；blocked 需 blocked_reason；edit 需新 objective。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["pause", "resume", "complete", "blocked", "edit"], "description": "更新动作" },
                    "objective": { "type": "string", "description": "edit 时的新目标文本" },
                    "blocked_reason": { "type": "string", "description": "blocked 时的阻塞原因" },
                },
                "required": ["action"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "ask_user_question".to_string(),
            description: "向用户提问（可带选项）并等待回答；用于需要用户决策/确认的关键节点。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "要问用户的问题" },
                    "options": { "type": "array", "items": { "type": "string" }, "description": "可选答案列表（用户也可自由输入）" },
                },
                "required": ["question"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "session_search".to_string(),
            description: "跨会话检索消息内容（多词 AND 语义），返回会话 id/事件类型/内容片段（最多 50 条）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "query": { "type": "string", "description": "检索关键词（空格分隔多词）" } },
                "required": ["query"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "session_trace".to_string(),
            description: "查询会话血缘：祖先链（分叉来源逐级向上）与直接后代（由本会话分叉出的会话）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "session_id": { "type": "string", "description": "会话 id（空 = 当前会话）" } },
                "required": [],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "session_event_read".to_string(),
            description: "读取单个完整会话事件（按日志 seq 定位；只读）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "会话 id（空 = 当前会话）" },
                    "seq": { "type": "integer", "description": "事件日志序号" },
                },
                "required": ["seq"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "session_event_search".to_string(),
            description: "在指定会话的日志内搜索事件（关键词匹配），返回 (seq, 事件类型, 内容片段)。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "会话 id（空 = 当前会话）" },
                    "query": { "type": "string", "description": "检索关键词" },
                },
                "required": ["query"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "session_event_trace".to_string(),
            description: "追踪单个会话事件的关系：目标事件的来源 seq 与引用它的派生事件（DSH session_event_trace；追加式日志无替换链，输出 Replaced by 恒为 none）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "会话 id（空 = 当前会话）" },
                    "seq": { "type": "integer", "description": "目标事件日志序号" },
                },
                "required": ["seq"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "session_list".to_string(),
            description: "列出全部 Harness 会话（id/标题/消息数/更新时间），用于维护会话与定位目标会话。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "session_create".to_string(),
            description: "新建一个空 Harness 会话，返回新会话 id（用于开启干净对话）。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "session_rename".to_string(),
            description: "重命名 Harness 会话（id 缺省 = 当前会话；title 为会话新标题）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "会话 id（空 = 当前会话）" },
                    "title": { "type": "string", "description": "新标题" },
                },
                "required": ["title"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "session_clear".to_string(),
            description: "清空指定 Harness 会话的聊天记录（删除全部消息与工具日志，保留会话本身/预设/角色；id 缺省 = 当前会话）。用于重新开始一段干净的对话。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "session_id": { "type": "string", "description": "会话 id（空 = 当前会话）" } },
                "required": [],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "session_delete".to_string(),
            description: "删除 Harness 会话及其全部日志（不可恢复；id 缺省 = 当前会话）。执行前需要用户批准。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "session_id": { "type": "string", "description": "会话 id（空 = 当前会话）" } },
                "required": [],
            }),
            requires_approval: true,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "attachment_list".to_string(),
            description: "列出当前会话的附件（名称/类型/路径/图片 sha256）。图片可经 read_image 读取路径为视觉输入。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "todo_write".to_string(),
            description: "维护当前会话的待办列表：传入完整新列表（覆盖旧列表），每项含 content 与 status(pending/in_progress/completed)。用于拆解多步任务并跟踪进度。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "description": "完整待办列表",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string" },
                                "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] },
                            },
                            "required": ["content", "status"],
                        },
                    },
                },
                "required": ["items"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "plan_enter".to_string(),
            description: "进入计划模式：先制定方案再执行。计划模式下仅只读工具可用（写入/执行类工具被拦截）。plan 参数为方案文本（可空）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "plan": { "type": "string", "description": "方案文本，可空" } },
                "required": [],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "plan_exit".to_string(),
            description: "退出计划模式，恢复全部工具可用。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "goal_set".to_string(),
            description: "设置/更新当前会话的长期目标（objective）。用于声明本次会话要达成的总体目标。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "objective": { "type": "string", "description": "目标描述" } },
                "required": ["objective"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "task".to_string(),
            description: "把任务委派给一个子代理（全新上下文）：子代理独立多轮思考并返回最终结论。用于拆解大任务时并行/隔离处理子问题。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "task": { "type": "string", "description": "子任务完整描述（子代理看不到当前会话上下文）" } },
                "required": ["task"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "skill_list".to_string(),
            description: "列出当前可用的技能（skill）及其用途说明。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "skill_load".to_string(),
            description: "读取指定技能（skill）的完整说明文档，按文档指示执行。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "name": { "type": "string", "description": "技能 id" } },
                "required": ["name"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "lsp_hover".to_string(),
            description: "向配置的语言服务器查询工作区文件某位置的类型/文档信息（hover）。需要治理面板配置 LSP 服务器。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "工作区内的文件路径" },
                    "line": { "type": "integer", "description": "行号（0 起）" },
                    "column": { "type": "integer", "description": "列号（0 起）" },
                },
                "required": ["file"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "lsp_definition".to_string(),
            description: "查询符号定义位置（goToDefinition）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "工作区内的文件路径" },
                    "line": { "type": "integer", "description": "行号（0 起）" },
                    "column": { "type": "integer", "description": "列号（0 起）" },
                },
                "required": ["file"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "lsp_references".to_string(),
            description: "查询符号的全部引用位置（findReferences）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "工作区内的文件路径" },
                    "line": { "type": "integer", "description": "行号（0 起）" },
                    "column": { "type": "integer", "description": "列号（0 起）" },
                },
                "required": ["file"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "lsp_implementation".to_string(),
            description: "查询符号的实现位置（goToImplementation）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "工作区内的文件路径" },
                    "line": { "type": "integer", "description": "行号（0 起）" },
                    "column": { "type": "integer", "description": "列号（0 起）" },
                },
                "required": ["file"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "plugin_list".to_string(),
            description: "列出已定义的全部动态插件（id/名称/启用状态/工具清单）。定义插件前先查询，避免同名覆盖。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "plugin_define".to_string(),
            description: "定义或更新一个动态插件（DSH extensions 语义）：tools 内每个工具由模型提供 name/description/parameters/实现代码 code（async 函数体，可用 args 与 ctx.fetch/ctx.log）。更新即产生新版本。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "插件名称（非空）" },
                    "description": { "type": "string", "description": "插件说明" },
                    "id": { "type": "string", "description": "插件 id（空 = 新建）" },
                    "enabled": { "type": "boolean", "description": "是否立即启用（默认 true）" },
                    "tools": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string", "description": "工具名" },
                                "description": { "type": "string", "description": "工具说明" },
                                "parameters": { "type": "object", "description": "OpenAI 参数 schema" },
                                "code": { "type": "string", "description": "实现代码（async 函数体）" },
                                "requires_approval": { "type": "boolean", "description": "调用是否需要审批（默认 false）" },
                            },
                            "required": ["name", "code"],
                        },
                    },
                },
                "required": ["name", "tools"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "plugin_delete".to_string(),
            description: "删除指定动态插件（取消定义）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "插件 id" } },
                "required": ["id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "plugin_enable".to_string(),
            description: "启用指定动态插件（其工具进入可用注册表）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "插件 id" } },
                "required": ["id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "plugin_disable".to_string(),
            description: "停用指定动态插件（其工具从可用注册表移除）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "插件 id" } },
                "required": ["id"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "run_code".to_string(),
            description: "运行模型编写的程序（DSH Code Mode 语义）：code 为 async 函数体（TypeScript/JavaScript 方言），可用 args 参数与 ctx.fetch/ctx.log 绑定；在安全前端沙箱执行，返回日志与返回值。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "language": { "type": "string", "description": "语言（typescript / javascript）" },
                    "code": { "type": "string", "description": "程序：async 函数体" },
                    "args": { "type": "object", "description": "传入 args 的对象（可空）" },
                },
                "required": ["code"],
            }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
        ToolSpec {
            name: "workflow_run_js".to_string(),
            description: "运行模型编写的 JS 编排脚本（DSH workflow 组合子）：code 为 async 函数体，ctx 提供 agent(prompt)→派生子代理并返回其结论、parallel([...])→并发执行、pipeline(items, ...stages)→逐阶段流水线；返回脚本返回值（JSON）。需审批；编排多轮子代理，耗时可能较长。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "编排脚本：async 函数体" },
                    "args": { "type": "object", "description": "传入 args 的对象（可空）" },
                },
                "required": ["code"],
            }),
            requires_approval: true,
            run: crate::llm::agent::ToolRunner::Fn(stub),
        },
    ]
}

fn prompt_sections() -> &'static Mutex<Vec<PromptSection>> {
    static P: OnceLock<Mutex<Vec<PromptSection>>> = OnceLock::new();
    P.get_or_init(|| {
        Mutex::new(vec![PromptSection {
            order: 0,
            title: "harness-base".to_string(),
            content: "你是 DeepSeek Harness（ST 版）的智能代理。可以调用注册的工具完成用户请求；\
                       工具执行结果会作为上下文回传。回答使用用户的语言。"
                .to_string(),
        }])
    })
}

/// 注册/覆盖工具（插件/扩展接入点：阶段 3+ 的 preset/动态插件启用）
#[allow(dead_code)]
pub fn register_tool(spec: ToolSpec) {
    registry().lock().unwrap().register(spec);
}

/// 注册 prompt 分区（order 升序组装；扩展接入点）
#[allow(dead_code)]
pub fn add_prompt_section(section: PromptSection) {
    prompt_sections().lock().unwrap().push(section);
}

/// 按会话作用域过滤后的 OpenAI tools 定义（preset 禁用的工具不注入模型）
pub fn tools_json_scoped(scope: &crate::harness::preset::SessionScope) -> Value {
    let reg = registry().lock().unwrap();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut arr: Vec<Value> = Vec::new();
    // 启用插件的工具优先（DSH extensions：插件工具遮蔽同名内置工具，
    // 与 llm/agent 的 tools_json 解析顺序一致）
    for (_pid, t) in crate::llm::agent_plugins::enabled_plugin_tools() {
        if !scope.is_disabled(&t.name) && seen.insert(t.name.clone()) {
            arr.push(json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            }));
        }
    }
    for t in reg.specs.values() {
        if !scope.is_disabled(&t.name) && seen.insert(t.name.clone()) {
            arr.push(json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            }));
        }
    }
    Value::Array(arr)
}

/// 计划模式下仍可用的只读工具（其余被拦截；DSH plan 模式守卫）
pub fn is_readonly_tool(name: &str) -> bool {
    matches!(
        name,
        "web_search"
            | "fetch_web_page"
            | "get_current_time"
            | "search_knowledge_base"
            | "read_file"
            | "list_dir"
            | "todo_write"
            | "job_list"
            | "job_output"
            | "spill_read"
            | "session_ref"
            | "workspace_list"
            | "workspace_create"
            | "workspace_switch"
            | "terminal_list"
            | "terminal_read"
            | "schedule_list"
            | "subagent_list"
            | "subagent_output"
            | "goal_get"
            | "session_search"
            | "session_trace"
            | "session_event_read"
            | "session_event_search"
            | "session_event_trace"
            | "session_list"
            | "attachment_list"
            | "lsp_hover"
            | "lsp_definition"
            | "lsp_references"
            | "lsp_implementation"
            | "plan_enter"
            | "plan_exit"
            | "goal_set"
            | "plugin_list"
    )
}

/// 按作用域判定审批要求（preset 覆盖优先，否则全局定义）
pub fn requires_approval_scoped(name: &str, scope: &crate::harness::preset::SessionScope) -> bool {
    if let Some(v) = scope.requires_approval_override(name) {
        return v;
    }
    registry().lock().unwrap().requires_approval(name)
}

/// 组装系统提示词：全局分区 + 预设附加分区（order 升序拼接）
pub fn assemble_system_prompt_scoped(scope: &crate::harness::preset::SessionScope) -> String {
    let mut v = prompt_sections().lock().unwrap().clone();
    for s in &scope.prompt_sections {
        v.push(PromptSection {
            order: s.order,
            title: s.title.clone(),
            content: s.content.clone(),
        });
    }
    v.sort_by_key(|s| s.order);
    v.iter()
        .map(|s| format!("[{}]\n{}", s.title, s.content))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// 守卫执行管道（异步 + 超时）：工具在阻塞线程池执行，超时放弃等待
/// （guard：工具超时可配置，DSH guard 包语义）
pub async fn execute_tool_guarded(
    app: &tauri::AppHandle,
    name: &str,
    args: &Value,
    timeout_secs: u64,
) -> ToolExecOutcome {
    let started = std::time::Instant::now();
    let app2 = app.clone();
    let name2 = name.to_string();
    let args2 = args.clone();
    let fut = tauri::async_runtime::spawn_blocking(move || {
        registry()
            .lock()
            .unwrap()
            .execute(Some(&app2), &name2, &args2)
    });
    match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), fut).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => ToolExecOutcome {
            ok: false,
            result: format!("工具执行异常: {}", e),
            duration_ms: started.elapsed().as_millis() as u64,
        },
        Err(_) => ToolExecOutcome {
            ok: false,
            result: format!("工具执行超时（{} 秒），已放弃等待", timeout_secs),
            duration_ms: started.elapsed().as_millis() as u64,
        },
    }
}

/// 工具目录（前端展示）
#[derive(Serialize)]
pub struct HarnessToolInfo {
    pub name: String,
    pub description: String,
    pub requires_approval: bool,
    /// 参数 schema（OpenAI JSON Schema 字符串，工具目录可展开查看）
    pub parameters: String,
}

pub fn tool_infos() -> Vec<HarnessToolInfo> {
    let reg = registry().lock().unwrap();
    let mut v: Vec<HarnessToolInfo> = reg
        .specs
        .values()
        .map(|t| HarnessToolInfo {
            name: t.name.clone(),
            description: t.description.clone(),
            requires_approval: t.requires_approval,
            parameters: t.parameters.to_string(),
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn truncate(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        s.to_string()
    } else {
        format!("{}…", chars[..n].iter().collect::<String>())
    }
}

/// 工具目录（前端展示）
#[tauri::command]
pub async fn get_harness_tools() -> Result<Vec<HarnessToolInfo>, String> {
    Ok(tool_infos())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spec() -> ToolSpec {
        ToolSpec {
            name: "h_test_echo".to_string(),
            description: "测试".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: crate::llm::agent::ToolRunner::Fn(|_app, args| {
                Ok(args
                    .get("x")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string())
            }),
        }
    }

    #[test]
    fn registry_register_get_and_execute() {
        let mut r = ToolRegistry::new();
        r.register(sample_spec());
        assert!(r.get("h_test_echo").is_some());
        let out = r.execute(None, "h_test_echo", &json!({ "x": "hi" }));
        assert!(out.ok && out.result == "hi");
        // 未知工具报错
        let miss = r.execute(None, "nope", &json!({}));
        assert!(!miss.ok && miss.result.contains("未知工具"));
    }

    #[test]
    fn guard_pipeline_pre_hook_vetoes_execution() {
        let mut r = ToolRegistry::new();
        r.register(sample_spec());
        r.add_pre_hook(|name, _args| {
            if name == "h_test_echo" {
                Some("否决执行".to_string())
            } else {
                None
            }
        });
        let out = r.execute(None, "h_test_echo", &json!({ "x": "hi" }));
        assert!(!out.ok);
        assert_eq!(out.result, "否决执行");
    }

    #[test]
    fn register_same_name_overrides() {
        let mut r = ToolRegistry::new();
        r.register(sample_spec());
        r.register(sample_spec()); // 同名覆盖 → 仍只有一个
        assert_eq!(r.names().len(), 1);
    }

    #[test]
    fn system_prompt_assembles_in_order() {
        // 全局分区：harness-base(order 0) 恒在首位
        let prompt =
            assemble_system_prompt_scoped(&crate::harness::preset::SessionScope::default());
        assert!(prompt.contains("[harness-base]"));
        add_prompt_section(PromptSection {
            order: 100,
            title: "test-late".to_string(),
            content: "LATE".to_string(),
        });
        let prompt2 =
            assemble_system_prompt_scoped(&crate::harness::preset::SessionScope::default());
        let base_pos = prompt2.find("[harness-base]").unwrap();
        let late_pos = prompt2.find("[test-late]").unwrap();
        assert!(base_pos < late_pos, "order 升序组装");
    }

    #[test]
    fn plugin_tools_registered_and_readonly_guard() {
        let scope = crate::harness::preset::SessionScope::default();
        let tools = tools_json_scoped(&scope);
        let names: std::collections::HashSet<String> = tools
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t["function"]["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        for n in [
            "plugin_list",
            "plugin_define",
            "plugin_delete",
            "plugin_enable",
            "plugin_disable",
            "run_code",
        ] {
            assert!(names.contains(n), "工具目录应包含 {n}");
        }
        assert!(is_readonly_tool("plugin_list"), "plugin_list 应只读");
        assert!(!is_readonly_tool("run_code"), "run_code 不应只读");
        assert!(!is_readonly_tool("plugin_define"), "plugin_define 不应只读");
    }

    #[test]
    fn approval_scoped_override_wins_else_global() {
        // requires_approval_scoped：preset override 优先，否则全局定义
        use crate::harness::preset::{SessionScope, ToolOverride};
        use std::collections::HashMap;
        // 无 override → 全局定义（exec_command 需审批）
        let plain = SessionScope::default();
        assert!(
            requires_approval_scoped("exec_command", &plain),
            "exec_command 全局应需审批"
        );
        assert!(!requires_approval_scoped("read_file", &plain));
        // override 为 false → 免审批（preset 覆盖优先）
        let scoped = SessionScope {
            overrides: HashMap::from([(
                "exec_command".to_string(),
                ToolOverride {
                    requires_approval: Some(false),
                    timeout_secs: None,
                },
            )]),
            ..Default::default()
        };
        assert!(
            !requires_approval_scoped("exec_command", &scoped),
            "preset override false 应免审批"
        );
        // override 为 true → 需审批（覆盖全局免审批工具）
        let scoped2 = SessionScope {
            overrides: HashMap::from([(
                "read_file".to_string(),
                ToolOverride {
                    requires_approval: Some(true),
                    timeout_secs: None,
                },
            )]),
            ..Default::default()
        };
        assert!(
            requires_approval_scoped("read_file", &scoped2),
            "preset override true 应需审批"
        );
    }

    #[test]
    fn readonly_whitelist_covers_query_tools_and_excludes_writers() {
        // plan 模式守卫：查询/只读工具全在白名单，写/执行工具被排除
        let expected_readonly = [
            "web_search",
            "fetch_web_page",
            "get_current_time",
            "search_knowledge_base",
            "read_file",
            "list_dir",
            "todo_write",
            "job_list",
            "job_output",
            "spill_read",
            "session_ref",
            "workspace_list",
            "workspace_create",
            "workspace_switch",
            "terminal_list",
            "terminal_read",
            "schedule_list",
            "subagent_list",
            "subagent_output",
            "goal_get",
            "session_search",
            "session_trace",
            "session_event_read",
            "session_event_search",
            "session_event_trace",
            "session_list",
            "attachment_list",
            "lsp_hover",
            "lsp_definition",
            "lsp_references",
            "lsp_implementation",
            "plan_enter",
            "plan_exit",
            "goal_set",
            "plugin_list",
        ];
        for name in expected_readonly {
            assert!(is_readonly_tool(name), "只读工具应在白名单: {name}");
        }
        // 写/执行工具绝不在白名单（plan 模式必须拦截）
        let writers = [
            "exec_command",
            "write_file",
            "edit_file",
            "str_replace_editor",
            "run_code",
            "plugin_define",
            "plugin_delete",
            "plugin_enable",
            "plugin_disable",
            "goal_create",
            "goal_update",
            "subagent",
            "send_message",
            "workflow_run_js",
            "schedule_create",
            "schedule_delete",
            "session_clear",
            "session_delete",
            "session_rename",
        ];
        for name in writers {
            assert!(!is_readonly_tool(name), "写工具不应在只读白名单: {name}");
        }
    }
}
