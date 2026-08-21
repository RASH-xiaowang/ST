# DSH Web UI 界面细节验收清单

> 说明：DSH 为双语（中/英）界面，中文为源码「文案源」。以下括号内文案为界面实际显示 / aria-label 原文。所有组件均为 React + CSS Modules，颜色只用 `--dsw-*` 设计令牌。本文供另一 Svelte 5 桌面应用完整复刻界面时作为验收参照。

---

## 整体 UI 信息架构总结

```
┌─────────────────────────────────────────────────────────┐
│ 顶栏(会话头): 面包屑(会话层级) │ [后台任务] [utilities]   │
│              视图标签页: 对话 | 轨迹                       │
├──────────┬──────────────────────────────┬───────────────┤
│ 侧栏      │ 中间列(会话区)                │ 详情列(details)│
│ Logo/新会话│  消息流(可滚动)               │  详情面板      │
│ 搜索      │   - 用户/助手/工具/系统节点    │  (点击工具行)  │
│ 工作区树  │   - 加载更早/回到底部          │  输入/输出     │
│ 会话列表  │  输入区(常驻底部):              │               │
│ 设置按钮  │   [待办] [目标] [排队]          │               │
│          │   [+命令][plan][权限][模型][⏺上下文][发送] │               │
│          │   统计条: X轮·Y步 | LLM·工具 | 首token·tok/s │               │
│          │          | 缓存命中% | 输入/输出tok          │               │
└──────────┴──────────────────────────────┴───────────────┘
```

**导航/设置项全集**：

- 侧栏：新会话、搜索、工作区/会话树、设置。
- 会话头：面包屑（会话层级）、后台任务、对话/轨迹标签页。
- 设置面板：通用设置〔权限 / Agent 预设 / 输入行为 / 外观〕、模型〔提供方管理 / 自定义提供方 / 模型目录 / 首次引导〕、插件〔插件配置 / 插件列表〕、Agent 预设〔管理/复制/删除〕。

**关键可交互控件全集**：发送/停止、命令菜单、斜杠菜单（命令/技能/子智能体）、模型选择器（模型+推理等级两级菜单）、上下文环形仪表、附件导轨、图片灯箱、审批卡（允许一次/拒绝）、提问卡/计划审阅、消息反馈（👍👎+备注）、复制/分支、任务清单、目标条、排队条、后台任务下拉、工具行展开/Inspect、轨迹工具栏+记录检查器、工作区/会话拖拽排序+重命名/删除/归档/分叉、目录浏览器（Miller 列+路径编辑+新建文件夹+显示隐藏）。

---

## 1. ui-conversation（会话主对话区）

### 整体骨架 `ConversationRoot`

- 三态：`hero`（空态居中）/ `settling`（加载隐藏）/ `active`（正常）。
- 结构 = 会话头部 `header` + 滚动体 `[data-conversation-scroll]` + 底部常驻输入区 `composerSeat`。
- 空态 `EmptyHero`：大鱼 `FishLogo`（34px）+ 标题「**探索未至之境**」+ 徽标「**预览版**」+ 工作区选择器 `WorkspaceChip`（文件夹图标 + 名称 + 下拉箭头，未选时显示「**选择工作区**」）+ Agent 预设座位。顶部有蓝色模糊椭圆光晕 `HeroGlow`。

### 会话头部 `ConversationSessionHeader`

- 面包屑导航 `aria-label="会话层级"`：`/` 分隔的祖先链（子代理会显示父→子链），当前项禁用，非当前项可点击跳转。
- 头部右侧：`header.actions`（后台任务等）、`header.utilities`。
- 视图标签页 `role="tablist"`：默认「**对话**」（chat）、「**轨迹**」（trajectory）。

### 消息流 `ChatView`

- 底部跟随滚动 + 「**回到底部**」悬浮按钮（↓ 图标）；顶部「**加载更早**」分页按钮；状态「**载入历史…**」/ 错误「历史加载失败：{message}（{code}）」。
- 运行中尾部状态 `TurnStatus`：文字 **"Deep diving..."**，运行 ≥15s 后追加墙钟 `· 用时 X`。

### 消息渲染

