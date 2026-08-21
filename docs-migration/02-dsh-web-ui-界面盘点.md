# DeepSeek Harness (DSH) Web UI 界面盘点

> 范围：`packages/client/` 下 31 个 `ui-*` 包 + `web`/`web-react`/`modules` 布局层。只读研究，未改任何文件。Slot 命名遵循 `<域>.<入口>.<洞>` 组成路径；`children` = 子槽声明，`inject` = 依赖服务。

## 一、整体布局（页面骨架）

- **引导内核**（`web`）：`AppWebEntry.run()` 解析 `window.__DSH_BOOT__` 入口图 → 建模块装载器 → 渲染加载页 → 预取 immediately 层 → 挂 vendored Cordis Loader → 并发创建各插件条目 + shell 自有 app-shell 装配条目 → `loader.await()` + fiber 全扫（未 ACTIVE 则 fail-loud）。加载页硬编码英文：`HARNESS` 字标 + spinner + `Loading plugins…`；失败页 `Failed to load plugins` + 逐条列出失败条目。
- **唯一渲染点**：app-shell 的 `buildRenderApp` 只调 `ctx.slots.renderSlot('root', {})`，另挂 `DocumentTitle`（跟随当前会话标题）。
- **三栏骨架**（`ui-layout` 的 `AppFrame` 注册进 `root`，声明子槽 `sidebar`/`conversation`/`details`/`shell.overlay`）：
  - 左栏 `sidebar`（`ui-sidebar` SidebarRoot）：品牌标、折叠切换、新建会话按钮。
  - 中栏 `conversation`（`ui-conversation` ConversationRoot）：空态 hero / 会话视图环 / 常驻输入栏。
  - 右栏 `details`（`ui-conversation` DetailsPanel）：工具调用详情。
  - 浮层 `shell.overlay`（点击穿透、可叠加徽标/toast/状态）。
- **几何交互**：两侧列拖拽手柄（pointer capture + rAF 节流）调宽；窄视口自动折叠侧栏（rail 模式）；`ResizeObserver` 跟踪视口；主题快照由 `ThemePresenter` 写 `document.body`（`--dsw-*` token）。
- **装配说明**：`web-react` 提供 slot→React 渲染机制（`createSlotRenderer`/`SessionProvider`/`bindSnapshotSelector`），无 UI；`modules` 是浏览器侧模块装载系统（`__DSH_BOOT__` 线协议 + `__ModuleLoader__` 工厂注册），无 UI；`ui-slots` 是 Slot 核心（register/SlotMap/store/renderer 契约），无 UI。

## 二、每个 UI 包的界面清单

### ui-layout
- **Slot**：`root`（声明 `sidebar`/`conversation`/`details`/`shell.overlay`）；inject `[slots, theme]`，提供 `ctx.layout`（面板动作）。
- **组件**：`AppFrame` 三栏网格 + 两个 `DragHandle` 拖拽手柄；`LayoutController`（panel 动作面）；`ThemePresenter`（主题写 DOM）。
- **交互**：拖拽调宽、窄屏折叠、`openDetails/closeDetails/toggleSidebar`。

### ui-slots
- 纯 Slot 核心（`SlotCore`/`SlotMap`/store/renderer 契约、`renderSlot`、SlotOwnershipError）。不注册 slot、无 UI。

### ui-sidebar
- **Slot**：`sidebar`（声明 `sidebar.workspaces`/`sidebar.settings`/`sidebar.footer.action`）；inject `[slots, layout, sessions, workspaces, locale]`。
- **组件**：`SidebarRoot` 品牌标 + 折叠/展开（rail 56px）+ 新建会话按钮；宽/窄两态渲染。
- **文案**：`sidebar` 命名空间 4 键（新建会话、折叠 aria 等）。

