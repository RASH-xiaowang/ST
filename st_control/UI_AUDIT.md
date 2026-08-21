# ST 控制台 UI 审查报告

> 审查时间：2026-08-11 · 审查方式：运行真实应用（Tauri 调试版 + CDP 运行时探测）+ 源码级扫描
> 范围：主题 / 外壳 / 12 个主面板 / 微信子模块 / 弹窗 / 搜索 / 设置

## 0. 结论摘要

应用可以正常运行，数据真实（微信消息、LLM 用量、OCR 记录、系统监控均在渲染）。当前视觉世界「标本纸」方向自洽，但存在三类系统性问题：

1. **一致性被打破**：同一应用里混着纸面卡片、FancyUI 辉光/聚光/扫光/星光、微信面板的彩虹按钮、AI 角色的 emoji 图标，共四套互不相关的视觉语言。
2. **可读性被牺牲**：大量 9–11px 字号（微信面板 160 处、数据看板 9px 图表、自动化 37 处）、114 处硬编码颜色（WeChatPanel）、105 个按钮同屏（微信面板）。
3. **布局细节失修**：侧边栏导航每屏多出 11px 滚动、KB 内容区 185px 纵向溢出、数据看板内容区 843px 纵向溢出、部分长文本横向溢出。

## 1. 运行时实测（CDP 逐面板采集，窗口 1600×1000）

| 面板 | 卡片 | 按钮 | 主字号分布 | 备注 |
|---|---|---|---|---|
| 平台首页 | 16 | 19 | 13/12.5/12/10.5px | 首页空旷：4 张统计卡 + 2 张空状态卡 |
| AI 聊天 | 2 | 24 | 13/10.5/11/12px | 布局最简 |
| AI 文案 | 5 | 34 | 13/11.5/18/14px | 场景卡 12 个，emoji 图标 |
| 智能体 | 7 | 28 | 14/13/12/10.5px | — |
| AI 角色 | 8 | 25 | 13/12/10.5/14px | emoji 头像/图标（🎭🔍✎🗑） |
| 大模型 | 2 | 25 | 13/12/10.5/11px | 配置路径省略号异常 |
| 自动化 | 20 | 27 | 12/13/24/10.5px | 实时消息流 + 8 项统计 |
| 微信数据 | 0 | **105** | 12/13/10/14px | **40 处溢出，105 按钮，18 个子导航** |
| 知识库 | 16 | 37 | 11.5/11/12.5/13px | 内容区 185px 纵向溢出 |
| 数据看板 | 34 | 19 | 12/11/**9px**/13px | 内容区 843px 纵向溢出，9px 标签 28 处 |
| 数据库 | 1 | 41 | 12/13/11/14px | 表格密集，emoji 图标 |
| 图文识别 | 6 | 36 | 14/12/13/22px | — |

控制台告警：`[wechat:bus] WebSocket 已关闭`（微信实时总线断开时的常规告警，建议降噪）。

## 2. 系统性问题（按优先级）

### P1-1 四套视觉语言互相打架
**证据**：App.svelte 里每个激活导航项都套 `GlowBorder`（约 12 处）+ 标题栏 `SparklesText` + `GlowBorder` + 底部扫光线；首页 4 张卡 `CardSpotlight`；标题副文案 `FlipWords` 每 2.8s 换词；微信面板把 `RainbowButton`（彩虹描边 + 辉光 + 44px 深底）用在几乎所有操作与导航上；AI 角色/数据库/文案面板用 emoji 当图标。
**影响**：用户难以分辨“主操作 / 状态 / 装饰”，动画总量与 Operate 工具的场景不符，也违反 DESIGN.md 自己定的「每屏至多一次作者性动效」「青绿稀有」「禁 emoji 图标」三条规则。
**修复方向**：统一为一套组件语言；动效收敛到“状态反馈 + 至多一处作者性动效”；图标全部换 lucide/phosphor 同权重线性图标。

### P1-2 字号过小，可读性不足
**证据**：全库 `font-size < 12px` 共 160 处（WeChatPanel）、37（Automation）、33（DailySummary）、29（DbManager）、25（DataDashboard）、25（OcrPanel）等；数据看板图表标签 9px 共 28 处；微信面板 10px 时间戳 62 处。
**影响**：主界面信息密度过高，扫描成本上升，不符合桌面 1600×1000 的舒适阅读距离。
**修复方向**：正文/表格底线 12.5–13px，辅助信息 12px，图表标签 ≥11px；层级靠字重而非进一步缩小。

### P1-3 硬编码颜色破坏主题联动
**证据**：WeChatPanel 114 处、App.svelte 50 处（部分为令牌兜底）、WikiPanel 45 处、kbui.css 39 处、DailySummary 32 处；微信里 `#888/#2196f3/#07c160/#fff` 等直接写死。
**影响**：换个性化主题时这些区域不跟随，违背「令牌唯一」规则。
**修复方向**：微信模块继续从 `--app-*` 派生 `--wc-*`；残余硬编码收敛为语义令牌（如 `--wc-msg-bg`、`--wc-online`）。

### P1-4 布局溢出未修
**证据**：
- 侧边栏 `.nav-list` scrollHeight 721 / clientHeight 710（每屏多 11px，出现无意义滚动条）。
- 知识库 `kb-content` 185px 纵向溢出（内容比滚动容器高）。
- 数据看板 `dvr-root` 843px、`dvr-disks` 105px 纵向溢出。
- 大模型配置路径 `truncate` 失效（`hidden xl:inline` 组合覆盖了 `overflow:hidden` 语义，路径横向溢出 137px）。
- 微信聊天预览长文本（URL / 多行消息）横向撑出容器（最宽溢出 1304px）。
**修复方向**：分别修正 flex `min-width:0`、滚动容器高度链、`truncate` 的类组合，给聊天预览加 `word-break` 与最大宽度。

### P2-1 首页信息价值低
**证据**：首页只有 4 张统计卡 + 2 张“暂无 Agent/事件”空卡；`消息总量 9,786`、`监听端口` 等静态数字；无任何可操作入口。
**影响**：最常用的入口页没有承担“起点”职责。
**修复方向**：首页改为“工作台”：状态带 + 关键指标 + 快捷操作（AI 聊天、新建智能体、微信同步、OCR 接收）+ 最近事件流。

### P2-2 认知负荷过载点
**证据**：
- 微信面板同屏 105 个按钮、18 个子导航分类（聊天/AI问答/关系图谱/群监控/通讯录/朋友圈/收藏/表情/文件/记录/公众号/服务号/客服/年度总结/每日总结/原图Hook/隐私体检/备份管家/设置），超出工作记忆上限（≤4–7）。
- 自动化面板同屏 8 个统计项 + 实时流 + 规则/任务双页签。
**修复方向**：微信主导航按“数据 / 分析 / 管理 / 设置”分组折叠；统计条统一组件，允许横向滚动。

### P2-3 状态与反馈不一致
**证据**：微信面板“运行中/监控运行中/DB 状态/WS:0”等状态分散在头部与侧栏多处；彩虹按钮 hover 无语义；部分操作只有 toast 文案。
**修复方向**：统一状态徽标组件（dot + 文案 + 语气色），操作反馈统一 toast + 内联结果。

## 3. 已确认的代码级问题清单（修复时逐项核对）