- 用户气泡（右对齐）：文本 + 图片 `ImageGallery` + 额外 JSON 块（「附加内容块」）；下方操作行 = 时间 + 复制按钮。
- `AssistantMarkdown`：GFM + KaTeX 数学公式；块类型 = `text`（MarkdownText）、`reasoning`（Think 可展开灰字摘要行）、`image`（连续图片合并成画廊）、`tool-call`（交给工具行分组渲染）、未知块 JSON。
- 图片：单张长边 240px（宽高比夹在 0.25~4），多张 64px 方块；点击打开原图 Lightbox；加载失败显示「图片加载失败，点击重试」。
- 消息操作行 `MessageIconActions`：复制（图标→1s 对勾反馈）、**分支**（「在新对话中分支」，仅完成轮最后一条可用）、时钟（`HH:mm` + 可选 `· 用时 X` `· 首 token X秒` `· X tok/s`）。
- 特殊节点：上下文注入「上下文注入」/「跨会话召回」、压缩标记「上下文已压缩」（可展开摘要）、重试倒计时卡（`重试中（1/3）· 5s` + 展开看延迟/失败原因）、「**本轮运行失败**」、「**已达到输出 token 上限**」（提示发「继续」）。

### 统计条 `StatsLine`（输入框下方，`|` 分组）

- `{turns} 轮 · {steps} 步` ｜ `LLM {时长} · 工具调用 {时长}` ｜ `首 token 平均 {时长} · {X} tok/s` ｜ `缓存命中 {X}%` ｜ `输入 {X} tok · 输出 {X} tok`。
- 超长省略，hover 显示全文 tooltip。

### 输入框 `InputBar`（composer）

- 自动增高 textarea（最多约 14 行滚动）；placeholder 随状态：默认「**给智能体发消息**」、plan 模式「描述你的任务以生成计划」、排队可插话「Cmd/Ctrl+Enter 插话发送全部排队消息」、不可用「会话不可用」、父离线、hero「描述你想要构建的内容」、无工作区「选择一个工作区开始」。
- 工具栏（左）：`+` 按钮（「命令」菜单，plus 图标）｜ plan 芯片座位 ｜ 权限/Full access 选择 ｜ 附件导轨 ｜ 左插槽。
- 工具栏（右）：右插槽 ｜ **模型选择器**（`conversation.input.model`）｜ **上下文环形仪表** `ContextMeter` ｜ 发送/停止按钮。
- 发送按钮：默认箭头↑（「发送消息」），运行中变方块■（「停止生成」）；子代理可继续会话额外独立停止按钮。
- 键盘：Enter 发送、Shift+Enter 换行、Cmd/Ctrl+Enter 插话、Cmd/Ctrl+Z/Y 撤销重做、↑↓ 历史/菜单仲裁、Esc 关闭弹层；含 IME 组合守卫。
- 附件：拖拽图片（整页 DropOverlay 遮罩「图片拖动到此处即可添加 · 最多 X 张，每张 Y」）、粘贴图片、附件缩略导轨（左右翻页箭头、hover 删除、点击看原图）、发送前校验（格式/张数/单张/总量/分辨率/模型不支持）。
- 上下文环形 `ContextMeter`：点击展开面板「上下文已用 X% · ~used/contextWindow」+ 分段色条（系统提示词/工具/对话消息 三行图例）。
- 权限芯片「访问模式，当前：{name}」，切换 Full access 前弹 `RiskConfirmation`（「确认启用 Full access？」+ 勾选「我已了解风险，并愿意继续」+「启用 Full access」）。

### 任务清单 `TodoPanel`（输入框上方 dock）

- 折叠头「**任务**」+ 进度（`{done} 已完成 · {active} 进行中 · {pending} 待处理`）；展开显示条目：完成✓圆 / 进行中旋转环 / 待处理虚线环。

### 排队消息 `QueueDock`

- 计数头「{n} 条排队消息」（可折叠）；每条：预览文本 + 编辑✎/删除🗑/插话↗（仅运行中）。

### 审批接管 `ApprovalPanel`

- 琥珀色条「**等待审批**」+ 理由标题（或「工具 {toolName} 请求越权执行」）+ 灰代码命令 + 右侧「**拒绝**」/「**允许一次**」按钮。

### 详情面板 `DetailsPanel`

- 标题（工具名或「详情」）+ 关闭「关闭详情」；空态「点击消息流中的工具行查看详情」；正文 = 「输入」JSON 代码块 + 「输出」区（终端卡/原文）；运行中「运行中…」。

---

## 2. ui-sidebar（会话侧栏）

