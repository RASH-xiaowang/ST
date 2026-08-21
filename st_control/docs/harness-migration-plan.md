# DeepSeek Harness → ST「Harness」纯原生迁移蓝图

目标：把 `E:\ST\deepseek-harness-master`（DSH）的全部功能迁移进 ST 主控台导航
「Harness」界面。路线：**纯原生重写** —— 不依赖 Node，DSH 运行时
（packages/ 1,981 个 TS 文件、约 39.4 万行）用 Rust 重写，UI 用 Svelte 5
重建。DSH 源码目录是唯一事实来源，只读参考、不做修改。

## 迁移原则（源自 DSH architecture）

1. **一切皆插件**：服务注册是效应（effect），可逆；卸载即回滚（Cordis-lite）。
2. **模型可见 ⟺ 落日志**：进入模型请求的任何内容必须能从会话日志（追加式
   事件流）重建；新模型可见输入 → 新会话事件类型。
3. **能力接缝三角色**：Service Definition（接口）/ Service Provider（实现）/
   Consumer（消费，通常是模型工具）成组迁移，缺一不可。
4. **显式优于隐式**：默认值由所属实现做显式 resolve，不在 run() 里隐藏兜底。
5. **会话日志为唯一上下文来源**：UI、回放、标题、遥测都从事件流投影。

## 全量功能清单 → 阶段映射（零遗漏）

### 阶段 0 — 已完成（K-1 / K-2，AI 聊天内先行落地）

| DSH 包 | 迁移内容 | ST 落点 |
|---|---|---|
| core/agent-loop（部分） | 模型→tool_calls→执行→回传→循环（≤6 轮） | `llm/agent.rs` |
| core/tools（部分） | 工具注册表 + 8 个内置工具 + 执行 | `llm/agent.rs` |
| interaction（部分） | 审批门控（批准/拒绝/记住/超时） | `llm/agent.rs` |
| self-modification（部分） | 动态插件：定义/运行/更新/停用/删除 + 版本历史 | `llm/agent_plugins.rs` + 前端抽屉 |

### 阶段 1 — 已完成：导航入口 + 运行时骨架 + 会话核心

| DSH 包 | 迁移内容 | ST 落点 |
|---|---|---|
| web（外壳）+ bundle/web-app | 「Harness」导航入口与页面壳 | `App.svelte`（AI 工作台区，AI 聊天之后）+ `lib/harness/HarnessTab.svelte` |
| 无（ST 自研 Cordis-lite） | 服务注册表：provide/get/remove + Disposer 可逆效应（Drop 撤销 / disarm 常驻） | `harness/registry.rs` |
| core/session | 追加式 SessionEvent 日志（user_message / assistant_chunk / assistant_message / session_title）+ SQLite 持久化 + 首条用户消息标题投影 + 消息数 + 日志→模型消息/UI 投影 | `harness/session.rs` + `db.rs`（harness_sessions / harness_events 表，seq 单调递增） |
| llm/llm（适配层） | 流式对话：用户消息落日志 → 投影上下文 → 流式生成（增量实时推送 + 分批落日志）→ 回复边界落日志 → done | `harness/chat.rs`（复用 `llm/client::chat_completion_stream`） |

验证：7 个 harness 单测（注册表可逆、事件序列化、会话 CRUD、chunk 归组投影、标题投影、服务引导）；
E2E `e2e-harness-phase1.mjs` ALL_PASS（12 断言：导航入口 / 界面壳 / 建会话 / 流式回复 / 标题投影 /
消息数 / 整页重载日志回放 / 重命名 / 删除）；vision 复检布局协调。

### 阶段 2 — 已完成：工具与交互完备 + 会话内工具循环

| DSH 包 | 迁移内容 | ST 落点 |
|---|---|---|
| core/tools（完整） | 作用域工具注册表（全局作用域；会话作用域随 preset 阶段接入）+ 守卫执行管道（pre → 执行 → post，Rust 简化 waterfall：钩子返回 Some 即否决）+ 8 个内置工具复用「AI 聊天」实现（llm::agent 工具实现改 pub(crate) 共享） | `harness/tools.rs` |
| core/system-prompt | prompt 分区注册与 order 升序组装，注入系统提示词 | `harness/tools.rs`（PromptSection + assemble_system_prompt） |
| core/agent-loop | 会话内工具循环（≤6 轮）：user_message → assistant_tool_calls → tool_result* → assistant_message；模型可见 ⟺ 落日志（新事件类型入会话日志）；最终回答单块下发 | `harness/agent.rs`（替代 phase1 chat.rs） |
| interaction（完整） | 审批请求（10 分钟超时）+ `harness-approval-requested` 事件 + 批准/拒绝/记住并批准 IPC + 会话级信任（(session, tool)，30 分钟 TTL，删除会话联动清理） | `harness/approval.rs` |
| identity / settings | 匿名身份（data/harness/identity.json）+ 用户设置（最近提供方/模型，原子写） | `harness/identity.rs` / `harness/settings.rs` |
| core/session（扩展） | 新事件 assistant_tool_calls / tool_result；模型上下文投影（OpenAI tool_calls/role=tool）；UI 投影工具步骤挂到对应助手回复 | `harness/session.rs` |

前端（HarnessTab）：工具步骤卡（状态/耗时/展开参数与结果/复制）+ 审批卡（完整参数 + 三按钮）+
工具目录面板 + 提供方/模型选择持久化。

验证：harness 单测 15 个（注册表可逆 / 守卫否决 / 同名覆盖 / prompt 分区排序 /
审批信任作用域 / 身份稳定 / 设置回环 / 工具步骤投影等）；全库 cargo 259 passed；
`e2e-harness-phase2.mjs` ALL_PASS（21 断言：工具目录 / get_current_time 真实调用与详情 /
exec_command 审批卡三按钮 / 批准执行 / 记住并批准后同会话第三次免审批 /
重载后工具历史从日志回放 / 设置持久化）；phase1 探针回归 ALL_PASS；
vision 复检工具卡与「工具(8)」按钮布局协调。