### ui-workspace
- **Slot**：`sidebar.workspaces`（+子槽 `sidebar.workspaces.directoryFlow`）与 `conversation.hero.workspace`（+`conversation.hero.workspace.directoryFlow`）；inject `[slots, sessions, workspaces, locale]`。
- **组件**：`WorkspaceBrowser`（分区头、搜索框、分组/单列表树、会话行、工作区弹窗）；`WorkspacePicker`（hero 空态选择器 + 错误弹窗）；`Rows`/`tree`/`stores`。
- **交互**：新建会话、搜索会话（含内容搜索）、重命名工作区/会话、删除工作区、归档会话、分叉会话、手动拖拽排序、添加工作区（触发 directoryFlow）、视图选项（按工作区/单列表、手动/最近更新排序）、状态徽标（进行中/等待审批/计划待审/已完成、子代理数）。
- **文案**（60 键）：`新会话`、`工作区`、`会话`、`搜索会话…`、`添加工作区`、`重命名`、`删除工作区`、`分叉会话`、`归档会话`、`进行中`、`等待审批`、`计划待审`、`无匹配结果` 等。

### ui-conversation（最大包）
- **Slot**：`conversation`（声明 11 子槽：`conversation.session`/`.session.header`/`.composer`/`.composer.bar`/`.input.overlay`/`.input.dock`/`.composer.dock`/`.input.left`/`.input.right`/`.hero.workspace`/`.hero.agentPreset`）；另有 `conversation.session`（声明 `conversation.view`）、`conversation.session.header`（声明 `.actions`/`.utilities`）、`conversation.composer.bar`（声明 `.input.plan`/`.input.model`）、`conversation.view`（id `chat`，声明 `conversation.chat.node`）、`details`（声明 `conversation.details.tool`）、`conversation.composer`（审批 takeover `ApprovalPanel`，priority 1）、`conversation.composer.dock`（StatsLine）、`settings.general.item`（Enter 行为）。inject `[slots, layout, sessions, workspaces, locale, connection, remote, settingsScope, conversationEvents, conversationViews]`。
- **组件**：`ConversationRoot`/`ConversationSession`/`ConversationSessionHeader`（标题+视图 tab+动作行）/`DetailsPanel`（详情面板：输入/输出/运行中）/`EmptyHero`（hero 文案）/`InputBar`（输入栏：textarea、发送/停止、附件、访问模式、plan、模型座）/`ApprovalPanel`（审批：拒绝/允许一次）/`TodoPanel`+`QueueDock`（输入 dock）/`StatsLine`（统计行）/`ChatView`+`MessageItem`+`AssistantNodeView`+`TurnTailNodeView`+`ReasoningRow`+`CommandNodeView`+`CompactionCommandCard`+`GenericCommandCard`+`ContextBody`+`ContextMeter`+`PermissionSelect`+`EnterBehaviorRow`；chat 节点定义（assistant/command/compaction/message/tool/retry/turn-error/turn-max-tokens/turn-tail/inbox/fallback）。
- **交互**：发消息/停止生成、Cmd/Ctrl+Enter 插话发送排队消息、`/`+`@` 触发菜单、图片拖放上传（PNG/JPG/WebP/GIF，多图、灯箱看原图）、消息操作（复制、分叉分支、重试）、审批（允许一次/拒绝，Full access 风险确认弹窗）、详情面板 Inspect 跳转轨迹、forkAt、加载更早/回到底部、上下文用量环、压缩摘要展开。
- **文案**（约 170 键）：`对话`、`给智能体发消息`、`停止生成`、`发送消息`、`选择工作区`、`探索未至之境`、`详情`、`等待审批`/`拒绝`/`允许一次`、`任务`、`在新对话中分支`、`上下文已用`、`排队消息`、`插话发送`、终端/JSON/压缩系列文案等。

### ui-commands
- **Slot**：`conversation.input.overlay`（id `command-popup`，order 1）；inject `[inputTriggers, sessions, remote, remote.commands, locale]`。提供 `ctx.commandUi`（目录缓存 + `/` 命令源）。
- **组件**：`PopupSelectView`（popupSelect 弹层 shell）；`PopupSelectController`/`directory`/`service`。
- **交互**：`/` 命令候选弹层、选项确认（可选 confirmation 风险确认）、选项过滤。

### ui-input-trigger
- **Slot**：`conversation.input.overlay`（id `slash-menu`，order 0）；inject `[sessions, locale]`。提供 `ctx.inputTriggers`（触发检测/候选菜单/pick 管线）。
- **组件**：`MenuView`（候选菜单，listbox）；`controller`/`service`/`core/detect`/`core/menu`。
- **交互**：`/`/`@` 触发字符检测、按来源分组候选、pending 行、键盘选择、pick 后插入引用文本。
- **文案**：`slash.menu` 命名空间（组标题、listbox aria、pending 行）。