- 顶部 `logoRow`：展开态 `BrandWordmark` 品牌字标（点击 = 新建会话），折叠态 `FishLogo` 鲸鱼 + 面板图标（收起/展开切换，aria「收起侧边栏」/「打开侧边栏」）。
- 「**新会话**」按钮（图标 + 文案）。
- 中部 `sidebar.workspaces` 槽 = 工作区/会话浏览区（见 ui-workspace）。
- 底部 `footArea`：`sidebar.footer.action`（可扩展）+ 「**设置**」按钮。
- 折叠行为：56px 竖轨（仅图标）；150ms 收起动画（内容定格宽度淡出）；滚动条跟随指针（离开 2s 后隐藏）。

---

## 3. 设置页（ui-settings / ui-settings-general / ui-settings-models / ui-settings-plugins / ui-settings-plugin-inventory）

### 外壳 `SettingsRoot`

- 触发按钮（侧栏底部「**设置**」图标+文案）。
- 居中模态面板 1080×700：左侧导航竖栏（图标 + 标签），右侧内容区（头部「打开配置文件」动作 + 关闭按钮 + 内容）。
- 导航分区（按 order）：**通用设置**(0) → **模型**(10) → **插件**(15) → **Agent 预设**(20)。

### 通用设置 `GeneralSection`（items 按 order）

- **权限**（-20）：「选择新会话的默认权限模式」+ 下拉（切换 Full access 弹风险确认）。
- **Agent 预设**（-10）：下拉选择默认预设。
- 输入行为（EnterBehaviorRow）：「繁忙时 Enter 键行为」= 排队发送/插话发送 二选一。
- **外观**（10，ui-theme）：三个立方块「**浅色**/**深色**/**跟随系统**」。

### 模型页 `ModelsSection`（重点）

- 标题「模型」+ 说明「填入各提供方的 API 密钥即可使用其模型。」+ 只读提示。
- 提供方行：显示名 +（自定义）「自定义」标签 + API Key 状态点（绿「API 密钥已配置」/ 红「API 密钥缺失」）+「编辑」/「删除」按钮。
- 底部两个并排按钮：「**添加提供方**」（下拉选已知提供方）+「**添加自定义提供方**」。
- **提供方编辑器 `ProviderEditor`**：API 密钥（password，占位「输入 API 密钥」/ 已配置「已配置——输入新值可替换」/ 环境锁定只读）→ 折叠「**自定义设置**」：显示名称 / **API 地址**（DeepSeek 默认占位 `https://api.deepseek.com`）/ **API 协议** 下拉 / 模型目录编辑器。
- **自定义提供方 `CustomProviderCard`**：字段 = **Provider ID**（占位 `acme-gateway`，校验小写字母开头+字母数字短横线）→ 显示名称 → 基础 URL（占位 `https://gateway.example/v1`）→ API 协议 → API 密钥 → 模型列表。按钮「**创建提供方**」。
- **模型列表 `ModelListEditor`**：每行 = 模型 ID + 显示名称 + 展开「容量」（上下文窗口/最大输出 token，支持 `256K`/`1M` 后缀）+ 删除；按钮「添加模型」、「**获取可用模型**」（弹窗勾选候选模型，「添加所选」）、「恢复默认模型」。
- 删除提供方弹窗：「删除 {provider}？」+ 说明 + 「删除 {provider}」。
- **首次引导**：内测声明 `WelcomeNotice`（「内测声明 / 继续」）+ DeepSeek 引导弹窗「添加一个 API Key 开始使用」（仅密钥字段，「稍后配置」/「保存并继续」）。

### 插件页 `PluginsSettingsSection`

- 标题「插件」+ 标签页（tablist）：「**插件配置**」/「**插件列表**」。
- 配置页卡片（PluginCard）：「终端」（命令超时 ms / 单流输出上限 bytes）、「Agent 循环」（并行工具调用数）、「网页搜索」（API Key / 接口地址 / 单次请求最多搜索次数）；每卡「保存」/「放弃修改」/「未保存」标记。
- 列表页 `PluginInventorySettingsTab`：搜索「搜索插件」+「插件列表」计数 + 卡片（模块短名、启停标签「已启用/已停用」、Cordis 状态点「未挂载/等待依赖/加载中/已挂载/挂载失败/卸载中」，展开看 entryId + 配置状态 + Cordis 状态）。

---

## 4. ui-theme + ui-layout（主题与布局）

### 主题

- `AppearanceRow`（浅色/深色/跟随系统三立方块）。
- 设计令牌 `--dsw-*` 双主题（`design-platform.css` 浅色/深色各一套），语义组：`--dsw-alias-bg-*`、`--dsw-alias-button-*`、`--dsw-alias-label-*`、`--dsw-alias-state-*`（success/warn/error/business）、`--dsw-alias-markdown-*`、`--dsw-specific-*`（sidebar/bubble/menu/selector）。
- 主色 `--dsw-static-deepseek-*` 蓝；品牌主色 `rgb(65,118,230)`（`--dsw-alias-brand-primary-new-colorprimary-new-color`）。

