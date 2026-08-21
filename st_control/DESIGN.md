---
name: ST 控制台
description: 机台灰仪表台世界的本地数据与 AI 控制台
colors:
  bench-top: "#ecebe7"
  panel: "#f7f6f2"
  ink: "#26282e"
  night-top: "#16181d"
  dark-panel: "#1f2229"
  moon: "#dce0e8"
  brand: "oklch(0.66 0.12 205)"
  brand-strong: "oklch(0.52 0.12 205)"
  wechat-blue: "#576b95"
  success: "#16a34a"
  warning: "#d97706"
  danger: "#dc2626"
typography:
  display:
    fontFamily: "JetBrains Mono, ui-monospace, Cascadia Mono, Consolas, monospace"
    fontSize: "26px"
    fontWeight: 700
    lineHeight: 1.05
    letterSpacing: "-0.02em"
  headline:
    fontFamily: "-apple-system, \"PingFang SC\", \"Microsoft YaHei\", \"Helvetica Neue\", sans-serif"
    fontSize: "16px"
    fontWeight: 700
    lineHeight: 1.2
  title:
    fontFamily: "-apple-system, \"PingFang SC\", \"Microsoft YaHei\", \"Helvetica Neue\", sans-serif"
    fontSize: "15px"
    fontWeight: 700
    lineHeight: 1.2
  body:
    fontFamily: "-apple-system, \"PingFang SC\", \"Microsoft YaHei\", \"Helvetica Neue\", sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "-apple-system, \"PingFang SC\", \"Microsoft YaHei\", \"Helvetica Neue\", sans-serif"
    fontSize: "11px"
    fontWeight: 600
    letterSpacing: "0.14em"
    textTransform: "uppercase"
rounded:
  sm: "8px"
  md: "10px"
  lg: "12px"
  xl: "16px"
  full: "9999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "20px"
components:
  button-primary:
    backgroundColor: "{colors.brand-strong}"
    textColor: "#ffffff"
    rounded: "{rounded.md}"
    padding: "8px 16px"
  button-secondary:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
    padding: "8px 16px"
  card:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    padding: "16px 18px"
  meter-card:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    padding: "14px 16px"
---

# Design System: ST 控制台

## Overview

**Creative North Star: "个人仪表台（Bench Console）"**

ST 控制台是一块摆在日间工作台上的个人仪表台：机台灰的台面承载一排插在机架上的仪表，骨白仪表面板是每一块内容卡，炭黑刻线墨承担正文，深青蓝只作为“活体”指示灯与主操作出现。用户像操作精密仪表一样使用它——状态靠指示灯，数据靠等宽读数，操作靠那颗明确的主键。

本系统服务于 Operate 模式：Windows 桌面控制台（Tauri + WebView2），用户在 1600×1000 窗口里高频扫描微信数据、LLM 用量、Agent、自动化与知识库。可扫描性与一致性优先；所有颜色走 `--app-*` / 语义令牌，个性化主题（11 组背景 + 8 组文字 + 字体 + 透明度）仍然实时联动。**明暗双态共用同一套仪表台组件语言**：浅色默认主题「仪表台」，深色旗舰主题「仪表台深色」（`#16181d` 夜航台面 + `#1f2229` 深面板 + 月白正文）；全部派生令牌从主题自身颜色推导，任何主题下都不会出现“白底亮字”或“深底暗字”。

**Key Characteristics:**
- 机台灰台面 + 骨白面板 + 炭黑刻线墨；层级靠发丝级规则线与字重，不靠装饰。
- 青绿稀有：accent 只用于在线状态点、关键数字、选中项与主操作。
- 等宽读数：仪表数值、端口、时间戳、版本等“测量值”用 mono 呈现。
- 令牌单一来源：`src/app.css` 的 `:root` / `.dark`，联动 `--app-*` 个性化变量。
- 动效克制：状态反馈（LED 点亮、按键按下位移、数值滚动）为常态；作者性动效只出现在启动页等一次性场景。

## Colors

机台灰地面 + 骨白仪表面板 + 炭黑墨 + 深青蓝主操作，语义色只做文字/边框/徽标。

### Primary
- **深青蓝 Primary**（`oklch(0.52 0.12 205)`，约 #007b89）：主按钮、主链接、选中态、关键数值。白字对比度 ≥4.5:1。
- **青蓝 Brand**（`oklch(0.66 0.12 205)`，约 #22d3ee）：LED 状态灯、焦点环、活体指示；不承载正文。

### Secondary
- **微信蓝**（`#576b95`）：仅微信模块的品牌强调（命名语义令牌 `--app-wc-accent`）。

### Neutral
- **机台灰 Bench Top**（`#ecebe7`）：全局背景、内容区台面。
- **骨白 Panel**（`#f7f6f2`）：卡片、弹层、输入表面。
- **炭墨 Ink**（`#26282e`）：正文、标题、关键数字（对骨白 ≥12:1）。
- **Muted**（ink 64% 混入灰）：次要文字（≥4.5:1），全部经 `color-mix` 派生。

