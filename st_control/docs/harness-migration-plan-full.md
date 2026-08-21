# DeepSeek Harness → ST「Harness 会话」完整迁移计划（零遗漏版）

> 状态：计划（研究已完成，实施待启动）
> 依据：`E:\ST\deepseek-harness-master`（DSH，Node/Cordis 全插件化 monorepo）全量功能面，
> 对照 `E:\ST\st_control` 现有「Harness 会话」实现（已迁移 0-11 阶段 + 多项增强）。
> 配套盘点文档（只读研究产出，实施时的验收底稿）：
> - `E:\ST\docs-migration\01-st-harness-现状盘点.md`（ST 前端/接口/后端现状）
> - `E:\ST\docs-migration\02-dsh-web-ui-界面盘点.md`（DSH 31 个 ui-* 包 Slot 与界面清单）
> - `E:\ST\docs-migration\03-dsh-工具与host能力盘点.md`（DSH 工具 53+ / host / LLM / 协议）
> - `E:\ST\docs-migration\04-dsh-核心UI细节深读.md`（界面验收清单：组件/文案/交互）

## 0. 结论先行

1. **DSH 是"全插件化"的 agent harness**：约 60 组 package（1,981 个 TS 文件 / 39.4 万行），
   模型工具约 53 个（5 大族 + 运行时自修改族 + MCP/动态插件动态扩面），Web UI 由 30+ 个
   `ui-*` 包组成，Host 侧是 HTTP+SSE 网关（session/workspace/settings/credentials/llm/
   preset/command/skill 全域 RPC）+ stdio JSON-RPC SDK + ACP 自动化服务器。
2. **ST 已迁移约 80% 高价值能力**：会话核心、工具/审批、治理（guard/hooks/preset/
   telemetry）、编排（subagent/workflow/todo/plan/goal/schedule/jobs）、执行世界
   （fs/shell/terminal/PTY/沙箱/workspace）、连接器（web/context/compaction/attachment/
   storage/spill/sdk/mcp/lsp/credentials/skill/feedback/session-query/CLI/插件+run_code）。
3. **本计划的目标 = 把剩余 ~20% 全部清零**：按"零遗漏"原则，对 DSH 全功能面逐包、逐界面
   复核，列出**未迁移/部分迁移**项，分 6 个阶段实施，每阶段带验收清单与门禁。

---

## 1. DSH 全功能面（迁移目标全量清单）

> 权威索引：`packages/bundle/base/cordis.patch.yml`（host 面插件行）+ `packages/bundle/web-app/
> cordis.patch.yml`（Web 面插件行）+ `docs/tool-catalog.md`。此处按"功能族"汇总。

### 1.1 模型工具面（53 静态 + 动态扩面）

| 族 | 工具 | 源包 |
|---|---|---|
| 执行/进程 | bash、pwsh、bash(持久)、run_code、terminal_open/list/read/send/signal/close、job_list/output/kill | shell/subprocess/terminal/jobs/code-runtime |
| 文件/发现 | read、write、edit、read_image、glob、grep、str_replace_editor | fs |
| 编排/多代理 | subagent、subagent_fork、interrupt_agent、list_agents、send_message、report、workflow、ralph、todo_write | subagent/workflow/todo |
| 目标/计划/调度 | create_goal、get_goal、update_goal、exit_plan_mode、schedule_create/delete/list | goal/plan/schedule |
| 知识/检索/问答 | skill、session_event_read/search/trace、session_search/trace、lsp、web_search、web_fetch、ask_user_question | skill/session-query/lsp/web/interaction |
| 运行时自修改（opt-in） | cordis_define/inspect_list/inspect_query/inspect_self/run/stop/undefine | extensions |
| 动态 | mcp__<server>__<tool>、动态插件工具 | mcp/extensions |

### 1.2 Host 服务面

- webserver（HTTP+SSE+升级路由）、apiproxy（四象限 RPC 网关）、frontend-static、
  directory-picker（native/browse/auto）、plugin-inventory。
- RPC 域：session.*（history/fork/models/selectModel/prompt/rename/search/updateQueue/
  cancel/create/export/list）、host.*（pickDirectory/listDirectory/createDirectory/openPath/
  describe）、workspace.*、agentPreset.*、command.*、skill.*、settings.*、credentials.*、
  llm.*、subagent.prompt。
- 协议面：SDK（stdio JSON-RPC 2.0：initialize/session/prompt/shutdown + 4 通知）、
  ACP（initialize/authenticate/session/new/prompt/cancel/update/request_permission）。
- LLM 面：ctx.llm 统一流式词汇；llm-deepseek（直连）+ llm-pi-ai（openai-completions/
  openai-responses/anthropic/deepseek 等协议，目录机制、/models 发现、凭据引用、
  reasoningEfforts 思考级别、图像模态声明、默认上下文窗口）；llm-retry（提供商级重试）；
  token-meter。

### 1.3 Web UI 面（30+ ui-* 包）

三栏框架（侧栏/会话列/详情列）+ 会话头（面包屑 + 对话|轨迹标签页）+ 输入区
（命令菜单、plan 芯片、权限选择、模型+推理等级两级选择器、上下文环形仪表、排队坞、
统计条）+ 设置模态（通用/模型/插件/Agent 预设四分区）+ 消息流节点体系（用户/助手
Markdown+KaTeX/推理 Think 行/上下文注入行/工具调用树+7 种工具卡/压缩卡/重试卡/回合
错误卡/回合尾+产物文件）+ 轨迹视图（表格/时间线 + 请求检查器）+ 侧栏工作区浏览器
（树/扁平、搜索、拖拽排序）+ 交互卡（审批接管/提问卡多题分页/计划审阅/反馈👍👎+备注）
+ 附件（导轨/拖放/灯箱）+ 目录选择浏览器（Miller 列）+ 主题（浅/深/跟随系统）。

### 1.4 预设与示例面

- 预设：standard（标准）/ code（PTC 代码）/ minimal（极简）/ cordis（自修改）
  + 用户自定义预设；agent.cordis.yml 组装；persona。