### 阶段 3 — 已完成：组合与治理（guard / hooks / preset / telemetry）

| DSH 包 | 迁移内容 | ST 落点 |
|---|---|---|
| guard | 工具超时（5~300s，默认 30）+ 循环卫生（最大轮次 1~12，默认 6）：均为校验过的设置项（settings），「部署可变项 = 可配置」 | `harness/tools.rs`（execute_tool_guarded：spawn_blocking + timeout 放弃等待）+ `harness/settings.rs` |
| hooks | 外部钩子桥：turn_start / turn_end / tool_executed → PowerShell 命令（环境变量 HARNESS_EVENT/HARNESS_SESSION + stdin JSON 载荷，≤10 秒上限），结果经 `harness-hook-fired` 事件回传 | `harness/hooks.rs` |
| preset | 预设组合与会话作用域：disabled_tools 过滤 / overrides（requires_approval、timeout_secs）/ 附加 prompt 分区；默认预设来自设置，每次回合动态应用 | `harness/preset.rs` + `harness/tools.rs`（tools_json_scoped / requires_approval_scoped / assemble_system_prompt_scoped）+ `get_harness_scope` |
| telemetry（并入 session） | 会话用量：每轮 prompt/completion tokens 与成本落库，聚合查询 | `harness_usage` 表 + `harness/session.rs`（record_usage / harness_usage_summary） |

前端：治理抽屉（设置：超时/轮次/默认预设；钩子：增删改 + 触发记录；预设：表单
含禁用工具多选与提示词分区）+ 头部会话用量徽标。修复 persistSelection 部分对象
覆写设置的问题（合并保留 guard/preset 配置）。

验证：harness 单测 23 个；全库 cargo **263 passed**；`e2e-harness-phase3.mjs`
ALL_PASS（20 断言：预设/设置/作用域禁用 web_search / 治理抽屉 / telemetry 用量 /
钩子触发记录 / exec_command 预设覆盖 1 秒超时守卫生效 / 清理恢复）；
phase1/phase2 探针回归 ALL_PASS；vision 复检治理按钮与用量徽标布局协调。

### 阶段 4 — 已完成：编排能力（subagent / workflow / todo / plan / goal / schedule）

| DSH 包 | 迁移内容 | ST 落点 |
|---|---|---|
| subagent | `task` 工具：全新上下文子代理（独立消息序列 + 工具循环 ≤4 轮，返回结论；需审批工具拒绝） | `harness/subagent.rs` + agent 循环拦截 |
| workflow | 有序阶段流水线：逐阶段一轮对话，前序输出注入后序提示词，结果落会话日志（workflow_run 事件） | `harness/workflow.rs` |
| todo | `todo_write` 工具 + TodoUpdate 会话事件 + UI 待办卡（状态/进行中/完成） | `harness/session.rs` + tools.rs |
| plan | `plan_enter`/`plan_exit` + logged state：计划模式守卫拦截非只读工具（不弹审批直接落日志） | agent 循环 + tools.rs（is_readonly_tool） |
| goal | `goal_set` 工具 + GoalSet 事件 + UI 目标横幅 | 同上 |
| schedule / jobs | 定时条目（间隔分钟）→ 每 30 秒调度器自动触发一轮代理对话 +「立即运行」手动触发 | `harness/schedule.rs` |
| interaction（扩展） | 人工命令 `harness_execute_tool`：不经过模型直接派发一次工具调用（DSH ctx.commands 语义） | `harness/agent.rs` |

前端：治理抽屉新增定时/工作流标签（新建/编辑/删除/运行），会话运行状态投影
（待办卡/计划横幅/目标横幅）。修复：人工命令须路由会话编排工具；
提供方解析回退链（显式 → Harness 设置记忆 → 全局默认 → 首个启用）。

验证：harness 单测 23 个；全库 cargo **265 passed**；`e2e-harness-phase4.mjs`
ALL_PASS（20 断言：todo_write 人工派发与投影 / 计划模式守卫拦截 exec_command
不弹审批 / goal 投影 / 子代理 task 返回 56088 / 定时任务立即运行落日志 /
两阶段工作流输出落日志 / 治理抽屉新标签）；phase1~3 探针回归 ALL_PASS。

### 阶段 5 — 已完成：执行与文件世界（shell / subprocess / terminal / fs / sandbox）

| DSH 包 | 迁移内容 | ST 落点 |
|---|---|---|
| shell | Shell 能力接缝（Service/Provider/Consumer）：本地 PowerShell 提供者（输出重定向临时文件防死锁 + 超时强制终止）；受限执行世界（cwd 限制在 agent_workspace，政策可放行）；人工命令 `harness_shell_run` | `harness/shell.rs` |
| fs | 文件系统能力接缝：FsService（read/write/list/delete + 路径沙箱策略）；内置 read_file/write_file/list_dir 工具改为**消费 FsService**（单一提供者）；人工命令 harness_fs_read/delete | `harness/fs.rs` + `llm/agent.rs`（工具委托） |
| terminal | 持久终端会话：cwd 状态保持（命令尾部注入定位标记 + PowerShell FileSystem 提供者前缀规范化）+ 输入/输出日志 + 会话持久化 | `harness/terminal.rs` |
| sandbox / subprocess | 受限执行世界：SandboxPolicy / FsPolicy（allow_workspace_escape，设置项默认 false）统一约束 shell/fs/终端；进程超时与输出上限 | `harness/shell.rs` / `harness/fs.rs` / settings |
| lsp | 推迟：需要外部语言服务器进程，留待阶段 7 与扩展生态一并评估 | — |

前端：治理抽屉新增「终端」标签（新建/删除会话、cwd 显示、命令输入与日志）；设置新增「受限执行世界」开关。

