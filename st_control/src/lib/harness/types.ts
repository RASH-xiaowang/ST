// Harness（DSH 纯原生迁移）— 前端类型定义（与后端 src-tauri/src/harness 对齐）

/** 会话元信息（消息数 = 用户消息数） */
export interface HarnessSessionMeta {
  id: string;
  title: string;
  /** 会话级预设覆盖（"" = 跟随全局设置） */
  preset_id: string;
  /** 会话归属工作区（"" = 默认工作区；DSH 工作区浏览器） */
  workspace_id: string;
  created_at: string;
  updated_at: string;
  message_count: number;
  /** 归档标记（DSH 归档会话：从常规列表隐去，保留在已归档分组） */
  archived: boolean;
}

/** 会话级 AI 角色（原「AI 聊天」角色注入迁移；日志投影） */
export interface HarnessRoleView {
  name: string;
  prompt: string;
}

/** 工具调用视图（模型可见的调用事实） */
export interface HarnessToolCallView {
  id: string;
  name: string;
  arguments: string;
}

/** 会话日志事件（追加式；type 判别，与 Rust HarnessEvent 对齐） */
export type HarnessEvent =
  | { type: "user_message"; id: string; content: string }
  | { type: "assistant_chunk"; id: string; delta: string; done: boolean }
  | {
      type: "assistant_message";
      id: string;
      content: string;
      /** 推理过程全文（Think 推理行；DSH ReasoningRow 迁移） */
      reasoning?: string | null;
    }
  | { type: "assistant_tool_calls"; id: string; calls: HarnessToolCallView[] }
  | {
      type: "tool_result";
      id: string;
      ok: boolean;
      result: string;
      duration_ms: number;
    }
  | { type: "session_title"; title: string }
  | { type: "session_forked"; source: string; boundary_seq: number }
  | { type: "session_cleared" }
  | { type: "role_set"; name: string; prompt: string };

/** 工具步骤视图（UI 展示用投影） */
export interface HarnessToolStepView {
  id: string;
  name: string;
  args: string;
  status: string;
  result?: string;
  duration_ms?: number;
}

/** UI 投影消息（后端从日志投影，渲染与回放同源） */
export interface HarnessDisplayMessage {
  role: "user" | "assistant" | "meta";
  content: string;
  /** 日志序号（fork 边界、定位/回放锚点） */
  seq: number;
  /** 该回复之前发生的工具步骤 */
  tools?: HarnessToolStepView[];
  /** 推理过程全文（Think 折叠行） */
  reasoning?: string | null;
  /** meta 行（compaction / role / context / workflow / skill）专用 */
  kind?: string;
  title?: string;
  detail?: string;
  /** 工作流阶段结构化视图（DSH WorkflowRunPanel：阶段进度点 + 状态文案） */
  workflow?: { workflow_id: string; name: string; stage: number; total: number };
}

/** harness_chat_stream 通道事件 */
export type HarnessStreamEvent =
  | { type: "user_message"; seq: number; id: string; content: string }
  | {
      type: "assistant_chunk";
      id: string;
      delta: string;
      /** 推理增量（实时 Think 行；空正文时的增量） */
      reasoning_delta?: string;
      done: boolean;
    }
  | {
      type: "assistant_tool_calls";
      id: string;
      calls: HarnessToolCallView[];
    }
  | {
      type: "tool_result";
      id: string;
      name: string;
      ok: boolean;
      result: string;
      duration_ms: number;
    }
  | {
      type: "done";
      content: string;
      seq: number;
      model: string;
      prompt_tokens: number;
      completion_tokens: number;
      cost: number;
    }
  | { type: "error"; message: string }
  | { type: "goal_auto_round"; round: number; max: number };

/** 工具目录条目（get_harness_tools；parameters = 参数 schema JSON 字符串） */
export interface HarnessToolInfo {
  name: string;
  description: string;
  requires_approval: boolean;
  parameters?: string;
}