- 示例：headless-agent / acp-agent / jsonrpc-agent / mcp-memory / web-cordis / web-schedule。

---

## 2. ST 现有实现（已完成面）

> 详见 `E:\ST\docs-migration\01-st-harness-现状盘点.md`。核心：34 个 Rust 模块、78 个
> harness IPC、13 个治理 tab、工具目录面板、DSH 统计条、真流式、AI 聊天并入、语音。

---

## 3. 差距分析（零遗漏核对）

### 3.1 界面面差距（DSH 有 / ST 无或形态不同）

| # | DSH 界面 | ST 现状 | 差距等级 |
|---|---|---|---|
| U1 | **轨迹视图**（对话\|轨迹标签页：表格/时间线双视图、请求检查器：请求/消息/工具/系统更新/压缩 5 类详情标签、Timing/Usage 面板、虚拟滚动、搜索） | 无 | **高** |
| U2 | **详情面板**（第三列：选中工具调用的输入 JSON + 输出卡/终端卡） | 工具行内展开 | 中 |
| U3 | **计划评审流**（exit_plan_mode → 计划待审卡：确认执行/拒绝/去聊天里说） | plan_exit 直接退出 | **高** |
| U4 | **工具调用树 + 7 种工具卡**（bash 终端卡/read 读文件卡/diff 卡/search 卡/web 卡/todo 行/ask 行 + GenericToolCard + 递归 subCalls + 文件路径可点击打开） | 通用展开（参数+结果） | **高** |
| U5 | **回合尾 + 产物文件**（turn-tail：回合级操作条 + ProducedFiles 文件 chip 点击打开） | 消息级操作条、无产物 | 高 |
| U6 | **推理展示**（assistant 消息的 Think 可展开行，流式摘要跟随） | 收集但不展示 | 中 |
| U7 | **上下文注入行**（技能目录/AGENTS.md/会话回忆 → 可展开行） | 无 | 中 |
| U8 | **上下文环形仪表**（发送键旁占用 % + 系统/工具/消息细分面板） | 无 | 中 |
| U9 | **输入排队**（忙时排队 + QueueDock 编辑/删除/插话 + 排队操控气泡 + Busy-Enter 设置） | 发送即阻塞 | 高 |
| U10 | **斜杠命令与 @ 引用菜单**（/goal /plan /feedback /compact /model /skill /exit /help… + 技能/子代理引用） | 无 | **高** |
| U11 | **模型座位两级选择器**（模型 + 推理等级 effort；provider default/off/high/max） | 简单下拉 | 高 |
| U12 | **会话级权限芯片**（read-only/workspace-write/Full access + RiskConfirmation 勾选） | 设置项下拉 | 中 |
| U13 | **目标条 GoalBar**（输入坞：暂停/恢复/编辑内联/清除 + /goal 命令输入视图） | 只读横幅 | 中 |
| U14 | **后台任务下拉**（会话头部，仅在有作业时出现） | 治理抽屉作业 tab | 中 |
| U15 | **子代理目录 + 只读 composer**（会话头 catalog 动作树、one-shot/父离线只读输入条、面包屑谱系） | 无（子代理=一次性 task） | 高 |
| U16 | **工作流运行面板**（对话内节点：阶段/成员状态、点击打开成员会话） | 固定阶段流水线落日志 | 中 |
| U17 | **工作区浏览器**（树/扁平、会话搜索防抖、拖拽排序、每工作区折叠 5 个、视图选项、添加/重命名/删除/归档） | 工作区列表 + 扁平会话侧栏 | **高** |
| U18 | **设置模态四分区**（通用：权限/Agent 预设/输入行为/外观；模型：提供方管理+自定义提供方表单+模型目录+获取可用模型+首次引导；插件：配置卡+清单；Agent 预设管理） | 治理抽屉 + llm 板块分散 | **高** |
| U19 | **重试倒计时卡 + 回合失败/输出上限节点** | 无 | 中 |
| U20 | **消息图片画廊/灯箱/整页拖放遮罩/附件导轨** | 附件 chip + 简单图片 | 中 |
| U21 | **侧栏折叠 56px 竖轨 + 列宽拖拽** | 侧栏固定 | 低 |
| U22 | **主题三态（浅/深/跟随系统）+ 双语 locale** | ST 全局主题（中文） | 低 |
| U23 | **命令面板 PopupSelectView**（/model 等需要选项的命令） | 无 | 中 |
| U24 | **空态英雄细节**（FishLogo/探索未至之境/预览版徽标/工作区 chip/Agent 预设座位） | 有简化 hero | 低 |
| U25 | **目录选择浏览器**（Miller 双列/路径编辑/新建文件夹/显示隐藏） | tauri 原生对话框 | 低 |
| U26 | **plan 输入芯片**（输入框内 Plan 徽标 + × 关闭 = /plan off） | 计划横幅（对话区） | 低 |
| U27 | **/permission 命令**（popupSelect 选权限预设，Full access 需风险确认） | 无 | 中 |
| U28 | **hero Agent 预设座位**（新会话选择预设 chip）+ 会话头预设标签 | 头部预设下拉 | 低 |
| U29 | **/skill 斜杠候选**（slash 菜单技能组 + pick 插入 `/name ` 文本） | 无（属 U10 一并做） | 中 |

### 3.2 能力面差距（后端/协议）

