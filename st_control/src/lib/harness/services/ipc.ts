// Harness（DSH 纯原生迁移）— Tauri IPC 封装层
import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  HarnessSessionMeta,
  HarnessDisplayMessage,
  HarnessStreamEvent,
  HarnessToolInfo,
  HarnessSettings,
  HarnessIdentity,
  HarnessPreset,
  HarnessHook,
  HarnessUsageSummary,
  HarnessSessionState,
  HarnessSchedule,
  HarnessWorkflow,
  WorkflowRunResult,
  ToolDispatchResult,
  ShellResult,
  TerminalSession,
  TerminalLogEntry,
  AttachmentMeta,
  McpServerConfig,
  SkillInfo,
  FeedbackRecord,
  SearchHit,
  CredentialView,
  LspServerConfig,
  HarnessJobRecord,
  WorkspaceEntity,
  HarnessRoleView,
  HarnessTrajectory,
  TurnFileView,
  ContextMeterView,
  SubagentNode,
} from "../types";

export const harnessApi = {
  listSessions: () => invoke<HarnessSessionMeta[]>("harness_list_sessions"),
  /** 创建会话；workspaceId 为空 = 默认工作区（DSH 工作区浏览器） */
  createSession: (workspaceId?: string | null) =>
    invoke<HarnessSessionMeta>("harness_create_session", {
      workspaceId: workspaceId ?? "",
    }),
  /** 设置会话归属工作区（会话在工作区间移动） */
  setSessionWorkspace: (id: string, workspaceId: string) =>
    invoke<void>("harness_set_session_workspace", { id, workspaceId }),
  /** 会话祖先链（DSH 面包屑：SessionForked 溯源逐级向上，近→远） */
  sessionLineage: (id: string) =>
    invoke<Array<[string, string]>>("harness_session_lineage", { id }),
  /** 子代理目录树（DSH SubagentCatalog：会话头树目录） */
  subagentCatalog: (sessionId: string) =>
    invoke<SubagentNode[]>("harness_subagent_catalog", { sessionId }),
  /** 设置会话归档标记（DSH 归档会话：归档/恢复） */
  setSessionArchived: (id: string, archived: boolean) =>
    invoke<void>("harness_set_session_archived", { id, archived }),
  /** 设置会话手动排序序号（DSH 拖拽排序：交换双方各写一次） */
  setSessionOrder: (id: string, orderIndex: number) =>
    invoke<void>("harness_set_session_order", { id, orderIndex }),
  /** 交换两个会话的排序序号（DSH 拖拽排序：前端拖放即交换） */
  swapSessionOrder: (a: string, b: string) =>
    invoke<void>("harness_swap_session_order", { a, b }),
  renameSession: (id: string, title: string) =>
    invoke<void>("harness_rename_session", { id, title }),
  /** B19：LLM 生成会话标题（手动触发，重命名并返回新标题） */
  generateTitle: (sessionId: string) =>
    invoke<string>("harness_generate_title", { sessionId }),
  /** B2：workflow JS 编排的子代理原语（前端 ctx.agent 调用） */
  workflowAgent: (sessionId: string, prompt: string) =>
    invoke<string>("harness_workflow_agent", { sessionId, prompt }),
  deleteSession: (id: string) =>
    invoke<number>("harness_delete_session", { id }),
  /** 清空会话聊天记录（维护会话：删除事件与用量，保留会话元信息） */
  clearSession: (id: string) => invoke<void>("harness_clear_session", { id }),
  /** UI 投影：日志 → 展示消息（渲染与回放同源） */
  displayMessages: (id: string) =>
    invoke<HarnessDisplayMessage[]>("harness_display_messages", { id }),
  /** 轨迹台账（DSH Trajectory 迁移：「轨迹」标签页数据源） */
  trajectory: (id: string) => invoke<HarnessTrajectory>("harness_trajectory", { id }),
  /** 回合产物文件（DSH ProducedFiles 迁移） */
  turnFiles: (id: string) => invoke<TurnFileView[]>("harness_turn_files", { id }),
  /** 上下文占用（DSH ContextMeter：输入区环形仪表） */
  contextMeter: (id: string) =>
    invoke<ContextMeterView>("harness_context_meter", { sessionId: id }),
  /** 打开文件/目录（产物 chip / 工具路径点击） */
  openPath: (path: string) => invoke<void>("harness_open_path", { path }),
  /** 分叉：复制 boundary_seq 之前的事件为新会话，返回新会话元信息 */
  forkSession: (id: string, boundarySeq: number) =>
    invoke<HarnessSessionMeta>("harness_fork_session", {
      id,
      boundarySeq,
    }),
  /** 会话级预设覆盖（"" = 跟随全局默认） */
  setSessionPreset: (id: string, presetId: string) =>
    invoke<void>("harness_set_session_preset", { id, presetId }),
  /** 会话级 AI 角色（原「AI 聊天」角色注入迁移；空 prompt = 清除） */
  setSessionRole: (id: string, name: string, prompt: string) =>
    invoke<void>("harness_set_session_role", { id, name, prompt }),
  /** 读取会话当前 AI 角色（日志投影） */
  getSessionRole: (id: string) =>
    invoke<HarnessRoleView>("harness_get_session_role", { id }),
  /** 转写导出：日志投影为 Markdown（回放/存档）；path 非空时写文件并返回路径 */
  exportSession: (id: string, path?: string | null) =>
    invoke<string>("harness_export_session", { id, path }),
  /** 配置束导出（预设+技能+MCP+LSP+钩子）；path 非空时写文件并返回路径 */
  exportBundle: (path?: string | null) =>
    invoke<string>("harness_export_bundle", { path }),
  /** 配置束导入（文件路径或 JSON 文本）；返回合并条目数 */
  importBundle: (path?: string | null, json?: string | null) =>
    invoke<number>("harness_import_bundle", { path, json }),

  /** 会话对话流（工具循环）：落日志 → 投影上下文 → 模型/工具循环 → 回答落日志 */
  chatStream: (
    sessionId: string,
    providerId: string | null,
    model: string | null,
    content: string,
    onEvent: (ev: HarnessStreamEvent) => void,
  ): Promise<void> => {
    const channel = new Channel<string>();
    channel.onmessage = (msg: string) => {
      try {
        onEvent(JSON.parse(msg) as HarnessStreamEvent);
      } catch {
        /* 忽略无法解析的帧 */
      }
    };
    return invoke<void>("harness_chat_stream", {
      sessionId,
      providerId,
      model,
      content,
      onEvent: channel,
    });
  },
  /** 请求中断指定会话的进行中回合（UI「停止」） */
  cancelTurn: (sessionId: string) =>
    invoke<void>("harness_cancel_turn", { sessionId }),
  /** 人工目标操作（DSH GoalBar：pause/resume/complete/clear/blocked/edit） */
  goalAction: (
    sessionId: string,
    action: string,
    blockedReason?: string | null,
    objective?: string | null,
  ) =>
    invoke<void>("harness_goal_action", {
      sessionId,
      action,
      blockedReason: blockedReason ?? null,
      objective: objective ?? null,
    }),

  // ─── 工具 / 审批 / 身份 / 设置 ───
  getTools: () => invoke<HarnessToolInfo[]>("get_harness_tools"),
  approveTool: (id: string) => invoke<boolean>("approve_harness_tool", { id }),
  rejectTool: (id: string) => invoke<boolean>("reject_harness_tool", { id }),
  /** 记住批准（M8：参数参与信任指纹，仅完全相同参数的命令免审批） */
  trustTool: (sessionId: string, tool: string, argsJson?: string) =>
    invoke<void>("trust_harness_tool", {
      sessionId,
      tool,
      arguments: argsJson ?? "",
    }),
  answerQuestion: (id: string, answer: string) =>
    invoke<boolean>("harness_answer_question", { id, answer }),
  getIdentity: () => invoke<HarnessIdentity>("get_harness_identity"),
  getSettings: () => invoke<HarnessSettings>("get_harness_settings"),
  saveSettings: (settings: HarnessSettings) =>
    invoke<HarnessSettings>("save_harness_settings", { settings }),
  usageSummary: (sessionId: string) =>
    invoke<HarnessUsageSummary>("harness_usage_summary", { id: sessionId }),
  sessionState: (sessionId: string) =>
    invoke<HarnessSessionState>("harness_session_state", { id: sessionId }),
  /** 人工命令：不经过模型，直接在会话中派发一次工具调用 */
  executeTool: (sessionId: string, name: string, argumentsJson: string) =>
    invoke<ToolDispatchResult>("harness_execute_tool", {
      sessionId,
      name,
      arguments: argumentsJson,
    }),
  /** 无锁派发（仅前端执行桥 ctx.tools 使用：外层派发已持会话锁，
   *  嵌套调用再取锁会死锁） */
  executeToolNoLock: (sessionId: string, name: string, argumentsJson: string) =>
    invoke<ToolDispatchResult>("harness_execute_tool_nolock", {
      sessionId,
      name,
      arguments: argumentsJson,
    }),

  // ─── 预设（组合与会话作用域）───
  listPresets: () => invoke<HarnessPreset[]>("list_harness_presets"),
  savePreset: (preset: HarnessPreset) =>
    invoke<HarnessPreset>("save_harness_preset", { preset }),
  deletePreset: (id: string) => invoke<void>("delete_harness_preset", { id }),

  // ─── 外部钩子 ───
  listHooks: () => invoke<HarnessHook[]>("list_harness_hooks"),
  saveHooks: (hooks: HarnessHook[]) =>
    invoke<HarnessHook[]>("save_harness_hooks", { hooks }),

  // ─── 定时任务（schedule） ───
  listSchedules: () => invoke<HarnessSchedule[]>("list_harness_schedules"),
  saveSchedule: (schedule: HarnessSchedule) =>
    invoke<HarnessSchedule>("save_harness_schedule", { schedule }),
  deleteSchedule: (id: string) =>
    invoke<void>("delete_harness_schedule", { id }),
  runScheduleNow: (id: string) =>
    invoke<void>("run_harness_schedule_now", { id }),

  // ─── 工作流（workflow） ───
  listWorkflows: () => invoke<HarnessWorkflow[]>("list_harness_workflows"),
  saveWorkflow: (workflow: HarnessWorkflow) =>
    invoke<HarnessWorkflow>("save_harness_workflow", { workflow }),
  deleteWorkflow: (id: string) =>
    invoke<void>("delete_harness_workflow", { id }),
  runWorkflow: (workflowId: string, sessionId: string) =>
    invoke<WorkflowRunResult>("run_harness_workflow", {
      workflowId,
      sessionId,
    }),

  // ─── 执行世界（fs / shell / terminal） ───
  fsRead: (path: string) => invoke<string>("harness_fs_read", { path }),
  fsDelete: (path: string) => invoke<void>("harness_fs_delete", { path }),
  shellRun: (command: string, cwd?: string | null, timeoutSecs?: number | null) =>
    invoke<ShellResult>("harness_shell_run", {
      command,
      cwd,
      timeoutSecs,
    }),
  listTerminals: () => invoke<TerminalSession[]>("list_harness_terminals"),
  createTerminal: (name?: string | null) =>
    invoke<TerminalSession>("create_harness_terminal", { name }),
  deleteTerminal: (id: string) =>
    invoke<void>("delete_harness_terminal", { id }),
  terminalLogs: (id: string) =>
    invoke<TerminalLogEntry[]>("harness_terminal_logs", { id }),
  terminalSend: (id: string, input: string) =>
    invoke<string>("harness_terminal_send", { id, input }),
  /** PTY 真终端（ConPTY）：启动/停止/发送/调整尺寸/状态 */
  terminalStartPty: (id: string, rows?: number | null, cols?: number | null) =>
    invoke<void>("harness_terminal_start_pty", { id, rows, cols }),
  terminalStopPty: (id: string) =>
    invoke<void>("harness_terminal_stop_pty", { id }),
  terminalSendPty: (id: string, input: string) =>
    invoke<string>("harness_terminal_send_pty", { id, input }),
  terminalResizePty: (id: string, rows: number, cols: number) =>
    invoke<void>("harness_terminal_resize_pty", { id, rows, cols }),
  terminalPtyStatus: (id: string) =>
    invoke<{ running: boolean; rows: number; cols: number }>(
      "harness_terminal_pty_status",
      { id },
    ),

  // ─── 附件 / MCP ───
  attachFile: (sessionId: string, sourcePath: string) =>
    invoke<AttachmentMeta>("harness_attach_file", { sessionId, sourcePath }),
  listAttachments: (sessionId: string) =>
    invoke<AttachmentMeta[]>("harness_list_attachments", { sessionId }),
  listMcpServers: () => invoke<McpServerConfig[]>("list_harness_mcp_servers"),
  saveMcpServers: (servers: McpServerConfig[]) =>
    invoke<McpServerConfig[]>("save_harness_mcp_servers", { servers }),

  // ─── 技能 / 反馈 / 会话查询 / KV / CLI ───
  listSkills: () => invoke<SkillInfo[]>("list_harness_skills"),
  saveSkill: (skill: SkillInfo) =>
    invoke<SkillInfo>("save_harness_skill", { skill }),
  deleteSkill: (id: string) => invoke<void>("delete_harness_skill", { id }),
  submitFeedback: (sessionId: string, rating: string, comment?: string, messageSeq?: number) =>
    invoke<void>("harness_submit_feedback", {
      sessionId,
      rating,
      comment,
      messageSeq,
    }),
  listFeedback: () => invoke<FeedbackRecord[]>("harness_list_feedback"),
  searchSessions: (query: string) =>
    invoke<SearchHit[]>("harness_search_sessions", { query }),
  kvPut: (key: string, value: string) =>
    invoke<void>("harness_kv_put", { key, value }),
  kvGet: (key: string) => invoke<string | null>("harness_kv_get", { key }),
  listSpills: (sessionId: string) =>
    invoke<string[]>("harness_list_spills", { sessionId }),
  cli: (input: string) => invoke<string>("harness_cli", { input }),

  // ─── 凭据 / LSP ───
  credentialList: () => invoke<CredentialView[]>("harness_credential_list"),
  credentialPut: (key: string, value: string, storeInEnv?: boolean) =>
    invoke<void>("harness_credential_put", { key, value, storeInEnv }),
  credentialDelete: (key: string) =>
    invoke<void>("harness_credential_delete", { key }),
  listLspServers: () => invoke<LspServerConfig[]>("list_harness_lsp_servers"),
  saveLspServers: (servers: LspServerConfig[]) =>
    invoke<LspServerConfig[]>("save_harness_lsp_servers", { servers }),

  // ─── 后台作业（DSH jobs） ───
  jobList: (sessionId: string) =>
    invoke<HarnessJobRecord[]>("harness_job_list", { sessionId }),
  jobOutput: (id: string) => invoke<string>("harness_job_output", { id }),
  jobKill: (id: string) => invoke<void>("harness_job_kill", { id }),

  // ─── 工作区（DSH workspace） ───
  listWorkspaces: () => invoke<WorkspaceEntity[]>("list_harness_workspaces"),
  createWorkspace: (title: string) =>
    invoke<WorkspaceEntity>("create_harness_workspace", { title }),
  deleteWorkspace: (id: string) =>
    invoke<void>("delete_harness_workspace", { id }),
  setWorkspaceStatus: (id: string, status: string) =>
    invoke<void>("set_harness_workspace_status", { id, status }),
};