/** 用户设置（最近使用的提供方/模型 + guard 配置 + 默认预设） */
export interface HarnessSettings {
  last_provider_id: string;
  last_model: string;
  /** 工具执行超时（秒，5~300；null = 默认 30） */
  tool_timeout_secs?: number | null;
  /** 最大工具轮次（1~12；null = 默认 6） */
  max_agent_rounds?: number | null;
  /** 默认预设 id（null/"" = 不启用） */
  preset_id?: string | null;
  /** 受限执行世界：允许访问 agent_workspace 之外（默认 false） */
  allow_workspace_escape?: boolean;
  /** 沙箱模式：read-only / workspace-write / danger-full-access */
  sandbox_mode?: string;
  /** 当前工作区 id（""/default = 默认工作区） */
  workspace_id?: string;
  /** 上下文压缩预算（token 估算；null = 默认 24000） */
  context_budget_tokens?: number | null;
  /** 启用上下文压缩（默认 true） */
  enable_compaction?: boolean;
  /** 繁忙时 Enter 键行为（DSH busyEnter）：queue=排队 / steer=插话 */
  busy_enter?: string | null;
  /** 会话级推理等级（DSH reasoningEffort：off/high/max；空 = 跟随提供方默认） */
  reasoning_effort?: string | null;
  /** 联网搜索提供商（DSH web 提供商缝）：bing / deepseek */
  web_search_provider?: string | null;
}

/** 预设附加 prompt 分区 */
export interface PresetPromptSection {
  order: number;
  title: string;
  content: string;
}

/** 工具覆盖项（preset） */
export interface ToolOverride {
  requires_approval?: boolean;
  timeout_secs?: number;
}

/** 预设（组合 + 会话作用域） */
export interface HarnessPreset {
  id: string;
  name: string;
  description: string;
  disabled_tools: string[];
  overrides: Record<string, ToolOverride>;
  prompt_sections: PresetPromptSection[];
  created_at: string;
  updated_at: string;
}

/** 外部钩子 */
export interface HarnessHook {
  id: string;
  event: string;
  /** 匹配器（CC 方言）：空 = 全部命中；非空 = 载荷包含该子串才触发 */
  matcher?: string;
  command: string;
  enabled: boolean;
}

/** 钩子触发回传 */
export interface HarnessHookFired {
  id: string;
  event: string;
  ok: boolean;
  output: string;
}

/** 会话用量聚合（telemetry；含 DSH 统计条字段） */
export interface HarnessUsageSummary {
  session_id: string;
  /** 模型回合数（轮） */
  turns: number;
  /** 工具调用步数 */
  steps: number;
  prompt_tokens: number;
  completion_tokens: number;
  cost: number;
  /** LLM 请求墙钟合计（毫秒） */
  llm_wall_ms: number;
  /** 工具调用墙钟合计（毫秒） */
  tool_wall_ms: number;
  /** 首 token / 首字节延迟平均（毫秒） */
  first_token_avg_ms: number;
  /** 输出 tokens / 秒 */
  tokens_per_sec: number;
  /** 缓存命中率（0~1） */
  cache_hit_rate: number;
  input_tokens: number;
  output_tokens: number;
}

/** 匿名身份 */
export interface HarnessIdentity {
  id: string;
  created_at: string;
}

/** 审批请求载荷（harness-approval-requested 事件） */
export interface HarnessApprovalPayload {
  id: string;
  session_id: string;
  tool: string;
  description: string;
  arguments: string;
}

/** 待办条目 */
export interface TodoItem {
  id: string;
  content: string;
  status: string;
}

/** 会话运行状态（plan / goal / todo，日志投影） */
export interface HarnessSessionState {
  plan_mode: boolean;
  plan_text: string;
  goal: string;
  /** active / paused / blocked / complete */
  goal_status: string;
  goal_revision: number;
  goal_blocked_reason: string;
  goal_max_rounds?: number | null;
  todos: TodoItem[];
}

/** 定时任务 */
export interface HarnessSchedule {
  id: string;
  name: string;
  session_id: string;
  prompt: string;
  interval_minutes: number;
  enabled: boolean;
  next_run_at: number;
  last_run_at?: number | null;
  created_at: string;
}

/** 工作流阶段 */
export interface WorkflowStage {
  name: string;
  prompt: string;
}

/** 工作流 */
export interface HarnessWorkflow {
  id: string;
  name: string;
  description: string;
  stages: WorkflowStage[];
  created_at: string;
  updated_at: string;
}