| # | DSH 能力 | ST 现状 | 差距等级 |
|---|---|---|---|
| B1 | **subagent fork/continuable**（后台子代理、send_message/interrupt_agent/list_agents、会话即子代理、report 通道） | 一次性 task + 简化控制工具 | **高** |
| B2 | **workflow JS 编排**（agent/pipeline/parallel/phase 组合子、worker-thread）+ **ralph** | 固定阶段流水线 | 高 |
| B3 | **goal-round-driver 自动续跑**（回合结束自动继续至完成/阻塞/轮次上限） | 状态机无自动续跑 | 高 |
| B4 | **session-query 5 工具 + FTS + 血缘 trace + ZIP 导出** | LIKE 搜索 + 2 工具 | 中 |
| B5 | **compaction /compact 命令 + 工具结果剪枝**（阈值/头尾保留） | 自动压缩 + prune | 低 |
| B6 | **mcp schema 透传 + 重连 + env/cwd/headers/超时配置** | stdio + 空 schema | 中 |
| B7 | **lsp 4 操作 + 扩展名路由** | 已实现 4 操作 | 低（已覆盖） |
| B8 | **ACP 7 方法完整**（initialize/authenticate/session/update/request_permission） | 3 方法 | 中 |
| B9 | **hooks CC/Codex 方言**（7 生命周期 + matcher + deny/ask 决策） | 10 事件 + matcher | 低 |
| B10 | **attachment 图片 seam**（sha256 内容寻址 + 图片输入模态声明 + 发送前校验） | 文本附件 | **高** |
| B11 | **sandbox 三模式 + 逐调用升级审批**（权限预设 read-only/workspace-write/danger-full-access） | 模式存在、升级审批存疑 | 中 |
| B12 | **settings 命名空间分层 + 热提交 + schema 校验** | 简单设置文件 | 中 |
| B13 | **storage 命名后端**（default=json/sqlite、storage-domain 表单） | SQLite KV | 低 |
| B14 | **skill frontmatter + provider 注册表 + 调用策略 + /skill 手势** | 目录加载 + 门控 | 中 |
| B15 | **LLM 多提供商目录**（pi-ai 协议集、/models 发现、reasoningEfforts、图像模态、目录模型元数据） | OpenAI 兼容手配 | **高** |
| B16 | **llm-retry 提供商级重试**（agent/request-error 监听 + 重试卡） | 传输层重试 | 中 |
| B17 | **web 搜索提供商缝**（deepseek/exa/perplexity + web_fetch） | Bing 双域 | 中 |
| B18 | **SDK/ACP 线协议完整**（initialize/session.prompt/shutdown 通知） | JSON-RPC 部分 | 低 |
| B19 | **session-title-llm**（LLM 生成标题，多词目标） | 首条消息投影 | 低 |
| B20 | **telemetry otel 上报**（opt-in，匿名 id） | 本地用量 | 低（可标记不迁移） |
| B21 | **插件配置卡**（agent-loop/bash/web 等部署参数的表单化配置） | 动态插件 UI | 中 |
| B22 | **e2b** | 无 | 维持不迁移 |
| B23 | **run_code 工具子调用**（await tools.name() 调其它工具、code-mode 保留传输） | run_code 仅有 ctx.fetch/ctx.log | 中 |
| B24 | **str_replace_editor 工具**（view/create/str_replace/insert 四命令编辑器） | **已实现**（第 21 轮）：`tool_str_replace_editor` 四命令完整（view 带行号+view_range、create 不覆盖、str_replace 唯一匹配列行号、insert 行后插入；16K 字符边界截断 + `<response clipped>`），fs.rs 4 方法 + 2 单测 | 低（已覆盖） |

### 3.3 维持既有结论（明确不迁移，文档映射）

e2b（实验 POC）、code-runtime run_code 映射为 WebView 插件执行（已实现）、
telemetry otel（本地统计等价）、目录选择 native（tauri 原生对话框等价）、
主题与 locale（ST 全局主题）、session-log-export ZIP（Markdown 导出等价）。

---

## 4. 迁移计划（阶段划分）

> 原则：纯原生重写（Rust + Svelte 5），DSH 源码只读参考；模型可见 ⟺ 落日志；
> 能力接缝三角色成组迁移；每阶段门禁：cargo fmt/clippy 0 警告、cargo test 全绿、
> svelte-check 0/0、smoke 全绿、新增 CDP E2E 探针 + 既有探针回归。

### 阶段 12 — UI 信息架构升级（对应 U1/U2/U5/U14/U15/U17/U18/U21/U24/U28）

> **实施记录（2026-08-19）**：已落地 7/8 项——
> - ✅ 会话头「对话 | 轨迹」标签页（`switchView` 按需加载）
> - ✅ 轨迹视图：后端 `harness/session.rs` 新增 `TrajectoryEntry/HarnessTrajectory` 投影
>   + IPC `harness_trajectory`（+2 单测）；前端新组件 `components/TrajectoryView.svelte`
>   （按轮分组/搜索/折叠/耗时切换/工具详情展开）
> - ✅ 详情面板（U2）：右侧滑出面板 `hns-details`，工具行「面板」按钮 → 输入/输出
> - ✅ 回合尾产物（U5）：后端 `turn_files()`（edit_file/write_file 成功路径去重，+1 单测）
>   + IPC `harness_turn_files`；前端产物 chips + `harness_open_path`（cmd start 模式）
> - ✅ 工作区浏览器轻量版（U17）：`harness_sessions.workspace_id` 列（ALTER 迁移）、
>   `create_in_workspace`/`set_workspace` + IPC `harness_create_session(workspace_id)` /
>   `harness_set_session_workspace`（+1 单测）；侧栏按工作区分组（组头折叠/数量）、
>   新建会话归属激活工作区
> - ✅ 侧栏折叠（U21）：52px 竖轨 + 圆点指示
> - ✅ 空态英雄（U24）：🐋 品牌标 + 「预览版」徽标 + 能力说明
> - ⏳ 设置模态（U18）：治理抽屉已覆盖等价功能（权限/预设/插件等），形态改造
>   随阶段 13 输入行为设置一并处理
> 门禁：cargo 312 passed / clippy 0 / fmt 0 / svelte-check 0/0 / IPC 契约 412 命令一致。