### Dark 中性
- **夜航台面 Night Top**（`#16181d`）：「仪表台深色」背景。
- **深面板 Dark Panel**（`#1f2229`）：深色下的卡片、弹层、输入表面（`--card` / `--popover` 同一来源）。
- **月白 Moon**（`#dce0e8`）：深色下正文与标题。
- 深色下边框 = 月白 18% 发丝线、输入描边 = 月白 32%、侧栏 = 台面 70% 混银灰——全部经 `color-mix` 从文本色派生。

### Named Rules
**The 青绿稀有 Rule.** 青绿只出现在“活体”语义上：在线状态点、关键数字、选中项、主操作。任何把青绿铺成背景或装饰的做法都是违规。

**The 令牌唯一 Rule.** 新增颜色必须走 `--app-*` / `--brand` / `--primary` / 语义令牌，禁止硬编码色板；模块私有变量（`--kb-*` / `--wc-*` / `--dg-*`）一律从 app 令牌派生。

**The 浮层跟随 Rule.** `--popover` 必须等于 `--card`：浅色弹窗骨白、深色弹窗深面板，任何主题下弹层与浮层都不能写死浅色。

**The 同色系派生 Rule.** 中性表面（侧栏/悬浮/次要底）只能在主题自身的 `bg ↔ card` 之间插值，次要文字只能从 `fg` 派生——禁止混入固定灰/米灰，否则暖棕、勃艮第、墨金等有色调的深色主题会“发脏、偏色”。

**The 首帧成色 Rule.** `index.html` 首帧脚本必须同时写入 `--app-bg-color` / `--app-font-color` / `--app-color-card-bg`（卡片色表与 PreferencesPanel 保持一致），并做明暗配对自适应（深底+深字自动换浅字），保证用户不打开设置也能得到协调配色。

## Typography

**Body Font:** 系统无衬线栈 `-apple-system, "PingFang SC", "Microsoft YaHei", "Helvetica Neue", sans-serif`（不加载外部字体，避免网络阻断首屏）。
**Mono Font:** `JetBrains Mono, ui-monospace, Cascadia Mono, Consolas`，仅用于数据、时间戳、端口、版本等测量值。

**Character:** 单一字体家族承担全部层级，等宽字体只做“仪表读数”；扫描型工具不需要展示字体对，层级由字重与字号承担。

### Hierarchy
- **Display / 仪表读数** (Mono 700, 26px, 1.05, -0.02em)：统计数字，`tabular-nums` 平滑滚动（LiveNumber）。
- **Headline** (700, 16px, 1.2)：面板大标题、首页标题。
- **Title** (700, 15px, 1.2)：面板头部标题（纯色，禁止渐变文字）。
- **Body** (400, 14px, 1.5)：正文；长文行宽 65–75ch。
- **Label** (600, 11px, 0.14em, 大写)：刻字式微标签（仪表卡刻度、导航分组、快捷操作标题）；`letter-spacing` 仅用于这类标签。

## Layout

- 窗口 1600×1000（最小同尺寸，桌面专用，无移动端断点）。
- 顶栏 38px（自定义标题栏：LED 品牌块 + 窗口控制 + 拖拽区）→ 左侧导航 232px（可折叠 64px）→ 内容区。
- 内容区 padding 16px，面板间 gap 12px；面板独立滚动，全部面板同帧渲染、隐藏非活跃（后台任务不中断）。
- 面板内部组织：头部（PanelHeader，底部发丝线）→ 工具栏单行 → 表格/卡片网格 → 分页。
- 首页（工作台）结构：状态带（运行状态 + 端口）→ 四块仪表卡（LED + 等宽读数）→ 机架式快捷入口 → Agent/事件记录。
- 微信面板内部：左导航分组（会话/智能/数据/订阅/总结/安全）→ 会话列表（260px）→ 主内容区。

## Elevation & Depth

机台系统默认扁平：**层级靠边框与色差，不靠阴影**。阴影只用于浮层（弹窗、下拉、toast、抽屉），且必须带偏移 + 柔和模糊。

### Shadow Vocabulary
- **浮层**（`0 18px 50px rgba(35,48,44,0.22)`）：Dialog / 大模态。
- **抽屉**（`12px 0 32px rgba(35,48,44,0.22)`）：侧边栏抽屉。
- **Toast**（`0 6px 24px rgba(0,0,0,0.3)`）：右下角通知。

**The 单次声明 Rule.** 卡片只声明边框或阴影之一，禁止“1px 边框 + 宽软阴影”的幽灵卡。

## Shapes