验证：harness 单测 30 个；全库 cargo **272 passed**；`e2e-harness-phase5.mjs`
ALL_PASS（13 断言：shell echo / 受限世界拒绝工作区外 cwd / shell→fs 世界统一
（写→读→删）/ fs 越界拒绝 / 终端 cwd 状态保持与持久化 / 终端 UI）；
phase1~4 探针回归 ALL_PASS。修复：终端 cwd 规范化（FileSystem 提供者前缀）、
计划模式守卫消息不再诱导模型自行 plan_exit。

### 阶段 6 — 已完成：协议与连接器（web 接缝 / context / compaction / attachment / sdk / mcp）

| DSH 包 | 迁移内容 | ST 落点 |
|---|---|---|
| web | Web 能力接缝（Service/Provider/Consumer）：Bing 双域搜索 + 网页抓取实现自 llm/agent 上移至 WebService；内置 web_search/fetch_web_page 工具改消费 | `harness/web.rs` + `llm/agent.rs`（委托） |
| context | 请求上下文提供者注册表：默认提供者投影会话状态（目标/计划模式/待办），每轮组装进系统提示词（模型可见 ⟺ 落日志：来源均为日志投影） | `harness/context.rs` + agent 循环 |
| compaction | 上下文压缩：token 预算（设置 context_budget_tokens，4000~128000）+ 开关；超预算时模型生成旧轮摘要替换历史，Compaction 事件落日志 | `harness/compaction.rs` |
| attachment | 附件：文件复制进工作区 attachments/、AttachmentAdded 事件落日志（列表/回放同源）、文本附件内容预览注入上下文；前端回形针按钮（tauri 对话框）+ 附件 chip | `harness/attachment.rs` + HarnessTab |
| sdk | JSON-RPC 2.0 服务（127.0.0.1:4770，本地无鉴权）：sessions.list / session.create / session.display / session.state / session.chat / tool.execute / usage.get | `harness/sdk.rs` |
| mcp | MCP 客户端（stdio）：initialize + tools/list + tools/call（无状态派生会话）；外部工具注册为 mcp_<server>_<tool> 进 Harness 工具注册表（ToolRunner::Dyn 支持捕获闭包）；配置持久化 + IPC | `harness/mcp.rs` + `llm/agent.rs`（ToolRunner） |

验证：harness 单测 36 个；全库 cargo **279 passed**；`e2e-harness-phase6.mjs`
ALL_PASS（14 断言：SDK 健康检查/会话创建/对话/投影、compaction 事件落日志、
附件附加与投影、MCP 服务器注册 + echo 工具真实回显）；phase1~5 探针回归
ALL_PASS；IPC 契约 369 命令全一致。

### 阶段 7 — 已完成：扩展生态（skill / feedback / session-query / storage / spill / 示例）

| DSH 包 | 迁移内容 | ST 落点 |
|---|---|---|
| skill | 技能能力：目录约定 data/harness/skills/<id>/SKILL.md；IPC 列表/保存/删除；模型工具 skill_list / skill_load；前端技能管理标签 | `harness/skill.rs` + tools.rs + HarnessTab |
| feedback | 会话反馈：好/差评 + 评论（harness_feedback 表）+ IPC；助手回复 👍/👎 按钮 | `harness/feedback.rs` + db.rs + HarnessTab |
| session-query | 会话查询：按关键词搜索事件载荷（LIKE，命中片段）+ 侧栏搜索框 | `harness/session.rs`（search）+ db.rs + HarnessTab |
| storage | 存储能力：SQLite KV（harness_kv 表）+ StorageService + IPC put/get/delete | `harness/storage.rs` + db.rs |
| spill | 上下文溢写：压缩前完整转录写盘（data/harness/spill/），列表 IPC | `harness/compaction.rs` |
| examples | 示例即开即用：首次启动种子「示例-只读办公」预设（禁用执行/写入/子代理 + 办公提示词分区） | `harness/preset.rs`（seed_examples） |
| code-runtime / extensions / api / typert / boot / util | 映射记录：代码运行时 = 前端 WebView 动态插件执行（K-2）；扩展 = preset/hooks/mcp 组合；远程 BFF = SDK JSON-RPC（阶段 6）；类型图 = Rust 静态类型 + clippy/rustdoc 门禁；引导 = harness::init；工具 = 零依赖 helper 收敛 | 文档映射（本表） |

验证：harness 单测 39 个；全库 cargo **282 passed**；`e2e-harness-phase78.mjs`
ALL_PASS（14 断言：示例预设种子 / 技能保存与工具注册 / 反馈落库 / 会话查询命中 /
KV 读写删 / spill 溢写文件 / CLI 面板 / 技能 UI / 反馈按钮）；phase1~6 探针回归
ALL_PASS；IPC 契约 380 命令全一致。

### 阶段 8 — 已完成：外围产物（CLI 等价物 / 文档站投影）

| DSH 资产 | 迁移内容 | ST 落点 |
|---|---|---|
| apps/cli | Harness CLI 等价物：`harness_cli` 命令串分发（sessions list / session create / session chat <id> <文本> / session show <id> / tools list / usage <id>）+ 前端 CLI 标签面板 | `harness/sdk.rs`（harness_cli）+ HarnessTab |
| python/ | 评估结论：Python SDK 面向外部集成；ST 侧等价面为 JSON-RPC SDK（阶段 6）+ CLI（本阶段），Python 客户端绑定留待需求驱动 | 文档结论 |
| website/ | 文档站投影：迁移蓝图、阶段记录与 API 清单维护在 docs/harness-migration-plan.md；运行时文档经 ST「API 文档」弹窗 + rustdoc 门禁 | 文档结论 |

## 迁移完成总结

- 8 个阶段全部完成：导航入口、会话核心、工具/审批、治理、编排、执行世界、
  协议连接器、扩展生态与外围产物。
- 全程门禁：cargo fmt/clippy 0 警告 / 单测全绿 / svelte-check 0/0 /
  46 smoke + voice / IPC 契约命令全一致。
- CDP E2E 探针（phase1~6 + phase78 + phase9，覆盖 130+ 断言）全部 ALL_PASS，
  每阶段交付后既有探针回归保持全绿。