### 布局 `AppFrame`

- 三列网格 `gridTemplateColumns: [sidebar px] minmax(0,1fr) [details px]`。
- 侧栏/详情列拖拽把手（指针捕获 + rAF 节流）。
- 窄视口（< `SIDEBAR_AUTO_COLLAPSE`）自动折叠侧栏。
- `shell.overlay` 覆盖层。
- 槽：`sidebar` / `conversation` / `details` / `shell.overlay`。

---

## 5. ui-agent-preset + ui-workspace

### Agent 预设

- 选择器菜单（PresetMenu）：内置预设 = **标准模式 / PTC 模式（代码）/ 极简模式 / 创造模式**（各带描述）；用户预设名后标「**自定义**」。
- 设置页「Agent 预设」：内置/自定义分组、描述、按钮「设为默认」「查看」「复制」「删除」；复制弹窗（标识符 `my-agent` + 名称 + 组装 agent.cordis.yml 说明 +「创建」）；删除弹窗「删除该预设？」。

### 工作区/会话浏览 `WorkspaceBrowser`

- 区域头：标题「工作区」/「会话」（flat 模式）、搜索框（占位「搜索会话…」，250ms 防抖，内容搜索失败回退名称匹配）、「视图选项」菜单（分组方式：按工作区/单列表；排序方式：手动/最近更新）、「添加工作区」+ 按钮。
- 树行 `ProjectRowItem`（工作区头）：文件夹图标 + 标题 + 折叠箭头 + hover 显示 `+`（在该工作区新建会话）与 `⋯` 菜单（重命名/删除）；悬停卡显示完整路径/创建时间。
- 会话行 `SessionNodeItem`：状态点 + 标题 + 相对时间（刚刚/X分钟/X小时…前）+ `⋯` 菜单（重命名/分叉会话/归档会话）；悬停卡显示完整状态（运行中/等待审批/计划待审/等待回答/已完成/空闲/N 个子代理运行中）。
- 状态点语义 `StateDot`：绿=done、琥珀=warning、蓝=ongoing（旋转 chase）、红=error。
- 拖拽排序：工作区与会话均支持（手动排序时写 Host 顺序）；每个工作区默认显示前 5 个会话 +「展开其余 N 个会话」。
- 弹窗：重命名工作区/会话、删除工作区（说明会话转「未分组」保留）。
- 工作区选择/添加 `WorkspacePicker`：菜单列出工作区 + 「添加工作区…」；添加 = 调起目录选择流；错误弹窗「无法打开文件夹」+「重新选择」。

---

## 6. ui-input-trigger + ui-commands（斜杠命令与命令面板）

### 斜杠菜单 `MenuView`（combobox，焦点留在 textarea）

- 输入 `/` 触发；分组标题 = 「**命令** / **技能** / **子智能体**」（按 source 名）。
- 每组下条目（图标 + 名称 + 描述）；加载中组显示「正在加载…」。
- `aria-activedescendant` 键盘高亮；`role="listbox"`；`role="option"`；高度上限 320px。

### 命令选项面板 `PopupSelectView`（焦点夺取型）

- 顶部搜索框（占位「搜索…」）+ 选项列表（label + detail + 选中对勾）+ 状态「正在加载选项…/正在应用…/无选项」+ 错误「重试」按钮。
- 支持需要确认的命令弹 `RiskConfirmation`。
- 键盘：Enter/↑↓/Esc。

---

## 7. ui-primitives（基础原语清单）

导出组件：