### ui-message-feedback
- **Slot**：`conversation.chat.assistant-actions`（id `feedback`，order 10）；inject `[slots, remote, remote.messageFeedback, locale]`。
- **组件**：`MessageFeedbackActions`（点赞/点踩 + 备注）。
- **交互**：like/dislike 切换、评分备注、clear。

### ui-attachment（纯组件，零 cordis，不注册 slot）
- **组件**：`AttachmentRail`（草稿图片横轨）、`DropOverlay`（全页拖放遮罩）、`ImageLightbox`（原图灯箱）、`MessageImage`/`ImageGallery`（历史图片画廊）。文案由宿主命名空间传入。

### ui-tool
- **Slot**：`conversation.chat.node` key `tool-call`（声明 `tool.call.toolview`）；`conversation.details.tool`（`ToolDetails`）；7 个内置原子 toolview（key 名）：`bash`/`read`/`file-mutation`/`search`/`web`/`todo`/`ask-question`。inject `[slots]`。
- **组件**：`ToolCallTree`（工具调用树/折叠）、`ToolDetails`（详情面板工具输出）、`ToolRow`/`GenericToolCard` + toolviews（read/search/web/bash/todo/ask-question/file-mutation/plan-summary）+ models（diff/read/search/terminal/web/tool-call card-model）。
- **交互**：工具调用行展开/折叠、点选 Inspect、详情面板输入/输出、diff/终端/JSON/Web 结果渲染。

### ui-trajectory
- **Slot**：`conversation.view`（id `trajectory`，order 10，tab 标签 `轨迹`）；inject `[slots, conversationEvents, conversationViews, sessions, locale]`。注册 message/request-header/assistant/tool/compaction 等轨迹节点定义。
- **组件**：`TrajectoryView`/`TrajectoryTable`（虚拟行表格）/`TrajectoryToolbar`/`TrajectoryTurn`/`TrajectoryTurnHeader`/`TrajectoryGroupHeader`/`TrajectoryCell`/`TrajectoryTimeline`/`TrajectoryPreview`/timeline/layout/virtual-rows。
- **交互**：视图 tab 切换（对话/轨迹）、虚拟滚动、时间线/瀑布分组、加载更早、Inspect 跳转、实际耗时展示。

### ui-primitives（纯 UI 原语库，不注册 slot）
- 基础：`Button`/`Input`/`Pill`/`Modal`/`Menu`/`Tooltip`/`Toast`/`HoverCard`/`DisclosureRow`/`JsonTree`/`DiffBlock`/`StateDot`/`RiskConfirmation`/`ConnectionBanner`/`OnboardingSurface`/`BrandWordmark`/`FishLogo`。
- Markdown/终端块：`MarkdownText`/`MessageText`/`CodeBlock`/`JsonBlock`/`TerminalBlock`/`ReadBlock`/`SearchBlock`/`WebBlock`/`katex`(数学)/`highlight`/`ansi`/`incremental`/`cjkFriendlyStrong`。
- 工具函数：`clipboard`/`use-copy-feedback`/`useAnchoredMaxHeight`/`pointer-grace`/`head-tail-cap`。

### ui-settings（设置域基座）
- 无 UI；提供 `ctx.settingsScope`（设置命名空间 Host 传输）。slot 契约：`settings.trigger`/`.header`/`.action`/`.close`/`.section`/`.plugins.tab`/`.onboarding`/`.general.item`。

### ui-settings-general（设置外壳）
- **Slot**：`sidebar.settings`（声明 `settings.trigger`/`.header`/`.action`/`.close`/`.section`/`.onboarding`）；另注册 `settings.trigger`(TriggerContent)/`.header`(HeaderContent)/`.action`(打开文档，仅 loopback)/`.close`(CloseLabel)/`.section` id `general`（声明 `settings.general.item`）。inject `[slots, locale, connection]`。
- **组件**：`SettingsRoot`（面板 chrome + 分区导航 + onboarding stage）、`GeneralSection`、`chrome`、`SettingsDocumentAction`（打开文档）。
- **交互**：打开/关闭设置面板、分区导航、General 分区栈式行、onboarding 协调器。
- **文案**：`settings` 命名空间（触发器标签、面板标题、关闭 aria、General 分区名）。