- 纯原生零 Node 依赖：DSH 全部能力由 Rust 重写（39 万行 TS 的功能面），
  UI 由 Svelte 5 重建；DSH 源码目录保持只读参考。

## 补全收尾（阶段 9）：清单遗留缺口清零

| DSH 包 | 迁移内容 | ST 落点 |
|---|---|---|
| credentials | 凭据引用能力：键值凭据存储（掩码展示）+ .env 提供者（data/harness/.env）；子进程（hooks/MCP/LSP/shell）统一注入 HARNESS_CREDENTIAL_<KEY>；前端凭据标签 | `harness/credentials.rs` + hooks/mcp/shell/lsp 注入点 + HarnessTab |
| lsp | 语言服务器能力：stdio 客户端（Content-Length 帧）+ initialize/didOpen/hover + `lsp_hover` 模型工具（15 秒硬超时防挂死）+ 服务器配置 IPC + 前端 LSP 标签；E2E 用 PowerShell LSP 测试服务器 | `harness/lsp.rs` + tools/agent + `.codex_tests/lsp-echo-server.ps1` |
| acp | ACP 语义（自动化入口）：session/new（含 goal）/ session/prompt（stopReason=end_turn）/ session/cancel（同步模式说明）经 SDK JSON-RPC 暴露 | `harness/sdk.rs`（dispatch 扩展） |
| code-runtime | 映射收口：代码运行时 = AI 聊天动态插件 WebView 执行（K-2）+ Harness 技能/钩子命令执行；类型图 = Rust 静态类型 + clippy/rustdoc | 文档映射 |

验证：全库 cargo **285 passed**；`e2e-harness-phase9.mjs` ALL_PASS（15 断言：
凭据掩码/.env 提供者/子进程注入/删除、LSP 服务器配置与 hover 回显/未配置优雅
报错、ACP session/new+prompt+cancel、凭据/LSP 标签）；phase1~6 + phase78
探针回归 ALL_PASS；IPC 契约 385 命令全一致。

## 扩展补全（阶段 10）：分叉回放 / 每会话预设 / MCP 管理 / 语音 / PTY

| 方向 | 迁移内容 | ST 落点 |
|---|---|---|
| 会话分叉与回放 | `ctx.sessions.fork(source, boundary)` 语义：复制 `seq <= boundary` 的事件为新会话；`SessionForked { source, boundary_seq }` 落日志（可溯源）；投影消息携带 `seq` 锚点（UI 每消息「分叉」按钮）；`harness_export_session`（Markdown 回放转写，可选写文件） | `harness/session.rs`（fork/export_markdown/flush_md）+ `db.rs`（fork_harness_session）+ HarnessTab |
| 每会话预设作用域 | 会话行新增 `preset_id`（"" = 跟随全局默认预设）；`scope_for_session_id` 贯穿回合上下文/工具目录/派发层（禁用工具在派发层双保险拦截）；头部预设下拉 + `harness_set_session_preset` | `db.rs` + `harness/session.rs` + `harness/preset.rs` + `harness/agent.rs` + HarnessTab |
| MCP 管理 UI + 导入导出 | MCP 标签页（列表/新建/编辑/删除/启停，工具自动注册）；配置束（presets+skills+mcp+lsp+hooks）导出到文件/剪贴板、文件/粘贴导入（按 id 合并同 id 覆盖） | `harness/portability.rs`（新）+ HarnessTab MCP tab |
| 语音对话集成 | 复用 ST 语音栈：助手回复「朗读」（提供方 TTS → SAPI 系统语音兜底，ttsPlayer 状态机）+ 麦克风输入（voiceRecorder VAD → blobToWav16kMono → 本地/云端 STT → 输入框） | HarnessTab + `llm/services/{voice,ttsPlayer,voiceRecorder}` |
| PTY 真终端 | ConPTY（windows crate Win32_System_Console/Pipes，无新依赖）：CreatePseudoConsole + PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE 启动 powershell；读线程收 UTF-8、ANSI 剥离；cwd 标记定位；启动/停止/发送/调整尺寸/状态 IPC；旧系统降级回非 PTY 状态保持终端。关键修复：STARTF_USESTDHANDLES 显式接管子进程 stdout/stderr（防止控制台子系统父进程的重定向句柄泄漏）、stdin 置 NULL（ConPTY 独占输入管道）、\r\n 提交行 + 标记等待完成信号 | `harness/pty.rs`（新）+ `harness/terminal.rs` 协作 + HarnessTab 终端 tab |

验证：全库 cargo **290 passed**（+5：portability 2 + pty 2 + fork 回放 1）；`e2e-harness-phase10.mjs`
**ALL_PASS（47 断言）**：分叉边界/分叉事件溯源/回放导出、每会话预设派发拦截、
配置束导入导出、语音入口、PTY 状态保持（变量跨命令存活=真终端）/尺寸/停止；
phase1/phase78/phase9 回归 ALL_PASS；IPC 契约 395 命令全一致。

## 门禁（每阶段一致）

- Rust：`cargo fmt --check` / `cargo clippy --lib --no-default-features` 0 警告 /
  `cargo test --lib --no-default-features` 全绿（新功能带单测）
- 前端：`npx svelte-check --output human` 0/0、全量 smoke、`smoke-ipc-contract`
- E2E：每阶段新增 CDP 探针（`.codex_tests/e2e-harness-*.mjs`），真实 UI 验证
- 验收截图存 `data/ui-audit/`，vision 复检

## 已建对照

DSH 概念 → ST 落点速查：SessionEvent 日志 → `harness_events` 表；
`ctx.sessions` → `harness/session.rs`；`ctx.tools` → `llm/agent.rs` 注册表
（阶段 2 扩展作用域）；`ctx.llm` → `llm/client`；`ctx.agents` →
`harness/agent`（阶段 2+）。

## 全量核对审计（阶段 10 后，对照 DSH 全部 packages 逐包复核）

6 组并行审计（infra / session / exec / orchestration / connectors / misc），
结论：**covered 21 / partial 23 / missing 5**。

