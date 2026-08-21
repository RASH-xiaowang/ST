# ST「Harness 会话」板块现状盘点

> 研究范围：`src/lib/harness/HarnessTab.svelte`（约 4110 行）、`src/lib/harness/types.ts`、`src/lib/harness/services/ipc.ts`、`src-tauri/src/harness/`（34 个 .rs 文件）、`docs/harness-migration-plan.md`。
> 说明：只读盘点，未修改任何源码。对比基准为 DeepSeek Harness（DSH，Node 生态）全功能面。

---

## 一、前端 UI 现状（`HarnessTab.svelte`）

### 1. 整体布局

- 左 **260px 会话侧栏** + 右 **主区**（头部 / 统计条 / 消息区 / 底部输入条）。
- 「治理中心」为**全高右侧滑出面板**（470px 毛玻璃 + 左侧阴影 + 入场动画），非浮动抽屉。
- 「工具目录」为头部按钮切换的下拉面板。
- 设计系统沿用应用令牌 `--app-color-*`，Harness 内部派生 `--hns-*` 语义变量；主区径向渐变背景、头部毛玻璃 + 标题发光点、侧栏会话卡片激活高亮、消息气泡非对称圆角、审批/计划/目标横幅左色轨、输入条悬浮毛玻璃 + focus 光环。

### 2. 头部（`hns-bar`）

- 会话标题 + 轻提示 notice（分叉/导出/预设切换反馈，3 秒自动消失）。
- 提供方下拉 + 模型下拉（选择持久化到 Harness 设置，回退全局默认）。
- 「工具（N）」按钮（切换工具目录面板）。
- 「治理」按钮（打开治理中心）。
- AI 角色下拉（复用 `llmApi.getAiRoles()`，会话级持久化、按日志投影回显；选「无角色」清除）。
- 会话预设下拉（`预设：全局默认 / 预设：<name>`，`""` = 跟随全局默认预设）。
- 「导出」按钮（Markdown 回放转写）。

### 3. 统计条（`hns-stats`，DSH 会话遥测）

- `X 轮 · Y 步 | LLM 墙钟 · 工具调用墙钟 | 首 token 平均 · tok/s | 缓存命中 % | 输入/输出 token | 成本`
- 回合完成后随 usage 刷新；`fmtWall` / `fmtSec` / `fmtTok` 做 DSH 风格紧凑格式化。

### 4. 会话侧栏功能

- 新建（PlusIcon）。
- 重命名（内联编辑，Enter 提交 / Escape 取消）。
- 删除（`window.confirm` 二次确认，删除后自动落到可用会话）。
- 清空聊天记录（Eraser 图标 + 确认；保留会话/预设/角色）。
- 搜索（`harness_search_sessions`，命中列表点击跳转会话，截断 60 字，按 event_type 标「问/答」）。
- 消息数徽标（用户消息数）。
- 激活会话高亮（内轨 + 描边）。

### 5. 对话区

- **用户气泡**：MessageBody 渲染 + 复制 + 「分叉」（`seq>0` 时）。
- **助手气泡**：工具时间线（`hns-tool-timeline`，左侧竖线连接各步骤节点到回复气泡）+ 回复气泡 + 反馈条。
  - 工具步骤：圆形状态节点（完成绿 ✓ / 失败红 ✕ / 执行中强调色脉冲点）；行内等宽工具名 + 参数摘要 + 状态徽标 + 时长 + 展开 chevron；展开显示参数/结果 + 复制；失败与重试序列如实展示。
  - 反馈条：分叉 / 朗读（TTS）/ 复制 / 👍 / 👎（按消息级 `seq` 打分）。