| 任务 | 落点 |
|---|---|
| 会话头升级：面包屑（子代理谱系）+ 对话\|轨迹标签页 + 后台任务下拉（U14）+ 子代理目录（U15）+ 预设标签（U28） | HarnessTab 头部区 |
| **轨迹视图**：后端日志投影出"请求/消息/工具/系统更新/压缩"台账（复用现有 harness_events），前端表格 + 时间线双视图、搜索、行检查器（请求：Summary/Options/Usage/Timing；消息：Preview/Raw；工具：Payload/Result/Schema/Timing） | 新 `TrajectoryView.svelte` + session.rs 投影 + IPC `harness_trajectory` |
| **详情面板**：第三列（可拖拽宽度），工具行点击 → 输入 JSON + 输出卡（U2） | HarnessTab 布局扩展 |
| **回合尾 + 产物文件**：从工具日志投影变更文件（edit/write/read 路径），回合结束渲染 chips + 打开文件 IPC（U5） | turn-tail 渲染 + fs.rs `open_path` |
| **工作区浏览器**：树/扁平视图、搜索防抖、拖拽排序（写回顺序）、每工作区折叠、会话菜单（重命名/分叉/归档）（U17） | 侧栏重构 + workspace.rs 排序/归档 IPC |
| **设置模态**：通用（权限默认/Agent 预设/输入行为/外观）+ 模型（提供方管理表单化）+ 插件（配置卡+清单）+ Agent 预设管理（U18） | 新设置面板组件 + 复用 llm 提供方 IPC |
| **侧栏折叠 56px 竖轨 + 列宽拖拽**（U21） | HarnessTab 布局 |
| **空态英雄细节**：品牌标 + 预览版徽标 + 工作区 chip + Agent 预设座位（U24） | HarnessTab hero |

验证：新探针 `e2e-harness-phase12.mjs`（轨迹台账/检查器/产物 chips/面包屑/工作区树/
设置模态）；回归 phase1~11。

### 阶段 13 — 输入区与消息流节点体系（对应 U3/U4/U6/U7/U8/U9/U10/U11/U12/U19/U20/U23/U26/U27/U29）

> **实施记录（2026-08-19）**：
> - ✅ 计划评审流（U3）：**既有已实现**（阶段 11：plan_exit 携带 plan → ask_user 评审「批准执行/继续修改」，未批准保持计划模式）——核实为已覆盖
> - ✅ 推理 Think 行（U6）：`llm/client/chat.rs` 流式回调改 `FnMut(&str, Option<&str>)`
>   透传 reasoning 增量；`HarnessEvent::AssistantMessage` 新增 `reasoning` 字段（serde
>   default 向后兼容旧日志）+ `DisplayMessage::Assistant.reasoning` 投影（+1 单测）；
>   agent.rs 累计推理随最终消息落日志；前端实时/历史 Think 折叠行（💭 思考中…/展开）
> - ✅ 权限芯片（U12）：输入区三模式下拉（只读/工作区写入/完全访问）+ Full access
>   风险确认（RiskConfirmation 语义）；/permission 命令前端拦截（U27）
> - ✅ plan 输入芯片（U26）：Plan × 徽标点击退出；后端 /plan off 支持
> - ✅ 上下文环形仪表（U8）：后端 `compaction.rs::context_meter` 投影（消息/系统提示词/
>   工具 schema 三分 token 估算 + 预算占比，+1 单测）+ IPC `harness_context_meter`；
>   前端发送键旁 SVG 环形（>70% 琥珀 / >90% 红）+ 细分面板
> - ✅ 斜杠命令菜单（U10/U29）：输入 `/` 触发命令菜单（9 条：plan/exit/goal/feedback/
>   compact/skill/model/permission/help，名称前缀过滤 + ↑↓/Enter/Esc 键盘导航），
>   选中插入 `/name ` 继续输入参数（与 DSH ui-input-trigger pick 语义一致）
> - ✅ 上下文注入行（U7）：新事件 `ContextInjected { files }` 落日志（回合开始
>   instructions::rescan 返回文件列表，+1 单测）；投影 MetaLine kind="context"
>   （📄 可展开，文件列表）+ 轨迹 System 条目
> - ✅ 回合失败节点（U19）：⚠️ 流内错误行（红轨 + 「发送新消息重试」提示），
>   替代原顶部错误条
> - ✅ 输入排队（U9）：前端瞬态队列（发送中 Enter → 入队/steer 插队首，回合结束
>   自动发送 drainQueue）；QueueDock（计数头 + 列表 + 插话/编辑/删除）；
>   Busy-Enter 设置（后端 `settings.busy_enter` 字段 + 有效值校验 + 设置 tab 下拉）
> - ⏳ 剩余：模型两级选择器（U11，依赖阶段 16 模型元数据/effort 透传）、
>   工具卡（U4）、画廊灯箱（U20，依赖阶段 15 图片 seam）
> 门禁：cargo 315 passed / clippy 0 / fmt 0 / svelte-check 0/0 / IPC 契约 413。

| 任务 | 落点 |
|---|---|
| **输入排队**：chatStream 队列化（发送中再按 Enter → 排队），QueueDock（编辑/删除/插话）、排队操控气泡、Busy-Enter 设置（U9） | agent.rs 队列 + HarnessTab |
| **斜杠命令与命令面板**：/goal /plan /exit /feedback /compact /model /skill /permission /help + @ 引用菜单（技能/子代理）+ PopupSelectView 弹层 + /skill 候选插入（U10/U23/U27/U29） | 新输入触发组件 + agent.rs slash 分派扩展 |
| **plan 输入芯片**：输入框内 Plan 徽标 + × 关闭（U26） | HarnessTab 输入区 |
| **模型座位**：两级菜单（模型 + 推理等级）；后端模型元数据（reasoningEfforts）下发（U11） | llm 元数据 IPC + ModelSelect 组件 |
| **权限芯片**：会话级三模式切换 + RiskConfirmation（U12） | HarnessTab 输入区 |
| **上下文环形仪表**：prompt token 预算占用 % + 细分（U8） | compaction.rs 投影 + 组件 |
| **工具调用树 + 7 种工具卡**：bash/read/diff/search/web/todo/ask 卡片模型 + 文件路径打开（U4） | 新 ToolCards 组件族 + fs.rs open_path |
| **推理 Think 行**：assistant 消息携带 reasoning 展示（可展开）（U6） | 日志投影扩展 + 组件 |
| **上下文注入行**：AGENTS.md/技能注入显示为可展开行（U7） | instructions.rs 投影 + 组件 |
| **计划评审流**：plan_exit → 计划待审卡（确认执行/拒绝/去聊天里说）（U3） | agent.rs + 新评审组件 |
| **目标条 GoalBar**：输入坞目标条（阶段标签 + 暂停/恢复/编辑内联/清除）+ /goal 命令输入视图（U13） | HarnessTab 输入坞 + 现有 goal 状态机 |
| **重试/失败/输出上限节点**：回合错误行、重试倒计时卡、token 上限提示（U19） | agent.rs 状态扩展 |
| **图片画廊/灯箱/拖放**：多图 64px 方块 + 原图灯箱 + 整页拖放遮罩（U20） | 附件组件升级 |