### 整包缺失（missing，需立项）

| DSH 包 | 缺失能力 | 建议优先级 |
|---|---|---|
| jobs | 后台作业运行时：JobRegistry（start/list/read/kill/wait + 完成通知 + 会话隔离）+ 模型工具 job_output/job_list/job_kill + ui-jobs 列表。注：蓝图阶段 4 曾把 schedule.rs 记作 jobs 映射，实为 DSH schedule 包，此处更正 | 高（工具类核心） |
| workspace | 多工作区注册表实体（create/get/list/delete/排序/会话成员/归档）；ST 仅固定 agent_workspace 单目录 | 中 |
| context（agent-instructions / session-reference） | AGENTS.md/CLAUDE.md 加载与动态发现注入、跨会话 @ 引用快照；ST context.rs 仅为目标/计划状态投影，非此概念 | 高 |
| spill（工具输出溢写） | 超限工具结果落盘 + head/tail 预览 + 定位符替换（可检索）；ST 的 spill 实为压缩前转录归档（语义不同） | 中 |
| e2b | E2B 云沙箱适配器（DSH 侧为实验性 POC） | 低（可标记不迁移） |

### 主要能力已落地、有具体缺口（partial，按优先级）

- **fs**：缺 edit / glob / grep / read_image 模型工具（仅 read/write/list_dir）。
- **shell/subprocess**：无后台进程（run_in_background → 作业句柄）、无进程树级终止、8KB 截断后无完整输出取回。
- **terminal**：缺 terminal_open/send/read/signal/close/list 六个模型工具与信号（ConPTY/UI/IPC 已完备，仅模型面缺口）。
- **subagent**：仅一次性全新上下文委派；缺 fork / continuable 后台子代理 / send_message、interrupt_agent 控制工具。
- **workflow**：固定顺序阶段流水线；缺模型编写 JS 编排脚本（agent/pipeline/parallel/phase 组合子）与 Ralph。
- **goal**：仅单目标字符串；缺 create/get/update 工具、状态机（pause/blocked/complete）、revision、轮次预算与自动续跑。
- **interaction**：缺 ask_user_question 工具与 user-questions 接缝、slash 命令注册表、permission-presets。
- **sandbox**：缺三模式文件效应策略（read-only/workspace-write/danger-full-access）与逐调用升级审批；现为布尔 allow_workspace_escape。
- **hooks**：缺 Claude Code / Codex hooks.json 方言（7 生命周期点 + matcher + deny/ask 决策）。
- **mcp**：仅 stdio、工具 schema 置空不透传、无重连与配置项（env/cwd/headers/超时）。
- **lsp**：仅 hover；缺 definition/references/implementation 与按扩展名映射。
- **acp**：仅 session/new|prompt|cancel 三方法；缺 initialize/authenticate、session/update 流式、request_permission，cancel 为说明性 no-op。
- **session-query**：LIKE 关键词搜索；缺 FTS5、血缘 trace、5 个模型检索工具、原始日志 ZIP 导出。
- **compaction**：预算自动压缩已落地；缺 /compact 命令、工具结果剪枝、显式区间压缩。
- **attachment**：文件复制 + 文本预览；缺不可变图片 seam（内容寻址 + 模型 ImageBlock 视觉输入）。
- **storage**：SQLite KV 单后端；缺命名后端注册表（json）与 domain 表单。
- **schedule**：仅固定间隔 + UI 驱动；缺 schedule_create/list/delete 模型工具与 at/after 选择器。
- **feedback**：会话级评分；缺按消息级 sidecar 与 /feedback 命令。
- **plan**：plan_enter/exit + 只读守卫已落地；缺 exit_plan_mode 方案评审流与 /plan。
- **guard**：超时/轮次卫生已落地；缺 repeat-tool-reminder 重复调用提醒。
- **skill**：目录加载 + list/load 工具；缺 frontmatter/provider 注册表/调用策略/会话目录自动注入。
- **extensions**：UI 驱动动态插件已有；缺模型面 cordis_inspect/define/run/stop/undefine 工具集（蓝图已结论为 preset/hooks/mcp 组合，可维持）。
- **code-runtime**：无 run_code 接缝（蓝图已结论映射为 WebView 插件执行，可维持）。

### 已覆盖（covered，核对无误）

core、boot、bundle、api、util、runtime-diagnostics（开发基础设施）、host、client、
session、llm、web、todo、identity、settings、preset、examples、test-support、typert、
credentials、sdk。

## 缺口补齐（阶段 11：按优先级逐项落地）