| # | 文件 | 问题 |
|---|---|---|
| 1 | App.svelte:940-946 | `.nav-list` 内容 11px 溢出，出现无意义滚动条 |
| 2 | App.svelte 各导航项 | 激活态 `GlowBorder` ×12，动效超限 |
| 3 | App.svelte 标题栏 | SparklesText + GlowBorder + 扫光三重装饰动效 |
| 4 | App.svelte 首页 | CardSpotlight ×4 + FlipWords，首页空 |
| 5 | WeChatPanel.svelte | RainbowButton 用于全部导航/操作（44px 深底彩虹按钮）；114 处硬编码色；160 处 <12px 字号；聊天预览横向溢出；105 按钮 |
| 6 | WeChatPanel.svelte:5538 | `.wc-chat-list` 滚动容器高度链断裂 |
| 7 | kb/WikiPanel.svelte + kbui.css | `kb-content` 185px 溢出；45+39 处硬编码色 |
| 8 | DataDashboard.svelte | 9px 标签 28 处；`dvr-root` 843px 溢出 |
| 9 | llm/components/ProviderConfigTab.svelte | 路径 truncate 失效（`hidden xl:inline` 覆盖） |
| 10 | llm/components/AiRolesPanel.svelte | emoji 图标（🎭🔍✎🗑）与 emoji 头像 |
| 11 | DbManager.svelte | emoji 图标（🗂🔒★📄 等）29 处 <12px |
| 12 | AutomationPanel.svelte | 37 处 <12px；8 统计项无组件化 |
| 13 | OcrPanel.svelte | 25 处 <12px |
| 14 | copywriting/AiCopyPanel.svelte | 场景卡 emoji 图标 |
| 15 | chat/AiChatPanel.svelte | 结构偏简单，需与全局聊天统一 |
| 16 | Search/GlobalSearch.svelte | 12 处 <12px |
| 17 | 控制台 | `[wechat:bus] WebSocket 已关闭` 告警需降噪/重连提示 |

## 4. 优化建议（方向无关，立即生效）

1. **统一组件语言**：只保留一套按钮/卡片/输入/徽标/表格规范；FancyUI 仅用于“至多一处作者性动效”。
2. **令牌优先**：所有颜色走 `--app-*` / 语义令牌；微信模块继续派生 `--wc-*`。
3. **字号底线**：正文 ≥13px、辅助 ≥12px、图表 ≥11px；用字重与留白分级。
4. **修溢出**：侧边栏、KB、数据看板、长文本预览四项布局修复。
5. **图标化**：emoji → lucide-svelte / phosphor-svelte 线性图标，统一 1.5px stroke。
6. **重做首页**：从“空监控”变为“工作台”，承载状态 + 关键指标 + 快捷入口 + 事件流。
7. **收敛导航**：微信 18 个子导航分组折叠；全局侧栏保持 4 组。
8. **动效收敛**：prefers-reduced-motion 一律停用装饰动画；默认去掉逐项辉光。

## 5. 重设计范围（已确认方向：仪表台 Bench Console，seed b9f46f82）

用户已确认方向为「仪表台 Bench Console」，以下重设计批次已执行：

- **主题层**：`src/app.css`（令牌、材质、圆角、阴影、字体）
- **外壳**：`App.svelte`（标题栏/侧栏/内容区/首页/弹窗/通知）
- **共享组件**：`PanelHeader`、`FancyCard/FancyStat`（或替换）、`GlobalSearch`、`SettingsModal`、`ApiHelpModal`
- **12 个主面板**：monitor / ai_chat / ai_copy / agents / ai_roles / llm / automation / wechat / kb / data_dashboard / db_manager / ocr
- **微信子模块**：WeChatPanel、AskPanel、DailySummary、AnnualSummary、GraphView、GroupMonitor、HookManager、PrivacyScan、RelationshipGraph、WeChatConfig、BackupManager、GeneralRecords
- **KB 子模块**：KbDashboard、KbDocs、KbChat、KbSettings、KbActivity、WikiPanel、kbui.css

### 已执行的关键变更

| 区域 | 变更 |
|---|---|
| 视觉世界 | 机台灰台面 `#ecebe7` + 骨白面板 `#f7f6f2` + 炭墨正文 + 青蓝仅作活体指示/主操作 |
| 标题栏 | LED 品牌块 + 刻线分隔，去掉 SparklesText/GlowBorder/扫光 |
| 侧栏 | 平键导航（激活=浅青 wash + 3.5px 指示条），修复 11px 无意义滚动 |
| 首页 | 状态带（运行中/端口）+ 4 块仪表卡（LED + 等宽读数）+ 8 个快捷入口 |
| 微信面板 | 18 子导航分 6 组；100+ 彩虹按钮统一为骨白平键；聊天预览两行截断；状态徽标令牌化 |
| 知识库 | 令牌对齐仪表台；最小字号升至 11.5px |
| 数据看板 | 9px 图表标签升至 11px |
| 自动化 | 统计卡去聚光改平卡，霓虹色改语义色 |
| 图标 | AI 角色 / 数据库 / 文案 / 关系图谱等 emoji 换 lucide 线性图标 |
| 启动页 | 微信启动页去掉 GlowBorder/Meteors，改 LED 进度 |

### 收尾批次（第二次运行补充）

| 项 | 结果 |
|---|---|
| 微信消息类型图标 | 文件/图片/语音/视频/链接/位置/笔记/收藏/设置分类等 emoji 全部替换为统一 1.6-stroke 线性 SVG |
| 子面板字形图标 | 备份管家/通用记录/隐私体检/关系图谱/群监控/问答等卡片标题、空状态、错误提示去 emoji |
| Toast 前缀 | 微信 23 处 ✅/❌ 前缀移除（由 toast 颜色承担语义） |
| 最小字号 | 全库 `font-size ≤11px` 由 396 处降至 0（9px 图表标签清零；热力图计数等微数据 ≤10.5px） |
| 生产构建 | `npm run build` 通过；`svelte-check` 0 错误 |
| 运行时复测 | 首页/数据库/图文识别裁切 0；其余仅正常滚动与省略号 |

> 本报告为「优化建议」交付物；重设计已按确认方向落地并通过最终验证，DESIGN.md 与侧车已重写为仪表台世界。

## 6. 深色模式重设计（追加批次）

### 问题
- `--popover` 写死浅白 `#faf9f6`：深色主题下弹窗/下拉/抽屉为“白底亮字”，不可读。
- 星尘/星空画布写死浅底（`hsla(210,12%,92%)`）+ 深青星点：深色主题下会画出一块浅色面板。
- 部分浅色定制值（开关旋钮白芯、文档预览 iframe 白底、视频播放遮罩）属于“该白”的元素，保留。

### 已修复
| 项 | 结果 |
|---|---|
| 新增主题 | 「仪表台深色」bench-dark：`#16181d` 台面 + `#1f2229` 面板，设置→个性化可选 |
| 浮层令牌 | `--popover = --card`（明暗双态跟随）；`.dark` 兜底改为夜航仪表台深色值 |
| 深色派生 | 边框/输入/侧栏/次要文字全部从 `--app-*` 推导，深色下为月白发丝线与银灰侧栏 |
| 星空自适应 | 微信空状态星尘 + 启动页星尘按 `--app-bg-color` 亮度切换：浅底深青星点 / 深底亮青星点，MutationObserver 实时跟随主题切换 |
| 验证 | svelte-check 0 错误；`npm run build` 通过；深色运行时实测：卡片 `#1f2229`、弹层跟随、星空 avgLum 40（夜空）+ 0.5% 亮星 |