### ui-settings-models
- **Slot**：`settings.section` id `models`（order 10，标签 `模型`）；`settings.onboarding` 两个：`welcome-notice`(order -100) 与 `deepseek-official`(order 0)。inject `[slots, locale, connection, remote]`。
- **组件**：`ModelsSection`、`ModelListEditor`、`DeepSeekModelsEditor`、`ProviderEditor`、`CustomProviderCard`、`EditorFooter`、`OnboardingModal`、`DeepSeekOnboardingDialog`、`WelcomeNotice`、`apiKey`、`store`/`welcome-store`/`onboarding-copy`。
- **交互**：模型列表编辑、供应商/自定义供应商编辑、API key 输入、DeepSeek 官方 onboarding 引导弹窗、欢迎通知。

### ui-settings-plugins
- **Slot**：`settings.section` id `plugins`（order 15，标签 `插件`，声明 `settings.plugins.tab`）；`settings.plugins.tab` id `configurable`（声明 `settings.plugin.item`）；`settings.plugin.item` 三卡：`bash`/`agent-loop`/`web-search`。inject `[slots, locale, connection, remote, settingsScope]`。
- **组件**：`PluginsSettingsSection`（tab chrome）、`ConfigurablePluginsTab`、`PluginCard`、`BashCard`、`AgentLoopCard`、`WebSearchCard`、`fields`/`card-form`（表单字段/密钥）。
- **交互**：插件配置卡（bash/agent-loop/web-search 字段表单、密钥写入、凭据刷新）。

### ui-settings-plugin-inventory
- **Slot**：`settings.plugins.tab` id `all`（order 10，标签 `全部`/inventory）。inject `[slots, locale, remote, remote.pluginInventory]`。
- **组件**：`PluginInventorySettingsTab`（只读 Host 插件清单列表）。
- **交互**：懒加载插件清单 tab、错误重试。

### ui-permission-presets
- **Slot**：`settings.general.item` id `permission`（order -20，默认预设）；并对 `/permission` 命令做 `popupSelect` decoration。inject `[commandUi, sessions, slots, locale, connection, remote]`。
- **组件**：`PermissionRow`（默认权限预设行）；popup 选项含 Full access 风险确认弹窗。
- **交互**：选择默认预设；`/permission` 弹层选预设、Full access 需确认（`我已了解风险，并愿意继续`）。

### ui-model-selection
- **Slot**：`conversation.input.model`（`ModelSelect`）+ `/model` popupSelect 命令。inject `[commandUi, connection, locale, sessions, slots, remote]`。提供 `ctx.modelDirectories`。
- **组件**：`ModelSelect`（composer 模型座 + 菜单）。
- **交互**：选模型（按供应商分组、当前项标记 active、失败行不可选）、无适配器时 composer block。

### ui-agent-preset
- **Slot**：`conversation.hero.agentPreset`（新会话 chip `AgentPresetSeat`）、`conversation.session.header.actions` id `agent-preset`（order -10，只读 `AgentPresetLabel`）、`settings.general.item` id `agent-preset`（默认预设 `AgentPresetRow`）、`settings.section` id `agent-presets`（order 20，`AgentPresetSection`）。inject `[slots, locale, connection, remote]`。
- **组件**：`AgentPresetSeat`/`AgentPresetLabel`/`AgentPresetRow`/`AgentPresetSection`/`PresetMenu` + seat/section/settings store。
- **交互**：新会话选预设、会话头显示预设、默认预设选择、预设名册管理（复制/删除/设默认/打开文件位置/会话式创作入口）。

### ui-theme
- **Slot**：`settings.general.item` id `appearance`（order 10，`AppearanceRow`）。inject `[slots, locale, connection, remote, settingsScope]`。提供 `ctx.theme`（主题注册 + 偏好）。
- **组件**：`AppearanceRow`（外观行）+ `styles/`（`--dsw-*` token 样式表）。
- **交互**：浅色/深色/跟随系统切换；第三方主题 token override 层；`theme/change` 事件驱动。