| 轮次 | 补齐内容 | 落点 |
|---|---|---|
| ① | **jobs**：后台作业运行时（start/output/list/kill + 会话隔离 + 完成状态机）+ exec_command `run_in_background` + 模型工具 job_list/job_output/job_kill + 治理抽屉「作业」tab；**fs 工具** edit_file（字面替换/歧义报错）/glob（**/\*/? 段匹配）/grep（regex file:line）/read_image（base64 视觉引用） | `harness/jobs.rs` + `harness/fs.rs` + tools/agent + HarnessTab |
| ② | **context**：agent-instructions（每回合扫描工作区 AGENTS.md/CLAUDE.md 注入 system-reminder，预算封顶）+ session_ref 跨会话引用工具；**spill**：工具输出溢写（SpillStore 落盘 + head/tail 预览 + locator）+ spill_read 工具 | `harness/instructions.rs` + `harness/spill.rs` + agent 循环 |
| ③ | **workspace**：注册表实体（create/list/delete/status/当前工作区设置）+ workspace_list/create/switch 工具 + 终端/Shell/exec 默认 cwd 锚定；**沙箱三模式**（read-only 守卫 / workspace-write / danger-full-access + 逐调用 sandbox_permissions 升级审批）；**subprocess 进程树终止**（taskkill /T） | `harness/workspace.rs` + settings + shell/pty/jobs kill 路径 |
| ④ | **terminal 模型工具**（open/send/read/signal(Ctrl+C)/close/list）+ **schedule 模型工具**（create 支持 every/一次性 after_seconds、list、delete）+ **guard repeat-tool-reminder**（阈值 [3,5,8] 注入） | tools/agent + terminal/pty + schedule.rs |
| ⑤ | **subagent fork 语义**（分叉子会话 + 后台运行 + send_message 跟进 + interrupt_agent 中断 + subagent_list/output）；**goal 生命周期状态机**（create/get/update：pause/resume/complete/blocked/edit + revision + max_goal_rounds，GoalUpdate 事件落日志） | `harness/subagent.rs` 扩展 + session GoalUpdate + agent 派发 |
| ⑥ | **interaction**：ask_user_question 工具 + user-questions 接缝（事件 + UI 问题卡 + 回答 IPC）+ slash 命令（/plan /exit /goal /feedback /compact /skill /help）+ plan_exit 方案评审流 + **feedback 消息级**（message_seq 列 + 按消息打分） | `harness/interaction.rs` + HarnessTab 问题卡 + agent slash 分派 + db/feedback |
| ⑦ | **hooks CC/Codex 方言**（SessionStart/UserPromptSubmit/PreToolUse/PostToolUse/Stop/Subagent 事件 + matcher 匹配器 + PreToolUse deny/ask 决策拦截）；**MCP schema 透传**（tools/list inputSchema 注册进工具目录）；**LSP** definition/references/implementation + 扩展名路由（extensions 字段） | hooks.rs + mcp.rs + lsp.rs + 前端表单 |
| ⑧ | **ACP 补全**（initialize/authenticate/session/update 流式等价/session/request_permission；cancel 真中断）；**session-query**（多词 AND 检索 + traceSession 血缘 + session_search/session_trace 工具）；**compaction**（/compact 命令 + prune_tool_results 工具结果剪枝） | sdk.rs + db/session + compaction.rs + tools |
| ⑨ | **attachment 图片 seam**（sha256 内容寻址对象 + 上下文图片引用提示 + attachment_list 工具）；**storage 命名后端**（default=SQLite / json:<名称> 文件后端 + 名册 IPC）；**skill frontmatter**（name/description/disable-model-invocation 解析 + 模型调用门控 + /skill 手势注入） | attachment.rs + storage.rs + skill.rs |

验证：cargo **306 passed**；`e2e-harness-phase11.mjs` **ALL_PASS（45 断言）**；
phase1~6 + phase78 + phase9 + phase10 回归 ALL_PASS；svelte-check 0/0；
IPC 契约命令全一致。

### 最终回归修正（阶段 11 收尾）

全量回归发现 4 组失败，逐一修复后 10 探针全部 ALL_PASS：

- **phase3 超时回合/工具超时守卫**：模型在 1 秒超时后改走 `run_in_background` 后台作业重试并轮询
  job_output，耗尽 max_agent_rounds 后回合以裸 Err 结束（无最终消息、工具步骤无法投影）。
  修复：轮次用尽时合成收尾消息（含伴随文本兜底），走正常 chunk/AssistantMessage/done 收尾，
  工具步骤挂到最终消息上（`agent.rs`）。
- **phase9 session/cancel**：cancel 由同步 no-op 升级为真中断（request_cancel + `cancelled:true`），
  探针断言同步为新语义。
- **phase78 技能 UI**：前序探针遗留打开的治理抽屉导致 openDrawer 全量刷新被跳过（技能列表陈旧）。
  探针改为 close-then-open 强制刷新，并在各探针结束关闭抽屉。
- **phase10 MCP/终端 tab**：phase9 遗留打开态使 phase10 的治理开关点击变成「关闭抽屉」，
  4 个 tab 断言全失败。探针同样改为 close-then-open；终端会话按钮校验恢复。

### 维持既有结论（不迁移，文档映射）

- **e2b**：实验性 POC，标记不迁移。
- **code-runtime**：run_code 接缝 → WebView 动态插件执行（agent_plugins.rs）。
- **extensions**：模型面 cordis 工具集 → preset/hooks/mcp 组合 + UI 驱动动态插件。
- **session-log-export ZIP / FTS5 / goal-round-driver 自动续跑 / workflow JS 编排与 Ralph**：
  以 Markdown 导出、多词 AND 检索、schedule 周期提示与固定阶段流水线为等价替代。

## AI 聊天并入 Harness 会话（板块移除）

- **目标**：原独立「AI 聊天」板块整体移除，其能力并入 Harness 会话本体
  （两者均为 AI 对话，统一为一个界面）。Harness 会话原已具备：流式输出、
  语音输入（麦克风）与回复播报（TTS）、多模态附件、历史持久化、工具调用与治理审批、
  图表渲染（共享 MessageBody）——并入后补齐最后一项缺口：**AI 角色注入**。
- **落点**：
  - `session.rs`：新增 `RoleSet { name, prompt }` 事件（落日志，模型可见 ⟺ 落日志）；
    `set_role` / `role` / `role_from_events` 投影；IPC `harness_set_session_role` /
    `harness_get_session_role`。
  - `agent.rs`：回合组装系统提示词时把会话角色作为 `[AI 角色：<name>]` 最高优先级分区注入。
  - `HarnessTab.svelte`：头部新增「AI 角色」选择器（复用 `llmApi.getAiRoles()` +
    `composeSystemPrompt`），选择即持久化到会话，切换会话按日志投影回显；
    移除上一轮引入的视图切换条与内嵌 GlobalChatTab 子视图（单一聊天界面）。
  - `App.svelte`：移除 harnessView 绑定；`navigateToTab('ai_chat')` 直接进入 Harness；
    概览页卡片更名「AI 对话」直达 Harness 会话。
  - 清理：`AiChatPanel.svelte` 保留为备用入口；14 个断言原独立面板 UI 的旧 e2e 脚本
    头部标记停用；`ui-audit-all.mjs` / `verify-nav-cdp.mjs` 维持 13 项导航。