/** 工作流运行结果 */
export interface WorkflowRunResult {
  workflow_id: string;
  stages: Array<{ name: string; ok: boolean; output: string }>;
}

/** 人工工具派发结果 */
export interface ToolDispatchResult {
  id: string;
  ok: boolean;
  result: string;
  duration_ms: number;
}

/** Shell 执行结果 */
export interface ShellResult {
  ok: boolean;
  output: string;
  timed_out: boolean;
  duration_ms: number;
}

/** 文件系统条目 */
export interface FsEntry {
  name: string;
  is_dir: boolean;
}

/** 终端会话（持久 cwd） */
export interface TerminalSession {
  id: string;
  name: string;
  cwd: string;
  created_at: string;
}

/** 终端日志条目 */
export interface TerminalLogEntry {
  input: string;
  output: string;
}

/** 附件元信息 */
export interface AttachmentMeta {
  id: string;
  name: string;
  path: string;
  kind: string;
  size: number;
  created_at: string;
}

/** MCP 服务器配置 */
export interface McpServerConfig {
  id: string;
  name: string;
  command: string;
  args: string[];
  enabled: boolean;
  /** 额外环境变量（与凭据注入合并） */
  env?: Record<string, string>;
  /** 服务器工作目录（空 = 继承） */
  cwd?: string | null;
}

/** 技能 */
export interface SkillInfo {
  id: string;
  name: string;
  description: string;
  content: string;
}

/** 子代理目录节点（DSH SubagentCatalog 迁移：会话头树目录） */
export interface SubagentNode {
  id: string;
  title: string;
  /** 模式：continuable（分叉会话可继续聊） */
  mode: string;
  /** 活动状态：running（有进行中回合）| inactive */
  activity: string;
  has_children: boolean;
  children: SubagentNode[];
}

/** 反馈记录 */
export interface FeedbackRecord {
  id: number;
  session_id: string;
  rating: string;
  comment: string;  /** 助手消息序号（消息级反馈） */
  message_seq?: number | null;
  created_at: string;
}

/** 会话查询命中 */
export interface SearchHit {
  session_id: string;
  event_type: string;
  snippet: string;
}

/** 凭据视图（掩码） */
export interface CredentialView {
  key: string;
  masked: string;
}

/** LSP 服务器配置 */
export interface LspServerConfig {
  id: string;
  name: string;
  command: string;
  args: string[];
  /** 文件扩展名映射（如 ["rs","toml"]）：按扩展名路由服务器 */
  extensions: string[];
  enabled: boolean;
}

/** 后台作业记录（DSH jobs 迁移） */
export interface HarnessJobRecord {
  id: string;
  session_id: string;
  name: string;
  /** running / done / error / killed */
  status: string;
  created_at: string;
  finished_at?: string | null;
}

/** 工作区实体（DSH workspace 迁移） */
export interface WorkspaceEntity {
  id: string;
  title: string;
  dir: string;
  status: string;
  created_at: string;
}

/** 轨迹台账条目（DSH Trajectory 迁移：对话|轨迹 标签页数据源，日志投影） */
export type TrajectoryEntry =
  | { kind: "user"; seq: number; time: string; content: string }
  | {
      kind: "assistant";
      seq: number;
      time: string;
      content: string;
      turn: number;
      steps: number;
      tool_calls: number;
    }
  | {
      kind: "tool";
      seq: number;
      time: string;
      id: string;
      name: string;
      args: string;
      ok: boolean;
      result: string;
      duration_ms: number;
    }
  | { kind: "system"; seq: number; time: string; event: string; summary: string; detail: string };

/** 轨迹台账（entries + 汇总计数） */
export interface HarnessTrajectory {
  entries: TrajectoryEntry[];
  turn_count: number;
  tool_call_count: number;
}

/** 回合产物文件（DSH ProducedFiles 迁移） */
export interface TurnFileView {
  path: string;
  seq: number;
}

/** 上下文占用投影（DSH ContextMeter 迁移：输入区环形仪表） */
export interface ContextMeterView {
  used_tokens: number;
  budget_tokens: number;
  /** 0~1 占用率 */
  percent: number;
  system_tokens: number;
  tools_tokens: number;
  messages_tokens: number;
}