深色截图：`E:\ST\.codex_shots\dark_20260811\`（首页/AI 聊天/大模型/自动化/微信/知识库/数据看板/数据库/微信星空）。

### 追加修复：深色主题配色不协调（根因与验证）

**根因**：设置弹窗为 `{#if open}` 条件挂载，用户从未打开设置时 `applyPrefs()` 不执行，`--app-color-card-bg` 一直停留在浅骨白兜底 → 深色主题下“外壳深、卡片白、亮字白”的割裂观感；此外首帧只写背景/文字色，不写卡片色，且深色背景可搭配深色文字（如 ink）直接不可读。

**修复**：
| 项 | 结果 |
|---|---|
| 首帧卡片色 | `index.html` 首帧新增 CARD 表（12 主题 → 卡片面），与 PreferencesPanel 同步，冷启动即写 `--app-color-card-bg` |
| 兜底派生 | `--app-color-card-bg` 兜底改为 `bg 88% + fg` 派生，深底自动深面板 |
| 同色系表面 | 侧栏 = `card 45% + bg`、muted = `card 88% + bg`（色相 Δ0），次要文字 = `fg 60% 透明`（色相跟随正文） |
| 明暗配对 | 首帧 + PreferencesPanel 双处自动适配：深底+深字 → moon，浅底+浅字 → ink |
| 星空底色 | 星尘画布底色直接取主题 `--app-bg-color`，暖棕/勃艮第等主题不再出现冷蓝面板 |

**实测（不打开设置、冷启动）**：全部深色主题卡片亮度 0.08–0.13（深面板）、文字亮度 0.88（自动浅色）、正文对比度 5.07–7.12（≥4.5）；侧栏与背景色相差 0°。

## 7. 设置界面重设计（追加批次）

### 框架与位置
- **固定尺寸**：设置弹窗 960×720（上限 88vh），窗口内水平垂直居中（实测 2560×1392 下 frame = 960×720 @ (800,336)）。
- **两栏结构**：左侧导航栏 208px（刻字分组标题 + 图标项 + 激活主色条 + 底部服务状态灯）+ 右侧内容区 750px 独立滚动。
- 页头统一：每页「刻字页头（标题 + 说明 + 计数徽标）」→ 内容卡。

### 每页重排
| 页签 | 布局 |
|---|---|
| 常规 | 服务卡（状态 LED + 端口输入）+ 关于卡（dl 双栏） |
| 个性化 | 字体/背景/文本色卡 + 透明度滑杆；色卡文字按色卡亮度自动取深/浅（修复 OLED 等深色卡深字不可读） |
| 服务器 | 4 格仪表统计（监听地址/应用名/版本/状态，mono 值） |
| Agent 日志 | 控制台式日志行（mono 时间戳 + 方向徽标 + 省略号正文），计数在页头 |
| 数据库 | 4 格统计（引擎/大小/事件/日志）+ 路径卡 + 参数双栏表单 + 保留/清理动作行 |
| 微信配置 | 自包含页签嵌入；补齐缺失令牌（`--app-radius-*`/`--app-shadow-sm`/`--app-color-accent`），卡片 12px 圆角、微信蓝品牌块、状态徽标令牌化 |

### 修复
- 设置弹窗由 980px 浮动改为固定 960×720；移除旧 `.modal-settings/.settings-*` 孤儿样式。
- 微信配置此前引用未定义令牌导致圆角/阴影/图标色失效，已在 App.svelte `:root` 补齐。
- 验证：svelte-check 0 错误、`npm run build` 通过、六页签运行时实测渲染正常。

截图：`E:\ST\.codex_shots\dark_20260811\settings\`（general/personalize/server/log/database/wechat）。

## 8. CDN 原图配额放开（追加批次）

按用户要求彻底移除客户端每日 10 张限制：

| 项 | 结果 |
|---|---|
| 配额代码 | `cdn_image.rs` 删除 `DAILY_DOWNLOAD_LIMIT`、`QUOTA_LOCK`、`quota_used/quota_consume/quota_remaining` 及下载前的上限拦截 |
| IPC | `get_cdn_image_status` 不再返回 `dailyLimit`，仅返回 `enabled`（前端本就不使用该字段） |
| 行为 | 本地无原图时不再有任何客户端每日计数，CDN 下载结果仍按 fileid 缓存；`is_cdn_enabled` 开关保留 |
| 验证 | `cargo build` 通过；重启后经 CDP 实测 `get_cdn_image_status` 返回 `{"enabled":true}` |

说明：旧 `.cdn_quota.json` 计数文件不再被读取，可留可删（不影响行为）。

## 9. CDN 原图「本地 AES-ECB 解密」（追加批次）

按用户要求实现 c3o.re 作者建议的「怕不安全自己写 aes ecb」：aeskey 不再发给第三方，改为在本地解密。

### 实测确认的接口语义（用真实图片消息验证）

- `type=orig`（不带 key）：返回原始加密字节（179152 = 明文 179146 + PKCS7 补 6）
- `type=orig&key=<32位hex>`：服务端 AES-ECB 解密后的原图
- `type=file`：对聊天图片不可用（超时 / Not Found）

本机解密验证：AES-128-ECB + PKCS7 去填充后长度 179146、md5 2a1fede873ce35dd0f201b6429c61337，与消息 XML 的 hdlength/md5 完全一致。

### 实现

- 解密函数：`image.rs` 新增 `aes_ecb_decrypt_file`（按 key 长度选 AES-128/192/256，整段 ECB + PKCS7 去填充）与 `decode_cdn_aes_key`（hex/原始字节）
- 下载路径：`cdn_image.rs` 本地解密模式改为 `type=orig` 不带 key；若返回已是图片魔数则直接使用（兼容非加密图）
- 解密开关：`.cdn_settings.json` 新增 `localDecrypt`（默认 true），`get_cdn_image_status` 返回 `localDecrypt`，新增 `set_cdn_image_local_decrypt`
- 设置 UI：微信配置页新增「原图解密方式：本地解密 / 服务端解密」开关
- token 账号探测：修复「文件夹选取错误」——服务端要求 `weixinIDFolder` 等于当前登录微信目录，失败后自动枚举同级 `wxid_*` 目录逐个尝试并缓存成功目录

### 验证

- `cargo build`、`npm run build`、`svelte-check`（0 错误/174 警告，与基线一致）通过
- 运行时 CDP 实测：`get_cdn_image_status` 返回 `{"enabled":true,"localDecrypt":true}`；开关往返切换均持久化；设置 → 微信配置页「原图解密方式 / 本地解密」已渲染
- 截图：`E:\ST\.codex_shots\settings_wechat_toggle.png`

## 10. 微信空状态背景：星尘 → Gargantua 黑洞（追加批次）

按用户要求，把微信「未选会话」空状态的 1200 颗青蓝星尘 Canvas 换成 Kimi 分享的
GARGANTUA 黑洞实时光线追踪（Schwarzschild 黑洞 + 吸积盘 + 引力透镜）。

### 实现

- 资源整体移植到 `public/gargantua/`（index.html / css / js / vendor/three.module.js + jsm 插件），
  完全本地运行，无外部请求；已去掉原页面的 Kimi SDK 与音频依赖。
- `main.js` 新增 `?bg=1` 纯背景模式：隐藏 HUD/开场/提示/控制台，关闭指针与键盘交互，
  不创建音频元素，默认电影镜头自动循环；配合 `q=standard`（200 步/DPR 1）保证流畅。
- 新组件 `GargantuaBackdrop.svelte`：全尺寸 iframe + 兜底深空渐变；`pointer-events:none`。
- `WeChatPanel.svelte`：删除旧 `$effect` 星尘动画与 `.wc-ns-canvas`，空状态改挂新组件，
  保留「从左侧选择一个会话」底部提示。

### 验证

- CDP 独立页面：`ready/hudOff/deckHidden` 全部生效，无 fatal，WebGL 可用；
  截图采样 4320 色、中央高光像素（吸积盘），非黑屏。
- 应用内：点击微信数据 → 空状态 `.ga-backdrop iframe` 挂载、提示文字保留；
  截图微信区域 5967 采样色、10805 高光像素。
- `svelte-check` 0 错误/174 警告（与基线一致）；`npm run build` 通过，`dist/gargantua` 14 个文件。

截图：`E:\ST\.codex_shots\gargantua_bg.png`、`E:\ST\.codex_shots\wechat_gargantua.png`。

## 11. 全应用星空图统一替换为黑洞（追加批次）

用户要求「所有星空图都换成黑洞」，除第 10 批的微信空状态外，补齐其余两处：

| 位置 | 处理 |
|---|---|
| 微信启动页 `WeChatBootstrap.svelte` | 删除 300–500 星尘 Canvas 动画（含主题监听），改挂 `GargantuaBackdrop`；卡片 z-index 保持在上层 |
| `AnimatedBackground.svelte`（fancyui-migration 演示组件） | `stars` / `falling-stars` 两档改为渲染 `GargantuaBackdrop`，其余模式（网格/流星/粒子/矩阵）不动 |

### 验证

- 运行中实测：点「微信数据」250ms 内启动页 `.wbs .ga-backdrop iframe` 挂载，
  截图 883 采样色、5680 高光像素（黑洞渲染正常，卡片在上层可读）。
- 全局搜索确认不再有星尘/星空 Canvas 实现（仅剩 Gargantua shader 内部的星空贴图，属于画面本身）。
- `svelte-check` 0 错误/174 警告（与基线一致）；`npm run build` 通过。

截图：`E:\ST\.codex_shots\bootstrap_gargantua.png`。

## 12. 微信启动页初始化组件重设计（追加批次，含长条化）

用户反馈：点进「微信数据」后，原 640px 居中大卡（58px logo + 大标题 + 大留白）把黑洞主体挡住。

### 新设计：横贯底部长条 HUD（v3 → v4 透明化）

- 长条横贯微信面板整宽（实测 1280×62，长宽比 ≈20.6:1，距底 24px）。
- v4 透明化：背景不透明度 76% → **26%**，模糊 blur(16px) → **blur(9px)**，边框/阴影同步减淡；
  实测长条区域 65% 为暗色（深空透过可见）、27% 高亮为吸积盘光芒透过长条，不再遮挡黑洞。
- 文字加轻投影（text-shadow 0 1px 3px rgba(0,0,0,.55)），保证在亮盘上仍可读。
- 单行内容：星芒图标 + 标题 + 中文状态（可截断）+ 弹性留白 + 英文状态（等宽）+ 等宽百分比读数；
  底部 2px 发光进度线横贯全宽，≥95% 时流光 indeterminate。
- 阻塞态同款长条：红点 + 标题 + 胶囊条目列表（可换行）+ 右侧「前往微信配置」按钮。
- 保留：进度 ≥95% 的流光 indeterminate 动画、主题令牌（`--app-*`）、`prefers-reduced-motion` 语义（由
  Gargantua 页自行降级）。
- 附带修复：`GargantuaBackdrop` 增加加载淡入（iframe onload 前显示深空兜底渐变，就绪后 0.55s 淡入），
  消除冷启动白闪。

### 验证

- CDP 实测：长条 1280×62 @ (276,848)（1600×1000 视口，面板区自侧栏后起），bottom 留白 24px；
  状态/百分比/英文状态渲染正常。
- 截图像素：黑洞中心区 3255 采样色 / 2669 高光像素（可见未被遮挡）；长条区域深空透过（暗色 65%）。
- `svelte-check` 0 错误/174 警告（与基线一致）。

截图：`E:\ST\.codex_shots\bootstrap_gargantua_v4.png`。

## 16. Rust 单测与 CI（追加批次）

### 诊断（测试二进制 0xC0000139）

- `cargo test --lib` 编译正常，但测试二进制在加载器初始化阶段以
  `0xC0000139 STATUS_ENTRYPOINT_NOT_FOUND` 退出（仅加载 exe/ntdll/kernel32/
  vcruntime140 后即失败）。
- 最终根因：`tauri-plugin-dialog(rfd)` 静态导入 `comctl32!TaskDialogIndirect`，
  该函数只存在于 Common Controls **v6**（WinSxS）。主程序由 tauri-winres 嵌入
  v6 清单，但 `cargo test --lib` 的 harness 没有清单，加载器绑定到 System32
  的 comctl32 v5.82 后找不到入口点 → `0xC0000139`（与 Windows 报错弹窗
  「无法定位程序输入点 TaskDialogIndirect」一致）。
- 排除过程：逐函数核对导入/导出全部存在（排除缺 DLL）；DirectML 0 字节副本、
  VC 运行库版本、ort 特性、增量构建均非主因；`__TAURI_WORKSPACE__` workaround
  无效——tauri 的 build.rs 虽发出 `/MANIFESTINPUT`，但依赖 crate 的
  `rustc-link-arg` 不会作用到宿主项目的测试链接，且其清单路径对 registry
  安装也不存在；`cargo:rustc-link-arg-tests` 也不作用于 `--lib` 的 harness
  （其 target kind 是 lib 而非 test）。

### 修复

- `build.rs`：Windows/MSVC 下对**所有目标**（含 lib 单测 harness）追加
  `/MANIFEST:EMBED /MANIFESTINPUT:windows-app-manifest.tests.xml`
  （Common Controls v6 清单），并用 `cargo:rustc-link-arg-bins=/MANIFEST:NO`
  仅对主程序关闭链接器嵌入，避免与 tauri-winres 的清单重复（CVT1100）。
- 新增 `src-tauri/windows-app-manifest.tests.xml`（comctl32 v6 依赖）。
- 顺带修正 `decode_cdn_aes_key`：32 字符非 hex 串（如 `zzzz…`）不再被当作
  原始 AES-256 密钥接受，避免垃圾 aeskey 进入解密流程。

### 落地

| 项 | 处理 |
|---|---|
| 可选特性 | `onnx-ocr`（默认开启，拉 rapidocr-core/ort）；`--no-default-features` 时测试依赖面最小 |
| CI 工作流 | `.github/workflows/ci.yml`：windows-latest 上 `cargo test --lib --no-default-features` + `cargo build`（默认特性）+ 前端 svelte-check/build |
| 运行脚本 | `scripts/run-rust-tests.ps1`：以最小特性跑测试（已去掉过时的 `--no-run` 回退） |

### 验证

- `cargo test --lib --no-default-features`：**178 passed / 0 failed / 16 ignored**
  （此前测试二进制根本无法启动）；
- `cargo build`（默认特性，含 onnx-ocr）通过；主程序清单仍为单份
  Common Controls v6，应用行为不变。

## 13. 聊天界面图片看不到问题（定位与修复）

### 根因（实测数据）

- 全库 22,614 张图片消息：**10,417 张（46%）只有 `cdnmidimgurl`、本地无 `.dat`**，
  c3o.re CDN 网关对中图 fileid 不响应（`type=orig/thumb/file/image` 全部超时或 Not Found）——这些图物理上无法恢复。
- 原实现对这类图仍发起 CDN 下载，curl 超时 **60 秒/张**；HTTP 媒体接口同样被拖住（实测 20s+ 超时），
  懒加载队列（并发 4）被坏图堵死，好图也迟迟不显示。
- 103 个有图会话全部受影响（最大群 7991 张图里 5120 张失效）。

### 修复

| 项 | 处理 |
|---|---|
| 快速失败 | `cdn_image.rs`：消息 XML 缺 `cdnbigimgurl`（仅中图）时直接跳过 CDN，秒级返回失败占位 |
| 超时收紧 | CDN 下载 curl `--max-time` 60s → **15s**（真原图正常 1–2s 内返回） |
| 自动补显 | `WeChatPanel.svelte`：失败图 12s×4 次有界自动重试，微信本地缓存/网络恢复后自动显示；切会话/销毁时清空定时器 |

### 验证

- IPC 探测 4 张图总耗时 **121s → 8.9s**；好图照常返回（CDN 原图 179KB、本地 PNG）。
- HTTP 媒体接口（UI 实际路径）：好图 **161ms / 30ms**；失效图 **404 于 30–78ms**（修复前 ≥20s 超时）。
- `cargo build`、`npm run build`、`svelte-check`（0 错误/174 警告）全部通过。

说明：失效图是历史数据缺失（本地无缓存、CDN 无中图），无法凭空恢复；现在它们秒级显示失败态，
不再阻塞其它图片，且微信若稍后把文件下载到本地，自动重试会直接补显。

## 14. 「用本地图片补位」：md5 变体补查 + 失效占位气泡（追加批次）

用户建议用本地已解密图片补位。经全量核对（attach `.dat` 全部 md5 变体、解码缓存
`decoded_images`、`message_resource.db`、全盘其它微信数据根）：

- 可达会话内：主 md5 未命中但 `originsourcemd5/hdmd5` 命中的图片 **0 张**；
- 全库 22,614 张图里仅 51 张存在变体命中（且不在当前会话列表可达范围）——
  即失效图在本地**确实没有**任何可用副本，无法用真实图片补位。

### 实现

| 项 | 处理 |
|---|---|
| md5 变体补查 | `image.rs`/`cdn_image.rs`：主 md5 本地未命中时，解析消息 XML 的 `originsourcemd5/hdmd5` 依次补查本地 dat/解码缓存，命中即出图（防御未来 md5 不一致场景） |
| 失效占位气泡 | 前端将文本失败态改为 180×118 主题化占位气泡：虚线边框 + 破损图片 SVG 图标 + 「图片已失效 / 点击重试」，视觉补位、聊天布局完整 |

### 验证

- UI 实测：打开含失效图的会话，**4 个占位气泡立即渲染**（`failText=图片已失效`），0 个卡在加载态。
- `cargo build`、`npm run build`、`svelte-check`（0 错误/174 警告）全部通过。

截图：`E:\ST\.codex_shots\placeholder_bubbles.png`。

## 15. 微信监控初始解密失败修复（追加批次）

用户日志：

```
[automation] 订阅中断（第 N 次）: 微信监控未运行，暂无 router
[monitor] 全量解密结果无效，丢弃临时文件下轮重试
[monitor] 初始解密失败: 解密结果无效（源库可能正在被写入）
```

### 根因

- `do_full_refresh` 直接对流读取正在被微信（SQLCipher + WAL）写入的 `session.db`
  逐页解密，WAL 也是直接 `fs::read`——checkpoint 原地改写主库页 / WAL 追加写时，
  单次读取会拿到撕裂页：解密后 SQLite 头虽在但 `sqlite_master` 损坏，
  `sqlite_healthy` 校验失败 → 整轮解密被丢弃。
- 原实现失败后只能等下一轮 5s 轮询，写入窗口持续时反复失败；
  监控未完成初始解密 → automation 订阅拿不到 router（前两条 WARN 为启动期竞态，
  router 注册后自动重连，属自愈噪音）。

### 修复（monitor.rs）

| 项 | 处理 |
|---|---|
| 一致性快照 | 新增 `stage_stable_copy`：连续复制两次并逐字节比对，不一致说明写入窗口内，短暂重试后拿到稳定副本 |
| 全量刷新 | `do_full_refresh` 改为：暂存主库+WAL 快照 → 解密暂存副本 → WAL patch → 健康校验 → 原子替换；失败本轮内重试 3 次（250ms 间隔） |
| WAL 增量 | `do_wal_refresh` 的 WAL 也先双复制暂存再应用，避免读到写一半的帧 |

### 验证

- `cargo build` 通过；应用重启后监控自动启动，`get_wechat_monitor_status` → `running: true`；
  解密副本 `decrypted/session/session.db` 健康（7 表 / 366 会话行，mtime 为本次启动后）。
- 说明：message 分库（图片/语音按需解密路径）同样直读源库，后续若出现同类问题可复用
  `stage_stable_copy` 加固。

## 17. 语音转写失败提示误导（追加修复）

### 根因

- 语音「转文字」失败时，前端把除「未配置」正则外的所有异常统一显示为
  「（转写失败，点击重试）」，真实原因被吞掉。
- 本机 LLM 仅配置了 DeepSeek 聊天模型，无 SenseVoice/Whisper 等转写模型；
  后端 `resolve_transcription_provider` 返回
  「未找到支持语音转写的提供方。请在 LLM 设置中添加带 SenseVoice/Whisper
  的提供方…」，但前端正则只匹配 `未配置|无可用|未找到.*模型`，未命中
  「未找到…提供方」，于是落入泛化文案——用户反复点重试也永远失败。

### 修复

- `WeChatPanel.svelte` 转写 catch 分支：
  - 正则补充 `未找到.*提供方`，未配置时显示
    「未配置可用的大模型，无法转写；可在 设置 → 大模型 中接入
    SenseVoice/Whisper 后重试」；
  - 其余失败透出真实错误（截断 100 字符）+「点击可重试」，保留重试按钮。

### 验证

- 实测点击语音「转文字」：提示由「（转写失败，点击重试）」变为
  「（未配置可用的大模型，无法转写；可在 设置 → 大模型 中接入
  SenseVoice/Whisper 后重试）」，重试按钮仍可用；
- `svelte-check` 0 错误 / 174 警告（与基线一致）。

## 18. 图片体检界面重设计（仪表台语言）

### 变更

- 弹窗重做为仪表台读数结构（`WeChatPanel.svelte` 新增 `wc-checkup-*`）：
  - 头部：LED 状态灯（有缺失=琥珀、全可用=成功绿）+ 标题 + 扫描时间/会话数 meta；
  - 四块仪表卡：总图片（中性）/ 本地可解（成功绿）/ CDN 可下（青蓝）/ 缺失（琥珀），
    等宽 26px 读数；
  - 可用性占比条 + 图例（本地/CDN/缺失三段，色彩与仪表卡同语义）；
  - 工具行：会话搜索（名称/wxid）、「仅看缺失」筛选、排序（缺失最多/图片最多/名称）；
  - 表格：显示名 + wxid 双行、等宽数字右对齐、缺失率迷你条、有缺失行琥珀底；
  - 页脚：缺失总数 + 主按钮「导出缺失清单 CSV」（`--primary` 填充）+ 重新扫描 + 关闭；
  - 首扫用骨架屏（非居中转圈），无缺失时显示「全部图片可用」教学态，失败态内联报错+重试。
- `app.css` 新增语义令牌 `--app-success / --app-warning / --app-danger`，替代硬编码色。
- 全部取色走 `--wc-*` / `--app-*` / `--brand` / `--primary` 令牌；弹层背景跟随 `--card`。

### 验证

- 实测扫描：总图 44371 / 本地 62 / CDN 20237 / 缺失 24072；199 会话中 104 个有缺失，
  占比条 0.1% / 45.6% / 54.3%；
- 搜索 wxid、仅看缺失（104 行）、排序切换均实时生效；深色主题弹层背景
  rgb(15,38,40)（令牌派生），文本月白，对比正常；
- `svelte-check` 0 错误 / 174 警告；impeccable 检测器已处理（移除 `transition: width`
  布局动画、读数对齐 26px 字阶）。

## 19. 聊天头像实时加载修复（新消息头像不显示）

### 根因（两层）

1. **后端数据滞后**：`get_user_avatar` 直接读静态解密副本
   （`decrypted/head_image/head_image.db`、`decrypted/contact/contact.db`），
   绕过监控的 `MonitorDBCache`。微信新头像/新联系人先写源库 WAL，
   静态副本不会自动更新 → 新联系人的头像查询长期返回 none。
2. **前端失败后不重试**：头像首次取不到时 `avatarCache[u]` 被置为 `''` 占位，
   `enqueueAvatar` 见 `!== undefined` 直接跳过 → 同一会话生命周期内永不补拉，
   即使头像数据稍后到位也一直显示字母兜底。

### 修复

- 后端 `get_user_avatar`：查询前先经监控 `MonitorDBCache::get` 刷新
  `head_image/head_image.db` 与 `contact/contact.db`（mtime 感知，自动
  WAL 增量/全量重建，与消息库同一套已验机制）；监控未运行时回退静态副本。
- 前端 `WeChatPanel.svelte`：`avatarFailedAt` 记录失败时间，空占位在
  20s 冷却后可重新入队；新消息/刷新再次触发 `preloadAvatars` 时自动补拉，
  成功后立即更新气泡头像。

### 验证

- `get_user_avatar` 对群聊与自身 wxid 均返回 `kind: data`（base64 头像）；
- WAL 增量为就地补帧：无新帧时不动副本 mtime（触碰 mtime 实验确认），
  有真实新帧时自动写入，机制与消息分库一致；
- `cargo check` / `svelte-check` 0 错误 / 174 警告。

## 20. 内置离线语音转写（whisper.cpp，替代 API 转写）

### 变更

- **后端**（`src-tauri/src/stt/mod.rs`，默认特性 `local-stt` 启用）：
  - 集成 whisper-rs 0.16（whisper.cpp，MIT 开源），CPU 推理、多线程、
    无时间戳；引擎单例常驻，换模型自动释放旧实例；
  - 输入为 silk_decoder_rs 产出的 WAV（PCM16 24kHz），内部重采样 16kHz；
    支持 99 种语言自动检测，也可固定语言（zh/en/ja/ko/yue/fr/de/es/ru/
    pt/it/ar/th/vi/id 已在设置页列出）；
  - 配置存 `%APPDATA%\st-control\stt_config.json`，模型默认放
    `models/ggml-*.bin`；
  - 三条 IPC：`get_local_stt_status` / `set_local_stt_config` /
    `download_local_stt_model`（带 `stt-download-progress` 进度事件，
    下载源 huggingface + hf-mirror 国内镜像自动回退）；
  - 微信语音转写 `transcribe_message_voice`：本地已启用且模型就绪时
    优先本地识别，失败/空文本自动回退 LLM API，结果照常写入转写缓存；
  - `lib.rs` 对 `stt` 模块与命令注册增加 `#[cfg(feature = "local-stt")]`，
    `--no-default-features --features onnx-ocr` 构建同样通过。
- **前端**（`WeChatConfig.svelte` 新增「本地语音转写（离线）」卡片）：
  - 启用开关、模型文件选择、一键下载 Tiny/Base/Small、下载进度条、
    语言选择、模型状态（未配置/就绪/已加载 + 大小）。

### 编译环境

- `src-tauri/.cargo/config.toml` 设置 `CMAKE`（VS 2026 自带）
  与 `LIBCLANG_PATH`（Python clang native），whisper.cpp C++ 构建所需；
  cmake 不在 PATH 时 cargo 也能找到。
- `tauri.conf.json` 的 `build.features` 显式声明
  `["onnx-ocr", "local-stt"]`：tauri dev 默认传
  `--no-default-features`，若不声明，本地 STT 与 OCR 都不会编译进
  开发/打包产物，聊天「转文字」会误报「未配置可用的大模型」。
- `transcribe_message_voice` 本地分支先 `ensure_model_loaded` 再识别：
  应用重启后模型未常驻时，首次转写自动加载（约 1–2s），避免回退 LLM。

### 验证

- 默认特性与 `--no-default-features --features onnx-ocr` 两种构建均通过；
- 实测 `download_local_stt_model`：hf-mirror 下载 tiny 78MB / base 148MB，
  下载后自动加载成功（`model_loaded: true`）；
- 实测聊天语音「转文字」走本地：返回
  「哈 你是不是有點忙」（清晰中文）、「日常上課的一個形象我批一個毛彈
  但是我現在手冷的發脂」等；转写结果写入 `decoded_images/voices/<svr>.txt`
  缓存，全程未调用 LLM API；
- 在真实 `npm run tauri dev` 环境复测：清掉转写缓存后首次转写
  1542ms（含模型加载），`model_loaded` 由 false 变 true，结果
  「哈 你是不是有點忙」；再次转写命中缓存即时返回；
- whisper zh 默认输出繁体字形，个别口音/噪音音频识别偏差属引擎固有，
  可在设置页固定语言或换 Small 模型提升准确率；
- `svelte-check` 0 错误 / 174 警告。

## 21. 单聊实时消息方向与头像修复（最新消息无头像 / “我”位置错）

### 根因

微信 4.x 单聊的 SessionTable `last_msg_sender` 常为空字符串。监控在消息
分库查询因水位线已覆盖 / WAL 时序返回空时，会走 SessionTable 摘要 fallback：

- `sender_username` 置空 → 前端头像键为 `''`，拿不到真实头像，
  退化成聊天名的首字母占位（如「ai-微信助理」→ 字母 A）；
- `is_send` 按空 sender 与 self_username 比较恒为 false → 本人刚发的
  消息被放到左侧对方一侧（“我”的位置不对）。

### 修复

- 后端 `monitor.rs` 新增 `query_latest_message`：按最新 `sort_seq/local_id`
  直查消息分库（不套水位线过滤），fallback 摘要优先用真实发送者构造
  `sender_username / is_send / local_id / sort_seq`；查不到才退回旧摘要。
- 前端 `WeChatPanel.svelte`：
  - `toRealtimeMsg` 方向判断优先「sender_username == selfUsername」，
    再信任后端 `is_send`，杜绝单聊错判；
  - 实时消息按 `local_id` 原位替换旧摘要气泡（同一条消息只保留一个气泡，
    方向以 DB 行修正后的推送为准）；
  - 单聊实时消息同样预载发送者头像。

### 验证

- 复现：单聊「ai-微信助理」最新一条本人消息「666」此前渲染为
  左侧 + 字母 A 占位；修复后渲染为右侧 + 本人头像图片。
- 后端 `get_conversation_messages` 返回 `is_self: true`、
  `sender_username: wxid_umyqa86if3lm22`，与 UI 渲染一致；
- `cargo build`（tauri dev watcher 自动重编译）与
  `svelte-check` 0 错误 / 174 警告。

## 22. 微信配置迁移：设置弹窗 → 微信数据「设置」

### 变更

- 「设置」弹窗移除「微信配置」页签（`SettingsModal.svelte`：
  删除类型成员、NAV 条目与页面容器）。
- `WeChatPanel.svelte` 的「设置」页顶部新增「微信配置」分区，
  内嵌完整 `WeChatConfig`（检测本机微信数据 / 路径与数据库 /
  密钥配置 / HTTP API 服务 / 本地语音转写），下方保留原「通用数据」；
  新增 `.wc-settings-section` 与分隔线样式，容器无横向溢出、可滚动。
- 微信启动页「去配置」入口改为直接进入 微信数据 面板并打开「设置」页
  （`App.svelte` 通过 `openConfigTick` bindable prop + `$effect` 联动）。

### 验证

- 设置弹窗导航只剩 5 项（常规/个性化/服务器/Agent 日志/数据库）；
- 微信数据 → 设置：顶部渲染「微信数据配置」标题与全部配置卡片，
  下方通用数据 5 分类 236 条记录；来回切换标签重新挂载无报错；
- 布局：设置容器宽 1149px，scrollWidth = clientWidth（无横向溢出），
  内容纵向滚动正常；`svelte-check` 0 错误 / 174 警告。

## 23. 社交关系图谱增强：群友圈子 / 群聊网络内容升级

### 变更

- **后端**（`insights.rs`）：
  - 群节点新增 `shared_members`：该群命中的已选联系人明细
    （姓名 / 是否好友 / 消息量，按消息量取前 8）；
  - 响应新增 `group_names`（群 code → 群名，会话库缺失时从通讯录补），
    供前端展示「共同群」列表，不再只显示裸 code。
- **前端**（`RelationshipGraph.svelte` + `graphModel.ts`）：
  - 数据芯片按模式丰富：群友圈子显示 联系人 / 好友·群友 / 连线 /
    圈子 / 共同群阈值 / 扫描群·消息总量；群聊网络显示 群数 / 群成员 /
    连线 / 圈子 / 共同成员阈值；
  - 新增常驻「洞察」侧栏（可折叠）：
    - 群友圈子：亲密度榜（按消息量）、共同群榜、圈子概览；
    - 群聊网络：活跃群榜（按消息量）、命中榜（按共同成员）、
      规模榜（按群成员）、圈子概览；
    - 榜单行可点击选中对应节点，前三名高亮主题色。
  - 节点详情增强：联系人显示 关系 / 共同群数 / 消息量+亲密度排名 /
    活跃天数 / 最近联系 / 所属圈子成员数 / 共同群名称列表；
    群聊显示 群成员 / 命中成员 / 消息量 / 活跃天数 / 最近活跃 /
    所属圈子 / 共同成员名单（点击可跳到「群友圈子」查看该联系人）；
  - 图谱左上角新增图例（颜色 = 圈子、连线 = 共同群/成员数、
    节点大小 = 消息量/命中数）。

### 验证

- 群友圈子：61 位联系人（49 好友 · 12 群友）、320 连线、8 个圈子、
  扫描群 70 · 消息 24.2w；亲密度榜首位「蒙婵丽-p 姐 911」，
  详情含活跃天数 122 天、最近联系昨天、亲密度 #1、共同群列表；
- 群聊网络：69 个群、2327 群成员、696 连线、14 个圈子；
  群详情含群成员 266 人、命中成员、活跃天数 262 天、
  最近活跃 27 分钟前、共同成员名单；
- 共同群名称回退正常（通讯录补名后多数显示可读群名）；
- 洞察侧栏可折叠/展开，无横向溢出，svelte-check 0 错误 / 174 警告；
  impeccable 检测器 37 项 advisory（色彩/字号/圆角与既有微信模块
  组件惯例一致，无阻断项）。

### 联系人数量口径修正

此前芯片只显示「过滤后的可见节点数」（如 61 位），与真实通讯录数量
不符。现已对齐通讯录面板「全部」口径：

- 后端 summary 新增 `contact_book_total / contact_book_friends /
  contact_book_members / contact_book_official`（与通讯录面板一致，
  统计 friend/member/enterprise/group/official/service 六个可见分类）；
- 群友圈子芯片改为：通讯录 4540 人 · 好友 375 · 群成员 3770 ·
  图谱展示 61 位；群聊网络芯片显示群总数与图谱展示数；
- 实测与通讯录面板总数完全一致（4540），图谱展示数与后端 selected
  数量对得上。

## 24. 关系图谱缓存秒开 + 后台刷新

### 变更

- **后端**（`insights.rs`）：`build_relationship_graph` 成功构建后将结果
  落盘到 `%APPDATA%\st-control\relationship_graph.json`；
  新增 IPC `get_relationship_graph_cached`（无缓存返回 None）。
- **前端**（`RelationshipGraph.svelte`）：
  - 进入图谱先读模块缓存 → 磁盘缓存 → 都没有才全量加载；
  - 有缓存时立即渲染（无全屏「正在聚合消息统计…」加载态），
    随后在后台刷新一次（本会话只刷一次，避免重复全量扫描）；
  - 后台刷新期间只在芯片行显示轻量徽标
    「后台更新图谱 · 已组装 N 个节点（X%）」，完成后自动消失；
  - 手动「刷新」仍保持强制重建。

### 验证

- 首次（无缓存）仍全量加载并写入缓存（165KB）；
- 刷新页面模拟重启后再进入：图谱立即渲染（无全屏加载态），
  徽标显示「后台更新图谱 · 已组装 132 个节点（100%）」，
  后台完成后徽标消失、数据更新；
- `svelte-check` 0 错误 / 174 警告。

## 25. 群友圈子好友全量展示（真实好友 279 位）

### 根因（两层）

1. **好友分类错误**：`category_of` 把 local_type 0/1/2/5/6/7 兜底都算成
   friend，导致好友数显示 375。实测微信 4.x 真实好友 = local_type==1
   （排除公众号 gh_/群/企业微信 openim/系统账号/当前自己）= 280−1 = **279**，
   与用户在微信客户端看到的完全一致。
2. **节点选取只认消息记录**：后端节点来自会话消息统计，279 位好友里只有
   88 位有消息记录，其余好友从未进入图谱；前端又只保留「有共同群」的联系人，
   进一步缩到 50 位左右。

### 修复

- `contacts.rs category_of`：local_type=1 → friend；0/2 → member；
  @kefu.openim → service；其余不再兜底成 friend。
- `insights.rs`：
  - 全量好友（即使无消息记录）并入节点，好友数排除当前账号本体 = 279；
  - 节点排除只针对当前账号本体（本机其他账号若被添加为好友应正常展示）；
  - 我→节点边对无消息好友也生成（weight=1）。
- `graphModel.ts`：
  - 好友全量展示（无共同群也保留），「群友上限」滑杆只控制非好友数量；
  - 无共同群的好友保留「我」的边（weight=1）避免孤立漂移；
  - 孤立节点 community=-1（未分组，中性灰），不再把几百个孤立节点
    算成几百个「圈子」。
- 详情面板对未分组节点显示「未分组」。

### 验证

- 后端：好友 279（含 191 位零消息好友）、联系人节点 322；
- 图谱 UI：通讯录 4540 人 · 好友 279 · 群成员 3858 · 图谱展示 291 位
  （279 好友 + 12 位有共同群的群友）· 1418 连线 · 10 个圈子；
- 通讯录面板「好友」分类同步修正为 280（含本机账号一条，图谱口径
  已排除自身 → 279）；`svelte-check` 0 错误 / 174 警告。

### 口径统一（通讯录面板也排除本机账号）

- `contacts.rs get_contacts_uncached`：通讯录数据源直接排除本机当前账号
  （微信客户端通讯录也不显示自己），所有消费方口径一致；
- 实测：通讯录面板 全部 4539 · 好友 279；图谱 通讯录 4539 ·
  好友 279 · 图谱展示 291 位，两边完全一致。

### 群友上限滑杆对齐好友数

- 「群友上限（好友全量）」滑杆最高值改为动态等于好友数量
  （当前 279），步长 1，可精确拖到 279；默认 100/279；
- 数据加载前不会把滑杆值误钳小（仅在拿到真实好友数据后钳制）。

### 「我」居中 + 群友上限真正控制节点数

- `GraphCanvas.svelte`：「我」节点（self）固定在世界原点，`forceCenter` 改为
  以原点为圆心聚拢，整个力导向布局围绕「我」展开；self 的连线/命中判定/绘制
  均按中心坐标处理，平移缩放时「我」始终在画面中心；
- `graphModel.ts`：群友上限滑杆由「好友全量 + 群友前 N」改为对「好友+群友」
  整体按「好友优先、共同群多者优先」截取前 N 个——滑杆现在真正控制图谱展示
  的节点总数（默认 100，拖到 279 即好友全量）。

### 微信关系图谱全屏

- `RelationshipGraph.svelte` 图谱头部（刷新旁）新增「全屏」按钮：进入后
  `.rg-root` 固定覆盖整个窗口（z-index 9999），右侧洞察栏自动隐藏让画布
  最大化，标题与底部控制栏保留（可继续调上限/阈值/过滤）；
- 再次点击「退出全屏」或按 Esc 退出；画布 ResizeObserver 自动按窗口尺寸
  重排，力导向布局重新居中。

### 启动页黑洞重设计（poster 构图）

- 用户反馈初始化界面黑洞「没设计好」：原为电影镜头循环，黑洞位置/大小持续
  变化、时常偏小或偏移；
- `main.js` 新增 URL 氛围参数：`bright`（吸积盘亮度）、`star`（星空亮度）、
  `sky`（天光底色），复用 `urlOverrideKeys` 机制、仅本次运行生效；
- `GargantuaBackdrop.svelte` 新增 `cam / motion / bright / star / sky` props，
  空状态（无参调用）保持电影镜头不受影响；
- `WeChatBootstrap.svelte` 改用 `cam=poster&nocine=1`：黑洞居中、吸积盘 38°
  经典构图锁定，steps 120→170 提升清晰度，吸积盘 1.55× / 星空 1.45× / 天光
  0.055 提升画面明度，初始化瞬间画面稳定耐看。

### 微信关系图谱「外观与力度」参数面板

- 图谱底部控制条新增「外观与力度」按钮，弹出毛玻璃浮层：
  - 外观：箭头（开关）、文本透明度、节点大小、连线粗细、播放动画（开关）；
  - 力度：图谱向心力、节点间排斥力、相连节点吸引力、连线长度；
- 力度参数采用**相对倍率**（默认 1 = 保持原布局），对所有边（含「我」的枢纽边）
  统一缩放，默认观感不变；滑杆 150ms 防抖后重建力导向，拖动不卡顿；
- 「播放动画」关闭后仿真冻结（拖拽/平移仍可用），开启后自动恢复。

### 微信关系图谱全屏布局重设计

- 全屏时画布最大化：隐藏副标题与统计芯片行，顶部工具行改为紧凑单行，
  并内嵌全屏统计胶囊（好友/展示数 · 连线 · 圈子）；
- 底部控制区由三段式卡片压缩为单行紧凑条（数据 / 外观 / 力度横排，
  滑块与开关缩小），退出全屏自动恢复常规三段式布局；
- 洞察栏全屏隐藏；画布 ResizeObserver 自动按窗口尺寸重排并保持「我」居中。

## 25. 全局 CPU 优化（图谱前端 + 后端异步运行时）

### 图谱前端（`GraphCanvas.svelte` / `RelationshipGraph.svelte`）

- 仿真绘制限流到约 30fps（`lastTickDraw >= 33ms` 才 scheduleDraw）；
- 碰撞检测迭代 2 → 1；画布 DPR 上限 1.5（高 DPI 下每帧像素减 40%+）；
- 面板不可见（IntersectionObserver）或窗口最小化（visibilitychange）时
  `pauseSimulation()`：停 d3-force 仿真 + 取消 rAF，恢复时 `alpha(0.3).restart()`；
- 滑杆拖动防抖 150ms 后才重建图模型/仿真；后台刷新跳过逐 chunk 增量
  merge（原实现每个 chunk 都重建图模型，是图谱 CPU 大头）。

### 后端异步运行时（`src-tauri/Cargo.toml`）

- 根因：tokio 1.53.x 在 Tauri async runtime 下，`watch.changed()` /
  `broadcast.recv()` 等 channel await 病态空转，实测空闲 CPU 3.2 核；
- 修复：tokio 锁 `=1.48.0`（Cargo.toml 已加注释）。实测同一运行形态
  下空闲 CPU 从约 3200 ms/s 降至 0 ms/s（Get-Counter / 线程采样双验证）；
- 注意：监控自动启动时会做一次全量解密 session.db，启动后 1-2 分钟
  CPU 尖峰属预期（一次性），稳态后为 0。

### 微信监控解密重试风暴（`db_cache.rs`）

- 根因：`message/message_0.db` 等大分库在微信写入期间解密健康校验失败时，
  基线不推进，1 秒轮询每轮都重复全量解密（数百 MB），实测持续 9.3 核；
- 修复：新增 `DECRYPT_FAIL_COOLDOWN = 30s`，全量解密失败后记录时间戳，
  冷却期内跳过该库的全量解密（返回已有副本，仅记 warn），成功即清除标记；
- 实测：同样会触发 message_0.db 解密失败的场景下，CPU 从 9.3 核降至
  ~24 ms/s（约 0.02 核），监控仍正常运行、health 正常。

### 验证

- `svelte-check` 0 错误 / 176 警告（均为既有 a11y / 未用 CSS 提示）；
- 空闲稳态：st-control 0 ms/s，WebView 各进程合计 ≈0；
- 系统整体 CPU 12-17%（正常波动）；
- 会话/监控接口正常（`/api/v1/health` ok，monitor running，db ready）。