- 圆角基准 `--radius: 0.75rem`（12px）：卡片 12px（`--radius-lg`），弹层 16px（`--radius-xl`），小控件 8–10px（`--radius-md/sm`），药丸仅用于徽标/状态。
- 发丝规则线：边框统一为 `color-mix(ink 13%, transparent)` 的浅机台灰刻线。
- 选中指示：导航左侧 3.5px 主色条仅作为激活态语义，不做装饰。
- 仪表卡特征：左上刻字标签 + 右上 LED 状态灯 + 等宽大读数。

## Components

### Buttons
- **Shape:** 圆角 10px（`--radius-md`）。
- **Primary:** 深青蓝底 + 白字（对比度 ≥4.5:1），padding 8px 16px。
- **Secondary / Ghost:** 骨白/透明底 + 发丝边框，悬停浅青 wash。
- **仪表台平键（wc-key）:** 微信全站统一按钮：骨白键面 + 发丝边框 + 按下位移 1px（`:active`）+ 青蓝 hover 语义；激活态 `wc-ihb-active` = 青蓝描边 + 浅青底。
- **深色状态:** 深色下键面/卡片/输入随 `--card` 变深面板；主按钮仍为深青蓝底白字；LED 状态灯在深底改用更亮的青蓝（hue 190、95% 亮度）保证可见。
- **明暗配对:** 背景主题与文字主题自动配对——深色背景配浅色文字（默认 moon），浅色背景配深色文字（默认 ink），避免不可读组合。

### Cards / Containers
- **Corner Style:** 12px（`--radius-lg`）。
- **Background:** 骨白 `--card`（机台灰台面在卡片间隙透出）。
- **Border:** 1px 发丝刻线；**无阴影**（见 Elevation）。
- **Internal Padding:** 16/18px；仪表卡 14/16px。

### Meter Cards（首页/统计）
- 结构：刻字标签（11px 大写 + 0.14em 字距）→ LED 状态灯（活体点亮带微光）→ mono 大读数（26px）。
- 语义色：今日/在线类用深青蓝，告警用琥珀，完成用成功绿；不铺霓虹。

### Inputs / Fields
- **Style:** 骨白底 + `--input`（ink 27%）细边框，圆角 10px。
- **Focus:** 青蓝双层焦点环。
- **Error / Disabled:** danger 边框 + danger/10 背景；禁用 opacity 0.48。

### Navigation
- 全局侧栏：机台灰 `--sidebar`（比台面深一档），分组标签刻字式小号追踪字距；hover 浅青 wash，激活项浅青 wash + 左侧 3.5px 主色条（无光晕）。
- 微信内导航：6 组分组（会话/智能/数据/订阅/总结/安全）+ 设置置底；激活项左侧 2px 主题色条。

### Status Lamps
- LED 圆点（8px）+ 标签：运行/在线=青蓝或成功绿点亮带微光；警告=琥珀；错误=红；停用=灰（无光）。
- 深色下 LED 光晕更强（提高亮度/光晕半径），浅色下保持克制的 8px 微光。

### Startup（微信启动页）
- 骨白/深面板 + LED 进度条（微信蓝），一次 reveal 动画为唯一作者性动效。
- **星尘背景跟随主题**：浅色=浅灰底 + 深青星点（星图），深色=夜空深底 + 亮青星点；`prefers-reduced-motion` 下静止。

## Do's and Don'ts

### Do:
- **Do** 让机台灰与刻线说话：卡片 = 骨白面板 + 发丝边框，层级 = 字重与留白。
- **Do** 把青蓝留给“活体”：在线、关键数字、选中、主操作。
- **Do** 新颜色一律走令牌（`--app-*` / `--brand` / `--primary` / 语义色）。
- **Do** 仪表读数、端口、时间戳等测量值用等宽字体 + `tabular-nums`。
- **Do** 尊重 `prefers-reduced-motion`：关闭装饰动画；状态反馈（LED、位移）保留。
- **Do** 长文本预览用两行截断（line-clamp + word-break），避免单行无限溢出。
- **Do** 深色下让全部表面从 `--card` 派生（卡片/弹层/输入），边框从文本色派生发丝线。
- **Do** 星空/星尘画布按主题亮度切换：浅底深星、深底亮星，并监听 `--app-*` 变化实时切换。

### Don't:
- **Don't** 使用渐变文字、彩色 border-left >1px、硬偏移阴影、逐项光晕描边。
- **Don't** 把装饰性动效铺满界面；每屏至多一次作者性动效。
- **Don't** 在浅色机台世界使用深色控制台的霓虹辉光色板（亮青铺底、深蓝背景）。
- **Don't** 引入与令牌无关的硬编码色板或独立暗色背景。
- **Don't** 用 unicode 字形/emoji 代替图标体系（构建中微信消息类型等仍残留少量字形图标，属待清理缺陷，不构成系统规则）。
- **Don't** 在深色主题下写死浅色背景（白弹窗、白卡片、白输入）；任何浮层背景跟随 `--popover`。