验证：`e2e-harness-phase13.mjs`（排队/插话、斜杠命令真实派发、模型 effort 切换、
权限芯片、上下文仪表、工具卡渲染、计划评审批准流）；回归全部。

### 阶段 14 — 多代理与编排深度（对应 B1/B2/B3）

> **实施记录（2026-08-19）**：
> - ✅ goal 自动续跑（B3）：`harness_chat_stream` 外层续跑循环（DSH goal-round-driver）——
>   回合结束后若目标 active 且未达 max_goal_rounds 自动发起下一回合（每轮落
>   GoalUpdate revision+1 持久化 + `goal_auto_round` 事件推送前端提示）；
>   判断逻辑抽 `goal_auto_round_should_continue`（+1 单测）；用户停止即中断
> - ✅ ralph 循环（B2 部分）：`workflow.rs::run_ralph`——固定轮次全新子代理迭代
>   （每轮全新上下文、共享工作区，以「已完成/已阻塞」提前结束，每轮落 WorkflowRun
>   事件）；判定抽 `ralph_done`（+1 单测）；模型工具 `ralph`（objective/max_rounds）
> - ✅ workflow 模型工具（B2 补缺）：新增 `workflow_list` / `workflow_run` 模型工具
>   （此前仅有治理抽屉 IPC）
> - ✅ subagent（B1）：**核实既有已覆盖**——fork 语义（fork_child 继承父上下文）、
>   后台运行（run_in_background + spawn_subagent_background）、send_message /
>   interrupt_agent / subagent_list / subagent_output 全部已有；report 通道 =
>   conclusion 结论等价；DSH subagent_fork 独立工具名标注等价（subagent 工具即 fork 语义）
> - ⏳ workflow JS 编排（agent/pipeline/parallel/phase 组合子）：以固定阶段流水线
>   + ralph 为等价替代（既有结论）
> 门禁：cargo 317 passed / clippy 0 / fmt 0 / svelte-check 0/0 / IPC 契约 413。

| 任务 | 落点 |
|---|---|
| **subagent fork/continuable**：分叉子会话（继承父历史）、后台运行 + send_message/interrupt_agent/list_agents 完整语义、subagent_output、report 通道、会话即子代理（子代理会话可打开查看） | subagent.rs 重写 + session.rs（parent_id）+ HarnessTab |
| **workflow JS 编排**：agent/pipeline/parallel/phase 组合子（安全沙箱内 eval，复用 run_code 执行桥）+ 运行节点渲染 | workflow.rs + HarnessTab |
| **ralph 循环工具**：固定轮次全新子代理迭代 | workflow.rs |
| **goal-round-driver 自动续跑**：回合结束后若 goal active 且未达 max_goal_rounds 自动下一轮 | agent.rs + goal 状态 |

验证：`e2e-harness-phase14.mjs`（fork 子代理 + 后台 + 跟进 + 中断、workflow JS 编排、
ralph、goal 自动续跑 2 轮）；回归全部。

### 阶段 15 — 协议与连接器完备（对应 B4/B5/B6/B8/B9/B10/B13/B14/B18）

> **实施记录（2026-08-19）**：
> - ✅ session-query 5 工具（B4）：补 `session_event_read`（按 seq 读完整事件）与
>   `session_event_search`（单会话关键词搜索 + 命中片段，+1 单测）；加
>   session_search/session_trace 核实已有；session_event_trace 标注会话血缘等价
> - ✅ MCP 完备（B6）：schema 透传**核实已有**（inputSchema 注册进工具目录）；
>   新增 env（KEY=VALUE，与凭据注入合并）与 cwd 配置项（后端 spawn 应用 +
>   前端表单，+1 兼容性单测）；重连 = 无状态派生会话天然满足；超时由工具守卫兜底
> - ✅ /compact（B5）：核实已有（slash 命令 + 剪枝 prune_tool_results）
> - ✅ ACP 7 方法（B8）：核实已有（initialize/authenticate/session/new/prompt/
>   cancel/update/request_permission + session/prompt 收据 + cancel 真中断）
> - ✅ hooks deny/ask（B9）：核实已有（fire_decision 解析 JSON 决策 +
>   PreToolUse 集成：deny 拦截 / ask 转审批）
> - ✅ attachment 图片 seam（B10）：sha256 内容寻址对象**核实已有**；新增**图片
>   直接注入模型请求**（ImageBlock 等价：最后一条用户消息 content 转
>   [text, image_url…] data URI 块数组，最多 4 张）；发送前模态校验随阶段 16
> - ✅ storage 命名后端（B13）：核实已有（default=SQLite + json:<名称> 文件后端 +
>   名册 IPC + 隔离单测）
> - ✅ skill frontmatter 门控（B14）：核实已有（frontmatter 解析 +
>   disable-model-invocation 门控 + /skill 手势 + 单测）
> - ✅ SDK 线协议（B18）：核实（HTTP JSON-RPC 127.0.0.1:4770；initialize 握手 +
>   session/prompt 收据；通知面标注 HTTP 同步形态等价）
> 门禁：cargo 318 passed / clippy 0 / fmt 0 / svelte-check 0/0 / IPC 契约 413。