- `StateDot`：四态状态点（done 绿 / warning 琥珀 / ongoing 蓝旋转 chase / error 红）。
- `DisclosureRow`：可展开行（图标→hover 切换箭头）。
- `Button`：variant = primary/ghost/outline/toolbar；size = md 36px / sm 28px；可选前导 icon。
- `Pill`：药丸标签。
- `Input`：输入框。
- `Menu`：菜单（条目 label/separator/label 类型；支持 portal/selectedId/footer/dense）。
- `HoverCard`：悬停卡（可 copyText 复制）。
- `Modal`：居中模态（headless 模式；Esc/遮罩关闭；portal）。
- `OnboardingSurface`：引导表面。
- `RiskConfirmation`：风险确认（标题+描述+勾选确认+确认/取消）。
- `ConnectionBanner`：连接横幅。
- `FishLogo` / `BrandWordmark`：品牌标识。
- `Tooltip`：side = top/bottom/left/right，延迟显示。
- `Toast`：瞬态提示（锚定元素，自动淡出）。
- `writeClipboard`：剪贴板工具。
- `JsonTree`：JSON 树。
- `TerminalBlock`：终端卡（maxLines/折叠/信号/退出码）。
- `ReadBlock`：读文件卡（行号 + 语法高亮）。
- `DiffBlock`：diff 卡（hunk 增删）。
- `SearchBlock`：搜索卡（分组匹配 / 路径列表）。
- `WebBlock`：网页检索卡（引用列表 / 来源视图）。
- `CodeBlock`：代码块（语言横幅 + 复制「复制/复制成功」，shiki 高亮 + KaTeX）。
- `JsonBlock`：JSON 块。
- `MarkdownText`：GFM+math 渲染（流式增量解析）。
- `MessageText`：纯文本消息。
- `extractMarkdownPlainText`：Markdown 转纯文本。
- `icons/index`：全套图标（`Icon*Outline*` 等）。

---

## 8. ui-trajectory + ui-deliverables

### 轨迹视图 `TrajectoryView`（「轨迹」标签页）

- 工具栏 `TrajectoryToolbar`：`Duration` 切换（实际时长/等宽）、`Turns` 全部折叠/展开（⊞/⊟）、`Calls` 全部折叠/展开、搜索框（占位「搜索」）。
- 记录台账 `TrajectoryTable`：事件行按类型 `SYSTEM/USER/CONTEXT/COMPACTED/ASSISTANT/TOOL/SUBTOOL`（带图标）+ 时间/时长 + 折叠摘要（轮次「N steps · N tool calls」、助手工具调用摘要）；虚拟滚动；选中行打开本地检查器。
- 检查器详情标签：
  - 请求：`Summary / Options / Usage / Timing`。
  - 消息：`Summary / Preview / Raw / Source`。
  - 工具：`Payload / Result / Schema / Timing`。
  - 系统更新：`Diff / System Prompt / Tools`。
  - 压缩：`Summary / Raw Output`。
- 计时面板：Started / Total duration / TTFT / Generation / Throughput。
- Usage 面板：This request / Session cumulative（Input / Cached / Cache created / Output / Reasoning / Content）。

### 交付物 `ProducedFiles`

- 轮次尾部「**产物**」行：最多 6 个文件 chip（basename，点击打开）+「+ N 个文件」折叠 +（回环时）「在文件夹中显示」。

---

## 9. ui-subagent / ui-workflow-run / ui-goal / ui-jobs / ui-plan

### 子代理（ui-subagent）

- 只读 composer：标题「一次性子代理记录 / 此子代理暂时只读」+ 说明（一次性任务不支持后续消息 / 父会话离线）。
- 目录动作树：可展开下级子代理，计数「N 个子代理，正在运行」，模式「一次性/可继续」，运行状态，总活跃耗时。

### 工作流运行 `WorkflowRunPanel`

- 运行头：名称 + 成员数「N 个成员」+ 状态点 + 状态文字「运行中/已完成/失败/已取消/已中断」。
- 阶段区：阶段名 + 成员数 + 状态摘要（`运行中 N · 已完成 N · …`）。
- 成员行：状态点 + 成员名 + 状态；运行中的子代理成员可点击打开会话。
- 未完成阶段强制展开。

### 目标 `GoalBar`（输入框上方）

- 目标图标 + 阶段标签（「进行中的目标/已暂停的目标/受阻的目标」）+ 目标文本 + 操作（运行中→暂停⏸ / 已暂停→恢复▶ / 编辑✎ 内联输入框 / 清除🗑）。

### 后台任务 `JobListAction`（会话头部）

- 触发按钮「N 个后台任务运行中 / N 个后台任务」+ 下拉列表。
- 每行：状态点 + 类型 kind + 标签 + 状态/详情（「运行中/正在停止/已完成/已取消/已失败」）+ 运行时长。

### 计划模式 `PlanModeControl`（输入框内芯片）

- 显示 "**Plan**" + × 关闭按钮（点击 = 执行 `/plan off`）。
- aria「plan mode 已开启，按下关闭」。

---

## 10. ui-user-questions / ui-message-feedback / ui-skill / ui-tool / ui-permission-presets / ui-attachment

### 提问卡 `QuestionComposer`