- 验证：整合校验 8 断言 ALL_PASS；功能校验（角色应用/持久化/role_set 事件/真实收发/清除）
  ALL_PASS；全量 10 探针回归 ALL_PASS；svelte-check 0/0；smoke 46/0；
  cargo 306 passed。

## DSH 统计条（会话遥测）迁移

- **目标**：复刻 DSH 会话顶部的遥测统计条——「X 轮 · Y 步 | LLM 墙钟 · 工具调用墙钟 |
  首 token 平均 · tok/s | 缓存命中率 | 输入/输出 token 总量 | 成本」。
- **数据链路**（模型可见 ⟺ 落库）：
  - `llm/client/chat.rs`：`chat_completion_with_tools_raw` 改为返回
    `CompletionWithTools`（新增 wall_ms 墙钟、first_token_ms 首字节延迟——
    响应体首个网络块到达时间，非流式下的 TTFT 代理；cached_tokens 解析
    OpenAI `prompt_tokens_details.cached_tokens` 与 DeepSeek
    `prompt_cache_hit_tokens`）。
  - `db.rs`：`harness_usage` 新增列 llm_wall_ms / first_token_ms / requests /
    cached_tokens / tool_wall_ms（含旧库 ALTER 迁移）；聚合查询返回 8 元组。
  - `harness/agent.rs`：回合内累计 LLM 墙钟、首 token 合计、请求数、缓存命中、
    工具墙钟（工具时长同时从事件日志投影）；`record_usage` 失败不再静默（log::warn）。
  - `harness/session.rs`：`HarnessUsageSummary` 扩展 steps / llm_wall_ms /
    tool_wall_ms / first_token_avg_ms / tokens_per_sec / cache_hit_rate /
    input_tokens / output_tokens（步数与工具墙钟从事件日志投影）。
  - `HarnessTab.svelte`：头部下方新增 `.hns-stats` 统计条（DSH 风格分隔符排版，
    tabular-nums），回合完成后随 usage 刷新。
- 踩坑记录：旧库 ALTER 迁移漏掉 tool_wall_ms 列导致 INSERT 静默失败
  （`.ok()` 吞错），统计条全零——补迁移并给记录失败加日志后恢复。
- 验证：功能校验统计条真实数值 PASS（LLM 1s / 首 token 0.9s / 18 tok/s /
  缓存命中 29% / 输入 13K tok）；全量 10 探针回归 ALL_PASS；svelte-check 0/0；
  smoke 46/0；cargo 306 passed。

## Harness 会话真流式输出

- **背景**：原实现走 `chat_completion_with_tools_raw`（`stream:false`），
  整段回复一次性下发（phase1 探针曾记录「回答过快，未捕捉到流式指示（单块下发）」）。
- **落点**：
  - `llm/client/chat.rs`：新增 `chat_completion_with_tools_stream`——`stream:true` +
    `include_usage`，SSE 逐行解析；正文增量经 `on_delta` 回调；`delta.tool_calls`
    按 index 合并分片（缺 id 时合成，保证 ToolResult 可关联）；reasoning_content
    只收集不流入（无正文时回退）；usage/缓存命中/tool 遥测与 raw 版一致。
  - `harness/agent.rs`：工具循环改用流式调用，每个正文增量即时下发
    `assistant_chunk(done:false)` 事件；收尾 chunk 事件只发空 delta（正文已逐段
    渲染，避免 streamBuf 重复拼接），日志仍落完整权威 chunk（回放同源）。
  - 前端无需改动（streamBuf 本就支持逐 delta 追加）。
- 验证：`verify-harness-streaming.mjs`——长回复采样捕获 13 个渐进文本快照
  （逐 delta 渲染实证）、回复完整落消息、首 token 遥测真实值（0.9s）；
  全量 10 探针回归（含工具调用回合的流式分片合并）ALL_PASS。

## 会话维护能力 + 工作路径放大（自维护）

- **背景**：模型此前无任何会话管理工具（无法自清聊天记录），且 fs/exec 沙箱
  锚定在 data/agent_workspace，读不到自身源码。
- **会话维护**（`session.rs` / `agent.rs` / `tools.rs`）：
  - 新事件 `SessionCleared`（清空动作落日志，模型可见 ⟺ 落日志）；`SessionStore::clear_messages`
    （删事件+用量，保留会话/预设/角色）+ IPC `harness_clear_session`。
  - 模型工具 5 个：`session_list`（只读）/ `session_create` / `session_rename` /
    `session_clear`（清当前或指定会话）/ `session_delete`（需审批，内联审批卡）。
  - 前端：会话侧栏新增「清空聊天记录」按钮（Eraser 图标 + 确认）；done 事件后
    按日志投影重载消息（模型端清空/删除立即同步 UI），当前会话被删时自动落到可用会话。
- **工作路径放大**（`workspace.rs` / `fs.rs` / `shell.rs` / `instructions.rs`）：
  - 默认工作区（dir=""）从 data/agent_workspace 放大为**应用项目根**
    （dev = E:\ST\st_control）——exec_command/终端 cwd、fs 读改写、
    glob/grep、AGENTS.md 指令扫描全部跟随；显式创建的工作区仍在
    agent_workspace 下；沙箱三模式与越界审批语义不变。
  - glob/grep 全树扫描跳过重目录（target/node_modules/.git/dist/.svelte-kit/build）。
  - phase5 探针同步新边界（终端初始 cwd = 项目根；越界用例用项目根外路径）。
- 验证：`verify-harness-session-maintain.mjs` 16 断言 ALL_PASS——exec 锚定项目根、
  读自身源码 package.json、根外仍被拒、5 工具注册、UI 清空按钮、
  **模型真实调用 session_clear 自清会话**（预置历史无残留 + session_cleared 落日志 +
  UI 同步 + 确认回复）；全量 10 探针回归 ALL_PASS；svelte-check 0/0；smoke 46/0；
  cargo 306 passed。

## Harness 界面重设计（布局与组件 UI）