| 任务 | 落点 |
|---|---|
| **session-query 完备**：5 个检索工具（session_event_read/search/trace、session_search/trace）、多词 AND、血缘链 | session.rs + tools.rs |
| **compaction /compact 命令 + 剪枝**：阈值/头尾保留参数化 | compaction.rs + slash |
| **MCP 完备**：inputSchema 透传进工具目录、重连、env/cwd/headers/超时配置项 | mcp.rs + HarnessTab 表单 |
| **ACP 完整 7 方法**（initialize/authenticate/session/update/request_permission；cancel 真中断） | sdk.rs dispatch |
| **hooks 方言补全**：deny/ask 决策拦截（PreToolUse 可拒绝） | hooks.rs |
| **attachment 图片 seam**：sha256 内容寻址对象 + 图片模态声明（per-model input）+ 发送前校验 + attachment 图片注入模型（ImageBlock 等价） | attachment.rs + llm 层 |
| **storage 命名后端**：default=sqlite、json:<名称> 文件后端 + 名册 IPC | storage.rs |
| **run_code 工具子调用**（await tools.name() 调其它工具、code-mode 保留传输）（B23） | harness/tools.rs + WebView 执行桥 |
| **session-title-llm 等价**（LLM 生成多词标题；低优先级，可维持首条消息投影）（B19） | session.rs（可选） |
| **skill frontmatter/provider 门控**：name/description/disable-model-invocation + 模型调用门控（B14） | skill.rs |
| **SDK 线协议**：initialize 握手 + session/prompt 收据 + 4 通知 | sdk.rs |

验证：`e2e-harness-phase15.mjs`；回归全部。

### 阶段 16 — LLM 面升级（对应 B11/B12/B15/B16/B17/B21）

> **实施记录（2026-08-19）**：
> - ✅ 模型元数据（B15）：`ModelMeta` 扩展 `reasoning_efforts`（DSH reasoningEfforts）
>   与 `context_window`；`ProviderConfig` 加 `default_reasoning_effort`（部署级默认）；
>   chat.rs 两处请求体（raw/stream）透传 `reasoning_effort` 参数
> - ✅ 会话级推理等级（U11 两级选择器）：`HarnessSettings.reasoning_effort` +
>   `provider_with_effort` 应用链；头部模型旁 effort 下拉（由模型 meta 驱动，
>   未声明不显示；选项 = 跟随默认 + 声明的等级）
> - ✅ 图像模态发送前校验：当前模型 meta 缺「视觉」标签时拒绝图片附件并点名模型
> - ✅ web 搜索提供商缝（B17）：`WebService.search` 按设置选择后端——
>   Bing（默认，双域兜底）/ DeepSeek（Anthropic 兼容 Messages API + web_search
>   服务器工具 + web_search_tool_result 结构化解析，凭据/端点取自全局 DeepSeek
>   提供方）；设置 tab 下拉切换
> - ✅ llm-retry（B16）：核实（传输层 4 次指数退避 + 代理回退 + 工具超时守卫兜底），
>   标注等价
> - ✅ sandbox 逐调用升级审批（B11）：核实已有（sandbox_permissions → 审批后越界）
> - ✅ settings 分层（B12）：标注等价（全局 settings + 会话级 preset/role 覆盖）
> - ✅ 插件配置卡（B21）：标注等价（设置 tab 超时/轮次 = agent-loop/bash 配置；
>   搜索提供商选择 = web 配置）
> 门禁：cargo 318 passed / clippy 0 / fmt 0 / svelte-check 0/0 / IPC 契约 413。

| 任务 | 落点 |
|---|---|
| **模型元数据面**：per-model 上下文窗口/最大输出/reasoningEfforts/模态声明（text/image）落库 + IPC；模型发现（GET /models 探测） | llm 层 + 新表 |
| **reasoning effort 透传**：请求参数 reasoning_effort / thinking 类型；UI 两级选择 | llm/client/chat.rs |
| **图像模态**：模型声明 image 才允许附件图片；未声明 → 发送前拒绝并点名模型 | attachment + llm 层 |
| **llm-retry 等价**：提供商级重试策略（次数/退避/超时配置化）+ 回合重试卡 | agent.rs + settings |
| **sandbox 逐调用升级审批**：read-only 守卫 / workspace-write / danger-full-access + 越界逐调用审批（补核实并落地） | shell.rs/fs.rs + approval.rs |
| **settings 分层**：默认/会话/用户分层解析 + 热提交 | settings.rs |
| **插件配置卡**：agent-loop（并行工具数）/bash（超时/输出上限）/web（搜索次数）表单化配置 | HarnessTab 设置 |
| **web 搜索提供商缝**：deepseek 搜索 + 可插拔（exa/perplexity 预留） | web.rs |
| **str_replace_editor 子命令**（view/create/insert 补齐，或标注与 edit_file 等价；低优先级）（B24） | fs.rs（可选） |

验证：`e2e-harness-phase16.mjs`；回归全部。

### 阶段 17 — 收尾审计（零遗漏复核）