- 标题 + 眉题（eyebrow）+ 详情 Markdown + 选项（单选数字序号 / 多选复选框，「推荐」徽标）+ 自定义答案输入框（占位「输入你的答案」）。
- 分页「上一题/下一题」+「跳过本题」+「下一题/提交」。
- 校验：「请先完成这道问题」「请选择一个选项或填写自定义答案」。

### 计划审阅 `PlanReviewPanel`

- 条「**计划待审**」+ 计划 Markdown + 「**去聊天里说** / **拒绝** / **确认执行**」。

### 消息反馈 `MessageFeedbackActions`

- 👍「好的回答」/ 👎「有问题的回答」（再点取消标记）。
- 「补充说明」备注（textarea + 保存/取消）。

### 技能 `SkillRow`

- 状态点/技能图标 + 标题 "Skill" + 技能名摘要。
- 展开「说明」卡片显示指令原文 + Inspect 按钮。

### 工具 `ToolRow` / `ToolCallTree` / `GenericToolCard`

- 单行摘要行（16px 前导图标/状态点 + 标题 + · 摘要，整行可点击展开）。
- variant 图标：search 搜索 / read 浏览 / bash 终端 / write·edit 编辑 / code 代码 / others 闪光。
- 展开体 = 终端卡 / Diff / Read / Search / Web 卡，或 IN/OUT 标签卡（输入/输出）；code 变体渲染 TypeScript 代码块。
- 文件路径摘要渲染为可点击链接（打开宿主默认程序）。
- 「Inspect」跳轨迹。
- 递归渲染子调用 `subCalls`。

### 权限预设 `PermissionRow`

- 通用设置中的默认权限下拉（同 Full access 风险确认）。

### 附件（ui-attachment）

- `AttachmentRail`：横向缩略导轨 + 左右箭头 + hover 删除。
- `DropOverlay`：整页拖拽遮罩（禁用态灰色插画）。
- `MessageImage`/`ImageGallery`：单图大图 / 多图 64px 方块 + Lightbox。
- `ImageLightbox`：原图预览，可关闭。

---

## 11. ui-directory-picker-browse（目录选择浏览）

- 680×500 对话框（窄屏自适应）。
- 标题 + 面包屑（Home 起）/ 点击右侧铅笔进入**路径编辑**（`aria-label`「编辑路径」，输入即前缀过滤 + 250ms 防抖跟随跳转）。
- **Miller 双列**：左=父级列表，选中后右=子级列表，256px 底宽，选中继续下钻。
- 行：文件夹图标（选中开/未选中闭）+ 名称 + 右箭头；隐藏项（`.` 开头）默认隐藏。
- 页脚：「新建文件夹」（嵌套弹窗：名称输入 + 创建）｜「显示隐藏文件」开关（勾选）｜「取消」｜「打开」。
- 状态：「Loading…」浮层（慢扫描 300ms 才显示）、截断提示、错误提示；打开即选中文件夹（或所在层级）。

---

## 附：关键文案速查（中文源）

| 场景 | 文案 |
| --- | --- |
| 空态标题 | 探索未至之境 · 预览版 |
| 输入占位（默认） | 给智能体发消息 |
| 输入占位（无工作区） | 选择一个工作区开始 |
| 停止/发送 | 停止生成 / 发送消息 |
| 统计条 | X 轮 · Y 步 \| LLM · 工具调用 \| 首 token 平均 · tok/s \| 缓存命中 % \| 输入/输出 tok |
| 审批 | 等待审批 · 允许一次 · 拒绝 |
| 任务 | 任务 · N 已完成 / N 进行中 / N 待处理 |
| 目标 | 进行中的目标 / 已暂停的目标 / 受阻的目标 · 暂停/恢复/编辑/清除 |
| 提问 | 上一题 / 下一题 / 跳过本题 / 推荐 |
| 计划审阅 | 计划待审 · 确认执行 · 拒绝 · 去聊天里说 |
| 反馈 | 好的回答 / 有问题的回答 / 补充说明 |
| 侧栏 | 新会话 · 打开侧边栏 / 收起侧边栏 |
| 设置 | 设置 · 通用设置 · 模型 · 插件 · Agent 预设 |
| 主题 | 外观 · 浅色 / 深色 / 跟随系统 |
| 模型页 | 添加提供方 · 添加自定义提供方 · 获取可用模型 · 创建提供方 |
| 轨迹 | 轨迹 · Duration · Turns · Calls · 搜索 |
| 产物 | 产物 · + N 个文件 · 在文件夹中显示 |