- **meta 行**（`compaction` 🗜️ / `role_set`）：可展开 detail。
- **实时回合**：`liveTools` 时间线与流式回复气泡合并同一容器，逐 delta 渲染 + 光标；done 后回合并入历史（同构渲染，日志回放一致）。
- **计划横幅**（`plan_mode`）：计划模式 · 仅只读工具可用。
- **目标横幅**（`goal`）：🎯 + 状态（active / paused / blocked + 阻塞原因 / complete）。
- **待办卡**（`todos`）：○/▶/✓ 状态（pending / in_progress / completed）。
- **审批卡**（`pendingApprovals`）：工具名 + 参数摘要 + 复制 + 「记住并批准 / 批准 / 拒绝」三按钮。
- **提问卡**（`pendingQuestions`，DSH user-questions 接缝）：选项按钮 + 自由输入框 + 回答。
- **附件 chip**（`attachments`）：📎 名称，text 类标注「已注入上下文预览」。
- **空态 hero**：「DeepSeek Harness」标题 + 能力说明。

### 6. 底部输入条

- 回形针附件按钮（tauri 对话框选文件 → 复制进工作区 `attachments/`）。
- textarea（Enter 发送 / Shift+Enter 换行）。
- 麦克风语音输入（VAD → blobToWav16kMono → 本地/云端 STT → 输入框）。
- 发送中显示「停止」按钮（`harness_cancel_turn` 中断当前回合，已生成内容保留），否则显示发送按钮。

### 7. 工具目录面板

- 头部：标题 + 计数（`N 个工具 · M 需审批`）+ 搜索框（名称/说明过滤）。
- 按 **7 功能族分组**：会话管理 / 文件与内容 / 执行环境 / 信息检索 / 编排与协作 / 技能与语言服务 / 系统与集成，组标题 + 数量徽标。
- 每项可点击展开**参数 schema**（`HarnessToolInfo.parameters`）；需审批工具带 🔒 徽标；描述两行截断。

### 8. 治理中心（13 个 tab，图标网格分组）

- **基础**：设置 / 钩子 / 预设 / 定时 / 工作流
- **执行**：终端 / 作业
- **内容**：技能 / CLI
- **集成**：凭据 / LSP / MCP / 插件

各 tab 功能：