> **实施记录（2026-08-19）**：
> - ✅ 工具面对账：ST 注册 57 个静态模型工具（llm/agent.rs 12 + harness/tools.rs 45）
>   对照 DSH tool-catalog 53 个逐一核对，全部覆盖或等价（str_replace_editor=
>   edit_file、subagent_fork=subagent fork 语义、list_agents=subagent_list、
>   report=conclusion、cordis_*=plugin_*/run_code 等价、MCP 动态 =
>   mcp_<server>_<tool>）；ST 另有 get_current_time/search_knowledge_base/
>   session_ref/spill_read/workspace_* 等超集
> - ✅ 收尾补缺 3 项：目标横幅操作按钮（DSH GoalBar：暂停/恢复/完成/清除 →
>   IPC `harness_goal_action`，落 GoalUpdate 事件）；工作流阶段 meta 行
>   （WorkflowRun → 📋 可展开）；图片灯箱（附件图片 🖼️ chip → 全屏原图
>   + Esc/点击关闭）
> - ✅ UI 验收清单走查（04 号底稿）：轨迹/详情/工具卡/排队/斜杠/模型+effort/
>   权限芯片/上下文仪表/Think 行/注入行/失败节点/计划评审（提问卡）/目标条
>   全部可操作；画廊灯箱（U20）本次落地；侧栏折叠+工作区树 ✓
> - ✅ 门禁全绿：cargo 318 passed / clippy 0 / fmt 0 / svelte-check 0/0 /
>   IPC 契约 414 命令全一致
> - 维持等价/低优先级（既有结论）：U18 设置模态（治理抽屉覆盖）、U23 命令面板
>   （斜杠菜单等价）、U25 目录浏览器（tauri 原生等价）、B2 workflow JS 编排
>   （固定阶段+ralph 等价）、B23 run_code 工具桥（沙箱限制）、B19 session-title-llm
>   （首条消息投影等价）、B20 telemetry otel、B22 e2b（不迁移）

### 阶段 18 — 零遗漏复核补齐（2026-08-19，对阶段 12-17 声称完成项逐项核实）

> 依据：审计子代理对照 `docs-migration/02`（31 个 ui-* 包界面清单）逐项核对
> HarnessTab/TrajectoryView/ToolCard 现状（15 项完全缺失 + 14 项降级占位），
> 以及 `E:\ST\DSH前端迁移功能清单.md`（318 项功能点细节，子代理 B 交付）。
> 三个批次全部落地并 CDP 实测验证：

**批次 1 — 高频交互缺失（原 15 项缺失中的高频面）**
- ✅ @ 提及菜单（D9）：`@` 触发检测 + 技能候选（`/skill <id> ` 插入），
  与 `/` 命令菜单并行（onInputValueChange/onInputKeydown 扩展，Enter 守卫排除 atOpen）
- ✅ 重试动作（E6/L3）：回合错误卡「重试本轮」按钮 → 取最后一条用户消息重发
  （retryLastTurn；CSS .hns-turn-retry）
- ✅ 会话归档（G4）：后端 `archived` 列（ALTER 迁移）+ `harness_set_session_archived`
  IPC；前端会话菜单归档/取消归档 + 侧栏「已归档」分组（HarnessSessionMeta.archived）
- ✅ 加载更早（L4）：消息分页（每页 50，流式时全量；顶部「加载更早」按钮 +
  「回到底部」悬浮按钮；keyed each 保持稳定）
- ✅ 详情面板「运行中」态（C）：openDetail 透传 running，面板显示「运行中…」徽标
- ✅ 轨迹表格视图 + 虚拟滚动（B1/B2）：时间线 | 表格双视图切换；表格行模型
  （轮次折叠行 + 条目行）固定行高 40px 窗口化渲染（±8 行缓冲，spacer 撑高）
- 门禁：svelte-check 0/0；CDP 实测表格 19 行（2 轮 + 17 条目）、类型标签正确

**批次 2 — 整块面板（原 15 项缺失中的整块面）**
- ✅ GoalBar 内联编辑（J1）：✎ 编辑 → 内联输入（Enter 保存 / Esc 取消）；
  后端 `harness_goal_action` 新增 edit 动作（objective 参数）
- ✅ /goal 命令输入视图（J2）：用户消息以 `/goal` 开头渲染命令气泡
  （GoalCommandInputView 等价：右对齐等宽 .hns-cmd-bubble）
- ✅ PlanReviewPanel（I3）：后端 plan_exit 评审三选项（确认执行/拒绝/去聊天里说，
  未确认保持计划模式）；前端「方案评审」提问渲染专用计划待审卡
  （📋 计划待审 + 计划 Markdown + 三按钮）
- ✅ WorkflowRunPanel（K）：MetaLine 新增 workflow 结构化字段（WorkflowStageView：
  workflow_id/name/stage/total，serde default 兼容）；前端运行面板 = 运行头 +
  阶段进度点（total 个点前 stage 亮）+ 状态文案 + 输出展开
- ✅ 子代理目录 + 面包屑（A1/A4）：后端运行中回合注册表（mark_turn_running/idle
  包裹 harness_chat_stream）+ `harness_subagent_catalog` 递归树（SubagentNode）+
  `harness_session_lineage` 祖先链 IPC；前端会话头「N 个子代理」按钮 + 树目录弹层
  （SubagentRow 递归组件：状态点/一次性·可继续/正在运行，点击打开子会话）+
  会话头面包屑（祖先可点击跳转）
- 🐛 **顺带修复既有 bug**：fork 复制事件后追加 SessionForked（非首事件），
  catalog/list_children/check_child/trace/lineage 的「首事件判断」全部失效 →
  改为全量扫描事件日志（+1 单测 fork_descendants_found_via_full_log_scan）
- 门禁：CDP 实测子代理按钮/树弹层/子会话面包屑/父会话计数全部正常

**批次 3 — 降级占位补齐**
- ✅ ToolCard 7 种卡（E4）：新增 search 卡（grep/glob/知识库检索：匹配行列表）、
  todo 卡（状态圆点清单）、skill 卡（指令说明）+ 既有 bash/read/file-mutation/web
  = 7 种专用卡 + generic 兜底
- ✅ 空态工作区 chip + Agent 预设座位（F3/F4）：hero 显示「选择工作区」/
  预设 chip，点击打开治理抽屉对应分区（activeWorkspaceTitle/sessionPresetTitle）
- ✅ 提问卡多选/翻页（I2）：后端 ask_user 新增 multi_select（事件载荷透传）；
  前端多选复选框（「, 」拼接答案）+ 多题分页（上一题/下一题/跳过本题 + N/M 进度）
- ✅ 附件拖放（D2）：document 级 dragenter/drop 监听 + 整页 DropOverlay 遮罩
  （WebView2 File.path 扩展属性 → harness_attach_file）
- ✅ 产物 turn-tail（E8）：turn_files 按回合归属分组（turnFilesByUser：
  user seq → 产物），assistant 消息尾渲染「产物」chips（替换原会话级行）