### ui-plan
- **Slot**：`conversation.input.plan`（`PlanChip`）。inject `[slots, remote, remote.commands, locale]`。
- **交互**：plan mode 状态 chip，点击执行 `/plan off` 退出；锁定/退出中禁用；失败提示。
- **文案**：`plan mode 已开启，按下关闭` / `plan mode 已关闭，按下开启`。

### ui-goal
- **Slot**：`conversation.input.dock`（id `goal`，order 10，`GoalBar`）；`conversation.chat.node` key `command-input`（`GoalCommandInputView`）；注册 goal-command-input 节点定义。inject `[slots, sessions, remote, remote.goals, locale, conversationEvents]`。
- **交互**：目标条（阶段标签 + 目标文本 + 暂停/恢复/编辑/清除按钮）；编辑态内联输入 Enter 保存/Esc 取消；`/goal` 输入气泡。
- **文案**：`进行中的目标`/`已暂停的目标`/`受阻的目标`/`保存目标`/`取消编辑`/`暂停目标`/`恢复目标`/`编辑目标`/`清除目标`。

### ui-user-questions
- **Slot**：`conversation.composer`（链式 takeover，selector 认领 QuestionWait）。inject `[slots, locale]`。
- **组件**：`QuestionComposer`（路由到 `PlanReviewPanel` 或通用 `QuestionFlow`）；单选/多选/自定义答案/翻页/跳过；`PlanReviewPanel`（确认执行/拒绝/去聊天里说）。
- **文案**：`推荐`、`输入你的答案`、`下一题`、`跳过本题`、`计划待审`、`确认执行`、`拒绝`、`去聊天里说`、进度 `1 / 3`。

### ui-workflow-run
- **Slot**：`conversation.chat.node` key `workflow-run`（`WorkflowRunPanel`）；注册 workflow-run 节点定义。inject `[conversationEvents, slots, sessions, locale]`。
- **交互**：工作流运行面板（RunHeader + PhaseSection + MemberRow 折叠/展开，运行中强制展开，成员行可点击打开子会话）。
- **文案**：`运行中`/`已完成`/`失败`/`已取消`/`已中断`/`没有启动成员`/`未分阶段`。

### ui-subagent
- **Slot**：`conversation.session.header.actions` id `subagent-catalog`（order 10）；`conversation.composer`（只读 takeover，priority -10）；注册 `@` 触发源。inject `[inputTriggers, sessions, slots, locale]`。
- **组件**：`SubagentCatalogAction`（子代理树目录：展开箭头、StateDot、一次性/可继续、token/耗时）；`SubagentReadOnlyComposer`（只读说明）。
- **交互**：树键盘导航（方向键/Enter/Space/Escape）、点击打开子代理、懒加载、错误重试；`@` 提及子代理。
- **文案**：`{n} 个子代理`、`一次性`/`可继续`、`正在运行`/`当前未运行`、`重试`、`父会话当前不在线`。

### ui-jobs
- **Slot**：`conversation.session.header.actions` id `job-list`（order 20）。inject `[sessions, slots, locale]`。
- **组件**：`JobListAction`（后台任务弹层：StateDot + kind + label + status + 时长）。
- **交互**：开合弹层、Escape 关闭、运行中每秒刷新时长、状态（运行中/正在停止/已完成/已取消/已失败）。
- **文案**：`{n} 个后台任务运行中`、`后台任务`、`运行中`/`正在停止`/`已完成`/`已取消`/`已失败`。

### ui-directory-picker-browse
- **Slot**：`conversation.hero.workspace.directoryFlow` 与 `sidebar.workspaces.directoryFlow`（成对注册）。inject `[slots, workspaces, locale]`。
- **组件**：`DirectoryBrowser`（680×500 Modal：标题、面包屑导航、路径编辑输入、Miller 两列列表、状态区、页脚按钮、嵌套新建文件夹 Modal）。
- **交互**：点行选中 + 右栏预览、面包屑跳转、编辑路径输入（Enter 提交/Esc 取消）、250ms 防抖跟随、300ms 慢扫描才显示加载、新建文件夹（Enter 创建）、显示隐藏文件开关、打开/取消。
- **文案**：`选择工作区目录`、`主目录`、`新建文件夹`、`文件夹名称`、`未命名文件夹`、`创建`、`取消`、`打开`、`编辑路径`、`显示隐藏文件`、`加载中…`。