| tab | 功能 |
|---|---|
| 设置 | 工具超时(5~300s)/最大轮次(1~12)/默认预设/受限执行世界开关/沙箱三模式/当前工作区+工作区管理/上下文压缩开关+预算(4000~128000) |
| 钩子 | 事件下拉(10 种生命周期点)/匹配器/PowerShell 命令/启用开关/增删改 + 触发记录(最近 20 条) |
| 预设 | 名称/描述/禁用工具多选/附加提示词分区；列表增删改 |
| 定时 | 名称/提示词/间隔(1~10080 分钟)/启用；列表 + 立即运行 + 删除 |
| 工作流 | 名称/描述/阶段(每行「名称 \| 提示词」)；列表 + 运行 + 编辑 + 删除 |
| 终端 | 新建/删除会话；cwd 显示；命令输入与日志；PTY 启动/停止(ConPTY) |
| 作业 | 后台作业列表(状态:运行中/完成/已终止/错误)/查看输出/终止 |
| 技能 | SKILL.md 内容编辑(id 留空自动生成,首行 # 名称)；列表增删改 |
| CLI | 命令串输入(sessions list/session create/session chat/session show/tools list/usage) |
| 凭据 | 键值凭据(掩码展示)/写入 .env 提供者开关；列表增删 |
| LSP | 名称/命令/参数(逗号分隔)/扩展名映射/启用；列表增删改 |
| MCP | 名称/命令/参数/启用；列表增删改 + 配置束导入导出(预设+技能+MCP+LSP+钩子) |
| 插件 | 插件名称/说明/工具 JSON(async 函数体)/启用；列表启停/编辑/删除(复用 llmApi 插件 IPC) |

---

## 二、IPC 接口面（`services/ipc.ts`，共 78 个 harness 专用命令）

### 会话（15）
`harness_list_sessions` · `harness_create_session` · `harness_rename_session` · `harness_delete_session` · `harness_clear_session` · `harness_display_messages` · `harness_fork_session` · `harness_set_session_preset` · `harness_set_session_role` · `harness_get_session_role` · `harness_export_session` · `harness_export_bundle` · `harness_import_bundle` · `harness_chat_stream`（Channel 流式） · `harness_cancel_turn`

### 工具 / 审批 / 身份 / 设置（11）
`get_harness_tools` · `approve_harness_tool` · `reject_harness_tool` · `trust_harness_tool` · `harness_answer_question` · `get_harness_identity` · `get_harness_settings` · `save_harness_settings` · `harness_usage_summary` · `harness_session_state` · `harness_execute_tool`

### 预设（3）
`list_harness_presets` · `save_harness_preset` · `delete_harness_preset`

### 钩子（2）
`list_harness_hooks` · `save_harness_hooks`

### 定时（4）
`list_harness_schedules` · `save_harness_schedule` · `delete_harness_schedule` · `run_harness_schedule_now`

### 工作流（4）
`list_harness_workflows` · `save_harness_workflow` · `delete_harness_workflow` · `run_harness_workflow`

### 执行世界（13）
`harness_fs_read` · `harness_fs_delete` · `harness_shell_run` · `list_harness_terminals` · `create_harness_terminal` · `delete_harness_terminal` · `harness_terminal_logs` · `harness_terminal_send` · `harness_terminal_start_pty` · `harness_terminal_stop_pty` · `harness_terminal_send_pty` · `harness_terminal_resize_pty` · `harness_terminal_pty_status`

### 附件 / MCP（4）
`harness_attach_file` · `harness_list_attachments` · `list_harness_mcp_servers` · `save_harness_mcp_servers`

### 技能 / 反馈 / 查询 / KV / CLI（10）
`list_harness_skills` · `save_harness_skill` · `delete_harness_skill` · `harness_submit_feedback` · `harness_list_feedback` · `harness_search_sessions` · `harness_kv_put` · `harness_kv_get` · `harness_list_spills` · `harness_cli`

### 凭据 / LSP（5）
`harness_credential_list` · `harness_credential_put` · `harness_credential_delete` · `list_harness_lsp_servers` · `save_harness_lsp_servers`

### 后台作业（3）
`harness_job_list` · `harness_job_output` · `harness_job_kill`

### 工作区（4）
`list_harness_workspaces` · `create_harness_workspace` · `delete_harness_workspace` · `set_harness_workspace_status`

### 类型定义（`types.ts`，380 行，约 35 个接口）

核心接口：`HarnessSessionMeta`（含 `preset_id`）、`HarnessRoleView`、`HarnessToolCallView`、`HarnessEvent`（8 种追加式事件：user_message / assistant_chunk / assistant_message / assistant_tool_calls / tool_result / session_title / session_forked / session_cleared / role_set）、`HarnessToolStepView`、`HarnessDisplayMessage`（含 `seq` 锚点 + `tools`）、`HarnessStreamEvent`（含 done 的 token/cost）、`HarnessToolInfo`（含 `parameters`）、`HarnessSettings`（含沙箱三模式/工作区/压缩预算）、`HarnessPreset`、`HarnessHook`、`HarnessUsageSummary`（8 元组遥测）、`HarnessSessionState`（plan/goal/todos）、`HarnessSchedule`、`HarnessWorkflow`、`TerminalSession`/`TerminalLogEntry`、`AttachmentMeta`、`McpServerConfig`、`SkillInfo`、`FeedbackRecord`（含 `message_seq`）、`SearchHit`、`CredentialView`、`LspServerConfig`（含 `extensions`）、`HarnessJobRecord`、`WorkspaceEntity` 等。

---

## 三、Rust 后端模块一览（34 个文件，`src-tauri/src/harness/`）

| 模块 | 职责 | 关键函数 |
|---|---|---|
| mod.rs | 入口/引导 | `init`（注册 sessions store + fs/shell/web/storage 服务 + seed_examples + sdk::start） |
| registry.rs | Cordis-lite 服务注册表 | `provide` / `get` / `remove` + `Disposer.disarm` |
| session.rs | 会话核心（49KB） | `SessionStore.new/create/list/set_preset/preset_id/fork/export_markdown/rename/delete/append/events/set_role/clear_messages/role/role_from_events/record_usage/usage_summary/submit_feedback/list_feedback/search/trace/kv_put/kv_get/kv_delete/derive_model_messages/derive_display_messages/session_state` + 18 个 `harness_*` IPC |
| agent.rs | 回合编排（91KB 大头） | `request_cancel` / `harness_chat_stream` / `harness_cancel_turn` / `run_turn` / `handle_subagent_tool` / `run_turn_internal` / `harness_execute_tool` / `execute_tool_command` |
| tools.rs | 工具注册表 + 守卫执行（46KB） | `ToolRegistry.new/register/add_pre_hook/add_post_hook/get/requires_approval/names/execute`、`register_tool`、`add_prompt_section`、`tools_json_scoped`、`is_readonly_tool`、`requires_approval_scoped`、`assemble_system_prompt_scoped`、`execute_tool_guarded`、`tool_infos`、`get_harness_tools` |
| approval.rs | 审批门控 | `request_approval` / `approve_harness_tool` / `reject_harness_tool` / `trust_harness_tool` / `clear_trust_for_session` |
| preset.rs | 预设组合/作用域 | `SessionScope.is_disabled/tool_timeout/requires_approval_override`、`scope_for_preset/session/session_id`、`list/save/delete`、`get_harness_scope`、`seed_examples` |
| hooks.rs | 外部钩子桥 | `fire` / `fire_decision` / `list_harness_hooks` / `save_harness_hooks` |
| settings.rs | 用户设置 | `effective_sandbox_mode/workspace_escape/timeout_secs/max_rounds/budget_tokens`、`get/save/current` |
| identity.rs | 匿名身份 | `get_harness_identity` |
| schedule.rs | 定时调度 | `start`（30s 调度器）、`list/save/delete/run_now`、`list_for_session/create_for_session/delete_for_session` |
| workflow.rs | 工作流 | `run_workflow`、`list/save/delete/run_harness_workflow` |
| subagent.rs | 子代理 | `run_subagent` / `fork_child` / `check_child` / `conclusion` / `list_children` |
| terminal.rs | 持久终端 | `list/create/delete/logs/send`、`send_regular`、`normalize_cwd`、`session_cwd/update_cwd/push_log/logs` |
| pty.rs | ConPTY 真终端 | `stop/start/send/is_running/send_raw/resize` + 5 个 `harness_terminal_*_pty` IPC |
| shell.rs | Shell 接缝 | `ShellService.workspace_root/resolve_cwd/run/run_with_policy`、`kill_tree`、`provide_service`、`harness_shell_run` |
| fs.rs | 文件接缝 | `FsService.resolve/read_text/write_text/list_dir/delete/edit_text/glob/grep/read_image_base64`、`provide_service`、`harness_fs_read/delete` |
| jobs.rs | 后台作业 | `start/list/output/kill/check_owner` + `harness_job_list/output/kill` |
| workspace.rs | 多工作区 | `default_workspace/workspace_dir/sandbox_root/current/create/list/delete/set_status` + 4 IPC |
| web.rs | Web 接缝 | `WebService.search/fetch`、`provide_service` |
| storage.rs | KV 存储 | `StorageService.put/get/delete/put_in/get_in/delete_in/backends`、`provide_service` + IPC |
| credentials.rs | 凭据 | `env_values/all_values/put_env/inject_env` + `harness_credential_list/put/delete` |
| context.rs | 请求上下文提供者 | `add_provider` / `assemble` |
| instructions.rs | AGENTS.md 注入 | `rescan` / `inject` / `session_ref` |
| compaction.rs | 上下文压缩 | `list_spills` / `prune_tool_results` / `maybe_compact` / `harness_list_spills` |
| spill.rs | 溢写 | `SpillStore.save/read`、`maybe_spill` |
| attachment.rs | 附件 | `attach_file` / `attachments_from_events` / `context_block` + `harness_attach_file/list_attachments` |
| skill.rs | 技能 | `save_skill/delete_skill`、`list_harness_skills`、`skill_list_result/skill_load_result`、`inject_next/drain_injections` |
| feedback.rs | 反馈 | `submit` / `list` + `harness_submit_feedback/list_feedback` |
| interaction.rs | 人工提问 | `ask_user` / `harness_answer_question` / `cancel_session_questions` / `options_from_args` |
| mcp.rs | MCP 客户端 | `mcp_store/persist/refresh_registry`、`list_harness_mcp_servers/save_harness_mcp_servers` |
| lsp.rs | LSP 客户端 | `lsp_store/persist/hover/query_via_tool`、`list_harness_lsp_servers/save_harness_lsp_servers` |
| sdk.rs | JSON-RPC + CLI | `start` / `harness_cli` / `dispatch` |
| portability.rs | 配置束导入导出 | `harness_export_bundle` / `harness_import_bundle` |

---

## 四、现状总结

1. **已覆盖**：核心会话（追加式事件日志 + 真流式 + 回放 + 分叉 + Markdown 导出）、工具循环 + 审批/信任、preset/hooks/settings/telemetry（统计条）、编排（subagent/workflow/todo/plan/goal/schedule/jobs）、执行世界（fs/shell/terminal/PTY/沙箱三模式/workspace）、协议连接器（web/context/compaction/attachment/storage/spill/sdk/mcp/lsp/credentials/skill/feedback/session-query/CLI/插件+run_code/配置束导入导出）、语音 TTS/STT、AI 角色注入——共 78 个 IPC、34 个 Rust 模块。
2. **缺口**（迁移计划自认的 partial/missing，与 DSH 对比）：subagent 缺 fork/continuable 后台子代理 + send_message/interrupt_agent；workflow 缺 JS 编排脚本/Ralph；goal 仅单目标字符串、缺完整状态机/revision/自动续跑；session-query 无 FTS5/血缘 trace/检索工具；compaction 缺 /compact 命令；mcp 缺 schema 透传/重连/env 配置；lsp 仅 hover；acp 仅 3 方法；e2b 明确不迁移。
3. **实现特征**：动态插件（extensions）与 run_code 复用 `llm/agent_plugins`，走前端 WebView `new Function` 沙箱执行桥（`harness-tool-exec-request` 事件）；模型可见 ⟺ 落日志为贯穿原则；UI 渲染与回放同源（均从日志投影）。
4. **迁移状态**：迁移计划声明 0-11 阶段 + 界面重设计 + DSH 统计条 + 真流式 + AI 聊天并入全部完成，E2E/CDP 探针 + IPC 契约门禁（全库 407 命令）均通过。
5. 总体：DSH 功能面已完成**约 80%+** 的高价值能力迁移，剩余缺口集中在深水区（多 agent 编排语义、goal 状态机、检索/血缘、协议完备性）。

---

## 五、不确定项

1. **SDK JSON-RPC 方法全集**：`sdk.rs` 的 `dispatch(method, params)` 实际暴露的方法集未逐一读取（迁移计划写 ACP 仅 `session/new|prompt|cancel` 三方法 + `sessions.list/session.create/session.display/session.state/session.chat/tool.execute/usage.get`）；若要精确清单需读该文件 dispatch 内部。
2. **内置模型工具精确名单**：`tools.rs` 中 `register_tool` 的全部调用点未逐条提取，仅从 `HarnessTab.toolCategory()` 分组与迁移计划间接推得（含 session_* 5 个、fs 工具 edit_file/glob/grep/read_image、terminal_*/workspace_*/job_*/schedule_*/subagent/plugin_*/run_code/spill_read/lsp_* 等）；如需精确清单应 grep `register_tool(` 调用点。
3. **沙箱三模式「逐调用升级审批」是否真正落地**：`fs.rs`/`shell.rs` 只见 `FsPolicy`/`SandboxPolicy` 参数，未见逐调用 `sandbox_permissions` 升级审批流（迁移计划阶段 11 声明已做，但需核对其具体实现位置）。
4. **共享层改动未展开**：统计条/流式/语音依赖的 `llm/client/chat.rs`、`llm/services/{voice,ttsPlayer,voiceRecorder}`、`llm/agent_plugins.rs` 属共享层，本盘点未展开其内部改动。
5. **统计条「步数/工具墙钟」来源**：`HarnessUsageSummary.steps` 与 `tool_wall_ms` 声明为「从事件日志投影」，但 `record_usage` 与日志投影的两套口径是否完全一致未逐一核对。