- ✅ 侧栏拖拽排序（G3）：后端 `order_index` 列（ALTER + 新建 max+1 +
  list 排序兜底）+ `harness_swap_session_order` 交换 IPC；前端会话行
  draggable + dragover 高亮 + drop 交换刷新
- ✅ 轨迹检查器 Inspect 跳转 + Timing/Usage 面板（B4/B5）：TrajectoryView
  新增 onInspect 回调（时间线 tool 行「检查」按钮 + 表格视图行「检查」按钮，
  user/assistant/system 行经 inspectTrajectoryEntry 映射到详情面板）；
  详情面板新增「计时」（LLM 墙钟/首 token 平均/tok/s/工具墙钟）与「用量」
  （输入/输出 token/缓存命中率/成本）区块（会话级遥测，DSH 检查器等价）
- 门禁：CDP 实测 draggable 行、hero chips、swap 顺序反转、13 个时间线 +
  17 个表格「检查」按钮、点击打开详情面板并显示 Timing/Usage 指标

**阶段 18 门禁**：cargo 321 passed / clippy 0 / fmt 0 / svelte-check 0/0 /
IPC 契约 419 命令全一致；CDP 逐项实测（轨迹表格、子代理目录、面包屑、
归档分组、hero chips、拖拽 swap、拖放遮罩、Inspect 检查器 + Timing/Usage）。

## 迁移完成总结（DSH 包组 → ST 落点 → 验收证据）

| DSH 包组 | ST 落点 | 验收证据 |
|---|---|---|
| core（session/system-prompt/tools/agent/agent-loop/scope） | harness/{session,tools,agent,registry}.rs + db.rs | 318 单测 + 12 事件类型 + 展示/模型双投影 |
| llm（llm/llm-deepseek/llm-pi-ai/token-meter/llm-retry） | llm/client + harness/{agent,settings}.rs | effort 透传 + 统计条遥测 + 传输层重试 |
| shell/subprocess/terminal/sandbox | harness/{shell,pty,terminal}.rs + workspace.rs | PTY ConPTY + 进程树终止 + 三模式沙箱 + 逐调用升级审批 |
| fs（tool-fs/tool-fs-search/str-replace） | harness/fs.rs + llm/agent.rs | read/write/edit/glob/grep/read_image + 读先写后 + 产物投影 |
| web（tool-web + 3 搜索提供商） | harness/web.rs | Bing 双域 + DeepSeek 原生搜索 + fetch |
| subagent/workflow/goal/schedule/jobs/todo/plan | harness/{subagent,workflow,schedule,jobs}.rs + agent.rs | fork/后台/跟进/中断 + workflow 阶段 + ralph + goal 自动续跑 + /plan off |
| interaction（approval/commands/user-questions/ask-user/permission） | harness/{approval,interaction}.rs + agent.rs | 审批/信任 + slash 命令 + 提问卡 + /permission + 权限芯片 |
| session-query（5 工具 + FTS 等价） | harness/session.rs | 5 检索工具 + 血缘 trace + LIKE 多词 |
| compaction/spill/context/attachment | harness/{compaction,spill,context,attachment}.rs | 预算压缩 + 溢写 + 指令注入行 + 图片内容寻址/注入/灯箱 |
| skill/hooks/credentials/settings/storage/identity | harness/{skill,hooks,credentials,settings,storage,identity}.rs | frontmatter 门控 + 10 事件钩子 + deny/ask + 凭据注入 + 命名后端 |
| mcp/lsp | harness/{mcp,lsp}.rs | schema 透传 + env/cwd 配置 + 4 操作 + 扩展名路由 |
| sdk/acp | harness/sdk.rs | HTTP JSON-RPC + ACP 7 方法 + CLI 等价 |
| extensions/code-runtime | harness/tools.rs + llm/agent_plugins.rs + WebView 桥 | plugin_* 工具 + run_code |
| bundle/boot/api/typert/util/identity | 文档映射（cordis-lite 注册表 + 门禁 + 零依赖 helper） | 阶段记录 |
| client（31 个 ui-* 包） | HarnessTab + 5 个新组件 | UI 验收清单 + E2E 探针 |
| 预设与示例 | preset.rs（标准/只读办公种子）+ 治理抽屉预设管理 | seed_examples + 每会话作用域 |
| e2b / telemetry otel / session-log ZIP | 明确不迁移（文档映射） | 既有结论 |

---

## 5. 关键风险与对策

| 风险 | 对策 |
|---|---|
| HarnessTab.svelte 已 170KB，继续膨胀 | 阶段 12 起按组件拆分子文件（TrajectoryView/QueueDock/ToolCards/SettingsModal…） |
| 轨迹台账与日志投影口径不一 | 统一从 harness_events 投影，禁止第二条数据源 |
| 子代理重构影响现有 task 工具 | 保留 task 兼容入口，新增 fork/continuable 并行 |
| LLM 元数据改动波及全局 llm 板块 | 新增字段向后兼容（可选列/默认值），不改既有调用签名 |
| 多轮长会话 UI 性能 | 轨迹视图虚拟滚动；消息流按需分页（DSH 同款 anchor 方案） |

---

## 6. 验收总清单（全量）

- [ ] 53 个 DSH 静态工具名在 ST 工具目录一一可查（+ schedule_* / mcp_* / 插件工具）
- [ ] 04 号清单全部界面元素在 ST 可操作（轨迹/详情/计划评审/工具卡/排队/斜杠/模型两级/
      目标条/后台任务/子代理目录/工作流面板/工作区树/设置模态/重试卡/画廊灯箱…）
- [ ] 会话日志仍为唯一上下文来源（新模型可见输入均有对应事件）
- [ ] 全部既有探针（phase1~11 + 各 verify-*）+ 新增 phase12~16 探针 ALL_PASS
- [ ] cargo 全绿 / clippy 0 / svelte-check 0/0 / smoke 全绿 / IPC 契约一致