### ui-directory-picker-native
- **Slot**：同 browse 的两个 directoryFlow 洞（renderless occupant，恒返回 null）。inject `[slots, workspaces]`。
- **交互**：无 UI；每次 `open` 上升沿调一次 `pick()` 调起**原生 OS 目录选择对话框**；结果 onPicked/onCancel/onError。

### ui-skill
- **Slot**：`tool.call.toolview` key `skill`（`SkillRow`）；注册 `/` 技能触发源（order 2）。inject `[inputTriggers, connection, sessions, slots, locale, remote]`。
- **组件**：`SkillRow`（技能工具行）。
- **交互**：`/skill` 候选（会话级缓存、预热、词库扫描）、pick 后插入 `/name ` 文本；`用户可用` 标记。

### ui-deliverables
- **Slot**：`conversation.chat.turnTail`（`ProducedFiles`，selector 认领）；提供 `chatFileMentions` 服务。inject `[slots, locale, conversationEvents, connection]`。
- **组件**：`ProducedFiles`（本轮产出文件行，chip 列表 + 打开）。
- **交互**：展示工具 `locations` 派生的产出文件，点击打开；结尾 prose 内联代码提及链接。

## 三、缺失/模糊项标注

1. **`ui-primitives`/`ui-attachment` 无 Slot**：二者是纯组件库，不注册 slot、无中文文案（文案由宿主传入），迁移时需确认其组件 API 契约而非挂载点。
2. **`ui-conversation` 内部子组件交互细节**（ChatView 消息操作按钮全集、InputBar 键盘命令、queue/todo dock 的精确 DOM）本次据 slot 契约 + 文案推断，未逐文件通读 `.tsx` 实现，建议迁移时按 `src/client/chat`、`skeleton`、`input`、`queue` 四目录逐文件核对。
3. **`ui-trajectory`/`ui-tool` 的渲染细节**（虚拟行、瀑布布局、各 toolview 的具体渲染字段）依据文件名 + 契约归纳，未读实现体。
4. **设置区各卡片的字段清单**（bash/agent-loop/web-search 的具体字段名、模型编辑器的字段）未逐一读取 `card-form`/`fields`/`ModelListEditor` 实现。
5. **`ui-settings-models` onboarding 文案与流程步骤**（DeepSeek 官方引导、欢迎通知的具体步骤）未读 `onboarding-copy.ts` 全文。
6. **全局快捷键全集**：确认了 Cmd/Ctrl+Enter 插话、Enter 行为设置、`/` `@` 触发、Esc 关闭弹层；是否存在其它全局快捷键未系统盘点。
7. **主题 token 全集**（`ui-theme/src/styles/` 全部 `--dsw-*`）未逐条列出（仅列了 `exportInspectTokens` 的 13 个别名）。
8. **`web`/`web-react`/`modules` 不属于 `ui-*` 槽体系**，其加载页文案为硬编码英文，迁移时需单独处理 boot 协议（`__DSH_BOOT__`/`__ModuleLoader__`）。

## 四、DSH Web UI 功能面概览

DSH Web UI 是一个由 Cordis Slot 机制拼装的三栏桌面式界面：左栏工作区/会话导航，中栏会话视图环（对话 + 轨迹）与常驻输入栏，右栏工具调用详情，另有浮层与全局设置面板。全部 31 个 `ui-*` 包中 24 个通过 `slots.register/inject` 挂载到 `root→sidebar/conversation/details` 及 `settings.*`/`conversation.*` 两级子槽，5 个（`ui-slots`/`ui-primitives`/`ui-attachment`/`ui-settings` 契约层/`ui-directory-picker-native`）为无 UI 或纯组件基础设施。核心交互面覆盖：会话增删改查/搜索/拖拽排序、多视图（对话/轨迹）与工具调用卡片、斜杠命令与 `@` 提及、图片拖放、审批/提问/计划/目标/队列/后台任务、以及模型/主题/插件/权限预设/Agent 预设/外观六大设置面。所有产品文案中英文双语，以中文为准。