- **目标**：重设计 Harness 会话的布局与组件视觉，重点是「治理中心」与「工具目录」。
  沿用应用设计令牌（--app-color-*），Harness 内部派生 --hns-* 语义变量；
  所有探针依赖的类名与 tab 文案保持不变（回归兼容）。
- **治理中心**（原浮动抽屉 → 全高右侧面板）：
  - 全高右侧滑出面板（470px，毛玻璃 + 左侧阴影 + 入场动画），头部「治理中心」标题/副标题/关闭。
  - tab 改图标网格：6 列网格 + 分组标签（基础：设置/钩子/预设/定时/工作流；
    执行：终端/作业；内容：技能/CLI；集成：凭据/LSP/MCP），tab 文案保持原样。
  - 表单升级为卡片式字段（标签 + 输入 + focus 高亮），列表项 hover 抬升，
    主按钮渐变强调、次要按钮描边。
- **工具目录**（原平铺列表 → 可检索目录）：
  - 头部：标题 + 计数（N 个工具 · M 需审批）+ 搜索框（名称/说明过滤）。
  - 按功能族分组（会话管理/文件与内容/执行环境/信息检索/编排与协作/
    技能与语言服务/系统与集成），组标题 + 数量徽标。
  - 每项可点击展开**参数 schema**（后端 HarnessToolInfo 新增 parameters 字段）；
    需审批工具带琥珀色徽标；描述两行截断。
- **整体**：主区径向渐变背景；头部毛玻璃 + 标题发光点；侧栏会话卡片
  （激活 = 强调色内轨 + 描边）；消息气泡非对称圆角 + 深浅渐变；
  审批/计划/目标横幅左色轨；输入条悬浮毛玻璃 + focus 光环；统计条、
  终端、待办、附件、反馈等全部统一到新令牌体系。
- 验证：4 个 UI 关键探针（phase1/3/10/78：会话聊天、治理抽屉、MCP/终端 tab、
  技能/CLI tab）ALL_PASS；全量 10 探针回归 ALL_PASS；svelte-check 0/0；
  smoke 46/0；cargo 306 passed；视觉走查截图（治理/工具目录/主界面）无重叠溢出。

## 工具调用执行显示（时间线设计）

- **设计原则**：工具调用发生在回复之前（先工具执行 → 结果 → 最终回复），
  显示位置与真实时序一致 —— **工具执行时间线置于回复气泡上方**，作为
  「回合」整体呈现：时间线（工具步骤）→ 回复气泡 → 反馈条。
- **历史消息**：`.hns-tool-timeline` 垂直时间线卡片 —— 左侧竖线连接各步骤
  节点到回复气泡（线性渐变，语义"执行流汇入回答"）；每步一个圆形状态节点
  （完成绿 ✓ / 失败红 ✕ / 执行中强调色脉冲点），行内 = 等宽工具名 + 参数摘要
  + 状态徽标 + 时长 + 展开 chevron；点击展开参数与结果（复制按钮保留）。
  失败与重试序列如实展示（失败红节点 + 后续成功节点）。
- **实时回合**：liveTools 时间线与流式回复气泡**合并为同一容器**
  （原先是两个分离的消息块），工具执行中/完成 + 逐字流式输出在同一回合内
  连续呈现；done 后回合并入历史（同构渲染，日志回放一致）。
- 兼容：探针依赖的类名（hns-tool-step/head/name/pre/running）与点击展开交互保留。
- 验证：`verify-tool-timeline.mjs` 9 断言 ALL_PASS（时间线在气泡上方、
  状态节点含失败/完成、展开详情、实时回合同容器合并、完成后落消息）；
  phase1/2（工具步骤出现/完成/展开）等全量回归 ALL_PASS；
  verify-no-duplicate / verify-harness-streaming 复验 ALL_PASS。

## 补全收尾（阶段 11）：Harness 代理接入 extensions + code-runtime

核对结论：DSH `extensions`（模型自修改运行时）与 `code-runtime`（Code Mode /
`run_code`）此前仅映射到「AI 聊天动态插件（K-2）」，**Harness 代理工具面未接入**。
本阶段把该能力补进 Harness 会话，DSH 包映射从「文档映射」升级为「直接实现」：

| DSH 包 | 迁移内容 | ST 落点 |
|---|---|---|
| extensions（tool-cordis） | `plugin_list` / `plugin_define` / `plugin_delete` / `plugin_enable` / `plugin_disable` 模型工具：定义/启停/删除动态插件（版本历史复用 llm/agent_plugins） | `harness/tools.rs`（规格）+ `harness/agent.rs`（handle_session_tool 派发 + 计划模式守卫 + 审批门控） |
| code-runtime | `run_code` 模型工具：async 函数体在前端 WebView 沙箱执行（args + ctx.fetch/ctx.log），返回日志与返回值 | `harness/tools.rs` + `harness/agent.rs` + `HarnessTab.svelte`（harness-tool-exec-request 监听 + 执行器） |
| extensions（client runner） | 启用插件的工具并入 Harness 模型工具目录（插件优先遮蔽同名内置），调用经前端执行桥回传 | `harness/tools.rs`（tools_json_scoped 插件优先）+ `llm/agent_plugins.rs`（run_plugin_tool_on 事件名参数化，独立 harness-tool-exec-request 通道避免双监听） |

- 前端：HarnessTab 治理抽屉新增「插件」标签（列表/新建/编辑/启停/删除，
  复用 `llmApi` 插件 IPC）；`harness-tool-exec-request` 监听与 AI 聊天同语义
  的 WebView 执行器。
- 复用：`llm/agent_plugins.rs` 提取同步助手 `define_plugin` / `delete_plugin` /
  `set_enabled`（AI 聊天与 Harness 共用）；测试期持久化路径可重定向，避免污染
  真实 `data/plugins/plugins.json`。
- 验证：harness/tools.rs + llm/agent_plugins.rs 各 +1 单测（工具注册与只读守卫 /
  define→disable→enable→delete 回环）；全库 cargo **308 passed**；
  svelte-check **0 errors / 0 warnings**；IPC 契约 407 命令全一致。
