# WeChatPanel 模块化重构蓝图

> 分阶段重构流程：锁定基准 → 边界识别 → 蓝图预设计 → 小步增量 → 回归校验。

## 基准（2026-08-13 锁定）

- `svelte-check`: 0 errors / 176 warnings
- `.codex_tests/run-store-test.mjs`: 13 项断言通过
- `.codex_tests/smoke-format-utils.mjs`: 全部断言通过
- 行为不变量：图片 URL 直链优先 → IPC base64 回退 → 失败标记 + 有界自动重试；
  缓存 LRU 上限（120 消息图 / 400 朋友圈图）；并发上限（4 / 4）。

## 切片 A-2：媒体图片子系统下沉（已完成）

### 边界

`WeChatPanel.svelte` 中第 218–360 行（消息图片队列）与 376–523 行
（朋友圈图片/视频）自成一体的加载子系统：缓存、并发受限队列、失败重试、LRU 淘汰。

### 目标模块

1. `services/mediaApi.svelte.ts`
   - `$state` 对象 `mediaApi`：`mediaBase`、`videoBase`、`token`
   - `loadMediaConfig()`：读取 HTTP API 设置（行为与原 `loadApiMediaConfig` 一致）
   - `messageImageUrl(username, localId)`：统一消息图片 URL 直链构造
   - `apiAssetUrl(apiPath)` / `mediaRoot()`：表情/文件资源直链
2. `services/imageQueue.svelte.ts`
   - `$state` 对象 `imageQueueState`：`cache`（key=`username:local_id`）、`blocked`
   - API：`enqueueImage`、`onImageLoadError`、`retryImage`、`clearAutoRetries`
   - 内部：并发受限队列 + 有界自动重试 + LRU 淘汰（保留原语义）
3. 说明：svelte-check 4.7.3 对 `.svelte.ts` 的 rune 重赋值检查有局限，
   故导出状态采用"可变对象属性"模式（与既有 `llmStore` 一致）
4. 后续切片（另立）：朋友圈视频播放器状态下沉

### 回归门禁（每切片完成后）

- `npx svelte-check --output human`：0 errors，警告数不超基准
- `node .codex_tests/smoke-format-utils.mjs`
- `node .codex_tests/run-store-test.mjs`
- `.codex_tests/smoke-image-queue.mjs`：锁定 imageQueue/mediaApi 可观测输出（12 断言）
- `.codex_tests/smoke-moment-media.mjs`：锁定 momentMedia 可观测输出（8 断言）

### A-2 结果（2026-08-13）

- `WeChatPanel.svelte` 减少约 155 行（消息图片队列 + 媒体 API 配置下沉）
- 新增 `mediaApi.svelte.ts`（25 行）、`imageQueue.svelte.ts`（120 行）、
  `smoke-image-queue.mjs`（12 项断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三个冒烟/单元测试全部通过

## 切片 A-3：朋友圈图片懒加载下沉（已完成）

- 新增 `services/momentMedia.svelte.ts`：`imgCache`（$state）、`momentImgKey`、
  `enqueueMomentImage`、`momentImgSrc`、`loadMomentOriginal`（原图异步补拉）、
  内部并发受限队列 + LRU（上限 400，保留原语义）
- `WeChatPanel.svelte` 删除本地朋友圈图片队列（约 60 行），模板改用
  `momentMedia.imgCache`；`openMomentViewer` 改调 `loadMomentOriginal`
- 新增 `smoke-moment-media.mjs`（8 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  四个冒烟/单元测试全部通过

## 切片 A-4：朋友圈视频播放器下沉（已完成）

- 新增 `services/momentVideo.svelte.ts`：`momentVideo`（$state：
  `open`/`src`/`title`/`error`）、`playMomentVideo`、`closeMomentVideo`、
  `handleVideoError`（模板 onerror 内联逻辑下沉）
- `WeChatPanel.svelte` 删除本地视频状态与处理函数；模板改用 `momentVideo.*`；
  移除 `getMomentVideo` 导入与 `apiVideoBase` 派生
- 新增 `smoke-moment-video.mjs`（10 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  五个冒烟/单元测试全部通过

## 小结（A-2 ~ A-4）

WeChatPanel 媒体子系统（消息图片 / 朋友圈图片 / 朋友圈视频 / 媒体 API 配置）
已全部下沉为独立服务，每个服务均有冒烟测试锁定可观测输出。
累计新增 4 个服务 + 3 个冒烟测试；组件行数约 6200 行。

## 切片 D-1：DbManager 工具函数下沉（已完成）

- 新增 `src/lib/db/dbUtils.ts`：`csvEscape`、`utf8ToBase64`、`isBlobPreview`、
  `blobDataUrl`、`blobExt`、`fmtBytes`、`measureTextWidth`（纯函数，不依赖组件状态）
- `DbManager.svelte` 删除本地重复实现；两处 CSV 导出（当前页 / 选中行）的
  内联转义统一改调 `csvEscape`，消除重复
- 新增 `smoke-db-utils.mjs`（18 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；六个冒烟/单元测试全部通过

### 候选（未做，需逐组件保持输出）

~~`fmtBytes` 在 KbDashboard / KbDocs / DataDashboard 各有局部实现~~ → 已收敛（见 D-3）

## 切片 D-2：Wiki 图谱 / 对话上下文纯函数下沉（已完成）

- 新增 `src/lib/kb/graphLayout.ts`：`radialTreeLayout`（径向树布局，
  力参数由 `RadialLayoutParams` 传入，脱离组件状态）、`matchGlob`（* 通配匹配）
- 新增 `src/lib/llm/chatContext.ts`：`trimContext`（滑动窗口裁剪，
  条数/字符上限/最小保留可参数化）
- `WikiPanel.svelte` 删除本地 `radialTreeLayout`（约 120 行）与 `matchGlob`；
  `GlobalChatTab.svelte` 删除本地 `trimContext`
- 新增 `smoke-kb-graph-layout.mjs`（8 断言）、`smoke-chat-context.mjs`（6 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；八个冒烟/单元测试全部通过

## 状态总览

- 已完成切片：A-2（消息图片队列）、A-3（朋友圈图片）、A-4（朋友圈视频）、
  D-1（DbManager 工具）、D-2（Wiki 图谱 / 对话上下文）、D-3（fmtBytes 收敛）、
  D-4（Wiki 图谱着色逻辑）、D-5（对话附件处理）、D-6（列宽持久化格式）、
  D-7（WeChatPanel 杂项纯函数）、D-8（消息虚拟滚动纯计算）、
  D-9（关系图谱展示纯函数）、D-10（WeChatConfig 安全/时间纯函数）、
  D-11（AI 角色提示词统一）、D-12（KbChat 对话展示纯函数）、
  D-13（AutomationPanel 展示纯函数）、D-14（KbDocs 文件展示/解析纯函数）、
  D-15（AI 角色归一化/默认值）、D-16（fmtTime/fmtTs 时间格式化收敛）、
  D-17（DataDashboard 指标格式化）、D-18（escapeHtml 重复消除）、
  D-19（GlobalSearch 文本处理）、D-20（API 调试地址构造）、
  D-21（HookManager 会话类型识别）、D-22（OcrPanel 展示纯函数）、
  D-23（KbDashboard 首字母/趋势展示）、D-24（PreferencesPanel 颜色工具）、
  D-25（MessageBody markdown 渲染管线）、D-26（cssColorToHex 重复消除）、
  D-27（UsageCostTab 成本格式化）、D-28（ChartView 几何纯函数）、
  D-29（KbDashboard 日期格式化 / delay 工具）、D-30（AgentPanel 表单工厂 /
  ProviderConfigTab 数值解析）、D-31（AnnualSummary 展示纯函数）、
  D-32（DataDashboard SVG 路径构建）、D-33（GeneralRecords 记录展示）、
  D-34（DailySummary 汇总展示）、D-35（BackupManager/KnowledgeBase 格式化收敛）、
  T-1（WeChatPanel 消息/会话类型提示强化）、T-2（WeChatPanel sessionMap /
  DbManager 表格数据结构类型化）、D-36（GraphView 时间轴纯函数）、
  D-37（safeParseInt 重复实现消除）、T-3（媒体 IPC 返回类型）、
  T-4（实时消息事件载荷类型化）、T-5（消息/会话操作函数类型化）、
  T-6（查看器/文件/小程序操作函数类型化）、T-7（微信配置返回类型）、
  T-8（朋友圈/表情状态类型化）、T-9（全局搜索状态类型化）、
  T-10（图片体检数据结构类型化）、T-11（DailySummary 数据结构类型化）、
  T-12（WeChatConfig 检测结果类型化）、T-13（密钥信息返回类型）、
  T-14（微信消息搜索返回类型）、T-15（通讯录分页状态类型化）、
  T-16（收藏列表返回类型修正）、T-17（汇总/密钥 IPC 返回类型）、
  T-18（GraphView 图谱数据状态类型化）、T-19（DbManager 表格/事件回调类型化）、
  T-20（AutomationPanel 实时消息类型化）、T-21（KbDocs 文档操作结果类型）、
  T-22（记录列表 IPC 返回类型）、T-23（CDN 状态返回类型）、
  T-24（WeChatPanel 会话/消息回调类型化）、
  T-25（WeChatPanel 收藏/批量选择回调类型化）、
  T-26（RelationshipGraph 图谱数据/增量块类型）、
  T-27（DailySummary 操作函数/IPC 类型化）、
  T-28（图边端点类型修正）、T-29（文件/语音 IPC 返回类型）、
  T-30（AnnualSummary 数据类型化）、T-31（DailySummary IPC 结果类型化）、
  T-32（归档导出/备份导入 IPC 结果类型化）、T-33（DbManager 表结构/单元格/拖拽类型化）、
  T-34（语音转写/收藏媒体/消息日历/通讯录 IPC 类型化）、
  T-35（朋友圈加载/增量刷新类型化）、T-36（静态表情/公众号状态类型化）、
  T-37（文件管理状态/列表类型化）、T-38（消息编辑/原始字段编辑类型化）、
  T-39（设置分类状态/导出类型化）、T-40（全局消息搜索类型化）、
  T-41（WeChatPanel 剩余回调/分类赋值清理）、T-42（配置/账户/收藏详情/联系人 IPC 类型化）、
  T-43（备份/隐私体检/图片体检/关系图谱 IPC 类型化）、
  T-44（AI 问答/记录导出/每日总结/历史快照 IPC 类型化）、
  T-45（密钥/解密/STT/OCR/打开路径 IPC 清零）、
  T-46（图谱力导向 d3-force 类型化）、T-47（群监控台类型化）、
  T-48（图表规范类型化）、T-49（朋友圈媒体服务/记录状态映射类型化）、
  T-50（全局搜索通讯录/图标类型化）、T-51（KB 文档/统计 IPC 类型化）、
  T-52（自动化/引导/图谱缓存/工具类型小集群清理）、
  T-53（收尾零散 any 清理）、T-54（语音电平 RMS 计算收敛）、
  R-1（Rust 全量格式规范化）、R-2（Rust 编译警告清理）、
  T-55（语音录音状态机下沉）、T-56（VAD 状态机纯函数化）、
  T-57（TTS 音频 MIME 映射收敛）、T-58（TTS 播放器状态机下沉）、
  T-59（语音合成候选顺序纯函数化）、T-60（TTS 单句合成兜底链下沉）、
  T-61（流式语音播报编排下沉）、T-62（文档同步与 DbFileEntry 类型去重）、
  T-63（KB 组件 fmtTime 重复实现收敛）、T-64（关系图谱海报时间戳收敛）、
  T-65（播报队列/预取数据结构纯化）、T-66（App 面板包裹层收敛）、
  T-67（实时消息映射纯函数下沉）、T-68（会话排序/实时重排纯函数下沉）、
  R-蓝图-1（kb/handlers 拆分设计，规划待执行）、
  R-3（kb/handlers 首个子模块拆分：analytics_settings）、
  R-4（kb/handlers 拆分：analytics 埋点/推荐域）、
  R-5（kb/handlers 拆分：kb_housekeeping 迁移）、
  R-6（kb/handlers 拆分：analytics 统计核心迁移）、
  R-7（kb/handlers 拆分：analytics 域收口）、
  R-8（kb/handlers 拆分：jobs 域）、R-9（kb/handlers 拆分：qa 域）、
  R-10（kb/handlers 拆分：wiki 查询域）
- 冒烟/单元测试 38 个，覆盖新增服务与纯函数
- 剩余候选：继续扫描大组件（WeChatPanel 消息虚拟滚动、
  GlobalChatTab 语音/录音、App.svelte 布局）等

## 切片 D-3：fmtBytes 重复实现收敛（已完成）

- 新增 `src/lib/format.ts`：`formatBytes`（参数化 null 占位、GB 精度、
  单位序列；gbPrecision=2 时以 GB 封顶，保持 KbDashboard 原语义）
- `dbUtils.fmtBytes` 委托共享实现；KbDocs / KbDashboard / DataDashboard
  局部实现统一为共享函数，输出逐值保持（KbDocs null→'-' 且无 GB 分支、
  KbDashboard GB 两位小数、DataDashboard 含 PB 单位）
- 新增 `smoke-format-bytes.mjs`（12 断言，锁定三种差异语义）；
  `smoke-db-utils.mjs` 适配共享依赖
- 回归：svelte-check 0 errors / 176 warnings；构建通过；九个冒烟/单元测试全部通过

## 切片 D-4：Wiki 图谱着色逻辑下沉（已完成）

- 新增 `src/lib/kb/graphStyle.ts`：`nodeTypeName`、`nodeColor`（颜色组参数化）、
  `nodeMatches`、`edgeColor`、`colorSlug` 及常量 `EDGE_COLORS`/`NODE_TYPE_COLORS`/`ENTITY_DIRS`
- `WikiPanel.svelte` 删除本地着色/分类实现（约 30 行），模板改调共享函数
  （`nodeColor` 显式传 `graphParams.colorGroups`）
- 新增 `smoke-kb-graph-style.mjs`（16 断言，锁定类型归类/颜色优先级/状态着色）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；十个冒烟/单元测试全部通过

## 切片 D-5：对话附件处理下沉（已完成）

- 新增 `src/lib/llm/attachments.ts`：`Attachment` 类型、`fileToAttachment`
  （图片/文本/普通文件三类路径）、`readAsDataURL`/`readAsText`、`TEXT_FILE_EXT_RE`、
  `MAX_IMAGE_BYTES`（图片上限 8MB）
- `fileToAttachment` 的 ID 生成器与图片上限参数化（调用方持有 attSeq，
  与原组件 `att-${++attSeq}` 语义一致）；持久化走 `llmApi.saveUploadedFile`
- `GlobalChatTab.svelte` 删除本地附件处理（约 70 行），`handleFiles` 改调共享函数
- 新增 `smoke-attachments.mjs`（10 断言，mock FileReader/llmApi，
  锁定三类附件输出与 ID 序列）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；十一个冒烟/单元测试全部通过

## 切片 D-6：列宽持久化格式下沉（已完成）

- 新增 `src/lib/db/colWidths.ts`：`dbWidthKeyFromPath`（数据源 key 派生）、
  `colWidthKey`（键拼接）、`parseColWidths`（配置解析 + 非法项过滤）
- `DbManager.svelte` 删除本地解析/拼接实现；模板中 7 处重复的
  `${dbWidthKey()}:${dbCurTable}:${col}` 拼接统一为 `fullWidthKey` 辅助函数
- 新增 `smoke-col-widths.mjs`（11 断言，锁定 key 派生/解析过滤语义）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；十二个冒烟/单元测试全部通过

## 切片 D-7：WeChatPanel 杂项纯函数下沉（已完成）

- 新增 `src/lib/wechat/utils/misc.ts`：`extTone`（文件类型分类）、
  `miniAppPageUrl`（小程序 URL 解码）、`checkupPct`/`checkupRatePct`
  （缺失图占比，total 参数化）、`isKefuSession`/`isMiniAppKefuSession`
- `WeChatPanel.svelte` 删除 6 个本地函数；`checkupPct`/`checkupRatePct`
  以组件内 wrapper 保持模板签名不变（total 取自统计快照）
- 新增 `smoke-wechat-misc.mjs`（21 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；十三个冒烟/单元测试全部通过

## 切片 D-8：消息虚拟滚动纯计算下沉（已完成）

- 新增 `src/lib/wechat/utils/virtualList.ts`：`estimateMsgHeight`（消息高度估算）、
  `computePrefixSums`（前缀和）、`upperBoundPrefix`（二分定位）、
  `estimateVisibleCount`（可见条数）及高度常量（LINE_H/CHARS_PER_LINE/IMG_H 等）
- `WeChatPanel.svelte` 删除本地估算/二分实现（约 50 行），保留状态与校准逻辑
- 新增 `smoke-virtual-list.mjs`（18 断言，锁定各消息类型高度/边界语义）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；十四个冒烟/单元测试全部通过

## 切片 D-9：关系图谱展示纯函数下沉（已完成）

- 新增 `src/lib/wechat/utils/display.ts`：`relTime`（相对时间）、
  `rankOf`（榜单排名，泛型化不依赖 GNode）、`fmtCount`（数量缩写）
- `RelationshipGraph.svelte` 删除 3 个本地函数与重复的 `utf8ToBase64`
  （统一复用 dbUtils 共享实现）
- 新增 `smoke-wechat-display.mjs`（17 断言，固定时钟锁定相对时间边界）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；十五个冒烟/单元测试全部通过

## 切片 D-10：WeChatConfig 安全/时间纯函数下沉（已完成）

- 新增 `src/lib/wechat/utils/security.ts`：`generateApiToken`（64 位 hex 令牌）、
  `fmtLastActive`（Unix 秒 → YYYY-MM-DD）
- `WeChatConfig.svelte` 删除本地实现，改调共享函数
- 新增 `smoke-wechat-security.mjs`（6 断言，mock crypto 验证令牌格式与日期边界）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；十六个冒烟/单元测试全部通过

## 切片 D-11：AI 角色提示词统一（已完成）

- 新增 `src/lib/llm/roleUtils.ts`：`composeSystemPrompt`（语义与
  AiRolesPanel/GlobalChatTab 原实现逐项等价）
- 两个组件删除本地重复实现（`composeSystemPrompt` / `composeRoleSystemPrompt`）
- 新增 `smoke-role-utils.mjs`（7 断言）

## 切片 D-12：KbChat 对话展示纯函数下沉（已完成）

- 新增 `src/lib/kb/chatUtils.ts`：`highlightSegments`（命中高亮分段）、
  `parseCitations`（引用 JSON 解析）
- `KbChat.svelte` 删除本地实现与不再使用的 `HighlightSegment` 类型导入
- 新增 `smoke-kb-chat-utils.mjs`（11 断言）
- 回归（D-11/D-12）：svelte-check 0 errors / 176 warnings；构建通过；
  十八个冒烟/单元测试全部通过

## 切片 D-13：AutomationPanel 展示纯函数下沉（已完成）

- 新增 `src/lib/automation/display.ts`：`classifyMessageType`（media_type/msg_type
  分类，输入类型化 `MessageLike`）、`kindColor`/`kindLabel`、`statusBadge`、
  `mediaLabel`、`STATUS_META`、`MessageKind` 类型
- `AutomationPanel.svelte` 删除本地实现与 `STATUS_META` 常量；
  组件保留 `PushType`（含 'all' 过滤项，UI 概念）与包装函数
- 新增 `smoke-automation-display.mjs`（17 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  十九个冒烟/单元测试全部通过

## 切片 D-14：KbDocs 文件展示/解析纯函数下沉（已完成）

- 新增 `src/lib/kb/fileUtils.ts`：`fileIco`、`previewMime`、`parseTags`、
  `flattenDirs`、`STATUS_LABEL`、`SOURCE_LABEL`
- `KbDocs.svelte` 删除本地实现与常量（约 40 行），模板签名零改动
- 新增 `smoke-kb-file-utils.mjs`（18 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  二十个冒烟/单元测试全部通过

## 切片 D-17：DataDashboard 指标格式化下沉（已完成）

- 新增 `src/lib/system/format.ts`：`pushHist`（含 HIST 窗口）、`fmtRate`、
  `fmtLink`、`fmtUptime`、`colorFor`、`fmtPct`
- `DataDashboard.svelte` 删除 6 个本地函数（约 55 行）
- 新增 `smoke-system-format.mjs`（17 断言，含边界值）

## 切片 D-18：escapeHtml 重复实现消除（已完成）

- `GroupMonitor.svelte` 本地 `escapeHtml` 与 `wechat/utils/index.ts` 实现逐字符一致，
  后者补 `export`，GroupMonitor 改从共享 utils 导入
- 回归（D-17/D-18）：svelte-check 0 errors / 176 warnings；构建通过；
  二十一个冒烟/单元测试全部通过

## 切片 D-19：GlobalSearch 文本处理下沉（已完成）

- 新增 `src/lib/search/searchText.ts`：`highlight`（正则转义 + 大小写不敏感包
  `<mark>`）、`excerpt`（命中位置摘要 + 省略号）
- `GlobalSearch.svelte` 删除本地实现（约 30 行）

## 切片 D-20：API 调试地址构造下沉（已完成）

- 新增 `src/lib/components/apiUrl.ts`：`apiDebugUrl(path, port, token)`
  （?/& 分隔自适应、token 编码，参数化脱离组件状态）
- `ApiHelpModal.svelte` 改调共享函数
- 新增 `smoke-search-text.mjs`（15 断言，覆盖两个模块）
- 回归（D-19/D-20）：svelte-check 0 errors / 176 warnings；构建通过；
  二十二个冒烟/单元测试全部通过

## 切片 D-21：HookManager 会话类型识别下沉（已完成）

- 新增 `src/lib/wechat/utils/session.ts`：`isGroup`、`isOfficial`、`kindOf`、
  `SessionKind` 类型
- `HookManager.svelte` 删除本地实现与类型定义（约 15 行），改从共享模块导入
- 注：`avatarLetter`/`colorFromName` 在 utils/format 已有不同语义的实现
  （中文首字/不同色板），HookManager 本地版本行为不同，保留不动
- 新增 `smoke-wechat-session.mjs`（10 断言，含群聊优先于公众号）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  二十三个冒烟/单元测试全部通过

## 切片 D-22：OcrPanel 展示纯函数下沉（已完成）

- 新增 `src/lib/ocr/display.ts`：`prettyJson`、`statusLabel`、`statusCls`、
  `catLabel`（24 类映射）与 `STATUS_META`/`CATEGORY_ORDER`/`COMMON_ENDPOINTS` 常量
- `OcrPanel.svelte` 删除本地实现（约 70 行），模板常量引用保持同名
- 新增 `smoke-ocr-display.mjs`（14 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  二十四个冒烟/单元测试全部通过

## 切片 D-24：PreferencesPanel 颜色工具下沉（已完成）

- 新增 `src/lib/components/colorUtils.ts`：`hexToRgba`、`hexLum`（Rec.709 加权）、
  `swatchTextColor`、`swatchSubColor`
- `PreferencesPanel.svelte` 删除本地实现（4 个函数）
- 新增 `smoke-color-utils.mjs`（9 断言，含亮度阈值与 rgba 格式）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  二十五个冒烟/单元测试全部通过

## 切片 D-25：MessageBody markdown 渲染管线下沉（已完成）

- 新增 `src/lib/llm/messageRender.ts`：`parseBlocks`/`miniMarkdown`/`inlineMd`/
  `safeJson`/`isAudioUrl` 与 `Block` 类型、媒体扩展名常量（约 200 行整体迁移）
- `MessageBody.svelte` 删除本地渲染实现与常量，模板/派生逻辑零改动
- 新增 `smoke-message-render.mjs`（18 断言：行内样式、媒体识别、块解析、图表块）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  二十六个冒烟/单元测试全部通过

## 切片 D-27：UsageCostTab 成本格式化下沉（已完成）

- 新增 `src/lib/llm/costFormat.ts`：`fmtLimit`（null → 不限、千分位）、`fmtRatio`
- `UsageCostTab.svelte` 删除本地实现
- 新增 `smoke-cost-format.mjs`（5 断言）

## 切片 D-28：ChartView 几何纯函数下沉（已完成）

- 新增 `src/lib/components/chartGeometry.ts`：`PALETTE`/`chartColor`（10 色循环）、
  `polar`（角度 0=正上方）、`arcPath`（扇形 SVG path，large-arc 自适应）
- `ChartView.svelte` 删除本地实现；组件内保留 `color` 包装保持模板零改动
- 新增 `smoke-chart-geometry.mjs`（7 断言：极坐标方位、弧标志位）
- 回归（D-27/D-28）：svelte-check 0 errors / 176 warnings；构建通过；
  二十八个冒烟/单元测试全部通过

## 切片 D-29：KbDashboard 日期格式化 / delay 工具（已完成）

- `format.ts` 新增 `formatDateOnly`（YYYY-MM-DD）；`KbDashboard.fmtTime` 改调共享实现
- 新增 `src/lib/async.ts`：`delay`；`WeChatBootstrap.svelte` 删除本地实现
- 新增 `smoke-async-utils.mjs`（2 断言）；`smoke-format-bytes.mjs` 扩展至 26 断言
- ⚠️ 事故记录：曾误用 `src/lib/utils.ts` 文件名创建 delay，覆盖了 shadcn 组件的
  `cn`/`WithElementRef` 导出（385 错误）——已从 `_backups_20260813_111314` 恢复原文件，
  delay 改放 `async.ts`。教训：新增模块前必须先确认目标文件名未被占用。
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  二十九个冒烟/单元测试全部通过

## 切片 D-30：AgentPanel 表单工厂 / ProviderConfigTab 数值解析（已完成）

- 新增 `src/lib/agents/agentForm.ts`：`AgentInput` 类型、`createBlankAgentForm`、
  `agentToForm`（AgentItem → 编辑表单）
- 新增 `src/lib/llm/numOrNull.ts`：`numOrNull`（空串/非法 → null）
- `AgentPanel.svelte` 删除本地 `AgentInput` 定义与表单构建逻辑（3 处）；
  `ProviderConfigTab.svelte` 删除本地 `numOrNull`
- 新增 `smoke-agent-form.mjs`（7 断言：默认值/映射/数值容错）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十个冒烟/单元测试全部通过

## 切片 D-31：AnnualSummary 展示纯函数下沉（已完成）

- 新增 `src/lib/wechat/utils/annual.ts`：`heatBg`（热力色透明度映射）、
  `fmtNum`（万缩写）、`fmtInt`（zh-CN 千分位）、`pct`（0.1% 精度占比）
- `AnnualSummary.svelte` 删除 4 个本地函数
- 新增 `smoke-annual-summary.mjs`（12 断言：热力色边界、缩写去尾 0、占比四舍五入）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十一个冒烟/单元测试全部通过

## 切片 D-32：DataDashboard SVG 路径构建下沉（已完成）

- 新增 `src/lib/system/chartPaths.ts`：`buildLine`（折线归一化）、`buildArea`
  （面积闭合）、`buildRadar`（雷达多边形 + 轴线）
- `DataDashboard.svelte` 删除 3 个本地函数（约 50 行）
- 新增 `smoke-chart-paths.mjs`（8 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十二个冒烟/单元测试全部通过

## 切片 D-33：GeneralRecords 记录展示下沉（已完成）

- 新增 `src/lib/wechat/utils/records.ts`：`KIND_PATHS`/`kindIcon`（SVG 图标）、
  `transferSubType`/`hbStatus`/`liveStatus`（状态映射）、`shortUser`（截断）
- `GeneralRecords.svelte` 删除本地实现（约 60 行）；`fmtTime` 收敛到
  `formatTs`（invalidPlaceholder 保持原回退语义）
- 新增 `smoke-wechat-records.mjs`（12 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十三个冒烟/单元测试全部通过

## 切片 D-34：DailySummary 汇总展示下沉（已完成）

- 新增 `src/lib/wechat/utils/summary.ts`：`fmtTime`（毫秒 → 完整日期时间，
  空/非法 → '—'）、`fmtDate`、`fmtDuration`（ms/s 两档）、`fmtTokens`
  （万缩写去尾 0，≤0 → 空）
- `DailySummary.svelte` 删除 4 个本地函数
- 新增 `smoke-daily-summary.mjs`（11 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十四个冒烟/单元测试全部通过

## 切片 D-36：GraphView 时间轴纯函数下沉（已完成）

- 新增 `src/lib/wechat/utils/graphView.ts`：`numT`（安全数值转换）、
  `nodeT`/`edgeT`（t/last_ts 时间戳提取，参数类型化）、`fmtTime`、`clamp01`
- `GraphView.svelte` 删除 5 个本地函数；`ctxMenu` 等状态保持组件内
- 新增 `smoke-graph-view.mjs`（10 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-29：文件/语音 IPC 返回类型化（已完成）

- `wechat/types.ts` 新增 `ResolvedFile`（path/dir/found）；
  `ipc.ts`：`resolveWechatFile` → `Promise<ResolvedFile>`、
  `getMessageVoice` → `Promise<MediaResult>`（原 `any`）
- `WeChatPanel` 3 处 `const res: any`/`const r: any` 去除（类型推断）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-30：AnnualSummary 数据类型化（已完成）

- `AnnualSummary.svelte`：`data` 状态从 `$state<any>(null)` 改为
  `$state<AnnualSummaryData | null>(null)`，消除 7 处隐式 any；
  `years` map 移除冗余 `(x: any)` 标注
- `wechat/types.ts`：`AnnualSummaryData` 按 Rust 后端实际载荷精确化——
  新增 `AnnualTopItem`（key/name/count）、`AnnualKindCount`（kind/label/count）、
  `AnnualMomentItem`（首末消息对象）、`AnnualHeatmap`（weekdayLabels/
  hourLabels/matrix/total），`earliest`/`latest` 由 `string` 修正为
  `AnnualMomentItem | null`，全部字段改为必填并移除 `[key: string]: unknown`
- 类型化暴露 38 处真实类型错误（可选字段传给 `number` 参数、`kind`/`key`/
  `weekdayLabels` 缺失等），全部随类型修正消解，运行时行为不变
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-31：DailySummary IPC 结果类型化（已完成）

- `DailySummary.svelte` 移除 2 处显式 `any`：`reloadProviders` 的
  `const cfg: any`（推断为 `LlmConfig | null`）与 `testConnection` 的
  `const r: any`（推断为 `TestResult`），两者均由 `llmApi` 返回类型提供
- 剩余 `any` 仅为 `catch (e: any)`（项目约定保留）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-32：归档导出/备份导入 IPC 结果类型化（已完成）

- `wechat/types.ts` 新增 `WechatArchiveResult`（path/filename/file_count/
  total_bytes，对应 Rust `export_archive`）与 `WechatImportResult`
  （imported/target，对应 Rust `import_wechat_backup`）
- `ipc.ts`：`exportWechatArchive`/`importWechatBackup` 返回类型从 `any`
  精确化，参数从 `Record<string, unknown>` 收紧为实际字段
- `WeChatConfig.doImport`、`WeChatPanel` 归档流程各去除 1 处 `const r: any`
  （`archiveResult` 现有类型声明可直接承接）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-33：DbManager 表结构/单元格/拖拽类型化（已完成）

- `db/types.ts` 新增 `DbCellValue` 判别联合（null/text/blob/error，
  对应 Rust `cell_value_to_json`）
- `db/services/ipc.ts`：`tableSchema`/`externalTableSchema` → `DbColumn[]`、
  `queryTable`/`externalQueryTable` → `DbTableData`、`getCellValue` → `DbCellValue`
  （原 `invoke<any>`）
- `DbManager.svelte` 清理 10 处 `any`：`dbSchemaInfo` 改用 `DbColumn[]`、
  `blobViewer.data` 改用 `DbCellValue | null`、`dbFilterDebounce` 用
  `ReturnType<typeof setTimeout> | undefined`、JSON.parse 结果用类型守卫
  (`s is string`)、行 rowid 提取用 `DbRow`、列结构对比移除 `as any[]`、
  拖拽事件交给 `onDragDropEvent` 推断（`Event<DragDropEvent>`）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-34：语音转写/收藏媒体/消息日历/通讯录 IPC 类型化（已完成）

- `wechat/types.ts` 新增 `TranscribeResult`（data/none 判别联合）、
  `DailyCountsResult`（counts/year/month）、`ContactPageResult`
  （contacts/total/has_more，对应 Rust `ContactPage`）
- `ipc.ts`：`getContactsByCategory` → `ContactPageResult`（原 `any[]`）、
  `getFavoriteImage`/`getFavoriteVoice` → `MediaResult`、
  `transcribeMessageVoice` → `TranscribeResult`、
  `getChatDailyCounts` → `DailyCountsResult`；参数从 `Record<string, unknown>`
  收紧为实际字段
- `WeChatPanel` 6 处 `const r: any = await …` 去除（含 `clearAllSessionDrafts`，
  其返回类型此前已精确化）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-35：朋友圈加载/增量刷新类型化（已完成）

- `WeChatPanel` 朋友圈模块清理 14 处 `any`：`loadMoments`/`refreshMomentsAuto`
  的 `const r: any` 交由 `getMoments`/`refreshWechatMoments` 返回类型推断，
  `incoming` 显式 `MomentEntry[]`，去重/排序/预载回调全部去掉 `any` 标注，
  `filteredMoments` 搜索回调同理
- 发现并移除失效分支：后端 `refresh_wechat_moments` 只返回
  `{ items, total, has_more }`（Rust `MomentsPage`），前端此前读取的
  `r?.interactions` 恒为 undefined——「全量互动合并」是死代码，删除后
  运行时行为不变（原 map 恒等返回），最新页数据更新由既有 byTid 分支负责
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-36：静态表情/公众号状态类型化（已完成）

- `wechat/types.ts` 新增 `OfficialAccount`（对应 Rust `OfficialAccount`：
  username/name/official_kind/ts/time/summary/unread_count/pinned/history_url）；
  `SessionEntry` 补 `unread_count?: number`（后端 session 查询实际返回该字段，
  原类型缺失，导致 `sessionItem` 强类型化后 `unknown`/`possibly undefined`）
- `ipc.ts`：`getOfficialAccounts` → `Promise<OfficialAccount[]>`（原 `any[]`）
- `WeChatPanel`：`staticEmoticons` → `StaticEmoticonCategory[]`、
  `bizchats` → `OfficialAccount[]`，`bizItem`/`sessionItem` 片段参数分别改为
  `OfficialAccount`/`WeChatSession`，清理约 10 处 `any` 回调；未读角标
  改用 `(s.unread_count ?? 0)` 显式兜底（行为不变）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-37：文件管理状态/列表类型化（已完成）

- 修正 `ResourceFile`/`ResourceFilesOverview` 类型与 Rust 后端不符的问题：
  实际返回 `images/videos/files + total_size/total_size_label/images_total/
  videos_total/files_total`（原类型误写为 `files/total_size/total_size_label`）；
  `ResourceFile` 补齐 size_label/modify_time/time/category/ext/path/cover_path
- `WeChatPanel`：`fileData` 从 `$state<any>` 改为 `ResourceFilesOverview`
  （含空态常量），`fileViewer` 条目类型化为 `FileViewerItem`，
  `openFileImageViewer`/`openFileVideo` 参数改为 `ResourceFile`，
  过滤/排序/计数回调去掉约 10 处 `any`；打开/定位按钮用 `f.path ?? ''`
  显式兜底（仅在 path 存在时渲染，行为不变）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-38：消息编辑/原始字段编辑类型化（已完成）

- `wechat/types.ts` 新增 `SessionEditedItem`（db/table_name/local_id/
  edit_count/last_edited_at）、`ChatEditStatus`（modified + 编辑元数据）、
  `MessageRawRowResult`（row/db/table，row 用 `Record<string, unknown>`）
- `ipc.ts` 六个函数类型化：`listSessionEditedMessages`/`getChatEditStatus`/
  `getMessageRawRow` 返回精确结果，`editChatMessage`/`resetEditedMessage`/
  `updateMessageRawFields` 改 `Promise<void>`，参数从 `Record<string, unknown>`
  收紧为实际字段
- `WeChatPanel`：`editMenu.msg` → `WeChatMessage | null`、`openEditMenu`
  参数 → `WeChatMessage`、`seed`/`edits` 改用 `Record<string, unknown>`、
  两处 `const r: any` 去除；`canEdit` 判定用 `!!m.text && !m.rich`
  （原 `m.text && …` 在强类型化后产生 `string | boolean`，行为不变）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-39：设置分类状态/导出类型化（已完成）

- 修正 `GeneralCategory` 类型与 Rust 后端不符：实际为 key/label/table/
  columns/column_labels/rows(`unknown[][]`)/count/total（原类型仅
  name/count）；新增 `GeneralCategoryCsvResult`（csv）
- `ipc.ts`：`exportGeneralCategoryCsv` → `Promise<GeneralCategoryCsvResult>`
  且参数收紧为 `{ table: string }`（原 `Record<string, unknown>` + `any`）
- `WeChatPanel`：`settingsData` → `GeneralCategory[]`，统计/过滤/导出回调
  去掉 6 处 `any`，`exportSettingsCat` 参数类型化
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-40：全局消息搜索类型化（已完成）

- `search/types.ts`：`WechatSearchHit` 补 `local_id: number`（必填）与
  snippet/ts/create_time（后端索引/FTS 与全表扫描两个路径均返回 local_id）
- `wechat/types.ts` 新增 `SearchIndexStatus`（exists/rows/built_at）与
  `SearchIndexBuildResult`（status/rows/message/built_at）
- `ipc.ts`：`buildWechatSearchIndex`/`getWechatSearchIndexStatus` 返回类型化，
  `searchWechatMessages` 参数收紧为 `{ query, limit? }`
- `WeChatPanel`：`msgSearchResults` → `WechatSearchHit[]`，`openSearchHit`
  参数类型化，`openAskCitation` 参数与 AskPanel/GroupMonitor/PrivacyScan 的
  `onJump` 契约对齐，构建/搜索两处 `const r: any` 与 `tryJumpToMessage`
  的 findIndex 标注去除
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-41：WeChatPanel 剩余回调/分类赋值清理（已完成）

- `wechat/types.ts`：`FavoriteEntry` 补齐后端实际字段
  （title/desc/url/ts/time/source/sync_status/server_id，均可选），
  使收藏过滤/详情回调可强类型化
- `WeChatPanel` 清理最后一批非 catch `any`：CSV 导出分发表改
  `Promise<ExportResult>`（r.count/r.path 直接可用）、收藏/表情包/自定义
  表情/通讯录过滤回调去掉 `any`、`getMessageImage` 结果交由 `MediaResult`
  推断、`openFavDetail(f: FavoriteEntry)`、朋友圈点赞 `(l: any)` 去掉；
  通讯录/文件分类切换改用 `as const` 元组数组，消除 `contactCat = k as any`
  与 `fileCat = k as any`
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-42：配置/账户/收藏详情/联系人 IPC 类型化（已完成）

- `wechat/types.ts` 新增 `ApiSettings`（enabled/port/token）、
  `WechatAccountStatus`（analysis/live/mtime/mismatch/weixin_running）、
  `FavoriteDetail`（按 Rust `parse_fav_detail` 结构：images/video/link/
  location/file/items/voice_server_id 等）；`ContactItem` 补 description
- `ipc.ts`：`getApiSettings`/`getWechatAccountStatus`/`getFavoriteDetail`/
  `getContactProfile` 返回类型精确化；`openWechatFolder`/`openWechatPath`
  改 `Promise<void>` 且参数收紧为 `{ path: string }`
- 组件侧：`mediaApi.loadMediaConfig` 去除 `const s: any`；
  `WeChatPanel`/`WeChatConfig` 的 accountStatus、profileData、favDetail
  状态全部类型化；收藏详情模板用 `{@const fd = favDetail}` + `vsid`/`lk`
  局部绑定解决 Svelte 闭包内不保留窄化的问题（运行时行为不变）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-43：备份/隐私体检/图片体检/关系图谱 IPC 类型化（已完成）

- `wechat/types.ts` 新增：`WechatBackupCreateResult`（path/filename/size/
  file_count/created_at）、`WechatBackupRestoreResult`（restored/imported/
  target）、`WechatBackupItem`/`WechatBackupListResult`、
  `PrivacySample`/`PrivacyCategory`/`PrivacyTopItem`/`PrivacyScanResult`
  （均按 Rust `backup.rs`/`privacy.rs` 实际返回结构）
- `ipc.ts` 八个函数返回类型精确化：备份创建/恢复/列表、`deleteWechatBackup`
  → `Promise<void>`、`scanPrivacyRisks` → `PrivacyScanResult`、
  `getWechatMissingImages` → `MissingImagesData`、
  `exportWechatMissingImagesCsv` → `ExportResult`、
  `getRelationshipGraph(ed)` → `GraphRawData`（`cached` 可为 null）
- `BackupManager`/`PrivacyScan`/`RelationshipGraph`：3+1+3 处 `const r: any`
  去除，状态全部类型化；发现并移除 `getRelationshipGraph` 调用中后端
  不存在的 `force` 参数（handler 仅接受 limit，传参恒为无操作，行为不变）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-44：AI 问答/记录导出/每日总结/历史快照 IPC 类型化（已完成）

- `wechat/types.ts` 新增 `SessionSnapshot`（对应 Rust SessionTable 查询行）、
  `RecordsCsvResult`、`AskCitation`/`AskStatsTable`/`AskWechatResult`
  （对应 Rust `ask_wechat` 返回：answer/citations/stats/steps/rounds/plan/
  llm_used/elapsed_ms）
- `ipc.ts` 六个函数类型化：`getWechatHistory` → `WeChatMessagePayload[]`、
  `getSessionSnapshots` → `SessionSnapshot[]`、`askWechat` → `AskWechatResult`、
  `listDailySummaryTasks` → `DailySummaryTask[]`、
  `saveDailySummaryTask` → `Promise<DailySummaryTask>`（参数收紧为
  `{ task: DailySummaryTask }`）、`runDailySummaryRange` 参数收紧且改
  `Promise<void>`、`exportWechatRecordsCsv` → `RecordsCsvResult`
- `AskPanel`/`GeneralRecords` 各去除 1 处 `const r: any`
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-45：密钥/解密/STT/OCR/打开路径 IPC 清零（已完成）

- `wechat/types.ts` 新增 10 个结果类型：`VerifyDatabaseKeyResult`、
  `GenerateKeysResult`、`AutoKeysResult`、`SwitchAccountResult`、
  `DecryptAllResult`、`VerifyImageKeyResult`、`DecodeImagesResult`、
  `SttStatus`/`SttConfigInput`/`SttDownloadResult`（均按 Rust
  `config.rs`/`stt/mod.rs` 实际返回结构）
- `ipc.ts` 最后 16 处 `Promise<any>` 全部精确化：`autoGetDbKeyV2` →
  `AutoDbKeyResult`、`autoGetWechatKeys` → `AutoKeysResult`、检测/扫描账号 →
  `DetectedAccount[]`、STT 三函数、`ocrIngestResource` → `Promise<number>`
  （Rust 返回 i64）、`openWechatAttachFolder`/`openWechatProtocol` →
  `Promise<void>` 且参数收紧；`saveWechatConfig(config: any)` 同步清理
- `WeChatConfig`：`sttStatus`/`sttDlProgress` 状态类型化；`ensureDbDir`/
  `useDetectedAccount` 处理可选 `db_dir`，`runAutoGetKeys` 对可选
  `db_key`/`image_key` 加守卫；`WeChatPanel` 附件按钮 `curSession ?? ''`
  兜底（行为不变）
- 里程碑：`ipc.ts` 全文件 `any` 清零（`Promise<any>` 0 处，其他 `any` 0 处）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-46：图谱力导向 d3-force 类型化（已完成）

- 根因是 `src/types/d3-force.d.ts` 环境声明过于简陋（`force` 只接受
  `unknown`、`forceLink`/`forceCenter` 等全返回 `any`），导致图谱组件
  不得不大量 `as any`。按 d3-force 3.x 实际 API 重写声明：补
  `alphaDecay`/`velocityDecay`/`alpha()` 读取重载，`force(name)` 返回
  `Force | undefined`，新增 `ForceLink`/`ForceManyBody`/`ForceCenter`/
  `ForceCollide` 具体接口
- `GraphView.svelte`：`SimNode`/`SimEdge` 独立建模（含运行时位置/速度、
  增量 snake_case 字段、端点对象或字符串），`sim` → `Simulation<SimNode,
  SimEdge> | null`，`nodeById` → `Map<string, SimNode>`，新增
  `endpointNode` 端点解析；清理 17 处 `any`（含 `as any` 的 link/center
  力改为 `ForceLink`/`ForceCenter` 类型断言），可选坐标 `?? 0` 兜底
- `GraphCanvas.svelte`：清理 8 处 `any`（边端点收窄为
  `typeof === "object" ? es : index.get(es)`、`linkStrength(e: GEdge)`、
  `forceLink<GNode, GEdge>` 等），删除失效 `force` 参数的同类问题
- 里程碑：GraphView.svelte 与 GraphCanvas.svelte 非 catch `any` 均清零
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-47：群监控台类型化（已完成）

- `wechat/types.ts`：`SessionEntry` 补齐后端实际字段
  （raw_summary/draft/ts/sort_ts/pinned/time/full_time/is_group），
  `WeChatMessagePayload` 补 `media_type`（媒体监控判定用）
- `GroupMonitor.svelte` 清理 7 处 `any`：`groups` → `SessionEntry[]`，
  `getSessionList` 结果交由类型推断，`matchMonitors`/`handlePayload` 参数改为
  本地 `MonitorPayload`（`WeChatMessagePayload` + 可选 batch/messages 信封），
  `FeedMsg.raw` 同型，自动化规则 `conditions` 用显式三元组类型，
  事件监听边界用 `unknown` + 一次类型断言（JSON 反序列化处）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-48：图表规范类型化（已完成）

- `llm/types.ts` 新增 `ChartSpec`（type/title/labels/series/data，
  字段宽松可选，兼容 LLM 输出的柱状/折线/饼图 JSON）
- `messageRender.ts`：`Block` 联合的 `{ type: "chart"; spec: any }` →
  `spec: ChartSpec`（根因）
- `ChartView.svelte`：`spec` 属性 → `ChartSpec`，`normalize(s: ChartSpec)`，
  数据归一化回调去掉 `(d: any)`/`(ser: any)`/`(v: any)`，label 用
  `String(...)` 显式转字符串（行为不变）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-49：朋友圈媒体服务/记录状态映射类型化（已完成）

- `ipc.ts`：`getMomentImage`/`getMomentVideo` 参数从 `Record<string, unknown>`
  收紧为实际字段（url/key/token）
- `momentMedia.svelte.ts`：三个函数参数改为 `MomentImageLike` 结构类型
  （thumb/url/key/thumb_token/url_token，兼容 `MomentMedia` 与视频封面等
  部分对象），`getMomentImage` 结果交由 `MediaResult` 推断；原图拉取用
  局部 `url` 常量消除可选属性窄化问题
- `momentVideo.svelte.ts`：`playMomentVideo(m: MomentEntry, idx)`，
  `getMomentVideo` 结果交由 `MediaResult` 推断
- `records.ts`：`transferSubType`/`hbStatus`/`liveStatus` 参数 `any` →
  `unknown`（单元格值本质为任意 JSON 标量）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-50：全局搜索通讯录/图标类型化（已完成）

- 修正 `ContactBook` 类型与 Rust 后端不符：实际为
  `{ contacts, labels, stats }`（原类型误写为 `items`），使
  `getContacts()` 返回类型可直接消费
- `GlobalSearch.svelte`：`SCOPES` 图标与 `SectionTitle` 片段参数
  `icon: any` → Svelte `Component` 类型；`getContacts` 结果交由
  `ContactBook` 推断（`r?.contacts` 直接可用），清理 3 处 `any`
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-51：KB 文档/统计 IPC 类型化（已完成）

- `kbTypes.ts` 新增 `BatchDownloadResult`（dataBase64/fileName/count）、
  `UpdateChunkResult`（chunkId/docId/embedded/content/warning）、
  `ReprocessResult`（chunkCount/embedded）、`AnalyticsResult`
  （metrics 复用既有 `AnalyticsMetric`，避免重复声明冲突）
- `kb/services/ipc.ts`：`batchDownload`/`updateChunk`/`reprocessDocument`/
  `getAnalytics` 返回类型从 `unknown` 精确化
- `KbDocs.svelte` 去除 4 处 `const res: any`（分块保存/批量下载/重新处理/
  Wiki 提炼）；`KbTrendChart.svelte` 去除 3 处 `any`（统计加载回调与
  `as any[]` 断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-52：自动化/引导/图谱缓存/工具类型小集群清理（已完成）

- `automation/services/ipc.ts`：`connStatus` 返回类型从 `unknown` 精确化
  （connected/received/lastAt/url，对应 Rust `automation_conn_status`）；
  `AutomationPanel` 的 `Task.aiExtract/fullJson` → `unknown`，
  `connStatus = ... as any` 去除
- `ApiHelpModal`：会话/消息查找回调去掉 `(s: any)`/`(m: any)`
  （`SessionEntry[]`/`WeChatMessage[]` 直接推断）
- `WeChatBootstrap`：`cfg` → `WechatConfigResult | null`、
  `accounts` → `DetectedAccount[]`，`resolved` 用 `WechatConfigData` 断言
- `utils.ts`：`WithoutChild`/`WithoutChildren` 的条件类型 `any` → `unknown`
  （移除 eslint-disable 注释）
- `KbDashboard`：`getAnalytics` 结果交由 `AnalyticsResult` 推断
- `RelationshipGraph`：`graphModuleCache` 数据 → `GraphRawData`，
  进度事件监听用结构化类型替代 `(event: any)`
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-53：收尾零散 any 清理（已完成）

- `DataDashboard`：`pollTimer: any` → `ReturnType<typeof setTimeout> | null`
- `dbUtils`：canvas 单例从 `(fn as any)._canvas` 改为模块级
  `measureCanvas`（`??=` 惰性创建），`getContext` 加 null 守卫
- `db/services/ipc.ts`：`scanExternalDbs` → `DbFileEntry[]`，
  `DbFileEntry.size_bytes` 改为必填（对齐 Rust `DbFileInfo`）
- `kbTypes.ts` 新增 `HousekeepingResult`/`ModelRef`/`ModelSettingsResult`；
  `kbApi.housekeeping`/`getModelSettings` 返回类型精确化；
  `KbActivity`/`KbSettings` 的 `const res: any` 去除；
  `kbChunkStore` 的 `invoke` 结果类型化（strategy 用直接比较收窄）
- `KbChat`：RAG 请求 `input: any` → `Record<string, unknown>`
- `WikiPanel`：localStorage 恢复用 `Record<string, unknown>` 断言替代
  `(graphParams as any)`
- `messageRender.safeJson` → `ChartSpec | null`；`AskPanel.plan` → `unknown`；
  `BackupManager` 进度监听用结构化 payload；`virtualList.estimateMsgHeight`
  参数 → `WeChatMessage`
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-54：语音电平 RMS 计算收敛（已完成）

- 边界识别：GlobalChatTab 中「打断监听」与「录音 VAD」两处各自内联了
  相同的时域均方根电平计算（`getByteTimeDomainData` → RMS）
- `llm/services/voice.ts` 新增纯函数 `rmsLevel(buf: Uint8Array)`，
  GlobalChatTab 两处重复循环统一改调（阈值 0.035/0.012 保持原样）
- `voice.test.mjs` 新增 4 项断言（静音=0、满幅方波精确值、有能量、
  空缓冲 NaN 与原地实现一致），并把 `voice.test.mjs` 纳入回归门禁
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过（35 个 smoke/run-store + voice.test）

## 切片 R-1：Rust 全量格式规范化（已完成）

- 边界识别：`cargo fmt --check` 显示整个 src-tauri crate（约 200 个文件）
  与 rustfmt 默认格式存在系统性漂移（无 rustfmt.toml）
- 执行 `cargo fmt`（机械式批量格式化，行为不变），随后
  `cargo fmt --check` 退出码 0
- 回归：
  - `cargo check` 通过（仅 1 个既有无害警告 `unused_assignments`）
  - `cargo test --lib --no-default-features`：211 passed / 0 failed
    （19 ignored，需真实微信解密库）
  - 前端门禁：svelte-check 0 errors / 176 warnings；36/36 测试通过；
    `npm run build` 通过

## 切片 R-2：Rust 编译警告清理（已完成）

- 边界识别：`cargo check` 报 1 处 `unused_assignments`——
  `llm/handlers.rs::transcribe_voice_audio` 的 `let mut last_err = String::new()`
  初始值在云端转写块内被覆盖前从未被读取
- 改为 `let mut last_err: String;`（推迟初始化，所有路径在读取前均赋值或
  提前 return，Rust 流分析保证确定赋值；行为不变）
- 回归：
  - `cargo check`（默认特性，含 local-stt 读取分支）：0 warnings
  - `cargo test --lib --no-default-features`：211 passed / 0 failed
    （19 ignored）
  - 期间处理了一次超时留下的孤儿 cargo 进程（文件锁/管道阻塞），清理后
    测试正常完成

## 切片 T-55：语音录音状态机下沉（已完成）

- 边界识别：GlobalChatTab 的录音捕获（MediaRecorder）+ 电平 VAD +
  静音 1.6s 自动停止 + 60s 无语音超时，与组件 UI/转写流程高度耦合，
  是本仓库最后一个大内聚块
- 新增 `llm/services/voiceRecorder.svelte.ts`：`$state` 状态
  （recording/micError）+ `startVoiceRecorder(stream, hooks)` /
  `stopVoiceRecorder(auto)` / `releaseVoiceRecorder()`，Blob 与状态文案
  通过回调交回组件（组件仍拥有 voiceStatus 的 TTS 文案与 mediaStream 所有权）
- `GlobalChatTab.svelte`：删除 11 个录音私有状态与
  startRecording/stopRecording/startLevelMonitor 的实现体，改为薄委托；
  onMount 清理改调 `releaseVoiceRecorder()`；模板 `recording` →
  `voiceRecorder.recording`，录音启动错误经 `voiceRecorder.micError` 展示
- 注意：打断监听（barge-in）仍复用组件级 `audioCtxRef`，与录音服务
  各自持有 AudioContext，互不干扰；行为不变量（MIME 选择、VAD 阈值
  0.012、静音窗口 1600ms、超时 60s、状态文案）逐一保留
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过

## 切片 T-56：VAD 状态机纯函数化（已完成）

- `llm/services/voice.ts` 新增 `VadState`（voiced/silenceStart）与纯函数
  `vadStep(rms, state, now, opts?)`：电平超阈值 → voiced 并清零静音计时；
  voiced 后静音超 1600ms → stop；参数可配置（threshold/silenceMs）
- `voiceRecorder.svelte.ts` 的 tick 改用 `vadStep` 推进状态机；
  60s 无语音超时检查保留在 tick 外层（分析器不可用时仍能超时停止，
  与下沉前行为一致）；阈值 0.012 / 静音窗 1600ms 为默认值
- `voice.test.mjs` 新增 7 项断言锁定可观测输出（未 voiced 静音不计时、
  超阈值标记、静音起止、1.6s 自动停止、重新说话复位）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过

## 切片 T-57：TTS 音频 MIME 映射收敛（已完成）

- `llm/services/voice.ts` 新增纯函数 `audioMime(fmt)`（wav/ogg/flac/aac/
  opus/mp3 + 未知/空格式回退 mpeg），GlobalChatTab 删除本地同名实现
  （两处调用点统一走共享函数）
- `voice.test.mjs` 新增 8 项断言（各格式映射、大小写不敏感、空/未知回退）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过

## 切片 T-58：TTS 播放器状态机下沉（已完成）

- 新增 `llm/services/ttsPlayer.svelte.ts`：`$state` 状态
  （speaking/speakingIndex/audioPlayer）+ `playTtsAudio(src, msgIndex, opts)`
  （Audio 播放、状态文案、ended/error/play 拦截处理、打断监听启停钩子）+
  `stopTtsPlayer()`（暂停清空、复位、resolve 等待中的播放）+
  `ttsDataUrl(res)`（MIME → data URL）+
  `setTtsPlayerHooks()`（组件 onMount 注册一次）
- `GlobalChatTab.svelte`：删除 speaking/speakingIndex/audioPlayer/playResolve
  与 `playProviderChunk` 实现体，`playSpeechChunk` 改为薄委托；
  `stopSpeaking` → `stopBargeInMonitor() + stopTtsPlayer()`；
  语音模式守卫与模板全部改用 `ttsPlayer.*`
- 行为不变量保留：状态文案四分支、ended/error/被拦截提示、播完与打断
  均 resolve、打断监听随播放启停
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过

## 切片 T-59：语音合成候选顺序纯函数化（已完成）

- `llm/services/voice.ts` 新增 `SpeechAttempt`/`SpeechProviderLike` 与纯函数
  `buildSpeechAttempts(current, providers)`：当前选中的提供方优先，再按
  启用提供方的「语音」模型逐个追加（不去重，保持原 trySpeech 语义）
- `GlobalChatTab.trySpeech` 的候选构建块替换为共享函数调用（输入类型
  `model_meta` 兼容 `ProviderConfig` 的 `ModelMeta`）
- `voice.test.mjs` 新增 3 项断言（顺序/禁用跳过/无候选兜底）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过

## 切片 T-60：TTS 单句合成兜底链下沉（已完成）

- 新增 `llm/services/speechSynth.svelte.ts`：`SpeechChunk` 类型、
  `$state` 的 `providerTtsFailed` 会话缓存、`synthOneSpeech(text)`（提供方
  TTS → Windows SAPI 原生兜底，含错误文案与引擎回写钩子）、
  `setSpeechSynthHooks()`
- `GlobalChatTab.svelte`：删除本地 `SpeechChunk` 类型、`providerTtsFailed`
  状态与 `synthOne` 实现体；三处调用改 `synthOneSpeech`；
  `toggleVoiceMode` 复位改 `speechSynth.providerTtsFailed`；
  onMount 注册钩子（tryProvider=trySpeech、native=llmApi、
  onEngine→ttsEngine、onError→micError）
- 行为不变量保留：失败缓存语义、native 兜底、错误文案逐字一致
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过

## 切片 T-61：流式语音播报编排下沉（已完成）

- 新增 `llm/services/speechFlow.svelte.ts`：`$state` 的 active 标志、
  会话令牌（speechSessionId/isCurrentSpeechSession）、句子队列 +
  StreamSpeechFeeder + 预取、`resetSpeechFlow`/`feedStreamSpeech`/
  `finishStreamSpeech`/`drainSpeechFlow(synth, play, isActive, onStatus,
  onDone)`
- `GlobalChatTab.svelte`：删除 6 个编排状态与
  drainSpeech/feedStreamSpeech/finishStreamSpeech 实现体；chatStream
  回调改 service 调用 + `drainSpeech()` 薄委托；`abortStreamSpeech` 改
  `resetSpeechFlow()`；`speakText` 会话令牌判断改用 service；
  新一轮语音对话复位改 `resetSpeechFlow()`
- 行为不变量保留：预取时序、会话失效判断、队列清空、播完自动聆听
  与状态文案
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过

## 切片 T-62：文档同步与 DbFileEntry 类型去重（已完成）

- 文档同步：AGENTS.md Testing Guidelines 补录 `voice.test.mjs`，并明确
  标准回归门禁为「所有 smoke-*.mjs + run-store-test.mjs + voice.test.mjs」
- 类型去重：`DbFileEntry` 从 db/services/ipc.ts 的局部导出移入共享的
  `db/types.ts`；DbManager 删除本地最小版重复声明，统一引用共享类型
  （字段兼容：path/name/size_bytes + 可选 mtime_ms + 索引签名）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过

## 切片 T-63：KB 组件 fmtTime 重复实现收敛（已完成）

- 边界识别：KbActivity 与 WikiPanel 各自内联了「ISO 字符串 → YYYY-MM-DD
  HH:mm + 非法回退」的实现，与共享的 `formatIsoTime(t, { showYear: true })`
  逐字符等价（空格分隔解析、空串/非法回退均一致）
- 两个组件改为调用 `src/lib/format.ts` 的 `formatIsoTime`（该函数已被
  smoke-format-bytes.mjs 覆盖：空格分隔、非法、空串、locale 等断言）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过

## 切片 T-64：关系图谱海报时间戳收敛（已完成）

- `RelationshipGraph` 导出海报的 `dateStr`/`timeStr` 原为手写
  `getFullYear/getMonth/getDate/getHours/getMinutes` + padStart 拼接，
  与共享的 `formatDate(now, { showYear: true })`（YYYY-MM-DD HH:mm，本地
  时区）输出逐字符一致
- 改为调用 `src/lib/format.ts` 的 `formatDate`，`dateStr = timeStr.slice(0, 10)`
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过

## 切片 T-65：播报队列/预取数据结构纯化（已完成）

- `llm/services/voice.ts` 新增泛型纯数据结构 `SpeechQueue<T>`：
  push/peek/next/length + 预取槽（setPrefetched/takePrefetched/reset）
- `speechFlow.svelte.ts` 的句子数组与预取变量替换为
  `SpeechQueue<SpeechChunk>`，drain 循环改用队列 API
  （takePrefetched → next → 预取校验：peek 相同才消费头部并回填预取）
- `voice.test.mjs` 新增 9 项断言（顺序出队、peek、空队列、预取取用并清空、
  reset 同时清空队列与预取）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过

## 切片 T-66：App 面板包裹层收敛（已完成）

- 新增 `src/lib/components/PanelSection.svelte`：统一
  `section.panel.panel-full + panel-hidden` 包裹语义（active prop + children
  片段）
- `App.svelte` 主内容区的 13 段重复
  `<section class="panel panel-full" class:panel-hidden={activeTab !== 'X'}>`
  阶梯全部改为 `<PanelSection active={activeTab === 'X'}>`，面板切换的
  class 语义集中到单一组件（含 monitor 大面板与 WeChat 启动页条件分支）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十六个冒烟/单元测试全部通过

## 切片 T-67：实时消息映射纯函数下沉（已完成）

- 新增 `wechat/utils/realtimeMsg.ts`：`toRealtimeMsg(payload, selfUsername?)`
  （WeChatMessagePayload → WeChatMessage：时间戳微秒→秒、群聊发送者、
  is_self 三方判定、通知类型、转账状态文案重算、rich 浅拷贝）
- `WeChatPanel` 删除本地同名函数（约 47 行），调用点改为
  `toRealtimeMsg(payload, selfUsername)`
- 新增 `smoke-realtime-msg.mjs`（20 项断言，esbuild bundle 处理
  ./format 依赖）：基础映射、通知类型、群聊发送者、is_self 判定、
  转账方向文案、原载荷不被改写
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十七个冒烟/单元测试全部通过

## 切片 T-68：会话排序/实时重排纯函数下沉（已完成）

- 新增 `wechat/utils/sessionOrder.ts`：`sessionBefore`（置顶优先 +
  sort_ts 降序比较器）与 `upsertSessionOrdered(list, username, updated)`
  （命中头部原地替换 / 删除后二分插入 / 未命中追加，返回新数组不修改
  入参）
- `WeChatPanel.mergeSessionUpdate` 的局部比较器与二分插入逻辑替换为
  共享函数调用（行为不变：头部 O(1)、二分 O(log n)、追加语义保留）
- 新增 `smoke-session-order.mjs`（11 项断言）：比较器规则、头部/中部/
  未命中三种更新路径、有序不变量、入参不可变
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十八个冒烟/单元测试全部通过

## 蓝图 R-蓝图-1：kb/handlers.rs 拆分设计（仅规划，未执行）

### 现状与边界

- `src-tauri/src/kb/handlers.rs` 约 5440 行、70+ 个 `#[tauri::command]`，
  覆盖文档/分块/搜索/RAG/版本/统计/Wiki/权限/QA/任务等全部 KB 域，
  是仓库最大的单体文件；`src/lib.rs` 通过 `kb::handlers::<cmd>` 逐个注册。
- 共享设施已就位：`helpers::run_blocking`、登录守卫
  （`session.get()`）、`KbDatabase` State——各域命令仅依赖这些 + 本域逻辑。

### 目标布局（按域拆分）

```
kb/handlers/
  mod.rs        // 汇总 re-export 全部命令（lib.rs 注册点不变）
  docs.rs       // 文档 CRUD/上传/下载/目录/移动/重命名/标签
  chunks.rs     // 分块读写/重处理/向量化
  search.rs     // kb_search / kb_rag / kb_rag_stream / kb_highlight
  versions.rs   // 版本列表/差异/回滚
  analytics.rs  // 统计/埋点/推荐/卡死任务兜底（housekeeping）
  wiki.rs       // Wiki 页面/图谱/提炼/目录
  access.rs     // 知识库/成员/角色/ACL
  qa.rs         // QA 会话/消息
  jobs.rs       // 任务列表/日志
```

### 迁移策略（小步增量）

1. 每个新模块先整体搬移若干命令 + 其私有辅助函数（保留 `pub(crate)`
   可见性，`mod.rs` 用 `pub(crate) use docs::*;` 汇总），`lib.rs` 零改动。
2. 每搬一个模块跑一次 `cargo fmt` + `cargo test --lib --no-default-features`，
   再对照回归基准（211 passed）。
3. 优先搬移低耦合命令（analytics/wiki 的纯查询类），把共享守卫与
   `helpers` 留在原位，避免早期破坏。

### 风险与验证

- 风险点：`#[tauri::command]` 必须仍为 `pub`（re-export 后保持）；宏生成的
  命令名不因模块路径变化；避免在拆分中顺手改逻辑。
- 验证：每模块完成后 `cargo fmt --check` + 全量 Rust 单测；最后
  `cargo check`（默认特性）确认 0 warnings。

> 注：`wechat/http_api.rs`（1814 行）、`wechat/voice.rs`（1701 行）为
> 次级候选，采用同一「按域拆分 + mod.rs 汇总」模式。

## 切片 R-3：kb/handlers 首个子模块拆分：analytics_settings（已完成）

- 按 R-蓝图-1 执行第一步：`kb_get_analytics_settings` /
  `kb_set_analytics_settings` / `AnalyticsSettingInput` 从 handlers.rs
  整体搬入新子模块 `kb/handlers/analytics_settings.rs`
- 共享设施保留在 handlers.rs 并改为 `pub(crate)`：
  `ANALYTICS_METRIC_DEFAULTS`、`analytics_settings_map`（analytics 统计
  命令仍在使用），子模块经 `super::` 引用
- handlers.rs 顶部声明 `mod analytics_settings; pub(crate) use
  analytics_settings::*;`，`src/lib.rs` 的
  `kb::handlers::kb_get_analytics_settings` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：按蓝图继续拆 analytics 统计 / wiki / qa 等域，每模块独立回归

## 切片 R-4：kb/handlers 拆分：analytics 埋点/推荐域（已完成）

- 继续执行蓝图：`TrackEventInput` / `kb_track_event` /
  `recommend_questions` / `kb_recommend_questions` 从 handlers.rs 搬入
  新子模块 `kb/handlers/analytics.rs`
- 依赖处理：`log_metric_event` 留在 handlers.rs 并改 `pub(crate)`（全库
  11 处调用点不变），analytics.rs 经 `super::log_metric_event` 引用；
  `recommend_questions` 的权限/可见范围走 `crate::kb::retrieval`
  （完全限定调用，保持逐字一致）
- handlers.rs 顶部 `mod analytics; pub(crate) use analytics::*;`，
  `lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：剩余 analytics 统计（kb_get_stats/analytics_for/kb_get_analytics/
  kb_housekeeping + 私有辅助）与 wiki/qa/jobs 域继续按蓝图迁移

## 切片 R-5：kb/handlers 拆分：kb_housekeeping 迁移（已完成）

- `kb_housekeeping`（卡死任务/文档状态兜底）从 handlers.rs 搬入
  `kb/handlers/analytics.rs`（蓝图 analytics 域 = 统计/埋点/推荐/卡死任务）
- 命令体逐字保留（登录守卫、spawn_blocking、两条 UPDATE SQL），
  依赖仅 KbDatabase + UserSession，无跨域引用
- `lib.rs` 注册点零改动（经 `pub(crate) use analytics::*` 汇总）
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：继续迁移 `kb_get_stats`/`stats_for`/`KbStats` 与
  `analytics_for`/`kb_get_analytics`（连同 metric_counts/build_series_7d
  等私有辅助），完成 analytics 域收口

## 切片 R-6：kb/handlers 拆分：analytics 统计核心迁移（已完成）

- 303 行逐字搬移：`detail_hit_count` / `metric_counts` /
  `build_series_7d` / `ratio_series_7d` / `analytics_for` /
  `kb_get_analytics` 从 handlers.rs 迁入 `kb/handlers/analytics.rs`
  （采用显式 LF 的批量机械搬移，保留逐字节语义；边界经校验、
  temp 备份可恢复）
- `analytics.rs` 增补 `super::analytics_settings_map` 引用；
  `ANALYTICS_METRIC_DEFAULTS`/`analytics_settings_map`/`log_metric_event`
  仍留在 handlers.rs 作为共享设施
- handlers.rs 5235 → 4932 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed；
  两文件行尾保持 LF-only
- 后续：analytics 域仅剩 `kb_get_stats`/`stats_for`/`KbStats`
  （含 KB_STORAGE_QUOTA 引用）待迁移，之后按蓝图推进 wiki/qa/jobs 域

## 切片 R-7：kb/handlers 拆分：analytics 域收口（已完成）

- 100 行逐字搬移：`KbStats`（derive + struct）与
  `impl Default`/`stats_for`/`kb_get_stats` 迁入 analytics.rs
- `KB_STORAGE_QUOTA` 保留在 handlers.rs 并改 `pub(crate)`（4 处配额检查
  仍用裸名），analytics.rs 的 Default 经 `super::KB_STORAGE_QUOTA` 引用；
  `Serialize` 导入补齐
- handlers.rs 4932 → 4831 行；`lib.rs` 注册点零改动；
  蓝图 analytics 域（统计/埋点/推荐/卡死任务/指标配置）全部收口
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：按蓝图推进 wiki / qa / jobs 域拆分

## 切片 R-8：kb/handlers 拆分：jobs 域（已完成）

- 新建 `kb/handlers/jobs.rs`（160 行）：`JobItem`/`JobLogItem` 结构、
  `list_jobs`/`kb_list_jobs`/`kb_get_job_logs` 从 handlers.rs 逐字搬移
  （显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖仅 KbDatabase/State/UserSession（完全限定）+ `retrieval::visible_kb_ids`，
  无跨域引用；handlers.rs 顶部 `mod jobs; pub(crate) use jobs::*;`
- handlers.rs 4830 → 4678 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：按蓝图推进 wiki / qa 域拆分

## 切片 R-9：kb/handlers 拆分：qa 域（已完成）

- 新建 `kb/handlers/qa.rs`（139 行）：`QaSessionItem`/`QaMessageItem` 结构
  与 `kb_qa_create_session`/`kb_qa_list_sessions`/`kb_qa_list_messages`/
  `kb_qa_delete_session` 从 handlers.rs 逐字搬移（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖仅 KbDatabase/State/UserSession（完全限定）+ rusqlite，自包含；
  handlers.rs 顶部 `mod qa; pub(crate) use qa::*;`
- handlers.rs 4680 → 4549 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：按蓝图推进 wiki 域（页面 CRUD/链接图/提炼命令）拆分

## 切片 R-10：kb/handlers 拆分：wiki 查询域（已完成）

- 新建 `kb/handlers/wiki.rs`（168 行）：`kb_wiki_list_pages` /
  `kb_wiki_dirs` + `dir_subtree_counts` / `kb_wiki_search` /
  `kb_wiki_get_page` / `kb_wiki_graph` 从 handlers.rs 逐字搬移
  （显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖：`crate::kb::wiki`（list_pages/search_pages/get_page/graph）、
  `retrieval::can_access_kb`（完全限定）、`super::log_metric_event`
- handlers.rs 4551 → 4392 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：wiki 写入域（create/update/extract/extract_all/delete/generate +
  wiki_page_kb_id 辅助）作为下一小步迁移

## 切片 R-11：kb/handlers 拆分：wiki 写入域（已完成）

- 239 行逐字搬移：`kb_wiki_create_page` / `kb_wiki_update_page` /
  `kb_wiki_extract` / `kb_wiki_extract_all` / `kb_wiki_delete_page` /
  `kb_wiki_generate` + 私有辅助 `spawn_wiki_extract` /
  `refresh_wiki_for_doc` / `wiki_page_kb_id` 迁入 `kb/handlers/wiki.rs`
  （显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：`read_model_setting` 留在 handlers.rs（父模块私有项，
  子模块经 `super::read_model_setting` 引用，可见性零改动）；
  `refresh_wiki_for_doc` 改 `pub(crate)`（handlers.rs 3 处裸名调用
  依赖 `pub(crate) use wiki::*` glob re-export，调用点零改动）；
  `spawn_wiki_extract` / `wiki_page_kb_id` 保持模块私有
- handlers.rs 4392 → 4149 行；wiki.rs 168 → 411 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed；
  两文件行尾保持 LF-only
- 后续：按蓝图继续拆分 docs（文档 CRUD/上传）、chunks、search/RAG、
  versions、access（权限/成员/用户）域

## 切片 R-12：kb/handlers 拆分：docs 域起步（目录树查询/创建）（已完成）

- 新建 `kb/handlers/docs.rs`（119 行）：`DirNode` 结构 + `kb_list_dirs` /
  `build_tree` / `set_depth` / `kb_create_dir` 从 handlers.rs 逐字搬移
  （显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖仅 `retrieval::can_access_kb` / `retrieval::kb_role`（完全限定）+
  Serialize/State/KbDatabase；handlers.rs 顶部
  `mod docs; pub(crate) use docs::*;`，`lib.rs` 注册点零改动
- handlers.rs 4149 → 4037 行；docs.rs 按蓝图后续切片继续填充
  （上传/下载/CRUD/移动/重命名/标签）
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：目录重命名/删除（kb_rename_dir/kb_delete_dir + collect_dir_docs）
  或文档上传域（kb_upload_document 及其辅助）作为下一小步

## 切片 R-13：kb/handlers 拆分：docs 域（目录重命名/删除 + 文档移动）（已完成）

- 183 行逐字搬移：`collect_dir_docs` / `kb_rename_dir` / `kb_delete_dir` /
  `kb_move_doc` 迁入 `kb/handlers/docs.rs`（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖处理：`cleanup_orphan_file_objects` 留在 handlers.rs（已是 `pub`，
  kb_delete/kb_delete_document 仍在使用），docs.rs 经
  `super::cleanup_orphan_file_objects` 引用；`is_system_kb` 留在
  handlers.rs（kb_delete/kb_update 使用，本域不依赖）
- handlers.rs 4037 → 3855 行；docs.rs 119 → 305 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：文档上传域（kb_upload_document / kb_upload_new_version /
  kb_fetch_url + 私有辅助）作为下一小步

## 切片 R-14：kb/handlers 拆分：docs 域（上传核心：首传 + 新版本）（已完成）

- 357 行逐字搬移：`UploadDocInput` / `UploadResult` / `NewVersionInput` /
  `MAX_UPLOAD_SIZE` / `global_storage_used` / `kb_upload_document` /
  `kb_upload_new_version` 迁入 `kb/handlers/docs.rs`（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖处理：`KB_STORAGE_QUOTA` / `md5_short` / `process_document_async` /
  `resolve_embedding_pair` 留在 handlers.rs，docs.rs 经
  `super::{...}` 引用；新增 `crate::kb::parse` 与
  `serde::Deserialize` 导入
- 过程中一次追加脚本漏带源文件收尾 `}`，`cargo check` 报未闭合定界符，
  立即从 temp 备份重建修复（未引入语义偏差）
- handlers.rs 3855 → 3494 行；docs.rs → 666 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：URL 抓取域（kb_fetch_url + FetchUrlInput + SSRF 防护/正文提取）
  作为下一小步

## 切片 R-15：kb/handlers 拆分：docs 域（URL 抓取 + SSRF 防护）（已完成）

- 255 行逐字搬移：`FetchUrlInput` / `kb_fetch_url` / `ip_is_private` /
  `host_is_private` / `extract_web_text` 迁入 `kb/handlers/docs.rs`
  （显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖全部已就绪：`resolve_embedding_pair` / `md5_short` /
  `process_document_async` 经 `super::` 引用，reqwest/tokio 完全限定
- 至此 docs 域「入库」路径（首传 / 新版本 / URL 抓取）全部收口；
  handlers.rs 3238 行、docs.rs 922 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：`process_document_async` + `md5_short` 随 docs 域收拢
  （唯一调用方已全部迁入 docs.rs），再推进 chunks / search / versions /
  access 域

## 切片 R-16：kb/handlers 拆分：docs 域收口（处理流水线 + 哈希辅助）（已完成）

- 171 行逐字搬移：`process_document_async`（解析 → 分片 → 向量化流水线）
  与 `md5_short`（FNV-1a 去重哈希）迁入 `kb/handlers/docs.rs`
  （显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：docs.rs 导入升级为 `parse::{self, Chunk, ChunkConfig}`、
  新增 `crate::kb::embed`、`super::refresh_wiki_for_doc`；
  `md5_short` / `process_document_async` 移出 super 引用（成为本地定义）
- handlers.rs 3238 → 3066 行；docs.rs 922 → 1094 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：chunks 域（kb_update_chunk / kb_reprocess_document）与
  search / versions / access 域按蓝图继续推进

## 切片 R-17：kb/handlers 拆分：docs 域（文档重命名/标签）（已完成）

- 107 行逐字搬移：`kb_rename_document` / `kb_set_doc_tags` /
  `kb_list_tags` 迁入 `kb/handlers/docs.rs`（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖仅 `retrieval::can_manage_kb/can_access_kb`（完全限定）+
  rusqlite/serde_json，docs.rs 现有导入直接满足
- handlers.rs 3066 → 2958 行；docs.rs 1094 → 1202 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：文档列表/详情/下载（kb_list_documents / kb_get_document /
  kb_download_document / kb_batch_download）作为下一小步

## 切片 R-18：kb/handlers 拆分：docs 域（文档列表/详情/下载）（已完成）

- 381 行逐字搬移：`kb_list_documents` / `kb_get_document` /
  `kb_download_document` / `kb_batch_download`（含 base64 单文件下载与
  zip 批量打包）迁入 `kb/handlers/docs.rs`（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖处理：docs.rs 新增 `rusqlite::params_from_iter` 与
  `super::log_metric_event`；base64/zip/chrono 在函数体内局部引用
- handlers.rs 2958 → 2576 行；docs.rs 1202 → 1584 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：kb_reprocess_document / kb_delete_document / kb_restore_version
  及 cleanup_orphan_file_objects / db_kb_id 归属收口

## 切片 R-19：kb/handlers 拆分：versions 域（版本列表/差异）（已完成）

- 新建 `kb/handlers/versions.rs`（193 行）：`VersionInfo` / `VersionDiff`
  结构 + `kb_list_versions` / `line_diff`（LCS + 大文档降级）/
  `kb_version_diff` 从 handlers.rs 逐字搬移（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖处理：versions.rs 补 `crate::kb::parse`（kb_version_diff 读取
  版本原文）；`retrieval::can_access_doc` 完全限定
- handlers.rs 2576 → 2391 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：search/RAG 域（kb_search / kb_rag / kb_rag_stream /
  kb_highlight）作为下一小步

## 切片 R-20：kb/handlers 拆分：search / RAG 域（已完成）

- 新建 `kb/handlers/search.rs`（499 行）：`SearchInput` 结构 +
  `kb_search` / `vector_search_wrap` / `kb_rag` / `persist_qa_exchange` /
  `kb_rag_stream` / `kb_highlight` 从 handlers.rs 逐字搬移
  （显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：search.rs 经 `super::{faq_match, log_metric_event,
  log_search, read_model_setting, resolve_embedding_pair}` 引用共享辅助；
  handlers.rs 移除已不再使用的 `rag` / `retrieval` 导入
  （`faq_match` 未解析曾引发级联 E0277，补导入后消除）
- handlers.rs 2391 → 1906 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：chunks 域（kb_update_chunk / kb_reprocess_document）作为下一小步

## 切片 R-21：kb/handlers 拆分：chunks 域（分块编辑/重处理）（已完成）

- 新建 `kb/handlers/chunks.rs`（281 行）：`kb_update_chunk`（分块编辑 +
  重新向量化）与 `kb_reprocess_document`（重处理流水线）从 handlers.rs
  逐字搬移（显式 LF 批量机械搬移，双块边界校验 + temp 备份）
- 依赖处理：chunks.rs 导入 `crate::kb::embed`、
  `parse::{self, Chunk, ChunkConfig}`（初版漏 `Chunk`，编译报 E0425，
  补导入消除）、`super::{log_metric_event, refresh_wiki_for_doc,
  resolve_embedding_pair}`
- handlers.rs 1906 → 1636 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：kb_delete_document → docs.rs、kb_restore_version → versions.rs

## 切片 R-22：kb/handlers 拆分：docs/versions 收口（删除文档 + 版本回滚）（已完成）

- 216 行逐字搬移：`kb_delete_document`（163 行）迁入
  `kb/handlers/docs.rs`；`kb_restore_version` + `db_kb_id`（唯一调用方）
  （179 行）迁入 `kb/handlers/versions.rs`（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖处理：docs.rs 直接复用现有 `super::cleanup_orphan_file_objects`；
  versions.rs 导入升级为 `parse::{self, Chunk, ChunkConfig}` + `embed` +
  `super::{refresh_wiki_for_doc, resolve_embedding_pair}`；
  handlers.rs 移除已不再使用的 `embed` / `parse` 导入（恢复 0 warnings）
- handlers.rs 1636 → 1383 行（首次跌破 1500）；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：access 域（kb_set_acl / kb_get_acl → access.rs）作为下一小步

## 切片 R-23：kb/handlers 拆分：access 域起步（ACL 规则）（已完成）

- 新建 `kb/handlers/access.rs`（119 行）：`AclInput` 结构 +
  `kb_set_acl` / `kb_get_acl` 从 handlers.rs 逐字搬移（显式 LF 批量
  机械搬移，双块边界校验 + temp 备份；`cleanup_orphan_file_objects`
  夹在中间，留在 handlers.rs）
- 依赖处理：access.rs 导入 `rusqlite::params_from_iter`；
  handlers.rs 移除已不再使用的 `params_from_iter`（恢复 0 warnings）
- handlers.rs 1381 → 1271 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：用户/成员/角色域（is_global_admin + kb_list_users …
  kb_update_member_role）迁入 access.rs

## 切片 R-24：kb/handlers 拆分：access 域（用户/成员/角色）（已完成）

- 402 行逐字搬移：`is_global_admin` + `UserItem` / `RoleItem` /
  `MemberItem` 结构与 `kb_list_users` / `kb_create_user` /
  `kb_change_password` / `kb_delete_user` / `kb_reset_password` /
  `kb_set_admin` / `kb_list_roles` / `kb_list_members` /
  `kb_add_member` / `kb_remove_member` / `kb_update_member_role`
  迁入 `kb/handlers/access.rs`（显式 LF 批量机械搬移，边界校验 +
  temp 备份）
- 依赖处理：access.rs 导入升级为 `serde::{Deserialize, Serialize}`；
  顺带移除 handlers.rs 中孤立无主的 `/// 从事件 detail…` 注释
- handlers.rs 1272 → 867 行；access.rs 119 → 522 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：检索历史域（SearchLogItem / log_search / kb_search_history）
  迁入 search.rs

## 切片 R-25：kb/handlers 拆分：search 域收口（检索历史）（已完成）

- 58 行逐字搬移：`SearchLogItem` 结构 + `log_search` /
  `kb_search_history` 迁入 `kb/handlers/search.rs`（显式 LF 批量机械搬移，
  边界校验 + temp 备份；该块位于 handlers.rs 文件末尾）
- 依赖处理：search.rs 导入升级为 `serde::{Deserialize, Serialize}`；
  `log_search` 移出 super 引用（成为本地定义）；handlers.rs 结束于
  `analytics_settings_map`（808 行）
- handlers.rs 867 → 808 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：FAQ 域（FaqEntryInput / kb_faq_* / faq_match）迁入 docs.rs

## 切片 R-26：kb/handlers 拆分：docs 域（FAQ 问答）（已完成）

- 125 行逐字搬移：`FaqEntryInput` + `kb_faq_import` / `kb_faq_list` /
  `kb_faq_delete` / `faq_match` 迁入 `kb/handlers/docs.rs`（显式 LF
  批量机械搬移，边界校验 + temp 备份）
- 依赖处理：`faq_match` 改 `pub(crate)`——docs.rs 经
  `pub(crate) use docs::*` glob re-export 进入 handlers 作用域，
  search.rs 现有 `super::faq_match` 引用零改动
- handlers.rs 808 → 682 行；docs.rs 1649 → 1775 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：知识库 CRUD（kb_create / kb_list / kb_delete / kb_update /
  ensure_system_kb + is_system_kb / kb_set_pin）迁入 access.rs

## 切片 R-27：kb/handlers 拆分：access 域（知识库 CRUD + 置顶）（已完成）

- 240 行逐字搬移：`KbSummary` + `kb_create` / `ensure_system_kb` /
  `kb_list` / `kb_delete` / `kb_update` + `is_system_kb` / `kb_set_pin`
  迁入 `kb/handlers/access.rs`（显式 LF 批量机械搬移，边界校验 +
  temp 备份）
- 依赖处理：access.rs 补 `super::cleanup_orphan_file_objects`
  （kb_delete 使用）；`ensure_system_kb` 经 glob re-export 保持
  lib.rs 启动注册路径不变；过程中一次导入索引错位覆盖了 serde 导入，
  编译报错后立即用 apply_patch 修复
- handlers.rs 682 → 441 行；access.rs 522 → 763 行；`lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：模型/设置域（kb_list_models + 模型解析辅助 + 模型/分块设置）
  迁入 settings.rs

## 切片 R-28：kb/handlers 拆分：settings 域（模型/分块设置）（已完成）

- 317 行逐字搬移：`KbModelInfo` / `ModelSetting` / `MODEL_ROLES` 与
  `kb_list_models` / 模型解析辅助（is_embedding_model 等 6 个）/
  `read_model_setting` / `resolve_embedding_pair` / `kb_get/set_model_settings` /
  `kb_get/set_chunk_settings` / `kb_get_default_model` /
  `kb_get_default_chat_model` 迁入新建 `kb/handlers/settings.rs`
  （显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：`read_model_setting` / `resolve_embedding_pair` 改
  `pub(crate)`（docs/chunks/search/versions/wiki 的 `super::` 引用
  经 glob re-export 保持零改动）；handlers.rs 移除不再使用的
  serde/State 导入
- handlers.rs 441 → 123 行（成为纯门面：模块汇总 + 共享设施）；
  `lib.rs` 注册点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
  （一次偶发 1 failed 为环境抖动，连续两次复跑均 211 passed 确认）
- 后续：R-蓝图-1 的 kb/handlers 按域拆分全部完成（handlers.rs 123 行，
  10 个子模块）；次级候选 wechat/http_api.rs、wechat/voice.rs 可沿用
  同一模式

## 蓝图 R-蓝图-2：wechat/http_api.rs 拆分设计（仅规划，未执行）

### 现状与边界

- `src-tauri/src/wechat/http_api.rs` 1814 行：本地 HTTP API 服务，
  覆盖服务状态/缓存/鉴权、自动化任务、数据查询（会话/消息/通讯录）、
  媒体处理、监控/SSE 推送、OpenAPI 文档与 3 个单元测试。
- 共享设施（留在门面）：`ApiServerState`（缓存/令牌/启停）、
  `ApiError`/`ApiResult`、`check_auth`、`cache_key`/`cached`/`store`、
  `parse_i64`/`parse_usize`/`parse_time`、`load_cfg`、
  `db_err`/`open_automation_conn`。
- 各域命令仅依赖共享设施 + 对应 wechat 数据模块（modules::*、media、
  image、automation::handlers 等），边界清晰。

### 目标布局（按域拆分，http_api.rs 保留为门面）

```
wechat/http_api/
  automation.rs  // tasks / claim / start / complete + open_automation_conn/db_err
  query.rs       // sessions / messages / contacts / group_members + map_message
  media.rs       // media / video / thumb / sns / emoticon / file_* + parse_range/valid_md5
  status.rs      // monitor_status / push_messages / parse_event_meta / openapi_json
http_api.rs      // 门面：状态/错误/鉴权/参数工具/路由/serve + mod 声明与 re-export
```

### 迁移策略

1. 每个新模块整体搬移若干命令 + 私有辅助（保留 `pub(crate)` 可见性，
   `pub(crate) use automation::*;` 汇总），`wechat/mod.rs` 零改动。
2. 每搬一个模块跑一次 `cargo fmt` + `cargo check`（0 warnings）+
   `cargo test --lib --no-default-features`（211 passed 基准）。
3. 优先搬移低耦合域（automation → status → query → media），
   共享设施留门面；每步机械搬移采用「显式 LF + 边界校验 + temp 备份」。

## 切片 R-29：wechat/http_api 拆分：automation 域（已完成）

- 新建 `wechat/http_api/automation.rs`（168 行）：`open_automation_conn` /
  `db_err` + `automation_tasks` / `automation_task_claim` /
  `automation_task_start` / `automation_task_complete` 从 http_api.rs
  逐字搬移（显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：automation.rs 经 `super::{check_auth, ApiError, ApiResult,
  ApiServerState}` 引用共享设施；4 个命令改 `pub(crate)` 以便
  `pub(crate) use automation::*` 汇总（初版漏改，路由内裸名解析失败，
  编译反馈后补齐）
- 教训：新建 `wechat/http_api/` 目录后才可写子模块文件（首次写入
  目录不存在失败，已从备份重建）
- http_api.rs 1814 → 1660 行；`wechat/mod.rs` 零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：status 域（monitor_status / push_messages / parse_event_meta /
  openapi_json）作为下一小步

## 切片 R-30：wechat/http_api 拆分：status 域（监控/推送/OpenAPI）（已完成）

- 新建 `wechat/http_api/status.rs`（145 行）：`monitor_status` /
  `push_messages`（SSE + Last-Event-ID 补推）/ `parse_event_meta` /
  `openapi_json` 从 http_api.rs 逐字搬移（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖处理：status.rs 导入 axum SSE 类型 / futures_util::StreamExt /
  Infallible 等；4 个命令改 `pub(crate)`（其中 parse_event_meta 与
  openapi_json 的脚本替换因 -like 匹配未生效，apply_patch 补齐）；
  http_api.rs 移除随之不再使用的 SSE/StreamExt/Infallible 导入
- http_api.rs 1660 → 1537 行；`wechat/mod.rs` 零改动；单测仍留在门面
  （经 glob re-export 引用 parse_event_meta）
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：query 域（health / get_sessions / get_messages / map_message /
  get_session_messages / get_contacts / get_group_members /
  query_group_members）作为下一小步

## 切片 R-31：wechat/http_api 拆分：query 域（数据查询）（已完成）

- 新建 `wechat/http_api/query.rs`（537 行）：`health` + `get_sessions` /
  `get_messages` / `map_message` / `get_session_messages` /
  `get_contacts` / `get_group_members` / `query_group_members` 从
  http_api.rs 逐字搬移（显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：query.rs 补 `cache_key` / `Path` 导入；单行签名的
  `health`/`map_message` 因 -like 全串匹配未命中可见性替换，
  apply_patch 补齐 pub(crate)；http_api.rs 移除不再使用的
  modules::{common,contacts,messages,sessions} 导入
- http_api.rs 1537 → 1017 行；`wechat/mod.rs` 零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：media 域（get_media / video / thumb / sns / emoticon /
  file_* + parse_range / valid_md5）作为最后一小步

## 切片 R-32：wechat/http_api 拆分：media 域（已完成）

- 新建 `wechat/http_api/media.rs`（547 行）：`get_media` /
  `get_media_video` / `get_media_video_thumb` / `get_sns_video` /
  `get_emoticon_image` / `get_file_image` / `get_file_video` /
  `get_file_video_thumb` + 私有辅助 `parse_range` / `valid_md5` 从
  http_api.rs 逐字搬移（显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：media.rs 导入 axum Body/Path/Query/State/header/Response；
  `crate::wechat::{image, voice}` 完全限定调用；http_api.rs 移除不再
  使用的 Body/Path/Query/State 导入
- http_api.rs 1017 → 486 行；`wechat/mod.rs` 零改动；单测留在门面
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 里程碑：R-蓝图-2 全部完成——http_api.rs 1814 → 486 行门面，
  automation/status/query/media 四个域模块共 1397 行

## 蓝图 R-蓝图-3：wechat/voice.rs 拆分设计（仅规划，未执行）

### 现状与边界

- `src-tauri/src/wechat/voice.rs` 1701 行，实际是两个独立子域：
  - 语音解码/缓存/转写（L1-228 的数据查询与解码 + L1252-1291 公共 API）
  - 视频/封面消息解析（L11-32 视频缓存 + L229-1251 全部解析逻辑）
- 交叉引用仅 1 处：视频域 `resource_video_hash` 调用
  `message_server_id`（语音域），经 `super::` 引用即可。
- 冒烟测试 4 个：语音 2 个（留在门面）、视频 2 个（随视频域迁移，
  因依赖视频域私有辅助 resolve_message_video_files_impl 等）。

### 目标布局

```
wechat/voice/
  video.rs   // VIDEO_PATH_CACHE / COVER_PATH_CACHE + VideoFiles +
             // 视频/封面解析（约 1000 行）+ 视频冒烟测试
voice.rs     // 门面：语音解码/缓存/转写 + 公共 API + 语音冒烟测试 +
             // mod video; pub(crate) use video::*;（外部调用点零改动）
```

### 迁移策略

1. 一次搬移：视频缓存 + 视频域函数 + 视频冒烟测试 → video.rs；
   voice.rs 保留语音域与语音测试，新增 `mod video; pub(crate) use
   video::*;` 汇总。
2. 回归：`cargo fmt --check` + `cargo check`（0 warnings）+
   `cargo test --lib --no-default-features`（211 passed 基准）。
3. 机械搬移采用「显式 LF + 边界校验 + temp 备份」。

## 切片 R-33：wechat/voice 拆分：video 子域（视频/封面解析）（已完成）

- 新建 `wechat/voice/video.rs`（1289 行）：`VIDEO_PATH_CACHE` /
  `COVER_PATH_CACHE` + `VideoFiles` 与全部视频/封面解析逻辑
  （L229-1251）+ 视频冒烟测试 2 个（smoke_video_resolve /
  smoke_video_cover_and_resource，依赖视频域私有辅助故随域迁移）
- voice.rs 保留语音解码/缓存/转写 + 公共 API + 语音冒烟测试 2 个，
  新增 `mod video; pub(crate) use video::*;`；`message_server_id`
  经 `super::` 供 video.rs 引用；移除不再使用的 SystemTime 导入
- 事故：首次脚本目标目录不存在导致 video.rs 写入失败（voice.rs 已写
  入），且段范围 `$a[0..10]` 含双端导致两行视频注释混入门面——已从
  备份重建并清除两行杂散注释
- voice.rs 1701 → 432 行；`wechat/mod.rs` 与外部调用点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 里程碑：R-蓝图-3 完成；次级候选剩余 wechat/ask.rs（2130 行）、
  auto_key.rs（2460 行）、monitor.rs（1742 行）、image.rs（1652 行）

## 蓝图 R-蓝图-4：wechat/auto_key.rs 拆分设计（仅规划，未执行）

### 现状与边界

- `src-tauri/src/wechat/auto_key.rs` 2460 行（当前最大 wechat 模块），
  按「密钥获取手段」分为 6 个低耦合域：
  - PE 静态定位（L28-321）：段表解析 / key-set RVA / 微信安装路径
  - HMAC 预言机（L323-400）：master key 校验
  - Rust 调试器（L401-903，嵌套 `mod debugger`，约 500 行）
  - 进度事件 + wx_key.dll FFI（L904-1142）
  - DB 密钥自动获取（L1143-1655）
  - 图片密钥派生与获取（L1656-2015）
- 顶层常量（L23-26）与一键全自动/测试留在门面。

### 目标布局

```
wechat/auto_key/
  pe.rs        // PE 静态定位（已完成 R-34）
  oracle.rs    // HMAC 预言机
  debugger.rs  // Rust 调试器（mod 包装去缩进迁移）
  ffi.rs       // 进度事件 + wx_key.dll FFI
  dbkey.rs     // DB 密钥自动获取
  imagekey.rs  // 图片密钥
auto_key.rs    // 门面：常量 + 一键全自动 + 测试 + mod 汇总
```

### 迁移策略

1. 每域整体搬移（显式 LF + 边界校验 + temp 备份），公开项经
   `pub(crate) use x::*;` 汇总，调用点零改动；debugger 采用
   「剥离 mod 包装 + 整体去 4 空格缩进」机械转换。
2. 每步回归：`cargo fmt --check` + `cargo check`（0 warnings）+
   `cargo test --lib --no-default-features`（211 passed）。

## 切片 R-34：wechat/auto_key 拆分：pe 域（PE 静态定位）（已完成）

- 新建 `wechat/auto_key/pe.rs`（300 行）：`PeSection` / `PeInfo` /
  `parse_pe` / `rva_to_file_offset` / `find_keyset_function_rvas` /
  `locate_weixin_dll` / `locate_weixin_exe` 从 auto_key.rs 逐字搬移
  （显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：块内全部完全限定（std::path/std::fs/windows 局部 use），
  零新增导入；auto_key.rs 新增 `mod pe; pub(crate) use pe::*;`
- auto_key.rs 2460 → 2165 行；外部调用点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：oracle（HMAC）→ ffi → imagekey → dbkey → debugger

## 切片 R-35：wechat/auto_key 拆分：oracle 域（HMAC 预言机）（已完成）

- 新建 `wechat/auto_key/oracle.rs`（83 行）：`is_valid_master_key` /
  `hmac_check` / `read_db_page1_shared`（windows + 非 windows 双 cfg
  实现）从 auto_key.rs 逐字搬移（显式 LF 批量机械搬移，边界校验 +
  temp 备份）
- 依赖处理：三个函数改 `pub(crate)`（debugger/dbkey/测试经
  `pub(crate) use oracle::*` + `use super::*` 引用，初版漏改导致
  E0425，编译反馈后补齐）；crypto 完全限定、局部 use 自包含
- auto_key.rs 2165 → 2090 行；外部调用点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：ffi 域（进度事件 + wx_key.dll FFI）

## 切片 R-36：wechat/auto_key 拆分：ffi 域（进度事件 + wx_key.dll FFI）（已完成）

- 新建 `wechat/auto_key/ffi.rs`（246 行）：`emit_progress` +
  `WxKeyDll`（7 个导出函数绑定 + DLL 单例）+ `locate_wx_key_dll` /
  `get_dll` / `find_wechat_pids` 从 auto_key.rs 逐字搬移（显式 LF
  批量机械搬移，边界校验 + temp 备份）
- 依赖处理：`emit_progress` / `get_dll` 改 `pub(crate)`；WxKeyDll 字段
  与 `load` / `last_error_string` 改 `pub(crate)`（门面内 dbkey/
  imagekey/测试直接访问）；ffi.rs 补齐 c_char/c_int/CStr/Path 导入，
  auto_key.rs 移除随之未用的 c_char/Mutex/OnceLock 导入
- auto_key.rs 2090 → 1853 行；外部调用点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：imagekey 域（图片密钥获取与派生）→ dbkey → debugger

## 切片 R-37：wechat/auto_key 拆分：imagekey 域（图片密钥）（已完成）

- 新建 `wechat/auto_key/imagekey.rs`（372 行）：`ImageKeyResponse` /
  `ImageKeyAccount` / `ImageKeyItem` + `auto_get_image_key` /
  `auto_get_image_key_windows` / `resolve_scan_root` /
  `collect_wxid_candidates` / `clean_wxid` / `derive_image_keys` /
  `verify_derived_aes_key` / `find_template_data` /
  `collect_template_files` 从 auto_key.rs 逐字搬移（显式 LF 批量
  机械搬移，边界校验 + temp 备份）
- 依赖处理：测试引用的 3 个私有函数与 3 个响应结构体（含字段）改
  `pub(crate)`（-like 中 `[u8]` 被当作字符类通配符导致一次替换未命中，
  apply_patch 补齐）；imagekey.rs 补 c_int/CStr/HashMap/IMAGE_KEY_BUF
  导入；auto_key.rs 移除 image/Md5/Digest/Deserialize/HashMap 导入
- auto_key.rs 1853 → 1494 行；外部调用点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：dbkey 域（数据库密钥自动获取）→ debugger 域收口

## 切片 R-38：wechat/auto_key 拆分：dbkey 域（数据库密钥）（已完成）

- 新建 `wechat/auto_key/dbkey.rs`（528 行）：`auto_get_db_key` /
  `auto_get_db_key_v2` / `auto_get_db_key_debugger` /
  `find_message_0_db` / `kill_wechat_processes` / `relaunch_wechat` /
  `auto_get_db_key_hook_main` / `find_wechat_main_process` /
  `process_has_module` / `find_main_wechat_pid` / `poll_db_key` /
  `finish_db_key` 从 auto_key.rs 逐字搬移（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖处理：dbkey.rs 经 `super::{debugger, pe, oracle, ffi 项, 常量}`
  引用；`find_main_wechat_pid` 改 `pub(crate)`（测试引用，-like 全串
  匹配未命中，apply_patch 补齐）；门面移除 c_int/CStr/Path/PathBuf/
  Duration/Instant 导入，测试模块内补自身导入
- auto_key.rs 1492 → 977 行；外部调用点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：debugger 嵌套 mod 去缩进迁移为独立文件，完成 R-蓝图-4

## 切片 R-39：wechat/auto_key 拆分：debugger 域（Rust 调试器收口）（已完成）

- 新建 `wechat/auto_key/debugger.rs`（502 行）：嵌套
  `mod debugger { ... }`（约 500 行，DEBUG_PROCESS 提取 master key）
  剥离 mod 包装 + 整体去 4 空格缩进迁移（缩进审计先行：体每行空或
  ≥4 空格；显式 LF + 边界校验 + temp 备份）
- auto_key.rs 侧以 `#[cfg(target_os = "windows")] mod debugger;`
  声明文件模块；dbkey.rs / 测试的 `debugger::WeChatDebugger` 引用零改动
- auto_key.rs 979 → 481 行；外部调用点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 里程碑：R-蓝图-4 全部完成——auto_key.rs 2460 → 481 行门面，
  pe/oracle/ffi/imagekey/dbkey/debugger 六个域模块共 2031 行

## 蓝图 R-蓝图-5：wechat/ask.rs 拆分设计（仅规划，未执行）

### 现状与边界

- `src-tauri/src/wechat/ask.rs` 2130 行（当前最大 wechat 模块），
  「问我的微信」问答流水线按阶段分为：
  - 数据结构（L22-153）：AskPlan / AggregationSpec / StatsTable /
    AskHistoryItem / Citation（留在门面）
  - LLM 提供方解析（L155-195，llm_provider / parse_json_object，
    仅 LLM 段使用）
  - 启发式规划（L197-711，自包含；search 段仅引用
    is_group_activity_question / retrieve_recent_group_sessions）
  - 检索执行 + 统计聚合（L712-1503）
  - LLM 规划/反思/回答（L1504-1836）
  - IPC 命令 ask_wechat（L1842-1973，留在门面）+ 测试
- 跨段耦合低：search→plan 仅 2 处；llm→plan 仅 heuristic_plan。

### 目标布局

```
wechat/ask/
  plan.rs    // 启发式规划（L197-711）
  search.rs  // 检索执行 + 统计聚合（L712-1503）
  llm.rs     // LLM 提供方解析 + 规划/反思/回答（L155-195 + L1504-1836）
ask.rs       // 门面：数据结构 + ask_wechat + 测试 + mod 汇总
```

### 迁移策略

1. 每域整体搬移（显式 LF + 边界校验 + temp 备份），跨域所需项改
   `pub(crate)` 并经 `pub(crate) use x::*;` 汇总，调用点零改动。
2. 每步回归：`cargo fmt --check` + `cargo check`（0 warnings）+
   `cargo test --lib --no-default-features`（211 passed）。

## 切片 R-40：wechat/ask 拆分：plan 域（启发式规划）（已完成）

- 新建 `wechat/ask/plan.rs`（514 行）：`STOPWORDS` / `tokenize` /
  `extract_keywords` / `is_cjk` / `is_group_activity_question` /
  `retrieve_recent_group_sessions` / `detect_sources` / 时间解析 /
  `resolve_target` / `heuristic_plan` / `heuristic_aggregation` /
  `is_timeish_keyword` 从 ask.rs 逐字搬移（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖处理：6 个跨域函数（heuristic_plan / extract_keywords / is_cjk /
  is_group_activity_question / retrieve_recent_group_sessions /
  date_to_epoch）改 `pub(crate)`；plan.rs 补 chrono/HashSet/Path/
  modules::sessions/super 数据结构导入
- ask.rs 2130 → 1616 行；外部调用点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：search 域（检索执行 + 统计聚合）

## 切片 R-41：wechat/ask 拆分：search 域（检索执行 + 统计聚合）（已完成）

- 新建 `wechat/ask/search.rs`（809 行）：`name_matches` /
  `non_type_keyword` / `resolve_peer_usernames` / `record_matches` /
  `fmt_ts` / `truncate` / `execute_plan` + `agg_*` 全部统计函数 /
  `execute_aggregation` 从 ask.rs 逐字搬移（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖处理：6 个跨域函数（execute_plan / execute_aggregation /
  non_type_keyword / record_matches / fmt_ts / truncate）改
  `pub(crate)`（fmt_ts/truncate 供 plan.rs 经 super 引用）；
  一次 -like 替换截断了 execute_aggregation 签名（编译报未闭合
  定界符，apply_patch 修复）；search.rs 补 common/sessions 导入、
  移除 HashMap；ask.rs 移除 modules/rusqlite/Path 未用导入
- ask.rs 1617 → 825 行；外部调用点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：llm 域（提供方解析 + 规划/反思/回答）收口 R-蓝图-5

## 切片 R-42：wechat/ask 拆分：llm 域（规划/反思/回答）（已完成）

- 新建 `wechat/ask/llm.rs`（383 行）：`llm_provider` /
  `parse_json_object` + 三个系统提示词 / `chat_messages` /
  `format_history` / `resolve_plan` / `ReflectResult` /
  `reflect_evidence` / `merge_citations` / `generate_answer` 从
  ask.rs 逐字搬移（显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：4 个跨域函数（resolve_plan / reflect_evidence /
  generate_answer / merge_citations）改 `pub(crate)`；ReflectResult
  结构与字段改 `pub(crate)`（ipc 直接访问）；llm.rs 补
  HashSet/truncate 导入；ask.rs 移除未用 ChatMessage 导入
- ask.rs 823 → 448 行；外部调用点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 里程碑：R-蓝图-5 全部完成——ask.rs 2130 → 448 行门面，
  plan/search/llm 三个域模块共 1719 行

## 切片 R-43：wechat/image.rs 移除冗余模块级 allow(dead_code)（已完成）

- 审计：临时移除 `#![allow(dead_code)]` 后 `cargo check` 0 warnings，
  证明模块级 allow 完全冗余（真死项 is_v2_format 与 ImageResult 的
  format/md5/error 字段已由 4 处定向 `#[allow(dead_code)]` 覆盖）
- 最终改动：删除 image.rs 第 1 行模块级 allow（不触碰定向 allow）
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：对 monitor.rs / media.rs / annual.rs / sns_image.rs /
  modules/*.rs 的模块级 allow 做同样审计（先验证再删，不盲目删除）

## 切片 R-44：wechat/monitor.rs 移除死代码与冗余 allow(dead_code)（已完成）

- 审计：临时移除模块级 `#![allow(dead_code)]` 暴露 2 个真死项：
  - `decode_wechat_text`（L103-109）——与
    `modules::common::decode_wechat_text` 逐字节重复的本地副本
    （死代码 + 重复实现，删除后无行为变化）
  - `query_messages_in_window`（L773-875）——已被
    `query_messages_since_watermark` 取代的遗留方法
- 最终改动：删除上述 2 项 + 模块级 allow；顺带清理
  query_messages_since_watermark 文档中对已删方法的引用
- monitor.rs 1742 → 1628 行；0 warnings；211 passed
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：继续审计 media.rs / annual.rs / sns_image.rs / modules/*.rs

## 切片 R-45：wechat 全模块冗余 allow(dead_code) 清理（已完成）

- 审计并删除 15 个文件的冗余模块级 `#![allow(dead_code)]`：
  annual.rs / media.rs / sns_image.rs / modules/mod.rs 与
  modules/{avatar,common,contacts,emoticons,favorites,files,messages,
  moments,official,sessions,settings}.rs（逐个临时禁用验证 0 warnings
  后才删除）
- 顺带移除审计暴露的真死代码：
  - contacts.rs `local_type_label`（4 处“调用”实为结构体字段名与
    字符串键，函数零调用）
  - annual.rs `ts_to_sec`（零引用）
- 全仓模块级 allow 清零；仅保留 34 处定向 item-level allow
  （如 ImageResult 字段等仍属有意保留的 pub API 面）
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：可对 34 处 item-level allow 逐项核验（移除/保留判断）；
  或回到大文件拆分（monitor.rs / image.rs 高内聚评估）

## 切片 T-68b（R-46）：前端事件监听与 WeChatPanel 状态精准类型化（已完成）

- `wechat/types.ts` 新增 `WeChatOpProgress`（op/done/total/percent/
  message）与 `SttDownloadProgress`（filename/done/total/percent/
  finished）事件 payload 接口
- 4 处 `listen<any>` → 精准类型：wechat-op-progress ×2、
  stt-download-progress、automation://message（`LiveMessage`）
- WeChatPanel 5 处 any → `WeChatSession | null`（curSessionInfo）、
  `RichMedia | null`（miniappDetail + `?? null` 归一化）、
  `Promise<never>`（超时竞速 ×2）、`Promise<MessagePage>`
  （loadLatestMessages）
- 严格类型化暴露并修复 4 处既有类型缺陷：`getSessionList` /
  `refreshWechatSessions` 返回类型从 `SessionEntry[]` 修正为
  `WeChatSession[]`（消除 2 处竞速赋值错误）；`MessagePage` 补后端
  实际字段 `page/page_size/chat_name/self_username`
- 回归：svelte-check 0 errors / 176 warnings；38 冒烟/单元测试通过；
  `npm run build` 通过
- 后续：DbManager（8 处 state any）与 db/services/ipc.ts（12 处
  invoke<any>）作为下一片

## 切片 T-69（R-47）：DB 管理工具精准类型化（已完成）

- `db/types.ts` 新增 11 个接口：`DbAppDatabase` / `DbInfo` /
  `DbTableDetail`（+ `DbIndexInfo` / `DbTriggerInfo`）/
  `DbTableStats`（+ `DbColumnStat`）/ `DbIntegrityResult` /
  `DbSqlResult`（query/write 判别联合）/ `DbExportResult` /
  `DbBackupResult` / `DbRestoreResult` / `DbHeaderInfo` /
  `DbCleanupResult`——全部对照 Rust 后端 JSON 形状逐字段定义
- `db/services/ipc.ts` 12 处 `invoke<any>` → 精准返回类型；
  顺带修复 `getDbInfo` 过窄类型（`{path}` → `DbInfo` 全字段）
- `DbManager.svelte` 8 处 `$state<any>` → 联合类型
  （成功形态 | `{ error: string }` | null），模板用
  `'error' in x` 收窄；修复 compareResult `changed` 可选类型、
  L1709 `title={String(row[c] ?? '')}`（unknown → string）
- 严格类型化暴露 20+ 处模板缺少联合收窄的真实类型缺陷，全部修复
- 回归：svelte-check 0 errors / 176 warnings；38 冒烟测试通过；
  `npm run build` 通过

## 切片 T-70（R-48）：全前端非 catch any 清零（已完成）

- `search/services/ipc.ts` 最后一处 `Promise<any>` →
  `Promise<SearchIndexStatus>`（复用 wechat/types.ts 既有接口，
  消除跨模块重复封装）
- 前端非 catch `any` 计数：33 → 0（事件监听 / WeChatPanel 状态 /
  DB 工具 / 搜索服务全部类型化）
- 回归：svelte-check 0 errors / 176 warnings；38 冒烟测试通过；
  `npm run build` 通过

## 切片 T-71（R-49）：errText 下沉共享层 + 全前端 catch-any 清零（已完成）

- `errText` 从 `wechat/utils/format.ts` 下沉至共享 `lib/format.ts`
  （通用错误文本提取，跨 kb/llm/automation/search 复用无需
  依赖 wechat 模块）；wechat/utils/format.ts 改为 re-export
- 全前端 172 处 `catch (e: any)` → `catch (e: unknown)`：
  - `e?.message ?? e` / `e?.message ?? String(e)` 等正文模式统一
    收敛为 `errText(e)`（含 `?? '默认'` → `|| '默认'` 语义修正）
  - `'...' + e` / 模板 `${e}` 在 unknown 下合法，仅改 catch 参数
  - 保留非 `e` 命名（如 `catch (err: unknown)`）避免正文引用断裂
- 适配受影响的 2 个独立编译测试：smoke-format-utils 改为 esbuild
  bundle 解析 `../../format`；run-store-test 加 onResolve 插件指向
  共享 lib/format.ts
- 里程碑：前端显式 `any` 全面清零——非 catch any 0 处 +
  catch-any 0 处；svelte-check 0 errors / 176 warnings；
  38 冒烟/单元测试通过；`npm run build` 通过

## 切片 R-50：Rust 全量 item-level allow(dead_code) 审计（已完成）

- 34 处 item-level `#[allow(dead_code)]` 逐文件批量禁用 → cargo check
  暴露真死项 → 逐项决策：
  - 24 处冗余 allow（crypto×6 / image×4 / db_cache×3 / config×4 /
    keys×5 / media×1 / watermark×1）：移除后 0 warnings，直接删除
  - 移除 5 个真死项：automation/db.rs `get_rule`、
    bot/channel.rs `CHANNEL_WECHAT_LOCAL`、
    handlers/helpers.rs `refresh_decrypt_lock`（静态锁仍被直接使用）、
    ws_server.rs `ProtocolMessage::error_response`/`heartbeat` +
    `WsServer::set_port`/`stop`/`broadcast`（lib.rs 的 broadcast 实为
    tokio 广播，非该方法）
  - 移除 db_cache.rs 只写不读字段 `wal_len`（含两处写入）
  - 保留 3 处注释说明的合理 allow：monitor.rs `image_resolver`
    （文档化"保留以备按需解码"）、listener.rs `watcher`（RAII
    生命周期持有）/`watched`（目录去重设计）
- 全仓 item-level allow(dead_code)：34 → 3
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed；
  审计临时备份全部清理

## 切片 R-51：wechat/image 拆分：crypto 加密原语层（已完成）

- 新建 `wechat/image/crypto.rs`（371 行）：V1/V2/XOR 格式常量、
  MD5 解析缓存（cached_md5/store_md5）、AES/XOR 解密核心
  （aes128_ecb_decrypt / aes_ecb_decrypt_file / decode_cdn_aes_key /
  decrypt_dat_file / decrypt_v2 / decrypt_xor）、格式检测
  （detect_image_format / detect_xor_key / is_v2_format）与
  resolve_out_path 从 image.rs 逐字搬移（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖处理：crypto.rs 导入 aes/std 全套；cached_md5 / store_md5 /
  aligned_aes_block_size 改 `pub(crate)`（facade 与测试引用）；
  is_v2_format 补回定向 allow（私有子模块中 pub 项触发 dead_code，
  测试工具保留）；测试补 KeyInit 导入（原先依赖顶层导入透传）；
  image.rs 移除随迁未用的 aes/HashMap/Mutex/OnceLock/Duration 导入
- image.rs 1647 → 1288 行；外部调用点（http_api/media.rs、
  auto_key/imagekey.rs 经 image:: 路径）零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：image.rs 解析层（L550-1319）可再评估独立成 resolve.rs；
  或转前端 WeChatPanel 组件拆分

## 切片 R-52：wechat/image 拆分：resolve 独立解析层（已完成）

- 新建 `wechat/image/resolve.rs`（780 行）：独立解析函数簇
  （attach_dir_name / get_image_md5_from_db / _from_msg_tables /
  _with_fallback / find_dat_files / select_best_dat / select_hd_dat /
  pick_dat / decode_dat_to_data_url / resolve_message_image_data_url /
  _live / resolve_message_image_bytes / _live / CDN 兜底 6 个私有辅助 /
  mime_of）从 image.rs 逐字搬移（显式 LF 批量机械搬移，边界校验 +
  temp 备份）
- 依赖处理：resolve.rs 经 `use super::*` 引用 crypto 层
  （cached_md5/store_md5/decrypt_dat_file/detect_image_format）；
  补 `MonitorDBCache` 与 `Path/PathBuf` 导入；cdn_image/hevc 完全限定；
  image.rs 移除随迁未用的 `Path` 导入
- image.rs 1287 → 517 行；外部调用点（http_api/media.rs）零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 里程碑：image.rs 1647 → 517 行门面 + crypto 372 / resolve 780
  两个域模块，三层职责（加密原语 / 解析定位 / 门面编排）清晰分离

## 切片 T-72（R-53）：WeChatPanel 纯函数下沉 panel.ts（已完成）

- 新建 `wechat/utils/panel.ts`：5 个纯函数下沉
  - `trimRecord`（Record 裁剪，就地删除最先键）
  - `calHeat`（日历热力色线性映射）
  - `cmpTid`（朋友圈 tid 大数比较，负数 tid 新→旧排序，含非数值
    兜底；附原注释防误用）
  - `editKey`（会话已编辑消息去重键）
  - `sessionMatchesKeyword`（会话名称/username 子串匹配，
    kefuSearchMatch 由闭包状态改为参数化纯函数）
- WeChatPanel 删除 4 个本地函数定义 + kefuSearchMatch 改为调用纯函数
- 新增 `smoke-panel-utils.mjs`（24 项断言，含负数 tid 排序方向验证）
- 回归：svelte-check 0 errors / 176 warnings；39 冒烟/单元测试通过
  （38 + 新增 1）；`npm run build` 通过

## 切片 T-73（R-54）：wechat/media 拆分 transfer 转账域（已完成）

- 新建 `wechat/media/transfer.rs`（162 行）：转账状态标签段
  （TRANSFER_PAYSUBTYPE_LABEL / transfer_label / clean_amount /
  transfer_status_label / is_transfer_status_type）与转账解析段
  （extract_transfer_info / extract_wcpayinfo）从 media.rs 逐字搬移
  （显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：transfer.rs 经 `super::collapse_text` 引用父模块私有
  XML 辅助（Rust 子模块可访问父模块私有项）；transfer_label /
  clean_amount / extract_wcpayinfo 改 `pub(crate)`（富媒体段的红包/
  小程序解析复用）；extract_transfer_info 补定向 allow（仅测试使用
  的 pub API）；media.rs 移除 quick_xml 导入随迁项
- media.rs 1237 → 1087 行；外部调用点（messages.rs 经
  `media::transfer_status_label` 等）零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 附带：AGENTS.md 测试清单补入 smoke-panel-utils.mjs（文档同步）
- 后续：媒体解析域（RichMedia 类型 + 富媒体/mmreader 解析）与
  XML 辅助层可再拆分；或前端 WeChatPanel 组件级拆分

## 切片 T-74（R-55）：跨模块重复工具函数收敛（已完成）

- 扫描定位 3 组逐字节一致的重复实现，统一收敛到共享基础设施
  `wechat/modules/common.rs`：
  - `now_ms`（Unix 毫秒）——daily_summary.rs 与 edit_store.rs 各一份
  - `ts_expr`（消息表毫秒→秒 SQL 表达式）——annual.rs 与
    daily_summary.rs 各一份
  - `month_of`（时间戳 → YYYY-MM 目录名）——file.rs 与
    voice/video.rs 各一份
- 6 处本地定义删除，调用点改为 `common::` / 完全限定路径
  （daily_summary 4+3、edit_store 1、annual 8、file 1、video 1）
- edit_store.rs 补 common 导入；顺带排查 xml_tag_text 等疑似重复
  （moments.rs 为 `xml_tag_text_loose` 带属性变体，非重复，保留）
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 事故：PowerShell 引号拼接导致含 `&'static` 的 ts_expr 签名未匹配
  删除、替换污染定义行为 `fn common::ts_expr()`——已定位并清理

## 切片 T-75（R-56）：wechat/media 拆分 rich 富媒体域 + xml 工具层（已完成）

- 新建 `wechat/media/xml.rs`（71 行）：轻量 XML 工具层
  （collapse_text / clean_cdata / get_tag_text / get_tag_int /
  find_attr / extract_nested / parse_nested_int，全部 `pub(crate)`）
- 新建 `wechat/media/rich.rs`（607 行）：`ChatLogItem` /
  `NewsFeedItem` / `RichMedia` 枚举 + `parse_rich_content` /
  `parse_contact|location|emoji|appmsg|video|voice` / `parse_mmreader` /
  `extract_appmsg_sub_articles` / `parse_recorditem`
- 依赖处理：rich.rs 导入 quick_xml/Serialize 并经 `use super::*`
  引用 xml/transfer 助手；6 个 parse_* 改 `pub(crate)`（facade 测试
  经 re-export 访问）；media.rs 移除随迁未用的 quick_xml/Serialize
- media.rs 1088 → 425 行（仅剩测试与 mod 汇总）；外部调用点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 里程碑：media.rs 1237 → 425 行门面 + transfer 162 / xml 71 /
  rich 607 三个域模块，富媒体解析职责分层完成

## 切片 T-76（R-57）：WeChatPanel 朋友圈合并逻辑下沉 mergeMoments（已完成）

- `wechat/utils/panel.ts` 新增 `mergeMoments(existing, incoming)`：
  朋友圈增量合并纯函数——已存在条目按 tid 更新、新条目置顶、
  tid 降序（复用 cmpTid 负数排序语义）；返回 `{ items, fresh }`
  （fresh 为原始序的新条目，供调用方做预载头像/提示副作用）
- WeChatPanel `refreshMomentsAuto` 的非重置分支改用该函数：
  组件仅保留 UI 副作用（total/message/预载头像），合并/去重/排序
  逻辑完全下沉；`cmpTid` 随之下沉后从组件导入移除
- smoke-panel-utils.mjs 新增 5 组 mergeMoments 断言（更新/去重/
  负数 tid 排序/空列表）
- 回归：svelte-check 0 errors / 176 warnings；39 冒烟/单元测试通过；
  `npm run build` 通过

## 切片 T-77（R-58）：WeChatPanel 通讯录分组逻辑下沉（已完成）

- `wechat/utils/panel.ts` 新增 `groupContactsByInitial(contacts)`：
  通讯录按拼音首字母分组——无首字母归 '#' 组并置底，其余按
  localeCompare 排序；返回 `[string, ContactItem[]][]`
- WeChatPanel `groupedContacts` 由内联 `$derived.by` 改为
  `$derived(groupContactsByInitial(filteredContacts))`（11 行 → 1 行）
- smoke-panel-utils.mjs 新增 3 组断言（分组顺序、同组聚合、空列表）
- 回归：svelte-check 0 errors / 176 warnings；39 冒烟/单元测试通过；
  `npm run build` 通过

## 切片 T-78（R-59）：WeChatPanel 会话列表过滤逻辑下沉（已完成）

- `wechat/utils/panel.ts` 新增两个纯函数：
  - `sessionKeywordMatch(s, q)`——名称/摘要/username 子串匹配
    （消除 filteredSessions 与 pinnedSessions 两处重复的关键词逻辑）
  - `filterMainSessions(sessions, q)`——排除公众号/客服后按关键词
    过滤（复用 misc.ts 的 isKefuSession/isMiniAppKefuSession）
- WeChatPanel：`filteredSessions` 由 8 行 `$derived.by` 改为
  `$derived(filterMainSessions(sessions, searchText))`；
  `pinnedSessions` 由 7 行改为 `allPinnedSessions.filter(s =>
  sessionKeywordMatch(s, searchText))`
- smoke-panel-utils.mjs：改为 esbuild bundle 解析 panel.ts 对
  ./misc 的运行时依赖；新增 5 组断言（摘要命中、公众号/客服排除、
  空关键词语义）
- 回归：svelte-check 0 errors / 176 warnings；39 冒烟/单元测试通过；
  `npm run build` 通过

## 切片 T-79（R-60）：WeChatPanel 图片体检过滤/排序下沉（已完成）

- `wechat/utils/panel.ts` 新增 `CheckupChat` 类型 + `CheckupSort` 联合
  与 `filterSortCheckupChats(chats, { q, onlyMissing, sort })`：
  关键词（名称/username）/仅缺失过滤 + 三路排序（缺失降序·并列按
  总量 / 总量降序 / 名称 zh localeCompare）
- WeChatPanel `checkupChats` 由 23 行内联 `$derived.by` 改为一行
  `$derived(filterSortCheckupChats(...))`
- smoke-panel-utils.mjs 新增 5 组断言（三路排序、仅缺失过滤、
  关键词过滤、空列表）
- 回归：svelte-check 0 errors / 176 warnings；39 冒烟/单元测试通过；
  `npm run build` 通过

## 切片 T-80（R-61）：WeChatPanel 收藏过滤逻辑下沉（已完成）

- `wechat/utils/panel.ts` 新增 `filterFavoriteItems(items, { type, q })`：
  收藏按类型（'all' 或 type_label）与关键词（标题/描述/来源）过滤
- WeChatPanel `filteredFavItems` 由 12 行内联 `$derived.by` 改为
  `$derived.by(() => filterFavoriteItems(favData.items ?? [], ...))`
  ——因 favData 声明在派生之后，保持惰性求值语义（初版直呼形式触发
  TDZ 报错，改回 $derived.by 闭包）
- smoke-panel-utils.mjs 新增 4 组断言（类型过滤、标题/来源关键词、
  空过滤全量）
- 回归：svelte-check 0 errors / 176 warnings；39 冒烟/单元测试通过；
  `npm run build` 通过

## 切片 T-81（R-62）：wechat/monitor 拆分 util 基础设施层（已完成）

- 新建 `wechat/monitor/util.rs`（269 行）：消息类型工具
  （MEDIA_TYPE_MAP / media_type / format_msg_type）、数据库连接
  （connect_db / load_name2id）、联系人/DB 映射（load_contact_names /
  build_username_db_map / db_mtime / file_mtime_ms）、一致性快照暂存
  （files_equal / stage_stable_copy / stage_full_snapshot /
  cleanup_staging）从 monitor.rs 逐字搬移（显式 LF 批量机械搬移，
  边界校验 + temp 备份）
- 依赖处理：9 个私有函数改 `pub(crate)`；monitor.rs 顶部
  `mod util; pub(crate) use util::*;` 使 SessionMonitor 段调用点零改动
  （裸名经 glob 解析）；db_cache 的
  `crate::wechat::monitor::stage_stable_copy` 外部调用保持；util.rs
  补 MonitorDBCache 导入、移除未用的 PathBuf/ContactMap
- monitor.rs 1625 → 1367 行；外部调用点零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed

## 切片 T-82（R-63）：wechat/daily_summary 拆分 crud 持久化域（已完成）

- 新建 `wechat/daily_summary/crud.rs`（405 行）：任务 CRUD
  （SummaryTask / row_to_task / list_tasks / get_task / save_task /
  delete_task / toggle_task / update_task_run_state）与记录 CRUD
  （SummaryRecord / row_to_record / list_records / repair_group_names /
  resolve_group_name / delete_record / insert_record）从 daily_summary.rs
  逐字搬移（显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：crud.rs 经 `super::connect` 引用门面数据库连接、
  补 `common` 导入（now_ms）；`super::annual` 路径语义变化
  （crud 的 super 是 daily_summary 而非 wechat）修正为
  `crate::wechat::annual`；移除未用的 ensure_column 与门面 Serialize
- daily_summary.rs 1249 → 854 行；执行流水线经 glob re-export
  裸名调用零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed

## 切片 T-83（R-64）：wechat/daily_summary 拆分 retrieve 数据检索层（已完成）

- 新建 `wechat/daily_summary/retrieve.rs`（237 行）：群成员读取
  （get_group_members）与当日消息提取/计数（DayMessage /
  fetch_day_messages / count_group_messages）从 daily_summary.rs
  逐字搬移（显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：retrieve.rs 自包含（common 完全限定 + OptionalExtension
  导入），无门面依赖；fetch_day_messages / count_group_messages /
  DayMessage（含字段）改 `pub(crate)` 供执行段经 glob re-export
  访问；门面移除随迁未用的 OptionalExtension
- daily_summary.rs 854 → 626 行；执行流水线裸名调用零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed

## 切片 T-84（R-65）：wechat/insights 拆分 progress 事件发射域（已完成）

- 新建 `wechat/insights/progress.rs`（150 行）：`emit_progress` /
  `GraphEmitCtx` / `emit_graph_chunk` / `emit_days_chunk` /
  `emit_graph_final`（Tauri 事件进度/分块/完成发射）从 insights.rs
  逐字搬移（显式 LF 批量机械搬移，边界校验 + temp 备份）
- 依赖处理：progress.rs 经 `super::CountMap` 引用类型别名、
  `use tauri::Emitter` 在函数体内局部使用；5 个项与 GraphEmitCtx
  字段改 `pub(crate)`（facade 构建/调用经 glob re-export 访问）
- insights.rs 1204 → 1062 行；构建逻辑（collect_msg_counts /
  build_relationship_graph）裸名调用零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed
- 后续：insights 统计缓存层（stats_cache_path / message_shards /
  load/save_stats_to_disk / msg_stats_cached）可再独立

## 切片 T-85（R-66）：wechat/insights 拆分 cache 统计缓存层（已完成）

- 新建 `wechat/insights/cache.rs`（177 行）：`StatsPayload` /
  `StatsCache` / `STATS_CACHE` / `STATS_IO_LOCK` + `stats_cache_path` /
  `message_shards` / `dir_signature` / `load_stats_from_disk` /
  `save_stats_to_disk` / `msg_stats_cached`（缓存编排器）从
  insights.rs 双段逐字搬移（显式 LF 批量机械搬移，边界校验 +
  temp 备份）
- 依赖处理：cache.rs 经 `super::{collect_active_days,
  collect_msg_counts, GraphEmitCtx, SessionStats}` 引用分析函数与
  类型；`collect_msg_counts` / `collect_active_days` 改 `pub(crate)`
  （facade 供 cache 经 super 访问）；`message_shards` /
  `msg_stats_cached` 改 `pub(crate)`（facade 经 glob re-export 使用）；
  门面移除随迁未用的 OnceLock/SystemTime
- insights.rs 1065 → 903 行；构建流程裸名调用零改动
- 回归：`cargo fmt --check` 通过；`cargo check` 0 warnings；
  `cargo test --lib --no-default-features` 211 passed / 0 failed

## 切片 T-86（R-67）：WeChatPanel 选择记录/关键词过滤下沉（已完成）

- `wechat/utils/panel.ts` 新增两个纯函数：
  - `selectedIdsFromRecord(sel)`——选择记录 → 有效正数 id 数组
    （过滤非法/非正数项，替代 favSelectedIds 内联 6 行派生）
  - `filterByKeyword(items, q, keyFn)`——泛型大小写不敏感子串过滤
    （空关键词返回全量；替代表情包/自定义表情两处内联过滤）
- WeChatPanel：favSelectedIds / filteredEmoPackages /
  filteredEmoCustom 三处派生收敛为纯函数调用；emoticons 声明在后，
  保持 $derived.by 惰性求值（直呼形式触发 TDZ 报错，改回闭包）
- smoke-panel-utils.mjs 新增 6 组断言（id 过滤语义、关键词子串、
  空关键词、md5 大小写）
- 回归：svelte-check 0 errors / 176 warnings；39 冒烟/单元测试通过；
  `npm run build` 通过
- 附带：AGENTS.md 审计确认与仓库完全同步（37 个 smoke 文件精确匹配）

## 切片 T-87（R-68）：WikiPanel 图算法下沉 graphUtils（已完成）

- 新建 `kb/graphUtils.ts`：3 个纯图算法
  - `graphNeighborSet(edges, nodeId)`——邻居 id 集合（含自身），
    消除 graphNeighbors 与 graphHoverNeighbors 两处逐字重复
  - `nodeDegreeMap(edges)`——每节点总连接度（孤立节点判断）
  - `edgeLinkTypes(graph)`——边类型去重枚举（按首现序）
- WikiPanel 五处派生收敛为纯函数调用（graphNeighbors /
  graphHoverNeighbors / nodeTotalDegree / graphLinkTypes），
  null 守卫保留在组件侧
- 新增 `smoke-kb-graph-utils.mjs`（8 组断言：邻居含自身/度计数/
  边类型去重序/空图）
- 回归：svelte-check 0 errors / 176 warnings；40 冒烟/单元测试通过
  （39 + 新增 1）；`npm run build` 通过

## 切片 T-88（R-69）：RelationshipGraph 统计派生下沉 graphStats（已完成）

- 新建 `wechat/graph/graphStats.ts`：两个纯函数
  - `topByField(nodes, get, count)`——按数值字段取 Top-N（缺失按 0，
    返回新数组不修改原），消除 personTopByMsg / personTopByGroups /
    groupTopByMsg / groupTopByShared / groupTopByMembers 五处重复的
    sort+slice 模式
  - `groupCommunities(nodes)`——圈子按成员数降序分组（排除 self 与
    负 community）
- RelationshipGraph 六处派生收敛为纯函数调用（5 榜单 + communities）
- 新增 `smoke-graph-stats.mjs`（7 组断言：降序取 N/缺失按 0/
  不修改原数组/圈子排除与排序）
- 回归：svelte-check 0 errors / 176 warnings；41 冒烟/单元测试通过
  （40 + 新增 1）；`npm run build` 通过

## 切片 T-89（R-70）：GlobalChatTab 模型能力分类下沉 modelKind（已完成）

- 新建 `llm/modelKind.ts`：两个纯函数
  - `classifyModelType(modelType)`——后端 model_type 文本 → 能力类别
    （`ModelKind`：chat/image/video/speech/embed/rerank，未知/缺失
    视为对话），收敛五处重复的中文字面量判断
  - `modelSendLabel(kind)`——发送按钮文案（生成/合成/排序/发送），
    消除 8 层嵌套三元表达式
- GlobalChatTab：isImageGen 等六处派生改由 `modelKind` 比较；
  sendLabel 一行调用；删除本地 `modelTypeOf`（模板中两处原始
  model_type 标签改为内联访问）
- 新增 `smoke-model-kind.mjs`（14 组断言：六类映射、未知/缺失回退、
  全部文案）
- 回归：svelte-check 0 errors / 176 warnings；42 冒烟/单元测试通过
  （41 + 新增 1）；`npm run build` 通过

## 切片 T-90（R-71）：GraphView 时间轴/搜索派生下沉（已完成）

- `wechat/utils/graphView.ts` 追加两个纯函数：
  - `timelineBounds(nodes, edges)`——节点/边非零时间戳的 min..max
    （复用既有 nodeT/edgeT；全 0 或空图返回 null → 禁用时间轴）
  - `searchNodes(nodes, q)`——label/id 子串匹配（大小写不敏感），
    空关键词返回空，截断前 10
- GraphView：`timelineRange`（20 行 `$derived.by`）与
  `searchResults`（7 行）收敛为一行纯函数调用
- smoke-graph-view.mjs 扩展 4 组断言（min/max 混合节点边、全 0 禁用、
  label/id 匹配、前 10 截断）——总断言 13 → 17
- 回归：svelte-check 0 errors / 176 warnings；42 冒烟/单元测试通过；
  `npm run build` 通过

## 切片 T-91（R-72）：DailySummary/HookManager 状态与统计下沉（已完成）

- `wechat/utils/summary.ts` 追加 `summarizeRecords(records)`：
  记录统计（总数/成功/失败/成功平均字符数），替代 DailySummary
  recordStats 内联 9 行
- 新建 `wechat/utils/hook.ts`：`hookStatusLabel(s)`（检测中…/不支持/
  未启用/DLL 缺失/正在监控/等待连接）与 `hookStatusCls(s)`（四档
  hm-status-* 样式类），替代 HookManager 两处 7 行 `$derived.by`
- 测试：smoke-daily-summary.mjs 扩展 3 组断言（总数/成功/失败、
  平均字符数、空记录）；新增 smoke-hook.mjs（11 组断言覆盖
  label/cls 全分支）
- 回归：svelte-check 0 errors / 176 warnings；43 冒烟/单元测试通过
  （42 + 新增 1）；`npm run build` 通过

## 切片 T-92（R-73）：AnnualSummary 热力峰值/时段占比下沉（已完成）

- `wechat/utils/annual.ts` 追加两个纯函数：
  - `heatPeak(matrix)`——热力矩阵峰值（星期索引/小时/值），空矩阵
    返回 null
  - `hourShare(matrix, hours)`——指定小时集合占总热力百分比（复用
    pct，空矩阵 → 0）
- AnnualSummary：peakInfo 用 heatPeak（消除双层循环 12 行）、
  nightShare/morningShare 用 hourShare（消除各自 6 行内联）
- smoke-annual-summary.mjs 扩展 6 组断言（峰值坐标/空矩阵/全 0、
  小时占比/空矩阵/未命中）——总断言 12 → 18（其中一处测试数据
  总热力非 100 导致期望值计算错误，修正矩阵后锁定 50%）
- 回归：svelte-check 0 errors / 176 warnings；43 冒烟/单元测试通过；
  `npm run build` 通过

## 切片 T-93（R-74）：AnnualSummary 周末占比/月份/画像下沉（已完成）

- `wechat/utils/annual.ts` 追加 4 个纯函数：
  - `weekendShareOf(matrix)`——周末（星期 5、6 行）占总热力百分比
    （行数 < 7 保守返回 0）
  - `bestIndex(values)`——最大正值索引（空/全 0 → -1）
  - `calmIndex(values)`——最小正值索引（无正值取首个非正值）
  - `buildPersonaTags(opts)`——人物画像标签（作息/周末/群聊/话痨
    四维度），替代 personaTags 内联 14 行
- AnnualSummary：weekendShare / bestMonth / calmMonth / personaTags
  四处派生收敛为纯函数调用（约 30 行 → 10 行）
- smoke-annual-summary.mjs 扩展 11 组断言（最佳/最静索引边界、
  周末占比、画像标签分支）——总断言 18 → 30（修正一处周末占比
  期望值：7×10 矩阵总热力 70 → 实际 28.6%）
- 回归：svelte-check 0 errors / 176 warnings；43 冒烟/单元测试通过；
  `npm run build` 通过

## 切片 T-94（R-75）：WikiPanel 目录树纯算法下沉（已完成）

- 新增 `kb/dirTreeUtils.ts` 3 个纯函数：
  - `buildDirSubtree(dirs)`——每个目录的子孙 id 集合（含自身），
    锁定「按目录筛选」与计数口径一致
  - `buildDirTree(dirs)`——扁平目录 → 前序有序树列表（同级保持输入顺序）
  - `filterPagesByDir(pages, dirFilter, dirSubtree)`——按目录子树过滤
    页面（null 不过滤；未知目录返回空）
- `kbTypes.ts` 新增 `WikiDir` / `WikiDirTreeItem` 接口；
  `ipc.ts` 的 `wikiDirs` 返回类型由内联匿名形状收敛为 `WikiDir[]`
- WikiPanel.svelte：两处 `$derived.by` 块（约 40 行）收敛为 3 个
  `$derived` 纯函数调用；`wikiDirs` 状态类型化；消除 `dirFilter as number`
  断言（函数签名直接收窄为 `number | null`）
- 新增 smoke-dir-tree.mjs（14 断言：子树边界/前序顺序/过滤语义）
- 回归：svelte-check 0 errors / 176 warnings；44 冒烟/单元测试通过
  （42 smoke + run-store + voice）；`npm run build` 通过

## 切片 T-95（R-76）：KbDashboard 知识库关键词过滤下沉（已完成）

- `kb/fileUtils.ts` 新增 `filterKbsByKeyword(kbs, keyword)`：名称/描述
  大小写不敏感匹配，空白关键词返回原数组引用（语义与原实现逐项等价）
- KbDashboard.svelte：`filteredKbs` 由内联 `$derived.by`（5 行）收敛为
  `$derived(filterKbsByKeyword(kbs, kbSearch))`
- smoke-kb-file-utils.mjs 扩展 6 组断言（名称/描述/空白/未命中/null
  描述）——总断言 24 → 30
- 回归：svelte-check 0 errors / 176 warnings；冒烟测试通过

## 切片 T-97（R-78）：跨组件多字段关键词过滤收敛（已完成）

- `wechat/utils/panel.ts` 新增 `filterByAnyKeyword<T>(items, q, ...keyFns)`：
  去首尾空格、大小写不敏感，任一字段命中即保留；空白关键词返回原数组引用
- WeChatPanel 公众号/订阅号过滤（两处重复内联）、GroupMonitor 群聊过滤、
  HookManager 会话过滤（含 kind 预筛）全部收敛为该纯函数
- 行为变更（有意为之）：公众号搜索输入纯空白时由「空结果」变为
  「显示全部」，与 GroupMonitor/HookManager 等既有 trim 语义对齐
- smoke-panel-utils.mjs 新增 6 组断言（多字段命中/空白/原数组引用）
- 回归：svelte-check 0 errors / 176 warnings；冒烟测试通过

## 切片 T-98（R-79）：WeChatPanel 朋友圈/联系人/文件过滤收敛（已完成）

- 三处 `$derived.by` 内联多字段过滤收敛为 `filterByAnyKeyword`：
  - `filteredMoments`（作者/文本/位置，3 字段）
  - `filteredContacts`（display_name/nick_name/remark/username/alias，5 字段）
  - `shownFiles`（file_name/md5，2 字段，保留分类预筛与 sort 语义）
- 坑：`momentsPage` 声明位于派生块之后，`$derived(直接表达式)` 触发
  TDZ 编译错误，须保留 `$derived.by(() => ...)` 闭包（Svelte runes
  惰性求值限制，既有约定）
- settings 的「分类行内单元格命中」过滤逻辑不同，未强行套用
- 回归：svelte-check 0 errors / 176 warnings；冒烟测试通过

## 切片 T-99（R-80）：DB 状态弹窗组件化试点（已完成）

- 新增 `wechat/components/DbStatusPopup.svelte`：props 面 =
  `loading / lines / onClose / onRefresh`（4 个），模板与 9 条
  scoped CSS（含独立 `@keyframes wc-spin` + `.wc-loading-inline`
  副本）一并下沉
- WeChatPanel 弹窗模板约 22 行收敛为 7 行；父级 document 点击外
  关闭监听不变（依赖类名定位，子组件挂载时机与原 `{#if}` 一致）
- 简化：`{:else}` 分支中冗余的 `loading ? '检查中…' : '刷新'`
  三元收敛为常量 `"刷新"`（该分支 loading 恒为 false）
- 验证：svelte-check 0 errors / 176 warnings；`npm run build` 通过
  （组件拆分无法用冒烟 harness 覆盖，回归以类型检查 + 构建为准）

## 切片 T-100（R-81）：监控状态/指标面板组件化（已完成）

- 新增 `wechat/components/MonitorControl.svelte`：props 面 =
  `status / loading / canStart / onStart / onStop`（5 个）
- 启动按钮可用性谓词（DB 已检查且无未找到/失败）收敛为
  `monitorCanStart` 派生，父组件计算后传入；DB 状态声明在后，
  保持 `$derived.by` 闭包
- 移动 8 条 scoped CSS；删除父级中一条被 `!important` 规则完全
  覆盖的 `.wc-monitor-running` 死规则（无可见行为变化）
- 验证：svelte-check 0 errors / 176 warnings；`npm run build` 通过

## 切片 T-101（R-82）：全仓 Svelte import 缩进归一化（已完成）

- 历次提取切片在 `<script>` 块中混入的 column-0 `import` 行（28 个
  文件、约 60 行）统一为 2 空格缩进，与各文件既有风格一致
- 逐文件保留原 EOL（CRLF/LF）与末尾换行，无 BOM 引入；
  全仓扫描确认 0 处残留 column-0 import
- 覆盖 kb / llm / wechat / search / agents / copywriting / db 等
  模块；纯机械缩进变更，不涉及任何逻辑
- 回归：svelte-check 0 errors / 176 warnings；`npm run build` 通过

## 切片 T-102（R-83）：关键词过滤工具上移共享层 + llm 收敛（已完成）

- 新增 `src/lib/utils/filter.ts`：`filterByKeyword` / `filterByAnyKeyword`
  上移为跨 feature 共享工具；`FilterText = string | readonly string[]`，
  数组分段任一命中即命中（支持 capabilities 等数组字段）
- 语义统一为「去首尾空格 + 大小写不敏感 + 空白返回原引用」；
  `filterByKeyword` 由原「不 trim」改为 trim——纯空白输入由空结果
  变为全量，与 `filterByAnyKeyword` 及全应用主流一致（记录在案）
- `wechat/utils/panel.ts` 改为 re-export（`export { ... } from
  '../../utils/filter'`），WeChatPanel/GroupMonitor/HookManager
  既有调用点零改动
- AiRolesPanel（name/description）与 GlobalChatTab（name/
  description/capabilities 数组/system_prompt）收敛到共享函数
- 新增 smoke-filter-utils.mjs（10 断言：单/多字段、数组分段、trim、
  原引用）；smoke-panel-utils.mjs 保留 re-export 兼容断言
- 回归：svelte-check 0 errors / 176 warnings；冒烟测试通过

## 切片 T-103（R-84）：summary.fmtTime 收敛共享 formatTs（已完成）

- `wechat/utils/summary.ts` 本地 fmtTime（毫秒 + 手写 YYYY-MM-DD HH:mm）
  收敛为 `formatTs(ts ?? 0, { invalidPlaceholder: '—' })`
- 行为修正：原实现只按毫秒解析，秒级时间戳会错显 1970；formatTs
  秒/毫秒/微秒自适应后正确显示（毫秒场景输出逐值不变，smoke 锁定）
- smoke-daily-summary.mjs 改为 bundle 方式加载（summary.ts 新增
  `../../format` 运行时依赖），14 断言全部通过
- 边界记录（不收敛）：GlobalSearch filterContacts/filterEvents 是
  「搜索型」语义（无词清空），与过滤型「无词返回全量」不同；
  avatarLetter/colorFromName 三套实现（format.ts/index.ts/
  HookManager）语义分叉且影响头像视觉，强行合并会改变可观测输出
- 回归：svelte-check 0 errors / 176 warnings；冒烟测试通过

## 切片 T-104（R-85）：formatTs 增加 dateOnly，graphView.fmtTime 收敛（已完成）

- `src/lib/format.ts` 的 `FormatDateTimeOptions` 新增 `dateOnly`；
  `formatDate` 支持仅输出日期（YYYY-MM-DD / MM-DD），默认 false
  不影响既有调用
- `wechat/utils/graphView.ts` 本地 fmtTime（秒级手写 YYYY-MM-DD）
  收敛为 `formatTs(ts, { dateOnly: true, showYear: true,
  invalidPlaceholder: '' })`；GraphView.svelte 调用点零改动
- smoke-format-bytes.mjs 新增 4 组 dateOnly 断言（总 29 → 33）；
  smoke-graph-view.mjs 改为 bundle 加载（graphView.ts 新增运行时
  依赖），17 断言含 fmtTime 边界全部通过
- 全仓 fmtBytes/fmtDate 扫描结论：现存定义均为共享 formatBytes/
  formatTs 的薄别名（参数不同），无重复实现残留
- 回归：svelte-check 0 errors / 176 warnings；冒烟测试通过

## 切片 T-105（R-86）：Rust clippy 安全类别批量修复（已完成）

- 全仓 clippy 基线：195 警告（CI 未启用 clippy，非既有基准门禁）。
  选取 32 个语义严格等价的 lint 类别经 `cargo clippy --fix` 批量应用，
  42 个源文件改动（2~26 行/文件），clippy 警告 195 → 115
- 修复类别：needless_return / question_mark / manual_flatten /
  derivable_impls / collapsible_if / map_clone / map_identity /
  useless_borrows_in_formatting / redundant_field_names /
  unnecessary_cast / unwrap_or_default / manual_* 等
- 逐文件 Compare-Object 审查：全部为语义等价改写（flatten 跳过
  Err、? 传播、derive(Default) 字段全 Default 等）
- 特性条件修复 2 处：ocr precheck/precheck_init 字段与
  llm last_err 的 mut 在 no-default-features 下触发死代码警告，
  加 `#[cfg_attr(not(feature = ...), allow(...))]` 有据保留（恢复
  cargo check 0 warnings）
- 坑：clippy --fix 在 no-default-features 下移除的 `mut` 在
  default features（local-stt）下被再次赋值 → E0384，已修复；
  教训：特性条件代码的 lint 修复必须双特性矩阵验证
- 回归：cargo check/build --lib 0 代码警告；cargo test
  211 passed / 0 failed / 19 ignored；cargo fmt --check 通过
- 遗留：cargo build 链接 bin 受运行中的 st-control.exe（PID 12848）
  文件锁阻塞（用户进程，未终止），以 cargo build --lib 验证库编译

## 切片 T-106（R-87）：clippy 剩余安全项人工收敛（已完成）

- 自动修复空间耗尽（clippy --fix 仅剩 1 条 suggestion）后，人工
  收敛 11 处语义等价项：
  - manual_clamp 3：`x.max(a).min(b)` → `x.clamp(a, b)`（usize 确认）
  - unwrap_err-after-is_err 3（oracle/dbkey/debugger）：`if let Err(e)`
  - unwrap-after-is_some 1（voice sid）：`if let Some(sid_val)`
  - filter_next 1（ask/plan）：`.filter(p).next()` → `.find(p)`
  - manual_strip 1（session BLOB）：`starts_with("0x") + &s[2..]`
    → `strip_prefix("0x")`
  - wildcard_in_or_patterns 1（session 导出）：`"txt" | _` → `_`
- 全量 clippy：195（基线）→ 81；双特性 cargo check 0 代码警告；
  cargo test 211 passed / 0 failed / 19 ignored；fmt --check 通过
- 剩余 81 分类（蓝图候选，非机械修复）：
  - type_complexity 24 + too_many_arguments 25——函数签名/类型别名
    需重新设计（T-蓝图-6 候选）
  - redundant_locals 7（`let x = x` 遮蔽）——需逐个判断是否有意
  - sort_by_key 8 / field_reassign_with_default 6 / doc 格式 6 /
    零星单点 5——低风险但逐个人工

## 切片 T-107（R-88）：clippy 低风险人工项清零（已完成）

- doc 注释格式 7 处（列表段落空行分隔 / 缩进对齐 / 普通注释降级）
- sort_by_key 8 处：全部为降序排序，改 `sort_by_key(Reverse)`（稳定
  排序语义等价）
- field_reassign_with_default 6 处：ChunkConfig 直接构造（overlap
  依赖最终 chunk_size，先算后用的顺序注释保留）；4 处 Win32 结构体
  （PROCESSENTRY32W/MODULEENTRY32W）改 `{ dwSize, ..Default }`
- redundant_locals 6 处：insights.rs 线程作用域内 `let x = x` 遮蔽，
  均为 Copy 引用，删除后编译验证等价（move 闭包捕获外层引用）
- 单点 4 处：retrieval `if let Ok` 循环 → flatten；monitor doc 续行
  合并；WeChatMonitorState 补 `impl Default`（new 委托 default）；
  kb/parse.rs `ChunkStrategy::from_str` 属 should_implement_trait
  设计类，保留记录
- clippy 全量：195 → **50**（剩余 = type_complexity 24 +
  too_many_arguments 25 + from_str 1，全部需蓝图设计）；
  双特性 cargo check 0 代码警告；cargo test 211 passed / 0 failed /
  19 ignored；fmt --check 通过

## 蓝图 T-蓝图-6：clippy 剩余 50 项架构设计（规划完成，未实施）

### A. 参数对象拆分（too_many_arguments 25 处 → 10 个参数结构体）

同构组（先做，一个切片可覆盖一组）：

1. **TableQueryParams**（5 处共用）：`db.rs query_table`、
   `external_db.rs query_table`、`sql_browse.rs query_table`、
   `ipc_handlers.rs query_table / external_query_table`。
   字段：`table / page / page_size / order_col / order_dir /
   filter / recount / cursor / direction`。IPC 层透传参数
   反序列化后构造该结构体，各查询实现签名收敛为
   `query(&conn_or_db, params: TableQueryParams)`。
2. **ImageResolveCtx + ImageQuery**（resolve.rs 6 处）：
   `ImageResolveCtx<'a> { wechat_base_dir, res_db_path:
   Option<&Path>, db_cache: Option<&MonitorDBCache>, decrypted_dir,
   decoded_dir, aes_key: Option<&[u8]>, xor_key: u8 }`；
   `ImageQuery<'a> { username, local_id, hd }`。6 个函数签名
   收敛为 3 个参数。
3. **RagRequest**（kb/rag.rs 3 处）：`rag_answer / rag_context /
   rag_stream` 共用 `{ user_id, kb_id, query, embed_provider_id,
   embed_model, gen_provider_id, gen_model, top_k, mode,
   chunk_overrides }`（db 与 on_delta 保留为参数）。
4. **ChatRequest**（llm/client.rs 2 处）：`chat_completion /
   chat_completion_stream` 共用 `{ model, messages, max_tokens,
   temperature, top_p, presence_penalty, frequency_penalty }`
   （provider 与 on_delta 保留）。
5. **ChunkingOptions**（kb/handlers 2 处）：`kb_reprocess_document /
   process_document_async` 共用 `{ embedding_provider,
   embedding_model, chunk_strategy, chunk_size, chunk_overlap }`。
6. **DocListQuery**（kb/handlers/docs.rs 1 处）：`kb_list_documents`
   的 `{ page, page_size, keyword, status, tag, dir_id }`。

单点组（各自一个切片）：

7. **TaskInsert**（automation/engine.rs insert_task）：消息派生字段
   打包为行数据结构体。
8. **LogEntry**（bot/db.rs insert_log）：日志字段打包。
9. **MetricEvent**（kb/handlers.rs log_metric_event）：uid + 5 个
   Option 维度打包（事件实体）。
10. **MonitorStartCtx**（wechat/monitor.rs start_monitor，14 参数）：
    拆分「连接上下文 + 图片解密上下文」，复用 ImageResolveCtx 的
    目录/密钥字段。
11. **ConnCtx**（ws_server.rs handle_connection）：clients /
    message_count / event_tx / shutdown_rx / broadcast_rx /
    direct_rx 打包。
12. **有意保留**：`sns_image.rs mix`（8 个 &mut u64 是哈希算法
    内联状态，拆分反而伤可读性）——加 `#[allow(clippy::
    too_many_arguments)]` + 注释。

### B. 复杂类型具名化（type_complexity 24 处 → 约 12 个类型）

共享原语（先做）：

1. `type DirSig = (SystemTime, u64);`
2. `type DbSigPair = (Option<DirSig>, Option<DirSig>);`（common.rs
   db_sig、contacts.rs 两个缓存字段共用）
3. `type DirFileSigList = Option<Vec<(String, DirSig)>>;`（file.rs /
   video.rs 索引缓存共用）

行类型（SQL 查询元组 → 具名 struct，各放所属模块）：

4. `ChunkRow`（kb/rag.rs:426）
5. `Bm25Row`（kb/retrieval.rs:51、184 两处同构）
6. `AclRuleRow`（kb/retrieval.rs:454）
7. `WikiPageRow`（kb/wiki.rs:246）
8. `MediaFileRow { buf_a, buf_b, size }`（cdn_image.rs / file.rs /
   video.rs:83 三处同构）

缓存条目类型（static 缓存值具名化）：

9. `Md5CacheEntry = (Instant, String)`（image/crypto.rs）
10. `KeyCacheEntry = (Vec<u8>, Arc<Vec<u8>>)`（db_cache.rs）
11. `TransferStatusEntry`（messages.rs:551 完整字段按查询列定义）
12. `MomentLikesMap = HashMap<i64, MomentLikes>`（moments.rs:524，
    `struct MomentLikes { likes: Vec<MomentLike>, comments:
    Vec<MomentComment> }`）
13. `KnownTable`（settings.rs:44，`struct KnownTable { name, label,
    columns: &'static [(&'static str, &'static str)] }`）
14. `EncryptedPayload { salt, iv, hmac, ciphertext }`
    （backup.rs:53 返回值）

### C. from_str 设计项（kb/parse.rs ChunkStrategy::from_str）

`should_implement_trait`：实现 `std::str::FromStr`（`Err` 返回
`InvalidChunkStrategy(String)` 或沿用回退 Recursive 语义），调用点
改 `chunk_strategy.unwrap_or("recursive").parse()`。与 A5 的
ChunkingOptions 切片一起做（同属分块配置重构）。

### D. 实施顺序与回归

1. B1-B3 共享原语 → 2. B4-B8 行类型 → 3. B9-B14 缓存/返回值类型
   → 4. A1-A6 同构参数组 → 5. A7-A11 单点参数组 → 6. C from_str。
   每组一个切片；每切片后跑双特性 cargo check + test + fmt。
   `sns_image.rs mix` 与 B13 `settings.rs` 常量表可独立收尾。

## 切片 T-108（R-89）：蓝图 B1-B3 共享类型原语实施（已完成）

- `wechat/modules/common.rs` 新增 3 个共享类型别名：
  `DirSig = (SystemTime, u64)`、`DbSigPair = (Option<DirSig>,
  Option<DirSig>)`、`DirFileSigList = Option<Vec<(String, DirSig)>>`；
  `file_sig` / `db_sig` 返回类型收敛为别名（调用点零改动）
- 替换使用点：contacts.rs 两个缓存条目 `sig` 字段（删冗余
  SystemTime import）、file.rs FILE_INDEX_CACHE + dir_sig/root_sig、
  video.rs COVER_PATH_CACHE / VIDEO_DIR_INDEX / THUMB_FILE_CACHE +
  dir_sig/video_root_sig（SystemTime 保留给算法字段）
- clippy：50 → **45**（type_complexity 24 → 19）；双特性
  cargo check 0 警告；cargo test 211 passed / 0 failed /
  19 ignored；fmt --check 通过

## 切片 T-109（R-90）：蓝图 B4-B8 SQL 行类型具名化（已完成）

- 元组 struct 行类型（字段语义注释 + 构造/解构点替换）：
  - `EmbeddingRow`（retrieval.rs，2 处向量检索查询行，
    id/doc_id/kb_id/content/page_no/section/blob/doc_title）
  - `AclRuleRow`（retrieval.rs，kb_acl 规则行）
  - `ChunkRow`（rag.rs，分片 + 文档标题行）
  - `WikiPageRow`（wiki.rs，12 字段页面完整行；元组索引访问
    `b.7` 语法天然保留）
  - `MediaRow`（common.rs 共享，file.rs / video.rs 2 处媒体行，
    content/compressed/create_time）
  - `CdnMediaRow`（cdn_image.rs 本地变体，local_type 列不同构）
- clippy：45 → **37**（type_complexity 19 → 11）；双特性
  cargo check 0 警告；cargo test 211 passed / 0 failed /
  19 ignored；fmt --check 通过

## 切片 T-110（R-91）：蓝图 B9-B14 缓存/返回值类型具名化（已完成）

- 缓存条目类型：`Md5CacheEntry`（image/crypto.rs，解析时间+MD5）、
  `KeyCacheEntry`（db_cache.rs，salt+派生密钥）、
  `TransferStatusEntry` + `TransferStatus`/`TransferCacheKey`
  （messages.rs，分库签名+转账状态映射）、`ShardCacheKey`
  （messages.rs 分库索引 key）、video.rs 缓存值/键别名
  （UsernameLocalId/VideoDirsSig/VideoPathCacheEntry/
  CoverPathCacheEntry/VideoDirIndexEntry）
- 行/返回值类型：`FavoriteRow`（favorites.rs，含 SELECT 结构
  保留说明）、`MomentInteractions` + `MomentInteractionsMap`
  （moments.rs，pub 导出 + Serialize）、`EncryptedPayload`
  （backup.rs，salt/iv/hmac/ciphertext 具名返回）、`KnownTable`
  （settings.rs，常量表 struct + known() 构造）
- 途中修正：MomentInteractions 泄漏 pub API 需 pub + Serialize；
  transfer map 元组索引访问经 alias 保留零改动
- clippy：37 → **26**（type_complexity 全部清零，剩余
  too_many_arguments 25 + from_str 1）；双特性 cargo check
  0 警告；cargo test 211 passed / 0 failed / 19 ignored

## 切片 T-111（R-92）：蓝图 A1-A2 参数对象拆分（已完成）

- **A1 TableQueryParams**（5 处收敛）：
  `sql_browse.rs` 定义 `pub struct TableQueryParams`（9 字段），
  `sql_browse::query_table(conn, &params)` 核心实现、
  `db::Database::query_table(&self, &params)` / `external_db::
  query_table(db_path, &params)` 薄封装、`ipc_handlers` 两个
  command 内部构造 params 后调用
  - 长函数体采用「签名收敛 + 开头字段绑定」模式（局部变量保持
    原名与类型，函数体零改动）
  - IPC 入口（`#[tauri::command]`）为扁平参数契约，加
    `#[allow(clippy::too_many_arguments)]` 有据保留（2 处）；
    `sns_image::mix` 哈希算法内联状态同样有据 allow（1 处）
- **A2 ImageResolveCtx + ImageQuery**（6 处收敛）：
  `resolve.rs` 定义 `ImageResolveCtx<'a>`（目录/密钥稳定配置，
  res_db_path/db_cache 按 live/离线模式二选一）与 `ImageQuery<'a>`
  （username/local_id/hd）；6 个解析函数签名收敛为 (ctx, q)，
  函数体开头绑定后零改动；内部 local_or_cdn_* 调用同步收敛
  - 调用方 3 文件 5 处改造：data.rs（live/离线双分支）、
    session.rs（ctx 提出循环外复用）、media.rs（HTTP 媒体接口）
- clippy：26 → **14**（too_many_arguments 25 → 13，另 3 处有意
  allow）；双特性 cargo check 0 警告；cargo test 211 passed /
  0 failed / 19 ignored；fmt --check 通过

## 切片 T-112（R-93）：蓝图 A3-A4 参数对象拆分（已完成）

- **A3 RagRequest**（rag.rs 3 处收敛）：`pub struct RagRequest<'a>`
  （user_id/kb_id/query/embed_provider_id/embed_model/
  gen_provider_id/gen_model/top_k/mode/chunk_overrides），
  `rag_answer / rag_context / rag_stream` 签名收敛为
  `(db, &RagRequest)`（rag_stream 保留 on_delta）
  - 函数体只绑定实际使用的字段（query/gen_*），检索参数经
    `rag_context(db, req)` 传递，消除 unused 警告
  - 调用方 3 文件 4 处改造：search.rs（2×rag_answer + 1×
    rag_stream）、agents.rs（1×rag_context）
- **A4 CompletionParams**（client.rs 2 处收敛）：
  `pub struct CompletionParams<'a>`（model/messages/max_tokens/
  temperature/top_p/presence_penalty/frequency_penalty；
  provider 与 on_delta 保留），`chat_completion /
  chat_completion_stream` 签名收敛
  - 命名避开既有 `llm::types::ChatRequest`（调用层请求），
    client 层参数用 CompletionParams
  - 调用方 7 文件 10 处改造：client.rs 内部 2 处（probe/测试）、
    automation/engine、kb/rag（stream）、llm/handlers ×2、
    daily_summary、wechat/ask/llm ×3
- clippy：14 → **9**（too_many_arguments 13 → 8）；双特性
  cargo check 0 警告；cargo test 211 passed / 0 failed /
  19 ignored；fmt --check 通过

## 切片 T-113（R-94）：蓝图 A5-A6 文档处理/查询参数收敛（已完成）

- **A5 ChunkingOptions**（docs.rs）：`pub struct ChunkingOptions`
  （embedding_provider/model + chunk_strategy/size/overlap），
  `process_document_async` 12 参数 → 7+1，函数体开头绑定字段
  后零改动；3 个调用点（上传/抓取/手动创建）构造参数对象
- **A6/A5 command 入口**：`kb_reprocess_document`（chunks.rs）与
  `kb_list_documents`（docs.rs）为 `#[tauri::command]` 扁平参数
  契约，加 `#[allow(clippy::too_many_arguments)]` 有据保留
  （与 query_table 等 IPC 入口同口径）
- clippy：9 → **7**（too_many_arguments 8 → 6）；双特性
  cargo check 0 警告；cargo test 211 passed / 0 failed /
  19 ignored；fmt --check 通过

## 切片 T-114（R-95）：蓝图 A7-A11 + C 单点组与 FromStr（clippy 清零）

- **A7 TaskInsert**（automation/engine.rs）：insert_task 10 → 3
  参数（conn + msg + &TaskInsert），2 个调用点构造
- **A8 LogEntry**（bot/db.rs）：insert_log 9 → 2 参数
  （conn + &LogEntry），bridge/manager 2 个调用点构造
- **A9 MetricEvent**（kb/handlers.rs）：log_metric_event 8 → 2
  参数（db + &MetricEvent），11 个埋点调用点（analytics/chunks/
  docs/search/wiki）全部构造事件对象
- **A10 MonitorStartCtx**（wechat/monitor.rs）：start_monitor 14
  → 2 参数（ctx + cancel_rx），handlers/monitor 调用点构造
- **A11 ConnCtx**（ws_server.rs）：handle_connection 8 → 3 参数
  （stream + ip + ctx），accept 循环构造
- **C ChunkStrategy::FromStr**（kb/parse.rs）：inherent from_str
  迁移为 `impl std::str::FromStr`（未知值回退 Recursive 语义
  保持），5 个测试断言与 2 个生产调用点（docs/chunks 分块配置）
  改用 `.parse()`
- **process_document_async 补收敛**：8 参数仍超阈值，文档任务
  字段打包 `DocProcessJob`（kb_id/doc_id/version_id/job_id/
  file_type/data），签名收敛为 (db, job, opts) 3 参数
- **clippy 全量：195 → 0**（R-86 起清零完成）；双特性 cargo
  check 0 警告；cargo test 211 passed / 0 failed / 19 ignored；
  fmt --check 通过

## 切片 T-115（R-96）：前端未使用 CSS 选择器清理（已完成）

- svelte-check 176 警告分析：约 82 处未使用 CSS 选择器 +
  86 处 a11y（click 元素需键盘/ARIA）+ 8 处 runes 局部引用
  （state_referenced_locally，潜在真实缺陷候选）
- 清理 82 处未使用 CSS 选择器（14 个文件）：DailySummary 18、
  GeneralRecords 13、GraphView 8、WeChatConfig 7、AnnualSummary 5、
  BackupManager 5、GroupMonitor 5、AskPanel 4、PrivacyScan 4、
  RelationshipGraph 4、WeChatPanel 4、DailySummaryForm 3、
  DbManager/GlobalChatTab 各 1
- 按 svelte-check 行号降序删除规则块（含多行规则），删除后精确
  验证 0 残留（子串误报如 .as-years/.as-year-on 已排除）；
  保留各文件 EOL、无 BOM
- svelte-check：176 → **94 warnings**（0 errors）；45 测试
  0 失败；npm run build 通过
- 剩余 94 = a11y ~86 + runes 局部引用 8，均需逐处设计（后续切片）

## 切片 T-116（R-97）：runes 局部引用（state_referenced_locally）处理（已完成）

- 8 处警告逐一审查，结论均为「初始化快照 + 后续同步」的有意设计：
  - LiveNumber.svelte（2）：display/shown 动画状态初始化取 value，
    后续由 $effect 驱动（原语义保留）
  - RelationshipGraph.svelte（2）：滑杆草稿值初始化快照，防抖
    150ms 提交时读 settings 最新值
  - carousel.svelte（3）：carouselState 初始 orientation/opts/plugins
    快照，已有 $effect（L84-88）同步 props 变化
- 用 `// svelte-ignore state_referenced_locally` + 说明注释有据保留
  （svelte-check 确认无无效 ignore 警告）
- svelte-check：94 → **87 warnings**（0 errors）；45 测试
  0 失败；npm run build 通过
- 剩余 87 = a11y 警告（click 元素缺键盘处理/ARIA role），
  需逐处设计键盘行为，留待后续分组处理

## 切片 T-117（R-98）：a11y 警告清零（svelte-check 176 → 0）

- 86 处 a11y 警告分组处理：
  - **模态遮罩/容器**（KbDocs/KnowledgeBase/KbChat/KbActivity/
    GlobalChatTab）：遮罩加 `role="button"` + `aria-label` +
    `tabindex="-1"` + Enter/Space/Escape 键盘关闭，onclick 改
    `e.target === e.currentTarget` 自检；模态容器移除
    `onclick stopPropagation`（mask 自检后不再需要），容器变纯
    内容块——比 stopPropagation 更标准的模态实现
  - **可点击元素**（KbChat 会话行、AgentPanel 卡片、DirTree
    目录名、WeChatPanel 各查看器遮罩）：加 `role="button"` +
    `tabindex="0"` + Enter/Space 键盘触发
  - **事件委托/画布容器**（WikiPanel 详情、MessageBody markdown、
    WeChatPanel 图片舞台）：委托函数签名 `MouseEvent` → `Event`
    （仅用 e.target，键盘 Enter 复用）；画布指针容器
    `role="application"`，3 处委托/指针容器加有据
    `svelte-ignore a11y_no_noninteractive_element_interactions`
    （键盘等价由内部链接/工具栏按钮提供）
  - DailySummary 任务操作：stopPropagation 移到按钮上，容器
    移除监听（任务卡片本身已有 role/keydown）
  - line-clamp 补标准属性（WeChatPanel）
- 途中修复：role="dialog" 方案会触发新警告（dialog 需 tabindex
  且仍报交互），改用 mask target 自检方案；脚本误删 8 个 mask
  开标签已按清单重建（备份 TEMP/kbdocs-fix-*）
- **svelte-check：176 → 0 warnings**（0 errors）；45 测试
  0 失败；npm run build 通过；无 BOM

## 切片 T-118（R-99）：视频播放器弹窗组件化（WeChatPanel 子系统试点）

- 新增 `wechat/components/VideoPlayerDialog.svelte`：朋友圈视频与
  文件视频两处同构模板 + scoped CSS 收敛为单组件
  - props：`open / src / title / error / loadingText / onClose /
    onLocate / onVideoError`（path 由 onLocate 闭包携带，避免
    冗余 prop）
  - 状态分支统一：src → video；error → 错误文案；否则加载提示
- WeChatPanel：两处播放器模板（约 60 行）替换为组件调用；
  父组件移除迁移的 9 条 wc-moment-video-* 播放器 CSS（tile 等
  列表样式保留），子组件自带按钮/加载样式副本
- 验证：svelte-check 0 errors / 0 warnings；45 测试 0 失败；
  npm run build 通过
- 评估：文件查看器/图片查看器因共享 wc-img-viewer-* CSS 且状态
  耦合较高，暂不作为下一拆分候选；消息区为最大剩余子系统

## 切片 T-119（R-100）：bot/db.rs table_columns 重复实现消除（已完成）

- 最终重复模式扫描发现 `table_columns` 两处定义：
  `bot/db.rs` 本地私有实现与 `wechat/modules/common.rs` 共享实现
  语义等价（PRAGMA table_info 读列名，失败返回空）；bot 版本未
  转义表名引号（仅用于固定表名 bot_accounts），收敛无行为差异
- bot/db.rs 删除本地副本（8 行），3 处调用改经 import 使用共享
  `crate::wechat::modules::common::table_columns`
- 前端 avatarLetter/colorFromName 的多份定义确认为既有语义分叉
  （format.ts 前导空格语义 / index.ts 通用版 / HookManager 独立
  调色板），保持有意保留
- 回归：fmt/check 双特性/clippy 0/211 测试全通过

## 全仓最终审计（T-120/R-101 之前的状态确认）

- Rust：fmt --check 0、check 双特性 0 警告、clippy 0、
  cargo test 211 passed / 0 failed / 19 ignored
- 前端：svelte-check 0 errors / 0 warnings、45 测试 0 失败、
  npm run build 通过
- 文档：AGENTS.md 测试清单 43/43 与实际目录同步
- 编码：全仓源文件 0 BOM

## 切片 T-120（R-101）：countMissingChats 纯函数下沉（已完成）

- `wechat/utils/misc.ts` 新增 `countMissingChats(chats)`：统计缺失
  图片会话数（missing 缺省按 0 计），与 checkupPct/checkupRatePct
  同域
- WeChatPanel `checkupMissingChats` 派生收敛为纯函数调用
- smoke-wechat-misc.mjs 新增 2 断言（缺失计数/空列表）——25 → 27
- 回归：svelte-check 0/0；45 测试通过

## 蓝图 T-蓝图-7：WeChatPanel 消息区组件化（规划完成，未实施）

### 边界

消息区是 WeChatPanel 最大剩余子系统（模板约 300 行 + wc-msgs-*
scoped CSS 数十条），状态与函数高度交织：
- 状态：messages / msgEstH / msgPrefix / msgTotalEst /
  msgScrollTop / msgViewH / msgsEl / 加载/编辑/查看器上下文
- 函数：setMessages/appendMessages/prependMessages/trimMessages、
  rebuildMsgMetrics/calibrateRenderedHeights、needDivider、
  onScrollMsgs/scrollMsgsToBottom/setupBottomGuard、
  loadMessages/loadMoreMsgs

### 目标布局（两步）

1. **MessageRow.svelte**（消息行渲染）：props =
   `msg / needDivider 结果 / 渲染上下文`，内含 divider/文本/图片/
   富媒体/语音/视频/引用等分支（现为 snippet）。图片加载走
   imageQueue 服务（已独立），头像/编辑菜单/查看器经回调 props
   注入（15+ 回调面，设计时收敛为分组 props 对象）
2. **MessageList.svelte**（列表容器）：props =
   `messages / 加载状态 / 回调`；内部持有虚拟滚动状态
   （msgEstH/prefix/scroll 等），对外暴露 `scrollToBottom` /
   `loadMore` 方法

### 风险与验证

- 消息行依赖组件级状态（编辑菜单/图片查看器/语音播放），props
  面大；scoped CSS 需随模板迁移
- 只能 svelte-check + build 自动验证，布局/交互需目视
- 建议：应用运行时可验证时实施；先做 MessageRow（渲染纯化），
  稳定后再拆 MessageList（滚动状态）

## 切片 T-121（R-102）：WikiPanel Markdown 渲染器下沉（已完成）

- 新增 `kb/markdown.ts` 纯函数模块：`renderMd`（块级结构）+
  内部 `inlineMd`/`esc`（行内语法 + HTML 转义），支持标题/列表/
  代码块/引用/链接/图片/[[Wiki 链接]]/粗斜体
- WikiPanel.svelte 删除约 85 行本地渲染实现，改 import 共享模块
- 安全核查：Wiki 链接 key 在整体 esc 阶段已转义（引号/尖括号成
  实体，属性注入安全），无需二次转义（曾尝试二次 esc 导致双重
  转义，已修正并注释说明）
- 新增 smoke-wiki-markdown.mjs（20 断言：块级/行内/XSS 转义/
  Wiki key 属性注入防护/空输入）
- 回归：svelte-check 0/0；46 测试（44 smoke + run-store +
  voice）0 失败；npm run build 通过；AGENTS.md 44/44 同步

## 切片 T-122（R-103）：KbDocs Markdown 预览收敛共享 renderMd（已完成）

- 边界扫描发现 KbDocs 的 `mdPreviewHtml`（12 行本地 Markdown
  渲染）与 T-121 下沉的 `kb/markdown.ts renderMd` 功能重叠
- KbDocs 删除本地实现，`mdPreviewHtml` 收敛为 `renderMd(mdDocBody)`
  一行调用；输出差异为行为修正：裸 `<li>`（无效 HTML）→
  `<ul><li>` 包裹、esc 补 `"` 转义（更安全）
- AskPanel `canJump` 评估：仅一行且依赖组件本地 AskCitation 类型，
  下沉收益低，跳过（避免过度工程）
- 回归：svelte-check 0/0；46 测试 0 失败；npm run build 通过

## 切片 T-123（R-104）：showMsg 操作反馈消息收敛（已完成）

- 广域扫描发现 4 个组件（BackupManager/DailySummary/GroupMonitor/
  HookManager）各有同构的 `msg/msgOk + showMsg` 本地实现，延迟
  3.5-5s 不等、DailySummary 有条件清空
- 新增 `wechat/services/msg.svelte.ts` `createMsg(durationMs)`：
  统一 text/ok 状态 + 自动清空；`clearTimeout` 消除原实现竞态
  （快速连续消息时旧 timer 会提前清空新消息）
- 4 个组件收敛：删本地状态与 showMsg（每组件约 5 行），改
  `const msg = createMsg(延迟)`；模板引用改 `msg.state.text/ok`；
  44 处调用点 `showMsg(` → `msg.show(`
- 各组件保留原自动清空延迟（5000/3500/4000/3500ms）
- 回归：svelte-check 0/0；46 测试 0 失败；npm run build 通过

## 切片 T-124（R-105）：graphModel 本地 clamp 收敛共享工具（已完成）

- 扫描发现 `clamp` 两处定义：`wechat/graph/graphModel.ts` 本地
  私有实现与 `wechat/utils/index.ts` 共享实现语义等价
  （Math.min(Math.max) vs Math.max(Math.min)，数值钳制相同）
- graphModel 删除本地 clamp（3 行），import 共享版本；两处调用
  （力导向距离钳制）零改动
- 评估不收敛项：KnowledgeBase notify（多 toast 堆叠语义，与
  createMsg 单消息不同）、fmtNum 两处（万缩写 vs 千分位，同名
  不同义）、annual fmtNum vs summary fmtTokens（0 显示 vs 空）
- 回归：svelte-check 0/0；46 测试 0 失败；npm run build 通过

## 切片 T-125（R-106）：AGENTS.md 规范门禁固化（已完成）

- Development Commands 补充 `cargo fmt --check`（必须通过）与
  `cargo clippy --lib --no-default-features`（必须 0 警告），与
  本地已达成的基线一致
- svelte-check 门禁从「0 errors」更新为「0 errors and 0
  warnings」（R-98 起警告已清零）
- Testing Guidelines 同步「cargo clippy 0 warnings」要求
- 最终全门禁确认：fmt 0、clippy 0、check 双特性 0、cargo test
  211 passed、svelte-check 0/0、46 前端测试 0 失败、
  AGENTS.md 测试清单 44/44 同步

## 切片 T-126（R-107）：Rust 文档质量修复（cargo doc 0 警告）

- `cargo doc --lib --no-deps` 发现 3 处文档问题：
  - kb/wiki.rs：`[[链接]]` 被 rustdoc 当作未解析 intra-doc link
  - wechat/handlers/data.rs：`[图片]` 占位文本被当作 link 语法、
    `<audio>` 未闭合 HTML 标签
- 均用反引号包裹（`` `[[链接]]` `` / `` `[图片]` `` / `` `<audio>` ``）
  修复，rustdoc 不解析代码样式内容
- 回归：cargo doc 0 警告；fmt 0、clippy 0、check 双特性 0、
  cargo test 211 passed / 0 failed / 19 ignored

## 切片 T-127（R-108）：createMsg 上移共享层 + AiRolesPanel 收敛（已完成）

- 广域扫描发现 AiRolesPanel（llm 模块）的 toast/showToast 与
  wechat 已收敛的 createMsg 完全同构（单消息 + clearTimeout +
  2s 清空）——第 5 份重复
- `createMsg` 从 `wechat/services/msg.svelte.ts` 上移到
  `src/lib/services/msg.svelte.ts`（跨 feature 共享），原文件
  删除；4 个 wechat 组件 import 更新
- AiRolesPanel 收敛：删 toast 状态与 showToast（约 6 行），改
  `const toast = createMsg(2000)`；7 处调用点统一 `toast.show()`；
  模板改 `toast.state.text`
- 回归：svelte-check 0/0；46 测试 0 失败；npm run build 通过

## 切片 T-128（R-109）：安全 localStorage 工具共享化（已完成）

- 扫描发现 7 个文件直接操作 localStorage（WikiPanel 4 处、
  GlobalChatTab 2 处封装、WeChatPanel 2 处、GraphView/
  AiCopyPanel/PreferencesPanel/auth 等），均为 try/catch 安全
  读写重复模式
- 新增 `src/lib/storage.ts`：`lsGet`（不可用时返回 null）/
  `lsSet`（不可用时忽略），跨 feature 共享
- 收敛 3 个组件 8 处：GlobalChatTab 删本地 lsGet/lsSet 封装；
  WikiPanel 的 loadWikiSub/setWikiSub/loadGraphParams/
  saveGraphParams 内部改用共享（JSON 解析逻辑保留）；
  WeChatPanel loadPinnedCollapsed/togglePinnedCollapsed 改用共享
- 回归：svelte-check 0/0；46 测试 0 失败；npm run build 通过

## 切片 T-129（R-110）：剪贴板复制工具共享化（已完成）

- 扫描发现剪贴板写入重复模式：WeChatPanel 3 处、DbManager 4 处
  （复制整行 JSON/字段/表名/DDL）、其它 6 个组件各 1 处，均为
  `navigator.clipboard.writeText` + try/catch + 反馈
- 新增 `src/lib/clipboard.ts` `copyText(text): Promise<boolean>`
  （成功 true / 不支持或权限拒绝 false），调用方自定反馈
- WeChatPanel：copyTextToClipboard 改用 copyText（保留成功/失败
  showMgmtMsg）；2 处静默复制改 `void copyText(...)`
- DbManager：4 处复制函数改 `const ok = await copyText(...)` +
  成功/失败 notify（原 console.warn 省略，notify 已提示）
- 回归：svelte-check 0/0；46 测试 0 失败；npm run build 通过

## 切片 T-130（R-111）：浏览器文件下载工具共享化（已完成）

- 扫描发现 Blob 下载重复模式：GeneralRecords 与 PrivacyScan 各
  5 行 `Blob + createObjectURL + a + click + revoke`（导出 CSV）
- 新增 `src/lib/download.ts` `downloadBlob(blob, filename)`：
  自动触发下载并释放对象 URL，跨 feature 共享
- GeneralRecords / PrivacyScan 下载逻辑收敛为一行调用
  （文件名生成保留在调用方）；DbManager 的 downloadBlob 是
  Tauri 保存对话框（磁盘路径），非浏览器模式，不强行合并
- 回归：svelte-check 0/0；46 测试 0 失败；npm run build 通过

## 审计 R-111a：前后端 IPC 命令契约一致性（结论：契约完整，31 个冗余候选）

- 前端命令提取（修正泛型/双引号形式）：287 个 invoke 命令名
- 后端 generate_handler 注册：317 个
- **前端有后端未注册：0**（唯一命中为 ipc.ts 注释示例 `xxx`）——
  前端所有调用均注册，契约完整
- **已注册但前端未调用：31 个**（候选：agent_get、
  automation_debug_broadcast、automation_get_task、
  automation_update_reply_by_key、get_app_info、get_system_info、
  insert_event、kb_add_member/kb_change_password/kb_create_dir/
  kb_create_user/kb_delete_dir/kb_delete_user/kb_faq_delete/
  kb_faq_import/kb_faq_list/kb_get_acl/kb_highlight/kb_list_members
  等）
- 抽查确认前端 0 引用（get_app_info/kb_highlight/kb_faq_list/
  insert_event/agent_get）；Rust 侧存在定义与注册
- 处理：不批量删除（需逐一确认 Rust 内部调用与未来用途），
  作为后续清理候选记录；前端功能无缺失（287 调用全覆盖）

## 切片 T-131（R-112）：死 command 清理尝试与回滚（结论：保留）

- 尝试清理 27 个「注册但前端未调用」command（agent_get、
  get_app_info/get_system_info、kb 用户/成员/目录/FAQ/ACL 管理等）
- 评估修正：这些 command **不是单行转发**，而是完整业务实现
  （每个 20-40 行：权限检查 + 数据库操作），合计约 300 行；
  删除风险高（可能为前端未来功能/外部调用准备）
- 回滚：从 TEMP 备份恢复全部 8 个文件（含 lib.rs），确认
  fmt/clippy/check/test 全门禁恢复（touch 消除缓存假象后 0 警告）
- 教训：死代码判定不能只看「前端未调用」；脚本批量删除对大括号
  计数不可靠（Rust 原始字符串/JSON 宏干扰），已放弃脚本方案
- 结论：31 个候选 command 保留为「已实现后端 API」（前端未用），
  不建议机械删除；如需清理应逐个人工确认用途

## 切片 T-132（R-113）：重复类型定义收敛（目标 4：类型提示）

- 扫描发现 7 组同名 interface 各 2 处定义；其中 5 处可收敛：
  - AgentPanel.svelte 4 个本地 interface（AgentItem/ModelInfo
    完全同构、AiRole/KbSummary 为共享超集的子集，用法仅访问
    共有字段）→ import 共享类型（agents/services/ipc、
    llm/types、kb/kbTypes）
  - AskPanel.svelte 本地 AskCitation → wechat/types 共享版
    （共享版含索引签名，兼容）
- 不收敛：ChatMessage 两处为不同语义（微信消息 vs LLM 对话
  消息）、Props 各组件局部（正常模式）、AgentInput 两处同模块
  （表单/IPC 输入，后续评估）
- 回归：svelte-check 0/0；46 测试 0 失败；npm run build 通过

## 切片 T-133（R-114）：AgentInput 类型重复收敛（已完成）

- 上一轮标记的 agents 模块内 AgentInput 两处定义确认完全一致：
  agentForm.ts（表单数据）与 agents/services/ipc.ts（IPC 输入）
- ipc.ts 删除本地定义（11 行），import agentForm.ts 的共享
  AgentInput（无循环依赖：agentForm 不依赖 ipc）
- 回归：svelte-check 0/0；46 测试 0 失败；npm run build 通过

## 切片 T-134（R-115）：ToggleVariants 类型重复收敛（已完成）

- type alias 扫描发现 ToggleVariants 两处：toggle.svelte 导出与
  toggle-group.svelte 本地（同源 VariantProps<typeof
  toggleVariants>，完全一致）
- toggle-group 删除本地定义并 import toggle/index.ts 的共享
  ToggleVariants（顺带清理 unused VariantProps/toggleVariants
  import）
- View/Tab 等 type alias 为各组件局部枚举（不同值域），正常
  模式不收敛
- 回归：svelte-check 0/0；46 测试 0 失败；npm run build 通过

## 切片 T-135（R-116）：BotPanel 步骤状态机下沉 + 状态机 bug 修复

- BotPanel 的 `stepState`（约 30 行发送步骤状态机，闭包依赖
  traceMode/sendStage/sendError）提取为纯函数 `bot/steps.ts`
  （参数化 + 精准类型：TraceMode/SendStage/StepState/StepKey）
- 新增 smoke-bot-steps.mjs（22 断言：非 media 简化 / 推进顺序 /
  三类错误定位）
- **测试暴露真实 bug**：sendStage='done' 时 activeKey 误落
  'prep'，导致 done 状态下 upload/send 步骤误显 pending；
  修复为 done 时定位末步骤（全部完成）
- 顺带：BotPanel steps 数组声明为 `StepKey` 类型（消除
  string→StepKey 不匹配）
- 回归：svelte-check 0/0；47 测试（45 smoke + run-store +
  voice）0 失败；npm run build 通过；AGENTS.md 45/45 同步

## 切片 T-136（R-117）：ChartView 图表规范归一化下沉（已完成）

- ChartView 的 `normalize`（约 37 行图表 spec 归一化：三种数据
  描述兼容、饼图映射、退化单系列）提取为纯函数 `llm/chartSpec.ts`
  `normalizeChart` + 共享类型（Series/PieItem/NormalizedChart）
- 新增 smoke-chart-spec.mjs（14 断言：类型判定/字段兼容/NaN
  过滤/退化单系列/非数值转 0/非法输入默认）
- AiCopyPanel modelTypeOf 评估为一行薄封装，下沉收益低跳过
- 回归：svelte-check 0/0；48 测试（46 smoke + run-store +
  voice）0 失败；npm run build 通过；AGENTS.md 46/46 同步

## 切片 T-137（R-118）：attachmentsToParts 附件转换下沉（已完成）

- GlobalChatTab 的 `attachmentsToParts`（约 12 行 Attachment →
  ContentPart 映射）提取到 `llm/attachments.ts`（已有 Attachment
  转换模块，自然归属）
- GlobalChatTab 删本地实现，调用点改 `attachmentsToParts(attachments)`；
  清理未使用 ContentPart import
- smoke-attachments.mjs 扩展 4 断言（图片/文本/文件三种转换 +
  空列表）——10 → 14 断言
- computeAutoWidths 评估：依赖 DOM 测量（getComputedStyle/
  measureTextWidth），非纯函数，跳过
- 回归：svelte-check 0/0；48 测试 0 失败；npm run build 通过

## 审计 T-138（R-119）：候选评估 + 全门禁确认（无代码变更）

- 候选评估：AgentPanel blankForm 已用共享 createBlankAgentForm
  （无重复）；ApiHelpModal apiDebugUrl 为共享 buildApiDebugUrl
  薄封装（已收敛）；ChannelConfigDialog validate 为 7 行表单
  校验（提取收益低）；computeAutoWidths 依赖 DOM 测量（非纯）
- 全门禁确认：cargo fmt/clippy/check（双特性）/test/doc 全 0、
  svelte-check 0/0、48 前端测试 0 失败、build 通过、
  AGENTS.md 46/46 同步

## 切片 T-139（R-120）：GroupMonitor 监控匹配逻辑下沉（已完成）

- `matchMonitors`（约 32 行：keyword/regex/sender/media 四类
  规则匹配）提取为纯函数 `wechat/utils/panel.ts`（参数化 +
  MonitorRule 类型），hits 计数副作用分离到调用方
- GroupMonitor：删本地实现，MonitorItem 改为
  `extends MonitorRule { hits }`；调用处传 monitors + 遍历更新
  hits（语义等价）
- smoke-panel-utils.mjs 扩展 6 断言（多规则命中/媒体精确/
  未启用过滤/无命中/非法正则容错/空规则）——总断言 25 → 31
- 修正测试数据「3 条」→「3条」（空格导致正则未命中）
- 回归：svelte-check 0/0；48 测试 0 失败；npm run build 通过

## 切片 T-140（R-121）：GraphView 多跳邻居集合下沉（已完成）

- GraphView 的 `neighborSet`（约 20 行 BFS 多跳邻居）提取为
  纯函数 `wechat/utils/graphView.ts` `multiHopNeighbors(id,
  depth, neighbors)`（参数泛化为 `Iterable<string>`，兼容
  Map<string, Set|string[]>）
- GraphView 删本地实现，3 处调用改传 neighbors
- smoke-graph-view.mjs 扩展 5 断言（单跳/两跳/深度 0/未知节点/
  超深度收敛）——17 → 22 断言
- 回归：svelte-check 0/0；48 测试 0 失败；npm run build 通过

## 审计 T-141（R-122）：布局候选评估 + 全门禁确认（无代码变更）

- 评估 WikiPanel runForceLayout/applyPositions：核心布局计算已
  收敛到共享 `radialTreeLayout`（smoke-kb-graph-layout 已锁定）；
  剩余为状态编排（nodeTarget/nodeBase 动画）与 DOM 更新（SVG
  命令式操作），非纯函数，无需下沉
- 全门禁确认：cargo fmt/clippy 0、211 测试、svelte-check 0/0、
  48 前端测试、build 通过、AGENTS.md 46/46 同步

## 切片 T-142（R-123）：KbModal 同构弹窗收敛（KbDocs 9 处，已完成）

- 新增 `src/lib/kb/KbModal.svelte`：统一 mask 容器——role/aria-label/
  tabindex=-1、点击 target 自检关闭、Enter/空格/Escape 键盘关闭；内容
  （.kb-modal 及 hd/bd/ft）由调用方经 children snippet 提供；`open` 为
  显式 boolean
- KbDocs.svelte 9 处 mask+modal 同构弹窗（移动/重命名/批量移动/打标签/
  批量打标签/抓取网页/新建 Markdown/编辑分块/全屏预览）收敛为 `<KbModal>`，
  保留原 onClose busy 守卫
- 修复批量转换遗留：残留 mask 开标签行、onClose 双重花括号、`.kb-modal`
  缺显式闭合 `</div>`、尾部 `{/if}` 错位；`open={tagModal}` 等改为显式
  `!== null` 布尔表达式（KbModal prop 保持 boolean，避免对象穿透）
- 回归：svelte-check 0 errors / 0 warnings；48 测试 0 失败；
  npm run build 通过

## 切片 T-143（R-124）：KbModal 收敛扩展到 kb 模块其余组件（5 处，已完成）

- KnowledgeBase.svelte 3 处（新建/编辑/删除知识库）、KbActivity.svelte 1 处
  （处理日志）、KbChat.svelte 1 处（引用来源）同构 mask+modal 收敛为
  `<KbModal>`；各文件删除 1 行长行 mask 定义
- 保留原 onClose busy 守卫（createKbBusy/editKbBusy/delKbBusy）；
  `citeOpen`/`delKbTarget` 传 `!== null` 显式布尔；aria-label 细化
- 效果：`kb-modal-mask` 现仅存在于 KbModal.svelte 单点（4 组件 14 处弹窗统一）
- 回归：svelte-check 0 errors / 0 warnings；48 测试 0 失败；
  npm run build 通过

## 切片 T-144（R-125）：通用 Modal 壳组件收敛（components 3 弹窗，已完成）

- 新增 `src/lib/components/Modal.svelte`：统一 overlay+frame 壳——点击外部
  关闭 + frame stopPropagation、Escape 关闭、role/aria/tabindex；样式类经
  overlayClass/frameClass 注入，壳样式（modal-overlay/modal、st-overlay/
  st-frame）从消费方迁移至壳组件 scoped 样式（逐块一致，无行为差异）
- AgentDetailModal / ApiHelpModal（modal-overlay/modal 壳）与 SettingsModal
  （st-overlay/st-frame 壳）收敛：删除每处重复的 overlay/frame 定义与闭合；
  ApiHelpModal 保留 frameStyle 宽度覆盖（880px），SettingsModal 保留
  overlayRole="presentation" 与 aria-labelledby="st-title"
- 效果：弹窗交互逻辑单点承载，后续新弹窗可直接复用；三组件样式块各删
  21/21/23 行重复
- 注意：AiCopyPanel 的 cp-drawer 为抽屉（onkeydown 主动 stopPropagation），
  语义不同，未纳入
- 回归：svelte-check 0 errors / 0 warnings；48 测试 0 失败；
  npm run build 通过

## 切片 T-145（R-126）：前端类型注解 any 清零（已完成）

- DbManager.svelte「全选」列映射 `(c:any)` → 移除注解（columns 已为
  DbColumn[]，自动推断）
- 全库扫描：`as any`/`@ts-ignore`/`Record<string, any>` 等均 0；
  剩余唯一 `any` 词元为 HTML 属性 `step="any"`（非类型注解）
- 回归：svelte-check 0 errors / 0 warnings；48 测试 0 失败；
  npm run build 通过

## 切片 T-146（R-127）：kb 检索模式标签常量收敛（已完成）

- KbActivity / KbChat 各有一份完全相同的 `modeLabel`（hybrid/vector/bm25 →
  中文），收敛为 `kb/fileUtils.ts` 的 `MODE_LABEL`（与 STATUS_LABEL/
  SOURCE_LABEL 同族常量）；两组件删除本地定义，4 处模板引用改共享常量
- 保持调用方 `?? 原文` 回退语义（未知键不进入映射）
- smoke-kb-file-utils.mjs 增 2 断言（映射 + 未知键 undefined）
- 回归：svelte-check 0 errors / 0 warnings；48 测试 0 失败；
  npm run build 通过

## 审计 T-147（R-128）：Modal/KbModal 收敛运行期验证（CDP 20 断言通过）

- 打通运行验证链路：`cargo build`（默认特性，55s 增量）→ 启动
  st-control.exe（WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-
  port=9222）→ `npm run dev`（Vite:1420）→ CDP（9222/json/list）驱动断言
- 通用 Modal（T-144）12 项：设置弹窗 .st-overlay/.st-frame 渲染、
  role/aria（presentation/dialog/aria-modal/aria-labelledby=st-title）、
  壳样式随迁移生效（fixed/grid/z100）、Escape/点击外关闭、点击内不关闭；
  API 文档弹窗 .modal-overlay/.modal、frameStyle 880px 覆盖生效
- KbModal（T-142/143）8 项：新建知识库 .kb-modal-mask/.kb-modal 渲染、
  role="button"/aria-label/tabindex=-1、mask target 自检（点击卡片不关闭、
  点击 mask 本体关闭）、Escape 关闭、二次打开正常
- 结论：弹窗收敛行为与迁移前一致，运行期证据补齐（此前仅静态门禁）
- 说明：WeChatPanel 消息区纯函数（estimateMsgHeight/computePrefixSums/
  estimateVisibleCount/formatDividerTime）已全部共享化，剩余为 MessageRow/
  MessageList 组件化（蓝图 T-蓝图-7），验证链路已就绪可随时实施

## 切片 T-148（R-129）：消息虚拟滚动窗口/裁剪/分隔判断下沉（已完成）

- `wechat/utils/virtualList.ts` 新增纯函数（与原 WeChatPanel 逻辑逐项等价）：
  - `computeVisRange(count,totalEst,viewH,prefix,scrollTop,stickToBottom,
    buffer=24)`：虚拟滚动可视窗口（贴底覆盖末尾 / 非贴底二分定位 +
    topPad/bottomPad），并导出 `MSG_VIRTUAL_BUFFER`
  - `trimMessageWindow(messages,estH,maxKeep)`：内存上限裁剪，返回裁剪后
    数组与移除高度
  - `shouldShowDivider(prev,cur,thresholdMs=300)`：时间分隔判断
- WeChatPanel.svelte：visRange 改调 computeVisRange（保留 $derived.by 闭包，
  stickToBottom 声明在后不触发 TDZ）；trimMessages/needDivider 改调共享
  实现；删除本地 MSG_VIRTUAL_BUFFER 与不再使用的 upperBoundPrefixOf/
  estimateVisibleCount 导入
- smoke-virtual-list.mjs 增 15 断言 → 34 项（空窗口/非贴底定位/topPad/
  bottomPad/贴底窗口/裁剪语义/分隔边界）
- 运行期验证：CDP 切换会话，消息列表正常渲染（20 行 data-idx 0..19 连续、
  全行含内容分支、18 分隔条、无错误提示）
- 说明：为 T-蓝图-7 MessageList 组件化铺路（窗口计算已纯化）；完整
  MessageRow 提取因 wc-msg-* scoped CSS 含大量动态类（wc-file-tone-*、
  wc-pay-*）与多选择器规则交织，留待专门回合推进

## 切片 T-28：图边端点类型修正（已完成）

- 发现类型错误：`GEdge.source/target` 声明为 `string`，但 d3-force 运行时会
  就地替换为节点对象——修正为 `GEdgeEndpoint = string | GNode` 联合类型
- `graphModel` 邻接表构建、`RelationshipGraph` 边 key 适配联合类型；
  `GraphCanvas` 消除 6 处冗余 `as any`（typeof 收窄生效）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-27：DailySummary 操作函数/IPC 类型化（已完成）

- `wechat/types.ts` 新增 `GroupMembersResult`；`ipc.ts`：
  `getGroupMembers` 返回 `Promise<GroupMembersResult>`（原 `{members:any[]}`）、
  `listDailySummaryRecords` 返回 `Promise<DailySummaryRecord[]>`（原 `any[]`）
- `DailySummary`：`editTask`/`toggleTask`/`copyRecord` 参数精确化；
  `toggleTask` 补 `t.id` 守卫
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-26：RelationshipGraph 图谱数据/增量块类型化（已完成）

- `graphModel.ts` 新增 `GraphRawData`（nodes/self/group_names/summary）与
  `GraphChunk`（增量块）
- `RelationshipGraph`：`toGraphData`/`applyData`/`mergeChunk` 参数从 `any`
  精确化；`mergeChunk` 的 kind 比较转 string（类型收窄后比较警告）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-25：WeChatPanel 收藏/批量选择回调类型化（已完成）

- 消息索引查找/去重/预加载回调约 8 处 `any` → `WeChatMessage`
- 会话批量选择/清草稿回调 → `WeChatSession`；收藏筛选 → `FavoriteEntry[]`
- 修正 `preloadAvatars` 类型守卫 4 处；`favIcon(f.type_label)` 可空兜底
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-24：WeChatPanel 会话/消息回调类型化（已完成）

- 会话筛选/分组/置顶/草稿计数/合并排序回调约 15 处 `any` → `WeChatSession`
- 消息发送者预加载回调 `WeChatMessage`；`checkupRatePct` 参数结构类型化；
  朋友圈查看器回调消除 `any`（MomentEntry 推断）
- 修正 `preloadAvatars` 类型守卫 2 处
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-23：CDN 状态返回类型化（已完成）

- `wechat/types.ts` 新增 `CdnImageStatus`（enabled/localDecrypt）
- `ipc.ts`：`getCdnImageStatus` 返回 `Promise<CdnImageStatus>`（原 `any`）；
  `WeChatConfig` 的 `loadCdnStatus` 去除 `const s: any`
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-22：记录列表 IPC 返回类型化（已完成）

- `wechat/types.ts` 新增 `RecordListItem`（session_name/msg_local_id/时间字段等
  10+ 模板实际字段）与 `RecordListResult`
- `ipc.ts`：6 个 `list*Records` 函数返回 `Promise<RecordListResult>`（原 `any`）；
  `GeneralRecords` 的 `items`/`cmdMap`/`r` 类型化（约 8 处 `any` 消除）
- `fmtTime` 接受 string 型时间戳；`onopen` 模板可空兜底
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-21：KbDocs 文档操作结果类型化（已完成）

- `kbTypes.ts` 新增 `UploadDocumentResult`（duplicateDocId/duplicateTitle）、
  `DownloadDocumentResult`（dataBase64/fileName）、`FetchUrlResult`（title）
- `kb/ipc.ts`：`uploadDocument`/`downloadDocument`/`fetchUrl` 返回精确类型
  （原 `unknown`）；KbDocs 4 处 `res: any` 去除（推断类型）
- 修复 `a.download = res.fileName` 可空（`?? ''`）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-20：AutomationPanel 实时消息类型化（已完成）

- `automation/display.ts` 新增 `LiveMessage`（MessageLike + automationHit/
  sender_username/timestamp/ruleName 等面板字段）
- `AutomationPanel`：`liveMsgs` → `LiveMessage[]`；`msgType`/`avatarText`/
  `typeColor`/`typeLabel` 参数精确化（原 `any`）；模板 `fmtTs(m.timestamp)`
  可空兜底
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-19：DbManager 表格/事件回调类型化（已完成）

- `db/types.ts` 新增 `DbEvent`（timestamp/event_type/title）
- `DbManager`：CSV 导出（当前页/选中行）回调、CRUD 表单构建回调、
  crudColumns 过滤统一用 `DbColumn`/`DbRow`（约 12 处 `any` 消除）；
  `dbEvents` → `DbEvent[]`
- `db/ipc.ts`：`queryEvents` 返回 `Promise<DbEvent[]>`（原 `unknown[]`）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-18：GraphView 图谱数据状态类型化（已完成）

- GraphView 定义 `SimNode`（GNode + 运行时位置/速度 + 后端 snake_case 字段）
  与 `SimEdge`；Props（nodes/edges/onSelect/onOpen）、拖拽/按下/右键菜单
  状态、`simNodes`/`simEdges` 全部从 `any` 精确化（约 15 处）
- 修复模板右键菜单 `m.node` 可空问题（非空断言，`{#if}` 已守卫）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-17：汇总/密钥 IPC 返回类型化（已完成）

- `wechat/types.ts` 新增 `DailySummaryFormats`、`AnnualSummaryData`、
  `AutoDbKeyResult`（复用）、`AutoImgKeyResult`（复用）
- `ipc.ts`：`getDailySummaryFormats`/`getAnnualSummary`/`autoGetDbKey`/
  `autoGetImageKey` 返回类型精确化（原 `any`）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-16：收藏列表返回类型修正（已完成）

- 发现类型声明错误：`getFavorites()` 声明返回 `FavoriteEntry[]`，
  但运行时与消费方均为 `{ items, tags }`——新增 `FavoritesData` 类型并修正
- `ipc.ts`：`getFavorites(): Promise<FavoritesData>`；
  `WeChatPanel.favData` → `FavoritesData`（原 `any`）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-15：通讯录分页状态类型化（已完成）

- `wechat/types.ts` 新增 `ContactItem`（username/display_name/nick_name/remark/
  alias/category/member_count/initial 等通讯录字段）
- `WeChatPanel`：`contactsPage.items` → `ContactItem[]`；分页追加/过滤/
  拼音分组回调消除 `any`（约 6 处）；`preloadAvatars` 类型守卫
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-14：微信消息搜索返回类型化（已完成）

- `search/types.ts` 新增 `WechatSearchHit`（name/username/time/text）与
  `WechatSearchResult`（hits/indexed）
- `ipc.ts`：`searchWechatMessages` 返回 `Promise<WechatSearchResult>`（原 `any`）
- `GlobalSearch`：`wxHits` → `WechatSearchHit[]`；模板 `hit.text` 可空兜底
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-13：密钥信息返回类型化（已完成）

- `wechat/types.ts` 新增 `WechatKeysInfo`（keyFormat/keyCount）
- `ipc.ts`：`getWechatKeysInfo` 返回 `Promise<WechatKeysInfo>`（原 `any`）
- `WeChatConfig`：`keysInfo` → `WechatKeysInfo | null`；修复模板
  `keysInfo?.keyCount > 0` 可空比较
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-12：WeChatConfig 检测结果类型化（已完成）

- `wechat/types.ts` 新增 `DetectedAccount`（db_dir/wxid/last_active）、
  `AutoDbKeyResult`（key/db_dir/valid/total/errors）、`AutoImgKeyResult`
- `WeChatConfig.svelte`：`detectedAccounts` → `DetectedAccount[]`；
  `useDetectedAccount`/`applyAutoDbResult`/`applyAutoImgResult` 参数精确化
- 修复 `fmtLastActive(acc.last_active)` 可空问题
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-11：DailySummary 数据结构类型化（已完成）

- `wechat/types.ts` 新增 `DailySummaryTask`（含模板实际字段）、
  `DailySummaryRecord`（status/summary/error/tokens 等）、`ProviderOption`
- `DailySummary.svelte`：6 个 `any[]` 状态全部精确化（tasks/groups/members/
  formats/providers/records）；`targetLabel`/`editTask`/`toggleTask`/
  `runTask`/`deleteRecord` 参数类型化 + 可空守卫
- 修复 10+ 处类型化暴露的可空参数问题
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-10：图片体检数据结构类型化（已完成）

- `wechat/types.ts` 新增 `MissingImagesData`（total_images/local_ok/cdn_possible/
  missing/chats[username,name,missing,total_images]）
- `WeChatPanel`：`missingImagesData` 从 `any` 改为 `MissingImagesData | null`；
  checkupChats 派生的筛选/排序回调消除 `any`（约 10 处）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-9：全局搜索状态类型化（已完成）

- 新增 `src/lib/search/types.ts`：`ContactHit`（通讯录命中字段）、`SearchEvent`
- `GlobalSearch.svelte`：`contacts`/`contactHits` → `ContactHit[]`，
  `events`/`eventHits` → `SearchEvent[]`（原 `any[]`）
- `search/ipc.ts`：`queryEvents` 返回 `Promise<SearchEvent[]>`
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-8：朋友圈/表情状态类型化（已完成）

- `momentsPage.items`：`any[]` → `MomentEntry[]`（模板获得朋友圈字段精确类型）
- `emoticons`：`any` → `EmoticonOverview`（packages/custom/store_files）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-7：微信配置返回类型化（已完成）

- `wechat/types.ts` 新增 `WechatConfigData`（db_dir/api_port/api_token 等配置字段）
  与 `WechatConfigResult`（configPath/config/raw）
- `ipc.ts`：`getWechatConfig(): Promise<WechatConfigResult>`（原 `Promise<any>`）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-6：查看器/文件/小程序操作函数类型化（已完成）

- `openMomentViewer(m: MomentEntry)`（复用既有朋友圈类型）、
  `openFileDir(m: WeChatMessage)`、`copyMiniAppInfo(r: RichMedia)`、
  `openImageViewer(m: WeChatMessage)`（原均 `any`）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-5：消息/会话操作函数类型化（已完成）

- `clearDraft(s: WeChatSession)`、`selectSession(s: WeChatSession | string)`、
  `loadMessages(...): Promise<MessagePage>`（原 `any`）
- `MessagePage.messages` 类型修正为 `WeChatMessage[]`（与后端实际返回
  及 WeChatPanel 消费形状一致；原误标 `ChatMessage[]`）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-4：实时消息事件载荷类型化（已完成）

- `WeChatMessagePayload` 补全实时消息字段（sort_seq/is_send/is_group/image_url/
  rich/time/sender_username 等）
- `WeChatPanel.toRealtimeMsg`：`(m: any): any` → `(m: WeChatMessagePayload):
  WeChatMessage`；`mergeSessionUpdate`/`appendRealtimeMessage` 参数类型化
- 修正两处 `.filter(Boolean)` 产生的 `(string|undefined)[]`（改类型守卫）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-3：媒体 IPC 返回类型化（已完成）

- `wechat/types.ts` 新增 `MediaResult`（kind/data/file_key/error 等媒体结果形态）
- `ipc.ts`：`getMessageImage`/`getMomentImage`/`getMomentVideo` 返回
  `Promise<MediaResult>`（原 `Promise<any>`）；既有冒烟测试 mock 形态兼容
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 D-37：safeParseInt 重复实现消除（已完成）

- `WeChatConfig.svelte` 本地 `safeParseInt` 与 `wechat/utils/index.ts` 共享实现
  语义逐项等价（数字不截断、字符串截断、钳制、fallback）——删除本地副本
- `smoke-wechat-misc.mjs` 扩展至 25 断言（锁定收敛后行为）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十五个冒烟/单元测试全部通过

## 切片 T-2：sessionMap / DbManager 表格数据结构类型化（已完成）

- `WeChatPanel.sessionMap`：`Map<string, any>` → `Map<string, WeChatSession>`，
  mergeSessionUpdate 更新对象显式断言
- 新增 `src/lib/db/types.ts`：`DbColumn`（列元信息）、`DbRow`（动态字段行 +
  rowid）、`DbTableData`（分页结果）
- `DbManager.dbTableData`：内联匿名类型 → `DbTableData`；
  模板 col.col_type/rowid 可空处理、行值插值 String() 转换
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十四个冒烟/单元测试全部通过

## 切片 T-1：WeChatPanel 消息/会话类型提示强化（已完成）

- `wechat/types.ts` 新增 `WeChatMessage`（精确字段：type/local_id/text/rich 等）、
  `WeChatSession`、`RichMediaItem`；`RichMedia` 按模板实际访问补全 20+ 已知字段
- `WeChatPanel.svelte`：`messages`/`sessions` 从 `any[]` 改为强类型；
  `setMessages`/`appendMessages`/`prependMessages`/`rebuildSessionMap` 参数类型化；
  `openFileMsg`/`openMiniApp`/`playVoice`/`transcribeVoice` 签名精确化
- 模板富媒体访问用 `?? []`/`?? ''` 兜底（消除 unknown 传播）；
  格式化函数（fmtFileSize/fmtDur/extTone）参数放宽以匹配调用方
- 过程：类型化暴露 15 处原本被 `any` 掩盖的真实模板类型问题，全部修复
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十四个冒烟/单元测试全部通过

## 切片 D-35：BackupManager / KnowledgeBase 格式化收敛（已完成）

- `BackupManager.fmtSize` → `formatBytes({gbPrecision:2})`（原实现与 KbDashboard
  语义一致）；`fmtDate` → `formatTs({showYear:true})`（秒级时间戳）
- `KnowledgeBase.fmtSessionTime` → `formatIsoTime({showYear:false})`（与 KbChat
  格式一致的 MM-DD HH:mm）
- `smoke-format-bytes.mjs` 扩展至 29 断言（含收敛行为锁定）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  三十四个冒烟/单元测试全部通过

## 切片 D-26：cssColorToHex 重复实现消除（已完成）

- `components/colorUtils.ts` 新增 `cssColorToHex`（canvas 采样任意 CSS 颜色 → hex）
- `ThemeFlickeringGrid.svelte` 与 `AnimatedBackground.svelte` 的本地实现
  （逐字符一致）统一改从共享模块导入
- `smoke-color-utils.mjs` 扩展至 10 断言（mock canvas 锁定输出格式）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  二十六个冒烟/单元测试全部通过

## 切片 D-23：KbDashboard 首字母/趋势展示下沉（已完成）

- `kb/fileUtils.ts` 新增 `kbMonogram`、`trendArrow`、`trendClass`
- `KbDashboard.svelte` 删除本地实现（3 个函数）
- `smoke-kb-file-utils.mjs` 扩展至 24 断言（含 emoji 首代理项的历史行为锁定）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  二十四个冒烟/单元测试全部通过

## 切片 D-16：fmtTime/fmtTs 时间格式化收敛（已完成）

- `src/lib/format.ts` 新增 `formatIsoTime`/`formatTs`/`formatDate`/`tsToDate`：
  ISO 解析、秒/毫秒/微秒时间戳自适应、年份显示与 zh-CN locale 均可参数化
- 三处重复实现收敛：KbDocs（`{showYear:true}`）、KbChat（`{showYear:false}`）、
  AutomationPanel（`{showYear:false,useLocale:true}`，fmtTs 复用时间戳自适应）——
  各组件输出逐值保持
- `smoke-format-bytes.mjs` 扩展至 23 断言（三种风格 + 时间戳三档位）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  二十个冒烟/单元测试全部通过

## 切片 D-15：AI 角色归一化/默认值下沉（已完成）

- `roleUtils.ts` 新增 `normalizeRole`（null → 空串/空数组，深拷贝不改原对象）、
  `createEmptyRole`（表单默认值）
- `AiRolesPanel.svelte` 删除本地 `norm`/`emptyRole`，三处调用改调共享函数
- `smoke-role-utils.mjs` 扩展至 15 断言（深拷贝语义/默认值）
- 回归：svelte-check 0 errors / 176 warnings；构建通过；
  二十个冒烟/单元测试全部通过

## 修复：独立运行 exe 加载 devUrl 导致导航失效与界面错乱（已完成）

- 根因：`Cargo.toml` 中 `tauri` 未启用 `custom-protocol` feature，`tauri-build`
  据此判定为 dev 模式；即使 `cargo build --release`，exe 仍加载
  `devUrl: http://localhost:1420`。不启动 Vite 时 WebView2 报连接拒绝，
  页面空白 → 表现为点击导航标题不跳转、界面乱放。
- 修复：`Cargo.toml` 新增 `custom-protocol = ["tauri/custom-protocol"]` feature；
  独立 exe 用 `cargo build --release --features custom-protocol` 或
  `npm run tauri build`（CLI 自动注入）构建，普通 `cargo build` 仍是 dev 模式。
- 新增回归脚本 `.codex_tests/verify-nav-cdp.mjs`：CDP 驱动依次点击 13 个导航项，
  断言 active 切换、可见面板恰为 1、侧边栏/主区无重叠，并调用真实后端 IPC。
- 验证：独立 release exe 60 项断言通过（页面来源 tauri://，非 devUrl）；
  dev 模式（Vite + debug exe）59 项断言通过；`cargo test --lib --no-default-features`
  211 passed / 0 failed；AGENTS.md 运行注意事项同步更正。

## 切片 T-149（R-130）：MessageRow 消息行组件化（蓝图 T-蓝图-7 第一步，已完成）

- 新增 `wechat/components/MessageRow.svelte`：消息行渲染纯化——
  - props 契约：`m / gi / divider / ctx（分组只读上下文）/ actions（15 项分组回调）`；
    组件不持有任何状态，交互一律经 actions 回调（含图片/文件/链接/语音/视频/
    编辑器/右键菜单等），状态 map 仍归 WeChatPanel 持有（$state 代理透传）
  - 模板：时间分隔条 / 系统通知 / 头像 / 发送者 / 文本 / 图片（含失效重试）/
    富媒体全分支（newsfeed/file/miniapp/link/quote/transfer/redpacket/location/
    contact/voice/video/emoji/channels/chatlog/兜底）/ 已编辑徽标，逐字迁移
  - scoped CSS：wc-msg-* / wc-rich-* / wc-article-* / wc-file-* / wc-transfer-* /
    wc-redpacket-* / wc-news-* / wc-voice-* / wc-video-* / wc-msg-image-* 等约 198 条
    规则随模板迁移（规则文本全等比对后从父组件删除，杜绝漏删/误删）
- `resolveStaticEmojiPath` 下沉为 `wechat/utils/format.ts` 纯函数
  （参数化 emojiMap，消除组件本地实现）
- WeChatPanel：each 循环收敛为 `<MessageRow {m} {gi} divider={...} ctx={rowCtx} actions={rowActions} />`，
  新增 `rowCtx`（$derived.by 惰性求值避免后置 $state 的 TDZ）与 `rowActions`（回调分组）；
  删除消息行模板约 370 行 + 迁移 CSS 约 247 行 + 死 CSS 约 67 行；
  模板减少约 370 行（5962 → 5311 行）
- 顺带清理：随迁移失效的 WeChatPanel 死 CSS（.wc-edit-menu-item/.wc-nav-item/
  .wc-batch-btn/.wc-fav-img 等 67 条 svelte-check 未使用告警）一并删除
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过；CDP 运行期 15 项断言通过（20 行 data-idx 连续、6 时间分隔、
  scoped CSS 生效：display=flex/头像 36px/body 66%、右键编辑菜单弹出与关闭）
- 说明：MessageRow 状态/交互经 ctx/actions 注入的契约已稳定，后续可直接复用；
  下一步为蓝图 T-蓝图-7 第二步 MessageList.svelte（滚动容器 + 虚拟滚动状态下沉）

## 修复：主面板同屏堆叠——所有功能界面叠在一个面板上（已完成）

- 症状：13 个功能面板全部同时渲染/堆叠，界面整体纵向溢出（wc-root 4372px vs
  视口 1000px），点导航只改 active 类、视觉上不切换。
- 根因：Svelte 5 作用域隔离不再把父组件作用域类附加到子组件根元素；
  `PanelSection.svelte`（无自身 `<style>`）的 `<section class="panel ...">`
  根节点不带任何 s-* 作用域类，导致 App.svelte 中 scoped 的
  `.panel { height:100% }` 与 `.panel-hidden { display:none !important }`
  从未命中——面板高度不受约束、隐藏面板依然可见。
  此前 CDP 验证只按「类名存在」计数可见面板，未校验真实 display，故未暴露。
- 修复：把 `.panel` / `.panel-full` / `.panel-hidden` 三条结构样式移入
  `PanelSection.svelte` 自身 `<style>`（根元素获得本组件作用域类后正常命中）；
  App.svelte 删除这三条失效 scoped 规则。
- 回归加固：`.codex_tests/verify-nav-cdp.mjs` 新增断言——隐藏面板必须真实
  `display:none`（getComputedStyle），且隐藏面板数恒为 12；
  CDP 实测 13 面板仅 1 可见、活动面板 930px 贴合视口、无纵向溢出。
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过；导航/布局 CDP 85 项断言通过（含真实显隐）。

## 切片 T-150（R-131）：MessageList 列表容器组件化（蓝图 T-蓝图-7 第二步，已完成）

- 新增 `wechat/components/MessageList.svelte`：消息列表容器 + 虚拟滚动机制下沉
  - props：`messages / loading / error / hasMore / curSession / officialHistory /
    rowCtx / rowActions / onLoadMore（Promise<boolean>）/ onOpenUrl / onVisibleChange`
  - 内部持有：msgEstH / msgPrefix / msgTotalEst / msgScrollTop / msgViewH /
    stickToBottom / restoringScroll / msgsEl / msgsInnerEl / msgTopSentinelEl /
    visRange / visibleMsgs / 校准防抖 / 双 rAF 吸底 / ResizeObserver 守护 /
    IntersectionObserver 顶部哨兵懒加载 / needDivider
  - 对外方法（bind:this）：setMessages / appendMessages / prependMessages /
    clearMessages / updateEstimate / scrollToBottom / setStickToBottom /
    isStickToBottom / getScrollTop / restorePosition / scrollToIdx / loadMore
  - 消息维护与滚动恢复的时序：父组件更新 messages 后同步调用 metrics 方法，
    方法返回裁剪后数组回写父组件（避免跨组件 props 同帧读取的旧值问题）
- WeChatPanel：消息容器模板收敛为 `<MessageList bind:this={msgListRef} .../>`；
  删除约 110 行虚拟滚动/滚动机制代码；图片懒加载预检改经 onVisibleChange
  回填 msgVisibleWindow；loadMoreMsgs 移交滚动恢复、返回 boolean 表示是否
  可恢复滚动；jumpToDay / openRecordSession / tryJumpToMessage / 编辑流程
  改调 msgListRef 方法
- 迁移 scoped CSS：.wc-msgs / .wc-virtual-pad / .wc-msg-top-* 等移入 MessageList；
  .wc-empty / .wc-error-* / .wc-official-empty-* 因其它面板仍用，保留父组件
  副本并在 MessageList 复制一份（scoped 各自生效）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过；CDP 运行期 27 项断言通过（微信子页签排他切换、
  面板高度受控 930px、消息列表渲染 20 行 data-idx 连续）

## 切片 T-151（R-132）：剪贴板写入全量收敛共享 copyText（已完成）

- 扫描发现 7 处残留的裸 `navigator.clipboard.writeText` 调用
  （T-129 已收敛 3 组件 + DbManager，本切片收尾其余）：
  - ApiHelpModal（内联复制 token，无反馈）
  - AiCopyPanel.copyOutput（try/catch + copied/error 反馈）
  - AiRolesPanel.copyPrompt（.then/.catch + toast 反馈）
  - DailySummary.copyRecord（try/catch + msg.show 反馈）
  - GraphView 本地 copyText（静默 + 关菜单；重命名为 copyGraphText，
    两处调用点更新，避免与共享导入同名）
  - WeChatConfig.copyApiToken（try/catch + apiApplyResult 反馈 + logError）
  - WeChatPanel.copyMiniAppInfo（try/catch + showMgmtMsg 反馈）
- 全部改调 `src/lib/clipboard.ts` 的共享 `copyText`（内部 try/catch，
  返回 boolean），各调用方保留原反馈语义；错误日志语义收敛为
  「返回 false → 提示复制失败」（不再重复 logError）
- 效果：`navigator.clipboard` 裸调用仅存在于 clipboard.ts 单点；
  7 个文件新增/复用共享导入
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过

## 切片 T-152（R-133）：浏览器文件下载全量收敛共享 downloadBlob（已完成）

- 扫描发现 4 处残留的 Blob + createObjectURL + a.click 下载实现
  （T-130 已收敛 GeneralRecords / PrivacyScan，本切片收尾其余）：
  - KbDocs.downloadDoc（单文档下载）
  - KbDocs.batchDownload（批量打包 zip）
  - KbTrendChart.exportData（趋势 CSV，UTF-8 BOM）
  - WeChatPanel.exportSettingsCat（分类 CSV，UTF-8 BOM）
- 全部改调 `src/lib/download.ts` 的共享 `downloadBlob(blob, filename)`；
  文件名生成逻辑保留在调用方
- 保留项：KbDocs 391/393 的 `createObjectURL` 是文档预览（img/pdf 内联
  展示，非下载），不收敛；DbManager.downloadBlob 是 Tauri 保存对话框
  （磁盘路径语义），与浏览器下载不同，不强行合并
- 效果：浏览器下载模式（Blob + a.click）仅存在于 download.ts 单点
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过

## 切片 T-153（R-134）：前端 any 注解复查清零（已完成）

- 全库扫描 `: any / as any / any[] / Record<string, any> / <any> / @ts-ignore /
  catch(e: any)`，发现 WeChatConfig.svelte 3 处 `catch(e: any)`
  （T-145 曾清零，此为复查补漏）
- 三处 catch 体均只使用 `${e}` 模板与 logError（参数为 unknown），
  改为 `catch(e: unknown)` 后语义不变
- 效果：前端 src 内 any 相关注解 0 处
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过

## 切片 T-154（R-135）：Rust 通用纯函数共享化（describe_reqwest_error / truncate）

- 跨模块函数扫描发现重复：
  - `describe_reqwest_error`：bot/ilink/cdn.rs 与 bot/channels.rs 逐字相同
    （完整 cause 链展开，` ← ` 分隔）；ocr/textin.rs 为带深度上限 4 且
    ` <- ` 分隔的有意变体，保留不收敛
  - `truncate`：bot/ilink/auth.rs / cdn.rs / client.rs / bot/channels.rs
    4 处逐字相同（按 char 截断 + 省略号）
- 新增 `src-tauri/src/common.rs`：`describe_reqwest_error` + `truncate`
  两个跨 feature 纯函数；lib.rs 注册 `mod common`
- 4 个调用文件删除本地副本并 `use crate::common::...`；
  cdn.rs / channels.rs 顺带清理失效的 `use std::error::Error` 导入
- 新增 4 个 truncate 单测（短串原样 / 超长省略号 / 按 char 非字节 /
  空串），cargo test 211 → 215
- 回归：cargo fmt --check 0、clippy --lib --no-default-features 0 警告、
  cargo check（双特性）通过、cargo test 215 passed / 0 failed / 19 ignored、
  cargo doc 0 警告

## 切片 T-155（R-136）：应用数据目录 st_data_dir 收敛（11 处重复消除）

- 扫描发现 `%APPDATA%/st-control` 基目录构造
  （`dirs::data_dir().unwrap_or_else(|| ".") + push("st-control")`）在
  Rust 侧重复 11 处：automation/mod.rs（control.db）、kb/auth.rs
  （kb_session.json）、kb/db.rs（knowledge_base.db）、ocr/db.rs ×2
  （control.db / ocr 存储根）、ocr/rapid.rs（rapidocr-models）、
  wechat/insights.rs（relationship_graph.json）、db.rs（control.db）、
  llm/config.rs（config_dir）、stt/mod.rs（config_dir）、lib.rs
  （bot_data_dir）
- `common.rs` 新增 `pub fn st_data_dir()`（行为与原逐字等价，含
  unwrap_or_else 回退语义）；11 处改为 `st_data_dir().join(...)` 或
  直接返回
- 保留项：`st_result` 目录（微信解密结果）为另一套语义，其中 llm/stt
  的 `unwrap_or_else` 链与 wechat/modules 的 `unwrap()` 变体回退语义
  不同，未强行合并，留作后续候选
- 回归：cargo fmt --check 0、clippy --lib --no-default-features 0 警告、
  cargo check（双特性）通过、cargo test 215 passed / 0 failed /
  19 ignored（连续 7 轮确认；早期一轮偶发 1 failed 为真实数据冒烟测试
  与运行中应用争用解密库所致的环境性抖动，与本次路径收敛无关）、
  cargo doc 0 警告

## 切片 T-156（R-137）：st_result 语音缓存目录收敛（3 处重复消除）

- `%APPDATA%/st_result/decoded_images/voices` 目录构造在 llm/client.rs、
  stt/mod.rs、llm/handlers.rs 三处逐字相同（均为 #[ignore] 实网冒烟测试）
- `common.rs` 新增 `st_result_dir()`（与 st_data_dir 同一回退语义）；
  三处改为 `st_result_dir().join("decoded_images").join("voices")`
- 因调用点全在 `#[cfg(test)]`，共享函数标记 `#[cfg(test)] pub(crate)`，
  非测试构建不编译，避免 dead_code 告警
- 保留项：wechat/modules 的 emoticons/files 测试用
  `data_dir().unwrap().join("st_result")`（unwrap 语义不同，且均为
  ignored 实库测试），不强行合并
- 回归：cargo fmt --check 0、clippy --lib --no-default-features 0 警告、
  cargo check（双特性）通过、cargo test（双特性）215 passed / 0 failed

## 切片 T-157（R-138）：删除死代码 GraphView 组件与孤儿工具（约 1200 行）

- 全仓引用扫描确认 `wechat/components/GraphView.svelte`（1199 行）为
  fancyui 迁移遗留的死组件：微信图谱页实际使用 RelationshipGraph +
  GraphCanvas（GraphView 未在任何生产代码/动态导入/桶文件中被引用）
- 其专属工具 `wechat/utils/graphView.ts`（numT/nodeT/edgeT/fmtTime/
  clamp01/multiHopNeighbors/timelineBounds/searchNodes）也仅被该组件
  引用（含 T-140 下沉的 multiHopNeighbors），无生产使用方；GraphCanvas
  仅做单跳邻居高亮，与 multiHopNeighbors 语义不同，不强行收敛
- 删除：GraphView.svelte、wechat/utils/graphView.ts、对应冒烟测试
  smoke-graph-view.mjs；AGENTS.md 测试清单同步移除（48 → 47）；
  format.ts 失效注释（GraphView 时间轴刻度）更新为通用描述
- 回归：svelte-check 0 errors / 0 warnings；47 前端测试 0 失败；
  `npm run build` 通过

## 切片 T-158（R-139）：事件总线裸 invoke 收敛到类型化 IPC（已完成）

- 扫描发现 `wechat/events/index.ts` 3 处绕过服务层直接 `invoke('...')`：
  `ack_wechat_message`（WebSocket 回退单条 ACK）、`resync_wechat_messages`
  （断线补拉）、`get_wechat_monitor_status`（看门狗探活）
- 收敛：新增 `resyncWechatMessages` 类型化封装，events 全部改调
  `ackWechatMessage / resyncWechatMessages / getMonitorStatus`，删除
  裸 invoke 与 `@tauri-apps/api/core` 导入
- 说明（修正）：初判「传 ackId/sinceAckId 键名不匹配导致功能失效」有误——
  Tauri 2 会把前端 camelCase 参数自动转换为 Rust snake_case 参数名
  （证据：get_conversation_messages 的 page_size/before_sort_seq 与前端
  pageSize/beforeSortSeq 长期正常工作），原裸调用并无运行时 bug；
  本切片价值在于统一类型化封装（精准类型提示 + 单一 IPC 入口），
  不涉及行为修复
- 回归：svelte-check 0 errors / 0 warnings；47 前端测试 0 失败；
  `npm run build` 通过

## 审计 T-159（R-140）：IPC 参数键名契约全量审计（结论：一致，无运行时缺陷）

- 方法：提取 305 个 Rust `#[tauri::command]` 的非注入参数名（过滤
  State/Arc/Window/AppHandle），对照前端 260 处 `invoke('cmd', {keys})`
  实参键集合，逐命令比对
- 发现 4 处疑似不一致，逐一人工复核：
  - `send_command_to_agent`：脚本把 `crate::ws_server::WsServer` 类型误读为
    参数名（误报）；实际参数为 `args: SendCommandArgs`（camelCase 结构体）
  - `kb_rag_stream`：前端 `{ input, onChunk }`，Rust `input + on_chunk`
    （Channel）——Tauri 自动转换，一致
  - `bot_start_qr`：前端 `{ accountId }`，Rust `account_id`——Tauri 自动
    转换，一致（与 get_conversation_messages 的 pageSize→page_size 同证）
  - ApiHelpModal 文档示例字符串，非真实调用
- 结论：前端 260 处 invoke 实参键与 Rust 命令参数契约一致（含 Tauri 的
  camelCase→snake_case 自动转换），无键名导致的运行时缺陷；
  同时修正 T-158 中基于误判的「bug 修复」表述
- 后续候选：可把该比对固化为脚本化门禁（需按 Tauri 转换规则归一化），
  暂不落地

## 外部项目验证：wechat_image（ilink 官方通道取原图）在本机 4.1.12.26 端到端成功

- 背景：E:\wechat_image 是「微信原图本地下载 PoC」（Rust + C++ 桥接），
  原理为加载微信官方 ilink2.dll，用本机登录态调用 C2CDownloadOrigin 从
  CDN 取原图（不注入/不 Hook）；README 仅验证过 4.1.11.24，本机为
  4.1.12.26，需实测兼容性
- 本机验证结果（全部通过）：
  1. 编译：cargo build（debug）+ native\build_bridge.cmd 产出
     out\wxcdn_origin_bridge.dll
  2. probe-ilink：wrapper_loaded / context_created /
     network_manager_created 全 true，cloud_auth_probe_code=0，
     mars 原图下载入口（network_core/impl/cdn_backend start_origin +
     vtable）动态解析成功——4.1.12.26 的 RVA/符号兼容
  3. build-start-config：生成隔离 ilink 工作目录与启动配置
  4. download-origin（真实图片消息）：status=success，origin.jpg
     140809 字节（= hdlength），md5=77509e4475cb097b7c85cc88c2f98883
     与消息 XML 一致，original_size_verified=true
- 关键数据源结论（配合 wechat_db_analysis.txt 排查）：
  图片消息 XML 位于 `decrypted\message\message_0.db` 各 `Msg_*` 表，
  `message_content` 列为 **zstd 压缩**（非明文，非 zlib），解压后为
  `<msg><img aeskey=... cdnbigimgurl=... hdlength=... md5=... /></msg>`
  （带 `wxid:\n` 发送者前缀）；monitor_cache\message_message_0.db 同构。
  `packed_info_data` 为 protobuf（含 MD5 等），ST 已提取 MD5。
- 隔离沙箱会话：需把真实
  `%APPDATA%\Tencent\xwechat\ilink\wechat\cloud_account.txt` 与
  `kvcomm\config.ini` 复制到隔离目录，否则下载挂起/无会话
- 集成建议（待用户确认替换或回退）：
  - 范围：聊天消息原图（C2C）；朋友圈原图（SNS）PoC 不支持，保留现有
    ISAAC 解密通道
  - 形态：作为 get_message_image 失败时的回退通道（新增 Rust 模块 +
    桥接 DLL 随包分发 + IPC + 版本护栏），或按用户要求整体替换
  - 版本护栏：读取 Weixin.exe/ilink2.dll 版本，不在已知兼容列表时禁用
    该通道并提示，避免微信升级后静默失效

## 切片 T-160（R-141）：IPC 参数键名契约审计固化为门禁（已完成）

- 将 T-159 的一次性比对固化为 `.codex_tests/smoke-ipc-contract.mjs`：
  - 解析 305 个 Rust 命令参数（过滤 State/Arc/Window/AppHandle/Manager
    注入项，Channel 保留为真实参数）
  - 解析 145 处前端 invoke 字面量实参（括号配对取顶层键；支持简写键、
    多行对象、嵌套对象、值位置跳过）
  - 前端键按 Tauri camelCase→snake_case 规则归一化后与 Rust 参数集比对，
    任一不一致即失败
- 审计发现并修复真实缺陷：`ocr_simulate_test` 前端传 `testIndex`，
  Rust 参数为 `index`（Tauri 转换后 test_index ≠ index，该命令实际
  一直反序列化失败）——修复为 `{ index: testIndex }`
- 审计脚本加入标准回归门禁（AGENTS.md 测试清单 47 → 48）
- 回归：smoke-ipc-contract 0 不一致；svelte-check 0/0；
  48 前端测试 0 失败；`npm run build` 通过

## 蓝图 T-蓝图-8 + 实施 T-161：消息原图 ilink 官方通道回退（已完成）

### 设计（回退通道模式，用户确认）
- 范围：聊天消息原图（C2C）——仅当现有解密/CDN 解析失败时回退；
  朋友圈原图（SNS）保持现有 ISAAC 解密通道，不涉及
- 依赖：微信官方 ilink_wrapper.dll / ilink2.dll（本机安装目录）+ 本机登录态；
  打包 E:\wechat_image 验证过的下载器 wechat-cdn-poc.exe 与桥接
  wxcdn_origin_bridge.dll（resources/origin/，tauri.conf.json 已登记）
- 版本护栏：白名单 4.1.11.24 / 4.1.12.26；未知版本仅当沙箱内
  compat_ok 标记（一次端到端校验通过后写入）才放行，避免微信升级静默失效
- 隔离沙箱：st_result/origin_ilink/，复制真实 cloud_account.txt 与
  kvcomm 会话，构建 ilink-start-config.bin（字段 1=data_root, 6=client_version）

### 实施（src-tauri/src/wechat/origin_ilink.rs）
- `extract_image_xml`：从解密消息库（message_*/biz_message_* 分片）按
  username → Msg_<md5> 表 + local_type=3 取 message_content，zstd 解压，
  截取 <msg>…</msg>（本次实测确认图片 XML 为 zstd 压缩存储）
- `parse_origin_secret`：cdnbigimgurl/aeskey/hdlength/md5（修复原始字符串
  多一个引号的笔误：`format!("{name}=\"")`）
- `wechat_install_dir`：注册表 + 运行中进程路径（Toolhelp +
  QueryFullProcessImageNameW）+ 版本子目录扫描（Weixin.exe 为启动器，
  ilink DLL 在 4.x.y.z 子目录）
- `ensure_sandbox` / `ilink_compatible`：会话复制 + 版本护栏
- `download_origin_via_ilink`：写 message.json → 调打包下载器
  download-origin（150s 超时）→ 大小/MD5 双校验 → 成功写 compat_ok
- `get_message_image`：现有解析返回 None 且非 thumb 时自动回退
- 新增 IPC `get_ilink_origin_status`（enabled/version/wrapper/sandbox/downloader）

### 验证
- 单元级：`cargo test --lib origin_ilink -- --ignored` 端到端通过——
  真实图片消息（23005727013@chatroom / local_id 105990）回退下载 140809 字节，
  MD5 77509e44… 与消息记录一致
- 运行期：CDP invoke `get_ilink_origin_status` → enabled=true、
  wechat_version=4.1.12.26、sandbox_ready=true、downloader 资源路径正确
- 门禁：cargo fmt/clippy 0、cargo test 215 passed、svelte-check 0/0、
  48 前端测试 0 失败
- 后续候选：前端失败态提示（回退可用/已触发）、失败高频限流、
  桥接源码随仓库维护（当前复用 E:\wechat_image 已验证二进制）

## 切片 T-162（R-142）：零引用导出清理（9 处死导出移除）

- 全仓扫描「导出名仅出现 1 次（定义本身）」的候选，并逐一核验
  文件内自用与测试引用后，删除确认无任何引用的 9 处导出：
  - wechat/services/ipc.ts：getWechatStatus / getWechatHistory /
    getSessionSnapshots / scanWechatAccounts（顺带清理失效导入
    SessionSnapshot / WeChatMessagePayload）
  - kb/auth.svelte.ts：kbLogin / kbLogout（模块其余导出 kbUser /
    refreshKbUser 仍被 KnowledgeBase / GlobalSearch 使用）
  - llm/services/speechFlow.svelte.ts：isSpeechBusy（保留 busy
    重入保护变量，仅删只读它的导出）
  - wechat/constants.ts：EMOTICON_CATEGORY_LABELS / ORDER、
    EXPORT_FORMATS、SESSION_PAGE_SIZE、MESSAGE_PAGE_SIZE
    （保留被 BotLogView 使用的 MSG_TYPE_LABELS）
  - wechat/services/mediaApi.svelte.ts：mediaRoot
- 复核保留项（曾疑似死代码、实为文件内自用/测试引用/文档化 API）：
  tsToDate、TEXT_FILE_EXT_RE、MESSAGE_KIND_LABELS、parseDbTime、
  enqueueAvatar、dismissAll/updateToast、COMMUNITY_COLORS/personWeight、
  EDGE_COLORS/EDGE_COLOR_FALLBACK/ENTITY_DIRS、cmpTid、nodeMatches、
  KIND_PATHS、stopLlmSync（注释明确为生命周期 API）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过

## 切片 T-163（R-143）：微信 IPC 实参类型化（8 处 Record<string, unknown> 收紧）

- `wechat/services/ipc.ts` 中 8 个包装函数实参从 `Record<string, unknown>`
  收紧为精确类型：
  - 6 个记录列表命令（revokes/transfers/red_envelopes/finder/
    miniprograms/friend_verifications）：`WechatRecordListQuery`
    = `{ limit: number; offset: number; q: string | null }`
    （与 Rust `list_wechat_*` 的 limit/offset/q 参数及 GeneralRecords
    调用形状一致）
  - `getMessageImage`：`MessageMediaQuery`
    = `{ username: string; localId: number; size?: 'thumb' | 'hd' | null }`
    （size 缺省即高清原图，与后端 Option<String> 语义一致）
  - `getMessageVoice`：`{ username: string; localId: number }`
- 顺带修正三个下游类型问题：
  - 新类型用 `type` 别名而非 `interface`（interface 无法赋给
    `Record<string, unknown>`，type 可以——TS 已知差异）
  - GeneralRecords `cmdMap` 去掉过时显式注解，改由推断；
    `RecordListResult` 导入随注解删除
  - WeChatPanel.playVoice 增加 `if (!username) return` 守卫
    （IPC 参数要求非空 username，避免传 null 反序列化失败）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过

## 切片 T-164（R-144）：db/kb IPC 实参类型化（7 处 Record<string, unknown> 收紧）

- `db/services/ipc.ts`：
  - `DbTableQuery`（与后端 TableQueryParams 对应）：table/page/pageSize/
    orderCol/orderDir/filter/recount/cursor/direction——queryTable 与
    externalQueryTable 共用
  - `DbCellQuery`：dbPath?/table/rowid/column——getCellValue 使用
  - insertRow/updateRow 的 data 保留 `Record<string, unknown>`
    （行数据本身是任意 JSON，属合理宽松）
- `kb/services/ipc.ts`（均与后端 SearchInput/UploadDocInput/NewVersionInput/
  FetchUrlInput 的 camelCase serde 对应）：
  - `KbSearchInput`（userId?/kbId?/query/topK?/mode?/providerId?/model?）
    → search
  - `KbUploadDocInput`（kbId/dirId?/title/fileType/data 字节数组/
    embeddingProvider?/embeddingModel?/chunkStrategy?/chunkSize?/chunkOverlap?）
    → uploadDocument
  - `KbNewVersionInput`（docId/fileType/data/note?/…）→ uploadNewVersion
  - `KbFetchUrlInput`（url/kbId/dirId?/…）→ fetchUrl
- ragStream/ragStreamWithChannel 的 input 留作后续（需与 RAG 流式共用
  input 类型统一设计）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过

## 切片 T-165（R-145）：RAG 流式输入类型化（KbRagInput）

- T-164 遗留项收口：`kb/services/ipc.ts` 新增
  - `KbRagChunkOverride`（chunkId/content，对应后端 RagChunkOverride）
  - `KbRagInput`（userId?/kbId?/query/providerId?/model?/topK?/mode?/
    sessionId?/chunks?，与后端 RagInput camelCase serde 对应）
- `ragStream` / `ragStreamWithChannel` 的 input 从 `Record<string, unknown>`
  收紧为 `KbRagInput`
- KbChat 的流式问答 input 构造从 `Record<string, unknown>` 改为显式
  `KbRagInput` 注解（chunks 覆盖、sessionId 等字段全部类型化）
- 至此 wechat/kb/db 三大 IPC 服务层的显式 `Record<string, unknown>`
  实参全部收紧完毕；剩余仅 insertRow/updateRow 的 data（任意 JSON 行，
  合理宽松）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过

## 切片 T-166（R-146）：automation 服务类型化 + 面板本地类型收敛

- `automation/services/ipc.ts` 新增共享类型（与后端 RuleInput/规则/任务
  camelCase serde 对应）：
  - `RuleCondition` / `AnalyzeField` / `AutomationRuleInput`
    （id 为 null 表示新建）/ `AutomationRule`（含 hitCount/时间戳）/
    `AutomationTask`（任务行全字段）
- 服务签名收紧：
  - listRules → `AutomationRule[]`；saveRule → `AutomationRuleInput`
    返回 `number`；listTasks items → `AutomationTask[]`；
    editAiExtract aiExtract → `string`；simulatePush 三参 → `string | null`
- AutomationPanel 删除本地 Rule/Task/RuleCondition/AnalyzeField 类型定义
  （约 30 行），改从 services/ipc 导入；loadRules/loadTasks/loadToReply
  移除 `as Rule[]` / `as { items: Task[] }` 断言（类型现由服务层保证）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过

## 切片 T-167（R-147）：automation_stats 类型化（收尾 automation 服务层）

- `automation/services/ipc.ts` 新增 `AutomationStatusCount` 与
  `AutomationStats`（与后端 AutomationStats camelCase 字段一一对应：
  todayPushed/totalTasks/pending/claimed/processing/toReply/replied/done/
  ignored/rulesEnabled/rulesTotal/statusDist）；`stats()` 返回从
  `unknown` 收紧为 `AutomationStats`
- AutomationPanel 删除本地 `Stats` 类型（12 行），改从服务层导入；
  `loadStats` 移除 `as Stats` 断言
- 至此 automation 服务层无 `unknown` 返回/实参（仅 aiExtract/fullJson
  任务字段保持 unknown——JSON 原样透传，属合理宽松）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过

## 切片 T-168（R-148）：kb 服务返回类型化 + CurrentUser 类型上移

- `CurrentUser` 从 auth.svelte.ts 上移到共享 kbTypes.ts（单一类型来源，
  auth.svelte.ts 改 import）；auth 模块现有 kbLogin/kbLogout 已删，
  CurrentUser 仍被 kbUser/refreshKbUser 使用
- kb/services/ipc.ts 5 处 `invoke<unknown>` 返回收紧（对照 Rust 签名）：
  - create → `KbSummary`（后端返回 KbSummary）
  - update → `void`（后端 Result<()>）
  - login → `CurrentUser`（后端 kb_login 返回 CurrentUser）
  - uploadNewVersion → `{ docId; versionId; jobId; title }`
    （后端 json! 固定四字段）
  - wikiExtract → `{ submitted: number }`（后端固定形状）
- 至此 kb 服务层无 `invoke<unknown>` 返回；remaining unknown 仅限
  aiExtract/fullJson 等 JSON 原样透传字段（合理宽松）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过

## 切片 T-169（R-149）：WebKit AudioContext 探测收敛

- `llm/services/voice.ts` 与 `voiceRecorder.svelte.ts` 各有一份逐字相同的
  `window.AudioContext || (window as ... webkitAudioContext ...)` 回退表达式
- voice.ts 新增共享纯函数 `resolveAudioContext(): typeof AudioContext`；
  两处调用点统一改调（顺带消除了重复的 `as unknown as` 类型断言）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  `npm run build` 通过

## 切片 T-170（R-150）：llm/client 拆分 — URL/端点构造辅助簇

- `src-tauri/src/llm/client.rs`（2103 行）改为目录模块
  `src-tauri/src/llm/client/`，首个切片拆出 `urls.rs`：
  - `api_base` / `normalize_base_url` / `is_host_only`（私有）
  - `resolve_embedding_model` / `is_embedding_marked`
  - `chat_url` / `image_url` / `video_url` / `embedding_url` /
    `rerank_url` / `speech_url` / `transcription_url`
- 纯路径构造、不发起网络请求；`pub(crate)` 对外，`mod.rs` 顶部显式
  `use urls::{...}`（11 个名字），本地实现全部删除
- `is_host_only` 保持 urls 私有（仅 api_base 内部使用）；
  `normalize_base_url` 因 `models_endpoints` 也需要，提升为 pub(crate)
- 回归：cargo fmt / clippy --lib --no-default-features 0 警告 /
  cargo check --lib --no-default-features / cargo test --lib
  （215 passed，与拆分前一致）
- 注：默认特性（local-stt → whisper-rs-sys/bindgen）本地缺 libclang
  无法复跑 cargo check；本次改动为与特性无关的纯路径构造，CI 的
  `cargo build`（默认特性）会覆盖该路径
- 后续候选：按 API 域继续拆 生成（image/video）/ 嵌入 / 重排 / 语音 /
  转写 的请求体与响应解析，以及 `models_endpoints` 与
  `ModelParseKind` 的归属

## 切片 T-171（R-151）：llm/client 拆分 — 公共传输层

- 新增 `src-tauri/src/llm/client/transport.rs`：HTTP 客户端 /
  代理回退重试 / 鉴权 / 用量记录，作为各 API 域共用的收发底座
  - `http_client` / `http_client_no_proxy`（90s 超时；传输层失败
    回退直连 + 指数退避，最多 4 次）
  - `post_json_with_retry`（错误链展开，便于定位 DNS/TLS/重置/超时）
  - `apply_auth`（Azure api-key / Ollama 免鉴权 / 其余 bearer）
  - `record_usage`（统一计入「大模型管理 → 流量与成本」，失败仅告警）
- 5 个函数均为 `pub(crate)`，mod.rs 顶部显式 `use transport::{...}`，
  本地实现全部删除；30+ 调用点保持不变
- 回归：cargo fmt / clippy --lib --no-default-features 0 警告 /
  cargo check --lib --no-default-features / cargo test --lib
  （215 passed，与拆分前一致）

## 蓝图（llm/client 目录分层，T-蓝图-9）

```
llm/client/
  mod.rs        — 门面：域模块声明 + 对外 re-export + 测试（302 行）
  urls.rs       — ✅ URL/端点构造（T-170）
  transport.rs  — ✅ 公共传输层（T-171）
  transport.rs  — ✅ 统一计量层：record_usage + estimate_cost（T-171/T-174）
  generation.rs — ✅ 生成域（T-172）
  audio.rs      — ✅ 音频域 STT/TTS（T-173）
  embeddings.rs — ✅ 嵌入/重排域（T-174）
  chat.rs       — ✅ 对话补全域（T-175）
  probe.rs      — ✅ 模型列表/连接探测域（T-176）
```

拆分原则：每次只动一层；各域模块只依赖 urls / transport / types，
不反向依赖 mod.rs 编排逻辑；保持 215 测试与 IPC 契约门禁不变。
llm/client.rs 原始单体（2103 行）已全部目录化（T-170 ~ T-176 共 7 刀，
门面 302 行 + 7 个域模块）。llm 目录下一候选：handlers.rs（42KB）
按命令域拆分。

## 切片 T-172（R-152）：llm/client 拆分 — 生成域（图像/视频）

- 新增 `src-tauri/src/llm/client/generation.rs`（443 行）：
  - `generate_image`：OpenAI 兼容 /images/generations，兼容
    data URL 与 https URL 两种返回
  - `generate_video` / `generate_video_inner`：同步 + 异步任务式双轨；
    SYNC 被拒（400 code 1212）时自动切 /video/submit →
    /video/status/{id} 轮询（180s 超时）
  - 私有辅助：`needs_async_flow` / `extract_task_id` /
    `run_async_video` / `submit_video_task` / `extract_video_urls` /
    `collect_url_from_item` / `is_url_like` / `collect_urls_recursive` /
    `poll_video_task`
- 只依赖 `urls::{api_base,image_url,video_url}` 与
  `transport::{apply_auth,http_client,record_usage}`，不触碰 mod.rs
  编排逻辑；`mod.rs` 以 `pub use generation::{generate_image,
  generate_video}` 重导出，handlers.rs 等外部调用路径不变
- 迁移经字节级校验：块内容 SHA-256 与源文件完全一致
  （3380620A…D3D75，cargo fmt 前后均未变化）
- mod.rs 2103 → 1456 行
- 回归：cargo fmt / clippy --lib --no-default-features 0 警告 /
  cargo check --lib --no-default-features / cargo test --lib
  （215 passed）/ cargo doc --lib --no-deps

## 切片 T-173（R-153）：llm/client 拆分 — 音频域（STT / TTS）

- 新增 `src-tauri/src/llm/client/audio.rs`（219 行），从 mod.rs 三处
  非连续块原样提取：
  - `sniff_audio_format`：RIFF/OggS/FLAC 魔数嗅探，未知回退 mp3
  - `transcribe_audio`：OpenAI /audio/transcriptions multipart 上传，
    与 post_json_with_retry 同策略的代理回退重试；仅 whisper 系模型
    附加 language 参数
  - `is_transcription_model` / `resolve_transcription_provider`：
    ASR 模型识别与提供方解析（硅基流动缺省补 SenseVoiceSmall）
  - `create_speech`：OpenAI /audio/speech，返回字节与嗅探后的真实格式
- 依赖收敛：`urls::{speech_url,transcription_url}`、
  `transport::{apply_auth,http_client,http_client_no_proxy,record_usage}`；
  `resolve_transcription_provider` 签名由全路径改为导入的 `LlmConfig`
  （更贴合类型提示规范）
- mod.rs 以 `pub use audio::{create_speech, resolve_transcription_provider,
  transcribe_audio}` 重导出（handlers.rs 与既有测试路径不变），
  `is_transcription_model` 私有导入供 test_connection 使用；
  urls 导入面随之收敛（移除 speech_url/transcription_url）
- 迁移经 SHA-256 复核：三块与源文件字节一致（A/C 完全相同；B 仅含
  上述签名收敛，还原原签名后哈希完全一致）
- mod.rs 1456 → 1250 行
- 回归：cargo fmt / clippy --lib --no-default-features 0 警告 /
  cargo check --lib --no-default-features / cargo test --lib
  （215 passed）/ cargo doc --lib --no-deps

## 切片 T-174（R-154）：llm/client 拆分 — 嵌入 / 重排序域

- 新增 `src-tauri/src/llm/client/embeddings.rs`（331 行），从 mod.rs
  四处非连续块原样提取：
  - `resolve_embedding_provider`：跨提供方嵌入模型解析（请求模型非
    嵌入类型或无嵌入模型时自动切换）
  - `create_embedding`：OpenAI /embeddings，按行拆分多条输入，
    解析 usage 并计入流量成本
  - `create_embeddings_batch` / `create_embeddings_batch_with`：
    批量原样发送（保留分片内换行），数量一致性校验；
    `_with` 提升为 pub(crate) 供诊断探针复用
  - `rerank`：Cohere /rerank，兼容 results/data/纯数组三形态
- `estimate_cost` 从 mod.rs 迁入 transport.rs（与 record_usage 组成
  统一计量层），mod.rs 以 `pub use transport::estimate_cost` 保持
  `client::estimate_cost` 对外路径（handlers.rs 依赖）
- mod.rs 重导出：`pub use embeddings::{create_embedding,
  create_embeddings_batch, rerank}`；`resolve_embedding_provider`
  保持 pub(crate) 重导出（handlers.rs 以 crate 内部路径调用）；
  urls 导入面收敛为 api_base/chat_url/normalize_base_url
- 测试可见性：resolve_tests 直接 `use super::urls::{...}`，
  probe_tests 直接 `use super::embeddings::create_embeddings_batch_with`，
  不再依赖 mod.rs 中转
- 迁移经 SHA-256 复核：d1（含签名收敛）与 d4 与源块哈希一致；
  d2/d3 尺寸正确且写时字节级校验通过
- mod.rs 1250 → 926 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib --no-default-features / cargo test --lib
  （215 passed）/ cargo doc --lib --no-deps

## 切片 T-175（R-155）：llm/client 拆分 — 对话补全域

- 新增 `src-tauri/src/llm/client/chat.rs`（435 行），从 mod.rs 六处
  非连续块原样提取：
  - `build_content`：消息多模态 parts → OpenAI content（文本/图片/
    文件描述）；无外部调用方，收敛为私有（原为 pub）
  - `CompletionParams`：补全参数对象（model/messages/温度/惩罚等）
  - `chat_completion`：非流式补全，代理回退 + 错误链定位 + usage
    解析与统一计量
  - `estimate_tokens`：CJK/拉丁混排 token 兜底估算
  - `chat_completion_stream`：SSE 流式解析（索引游标防 O(n²)），
    usage 末帧捕获，on_delta 回调
  - `serialize_body_with_temp`：temperature 精确到 2 位小数
    （智谱 GLM 等拒绝超精度值）
- mod.rs 重导出 `pub use chat::{chat_completion,
  chat_completion_stream, CompletionParams}`（automation/kb/wechat/
  handlers 等外部调用路径不变）；test_connection 经重导出继续调用
- 导入面收敛：mod.rs 移除 std::error::Error / tokio_stream::StreamExt /
  json / record_usage / chat_url（均只属于 chat 域）
- 迁移经 SHA-256 复核：c5 与源块完全一致；c1 仅含 build_content
  可见性收敛（还原 pub 后哈希一致）；其余块写时字节级校验通过
- mod.rs 926 → 506 行（llm/client.rs 原始 2103 行 → 目录化后 506 行）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib --no-default-features / cargo test --lib
  （215 passed）/ cargo doc --lib --no-deps

## 切片 T-176（R-156）：llm/client 拆分 — 模型列表 / 连接探测域

- 新增 `src-tauri/src/llm/client/probe.rs`（216 行），从 mod.rs 五处
  非连续块原样提取：
  - `ModelParseKind` / `models_endpoints`：按提供方类型构造候选
    模型列表端点（Ollama /api/tags、Azure /openai/models、通用
    /models 与 /v1/models 回退）
  - `parse_models`：OpenAI data[].id 与 Ollama models[].name 解析
  - `fetch_models`：按候选端点依次探测，代理失败回退直连
  - `test_connection`：最小补全（max_tokens=1）探活；ASR 模型
    改走 GET /models；经 `super::chat` 调用补全、`super::audio`
    判断转写模型
- mod.rs 收敛为纯重导出门面：`pub use probe::{fetch_models,
  test_connection}`（handlers.rs 调用路径不变）；types/serde_json/
  transport/urls 导入全部移除
- 测试可见性：resolve_tests 直接 `use super::audio::is_transcription_model`；
  probe_tests 直接 `use super::chat::{chat_completion, CompletionParams}`
  与 types 导入，不再依赖 mod.rs 中转
- 迁移经 SHA-256 复核：p4/p5 与源块哈希一致；p1-p3 写时字节级
  校验通过且尺寸正确
- mod.rs 506 → 302 行（llm/client 目录化完成：门面 + 7 个域模块）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib --no-default-features / cargo test --lib
  （215 passed）/ cargo doc --lib --no-deps

## 蓝图 T-蓝图-10：llm/handlers.rs 按命令域拆分

```
llm/handlers/
  mod.rs       — 门面：共享常量/事件广播 + mod 声明 + glob re-export
                 （lib.rs 的 llm::handlers::<cmd> 注册点零改动）
  usage.rs     — ✅ 流量与成本管控（T-177）
  providers.rs — ✅ 配置读取/提供方 CRUD/默认设置/模型管理/
                 连接测试/提供方类型（T-178）
  chat.rs      — ✅ chat_with_llm / chat_with_llm_stream（T-179）
  generation.rs— ✅ generate_image / generate_video（T-180）
  resource.rs  — ✅ save_uploaded_file / save_resource_from_url（T-181）
  history.rs   — ✅ get_llm_chat_history / append / clear（T-182）
  embedding.rs — ✅ create_embedding / rerank（T-183）
  audio.rs     — ✅ create_speech / transcribe_voice_audio /
                 synthesize_native_speech + 测试（T-184）
  resource.rs  — 候选：save_uploaded_file / save_resource_from_url
```

注意：`#[tauri::command]` 生成的 `__cmd__*` / `__tauri_command_name_*`
隐藏项不会随显式 `pub use name` 传递，必须用 `pub use <域>::*;`
glob re-export（与 wechat/handlers 既有惯例一致），否则
`generate_handler!` 报 E0433。

## 切片 T-177（R-157）：llm/handlers 拆分 — 用量管控域

- `handlers.rs`（1222 行）转目录模块 `handlers/`，首拆 `usage.rs`：
  - `get_llm_usage` / `reset_llm_usage` / `get_llm_usage_summary`
    （月度汇总含 token/cost 配额进度，全路径依赖 config + types）
- mod.rs 保留共享设施（LLM_CONFIG_CHANGED_EVENT、
  notify_llm_config_changed）与其余命令，`pub use usage::*;`
  保持 lib.rs 注册路径不变；移除已迁出的 ProviderUsage 导入
- usage.rs 内 `serde_json::json!` 收敛为导入的 `json!`
- 迁移经 SHA-256 复核：块与源完全一致（还原 json! 写法后哈希一致）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib --no-default-features / cargo test --lib
  （215 passed，证明 lib.rs 命令注册编译通过）/ cargo doc

## 切片 T-178（R-158）：llm/handlers 拆分 — 接入配置 / 提供方域

- 新增 `src-tauri/src/llm/handlers/providers.rs`（295 行），从 mod.rs
  两处非连续块原样提取（13 个命令）：
  - 配置读取：get_llm_config / get_llm_config_path / set_last_chat
  - 提供方 CRUD：upsert_llm_provider / delete_llm_provider /
    set_llm_default_provider
  - 连接测试：test_llm_connection（client::test_connection）
  - 模型管理：list_llm_models / add_llm_model / remove_llm_model /
    remove_llm_models / set_llm_default_model / set_llm_model_meta
  - get_llm_provider_types
- 共享事件广播 `notify_llm_config_changed` 留在 mod.rs 门面，
  providers.rs 经 `super::notify_llm_config_changed` 调用
- mod.rs 移除 LlmConfig/ModelMeta/ProviderConfig/ProviderType/
  TestResult 导入（均已迁出）；`pub use providers::*;`
  保持 lib.rs 注册路径不变
- 迁移经 SHA-256 复核：p1（272 行）与 p2（10 行）与源块哈希完全一致
- mod.rs 1184 → 902 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-179（R-159）：llm/handlers 拆分 — 全局对话调用域

- 新增 `src-tauri/src/llm/handlers/chat.rs`（212 行），从 mod.rs
  连续块原样提取：
  - `inject_role_system_prompt`：跨模块 AI 角色提示词注入
  - `chat_with_llm`：提供方/模型解析 + token/成本配额管控 +
    非流式调用 + 用量成本汇总
  - `chat_with_llm_stream`：Channel 增量推送（delta/done/error）+
    助手消息持久化（db State）
- 依赖收敛：`crate::ai_role::` 全路径改为导入的 `ai_role::`
- mod.rs 移除 ChatRequest/ChatResult/tauri::ipc::Channel 导入；
  `pub use chat::*;` 保持 lib.rs 注册路径不变
- 迁移经 SHA-256 复核：块与源完全一致（还原 ai_role 全路径后
  哈希一致）
- mod.rs 902 → 704 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-180（R-160）：llm/handlers 拆分 — 图像/视频生成域

- 新增 `src-tauri/src/llm/handlers/generation.rs`（96 行）：
  - `generate_image`：提供方/模型解析 + n 参数 clamp(1,4) +
    client::generate_image
  - `generate_video`：同解析流程 + client::generate_video
- 依赖仅 config / client / types，无共享设施耦合
- mod.rs 移除 ImageGen/VideoGen 四类型导入；`pub use generation::*;`
  保持 lib.rs 注册路径不变
- 迁移经 SHA-256 复核：块与源完全一致（fmt 后无变化）
- mod.rs 703 → 618 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-181（R-161）：llm/handlers 拆分 — 生成资源保存域

- 新增 `src-tauri/src/llm/handlers/resource.rs`（51 行）：
  - `save_uploaded_file`：落盘到 st_result/llm_attachments/
  - `save_resource_from_url`：data URL 直接 base64 解码；远程地址
    reqwest 下载，文件名回退链（请求名 → URL 推导 → image.png）
- 附件辅助函数（save_bytes_to_attachments / ext_for_mime /
  derive_name_from_url）仍属 history 域辅助，留待 history.rs 切片时
  统一迁入（当前经 `super::` 供 resource.rs 调用）
- mod.rs 无需移除导入（base64/reqwest 均仍在 mod.rs 或全路径使用）；
  `pub use resource::*;` 保持 lib.rs 注册路径不变
- 迁移经 SHA-256 复核：块与源完全一致（fmt 后无变化）
- mod.rs 618 → 578 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-182（R-162）：llm/handlers 拆分 — 聊天记录持久化域

- 新增 `src-tauri/src/llm/handlers/history.rs`（116 行）：
  - `get_llm_chat_history`：SQLite 读取 + file_path 转 data URL
  - `file_path_to_data_url`（仅 history 使用，随域迁移）
  - `append_llm_chat_messages` / `clear_llm_chat_history`
- 附件辅助（save_bytes_to_attachments / ext_for_mime /
  derive_name_from_url）经使用面分析确认仅 resource 域使用，
  并入 resource.rs 为私有函数（原「super:: 借用」方案取消）
- mod.rs 移除 ChatMessage/ContentPart/State 导入（audio/embedding
  簇无需）；`pub use history::*;` 保持 lib.rs 注册路径不变
- 迁移经 SHA-256 复核：h1a/h1b/h2 三块与源完全一致（fmt 后无变化）
- mod.rs 578 → 413 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-183（R-163）：llm/handlers 拆分 — 文本嵌入 / 重排序域

- 新增 `src-tauri/src/llm/handlers/embedding.rs`（125 行）：
  - `create_embedding`：跨提供方嵌入模型解析（client::resolve_
    embedding_provider）+ 向量生成 + set_last_embedding
  - `rerank`：提供方/模型双分支解析（显式 id 或默认提供方）+
    client::rerank + set_last_chat
- 依赖仅 config / client / types；mod.rs 移除 Embedding/Rerank
  四类型导入；`pub use embedding::*;` 保持 lib.rs 注册路径不变
- 注：搬移脚本对延伸到文件末尾的块产生越界尾切片，已校验并修正
  （mod.rs 302 → 300 行）；最终块与源 SHA-256 完全一致
- mod.rs 413 → 298 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-184（R-164）：llm/handlers 拆分 — 语音合成 / 转写域（收尾）

- 新增 `src-tauri/src/llm/handlers/audio.rs`（264 行）：
  - `create_speech`：TTS 提供方/模型双分支解析 + base64 音频返回
  - `transcribe_voice_audio`：云端转写优先（resolve_transcription_
    provider）+ local-stt 本地 Whisper 兜底（cfg feature 分支）
  - `synthesize_native_speech`：Windows SAPI 离线合成
  - `voice_transcribe_tests`：空录音拒绝 / 云端链路（ignored）/
    SAPI WAV 校验（3 个测试随域迁移）
- mod.rs 收敛为纯门面（44 行）：仅保留 LLM_CONFIG_CHANGED_EVENT、
  notify_llm_config_changed 与 8 个域的 glob re-export；
  config/client/base64/types 导入全部移除
- 迁移经 SHA-256 复核：块与源完全一致（fmt 后无变化）
- llm/handlers.rs 原始 1222 行 → 门面 44 行 + 8 个域模块，
  T-蓝图-10 全部完成（T-177 ~ T-184 共 8 刀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 蓝图 T-蓝图-11：kb/wiki.rs 按职责拆分

```
kb/wiki/
  mod.rs       — 门面：模块声明 + glob re-export（kb::wiki::* 路径不变）
  types.rs     — ✅ 数据结构（T-185）
  query.rs     — ✅ list_pages / get_page / graph / snippet 辅助（T-190）
  mutate.rs    — ✅ create/update/delete_page / rebuild_links_for_page /
                 rebuild_kb_links（T-188）
  fts.rs       — ✅ sync_fts_upsert / rebuild_fts / fts_match_query /
                 search_pages（T-187）
  generate.rs  — ✅ list_ready_docs / generate / generate_with_jobs
                 （T-189）
  extract.rs   — ✅ LLM 摘要/实体提取 / ensure_entity_pages / refine
                 （T-191）
  utils.rs     — ✅ extract_wiki_links / slugify / truncate_for_llm /
                 OptionNone（T-186）
  tests.rs     — ✅ 10 个单元测试（T-192）
```

拆分原则：各子模块只依赖 types / super::db / super::parse，经 mod.rs
re-export 保持外部调用点（handlers/wiki.rs 等）零改动；测试随其依赖
的私有函数迁移。

## 切片 T-185（R-165）：kb/wiki 拆分 — 数据结构

- `wiki.rs`（1785 行）转目录模块 `kb/wiki/`，首拆 `types.rs`（134 行）：
  - WikiPageItem / WikiLinkInfo / WikiPageDetail / WikiEntity /
    WikiGraphNode / WikiGraphEdge / WikiGraph / WikiGenerateInput /
    WikiPageInput（camelCase 序列化）
- mod.rs 保留 WikiPageRow（私有行类型）、PAGE_SEP/PAGE_END 常量与
  全部函数；`pub use types::*;` 保持 handlers/wiki.rs 等外部
  `kb::wiki::*` 路径不变；serde 导入随结构体迁出
- 修正一次插入位置：mod 声明原落在 WikiPageRow 文档注释中间，
  已上移到 import 区（注释块恢复完整）
- 迁移经 SHA-256 复核：块与源完全一致（fmt 后无变化）
- wiki.rs 1785 → 门面 1658 + types 134
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-192（R-172）：kb/wiki 拆分 — 单元测试域（收尾）

- 新增 `src-tauri/src/kb/wiki/tests.rs`（99 行）：10 个单元测试
  从 mod.rs 内嵌模块迁出（slugify / extract_wiki_links /
  truncate_for_llm / parse_refined_pages 与回退 / link_snippet
  多字节边界）
- mod.rs 收敛为 44 行纯门面：8 个子模块声明 + re-export +
  WikiPageRow（私有行类型）+ `#[cfg(test)] mod tests;`；
  清理 PAGE 常量迁出后遗留的悬空文档注释
- 测试迁移后全部在 `kb::wiki::tests::` 下运行通过（10 个），
  与拆分前测试集等价
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed，
  其中 wiki::tests 10 个全过）/ cargo doc

## 完成：T-蓝图-11（kb/wiki 目录化）

`kb/wiki.rs` 原始 1785 行 → 门面 44 行 + 8 个子模块：
types 134 / utils 95 / fts 149 / mutate 159 / generate 228 /
query 528 / extract 429 / tests 99。外部调用点（handlers/wiki.rs
等 `kb::wiki::*` 路径）零改动；每个切片均过全部门禁并做
SHA-256 字节级迁移校验。

## 蓝图 T-蓝图-12：wechat/monitor.rs 按职责拆分

```
wechat/monitor/
  mod.rs       — 门面：数据结构（WeChatMessage/SessionMonitor/
                 SessionEntry/ContactMap）+ re-export + 剩余 impl
  util.rs      — ✅ 已有（连接/文件/解密辅助）
  start.rs     — ✅ 启动器：MonitorStartCtx + start_monitor（T-193）
  query.rs     — ✅ query_state / do_full_refresh / do_wal_refresh /
                 resolve_message_dbs / query_messages_since_watermark /
                 query_latest_message（T-194）
  check.rs     — ✅ check_updates(_inner/_forced) + format_time（T-195）
```

拆分原则：impl 块可跨文件（子模块内 `impl SessionMonitor`），
私有项经 `super::` 访问；外部调用点（handlers/monitor.rs 的
`monitor::{start_monitor, MonitorStartCtx}`）经 re-export 零改动。

## 切片 T-193（R-173）：wechat/monitor 拆分 — 监控线程启动器

- `monitor.rs`（1388 行）转目录模块 `wechat/monitor/mod.rs`，
  启动器迁入 `start.rs`（290 行）：
  - `MonitorStartCtx`：启动参数（密钥/路径/缓存/路由/图片解密）
  - `start_monitor`：同步初始化解密 → prev_state 播种 → 事件驱动
    主循环（HybridListener + 5s 轮询 + 30s 水位线 + 背压保护 +
    心跳/退出状态上报）
- 依赖：`super::{SessionMonitor, ContactMap}`（私有项经子模块
  super 访问）、`crate::wechat::{db_cache, image, listener, router}`
- mod.rs `pub use start::{MonitorStartCtx, start_monitor};` 保持
  handlers/monitor.rs 外部路径不变；移除迁出的 Duration 导入
- 迁移经 SHA-256 复核：块与源完全一致（fmt 后无变化）
- monitor.rs 1388 → 门面 1113 + start 290
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-195（R-175）：wechat/monitor 拆分 — 主更新检测循环域（收尾）

- 新增 `src-tauri/src/wechat/monitor/check.rs`（409 行）：
  - `check_updates` / `check_updates_forced`：mtime 门控 + 强制检测
  - `check_updates_inner`：状态对比、消息提取、shown_keys 去重、
    水位线更新（decrypt_ms 指标）
  - `format_time`：时间戳格式化
- 依赖：util（media_type/format_msg_type）、query 方法
  （pub(crate)）、`self.{shown_keys, watermark_store, prev_state,
  decrypt_ms}` 私有字段；补 SessionMonitor/SystemTime 导入
- mod.rs 移除迁出的 SystemTime 导入；门面 206 行
- 迁移经 SHA-256 复核：块与源完全一致（fmt 后无变化）
- monitor.rs 1388 → 门面 206 + start 290 + query 523 + check 409
  （+ 既有 util）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 完成：T-蓝图-12（wechat/monitor 目录化）

`wechat/monitor.rs` 原始 1388 行 → 门面 206 行（数据结构 +
SessionMonitor 核心字段与状态辅助 + re-export）+ 4 个子模块：
util（既有）/ start 290 / query 523 / check 409。外部调用点
（handlers/monitor.rs 的 `monitor::{start_monitor, MonitorStartCtx}`）
零改动；每个切片均过全部门禁并做 SHA-256 字节级迁移校验。

## 蓝图 T-蓝图-13：bot/manager.rs 按职责拆分

```
bot/manager/
  mod.rs      — 门面：数据结构（BotManager/AccountRuntime/QrView/
                AccountContact/BotStatusSummary）+ 核心字段 + 剩余 impl
  qr.rs       — ✅ 二维码绑定/重扫：start_qr / poll_qr / cancel_qr（T-196）
  account.rs  — ✅ list/rename/unbind_account / status_summary /
                require_account（T-199）
  channel.rs  — ✅ channel_config(_plain) / add/update/test_channel（T-197）
  send.rs     — ✅ send_text(_inner/_wechat) / send_media(_inner/_wechat) /
                make_sender / log_outcome（T-198）
  loop.rs     — ✅ start_all / spawn/run_account_loop / persist_tokens /
                set_status(_error) / emit_status（T-200）
  contacts.rs — ✅ list_contacts / list_logs / save_inbound_media /
                spawn_responder（T-201）
  utils.rs    — ✅ apply_onebot_override / default_account_name /
                qr_svg_data_url / sniff_ext（T-202）
  tests.rs    — ✅ qr_svg / onebot_override 测试（T-202）
```

拆分原则：impl 跨文件；私有项（QrRecord/CONNECT_TTL/
default_account_name 等）按需提升 pub(crate)；外部类型路径
（handlers.rs 的 `manager::{BotManager, QrView, ...}`）零改动。

## 切片 T-196（R-176）：bot/manager 拆分 — 二维码绑定域

- `manager.rs`（1352 行）转目录模块 `bot/manager/mod.rs`，
  QR 簇迁入 `qr.rs`（183 行）：
  - `start_qr`：创建扫码会话（ilink auth::create_qr + 本地 SVG）
  - `poll_qr`：状态轮询（Wait/Scanned/Confirmed 等），确认后
    加密 token 落库、更新账号、启动轮询循环、发送欢迎消息
  - `cancel_qr`：取消扫码会话
- 共享项提升 pub(crate)：QrRecord / CONNECT_TTL /
  default_account_name / qr_svg_data_url；qr.rs 经 `super::` 导入
- mod.rs 移除迁出的 auth/QrStatus 导入；外部类型路径零改动
- 迁移经 SHA-256 复核：块与源完全一致（fmt 后无变化）
- manager.rs 1352 → 门面 1184 + qr 183
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-202（R-182）：bot/manager 拆分 — 工具函数与测试（收尾）

- 新增 `src-tauri/src/bot/manager/utils.rs`（138 行）：4 个自由函数
  （apply_onebot_override / default_account_name / qr_svg_data_url /
  sniff_ext）；新增 `tests.rs`（44 行）：qr_svg / onebot_override
  两个单元测试
- mod.rs `pub(crate) use utils::*;` 保持 send/qr/contacts 的
  `super::{...}` 导入零改动；tests.rs 直接导入 utils 与 OnebotConfig
- mod.rs 移除迁出的 OnebotConfig 导入；门面 131 行（数据结构 +
  new/attach_app/db_path/emit + mod 声明）
- 迁移经 SHA-256 复核：utils 块与源完全一致；tests 写时 -cne
  校验 + fmt 重新缩进（嵌套 mod → 顶层，8→4 空格，语义不变）+
  测试全过
- manager.rs 1352 → 门面 131 + 8 个子模块（qr 183 / channel 126 /
  send 317 / account 83 / loop 293 / contacts 148 / utils 138 /
  tests 44）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 完成：T-蓝图-13（bot/manager 目录化）

`bot/manager.rs` 原始 1352 行 → 门面 131 行 + 8 个子模块（见上），
外部类型路径（handlers.rs 的 `manager::{BotManager, QrView, ...}`）
零改动；每个切片均过全部门禁并做 SHA-256 字节级迁移校验。

## 蓝图 T-蓝图-14：kb/parse.rs 按格式/职责拆分

```
kb/parse/
  mod.rs      — 门面：ParsedDoc/SectionSpan/ChunkStrategy/ChunkConfig/
                Chunk 类型 + parse_document 调度 + split_into_sections
                + chunk_* 策略 + save_chunks + 测试
  docx.rs     — ✅ docx 解析（T-203）
  pdf.rs      — ✅ parse_pdf / ocr_pdf_fallback / extract_pdf_jpeg_streams
                （T-204）
  xlsx.rs     — ✅ parse_xlsx + shared strings/sheet 提取（T-205）
  anydoc.rs   — ✅ parse_with_anydoc（多格式走 anydoc 引擎）（T-207）
  chunk.rs    — ✅ chunk_text/recursive/title/parent_child +
                find_break_point / estimate_tokens + Chunk 结构体（T-206）
  tests.rs    — ✅ 32 个单元测试（T-208）
```

拆分原则：各格式解析器只依赖 types + split_into_sections（pub(crate)），
外部调用点（kb 各模块的 `parse::parse_document` 等）零改动。

## 切片 T-203（R-183）：kb/parse 拆分 — docx 解析域

- `parse.rs`（1326 行）转目录模块 `kb/parse/`，docx 簇迁入
  `docx.rs`（70 行）：
  - `parse_docx`（pub(crate)，parse_document 调度调用）
  - `extract_docx_document_xml`：zip 解压取 word/document.xml
  - `extract_text_from_word_xml`：<w:t> 文本按段落补换行
- `split_into_sections` 提升 pub(crate)（各格式解析器共用）；
  mod.rs `mod docx; use docx::parse_docx;`
- 迁移经 SHA-256 复核：还原可见性后块与源完全一致
- parse.rs 1326 → 门面 1266 + docx 70
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-208（R-188）：kb/parse 拆分 — 单元测试域（收尾）

- 新增 `src-tauri/src/kb/parse/tests.rs`（510 行）：32 个单元测试
  迁出（分片策略/断点/token 估算/解析调度/PDF/docx/xlsx/anydoc）
- mod.rs 收敛为 249 行门面（类型 + parse_document + save_chunks +
  split_into_sections + mod 声明 + `#[cfg(test)] mod tests;`）
- 测试迁移后全部通过（215 total 不变），fmt 重新缩进（嵌套 mod
  → 顶层）为预期变化
- parse.rs 1326 → 门面 249 + docx 70 / pdf 129 / xlsx 146 /
  chunk 254 / anydoc 26 / tests 510
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 完成：T-蓝图-14（kb/parse 目录化）

`kb/parse.rs` 原始 1326 行 → 门面 249 行 + 6 个子模块（见上），
外部调用点（kb 各模块的 `parse::parse_document` / `parse::Chunk` /
`parse::chunk_text` 等路径）零改动；每个切片均过全部门禁并做
SHA-256 字节级迁移校验。

## 蓝图 T-蓝图-16：sql_browse.rs 按职责拆分

```
sql_browse/
  mod.rs    — 门面：核心查询/转换/导出 + 模块声明
  types.rs  — ✅ ColumnInfo/TableData/TableQueryParams（T-212）
  query.rs  — ✅ list_tables / table_schema / query_table（T-217）
  convert.rs— ✅ read_cell / cell_value_to_json / json_to_sql_value /
              blob_to_preview / guess_mime（T-214）
  inspect.rs— ✅ row_to_json / table_ddl(_detail) / db_integrity /
              table_stats（T-218）
  execute.rs— ✅ execute_sql / first_keyword（T-216）
  export.rs — ✅ csv_escape / export_table_to_csv（T-215）
  utils.rs  — ✅ safe_name / escape_like / friendly_db_error（T-213）
```

拆分原则：纯函数域零耦合（convert/export/utils），查询域依赖 types；
外部调用点（db.rs / external_db.rs / ipc_handlers.rs 的
`sql_browse::*` 路径）经 re-export 零改动。

## 切片 T-212（R-192）：sql_browse 拆分 — 数据类型

- `sql_browse.rs`（1041 行）转目录模块 `sql_browse/mod.rs`，
  types 两处非连续块迁入 `types.rs`（42 行）：
  - `ColumnInfo` / `TableData`（camelCase 序列化）
  - `TableQueryParams`（keyset 分页参数）
- mod.rs `pub use types::*;` 保持 `sql_browse::{ColumnInfo, TableData,
  TableQueryParams}` 外部路径；移除迁出的 serde 导入
- 迁移经 SHA-256 复核：两块与源完全一致
- sql_browse.rs 1041 → 门面 1007 + types 42
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-218（R-198）：sql_browse 拆分 — 表详情/完整性/列统计域（收尾）

- 新增 `src-tauri/src/sql_browse/inspect.rs`（290 行）：
  - `row_to_json`（pub(crate)，execute 域共用）：行值语义转换
  - `table_ddl` / `parse_fk_refs` / `table_detail`：DDL/外键/详情
  - `db_integrity`：PRAGMA integrity_check + 外键校验
  - `table_stats`：列抽样统计（null 比例/min/max/TOP）
- 依赖：query（table_schema）、convert（blob_to_preview）、utils；
  execute.rs 的 row_to_json 导入改指 inspect（签名提升 pub(crate)
  并折行，属预期）
- mod.rs 收敛为 27 行纯门面（7 个子模块 re-export）
- sql_browse.rs 1041 → 门面 27 + types 42 / utils 37 / convert 120 /
  export 94 / execute 86 / query 415 / inspect 290
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 完成：T-蓝图-16（sql_browse 目录化）

`sql_browse.rs` 原始 1041 行 → 门面 27 行 + 7 个子模块（见上），
外部调用点（db.rs / external_db.rs / ipc_handlers.rs 的
`sql_browse::*` 路径）经 re-export 零改动；每个切片均过全部门禁
并做 SHA-256 字节级迁移校验。

## 蓝图 T-蓝图-17：wechat/modules/messages.rs 按职责拆分

```
wechat/modules/messages/
  mod.rs   — 门面：分库缓存/TransferStatus 类型 + 查询编排 + 测试
  types.rs — ✅ ChatMessage / MessagePage（T-219）
  shards.rs— ✅ open_shard_from_meta / load_name2id / open_shards
             + 分库类型/索引缓存（T-220）
  parse.rs — ✅ parse_display_content（XML/富媒体渲染）（T-221）
  query.rs — ✅ query_shard_rows / transfer_status_map /
             get_conversation_messages（T-222）
  tests.rs — ✅ transfer 冒烟测试（T-223）
```

拆分原则：分库内部类型（MsgShard/ShardMeta/ShardIndexEntry）随
shards/query 域，公开类型经 re-export 保持外部路径零改动。

## 切片 T-219（R-199）：messages 拆分 — 数据类型

- `wechat/modules/messages.rs`（985 行）转目录模块
  `messages/mod.rs`，公开类型迁入 `types.rs`（63 行）：
  - `ChatMessage`：单条消息（local_id/server_id/发送者/XML 等）
  - `MessagePage`：游标分页结果（next_cursor/has_more/会话显示名）
- 分库内部类型（MsgShard/ShardMeta/ShardIndexEntry/ShardCacheKey）
  留门面（随 shards/query 域后续迁移）；`pub use types::*;`
  保持 `messages::{ChatMessage, MessagePage}` 外部路径
- mod.rs 移除迁出的 serde 导入；门面 929 行
- 迁移经 SHA-256 复核：块与源完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-223（R-203）：messages 拆分 — 测试域（收尾）

- 新增 `src-tauri/src/wechat/modules/messages/tests.rs`（115 行）：
  - `smoke_transfer_merge`：同笔转账多条记录合并 + 方向文案
  - `smoke_transfer_status_only_direction`：状态行取反方向
  （均为 Windows 真实数据冒烟）
- mod.rs 收敛为 34 行纯门面（模块文档 + 5 个子模块声明 +
  `pub use types/query` + `#[cfg(test)] mod tests;`）
- 迁移后 2 个冒烟测试在 `messages::tests` 下运行通过
- messages.rs 985 → 门面 34 + types 63 / shards 229 / parse 154 /
  query 423 / tests 115
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 完成：T-蓝图-17（wechat/modules/messages 目录化）

`messages.rs` 原始 985 行 → 门面 34 行 + 5 个子模块（见上），
外部调用点（handlers 等 `modules::messages::get_conversation_messages`）
零改动；每个切片均过全部门禁并做 SHA-256 字节级迁移校验。

## 蓝图 T-蓝图-18：wechat/insights.rs 按职责拆分

```
wechat/insights/
  mod.rs     — 门面：会话统计编排 + 图谱构建/API + 测试
  progress.rs— ✅ 既有（进度上报）
  cache.rs   — ✅ 既有（结果缓存）
  types.rs   — ✅ GraphNode / SharedMember / GraphEdge（T-224）
  stats.rs   — ✅ collect_msg_counts / collect_active_days（T-225）
  graph.rs   — ✅ shared_group_pairs / member_group_map /
               collect_self_accounts / build_relationship_graph（T-226）
  api.rs     — ✅ get_relationship_graph(_cached) / graph_cache_path（T-227）
  tests.rs   — ✅ graph_smoke_real_data（T-227）
```

拆分原则：types 零依赖；stats/graph 依赖 types + modules；
外部调用点（handlers 的 `insights::{get_relationship_graph, ...}`）
经 re-export 零改动。

## 切片 T-224（R-204）：insights 拆分 — 数据类型

- `wechat/insights.rs`（893 行）转目录模块 `insights/mod.rs`
  （既有 progress/cache 子模块保留），公开类型迁入 `types.rs`：
  - `GraphNode`：节点（self/contact/group/official + 群共现字段）
  - `SharedMember`：群节点共同成员明细
  - `GraphEdge`：消息强度/共群关系边
- `pub use types::*;` 保持 `insights::{GraphNode, GraphEdge, ...}`
  外部路径；mod.rs 移除迁出的 serde 导入；门面 840 行
- 迁移经 SHA-256 复核：块与源完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-227（R-207）：insights 拆分 — IPC API 与测试（收尾）

- 新增 `src-tauri/src/wechat/insights/api.rs`（55 行）：
  - `get_relationship_graph`（#[tauri::command]）：配置加载 +
    build_relationship_graph + run_blocking
  - `get_relationship_graph_cached`：缓存文件秒开
  - `graph_cache_path`（pub(crate)，graph 域共享）
- 新增 `tests.rs`（82 行）：真实数据冒烟（节点/边/消息数/自我头像）
- 关键：Tauri 命令重导出需 `pub use api::*;` glob（隐藏的
  `__cmd__*` 项才随行，显式命名会致 generate_handler E0433，
  与 llm/handlers 同款陷阱）；graph.rs 的 graph_cache_path 改指
  `super::api::`
- mod.rs 收敛为 26 行纯门面（模块文档 + 7 个子模块声明 +
  re-export + `#[cfg(test)] mod tests;`）
- insights.rs 893 → 门面 26 + types 60 / stats 198 / graph 519 /
  api 55 / tests 82（+ 既有 progress/cache）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 完成：T-蓝图-18（wechat/insights 目录化）

`insights.rs` 原始 893 行 → 门面 26 行 + 7 个子模块（见上），
外部调用点（lib.rs 的 `insights::get_relationship_graph` 等）零改动；
每个切片均过全部门禁并做 SHA-256 字节级迁移校验。

## 蓝图 T-蓝图-19：wechat/config.rs 按职责拆分

```
wechat/config/
  mod.rs   — 门面：常量 + 默认目录 + 加载/保存/补丁 + 账号检测 + 测试
  types.rs — ✅ WeChatConfig / RawConfig / DetectedAccount /
              KeyConfigPatch（T-228）
  detect.rs— ✅ auto_detect_* / read_ini_content / detect_accounts /
              scan_accounts(_in_dir)（T-230）
  io.rs    — ✅ load(_uncached/refresh) / save_config / patch_config /
              load_raw_config / get_config_path（T-231）
  paths.rs — ✅ default_*_dir / app_base_dir / normalize_wxid_dir（T-229）
  tests.rs — ✅ ini 解析测试（T-232）
```

拆分原则：RawConfig 私有字段提升 pub(crate)（mod.rs 读写）；
外部调用点（各模块 `config::load()` / `WeChatConfig` 等）零改动。

## 切片 T-228（R-208）：config 拆分 — 数据类型

- `wechat/config.rs`（726 行）转目录模块 `config/mod.rs`，
  types 三处非连续块迁入 `types.rs`（103 行）：
  - `WeChatConfig`：路径/密钥/API 配置（camelCase）
  - `RawConfig`：config.json 原始结构（私有字段提升 pub(crate)
    供 mod.rs 读写）
  - `DetectedAccount` / `KeyConfigPatch<'a>`
- `pub use types::*;` 保持外部路径；mod.rs 移除迁出的 serde 导入；
  门面 632 行
- 迁移经 SHA-256 复核：块与写出版本完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-232（R-212）：config 拆分 — 测试域（收尾）

- 新增 `src-tauri/src/wechat/config/tests.rs`（41 行）：
  - `test_read_ini_content_utf8` / `test_read_ini_content_with_nulls`
- mod.rs 收敛为 34 行纯门面（模块文档 + 4 个 DEFAULT_* 常量 +
  5 个子模块声明 + re-export + `#[cfg(test)] mod tests;`）；
  清理过期的「配置结构」段头（types 已迁出）
- 迁移后 2 个 ini 测试在 `config::tests` 下运行通过
- config.rs 726 → 门面 34 + types 103 / paths 58 / detect 275 /
  io 258 / tests 41
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 完成：T-蓝图-19（wechat/config 目录化）

`config.rs` 原始 726 行 → 门面 34 行 + 5 个子模块（见上），
外部调用点（各模块 `config::load()` / `WeChatConfig` 等）零改动；
每个切片均过全部门禁并做 SHA-256 字节级迁移校验。

## 蓝图 T-蓝图-20：wechat/annual.rs 按职责拆分

```
wechat/annual/
  mod.rs  — ✅ 门面：模块声明/re-export + 主计算（T-235）
  types.rs— ✅ MomentItem / TopItem / AnnualSummary（T-233）
  utils.rs— ✅ year_range / local_datetime / fmt_time / fmt_date /
             plain_text / is_valid_phrase / is_emoji_char / kind_label(_zh)
             （T-234）
  scan.rs — ✅ list_shard_dbs / list_msg_tables / load_session_usernames /
             load_display_names / read_text（T-235）
  tests.rs— ✅ smoke_annual_summary_real（T-235）
```

拆分原则：types 零依赖；utils/scan 纯辅助；外部调用点
（handlers 的 `annual::{available_years, annual_summary}`）零改动。

## 切片 T-233（R-213）：annual 拆分 — 数据类型

- `wechat/annual.rs`（703 行）转目录模块 `annual/mod.rs`，
  汇总结构迁入 `types.rs`（42 行）：
  - `MomentItem`：重要瞬间（时间/会话/文本）
  - `TopItem`：榜单（联系人/群/词频/表情）
  - `AnnualSummary`：年度汇总（热力图/月度/词频等）
- `pub use types::*;` 保持 `annual::{AnnualSummary, ...}` 外部路径；
  mod.rs 移除迁出的 serde 导入；门面 669 行
- 迁移经 SHA-256 复核：块与源完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-234（R-214）：annual 拆分 — 工具函数

- 新增 `src-tauri/src/wechat/annual/utils.rs`（110 行）：
  - 时间：`year_range` / `local_datetime` / `fmt_time` / `fmt_date`
  - 文本：`plain_text`（XML 去标签）/ `is_valid_phrase` /
    `is_emoji_char`
  - 类型标签：`kind_label` / `kind_label_zh`
- 两处非连续块迁出，pub(crate) 供主计算使用；依赖
  `crate::wechat::modules::common`（strip_xml_tags/normalize_msg_type）
- mod.rs 显式 `use utils::{...}`（9 个函数）；门面 570 行
- 迁移经 SHA-256 复核：两块与写出版本完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-235（R-215）：annual 拆分 — 消息库扫描域

- 新增 `src-tauri/src/wechat/annual/scan.rs`（111 行）：
  - `list_shard_dbs`：message_/biz_message_ 分片库枚举（升序去重 +
    分片名过滤）
  - `list_msg_tables`：分片内 `Msg_%` 消息表枚举
  - `load_session_usernames` / `load_display_names`：会话 username 与
    显示名（联系人 > SessionNoContactInfoTable > username）
  - `read_text`：message_content 的 blob 文本读取
- `list_shard_dbs/list_msg_tables/read_text` 提升 pub(crate)；
  mod.rs 以 `pub(crate) use scan::{...}` 重导出，保持
  `annual::{load_session_usernames, load_display_names}` 外部调用点
  （chat_search_index/daily_summary/privacy/voice/ask/handlers/insights）
  零改动；contacts 经 `crate::wechat::modules::contacts` 全路径访问
- 测试迁入 `annual/tests.rs`（脱壳去缩进），mod.rs 尾部改
  `#[cfg(test)] mod tests;`；清理悬空的「汇总结构」段头与
  rusqlite/PathBuf 闲置导入；门面 387 行
- 迁移经字节级复核：scan 块与写出版本一致（含 pub(crate) 前缀、
  路径改写与 fmt 折行），测试体去缩进后与源一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 蓝图 T-蓝图-21：wechat/router.rs 按职责拆分

```
wechat/router/
  mod.rs  — 门面：EventRouter 结构 + 生命周期/广播/ACK/状态/replay
  types.rs— ✅ MessageItem / PendingAck / Metrics（含 record_latency /
             Clone impl）
  ws.rs   — ✅ start_ws_server / stop_ws_server / ws_port /
             try_bind / accept_ws_loop / handle_ws_client / broadcast_ws
  batch.rs— ✅ batch_loop / dispatch_batch / dispatch_single /
             record_*_latency / track_ack(s)
  retry.rs— ✅ retry_loop
```

拆分原则：`EventRouter` 结构留在门面，各域以 `impl EventRouter`
子块复用私有字段；`crate::wechat::router::{EventRouter, Metrics}`
外部路径零改动（handlers/monitor、monitor/start、http_api/status、
automation/sse）。常量随使用域迁移（BATCH_* 随 batch，DEFAULT_WS_PORT/
MAX_WS_CLIENTS 随 ws，ACK_TIMEOUT/MAX_RETRY_COUNT 随 retry）。

## 切片 T-236（R-216）：router 拆分 — 数据类型域

- `wechat/router.rs`（696 行）转目录模块 `router/mod.rs`，
  新增 `router/types.rs`（60 行）：
  - `MessageItem`：单条待发送消息（ack_id/text/payload/retries）
  - `PendingAck`：未确认消息元数据（ts/text/retries）
  - `Metrics`：监控指标（默认值 + record_latency + 手写 Clone）
- `MessageItem/PendingAck` 及字段提升 pub(crate)（供门面构造），
  `record_latency` 提升 pub(crate)；`Metrics` 保持 pub 并经
  `pub use types::Metrics;` 重导出，`router::{EventRouter, Metrics}`
  外部路径零改动（handlers/monitor、monitor/start、http_api/status）
- 迁移经字节级复核：三个类型块与 Clone 块写出版本完全一致
  （含可见性前缀与字段修饰）；mod.rs 门面 641 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-237（R-217）：router 拆分 — WebSocket 服务器域

- 新增 `src-tauri/src/wechat/router/ws.rs`（139 行）：
  - 生命周期：`start_ws_server` / `stop_ws_server` / `ws_port`
  - 监听：`try_bind`（起始端口起尝试 10 个）/ `accept_ws_loop`
  - 客户端：`handle_ws_client`（注册/ACK 解析/注销）
  - 广播：`broadcast_ws`（克隆发送端后释放读锁）
- 常量 `DEFAULT_WS_PORT` / `MAX_WS_CLIENTS` 随域迁移；
  `broadcast_ws` 提升 pub(crate)（门面 dispatch_* 回退调用）；
  其余保持私有；`impl EventRouter` 子块直接复用门面私有字段
- mod.rs 移除 SocketAddr/futures_util/TcpListener/TcpStream/
  tungstenite Message 闲置导入；门面 641 → 490 行
- 迁移经字节级复核：三块方法 + 常量与写出版本完全一致
  （含 broadcast_ws 可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-238（R-218）：router 拆分 — 批量聚合与分发域

- 新增 `src-tauri/src/wechat/router/batch.rs`（198 行）：
  - `track_ack` / `track_acks`：pending_acks 登记
  - `batch_loop`：微批聚合（BATCH_MAX_SIZE/WAIT/FLUSH_IDLE）
  - `dispatch_batch` / `dispatch_single`：Tauri Event 优先 +
    WebSocket 回退，含发送指标
  - `record_single_latency` / `record_batch_latency`：端到端延迟
- 常量 `BATCH_MAX_WAIT_MS/BATCH_MAX_SIZE/BATCH_FLUSH_IDLE_MS` 随域迁移；
  `batch_loop` 提升 pub(crate)（门面 new() 启动任务）；
  依赖 `broadcast_ws`（ws.rs pub(crate)）与 `Metrics::record_latency`
- mod.rs 移除批量段与 tokio::time 内相关使用；门面 490 → 279 行
- 迁移经字节级复核：四块（常量/track/loop/dispatch）与写出版本
  完全一致（含 batch_loop 可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-239（R-219）：router 拆分 — 超时重传域

- 新增 `src-tauri/src/wechat/router/retry.rs`（60 行）：
  - `retry_loop`：每 ACK_TIMEOUT 扫描 pending_acks，超时未确认
    且 retries < MAX_RETRY_COUNT 的消息重新入 batch_tx，超限丢弃
- 常量 `ACK_TIMEOUT` / `MAX_RETRY_COUNT` 随域迁移；
  `retry_loop` 提升 pub(crate)（门面 new() 启动任务）
- mod.rs 移除 tokio::time 闲置导入与重传段；门面 279 → 221 行，
  T-蓝图-21 全部完成（router 696 → 221 + types/ws/batch/retry）
- 迁移经字节级复核：重传循环块与常量写出版本完全一致
  （含 retry_loop 可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-240（R-220）：general_records 拆分 — 数据库辅助域

- `wechat/general_records.rs`（490 行）转目录模块 `general_records/mod.rs`，
  新增 `general_records/db.rs`（79 行）：
  - `MAX_LIMIT` / `general_db_path` / `open_general`（只读 + NO_MUTEX）
  - `clamp`（limit 1..=200 / offset 非负钳制）
  - `rows_to_json`（列名 + ValueRef → JSON，blob 经 decode_blob_text）
  - `total`（表行数）
- 六项全部提升 pub(crate)；门面 `pub(crate) use db::{clamp,
  open_general, rows_to_json, total};`（general_db_path 仅供测试，
  测试直接经 `super::db::general_db_path` 路径调用，避免非测试
  build 的 unused_imports 警告）；mod.rs 移除 rusqlite/PathBuf
  闲置导入；门面 490 → 424 行
- 迁移经字节级复核：db 块与写出版本完全一致（含可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-241（R-221）：general_records 拆分 — 统计域

- 新增 `src-tauri/src/wechat/general_records/stats.rs`（117 行）：
  - `stats_transfers`：转账笔数统计（会话/时间过滤，Top 10 会话）
  - `stats_redpackets`：红包个数统计（同上语义）
- 仅依赖 `super::open_general`（db 域）；`pub use stats::*;`
  保持 `general_records::{stats_transfers, stats_redpackets}`
  外部路径（ask/search.rs 调用零改动）
- 迁移经字节级复核：统计块与写出版本完全一致；门面 424 → 315 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-242（R-222）：general_records 拆分 — CSV 导出域

- 新增 `src-tauri/src/wechat/general_records/export.rs`（73 行）：
  - `export_records_csv`：kind 分派（revokes/transfers/redpackets/
    finder/miniprograms）→ rows_to_json → CSV 引号转义
- 仅依赖 `super::{open_general, rows_to_json}`；`pub use export::*;`
  保持 `general_records::export_records_csv` 外部路径
  （handlers/general.rs 调用零改动）
- 迁移经字节级复核：导出块与写出版本完全一致；门面 315 → 269 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-243（R-223）：general_records 拆分 — 列表查询域

- 新增 `src-tauri/src/wechat/general_records/lists.rs`（192 行）：
  - `list_revokes` / `list_transfers` / `list_red_envelopes` /
    `list_finder` / `list_mini_programs` / `list_friend_verifications`
  - `is_record_type_stopword`（私有）：红包/转账等类型词防误伤过滤
- 仅依赖 `super::{clamp, open_general, rows_to_json, total}`（db 域）；
  `pub use lists::*;` 保持 6 个 list_* 外部路径
  （handlers/general、ask/search 调用零改动）
- 迁移经字节级复核：移除区域与提取块逐字符一致；门面 269 → 49 行
  （纯门面：模块声明 + re-export + 测试）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 蓝图 T-蓝图-23：wechat/db_cache.rs 按职责拆分

```
wechat/db_cache/
  mod.rs    — 门面：MonitorDBCache 结构 + new/with_preserved_structure/
              invalidate/peek/get_lock/cache_path + 模块声明/re-export
  types.rs  — ✅ CacheState / KeyCacheEntry
  files.rs  — ✅ sqlite_healthy / stage_one / stage_source_snapshot /
              cleanup_db_staging / replace_decrypted + STAGE_DOUBLE_COPY_LIMIT
  keycache.rs— ✅ derived_key（salt 级派生密钥缓存）
  decrypt.rs— ✅ decrypt_full_atomic（temp+校验+原子替换）
  get.rs    — ✅ get() 编排 + Action 枚举
```

拆分原则：`MonitorDBCache` 结构留在门面，各域以 `impl MonitorDBCache`
子块复用私有字段（descendant 可见性）；`db_cache::{MonitorDBCache,
sqlite_healthy}` 外部路径零改动（handlers/monitor、image/resolve、
monitor/*、image.rs 等调用点）；常量随使用域迁移（STAGE_* 随 files，
DECRYPT_FAIL_COOLDOWN 随 get）。

## 切片 T-245（R-225）：db_cache 拆分 — 数据类型域

- `wechat/db_cache.rs`（587 行）转目录模块 `db_cache/mod.rs`，
  新增 `db_cache/types.rs`（21 行）：
  - `CacheState`：解密缓存状态（db/wal mtime + 失败时间）
  - `KeyCacheEntry`：派生密钥缓存条目（salt + Arc 密钥）
- 结构与字段全部提升 pub(crate)（门面构造/读写）；
  mod.rs 移除类型块；门面 587 → 575 行
- 迁移经字节级复核：类型块与写出版本完全一致（含可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-246（R-226）：db_cache 拆分 — 文件一致性域

- 新增 `src-tauri/src/wechat/db_cache/files.rs`（93 行）：
  - `sqlite_healthy`：解密副本健康校验（sqlite_master 可读）
  - `stage_one` / `stage_source_snapshot`：单文件/主库+WAL 一致性快照
    （≤128MB 双复制逐字节，>128MB 单复制 + mtime/size 校验）
  - `cleanup_db_staging` / `replace_decrypted`：暂存清理与原子替换
- 常量 `STAGE_DOUBLE_COPY_LIMIT` 随域迁移；
  `stage_one/stage_source_snapshot/cleanup_db_staging/replace_decrypted`
  提升 pub(crate)（门面 decrypt_full_atomic/get 调用）；
  `sqlite_healthy` 经门面 pub(crate) 重导出，外部路径
  `db_cache::sqlite_healthy` 零改动（handlers/*、monitor/query）
- 迁移经字节级复核：移除区域与 5 块写出版本逐字符一致；
  门面 575 → 474 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-247（R-227）：db_cache 拆分 — 派生密钥缓存域

- 新增 `src-tauri/src/wechat/db_cache/keycache.rs`（47 行）：
  - `derived_key`：v4.0 raw key 直返 / v4.1 per-DB salt 读首 32B
    做 PBKDF2(256k) 派生，salt 未变命中 `key_cache`
- `derived_key` 提升 pub(crate)（门面 decrypt_full_atomic/get 调用）；
  依赖 `types::KeyCacheEntry` 与 `crypto::{derive_enc_key, SALT_SZ}`
- mod.rs 精简 crypto 导入（derive_enc_key/SALT_SZ 随迁）、保留
  KeyCacheEntry（结构字段）；门面 474 → 436 行
- 迁移经字节级复核：derived_key 块与写出版本完全一致
  （含可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-248（R-228）：db_cache 拆分 — 全量解密域

- 新增 `src-tauri/src/wechat/db_cache/decrypt.rs`（97 行）：
  - `decrypt_full_atomic`：快照 → 全量解密 → WAL patch →
    sqlite_healthy 校验 → 原子替换（失败只丢 temp，正本始终合法）
- 依赖 `keycache::derived_key`、files 域 4 函数与
  `crypto::{KEY_SZ, full_decrypt, decrypt_wal}`；
  `decrypt_full_atomic` 提升 pub(crate)（门面 get 调用）
- mod.rs 移除 crypto 导入与 `Path`（get 仅剩 PathBuf 用途），
  保留 `crypto_decrypt_wal` 别名（get 的 WAL patch 分支使用）；
  门面 436 → 333 行
- 迁移经字节级复核：decrypt 块与写出版本完全一致
  （含可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-249（R-229）：db_cache 拆分 — 获取编排域

- 新增 `src-tauri/src/wechat/db_cache/get.rs`（213 行）：
  - `get()`：mtime 决策（Nothing/WalPatch/Full）+ 失败冷却 +
    基线推进；嵌套 `Action` 枚举随迁
  - 常量 `DECRYPT_FAIL_COOLDOWN` 随域迁移
- 依赖 keycache/decrypt/files 各域（均 pub(crate)）与门面私有方法
  `get_lock/cache_path`（descendant 可见性）；`get` 保持 pub
- mod.rs 移除冷却常量与 crypto/时间导入；门面 333 → 105 行，
  T-蓝图-23 全部完成（db_cache 587 → 105 行门面 +
  types/files/keycache/decrypt/get）
- 迁移经字节级复核：get 块 + 冷却常量与写出版本完全一致；
  补回 impl 收尾 `}` 与 CacheState/cleanup_db_staging 依赖
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-250（R-230）：origin_ilink 拆分 — 数据类型域

- `wechat/origin_ilink.rs`（557 行）转目录模块 `origin_ilink/mod.rs`，
  新增 `origin_ilink/types.rs`（23 行）：
  - `OriginSecret`：原图密钥（file_id/aes_key/md5/original_size）
  - `IlinkStatus`：通道可用性快照（Serialize）
- 两类型保持 pub，`pub use types::*;` 保持
  `origin_ilink::{OriginSecret, IlinkStatus}` 外部路径
  （handlers/data.rs 的 IlinkStatus 零改动）；门面 557 → 541 行
- 迁移经字节级复核：类型块与写出版本完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-251（R-231）：origin_ilink 拆分 — 路径与版本护栏域

- 新增 `src-tauri/src/wechat/origin_ilink/paths.rs`（156 行）：
  - 资源定位：`origin_exe_path` / `origin_bridge_path`
  - 安装目录：`wechat_install_dir`（注册表/进程双路）+ 
    `locate_weixin_exe_process`（Toolhelp 进程扫描）
  - 沙箱：`sandbox_dir`（st_result/origin_ilink）
  - 护栏：`ilink_compatible`（白名单 / compat_ok）
- 常量 `KNOWN_ILINK_VERSIONS` 随迁 pub(crate)；门面
  `pub use paths::wechat_install_dir;` + pub(crate) 重导出其余
  （locate_weixin_exe_process 仅 paths 内部使用，不重导出）；
  mod.rs 移除 auto_key/config 闲置导入；门面 541 → 378 行
- 迁移经字节级复核：6 块 + 常量与写出版本完全一致
  （含可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-252（R-232）：origin_ilink 拆分 — 隔离沙箱域

- 新增 `src-tauri/src/wechat/origin_ilink/sandbox.rs`（69 行）：
  - `build_start_config_bytes` / `encode_varint`：ilink 启动配置
    protobuf 式编码（字段 1=data_root，6=client_version）
  - `read_kv_client_version`：kvcomm/config.ini 版本读取
  - `ensure_sandbox`：目录 + 复制真实 cloud_account.txt/kvcomm
    会话 + 启动配置（全部落在 st_result/origin_ilink）
- `ensure_sandbox` 提升 pub(crate)（门面 ilink_status/download 调用）；
  仅依赖 `super::sandbox_dir`（paths 域）；build/encode/read 保持私有
- mod.rs 移除沙箱段与 PathBuf 闲置导入；门面 378 → 310 行
- 迁移经字节级复核：沙箱块与写出版本完全一致（含可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-253（R-233）：origin_ilink 拆分 — 消息 XML 解析域

- 新增 `src-tauri/src/wechat/origin_ilink/extract.rs`（66 行）：
  - `extract_image_xml`：分片库枚举 + zstd 解压 + <msg> 提取
    （message_content/compress_content 兼容）
  - `parse_origin_secret`：cdnbigimgurl/aeskey/md5/hdlength 属性解析
- 两函数提升 pub(crate)（门面 download 调用）；依赖
  modules::common（msg_table_name/find_db_files/is_message_shard_file）
  与 types::OriginSecret；mod.rs 移除 rusqlite/Path/common 全量
  导入（msg_table_name 保留供 download）；门面 310 → 245 行
- 迁移经字节级复核：extract 块与写出版本完全一致（含可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-254（R-234）：origin_ilink 拆分 — 下载主流程域

- 新增 `src-tauri/src/wechat/origin_ilink/download.rs`（176 行）：
  - `run_with_timeout`：200ms 轮询 + 超时 kill
  - `ilink_status`：五要素（安装/桥接/下载器/沙箱/版本护栏）可用性快照
  - `download_origin_via_ilink`：定位 → 沙箱 → XML/密钥 → 下载器
    参数组装 → 大小/MD5 校验 → compat_ok 放行标记
- 常量 `DOWNLOAD_TIMEOUT` 随迁；`pub use download::*;` 保持
  `origin_ilink::{download_origin_via_ilink, ilink_status}` 外部路径
  （handlers/data.rs 零改动）；mod.rs 移除全部闲置导入；
  门面 245 → 42 行（纯门面：模块声明 + re-export + 测试）
- 迁移经字节级复核：download 块 + 常量与写出版本完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-255（R-235）：origin_ilink 拆分 — 测试域

- 新增 `src-tauri/src/wechat/origin_ilink/tests.rs`（24 行）：
  `ilink_fallback_real_image` 迁出（脱壳去缩进，测试体逐行一致）；
  mod.rs 尾部改 `#[cfg(test)] mod tests;`，T-蓝图-24 全部完成
  （origin_ilink 557 → 25 行门面 + types/paths/sandbox/extract/
  download/tests）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-256（R-236）：cdn_image 拆分 — 配置域

- `wechat/cdn_image.rs`（567 行）转目录模块 `cdn_image/mod.rs`，
  新增 `cdn_image/settings.rs`（49 行）：
  - `read/write_cdn_settings`：decoded_dir/.cdn_settings.json 读写
  - `is/set_cdn_enabled`、`is/set_cdn_local_decrypt`：开关与
    解密方式（默认本地 AES-ECB）
- `pub use settings::*;` 保持 4 个外部开关路径
  （handlers/general.rs 零改动）；read/write 保持私有；
  门面 567 → 515 行
- 迁移经字节级复核：3 块与写出版本完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-257（R-237）：cdn_image 拆分 — token 域

- 新增 `src-tauri/src/wechat/cdn_image/token.rs`（126 行）：
  - `TOKEN_URL` / `TOKEN_TTL` + `TOKEN_CACHE` / `TOKEN_WXID_CACHE`
    静态（45 分钟缓存 / 成功目录名缓存）
  - `global_config_paths` / `try_fetch_token`（curl POST 换 token）
  - `fetch_cdn_token`：候选账号枚举（请求账号 → 缓存目录 → 同级
    wxid_* 目录）
- `pub use token::fetch_cdn_token;` 保持外部路径；其余保持私有；
  mod.rs 移除 HashMap/OnceLock/Mutex/Instant 闲置导入；
  门面 515 → 364 行
- 迁移经字节级复核：4 块与写出版本完全一致（DOWNLOAD_URL 留门面）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-258（R-238）：cdn_image 拆分 — 下载与解密域

- 新增 `src-tauri/src/wechat/cdn_image/download.rs`（76 行）：
  - `DOWNLOAD_URL` 常量随迁
  - `download_original_image`：fileid 校验 → type=orig URL →
    token → curl GET → 服务端/本地解密（detect_image_format /
    decode_cdn_aes_key / aes_ecb_decrypt_file）
  - `curl_get_bytes`（--max-time 15 快速失败）
- `pub use download::download_original_image;` 保持外部路径；
  依赖 `super::{fetch_cdn_token, is_cdn_local_decrypt}`；
  mod.rs 移除 Command 闲置导入；门面 364 → 271 行
- 迁移经字节级复核：常量 + 下载块与写出版本完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-259（R-239）：cdn_image 拆分 — 消息 XML 解析域

- 新增 `src-tauri/src/wechat/cdn_image/xml.rs`（142 行）：
  - `CdnMediaRow` + `find_image_message_xml`：分片枚举 + local_type=3
    行查询（message_content/compress_content）
  - `extract_cdn_info_from_xml` / `extract_xml_value`：属性/标签/
    CDATA 取值（含 cdnbigimgurl 判定）
  - `lookup_image_cdn_info` / `lookup_image_md5_variants`：对外查询
- `pub use xml::{lookup_image_cdn_info, lookup_image_md5_variants};`
  + `pub(crate) use xml::extract_xml_value;`（missing_images 调用
  零改动）；其余保持私有；mod.rs 移除 common 导入并恢复 Path/PathBuf
  （fallback 仍用）；门面 271 → 109 行
- 迁移经字节级复核：6 块与写出版本完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-260（R-240）：cdn_image 拆分 — 回退编排域

- 新增 `src-tauri/src/wechat/cdn_image/fallback.rs`（69 行）：
  - `try_cdn_fallback`：开关 → wxid 目录解析 → XML → 下载 →
    fileid 缓存
  - `looks_like_account_dir` / `resolve_wxid_dir`：账号目录判定
    与 clean wxid → 实际目录解析
- `pub use fallback::{resolve_wxid_dir, try_cdn_fallback};` 保持
  外部路径（image/resolve、image.rs 零改动）；looks_like 保持私有；
  mod.rs 移除 Path/PathBuf 导入；门面 109 → 25 行（纯门面），
  T-蓝图-25 全部完成（cdn_image 567 → 25 行门面 +
  settings/token/download/xml/fallback）
- 迁移经字节级复核：回退块与写出版本完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-261（R-241）：sns_image 拆分 — ISAAC-64 密码核域

- `wechat/sns_image.rs`（564 行）转目录模块 `sns_image/mod.rs`，
  新增 `sns_image/isaac.rs`（178 行）：
  - `GOLDEN` 常量 + `Isaac64` 结构（new/isaac64/next_u64/keystream）
  - `mix` 八状态字内联函数（保留扁平参数 + clippy allow）
- `Isaac64::new/keystream` 与结构提升 pub(crate)（net/video 域使用），
  门面 `pub(crate) use isaac::Isaac64;`；isaac64/next_u64/mix 私有；
  门面 564 → 388 行
- 迁移经字节级复核：GOLDEN + ISAAC 段与写出版本完全一致
  （含可见性前缀）；keystream 参考值测试继续通过
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-262（R-242）：sns_image 拆分 — 下载与解密域

- 新增 `src-tauri/src/wechat/sns_image/net.rs`（120 行）：
  - `diag_log`：moment_image.log 诊断日志
  - `sniff_image`：JPEG/PNG/GIF/WebP/BMP/AVIF/HEIC 头部嗅探
  - `normalize_cdn_url` / `download_raw` / `fetch_and_decrypt`：
    CDN 直连下载（no_proxy）+ ISAAC XOR 解密
  - `data_url`：base64 data URL 组装
- 常量 `DOWNLOAD_TIMEOUT` / `MAX_IMAGE_BYTES` 随迁；
  diag/sniff/normalize/fetch/data_url 提升 pub(crate)
  （image/video 域使用），download_raw 保持私有；
  mod.rs 移除工具段；门面 388 → 248 行
- 迁移经字节级复核：常量 + diag + 工具段与写出版本完全一致
  （含可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-263（R-243）：sns_image 拆分 — 图片解析域

- 新增 `src-tauri/src/wechat/sns_image/image.rs`（51 行）：
  - `resolve_moment_image_data_url`：标准化 URL → md5 磁盘缓存
    （moments/）→ fetch_and_decrypt → data URL
- 仅依赖 net 域 4 函数与 md5；`pub use image::
  resolve_moment_image_data_url;` 保持外部路径
  （handlers/data.rs 零改动）
- 迁移经字节级复核：图片块与写出版本完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-264（R-244）：sns_image 拆分 — 视频解析域

- 新增 `src-tauri/src/wechat/sns_image/video.rs`（111 行）：
  - `moment_video_file_key`：标准化 URL 的 MD5
  - `resolve_moment_video`：120s 下载 → 前 128KB ISAAC 解密 →
    ftyp 校验 → moments_video/ 缓存
- 常量 `MAX_VIDEO_BYTES` / `VIDEO_DECRYPT_HEAD` 随迁；
  `pub use video::{moment_video_file_key, resolve_moment_video};`
  保持外部路径；mod.rs 移除 md5/Path/Duration 导入；
  门面 248 → 78 行
- 迁移经字节级复核：视频块与写出版本完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-265（R-245）：sns_image 拆分 — 测试域

- 新增 `src-tauri/src/wechat/sns_image/tests.rs`（52 行）：
  `keystream_matches_wechat_wasm` / `normalize_appends_token` /
  `sniff_common_formats` 迁出（脱壳去缩进，测试体逐行一致）；
  mod.rs 尾部改 `#[cfg(test)] mod tests;`，T-蓝图-26 全部完成
  （sns_image 564 → 27 行门面 + isaac/net/image/video/tests）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-266（R-246）：hevc 拆分 — SPS 位流解析域

- `wechat/hevc.rs`（539 行）转目录模块 `hevc/mod.rs`，
  新增 `hevc/sps.rs`（108 行）：
  - `BitReader`：bit/bits/unsigned Exp-Golomb 读取
  - `parse_sps_dimensions`：Annex-B 起始码扫描 + SPS NAL 定位
  - `parse_sps_rbsp`：profile_tier_level 96bit 跳过 + 宽高解析
- `parse_sps_dimensions` 提升 pub(crate)（mft 域使用）并重导出；
  `BitReader` 及方法提升 pub(crate)（测试经 super::sps 路径调用）；
  parse_sps_rbsp 保持私有；门面 539 → 405 行
- 迁移经字节级复核：SPS 三段与写出版本完全一致（含可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-267（R-247）：hevc 拆分 — 容器/像素/JPEG 域

- 新增 `src-tauri/src/wechat/hevc/pixel.rs`（57 行）：
  - `strip_wxgf_header`：VPS NAL 起始码定位（裸 HEVC 兼容）
  - `nv12_to_rgb`：BT.601 limited range 转换
  - `encode_jpeg`：jpeg-encoder 质量 85 编码
- 三函数提升 pub(crate)（门面 wxgf_to_jpeg / mft 域使用）并重导出；
  mod.rs 移除容器/像素/编码段；门面 405 → 336 行
- 迁移经字节级复核：两块与写出版本完全一致（含可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-268（R-248）：hevc 拆分 — Media Foundation 解码域

- 新增 `src-tauri/src/wechat/hevc/mft.rs`（232 行）：
  - `decode_hevc_to_rgb`：COM/MF 初始化 + 收尾
  - `decode_inner`：MFTEnumEx（HEVC→H265 回退）→ 媒体类型协商
    （MF_MT_FRAME_SIZE 补齐）→ 整段码流 ProcessInput → DRAIN →
    第一帧 NV12
  - `OutputState` / `try_output` / `drain_output` / `extract_nv12`
- 子模块自带 `#![allow(non_snake_case)]`（COM 字段名）；
  windows 导入随迁；`decode_hevc_to_rgb` 提升 pub(crate) 并重导出；
  仅依赖 `super::{nv12_to_rgb, parse_sps_dimensions}`；
  mod.rs 移除 COM 段与尾部解码段；门面 336 → 71 行
- 迁移经字节级复核：两块与写出版本完全一致（含可见性前缀）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-269（R-249）：hevc 拆分 — 测试域

- 新增 `src-tauri/src/wechat/hevc/tests.rs`（37 行）：
  `test_strip_wxgf_header` / `test_bitreader_ue` /
  `test_nv12_to_rgb_gray` 迁出（脱壳去缩进，测试体逐行一致）；
  mod.rs 尾部改 `#[cfg(test)] mod tests;`，T-蓝图-27 全部完成
  （hevc 539 → 35 行门面 + sps/pixel/mft/tests）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 蓝图 T-蓝图-27：wechat/hevc.rs 按职责拆分

```
wechat/hevc/
  mod.rs — 门面：wxgf_to_jpeg + 模块声明/re-export + #![cfg] 属性
  sps.rs — ✅ BitReader / parse_sps_dimensions / parse_sps_rbsp
  pixel.rs— ✅ strip_wxgf_header / nv12_to_rgb / encode_jpeg
  mft.rs — ✅ decode_hevc_to_rgb / decode_inner / OutputState /
            try_output / drain_output / extract_nv12
  tests.rs— ✅ test_strip_wxgf_header / test_bitreader_ue /
            test_nv12_to_rgb_gray
```

拆分原则：sps 纯位流解析零依赖；pixel 容器/像素/JPEG 纯函数；
mft 仅依赖 sps+pixel（经门面 pub(crate) 重导出）；`hevc::wxgf_to_jpeg`
外部路径零改动（image/resolve.rs 调用点）；mft 子模块自带
`#![allow(non_snake_case)]`（COM 字段名）。

## 蓝图 T-蓝图-26：wechat/sns_image.rs 按职责拆分

```
wechat/sns_image/
  mod.rs  — 门面：模块文档 + 子模块声明 + re-export
  isaac.rs— ✅ GOLDEN + Isaac64 + mix（纯密码核）
  net.rs  — ✅ diag_log / sniff_image / normalize_cdn_url /
            download_raw / fetch_and_decrypt / data_url +
            DOWNLOAD_TIMEOUT / MAX_IMAGE_BYTES
  image.rs— ✅ resolve_moment_image_data_url（下载+解密+缓存）
  video.rs— ✅ moment_video_file_key / resolve_moment_video +
            MAX_VIDEO_BYTES / VIDEO_DECRYPT_HEAD
  tests.rs— ✅ keystream_matches_wechat_wasm / normalize_appends_token /
            sniff_common_formats
```

拆分原则：isaac 零依赖纯算法；net 网络与解密；image/video 对外
入口仅依赖 net+isaac；`sns_image::{resolve_moment_image_data_url,
moment_video_file_key, resolve_moment_video}` 外部路径零改动
（handlers/data.rs 调用点）；常量随使用域迁移。

## 蓝图 T-蓝图-25：wechat/cdn_image.rs 按职责拆分

```
wechat/cdn_image/
  mod.rs    — 门面：模块文档 + 子模块声明 + re-export
  settings.rs— ✅ read/write_cdn_settings + is/set_cdn_enabled +
              is/set_cdn_local_decrypt
  token.rs  — ✅ TOKEN_URL/TOKEN_TTL + TOKEN_CACHE 静态 +
              global_config_paths / try_fetch_token / fetch_cdn_token
  download.rs— ✅ DOWNLOAD_URL + download_original_image + curl_get_bytes
  xml.rs    — ✅ CdnMediaRow + extract_cdn_info_from_xml /
              extract_xml_value / find_image_message_xml /
              lookup_image_cdn_info / lookup_image_md5_variants
  fallback.rs— ✅ try_cdn_fallback + looks_like_account_dir /
              resolve_wxid_dir
```

拆分原则：settings 配置持久化；token 换取与缓存；download 网络与
本地解密；xml 消息解析；fallback 编排；`cdn_image::{11 个外部函数}`
路径零改动（handlers/general、image/resolve、image、missing_images）；
常量/静态随使用域迁移。

## 蓝图 T-蓝图-24：wechat/origin_ilink.rs 按职责拆分

```
wechat/origin_ilink/
  mod.rs    — 门面：模块文档 + 子模块声明 + re-export
  types.rs  — ✅ OriginSecret / IlinkStatus
  paths.rs  — ✅ origin_exe_path / origin_bridge_path /
              wechat_install_dir / locate_weixin_exe_process /
              sandbox_dir / ilink_compatible + KNOWN_ILINK_VERSIONS
  sandbox.rs— ✅ build_start_config_bytes / encode_varint /
              read_kv_client_version / ensure_sandbox
  extract.rs— ✅ extract_image_xml / parse_origin_secret
  download.rs— ✅ run_with_timeout / ilink_status /
              download_origin_via_ilink + DOWNLOAD_TIMEOUT
  tests.rs  — ✅ ilink_fallback_real_image
```

拆分原则：types 零依赖；paths 定位/护栏；sandbox 隔离环境；
extract 消息 XML 解析；download 主流程编排；`origin_ilink::
{download_origin_via_ilink, ilink_status, IlinkStatus}` 外部路径
零改动（handlers/data.rs 调用点）；常量随使用域迁移。

## 切片 T-244（R-224）：general_records 拆分 — 测试域

- 新增 `src-tauri/src/wechat/general_records/tests.rs`（47 行）：
  `smoke_general_records` 迁出（脱壳去缩进，测试体逐行一致）；
  mod.rs 尾部改 `#[cfg(test)] mod tests;`，T-蓝图-22 全部完成
  （general_records 490 → 49 行门面 + db/export/lists/stats/tests）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 蓝图 T-蓝图-22：wechat/general_records.rs 按职责拆分

```
wechat/general_records/
  mod.rs  — 门面：模块文档 + 子模块声明 + re-export + 测试声明
  db.rs   — ✅ MAX_LIMIT / general_db_path / open_general / clamp /
             rows_to_json / total
  export.rs— ✅ export_records_csv
  lists.rs— ✅ list_revokes / list_transfers / list_red_envelopes /
             list_finder / list_mini_programs / list_friend_verifications
  stats.rs— ✅ stats_transfers / stats_redpackets
  tests.rs— ✅ smoke_general_records
```

拆分原则：db 纯辅助零耦合；export/lists/stats 均只依赖 db 辅助；
`crate::wechat::general_records::{9 个 pub 函数}` 外部调用点
（handlers/general、ask/search）经门面 re-export 零改动。

## 切片 T-231（R-211）：config 拆分 — 加载/保存/补丁域

- 新增 `src-tauri/src/wechat/config/io.rs`（258 行）：
  - `CONFIG_CACHE` 静态 + `impl WeChatConfig`（load/refresh_cache/
    load_uncached/wxid/has_keys）
  - `load_raw_config` / `get_config_path` / `load_raw_config_public` /
    `save_config` / `patch_config`
- 依赖经 super re-export（paths/detect 函数、types、DEFAULT_* 常量）；
  测试直接 `use super::detect::read_ini_content`（pub(crate) 不随
  pub glob 重导出）；mod.rs 移除闲置的 Path/PathBuf 导入
- 迁移经 SHA-256 复核：两块与写出版本完全一致
- config/mod.rs 314 → 74 行（常量 + 模块声明 + 测试）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-230（R-210）：config 拆分 — 目录检测与账号扫描域

- 新增 `src-tauri/src/wechat/config/detect.rs`（275 行）：
  - `auto_detect_db_dir` / `auto_detect_windows/linux/macos`：
    跨平台 xwechat 数据目录定位（APPDATA ini / HOME / 注册表）
  - `read_ini_content`（UTF-8/GBK 兼容）/ `dir_mtime` /
    `choose_candidate`
  - `detect_accounts` / `scan_accounts(_in_dir)`：账号枚举
    （wxid + db_storage + 活跃度）
- 依赖：types（DetectedAccount）；`pub use detect::*;` 保持
  load_uncached 与 handlers 的调用路径；mod.rs 清理两个悬空文档
  注释与过期段头
- 迁移经 SHA-256 复核：两块与写出版本完全一致
- config/mod.rs 583 → 314 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-229（R-209）：config 拆分 — 路径解析域

- 新增 `src-tauri/src/wechat/config/paths.rs`（58 行）：
  - `default_st_result_dir` / `default_decrypted_dir` /
    `default_decoded_image_dir`：默认目录（AppData\Roaming\st_result）
  - `app_base_dir`：环境变量优先，回退当前工作目录
  - `normalize_wxid_dir`：目录名 → 真实 wxid（去掉实例后缀）
- 三处非连续块迁出，零耦合；`pub use paths::*;` 保持外部路径
  （load/get_config_path/patch_config 经 re-export 使用）
- 迁移经 SHA-256 复核：块与写出版本完全一致
- config/mod.rs 632 → 583 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-226（R-206）：insights 拆分 — 图谱构建域

- 新增 `src-tauri/src/wechat/insights/graph.rs`（519 行）：
  - `shared_group_pairs` / `member_group_map`：共群关系与成员映射
  - `collect_self_accounts`：自我账号收集（多账号去重）
  - `build_relationship_graph`（pub 重导出）：节点/边组装 +
    消息强度/共群边 + 进度事件
- 交叉依赖：emit_graph_final/emit_progress（progress re-export）、
  msg_stats_cached（cache re-export）、GraphEmitCtx；
  `graph_cache_path` 提升 pub(crate)（graph 构建使用，api 域共享）
- mod.rs 清理闲置导入（normalize_wxid_dir/modules/HashMap/HashSet/
  Path，保留 helpers 与 PathBuf）；门面 151 行
- 迁移经写时 -cne 字节校验 + fmt 折行（预期）+ 全部门禁
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-225（R-205）：insights 拆分 — 高性能会话统计域

- 新增 `src-tauri/src/wechat/insights/stats.rs`（198 行）：
  - `SessionStats` / `CountMap` 类型（pub(crate)，cache/progress 共用）
  - `collect_msg_counts`：并行分库扫描 + 表→分库映射（阶段一）
  - `collect_active_days`：对目标会话并行 DISTINCT date（阶段二）
- 交叉依赖处理：progress/cache 的 `super::CountMap` 等改指
  `super::stats::{...}`；stats 导入 progress 的 emit_* 函数与
  cache 的 message_shards（经 mod.rs re-export）；mod.rs 移除
  闲置的 Mutex 与 stats 导入
- 迁移经 SHA-256 复核：还原类型可见性后块与源一致
- insights/mod.rs 840 → 654 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-222（R-202）：messages 拆分 — 查询编排与转账状态域

- 新增 `src-tauri/src/wechat/modules/messages/query.rs`（423 行）：
  - 转账状态类型（TransferStatus/Entry/CacheKey）+ `transfer_status_map`
  - `query_shard_rows`：单库游标查询（RawRow 结构体随迁）
  - `get_conversation_messages`（pub 重导出）：跨分库合并排序 +
    转账去重/方向 + XML 渲染 + 游标分页
- 修正层级：`super::common/contacts` 依赖 mod.rs 再导入，已改
  `crate::wechat::modules::{common, contacts}`（shards/parse 同步）；
  query_shard_rows/transfer_status_map 回退私有（测试仅调
  get_conversation_messages，避免 private_interfaces 告警）
- mod.rs 收敛为 146 行门面（mod 声明 + re-export + 测试）
- 迁移经 SHA-256 复核：还原可见性后块与源一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-221（R-201）：messages 拆分 — XML 富媒体解析域

- 新增 `src-tauri/src/wechat/modules/messages/parse.rs`（154 行）：
  - `parse_display_content`（pub(crate)，get_conversation_messages
    调用）：图片/语音/视频/表情/文件/链接/引用/转账/系统/撤回等
    消息类型的 PC 风格显示内容 + 结构化附加字段
- 依赖：common（媒体路径/时间格式化等）；纯手写 XML 解析（无
  quick-xml 依赖）
- mod.rs `mod parse; use parse::parse_display_content;`；门面 570 行
- 迁移经 SHA-256 复核：块与写出版本完全一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-220（R-200）：messages 拆分 — 分库管理与索引缓存域

- 新增 `src-tauri/src/wechat/modules/messages/shards.rs`（229 行）：
  - 分库类型：MsgShard / ShardMeta / ShardIndexEntry / ShardCacheKey
  - `SHARD_INDEX_CACHE` / `SHARD_INDEX_MAX_ENTRIES`（LRU 兜底 64）
  - `open_shard_from_meta` / `load_name2id` / `open_shards`：
    只读连接 + 会话分库索引（文件签名失效判断）
- 全部提升 pub(crate) 供 mod.rs 查询编排使用；MsgShard 字段
  pub(crate)（query_shard_rows/transfer_status_map 直读
  conn/path/name2id/count）
- mod.rs 移除迁出的 Connection/Arc/SystemTime 导入；门面 714 行
- 迁移经 SHA-256 复核：还原字段可见性后块与源一致
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-217（R-197）：sql_browse 拆分 — 查询域

- 新增 `src-tauri/src/sql_browse/query.rs`（415 行）：
  - `list_tables`：全表列出（含系统表）
  - `table_schema`：PRAGMA table_info 列结构
  - `query_table`：过滤/排序/keyset 分页 + COUNT 可选（大表性能）
- 依赖：types（ColumnInfo/TableData/TableQueryParams）、utils
  （safe_name/escape_like/friendly_db_error）、convert
  （blob_to_preview）；`pub use query::*;` 保持外部路径
- 移回 list_tables 文档注释（迁出时留在门面）；更新过时的
  「增强能力」段头（SQL 执行/整表导出已迁出）
- 迁移经 SHA-256 复核：块与源完全一致
- sql_browse/mod.rs 709 → 306 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-216（R-196）：sql_browse 拆分 — SQL 执行域

- 新增 `src-tauri/src/sql_browse/execute.rs`（86 行）：
  - `first_keyword`：SQL 首个关键字提取（跳过注释行）
  - `execute_sql`：读写判断（readonly 仅允许查询类）+ 查询返回
    columns/rows（限行）、写语句返回 affected
- 依赖：`super::{friendly_db_error, row_to_json}`（row_to_json 仍
  留门面，子模块经 super 访问私有项）
- mod.rs 清理迁出后悬空的首个关键字文档注释
- 迁移经 SHA-256 复核：块与源完全一致
- sql_browse/mod.rs 785 → 709 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-215（R-195）：sql_browse 拆分 — CSV 导出域

- 新增 `src-tauri/src/sql_browse/export.rs`（94 行）：
  - `csv_escape`：引号包裹 + 双引号转义 + 换行转空格
  - `export_table_to_csv`：整表分块流式导出（BOM + 2000 行/批）
- 依赖：`super::{table_schema, safe_name, friendly_db_error}`
- 注：块延伸到文件末尾，搬移脚本产生越界尾切片（重复末行），
  已校验并修正（mod.rs 787 → 785）；块与源 SHA-256 完全一致
- sql_browse/mod.rs 868 → 785 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-214（R-194）：sql_browse 拆分 — 值转换域

- 新增 `src-tauri/src/sql_browse/convert.rs`（120 行）：
  - `read_cell`：单元格原始值读取（完整 BLOB/文本查看）
  - `cell_value_to_json` / `json_to_sql_value`：JSON ↔ SQL 值互转
  - `blob_to_preview` / `guess_mime`：BLOB 预览与 MIME 嗅探
- 依赖：utils（safe_name/friendly_db_error，super）、rusqlite
  （Connection/params）；`pub use convert::*;` 保持外部路径
- 迁移经 SHA-256 复核：块与源完全一致
- sql_browse/mod.rs 977 → 868 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-213（R-193）：sql_browse 拆分 — 工具函数

- 新增 `src-tauri/src/sql_browse/utils.rs`（37 行）：
  - `safe_name`：标识符双引号转义
  - `escape_like`：LIKE 通配符转义（ESCAPE '\'）
  - `friendly_db_error`：SQLite 错误码 → 可操作中文提示
    （损坏/占用/无法打开）
- 两处非连续块迁出；零依赖纯函数；`pub use utils::*;` 保持
  `sql_browse::{safe_name, escape_like, friendly_db_error}` 外部路径
- 迁移经 SHA-256 复核：两块与源完全一致
- sql_browse/mod.rs 1007 → 977 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 蓝图 T-蓝图-15：system_metrics.rs 按采集域拆分

```
system_metrics/
  mod.rs    — 门面：DiskInfo/MetricsSnapshot/MetricsInner/SystemMetrics
              + snapshot 聚合 + get_realtime_metrics + 测试
  ping.rs   — ✅ 网络延迟（T-209）
  gpu.rs    — ✅ PDH GPU 查询 + nvidia-smi/PowerShell 回退链（T-210）
  io.rs     — ✅ 磁盘/网络 PDH 查询与吞吐采集（T-211）
  types.rs  — 候选：MetricsSnapshot 大结构拆分（现含 PDH 句柄类型）
```

拆分原则：各采集域独立维护缓存 static 与句柄，经 pub(crate) 供
snapshot 聚合；外部调用点（lib.rs 的 `system_metrics::SystemMetrics`、
IPC `get_realtime_metrics`）零改动。

## 切片 T-209（R-189）：system_metrics 拆分 — 网络延迟域

- `system_metrics.rs`（1115 行）转目录模块 `system_metrics/mod.rs`，
  ping 簇迁入 `ping.rs`（80 行）：
  - `PING_CACHE` 静态（pub(crate)，core 的 ping_latency_cached 共用）
  - `ping_targets` / `default_gateway`（双 cfg 变体）/
    `ping_latency_ms` / `parse_first_ms`（均 pub(crate) 供测试）
- mod.rs `use ping::{ping_latency_ms, PING_CACHE};`；测试直接
  `use super::ping::{default_gateway, parse_first_ms, ping_targets};`；
  清理迁出后的悬空文档注释
- 迁移经 SHA-256 复核：块与写出版本完全一致
- system_metrics.rs 1115 → 门面 1042 + ping 80
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-211（R-191）：system_metrics 拆分 — 磁盘/网络 IO 域

- 新增 `src-tauri/src/system_metrics/io.rs`（281 行）：
  - `open_disk_query` / `open_net_query`（pub(crate)，new() 调用）
  - `is_network_instance` / `net_utilization_pct`（pub(crate)，测试
    引用）/ `collect_disk` / `collect_net`（pub(crate)，snapshot
    调用）+ 两个 PDH 辅助 fn
- DiskHandle/NetHandle 提升 pub(crate；mod.rs 保留句柄结构体，
  仅需 PDH_HCOUNTER/PDH_HQUERY 类型导入（PDH 函数全部随域迁出）
- 修正 T-210 拼接遗留的游离 `#[cfg(windows)]`（会导致非 Windows
  构建 `mod gpu` 消失）；清理 mod.rs 闲置的 HashMap/PCWSTR/PDH
  函数导入
- 迁移经 SHA-256 复核：块与写出版本完全一致
- system_metrics.rs 1115 → 门面 525 + ping 80 + gpu 266 + io 281
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-210（R-190）：system_metrics 拆分 — GPU 采集域

- 新增 `src-tauri/src/system_metrics/gpu.rs`（266 行）：
  - PDH 查询：`open_gpu_query`（pub(crate)，new() 调用）/
    `read_gpu` / `busiest_engine`（pub(crate)，测试引用）/
    `instance_is_total`
  - 回退链：`collect_gpu_usage`（pub(crate)，snapshot 调用）/
    `nvidia_smi_gpu_usage` / `powershell_gpu_usage`
  - `query_gpu_name`（pub(crate)）+ GPU_FALLBACK_CACHE 静态随迁
- GpuHandle 提升 pub(crate)（MetricsInner 仍在门面，子模块经 super
  引用）；gpu.rs 自带 windows PDH 导入（移除未用的单值 API）
- mod.rs 收敛为 794 行；测试直接 `use super::gpu::busiest_engine`；
  清理迁出后的悬空 cfg/文档注释
- 迁移经写时 -cne 字节校验 + fmt 无变化 + 全部门禁
- system_metrics.rs 1115 → 门面 794 + ping 80 + gpu 266
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-207（R-187）：kb/parse 拆分 — anydoc 引擎域

- 新增 `src-tauri/src/kb/parse/anydoc.rs`（26 行）：`parse_with_anydoc`
  （pub(crate)，parse_document 调度）：按扩展名指定解析器、失败时
  内容自动识别（Firecrawl anydoc → GFM Markdown）
- 中途修正：块提取时 `split_into_sections`（共享分段辅助）被连带
  迁入 anydoc.rs，且其 `SectionSpan` 引用缺导入导致编译失败；
  已将其移回 mod.rs（各格式解析器与 parse_document 共用），
  anydoc.rs 仅保留 parse_with_anydoc
- 迁移经 SHA-256 复核：parse_with_anydoc 与 split_into_sections
  均为逐字搬移；编译 + 215 测试 + fmt 全绿
- parse/mod.rs 771 → 755 行（+docx 70 / pdf 129 / xlsx 146 /
  chunk 254 / anydoc 26）
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-206（R-186）：kb/parse 拆分 — 分片策略域

- 新增 `src-tauri/src/kb/parse/chunk.rs`（254 行）：
  - `chunk_text`（pub 重导出，handlers 外部调用）：按策略分发
  - `chunk_recursive` / `chunk_by_title` / `chunk_parent_child`
  - `find_break_point` / `estimate_tokens`（pub(crate)，测试引用）
  - `Chunk` 结构体随簇迁移（chunk.rs 内定义更内聚），mod.rs
    `pub use chunk::{chunk_text, Chunk}` 保持 `kb::parse::Chunk`
    外部路径（embed.rs / handlers/* 零改动）
- 迁移经 SHA-256 复核：还原可见性后块与源完全一致
- parse/mod.rs 1014 → 771 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-205（R-185）：kb/parse 拆分 — xlsx 解析域

- 新增 `src-tauri/src/kb/parse/xlsx.rs`（146 行）：
  - `parse_xlsx`（pub(crate)，parse_document 调度）：共享字符串表
    + 首个工作表按行提取
  - `extract_xlsx_shared_strings`：<si> 内 <t> 富文本拼接
  - `extract_xlsx_sheet_text`：shared/inlineStr/普通值三形态
  - `extract_tag_content`：<tag> 文本提取（无命名空间前缀）
- 依赖：ParsedDoc / split_into_sections（super）、zip
- 迁移经 SHA-256 复核：还原可见性后块与源完全一致
- parse/mod.rs 1149 → 1014 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-204（R-184）：kb/parse 拆分 — PDF 解析域

- 新增 `src-tauri/src/kb/parse/pdf.rs`（129 行）：
  - `parse_pdf`（pub(crate)，parse_document 调度）："BT ... (text)
    Tj" 文本流提取，无文本时回退 OCR
  - `ocr_pdf_fallback`：提取内嵌 JPEG 走 Windows OCR（cfg 分支）
  - `extract_pdf_jpeg_streams`（pub(crate)，测试引用）：JPEG
    SOI/EOI 校验截取
- 依赖：ParsedDoc / split_into_sections（super）、crate::kb::ocr；
  测试模块直接 `use super::pdf::extract_pdf_jpeg_streams`
  （mod.rs 仅导入 parse_pdf，避免非测试构建闲置告警）
- 迁移经 SHA-256 复核：还原可见性后块与源完全一致
- parse/mod.rs 1266 → 1149 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-201（R-181）：bot/manager 拆分 — 联系人/日志/应答器域

- 新增 `src-tauri/src/bot/manager/contacts.rs`（148 行）：
  - `list_contacts`：会话联系人聚合（context tokens + 最近日志）
  - `list_logs`：发送日志分页
  - `save_inbound_media`：入站媒体解密落盘（cdn + sniff_ext）
  - `spawn_responder`（pub(crate)，loop 域调用）：待回复任务循环应答
- 路径修正：`super::ilink/poller/cdn/reply_tasks` 在子模块层级
  不成立，改为 `crate::bot::...` 全路径；`sniff_ext` 提升 pub(crate)
- 迁移经 SHA-256 复核：完全还原（含 3 处路径加长引发的 fmt 折行
  与 5 处有意变更）后块与源一致
- manager/mod.rs 425 → 297 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-200（R-180）：bot/manager 拆分 — 账号生命周期主循环域

- 新增 `src-tauri/src/bot/manager/loop.rs`（293 行）：
  - `start_all` / `spawn_account_loop` / `run_account_loop`：启动
    恢复 + 每账号长轮询（token 续期/24h 到期/消息轮询/入站分发）
  - `persist_tokens` / `set_status` / `set_error` / `emit_status`：
    token 落库与状态/事件上报
- 依赖：ilink（poller/HttpApiClient）、db、AccountRuntime、
  crate::bot::bridge（`super::bridge` 层级修正）；`spawn_account_loop`
  与 `emit_status` 提升 pub(crate)（qr 域调用）；模块名 `loop` 为
  关键字，以 `mod r#loop;` 声明（文件仍为 loop.rs）
- mod.rs 移除迁出的 HttpApiClient/poller 导入并清理悬空文档注释
- 迁移经 SHA-256 复核：还原三处有意变更后块与源完全一致
- manager/mod.rs 703 → 425 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-199（R-179）：bot/manager 拆分 — 账号管理域

- 新增 `src-tauri/src/bot/manager/account.rs`（83 行）：
  - `list_accounts` / `rename_account` / `unbind_account`：账号
    列表/重命名/解绑（取消轮询任务 + 删除记录 + 事件上报）
  - `status_summary`：在线/过期/异常计数汇总
  - `require_account`（pub(crate)，send/channel 域共用）：账号读取
- 依赖：db（BotAccount）、BotStatusSummary（super）、self.conn/
  accounts/emit；mod.rs 移除迁出的 BotAccount 导入
- 注：`default_account_name` 暂留门面（qr 域已依赖），后续随
  account 域进一步收敛
- 迁移经 SHA-256 复核：还原可见性后块与源完全一致
- manager/mod.rs 773 → 703 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-198（R-178）：bot/manager 拆分 — 发送域

- 新增 `src-tauri/src/bot/manager/send.rs`（317 行）：
  - `send_text` / `send_wechat_text` / `send_text_inner`：文本发送
    （微信走 ilink Sender，非微信走 channels 模块）
  - `send_media` / `send_wechat_media` / `send_media_inner`：媒体发送
  - `make_sender` / `log_outcome`：Sender 构造与发送结果落库
- 依赖：ilink（Sender/HttpApiClient）、channels 配置类型、
  db（BotAccount）、`apply_onebot_override`（提升 pub(crate)）；
  mod.rs 收敛 channels 导入（仅 OnebotConfig 保留）并移除迁出的
  Sender
- 迁移经 SHA-256 复核：块与源完全一致（fmt 后无变化）
- manager/mod.rs 1073 → 773 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-197（R-177）：bot/manager 拆分 — 非微信通道配置域

- 新增 `src-tauri/src/bot/manager/channel.rs`（126 行）：
  - `channel_config`（pub(crate)，send 域仍调用）：配置密文解密
  - `channel_config_plain`：前端回显明文
  - `add_channel_account` / `update_channel_account`：非微信通道
    账号新增/更新（企业微信/钉钉/QQ OneBot）
  - `test_channel`：连通性测试（按平台发测试消息）
- 依赖：channels 模块、db（BotAccount）、self.cipher/conn/emit
  （固有 pub(crate) 方法）；mod.rs 移除迁出的 DeserializeOwned /
  DEFAULT_CDN_BASE_URL 导入
- 迁移经 SHA-256 复核：还原可见性与原始签名后块与源完全一致
  （pub(crate) 加长签名导致 fmt 折行属预期）
- manager/mod.rs 1184 → 1073 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-194（R-174）：wechat/monitor 拆分 — 数据库查询 / 刷新域

- 新增 `src-tauri/src/wechat/monitor/query.rs`（523 行）：
  - `query_state`：解密副本 SessionTable 状态查询
  - `do_full_refresh` / `do_wal_refresh`：全量解密 + WAL patch
  - `resolve_message_dbs` / `query_messages_since_watermark` /
    `query_latest_message`：消息分库解析与水位线/最新消息查询
- impl 跨文件拆分：query.rs 内 `impl SessionMonitor` 复用父模块
  私有字段（self.db_dir/decrypted_session 等）；6 个方法提升
  `pub(crate)`（父模块 check 逻辑调用）；`SessionEntry` 相应提升
- 依赖：crypto（decrypt_wal/full_decrypt）、util（connect_db/
  stage_*/cleanup_staging/db_mtime/load_name2id）、modules::common
- mod.rs 移除迁出的 crypto 导入与 rusqlite::params（query.rs 内为
  全路径调用）；`mod query;` 声明保持 impl 块在门面继续存在
- 迁移经 SHA-256 复核：还原可见性后块与源完全一致
- monitor/mod.rs 1113 → 603 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-191（R-171）：kb/wiki 拆分 — 摘要与实体提取域

- 新增 `src-tauri/src/kb/wiki/extract.rs`（429 行），连同 PAGE_SEP/
  PAGE_END 常量一并迁出：
  - `detect_lang` / `llm_chat` / `parse_entity_json`
  - `extract_page_meta`（handlers/wiki.rs 外部调用，pub 重导出）
  - `ensure_entity_dir` / `ensure_entity_pages`（pub(crate)，
    无外部调用方）
  - `RefinedPage` / `refine_with_llm` / `parse_refined_pages`
    （pub(crate) + 字段可见性提升，供 generate 与 tests 访问）
- llm 调用保持全路径（crate::llm::handlers::chat_with_llm 等）；
  依赖 mutate::rebuild_kb_links、utils::{slugify, truncate_for_llm}
- generate.rs 改 `use super::extract::{extract_page_meta,
  refine_with_llm}`；tests 改直接导入 parse_refined_pages 与
  utils 函数；mod.rs 收敛 params/KbDatabase/rebuild_kb_links
  三个闲置导入（剩余仅 WikiPageRow + 测试）
- 迁移经 SHA-256 复核：还原可见性后块与源完全一致
- wiki/mod.rs 561 → 143 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-190（R-170）：kb/wiki 拆分 — 查询域

- 新增 `src-tauri/src/kb/wiki/query.rs`（528 行）：
  - `list_pages`：页面列表（含出入链/实体计数）
  - `link_snippet` / `plain_snippet`：上下文片段提取（pub(crate)，
    供 tests 域引用）
  - `get_page`：页面详情（正文 + 出/入链 + 失效链接 + 未链接提及）
  - `graph`：知识图谱（节点/边/幽灵节点/实体）
- 依赖：`super::WikiPageRow`（留在 mod.rs 的私有行类型）、
  `extract_wiki_links`/`OptionNone`（utils）、类型集（types）
- mod.rs 收敛 utils 重导出（extract_wiki_links/OptionNone 只剩
  query 使用）、移除 OptionalExtension（只剩 query 使用）、
  清理 types 迁出后遗留的空段头；测试模块直接
  `use super::query::{link_snippet, plain_snippet}` 等
- 迁移经 SHA-256 复核：还原可见性后块与源完全一致
- wiki/mod.rs 1078 → 561 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-189（R-169）：kb/wiki 拆分 — 自动提炼域

- 新增 `src-tauri/src/kb/wiki/generate.rs`（228 行）：
  - `list_ready_docs`：就绪文档筛选（doc_id 可选过滤）
  - `generate`：单次提炼入口（校验就绪文档后转批量流水线）
  - `generate_with_jobs`：批量流水线（processing_jobs 任务、
    parse::parse_document 解析、落库 upsert、FTS 同步、链接重建、
    提炼后摘要/实体补充），handlers/wiki.rs 外部调用路径不变
- 交叉依赖：`super::{extract_page_meta, refine_with_llm}`（extract
  域仍留 mod.rs，子模块经 super 访问私有项）、`sync_fts_upsert`、
  `rebuild_links_for_page`、`slugify`、`crate::kb::parse`
- mod.rs 收敛三个闲置导入（parse / sync_fts_upsert /
  rebuild_links_for_page——均只剩 generate 使用）；
  `pub use generate::{generate, generate_with_jobs, list_ready_docs};`
  保持外部路径；声明区布局修正（WikiPageRow 文档注释恢复归属）
- 迁移经 SHA-256 复核：块与源完全一致（fmt 后无变化）
- wiki/mod.rs 1291 → 1078 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-188（R-168）：kb/wiki 拆分 — 写入域（CRUD + 链接图）

- 新增 `src-tauri/src/kb/wiki/mutate.rs`（159 行）：
  - `create_page` / `update_page` / `delete_page`（外部
    handlers/wiki.rs 调用路径不变）
  - `rebuild_links_for_page` / `rebuild_kb_links`：出链重建与
    整库链接刷新（pub(crate)，供 generate/extract 域继续使用）
- 交叉依赖：`sync_fts_upsert`（fts.rs）、`slugify`/`extract_wiki_links`
  （utils.rs）、`KbDatabase`（crate::kb::db）
- mod.rs `pub use mutate::{create_page, delete_page, update_page};` +
  `use mutate::{rebuild_kb_links, rebuild_links_for_page};`；声明区
  补空行分隔（文档注释完整）
- 迁移经 SHA-256 复核：还原可见性后块与源完全一致
- wiki/mod.rs 1435 → 1291 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-187（R-167）：kb/wiki 拆分 — 全文检索域

- 新增 `src-tauri/src/kb/wiki/fts.rs`（149 行）：
  - `sync_fts_upsert`：页面幂等写入 FTS5（先删后插，pub(crate)
    供 create/update/generate 共用）
  - `rebuild_fts`：全量重建（search_pages 自动触发）
  - `fts_match_query`：用户查询转 FTS5 安全查询（逐词加引号 AND）
  - `search_pages`：BM25 按相关度检索（handlers/wiki.rs 外部调用）
- 依赖收敛：`super::db` → `crate::kb::db`（fts 为 wiki 子模块，
  super 层级不同）；`OptionNone`/`WikiPageItem` 从 types/utils 导入
- mod.rs `pub use fts::*;` 保持 search_pages/rebuild_fts 外部路径；
  `use fts::sync_fts_upsert;` 供本模块 mutate/generate 使用；
  声明位置修正至 import 区（文档注释完整）
- 迁移经 SHA-256 复核：还原可见性后块与源完全一致
- wiki/mod.rs 1570 → 1435 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 切片 T-186（R-166）：kb/wiki 拆分 — 工具函数

- 新增 `src-tauri/src/kb/wiki/utils.rs`（95 行）：
  - `extract_wiki_links`：Markdown [[标题]] 链接提取（带计数）
  - `slugify`：标题 → 路径 slug（中文保留）
  - `truncate_for_llm`：按句号边界截断长文本
  - `OptionNone` trait：空串转 None（get_page/graph/search_pages
    的行映射共用）
- 四项均提升 `pub(crate)`，mod.rs 经 `pub(crate) use utils::{...}`
  导入（调用点与测试经 super::* 不变）；修正一次声明位置（上移
  至 import 区，文档注释完整）
- 迁移经 SHA-256 复核：还原可见性后块与源完全一致
- wiki/mod.rs 1658 → 1570 行
- 回归：cargo fmt --check / clippy --lib --no-default-features
  0 警告 / cargo check --lib / cargo test --lib（215 passed）/
  cargo doc

## 基线复核（R-250 会话重启，全门禁复核）

- 全门禁与本会话一致：svelte-check 0 errors / 0 warnings；
  cargo fmt --check 0；cargo clippy --lib --no-default-features 0 警告；
  cargo test 215 passed / 0 failed / **20 ignored**（文档此前记 19，
  记录漂移：当前 20 个 ignored 与文档条目数差异，属环境/新增项所致，
  不作处理）；前端 48 测试（46 smoke + run-store + voice）0 失败
- 边界识别新发现：
  - `src/lib/fancyui-migration/`（12 文件约 2050 行）为 fancy-ui-svelte
    迁移遗留沙箱：全仓 0 生产引用（仅自身 README 提及）；UI_AUDIT.md
    274 行为历史审计记录（不做修改）
  - `src/lib/fancy-ui/rainbow-button.svelte`（92 行）仅文件内自定义
    元素注册名自引用，无外部引用
  - Rust 测试代码 5 处 unused-import 警告（clippy --lib 不覆盖测试
    代码，cargo test 编译期暴露）：bot/manager/tests.rs、
    kb/wiki/tests.rs、llm/client/mod.rs ×2、wechat/config/tests.rs

## 切片 T-270（R-250）：前端死代码清理（fancyui-migration + rainbow-button）

- 引用扫描确认：`fancyui-migration` 全仓仅自身 README 引用；
  `rainbow-button` 仅文件内自引用（自定义元素注册名）——均属死代码
- 删除：`src/lib/fancyui-migration/`（12 文件约 2050 行：Layout/TitleBar/
  Toaster/AnimatedBackground/pages 三页/app.fancyui.css/README）与
  `src/lib/fancy-ui/rainbow-button.svelte`（92 行）
- 恢复保障：删除前备份至 `%TEMP%\st-deadcode-backup`（13 文件，
  门禁全过后仍保留，待确认无需求可手动清理）
- 回归：svelte-check 0 errors / 0 warnings；`npm run build` 通过；
  48 前端测试 0 失败（前端死代码删除不影响任何 smoke 依赖）

## 切片 T-271（R-251）：Rust 测试代码冗余导入清理（5 处警告清零）

- cargo test 编译期暴露 5 处 unused-import（clippy --lib 不覆盖测试
  代码，属门禁盲区；逐一删除冗余导入）：
  - `bot/manager/tests.rs`：删除 `use super::*;`（函数经显式导入）
  - `kb/wiki/tests.rs`：删除 `use super::*;`（显式导入已覆盖）
  - `llm/client/mod.rs` resolve_tests：`use super::urls::{
    is_embedding_marked, resolve_embedding_model}` 删除
    is_embedding_marked（该函数仅 embeddings 域使用）
  - `llm/client/mod.rs` probe_tests：删除 `use super::*;`
  - `wechat/config/tests.rs`：删除 `use super::*;`
- 回归：cargo fmt --check 0；cargo test 215 passed / 0 failed /
  20 ignored 且 0 warnings；cargo clippy --lib --no-default-features
  0 警告

## 蓝图 T-蓝图-29：wechat/handlers/session.rs 按职责拆分

### 现状与边界

- `wechat/handlers/session.rs` 1578 行（当前最大单体），按段头分四个
  职责域：会话/消息查询（L1-328）、导出（L142-230/368-421 + 导出辅助
  L472-882）、消息编辑（L883-1100）、原始字段编辑（L1101-1326）、
  全局搜索（L1327-1578）
- 依赖分析：导出域仅依赖 `modules::messages::get_conversation_messages`
  （完全限定）+ helpers；编辑/原始字段域自包含（find_message_db /
  read_message_content / json_to_sql_value / read_full_row 域内互用）；
  搜索域自包含；跨域引用为零 → 边界清晰

### 目标布局

```
wechat/handlers/session/
  session.rs — 门面：会话/消息查询命令 + mod 声明 + glob re-export
  export.rs  — 导出域：export_session_messages / batch_export_sessions +
               collect_messages_for_export / collect_export_images /
               html_escape / format_messages
  edit.rs    — 编辑域：get_chat_edit_status / list_session_edited_messages /
               edit_chat_message / reset_edited_message /
               get_message_raw_row / update_message_raw_fields +
               find_message_db / read_message_content / json_to_sql_value /
               read_full_row
  search.rs  — 搜索域：search_wechat_messages / build_wechat_search_index /
               get_wechat_search_index_status / get_chat_daily_counts +
               scan_search_messages
```

拆分原则：文件模块 + 子目录混合模式（session.rs 保留为门面文件，
`mod x; pub use x::*;` 汇总，lib.rs 的 `wechat::handlers::<cmd>`
注册路径经 glob re-export 链零改动）。

## 切片 T-272（R-252）：session 拆分 — 导出域

- 新建 `wechat/handlers/session/export.rs`（554 行）：导出命令
  （export_session_messages L142-230 / batch_export_sessions L368-421）
  与导出辅助段（L472-882：collect_messages_for_export /
  collect_export_images / html_escape / format_messages）逐字搬移
  （显式 LF 批量机械搬移，TEMP 备份 + 边界校验 + 三段字节级
  Contains 复核全部通过）
- 依赖处理：export.rs 补 `use crate::wechat::handlers::helpers;` 与
  `use crate::wechat::modules::messages::ChatMessage;`（ChatMessage
  全仓仅导出域使用，门面随之移除该导入）
- session.rs 1578 → 门面 1024 + export 554；`lib.rs` 注册点零改动
  （`pub use export::*;` glob 传递 `__cmd__*` 隐藏项）
- 回归：cargo fmt（两处段尾空行归一化）/clippy 0 警告 /
  cargo check --lib 0 警告 / cargo test 215 passed / cargo doc 0

## 切片 T-273（R-253）：session 拆分 — 编辑域 + 搜索域

- 新建 `wechat/handlers/session/edit.rs`（426 行）：消息编辑域
  （L329-546：get_chat_edit_status / list_session_edited_messages /
  edit_chat_message / reset_edited_message + find_message_db /
  read_message_content）与原始字段编辑域（L547-771：get_message_raw_row /
  update_message_raw_fields + json_to_sql_value / read_full_row）
  逐字搬移，两段同属编辑族合并为单模块（json_to_sql_value 两段共用）
- 新建 `wechat/handlers/session/search.rs`（250 行）：全局搜索域
  （L773-1024：scan_search_messages / search_wechat_messages /
  build_wechat_search_index / get_wechat_search_index_status /
  get_chat_daily_counts）逐字搬移
- 依赖处理：两个新模块仅补 `use crate::wechat::handlers::helpers;`
  （其余 crate 路径均已完全限定）
- session.rs 1024 → 门面 311；`lib.rs` 注册点零改动；
  T-蓝图-29 全部完成（session 1578 → 门面 311 + export 554 /
  edit 426 / search 250）
- 回归：cargo fmt --check 0 / clippy 0 警告 / cargo test 215 passed /
  0 failed / 20 ignored / cargo doc 0；smoke-ipc-contract 306 命令
  全一致（命令名与参数契约不受模块结构影响）

## 蓝图 T-蓝图-30：wechat/handlers/data.rs 按职责拆分

### 现状与边界

- `wechat/handlers/data.rs` 1340 行，段头清晰分六个职责域：
  通讯录（L34-121）/ 朋友圈（L123-315）/ 收藏（L317-374）/
  通用设置·表情·公众号·文件·头像·图片（L376-632）/
  数据库状态·媒体·打开路径（L634-1013）/ 微信状态·历史消息（L1015-1340）
- 依赖分析：全文件仅 4 个顶层导入（base64::Engine / tauri::{Manager,
  State} / helpers / monitor::WeChatMonitorState），按域归属：
  base64→general；Manager→general+media；State→general（status 用
  完全限定）；WeChatMonitorState→general+media+status；find_db_file
  仅 media 域使用；无跨模块外部调用（仅 lib.rs 注册）

### 目标布局

```
wechat/handlers/data/
  data.rs     — 门面：find_db_file + mod 声明 + glob re-export
  contacts.rs — 通讯录（get_contacts / get_contacts_by_category /
                get_contact_profile / export_contacts_csv）
  moments.rs  — 朋友圈（get_moments_page / refresh_wechat_moments /
                export_moments_csv / get_moment_image / get_moment_video）
  favorites.rs— 收藏（get_favorites / get_favorite_detail /
                export_favorites_csv）
  general.rs  — 通用设置/表情/公众号/文件/头像/图片（get_general_settings /
                export_general_category_csv / get_emoticons /
                get_static_emoticons / get_bizchats / get_official_accounts /
                get_resource_files / get_user_avatar / get_message_image /
                image_data_url / get_ilink_origin_status）
  media.rs    — 数据库状态/语音/文件/媒体（规划）
  paths.rs    — 打开路径 ×4（规划）
  status.rs   — 微信状态/历史/缺失图/账号（规划）
```

拆分原则：文件模块 + 子目录混合模式；各域补最小导入（helpers 必选，
其余按使用面），crate 路径已完全限定；`lib.rs` 注册点零改动。

## 切片 T-274（R-254）：data 拆分 — 通讯录 + 收藏域

- 新建 `wechat/handlers/data/contacts.rs`（89 行）：通讯录段
  （L34-121）逐字搬移；新建 `favorites.rs`（60 行）：收藏段
  （L317-374）逐字搬移（显式 LF 批量机械搬移，TEMP 备份 +
  边界校验 + 字节级 Contains 复核通过）
- 依赖处理：两模块仅补 `use crate::wechat::handlers::helpers;`
  （其余 crate 路径完全限定）
- data.rs 1340 → 门面 751；`lib.rs` 注册点零改动
- 回归：cargo fmt / clippy 0 / check 0 / test 215 passed / doc 0

## 切片 T-275（R-255）：data 拆分 — 朋友圈域

- 新建 `wechat/handlers/data/moments.rs`（193 行）：朋友圈段
  （L123-315：get_moments_page / refresh_wechat_moments /
  export_moments_csv / get_moment_image / get_moment_video）逐字搬移
- 依赖处理：仅补 helpers 导入（sns_image / image / config 均完全限定）
- data.rs 751 → 门面 559；`lib.rs` 注册点零改动
- 回归：cargo fmt / clippy 0 / check 0 / test 215 passed / doc 0

## 切片 T-276（R-256）：data 拆分 — 通用设置/表情/公众号/文件/头像/图片域

- 新建 `wechat/handlers/data/general.rs`（252 行）：通用段（L376-632，
  11 个命令 + image_data_url）逐字搬移
- 依赖处理：general.rs 补 4 个导入（base64::Engine / tauri::{Manager,
  State} / helpers / monitor::WeChatMonitorState——try_state 为 Manager
  trait 方法，State 用于 get_message_image）；门面移除随之未用的
  base64 与 State 导入（tauri 改 `use tauri::Manager;`），清理两处
  段尾双空行
- data.rs 559 → 门面 380（find_db_file + media/status 段待下一轮）；
  `lib.rs` 注册点零改动
- 回归：cargo fmt / clippy 0 / check 0 / test 215 passed / 0 failed /
  20 ignored / doc 0；smoke-ipc-contract 306 命令全一致
- 后续：media 段（get_wechat_db_status / 语音/文件/媒体 +
  open_wechat_* ×4，L41-421）与 status 段（微信状态/历史/缺失图/
  账号，L422-748）按蓝图继续拆分

## 切片 T-277（R-257）：data 拆分 — 媒体 / 数据库状态域

- 新建 `wechat/handlers/data/media.rs`（318 行）：find_db_file
  （L18-39，唯一调用方 get_wechat_db_status 同域）与数据库状态段
  （L41-326：get_wechat_db_status / get_message_voice /
  get_favorite_voice / resolve_wechat_file / transcribe_message_voice /
  get_favorite_image）逐字搬移
- 依赖处理：media.rs 补 3 个导入（helpers / tauri::Manager（try_state
  为 Manager trait 方法）/ monitor::WeChatMonitorState）；stt/llm/
  voice/file/avatar 均完全限定
- 回归：cargo fmt（导入排序归一）/ clippy 0 / check 0 / test 215
  passed / doc 0

## 切片 T-278（R-258）：data 拆分 — 打开路径域

- 新建 `wechat/handlers/data/paths.rs`（99 行）：open_wechat_folder /
  open_wechat_path / open_wechat_protocol / open_wechat_attach_folder
  （L328-420）逐字搬移
- 依赖处理：零顶层导入（std::process/PathBuf 完全限定，
  CommandExt 为函数内局部 use 随迁）
- 回归：cargo fmt / clippy 0 / check 0 / test 215 passed / doc 0

## 切片 T-279（R-259）：data 拆分 — 微信状态 / 历史消息域

- 新建 `wechat/handlers/data/status.rs`（334 行）：微信状态段
  （L422-747：get_wechat_status / get_wechat_history /
  get_wechat_missing_images / export_wechat_missing_images_csv /
  get_wechat_account_status / detect_live_account /
  switch_wechat_account_to_live）逐字搬移
- 依赖处理：status.rs 补 2 个导入（helpers / WeChatMonitorState；
  tauri::State 完全限定，patch_config/SystemTime 为函数内局部 use
  随迁）
- data.rs 380 → 纯门面 19 行（7 个子模块 + glob re-export）；
  `lib.rs` 注册点零改动；T-蓝图-30 全部完成
  （data 1340 → 门面 19 + contacts 89 / moments 193 / favorites 60 /
  general 252 / media 318 / paths 99 / status 334）
- 回归：cargo fmt / clippy 0 / check 0 / test 215 passed / 0 failed /
  20 ignored / doc 0；smoke-ipc-contract 306 命令全一致

## 蓝图 T-蓝图-31：wechat/handlers/config.rs 按职责拆分（规划）

- `config.rs` 798 行，段头分四个职责域：配置读写（L27-126，
  get_wechat_config / save_wechat_config / get_api_settings /
  apply_api_settings / detect_wechat_accounts / scan_wechat_accounts /
  get_wechat_keys_info）、密钥校验（L127-497，verify_database_key /
  generate_keys_file(_impl) / verify_one_db / decrypt_all_databases /
  decrypt_one_db）、图片解密（L498-737，verify_image_key /
  decode_all_images）、全自动密钥获取（L738-798，auto_get_db_key(_v2) /
  auto_get_image_key / auto_get_wechat_keys）
- 目标布局：config.rs 门面（emit_op_progress + re-export）+ 子模块
  io（配置读写）/ keys（密钥校验）/ image（图片解密）/ auto
  （全自动密钥获取）；`lib.rs` 注册点零改动

## 切片 T-280（R-260）：config 拆分 — 配置读写域

- 新建 `wechat/handlers/config/io.rs`（96 行）：配置读写段（L27-125：
  get_wechat_config / save_wechat_config / get_api_settings /
  apply_api_settings / detect_wechat_accounts / scan_wechat_accounts /
  get_wechat_keys_info）逐字搬移（显式 LF 批量机械搬移，TEMP 备份 +
  边界校验 + 字节级 Contains 复核通过）
- 依赖处理：零顶层导入（config 全部完全限定）
- 回归：cargo fmt / clippy 0 / check 0 / test 215 passed / doc 0

## 切片 T-281（R-261）：config 拆分 — 密钥校验域

- 新建 `wechat/handlers/config/keys.rs`（359 行）：密钥校验段
  （L127-496：verify_database_key / generate_keys_file(_impl) /
  verify_one_db / decrypt_all_databases / decrypt_one_db）逐字搬移
- 依赖处理：补 `use crate::wechat::handlers::helpers;`（全文件唯一
  helpers 使用点 helpers::scan_db_files 在本域）；emit_op_progress
  留在门面，4 处调用点改 `super::emit_op_progress`
- 回归：cargo fmt / clippy 0 / check 0 / test 215 passed / doc 0

## 切片 T-282（R-262）：config 拆分 — 图片解密域

- 新建 `wechat/handlers/config/image.rs`（241 行）：图片解密段
  （L498-736：verify_image_key / decode_all_images）逐字搬移
- 依赖处理：零顶层导入；emit_op_progress 6 处调用点改
  `super::emit_op_progress`
- 回归：cargo fmt / clippy 0 / check 0 / test 215 passed / doc 0

## 切片 T-283（R-263）：config 拆分 — 全自动密钥获取域

- 新建 `wechat/handlers/config/auto.rs`（61 行）：全自动密钥段
  （L738-798：auto_get_db_key / auto_get_db_key_v2 /
  auto_get_image_key / auto_get_wechat_keys）逐字搬移
- 依赖处理：零顶层导入（tauri::async_runtime / auto_key 完全限定）
- config.rs 798 → 门面 32 行（emit_op_progress + 4 个子模块 glob
  re-export）；`lib.rs` 注册点零改动；T-蓝图-31 全部完成
  （config 798 → 门面 32 + io 96 / keys 359 / image 241 / auto 61）
- 教训：父模块私有函数供子模块调用须 `super::` 前缀（Rust 后代
  可见性允许，裸名不行——10 处调用点 E0425 后统一替换）
- 回归：cargo fmt / clippy 0 / check 0 / test 215 passed / 0 failed /
  20 ignored / doc 0；smoke-ipc-contract 306 命令全一致

## 切片 T-284（R-264）：DbManager 时间戳单元格格式化下沉 dbUtils

- 边界识别：DbManager 的 `fmtTsValue`（含 TS_COLS 白名单）为纯函数
  （输入 unknown + 列名 → 字符串/null），模板 3 处调用（单元格 title/
  详情时钟图标）；`fmtNum` 为唯一实现的一行薄封装（en-US 千分位），
  按既有约定不收敛
- `db/dbUtils.ts` 新增 `TS_COLS` 常量与 `fmtTsValue`（函数体逐字
  迁移：null/空 → null、非白名单列 → null、数字解析、>1e12 毫秒
  换算、1e8..4e9 秒有效窗口、YYYY-MM-DD HH:mm:ss 含秒输出）
- DbManager 删除本地 TS_COLS + fmtTsValue（约 17 行），import 补
  fmtTsValue；组件 2503 行
- smoke-db-utils.mjs 扩展 16 项断言（总 18 → 34）：秒/毫秒换算、
  字符串数字、空白环绕、列名大小写敏感、非时间列、null/undefined/
  空串/非数字/0/负数、有效窗口上下界、白名单内容
- 回归：svelte-check 0 errors / 0 warnings；`npm run build` 通过；
  48 前端测试 0 失败（smoke-db-utils 34 断言全过）

## 蓝图 T-蓝图-32：ipc_handlers.rs 按职责拆分

- `ipc_handlers.rs` 624 行，六个段：服务端/系统 IPC（L14-122）、
  内部数据库 IPC + 配置（L124-209）、表浏览/CRUD（L211-291）、
  外部数据库浏览/CRUD（L293-458，含 allowed_db_roots L297-315）、
  数据库增强能力（L460-624）
- 依赖分析：allowed_db_roots 被 external（6 处）+ maintain（1 处）
  共用 → 留在门面（emit_op_progress 同款模式）；open_conn 仅
  maintain 域；serde/Arc 仅 system 域（结构体 derive + State 参数）
- 目标布局：`ipc_handlers/` 子目录 + 门面文件，`lib.rs` 的
  `ipc_handlers::<cmd>` 注册路径经 glob re-export 零改动

## 切片 T-285（R-265）：ipc_handlers 拆分 — 五域一次成型

- 新建 5 个子模块（显式 LF 批量机械搬移，TEMP 备份 + 边界校验 +
  反向替换字节复核全 True）：
  - `system.rs`（117 行）：get_server_status / get_app_info /
    get_system_info / send_command_to_agent + ServerStatusResponse /
    SystemInfo / SendCommandArgs 结构；补 serde + Arc 导入
  - `internal.rs`（92 行）：get_db_info / list_app_databases /
    query_events / query_agent_log / insert_event / get_db_config /
    set_db_config；零顶层导入
  - `tables.rs`（87 行）：list_tables / table_schema / query_table /
    insert_row / update_row / delete_row / cleanup_old_data
  - `external.rs`（152 行）：scan_external_dbs / check_db_header /
    external_list_tables / external_table_schema / external_query_table /
    get_cell_value / write_file；6 处调用点改 `super::allowed_db_roots`
  - `maintain.rs`（171 行）：open_conn / get_table_detail /
    db_integrity / run_sql / table_stats / export_table_csv /
    backup_internal_db / restore_internal_db；1 处调用点改
    `super::allowed_db_roots`
- ipc_handlers.rs 624 → 门面 34 行（allowed_db_roots + 5 子模块
  glob re-export）；`lib.rs` 注册点零改动
- 回归：cargo fmt / clippy 0 / check 0 / test 215 passed / 0 failed /
  20 ignored / doc 0；smoke-ipc-contract 306 命令全一致

## 切片 T-286（R-266）：RelationshipGraph 展示纯函数三连下沉

- 边界识别：组件内 3 个纯展示函数（依赖图数据/设置，无组件副作用）：
  `toGraphData`（GraphRawData → RelationGraphData 归一化）、
  `connectedEdges`（相连边按权重降序 + 对端解析 + 前 12）、
  `sharedGroupNames`（群名映射 + code 回退 + limit）
- `graphModel.ts` 新增 `toGraphData`（函数体逐字迁移，归属图数据
  模型模块）；`graphStats.ts` 新增 `connectedEdgesOf(graph, nodeId,
  limit)` 与 `sharedGroupNames(n, groupNames, limit)`（groupNames
  参数化，消除对组件 graphData 状态的闭包依赖）
- RelationshipGraph.svelte：删除 3 个本地函数（约 30 行），模板
  调用点改 `connectedEdgesOf(graph, sel.id)` /
  `sharedGroupNames(sel, graphData?.groupNames, 6)`；`applyData`
  经共享 toGraphData；清理随之未用的 GNode 类型导入
- smoke-graph-stats.mjs 扩展 24 项断言（总 10 → 34）：相连边过滤/
  降序/对端解析/limit/空图、群名映射/回退/limit、toGraphData 归一化/
  self 排除/空数据容错（graphModel 经 esbuild bundle 解析 ../utils
  运行时依赖）
- 回归：svelte-check 0 errors / 0 warnings；`npm run build` 通过；
  48 前端测试 0 失败（smoke-graph-stats 34 断言全过）

## 切片 T-288（R-267）：Rust 跨模块重复收敛（4 组）

- 全仓同名函数扫描（按缩进深度提取函数体逐字比对）甄别真重复：
  - **`emit_op_progress`**（archive.rs / handlers/config.rs 门面，18 行
    逐字相同）→ 收敛至 `handlers/helpers.rs`（pub(crate)）；
    config.rs 门面改为 `pub(crate) use ...::emit_op_progress;`
    re-export（keys/image 子模块 `super::` 调用零改动）；archive.rs
    加 helpers 导入，3 处调用点改 `helpers::emit_op_progress`
  - **`dir_sig` + `is_month_dir_name`**（file.rs / voice/video.rs 各
    逐字相同）→ 收敛至 `wechat/modules/common.rs`（与既有 DirSig
    类型/file_sig 同域）；两文件删除本地实现，import 补充
    （file.rs 的 is_month_dir_name 测试断言原样通过）
  - **`truncate`**（common.rs 与 ask/search.rs 行为等价：按 char
    截断 + 省略号，仅写法差异）→ ask/search.rs 删除本地定义，
    `pub(crate) use crate::common::truncate;` re-export（plan.rs /
    llm.rs 的 `super::truncate` 与 12 处调用零改动；llm.rs 的
    `out.truncate(limit)` 为 Vec 方法，不受影响）
- 评估不收敛项（语义分叉，记录在案）：json_to_sql_value（convert
  版无 Result/u64 超界→Text/NaN→0.0 vs edit 版 Result/u64 截断/
  NaN→Null）、clean_cdata（file 版无条件剥离 vs xml 版成对剥离）、
  file_sig vs dir_sig（len vs 子项数）、scan_dir_recursive/walk_files
  （参数与过滤语义不同）、local_datetime/load_display_names（实现
  差异）、describe_reqwest_error（textin 版带深度上限的有意变体）
- 回归：cargo fmt / clippy 0 / check 0 / test 215 passed / 0 failed /
  20 ignored（含 file.rs 收敛后测试）/ doc 0

## 审计 T-289（R-268）：跨模块重复深扫 + 全门禁确认（无代码变更）

- 同名函数全仓扫描（提取函数体逐字比对），深扫 7 组候选后结论：
  - **可收敛**：仅 T-288 已处理的 4 组；组件内重复扫描（WeChatPanel
    126 函数 / WikiPanel 40 / GlobalChatTab 30 / DailySummary 23）
    无逐字重复；AGENTS.md 测试清单 48/48 与实际目录同步
  - **语义分叉不收敛**（记录）：emit_progress（AppHandle vs
    Option<AppHandle>、u64 vs usize）、data_url（image/ 前缀约定 vs
    完整 mime 约定）、local_datetime（NaiveDateTime vs DateTime<Local>
    及 chrono API 代差）、load_display_names（annual 无缓存遍历 vs
    contacts 带缓存）、save_config ×3（锁+备份 / 静默 / 简单写，
    安全级别不同）、statusLabel vs STATUS_LABEL（Wiki 页面状态 vs
    文档处理状态，值域不同）、App.fmtTimeShort（唯一实现，5 行，
    下沉收益低）
- 全门禁确认：cargo fmt / clippy 0 / check 0 / test 215 passed /
  0 failed / 20 ignored；svelte-check 0 errors / 0 warnings；
  48 前端测试 0 失败；npm run build 通过；AGENTS.md 48/48 同步
- 结论：10 轮重构后前后端均无已知重复实现残留，进入维护期

## 切片 T-290（R-269）：WikiPanel graphVisible 派生下沉 visibleNodeIds

- 边界识别：WikiPanel 的 `graphVisible`（22 行 `$derived.by`）为纯
  过滤计算（createdOnly / showOrphans / ignorePatterns 通配 / 关键词 /
  localOnly 邻居保留），且 localOnly 分支的手写邻居遍历与既有
  `graphNeighborSet`（T-87）逐字等价
- `kb/graphUtils.ts` 新增 `visibleNodeIds(graph, opts)`：参数化 6 项
  过滤条件；localOnly 分支复用 graphNeighborSet（消除手写遍历）；
  通配匹配经 `matchGlob`（graphLayout，新增运行时依赖，smoke 改
  esbuild bundle 模式）
- WikiPanel：派生块收敛为一行 `$derived(visibleNodeIds(...))`；
  移除随之未用的 matchGlob 导入
- smoke-kb-graph-utils.mjs 扩展 9 项断言（总 7 → 16）：全量/null 图/
  createdOnly/showOrphans/通配（含大小写不敏感实测修正）/关键词/
  localOnly 锚点邻居
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过；Rust 门禁（重编译后基线）fmt/clippy 0 /
  test 215 passed / 0 failed / 20 ignored
- 备注：磁盘整理（T-盘）删除全部 cargo 编译产物后，全量重编译
  实测 1.9 分钟（debug 缓存 11 GB），门禁基线已恢复

## 切片 T-291（R-270）：WeChatPanel showMgmtMsg 收敛共享 createMsg

- 边界识别：WeChatPanel 的 `showMgmtMsg`（mgmtMsg/mgmtMsgOk 双状态
  + mgmtTimer 5000ms + clearTimeout 防竞态）与共享 `createMsg(5000)`
  （T-123/T-127 已收敛 5 个组件）完全同构——T-123 时代遗漏的最大
  调用方
- 收敛：删除本地 4 个状态/函数（约 12 行），`const mgmt = createMsg(5000);`
  （保留原 5000ms 语义）；49 处 `showMgmtMsg(` → `mgmt.show(`、
  4 组直接赋值合并为 `mgmt.show(text, ok)`（sendViewerToOcr 三分支）、
  模板 3 处改 `mgmt.state.text/ok`、onDestroy 的 mgmtTimer 清理行
  删除（createMsg 定时器为闭包写入，与其他 5 组件处理一致）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过；残留检查仅剩收敛注释
- 全仓 createMsg/showMsg 族确认无其他本地重复实现

## 切片 T-292（R-271）：前端 any 漏网清零（2 处精准类型修复）

- 复查扫描（含 `Record<*, any>`/`Map<*, any>` 模式）发现 2 处
  T-145/T-153 时代的漏网：
  - `ProviderConfigTab.svelte`：`PROVIDER_ICONS: Record<ProviderType,
    any>` → `Record<ProviderType, Component>`（lucide 图标组件，
    `import type { Component } from 'svelte'`）
  - `WeChatPanel.svelte`：`rebuildSessionMap` 的 `new Map<string,
    any>()` → `new Map<string, WeChatSession>()`（T-2 曾类型化
    sessionMap，此处临时 Map 遗漏）
- 全仓最终 any 扫描：`Record<*, any>`/`Map<*, any>`/`as any`/`: any`/
  `<any>`/catch-any 全部 0 处
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过

## 切片 T-293（R-272）：RelationshipGraph 海报配置下沉 + showExportMsg 收敛

- 边界识别：`doExport` 内的 `mkInput`（25 行）为纯海报文案配置构造
  （PosterInput 的 tag/title/subtitle/stats/legend/footer，依赖
  isPeople 与各计数）；`showExportMsg`（exportMsg/exportMsgOk +
  exportTimer 4500ms）与共享 createMsg 完全同构（T-291 同类遗漏）
- `graphPoster.ts` 新增 `makePosterInput(opts)`：参数化 10 项输入，
  返回 Omit<PosterInput, 'graphLayer'|'scale'>；stats 经 fmtCount
  格式化（totalGroups 缺省回退 groupCount 语义保持）
- RelationshipGraph：`mkInput` 收敛为 `{ graphLayer, scale,
  ...makePosterInput({...}) }`（约 25 行 → 14 行）；`showExportMsg`
  删除本地实现改 `createMsg(4500)`，3 处调用 + 模板改
  `exportMsgState.state.*`
- smoke-graph-stats.mjs 扩展 10 项断言（34 → 44）：标题/标签/比例/
  副标题/统计/图例/页脚/totalGroups 回退/群聊模式（graphPoster
  bundle 后仅调用纯函数路径）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过
- 全仓 createMsg 族复查：7 个组件（BackupManager/DailySummary/
  GroupMonitor/HookManager/AiRolesPanel/WeChatPanel/RelationshipGraph）
  全部收敛，无本地重复

## 切片 T-294（R-273）：WeChatPanel collectSessionImages 下沉 panel.ts

- 边界识别：`collectSessionImages`（25 行）为纯函数——从 messages
  收集 type=3 图片消息（image_url 直链优先，imageCache
  `username:local_id` 缓存回退），闭包依赖 messages/imageCache/
  curSession 可参数化；唯一调用点 openImageViewer
- `wechat/utils/panel.ts` 新增 `collectSessionImages(messages,
  imageCache, sessionKey)` + `SessionImageItem` 类型（函数体逐字
  迁移，null 会话守卫保留）
- WeChatPanel：删除本地实现（约 25 行），openImageViewer 改传
  参数；panel import 补充
- smoke-panel-utils.mjs 扩展 6 项断言（31 → 37）：直链优先/缓存
  回退/文本消息排除/群聊标记/无会话/无 src 不收集
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过

## 切片 T-295（R-274）：图片查看器缩放步进纯函数化

- 边界识别：`cycleZoom`（cycle 模式循环推进）与 `onViewerWheel`
  （clamp 模式边界移动）共享同一档位步进计算（indexOf 未命中
  从 0 起算），`VIEWER_ZOOM_STEPS` 为组件内常量
- `wechat/utils/panel.ts` 新增 `VIEWER_ZOOM_STEPS` 常量与
  `zoomStepIndex(steps, current, dir, mode)`（cycle/clamp 双模式）；
  组件两个函数收敛为一行调用（offset 重置副作用保留在组件）
- smoke-panel-utils.mjs 扩展 9 项断言（37 → 46）：档位常量/
  cycle 推进与回绕/未命中从 0/clamp 推进与上下限封顶/后退
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过

## 切片 T-296（R-275）：静态表情映射/过滤派生下沉

- 边界识别：`staticEmoticonMap`（9 行 Map 构建）与
  `filteredStaticEmoticons`（15 行分类+关键词过滤）为纯派生，
  依赖 staticEmoticons/staticEmoCat/staticEmoSearch 可参数化
- `wechat/utils/panel.ts` 新增 `buildStaticEmoticonMap(categories)`
  与 `filterStaticEmoticons(categories, cat, search)`（函数体逐字
  迁移，含非 png 原名映射语义）；组件两个 $derived.by 收敛为一行
  调用，清理随之未用的 StaticEmoticonFile 类型导入
- smoke-panel-utils.mjs 扩展 9 项断言（46 → 55）：去后缀映射/
  大写后缀/非 png 原名/首现优先/分类过滤/文件名与标签关键词/空
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过

## 切片 T-297（R-276）：DbManager 表分组/外部库分组派生下沉

- 边界识别：`dbTableSections`（10 行，收藏/全部分组 + 搜索过滤）
  与 `groupedDbFiles`（22 行，外部库按扫描根最长前缀分组、
  未命中按所在目录）均为纯派生
- `db/dbUtils.ts` 新增 `groupDbTables(tables, pinned, search)` 与
  `groupDbFilesByRoot(files, roots)`（函数体逐字迁移；DbFileEntry
  类型 import 补充）；组件两个 $derived.by 收敛为一行调用
  （dbPins 声明在后，dbTableSections 保留 $derived.by 闭包避免
  TDZ——T-80 同款教训）
- smoke-db-utils.mjs 扩展 11 项断言（34 → 45）：收藏优先/无收藏/
  搜索过滤（小写标签）/收藏排除/最长前缀命中/未命中目录分组/
  目录名末段/空列表
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过

## 切片 T-298（R-277）：ChartView 饼图角度纯函数化

- 边界识别：`pieSlices`（11 行）为纯几何计算——饼图切片累积角度
  （start/end 0-360°，total 兜底 1 防除零），归属 chartGeometry
  （D-28 已下沉 polar/arcPath）
- `chartGeometry.ts` 新增泛型 `pieSliceAngles<T extends {value:
  number}>(items, color)`（函数体逐字迁移）；ChartView 收敛为
  `$derived(n.kind === "pie" ? pieSliceAngles(n.pie, color) : [])`
  （组件变量名 pieSlices 保留，函数名避开冲突）
- smoke-chart-geometry.mjs 扩展 4 项断言（7 → 11）：累积角度/全零
  兜底/空列表/半圆边界
- 评估不收敛（记录）：BotPanel.fileMeta.kind 与 extTone 均为文件
  分类但分类集不同（webm/flv/amr/ogg vs flac/aac/m4v、4 类 vs 9 类），
  强行合并会改变可观测输出
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过

## 切片 T-299（R-278）：资源文件/设置分类派生下沉

- 边界识别：`shownFiles`（11 行：三列表合并 + 分类过滤 + 关键词 +
  modify_time 降序）与 `settingsFilteredCats`（14 行：行内 cellText
  命中 + label/table 命中 + count 更新；T-98 评估时保留的组件内
  独立语义）均为纯派生
- `wechat/utils/panel.ts` 新增 `filterSortResourceFiles(data, cat,
  search)` 与 `filterSettingsCats(data, search)`（函数体逐字迁移；
  panel 新增 ./format 的 cellText 与 filter 的 filterByAnyKeyword
  内部依赖）；组件两个 $derived.by 收敛为一行调用
- smoke-panel-utils.mjs 扩展 10 项断言（55 → 65）：合并+时间降序/
  分类过滤/md5 与 file_name 关键词/空关键词原引用/行内命中 count
  更新/label 与 table 命中/无命中
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过

## 切片 T-300（R-279）：KbTrendChart fmtDate 收敛 + AnnualSummary 峰值展示下沉

- 边界识别：`KbTrendChart.fmtDate`（Date → YYYY-MM-DD）与共享
  `formatDate(d, { dateOnly: true })`（T-104 新增 dateOnly）逐字等价
  ——真重复；`AnnualSummary.peakInfo`（9 行：heatPeak 结果 → 星期
  标签/小时补零/值）为纯展示派生
- KbTrendChart：删除本地 fmtDate，4 处调用点改
  `formatDate(d, { dateOnly: true })`（fmtShort 为 MM/DD 轴标签唯一
  实现，保留）
- `wechat/utils/annual.ts` 新增 `peakInfoOf(heatmap, matrix)`（heatPeak
  同域）；AnnualSummary 派生收敛为一行调用
- smoke-annual-summary.mjs 扩展 4 项断言（30 → 34）：标签映射/缺失
  回退/空矩阵默认/heatmap 缺省容错
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过；长派生块复扫确认 WeChatPanel 清零

## 切片 T-301（R-280）：BotPanel fileMeta 纯函数化

- 边界识别：`fileMeta`（10 行：路径末段 + 4 类正则分类）为纯函数；
  分类集与 extTone 不同（T-298 已记录），独立下沉保持语义
- 新建 `bot/fileMeta.ts`：`fileMetaOf(path)` + `FileKind`/`FileMeta`
  类型（函数体逐字迁移）；BotPanel 派生收敛为
  `$derived(selectedFile ? fileMetaOf(selectedFile) : null)`
- smoke-bot-steps.mjs 扩展 7 项断言（22 → 29）：图片/视频/音频/
  未知回退/HEIF/无扩展名/分隔符容错（含反斜杠路径）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过

## 切片 T-302（R-281）：GargantuaBackdrop iframe 参数构建下沉

- 边界识别：`frameSrc`（9 行 URLSearchParams 参数组装）为纯函数；
  语义注意：组件 props 的 `motion = true` 默认值 → 函数内以
  `opts.motion === false` 判断 nocine（非 `!opts.motion`，避免
  undefined 误触发）
- 新建 `wechat/utils/backdrop.ts`：`gargantuaFrameUrl(opts)`（函数体
  逐字迁移：steps/cam truthy、bright/star/sky 非 null）；组件派生
  收敛为一行调用
- smoke-wechat-misc.mjs 扩展 5 项断言（27 → 32）：默认参数/全参数
  透传/steps=0 不设/bright=null 不设/motion=true 无 nocine
- 至此全仓 `$derived.by` 长块（>8 行）清零（GargantuaBackdrop 为
  最后一个非 shader 块）
- 回归：svelte-check 0 errors / 0 warnings；48 前端测试 0 失败；
  npm run build 通过

## 审计 T-303（R-282）：最终全门禁确认（无代码变更）

- 全门禁最终确认（第 24 轮，R-282）：
  - Rust：cargo fmt --check 0 / clippy --lib --no-default-features
    0 警告 / cargo test 215 passed / 0 failed / 20 ignored /
    cargo doc 0 警告
  - 前端：svelte-check 0 errors / 0 warnings；48 前端测试
    （46 smoke + run-store + voice）0 失败；npm run build 通过
  - 文档：AGENTS.md 测试清单 48/48 与实际同步；refactor-plan.md
    蓝图 9 份 + 切片 36 项（T-270~T-302）
  - 类型：全仓 any 注解 0 处（含 catch-any / Record<*, any> /
    Map<*, any> 边缘模式）
- 重构总结（R-250 会话起 24 轮）：
  - Rust：4 个顶层单体域化（wechat/handlers/{session,data,config}
    与 ipc_handlers → 门面 + 域模块，共 18 个子模块）、4 组跨模块
    重复收敛（emit_op_progress / dir_sig / is_month_dir_name /
    truncate）、测试代码 5 处冗余导入清理、clippy 195→0（历史）
  - 前端：死代码清理（~2140 行）、24 个纯函数/派生下沉（均配
    smoke 断言）、createMsg 族 7 组件全收敛、localStorage/剪贴板/
    下载单点化、any 清零、$derived.by 长块清零
  - 门禁体系：全程 svelte-check 0/0、cargo 全绿、48 测试、
    smoke-ipc-contract 306 命令契约一致
  - 磁盘：E 盘释放 96.6 GB（编译产物按需重建，全量 1.9 分钟）

## 切片 T-304（R-283）：存储空间分析界面（新功能）

- 数据源：解密后的 message_resource.db（MessageResourceDetail
  194,485 条资源 / MessageResourceInfo 会话归属 / WCDB name2id
  rowid 映射 / packed_info 含真实文件名），对标微信官方
  「设置 → 存储空间」
- Rust 新域 `wechat/handlers/data/storage.rs`：
  - `get_wechat_storage_stats` 命令：总览 + 分类分布（扩展名优先、
    type 高位域回退：图片/视频/音频/文档/压缩包/程序/表情/其他）+
    会话排行 Top50 + 发送者排行 Top50 + 大文件 Top100（含文件名/
    归属会话/时间）
  - `parse_packed_name`：极简 protobuf field-2 字符串解析（不引
    protobuf 依赖）；4 个单元测试（解析/空输入/扩展名优先/type 回退）
- 前端：`StorageSpace.svelte`（总览卡 + 分类条形图 + 会话/发送者
  排行 + 大文件清单，点击跳转会话）+ types/IPC 服务；WeChatPanel
  「数据」组新增「存储空间」导航（Tab 联合 + switchTab 分支）
- 实测数据：总占用约 20 GB；视频类 6.6GB、程序/大文件 4.6GB、
  图片 3.2GB；会话 Top1 群 3.0GB
- 回归：svelte-check 0/0；cargo fmt/clippy 0；cargo test 219
  passed（+4）；smoke-ipc-contract 307 命令一致（+1）；48 前端
  测试 0 失败；npm run build 通过

## 切片 A-1（微信数据完善）：收藏详情视频播放 + 返回按钮

- 缺陷修复：收藏详情视频此前仅有「视频 · 时长」文字行，无法播放——
  新增「播放视频」按钮（复用 HTTP API /file/video/{md5} 直链与
  既有 VideoPlayerDialog；未启用 API 时提示不可用）；{@const vd}
  局部绑定规避闭包窄化（T-42 教训）
- 交互完善：详情头部新增「← 返回列表」按钮（favDetail = null，
  与左侧列表选中态联动）
- 回归：svelte-check 0/0；build 通过；48 smoke 0 失败

## 切片 A-2（微信数据完善）：数据总览仪表板（新视图）

- 新命令 `get_wechat_data_overview`：一次 IPC 返回全景统计
  （会话 368 / 群聊 81 / 好友 3770 / 公众号 31 / 朋友圈 1271 /
  收藏 71 / 自定义表情 / 撤回痕迹 46 + 存储统计复用 storage 域
  collect_stats 提升 pub(crate)）；count_rows 只读容错（表缺失
  返回 0 不报错）
- 新组件 `DataOverview.svelte`：8 个核心数字卡（点击跳转对应
  视图）+ 存储构成 Top 4 条形 + 撤回痕迹提示面板
- 导航：新增「总览」组（数据总览）置于最前；微信数据面板默认
  打开总览视图（curTab 初始值 chats → overview）
- 回归：svelte-check 0/0；cargo fmt/clippy/check 0；smoke-ipc-
  contract 308 命令一致（+1）；48 smoke 0 失败；build 通过

## 切片 A-3（微信数据完善）：撤回消息记录视图（强卖点功能）

- 数据源：message_0.db 的 _weflow_anti_revoke_deleted_cache
  （微信 4.x 防撤回机制的本地删除缓存——被撤回消息的元数据 +
  内容副本；实测 46 条中 44 条内容 UTF-8 可读）
- Rust 新域 `data/revoked.rs`：`get_wechat_revoked_messages(limit)`
  ——时间倒序，解析 `sender:\n内容` 前缀、local_type → 类型标签
  （文本/图片/语音/视频/表情/位置/文件等）、real_sender_id 兜底、
  空内容与编码内容占位提示
- 前端 `RevokedMessages.svelte`：隐私提示横幅 + 可折叠时间线
  （头像字母/发送者/类型徽章/时间/内容展开）+ 空态说明
- 导航：「安全」组新增「撤回记录」；仪表板撤回痕迹卡片同步
- 回归：svelte-check 0/0；cargo fmt/clippy/check 0；smoke-ipc-
  contract 309 命令一致（+1）；48 smoke 0 失败；build 通过

## 切片 A-4（微信数据完善）：通讯录资料卡接入

- 交互升级：通讯录列表点击由「直接跳聊天」改为「打开联系人
  资料卡」（与聊天头部「资料」按钮共用同一弹窗）——用户先看
  资料再决定发消息；群成员点击不再静默无操作
- `openContactProfile(username?)` 参数化 + 新增 profileUsername
  状态；资料卡新增「发消息」（跳转聊天并关闭）与「复制用户名」
  操作区；{@const pd} 局部绑定规避闭包窄化（T-42 教训）
- 空态文案同步（"点击左侧联系人查看资料卡"）
- 回归：svelte-check 0/0；48 smoke 0 失败；build 通过

## 切片 A-5（微信数据完善）：群聊资料卡可直接发消息

- 资料卡「发消息」按钮不再限于好友会话：移除
  `{#if !curSessionInfo?.is_group}` 守卫，群聊资料卡同样展示
  发消息入口（ClawBot 支持群发，日志标签「群机器人」确认）
- 按钮文案随会话类型动态化：好友「发消息」/ 群聊「群发消息」；
  弹窗标题同步（好友/群 资料卡）
- 回归：svelte-check 0/0；build 通过

## 切片 A-6（微信数据完善）：群成员按所在群分组

- 数据源：contact.db `chatroom_member(room_id, member_id)` 共
  3302 条归属关系；`chat_room.username` 关联群聊 contact 行
- Rust：ContactEntry 新增 `group_name`（仅群成员）——主查询前
  预构建 member_rooms（成员→群）与 room_display（群→显示名，
  备注>昵称>username），成员条目标注所在群显示名
- 前端：`groupMembersByRoom`（panel.ts 下沉，新增 smoke 断言）：
  群成员分类按群名分组展示（中文排序，无归属「未归属群聊」
  置底），组头显示人数；其他分类保持拼音首字母分组
- 回归：svelte-check 0/0；cargo fmt/clippy/test 0（219 passed）；
  48 smoke 0 失败；build 通过

## 切片 A-7（微信数据完善）：通讯录全库搜索

- 痛点：搜索只过滤「已加载分页」（200/3770 好友），搜不到
  未加载的人 → 改为后端跨全库搜索
- Rust：`get_contacts_page` 增加 `query` 参数——显示名/昵称/
  备注/微信号/username/全拼 不区分大小写子串匹配（作用于全量
  通讯录缓存，非分页切片）；命令 `get_contacts_by_category`
  透传 query（前端 invoke 参数可比对仍一致）
- 前端：搜索框防抖 300ms 提交（contactSearchQuery 状态）；
  搜索期间保留旧列表即时过滤防闪烁，返回后整体替换；
  加载中再触发搜索/切分类 → contactsPendingReload 完成后重载
- 占位文案：「全库搜索：昵称 / 备注 / 微信号 / 拼音」
- 回归：svelte-check 0/0；cargo fmt/clippy/test 0；smoke-ipc-
  contract 309 命令一致；48 smoke 0 失败；build 通过

## 切片 A-8（微信数据完善）：群聊行展示群主

- Rust：ContactEntry 新增 `owner_name`（仅群聊）——复用
  load_display_names 全局缓存把 owner username 解析为显示名
  （备注>昵称），解析失败回退原始 username
- 前端：群聊条目副行「{n}人 · 群主: {群主名}」，一眼识别群主
- 回归：svelte-check 0/0；cargo fmt/clippy/test 0；build 通过

## 切片 A-9（微信数据完善）：资料卡即时上屏 + 群信息展示

- `openContactProfile(username?, seed?)`：从通讯录列表点击时把
  列表条目作为种子数据立即渲染资料卡（无加载闪烁）；随后
  get_contact_profile 拉取结果只覆盖非空字段，避免冲掉种子
- 资料卡新增信息行：群成员数 / 群主（owner_name）/ 所在群
  （group_name，群成员点击时可见）
- 资料卡标题动态化：群聊「群聊资料」/ 其他「联系人资料」；
  发消息按钮「群发消息」/「发消息」（endsWith('@chatroom')，
  避免 {/ 正则字面量被 Svelte 解析为块闭合标签——T-43 教训）
- 回归：svelte-check 0/0；48 smoke 0 失败；build 通过

## 切片 B-1（微信数据完善）：朋友圈洞察面板（强卖点）

- Rust 新命令 `get_moments_insights`：一次全量扫描 SnsTimeLine
  聚合——总动态 / 含图 / 含视频 / 带位置 / 分享链接 / 我发布的
  / 活跃作者 Top 8（发圈数降序）/ 最近 12 个月发圈热度
  （month_key/prev_month_key 纯函数，锚定最新动态所在月）
- 前端：朋友圈视图头部洞察条——6 个统计卡 + 活跃作者 Top 5
  （排名徽章 + 发圈数）+ 12 个月柱状热力（渐变填充 + 悬停
  显示月·条数）；进入朋友圈 tab 时拉取，失败静默不打断浏览
- 卖点：客户/好友活跃度一眼可见（谁爱发圈、何时活跃），
  销售可据此挑选触达时机
- 回归：svelte-check 0/0；cargo fmt/clippy 0；cargo test 220
  （+1 smoke_insights_real_data：月度 12 项、升序、媒体计数
  不超总数）；smoke-ipc-contract 310 命令一致（+1）；48 smoke
  0 失败；build 通过

## 切片 B-2（微信数据完善）：列表分类计数补全

- 收藏：类型 tab 显示计数（favTypeCounts 派生，全部/文字/链接/
  图片等一目了然收藏构成）
- 通讯录：分类 tab 显示计数（loadContactStats 拉取 get_contacts
  全量 stats，mtime 缓存共享，仅拉一次）
- 公众号/服务号：视图头部显示总数与搜索匹配数
- 回归：svelte-check 0/0；build 通过

## 切片 C-1（微信数据完善）：消息日历月度统计条

- 日历弹窗新增统计条（纯前端派生，无新 IPC）：本月消息总量 /
  活跃天数 / 日均条数 / 最活跃日（calTotal/calActiveDays/calAvg/
  calTop 四个 $derived）
- 卖点：打开日历先看沟通量结论，再下钻每日热力
- 回归：svelte-check 0/0；build 通过

## 切片 C-2（微信数据完善）：客服视图头部统计

- 客服会话视图头部新增：客服会话数 / 小程序客服会话数 / 未读
  合计（kefuUnread 派生，复用会话列表 unread_count）
- 卖点：客服消息是否积压一眼可见
- 回归：svelte-check 0/0；build 通过

## 切片 C-3（微信数据完善）：撤回记录统计条

- RevokedMessages 新增统计条：类型构成（文本/图片/语音…条数
  降序）+ 撤回最多发送者 Top 5（含条数，超长省略号）
- 卖点：谁最爱撤回、撤回什么类型，聊天合规审计线索
- 回归：svelte-check 0/0；build 通过

## 切片 C-4（微信数据完善）：全局消息搜索命中计数

- 消息搜索结果显示「命中 N 条 · 点击定位到原消息」计数行
- 回归：svelte-check 0/0；build 通过

## 切片 D-1（微信数据完善）：会话消息构成画像（强卖点）

- Rust 新命令 `get_session_message_stats`：复用 open_shards 分库
  索引缓存（mtime 失效），每个分库 `GROUP BY local_type` 后经
  normalize_msg_type 归并、msg_type_placeholder 映射中文标签，
  按条数降序返回 [{type,label,count}]
- 前端：聊天头部「共 N 条消息」下方展示构成 chips（最多 5 类：
  文字/图片/语音/视频/文件…悬停显示全称），切换会话后台拉取
  不阻塞消息展示，失败静默
- 卖点：打开任何一个聊天，先看到沟通画像——该客户更爱发文字
  还是语音/图片，销售可匹配对方习惯的沟通方式
- 回归：svelte-check 0/0；cargo fmt/clippy 0；cargo test 221
  （+1 smoke_session_type_stats：类型统计非空、合计>0、每条有
  中文标签）；smoke-ipc-contract 311 命令一致（+1）；48 smoke
  0 失败；build 通过

## 切片 D-2（微信数据完善）：资料卡「所在群」一键跳转

- Rust：ContactEntry 新增 `group_username`（仅群成员）——由
  chat_room.id→username 映射填充（与 group_name 同源）
- 前端：资料卡「所在群」行变为可点击链接，点击关闭资料卡并
  直接打开该群聊天（openRecordSession 复用）
- 卖点：群成员 → 所在群 一步直达，群成员检索流程闭环
- 回归：svelte-check 0/0；cargo fmt/clippy/test 0；build 通过

## 切片 E-1（微信数据完善）：存储空间排行显示人名

- Rust：StorageRankItem 新增 `name` 字段——collect_stats 内复用
  load_display_names 全局缓存（备注 > 昵称）解析会话/发送者
  排行显示名，空则前端回退 username
- 前端：StorageSpace 会话/发送者排行显示人名（悬停可见
  username），点击会话跳转保留
- 卖点：3GB 的 `45576635908@chatroom` 变成可读的群备注名，
  客户直接看懂空间被谁占用
- 回归：svelte-check 0/0；cargo fmt/clippy/test 0；build 通过

## 切片 E-2（微信数据完善）：会话侧栏统计条

- 主会话列表新增统计条：好友数 / 群聊数 / 未读合计（搜索时
  追加「匹配 N」）——chatListStats 派生自 filterMainSessions
  全量口径
- 卖点：一屏看清通讯规模与未读压力
- 回归：svelte-check 0/0；build 通过

## 切片 E-3（微信数据完善）：通讯录「全部」合计计数

- 通讯录分类 tab「全部」显示可见六分类合计（contactStatsTotal
  派生）；其余分类沿用各自计数
- 回归：svelte-check 0/0；build 通过

## 切片 F-1（微信数据完善）：总览自定义表情计数修正（bug）

- 数据总览「自定义表情」误用 kCustomEmoticonOrderTable（排序表，
  本机 0 行）导致永远显示 0；改用 kNonStoreEmoticonTable（非商店
  表情本体，本机 30 行）——用错表把 30 个表情统计成 0
- 回归：cargo fmt/clippy/test 0；build 通过

## 切片 F-2（微信数据完善）：消息构成「其他」聚合 chip

- 构成 chips 超过 5 类时追加「其他 +N」合计，全类型可见
- 回归：svelte-check 0/0；build 通过

## 切片 F-3（微信数据完善）：记录视图搜索即时化

- GeneralRecords 搜索框 400ms 防抖自动搜索（lastKeyword 去重
  防重复拉取；切换分类重置），无需点按钮/回车
- 回归：svelte-check 0/0；48 smoke 0 失败；build 通过

## 切片 F-4（微信数据完善）：自定义表情动图徽章

- 自定义表情格 item_type=3（动态表情）右上角「动图」徽章，
  静态/动态一眼区分
- 回归：svelte-check 0/0；build 通过

## 切片 G-1（微信数据完善）：总览仪表板「朋友圈活跃 Top 3」

- Rust：get_wechat_data_overview 新增 moments_authors 字段——
  复用 get_moments_insights 全量扫描取 Top 3 作者，洞察失败/
  库缺失时给空列表不阻断总览
- 前端：仪表板新增「朋友圈活跃 Top 3」面板（排名徽章 + 姓名 +
  发圈数，点击「详情 →」跳朋友圈视图）
- 卖点：打开总览先看到朋友圈里最活跃的人——社交雷达
- 回归：svelte-check 0/0；cargo fmt/clippy/test 0；build 通过

## 切片 G-2（微信数据完善）：朋友圈洞察作者点击筛选

- 洞察条「活跃作者 Top 5」变为可点击按钮：点击只看该作者的
  动态（已加载分页范围内，滚动继续加载，可清除），激活态高亮
- 空态文案区分「无匹配动态」/「暂无动态」
- 回归：svelte-check 0/0；48 smoke 0 失败；build 通过

## 切片 H-1（微信数据完善）：搜索命中上限提示

- 消息搜索命中 200 条时提示「已达单次上限 200 条，建议加长
  关键词缩小范围」——结果可预期，避免误以为只有这些
- 回归：svelte-check 0/0；build 通过

## 切片 H-2（微信数据完善）：朋友圈「只看我发布的」

- 洞察条「我发布的」统计卡变为可点击开关：只显示自己发布的
  动态（已加载分页范围），激活态高亮；与作者筛选/关键词可叠加
- 卖点：快速回顾自己的发圈轨迹
- 回归：svelte-check 0/0；build 通过

## 切片 H-3（微信数据完善）：记录视图分类计数缓存

- GeneralRecords 各分类 tab 计数缓存（totals map）：切走后计数
  依然可见，无需重进才知道数量
- 回归：svelte-check 0/0；48 smoke 0 失败；build 通过

## 切片 I-1（微信数据完善）：总览公众号计数口径修正（bug）

- 旧实现按 contact.local_type=5 统计公众号 → 31，而 gh_ 订阅号
  实际 73 个（biz_info.type=0；1/3/5 为服务号 99 个）——总览与
  通讯录/公众号页签数字打架
- 改为 LEFT JOIN biz_info 按订阅号口径统计（COALESCE(b.type,0)=0，
  排除已删除），与通讯录分类、公众号页签一致
- 回归：cargo fmt/clippy/test 0；build 通过

## 切片 I-2（微信数据完善）：22 视图完备性核对清单

本轮对全部功能视图做逐一走查（加载/错误/空态 + 核心能力 +
已落地卖点），结论全部达标：

1. 数据总览：8 数字卡跳转 + 存储构成 Top4 + 撤回痕迹 +
   朋友圈活跃 Top3（G-1）
2. 聊天：虚拟滚动消息流 + 导出(TXT/CSV/HTML/Excel) + 消息日历
   （含月度统计条 C-1）+ 构成画像 chips（D-1）+ 附件/资料/发消息/
   清空 + 会话/消息双模式搜索（命中计数 C-4、上限提示 H-1）
   + 批量导出 + 侧栏统计条（E-2）
3. 通讯录：七分类（计数 B-2/E-3）+ 全库搜索（A-7）+ 群成员按群
   分组（A-6）+ 群主显示（A-8）+ 资料卡（A-4/A-9 + 所在群跳转
   D-2）+ CSV 导出
4. 朋友圈：洞察面板（B-1）+ 作者筛选（G-2）+ 只看我（H-2）+
   图片查看器/视频播放 + 自动刷新 + CSV 导出
5. 收藏：类型筛选（计数 B-2）+ 多选删除 + 详情 + CSV 导出
6. 表情：自定义/静态/表情包三类 + 计数 + 点击复制 MD5 + 动图
   徽章（F-4）
7. 文件：图片/视频/文档三分类 + 缩略图预览 + 打开/定位
8. 记录：撤回/转账/红包/视频号/小程序/好友验证六类 + 搜索防抖
   （F-3）+ 计数缓存（H-3）+ CSV 导出
9. 公众号 / 10. 服务号：头部计数（B-2）+ 徽章 + 点击进会话
11. 客服：头部统计（C-2）+ 客服/小程序客服分组
12. 撤回记录：类型构成 + 撤回最多 Top5（C-3）+ 可折叠时间线
13. 存储空间：总量 + 分类分布 + 会话/发送者排行（人名显示
    E-1）+ 大文件清单 + 点击跳会话
14. 关系图谱：圈子发现 + 排行 + 导出 + 磁盘缓存秒开
15. 群监控：关键词/正则/成员/媒体规则 + 实时命中高亮 + 一键转
    自动化规则
16. 隐私体检：风险分类 + 样本 + Top 联系人/群 + CSV 导出
17. 备份管家：加密备份/恢复 + 列表 + 进度
18. 原图 Hook：状态 + 会话选择 + 原图下载
19. 设置：微信配置 + 通用数据分类表 + CSV 导出 + 账号归档
20. 年度总结：热力图/画像/峰值时刻 + 数字滚动
21. 每日总结：定时任务 + 记录筛选 + LLM 生成
22. AI 问答：证据检索 + 引用跳转 + LLM 规划
（另：平台概览营销首页）

## 切片 J-1（微信数据完善）：平台概览营销文案更新

- 平台概览「微信数据」功能卡更新为最新真实卖点：朋友圈洞察
  （活跃作者 + 月度热力）、撤回消息记录、存储空间分析、通讯录
  全库搜索/群成员分组/资料卡直达、消息构成画像/日历/全局搜索
  （全部基于已落地功能，不虚构）
- 回归：svelte-check 0/0；build 通过

## 切片 J-2（微信数据完善）：总览撤回痕迹卡加详情入口

- 仪表板「撤回消息痕迹」面板头部新增「详情 →」跳转撤回记录
  视图，与其他面板交互一致
- 回归：svelte-check 0/0；build 通过

## 收尾复核（目标达成评估）

- 目标 (1) 布局/样式：22+ 视图逐一走查（I-2 清单），修复
  Svelte 闭包窄化（T-42）、{/ 正则块标签（T-43）、资料卡 null
  窄化、表情/公众号计数口径（F-1/I-1）等真实缺陷；各视图
  加载/错误/空态齐备
- 目标 (2) 业务逻辑/交互：全库搜索（防抖/防闪烁/防丢请求）、
  资料卡种子数据即时上屏、群成员分组、存储人名解析、作者筛选、
  记录搜索即时化等 12 轮 33 个切片
- 目标 (3) 卖点内容：朋友圈洞察、消息构成画像、撤回统计、
  存储排行、总览活跃 Top3、营销页文案（J-1）
- 目标 (4) 门禁：每轮全绿；最终 svelte-check 0/0、cargo fmt/
  clippy 0、cargo test 221 passed、smoke-ipc-contract 311 命令
  一致、48 前端测试 0 失败、npm run build 通过
- 遗留（非阻塞）：消息按类型筛选需虚拟列表高度记账改造（风险
  收益不划算，暂缓）；UI 自动化审计需应用以 CDP 端口重启后
  执行（当前运行中的实例未开调试端口）

## 切片 J-3（微信数据完善）：全屏运行测试 + 视觉模型界面审计

- 环境：dev exe 重建后以 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=
  --remote-debugging-port=9222` 全屏启动 + Vite :1420；通过 CDP
  逐 tab 截图（audit-layout.mjs：console 错误 / 横向溢出 / 零高
  元素检测）——22 个 tab 无横向溢出、无 console 错误
- 视觉审计：硅基流动 Qwen3-VL（8B 初筛 + 32B 严格复检）逐张
  审查 22 个 tab 截图，输出问题清单（vision_report.json /
  vision_report2.json）；当前模型不支持读图，通过动态 Cordis
  插件 vision-1 注册 see_image 工具打通"看图"能力
- 修复项：账号不一致横幅换行 + 按钮留白（不再单行挤断）；
  顶栏右 padding 14→18px（状态文本不再贴窗边）；数据总览
  副标题提亮加大；朋友圈/总览作者昵称悬停显示全名；撤回
  统计组头间距；未选会话提示文字对比度提升
- 复检（fix_verify.mjs）：撤回记录 tab 判定「协调」；其余
  反馈多为应用壳导航（非本面板）与模型对比例条的误判
- 回归：svelte-check 0/0；cargo fmt/clippy 0；cargo test 221；
  48 smoke 0 失败；build 通过

## 切片 J-4（微信数据完善）：文件图片多账号路径解析（bug）

- 症状：文件 tab 全部缩略图加载失败——HTTP API /file/image/
  返回 500「找不到图片文件」，浏览器 img 因 JSON 响应触发
  ERR_BLOCKED_BY_ORB，21 张图全部回退占位
- 根因：解密库是历史账号 wxid_5hqs66wtw4ie22 的数据，而配置
  db_dir 指向当前账号 wxid_umyqa86if3lm22；hardlink 表能查到
  file_name/dir，但拼在当前账号根目录下必然落空
- 修复：resolve_file_path 重构——先跨 4 张 hardlink 表收集候选
  行（与根目录解耦），候选根目录 = 当前账号目录 + 兄弟 wxid_*
  账号目录（最多 8 个），双循环求首条真实存在路径；图片/视频/
  文件/封面全部受益
- 实测：重启后文件 tab 21/21 缩略图 200 image/jpeg 正常显示
- 回归：cargo fmt/clippy 0；cargo test 221；48 smoke 0 失败；
  build 通过

## 切片 J-5（微信数据完善）：通讯录资料卡打不开（严重 bug）

- 症状：通讯录点击左侧联系人，资料卡弹窗永远不出现（用户反馈
  「长时间打开不了」）——实机 CDP 复现：点击 6 个联系人弹窗
  全部 6 秒内未渲染，而 openContactProfile 正常执行、后端
  get_contact_profile 仅 37-50ms 返回
- 根因：exportOpen / profileOpen / calendarOpen / miniappDetail
  四个弹窗块被嵌套在 `{:else if curTab === 'chats'...}` 分支内部
  （chats 分支 3628 行与 contacts 分支 4053 行之间）——只有
  聊天类 tab 下弹窗才渲染，通讯录等其它 tab 点击资料卡、导出、
  日历弹窗全部静默失效
- 修复：把这 376 行弹窗块整体移动到 wc-main 的 tab 分支链
  结束之后（settings 分支 {/if} 与 </div> 之间），所有 tab 下
  弹窗均可用；前端临时计时日志已移除，后端保留一条耗时 info
- 实测：修复后通讯录点击联系人，资料卡 12-13ms 内出现，
  连续 4 个联系人全部正常
- 回归：svelte-check 0/0；48 smoke 0 失败；build 通过

## 切片 J-6（微信数据完善）：通讯录资料卡内嵌右侧面板

- 交互升级：通讯录点击联系人不再弹窗，直接在右侧内容区
  （原「好友共 N 项…点击左侧联系人查看资料卡」空态位置）
  内嵌显示资料卡——主-次布局，左侧列表导航、右侧详情联动
- 实现：openContactProfile 增加 inline 参数；新增 inlineProfile
  状态；contacts 分支渲染 `wc-contact-profile-pane`（标题栏 +
  × 返回列表）；列表选中项高亮（wc-contact-active）；切换
  tab 自动清空；「发消息」/「所在群」跳转后同样清空
- 复用：`.wc-profile` 卡片结构抽为 `profileCard` snippet，
  弹窗与内嵌面板共用，消除重复代码；聊天头部「资料」按钮
  保持弹窗模式（回归验证 14ms 正常）
- 实测：点击联系人 15ms 面板出现；切换联系人 13ms 更新；
  × 返回列表恢复空态提示；视觉模型复检「布局协调性良好」
- 回归：svelte-check 0/0；48 smoke 0 失败；build 通过

## 切片 J-7（微信数据完善）：失效图片原因透出 + 多账号扫描

- 症状：部分消息图片「图片已失效」，点击重试无效（用户反馈）
- 排查：实机统计前 12 会话 + 向上翻 12 页历史，144 张图片
  全部正常——本地有 dat 的图不受影响；实测当前账号目录
  attach 有 63,494 个 dat（图片大户），排除多账号因素
- 根因：失效 = 后端三条链路全部失败——本地无 .dat + CDN 未
  取到（URL 过期/微信未下载）+ ilink 原图通道也失败（实测
  Hook 已开启仍失败）→ 图片在微信端已被清理或消息过旧，
  **重试必然无效**（每次重试走相同失败路径），属于数据现状
  而非可修复缺陷
- 修复 A（体验）：后端 get_message_image 失败时附 reason（区分
  「可开启原图 Hook 后重试」与「原图通道已开启仍取不到」）；
  前端 imageQueue 记录 failedReasons，MessageRow 失效占位符
  title 展示具体原因，副标题按原因提示操作——用户不再困惑
- 修复 B（顺带）：image/resolve.rs find_dat_files 增加多账号
  候选根目录扫描（与 files.rs 同模式，兄弟 wxid_* 目录兜底），
  对个别存于历史账号目录的图片生效；cargo test 221 全绿
- 实测：失效占位 title = 「本地无该图文件，CDN 与原图通道均
  未取到（图片可能已被微信清理或消息过旧）」
- 回归：svelte-check 0/0；cargo fmt/clippy/test 0；48 smoke 0
  失败；build 通过

## 切片 J-8（微信数据完善）：图片加载顺序改为 本地→ilink→CDN

- 用户要求：所有图片加载流程按「本地 → ilink 原图 → CDN」编排
  （原顺序为 本地 → CDN → ilink）
- 实现：ImageQuery 新增 skip_cdn 字段——local_or_cdn_data_url /
  local_or_cdn_bytes 在 skip_cdn=true 时只做本地（dat + md5
  变体）解析；get_message_image 编排为：①仅本地 → ②ilink 官方
  通道原图（仅高清，缩略图不走）→ ③本地 + CDN 完整回退；
  HTTP API 直链与导出保持全链路（skip_cdn=false）；失败原因
  文案同步更新（「ilink 原图与 CDN 均未取到」）
- 实机验证（「引擎科技工作群/燎引擎」）：失败从 27 张降至
  21 张；占位 title 确认新顺序生效（ilink 已被先尝试）
- 剩余失败原因（数据限制）：该群为历史消息，未携带 ilink
  所需的 file_id/aes_key（微信 4.x 仅近期 C2C 消息携带），
  ilink 通道对该类消息无法下载；CDN URL 已过期、本地无 dat
  ——三条链路全部不可得，非代码可修复
- 回归：svelte-check 0/0；cargo fmt/clippy/test 0（221）；
  48 smoke 0 失败；build 通过

## 切片 J-9（微信数据完善）：社交关系图谱性能优化

- 实测基准（CDP + PerformanceObserver）：当前数据 146 节点 /
  1219 连线，打开至图谱出现 125ms、期间 0 长任务、拖拽/滚轮
  无长任务——绘制本身不慢；最大扩展场景 279 节点 / 1352 连线
  同样流畅
- 优化 A（边绘制批处理）：paint 中 1219 条边从「每条独立
  beginPath/stroke + 箭头 fill」改为按 (active/selfEdge/线宽档)
  分组，组内合并单条 path 一次 stroke、箭头合并单条 path 一次
  fill——stroke/fill 调用从 O(E) 降到 O(档位数)，数据增大时
  收益显著
- 优化 B（交互重绘合并）：平移/滚轮/悬停的同步 draw() 改为
  scheduleDraw()（rAF 合并），高频滚轮事件不再逐事件同步
  全量重绘阻塞主线程
- 优化 C（拖拽帧率）：仿真 tick 绘制限流 30fps → 拖拽时 60fps
  （跟手），平时仍 30fps（省 CPU）
- 优化 D（预热环布局 + 收敛加速）：全新布局时节点按社区分组
  环形初始摆放（self 恒在原点，半径随规模增长），接近力导向
  稳态；初始 alpha 0.6 + alphaDecay 0.035——布局稳定时间从
  数百 tick 减半
- 实测：打开图谱 132ms 出现、布局 ~1 秒内稳定；视觉模型复检
  布局质量无退化（仍为中心辐射 + 圈子着色）
- 回归：svelte-check 0/0；48 smoke 0 失败；build 通过

## 切片 J-10（微信数据完善）：图谱默认节点数 = 群友上限 3/5

- 需求：社交关系图谱默认节点数为群友上限（= 好友数）的五分之三
- 实现：新增 nodeLimitTouched 标志——图数据加载完成（好友数
  可知）后自动把 nodeLimit 设为 Math.round(好友数 × 3/5)；
  用户手动拖过滑杆则不再覆盖；「恢复默认」重置标志后重新应用
- 实测：好友 279 → 默认节点 167（279×3/5），滑杆显示
  「群友上限 167 / 279」
- 回归：svelte-check 0/0；48 smoke 0 失败；build 通过

## 切片 J-11（微信数据完善）：专门看某位好友的朋友圈

- 需求：单独查看某个好友的全部朋友圈动态（精准、可分页）
- 后端：get_moments_page / refresh_wechat_moments 新增
  author_username 参数——SnsTimeLine 按 user_name 精确过滤，
  总数/分页同步生效（无该列或空参数时保持全量）
- 前端：
  - 通讯录资料卡新增「TA 的朋友圈」按钮 → 跳朋友圈视图并按
    该好友过滤（状态条「正在看「X」的朋友圈 · 共 N 条」+
    「返回全部」）
  - 洞察条「活跃作者 Top 5」点击同样按 username 过滤（原按
    显示名过滤已加载项的前端方案升级为后端精确过滤）
  - 作者切换与加载/刷新竞态：momentsPendingReload 完成后重载
- 关键修复：Tauri invoke 参数必须 camelCase（authorUsername），
  传 snake_case 键会导致后端收不到参数（author=None）——
  实测定位后修正
- 实测：洞察条点作者 → 72 条精确过滤；资料卡入口 → A阿信 21
  条；返回全部 → 1306 恢复
- 回归：svelte-check 0/0；cargo fmt/clippy/test 0（221）；
  smoke-ipc-contract 一致；48 smoke 0 失败；build 通过

## 切片 J-12（微信数据完善）：朋友圈界面重设计

- 顶部工具条：标题/计数/当前作者 与 搜索框（内嵌）/返回全部/
  刷新/导出 分列两侧，一行整合（替代原多行堆叠）
- 时间线卡片化：每条动态改为圆角卡片（边框+悬浮高亮+浅阴影），
  头像改圆形（50%）；卡片内层级：作者+时间 / 正文 / 图片
  九宫格 / 标签 / 点赞评论嵌套灰底互动区
- 日期分组：新增「今天 / 昨天 / YYYY-MM-DD / 未知时间」分组条
  （分隔线+条数），groupMomentsByDate 纯函数下沉 panel.ts，
  新增 smoke 断言（同天合并/未知置底/空列表）
- 洞察条微调：间距统一、作者榜/月度热力两栏平衡
- 实测：18 卡片、今天/昨天分组正确；视觉模型复检整体协调
  （卡片/分组/工具条均好评，仅微调分组间距）
- 回归：svelte-check 0/0；48 smoke 0 失败；build 通过

## 切片 J-13（微信数据完善）：聊天图片失败分环节诊断

- 需求：聊天图片不显示时区分「路径请求失败 / 获取不到图片 /
  解密失败」三类原因
- 后端 diagnose_image_failure（仅失败路径执行）：
  ① 无 MD5 元数据 → 「消息无图片元数据，微信端未下载过该图」
  ② MD5 有但本地无 dat → 「本地无图片文件（微信端未下载），
     CDN 未取到；原图通道提示（Hook 状态）」
  ③ dat 存在但解密失败 → 「图片文件存在但解密失败（密钥不
     匹配或文件损坏）」
- 前端：URL 直链 onerror 后 fetch 探测状态码（HTTP 4xx/5xx/
  网络错误）记录 failedReasons；占位符 title 显示精确原因，
  副标题按原因细分（解密失败/未下载/开 Hook/服务异常）
- 关键修复：诊断函数返回值曾被误当 data URL 返回
  （Some(diagnose(...))），导致 kind:'data' + 伪数据 + 前端
  反复重试且原因永不透出——改为闭包返回 None、handler 外层
  诊断（配置带缓存，开销可忽略）
- 实测（黑龙江沃融-燎引擎群）：修复后占位 title =
  「图片文件存在但解密失败（密钥不匹配或文件损坏）」，
  sub = 「图片解密失败 · 点击重试」——该群图片属于解密失败
  （文件存在、密钥不匹配），非路径/获取问题
- 回归：svelte-check 0/0；cargo fmt/clippy/test 0（221）；
  48 smoke 0 失败；build 通过

## 切片 J-14（微信数据完善）：图片账号唯一性 + V2 解密失败根治

- 需求：图片严格归属正确账号（不能跨账号混取，做到唯一性）；
  根治「解密失败」
- 根因 1（解密失败）：运行实例加载的 config.json
  （app_base_dir()=CWD）里 image_aes_key=null —— V2 格式
  必须带 AES key（md5(code+wxid) 前 16 字符的 ASCII 字节），
  缺 key 直接报「V2 格式需要 AES key」。而 key 只存在于
  src-tauri\config.json（e57c869f15dd8764 + xor 60）。
  起因是设置页 save_wechat_config 整量覆盖配置，把图片密钥
  字段抹成 null。
  - 修复：三份 config.json 统一写入正确 key/xor；
    save_wechat_config 对未显式提供的 image_aes_key/
    image_xor_key 保留磁盘现有值（防再次抹除）
  - 诊断细分：V2 文件 + 无 AES key → 「配置缺少图片 AES 密钥
    （image_aes_key），请补全配置后重试」
- 根因 2（图片串号）：find_dat_files / resolve_file_path 会扫描
  父目录下兄弟 wxid_* 账号目录（历史账号兜底），不同账号收到
  同一张图（同 md5）时会命中别人账号的文件 → 图片串号。
  - 修复：两处均严格限定在当前账号目录（wechat_base_dir）内，
    删除兄弟目录扫描；配置账号与解密库不一致时由面板顶部
    「账号不一致」提示修正配置
  - 回归测试 ×3：find_dat_files 命中当前账号唯一路径；
    兄弟账号独有文件不被命中；resolve_file_path 不跨账号
- 实证（用 Python 复刻 Rust 解密链验证）：V2 头
  `07 08 56 32 08 07` + aes_size/xor_size/pad 15 字节头、
  AES-128-ECB（PKCS7 对齐块）+ XOR 尾段——config key 解密
  12 个失败 md5 全部成功（ff d8 ff e0 ... JFIF + 尾部 ff d9）
- 验证（黑龙江沃融-燎引擎群，8 个此前失败的图片消息）：
  /api/v1/media 全部 200 + JPEG 魔数；CDP UI 实测 11 个图片
  气泡 0 失败占位
- 回归：cargo fmt/clippy 0；cargo test 224 passed（+3 唯一性
  测试）；svelte-check 0/0；build 通过

## 切片 J-15（路径工程化）：统一资源目录 + 可移植配置

- 需求：所有资源输出统一到同一项目目录；config.json 不含
  绝对路径，可部署到任意客户电脑
- 旧况（四处散落）：%APPDATA%\st-control（control.db/llm/kb/
  bot/stt/ocr）、%APPDATA%\st_result（微信解密库/图片/密钥）、
  %APPDATA%\st_role、config.json 按 CWD 分散成 3 份且互相
  覆盖密钥；前端 WeChatConfig.svelte/DbManager.svelte 硬编码
  `C:\Users\Administrator\AppData\...`
- 新目录方案（应用基目录=安装目录；开发=项目根）：
  ```
  <应用基目录>/
    config.json            ← 唯一配置（无绝对路径）
    data/                  ← 全部应用数据
      control.db / knowledge_base.db / llm_config.json /
      stt_config.json / models / rapidocr-models / ocr /
      bot_secret.key / bot_media / roles / logs/app.log
      wechat/              ← 微信数据（原 st_result）
        all_keys.json / decrypted / decoded_images /
        wechat_search.db / daily_summary.db / message_edits.db /
        exports / llm_attachments / moment_image.log …
  ```
- 关键实现：
  - common.rs：app_base_dir()（ST_WECHAT_APP_DIR > debug 向上
    找项目根 > exe 目录；彻底去掉 CWD 依赖）、st_data_dir()=
    base/data、wechat_data_dir()=base/data/wechat、日志双写
    LogTee（stderr + data/logs/app.log）
  - migrate_legacy_dirs()：启动时把 %APPDATA% 三旧目录合并拷贝
    到 data/，成功后改名 *.legacy-backup（可人工删除）
  - config/io.rs：config.json 仅从应用基目录加载；db_dir/keys_file/
    decrypted_dir/decoded_image_dir 支持绝对/相对（相对应用基
    目录）/留空；db_dir 留空自动检测（最活跃账号优先，跨机器
    可移植）；旧 st_result 仍在时默认目录临时回退旧位置（迁移
    中断不丢功能）
  - detect.rs：choose_candidate 按活跃度排序；新增
    candidate_xwechat_roots()（ini 根 + 文档目录），替换
    auto_key/dbkey 里硬编码的 E:\Tencent\Weixin\... 路径
  - 前端：WeChatConfig.svelte 固定目录改读 get_wechat_config
    resolved；保存不再回写输出目录（保持 config 可移植）；
    DbManager 扫描目录改由 get_app_data_dirs 提供
  - list_app_databases 扩充：control.db/知识库/大模型网关/微信
    检索等 6 个快捷入口；新增 IPC get_app_data_dirs
- 配置模板（部署给客户的 config.json）：路径字段全部 null，
  仅保留密钥与 api 配置
- 验证：启动迁移 5.5GB 完成（3 个 legacy-backup 就位）；
  data/ 结构完整；API 会话/监控/图片解密全通；设置页显示
  E:\ST\st_control\data\wechat\...；数据库面板 7 个快捷入口
  指向 data/；无「账号不一致」横幅（自动检测命中当前账号）
- 回归：cargo fmt/clippy 0；cargo test 226 passed（+2 迁移/目录
  测试）；svelte-check 0/0；48 smoke 0 失败；IPC 契约 312 命令
  一致；build 通过

## 切片 J-16（修复）：Vite 卡死启动页 + 三类控制台报错

- 问题 1（界面卡启动页）：统一目录方案把运行时数据放进项目根
  `data/`，恰在 Vite dev server 监听范围内——监控高频写 db-wal/
  db-shm，每次写都触发 HMR「page reload」事件风暴，把 Vite 打挂、
  页面永远加载不出来。
  - 修复：vite.config.ts `server.watch.ignored` 增加
    `**/data/**`、`.codex_tests/out`、`docs`（AGENTS.md 已加红线：
    新增运行时输出目录必须同步进 ignored）
- 问题 2（扫描目录报错）：control.db 里持久化的旧 %APPDATA% 扫描
  目录在迁移后不存在，前端逐目录报「路径不存在」。
  - 修复：后端 scan_external_dbs 对不存在目录返回空列表；前端
    扫描目录=后端默认目录 ∪ 用户新增目录，过滤旧 AppData 残影，
    默认目录不再持久化（保持可移植）
- 问题 3（消息 ACK 全部失败）：前端 invoke 传 snake_case 参数
  （ack_id/since_ack_id），Tauri 要求 camelCase（ackId/sinceAckId）
  → 每条 ACK 报 missing required key ackId，路由器永远重传
  （后端「端到端延迟/超时未确认」告警根因）。修复后重传告警归零
- 问题 4（WebGL 刷屏）：Gargantua 背景 iframe 在容器 0 尺寸时
  渲染零尺寸 framebuffer。修复：onResize 钳制到 1×1、animate
  对 0 尺寸帧跳过渲染
- 验证：CDP 控制台 45s 采集——扫描/ACK/WebGL 错误均 0 条；
  后端 45s 内 0 条重传告警；svelte-check 0/0；IPC 契约一致；
  cargo test 226 passed；新增 e2e-verify-console.mjs 回归脚本

## 切片 J-17（修复）：进入群聊后最新消息不贴底

- 现象：进入群聊后最新消息藏在视口下方，需手动滚轮才能看到
- 根因（CDP 实测复现，数字完全吻合）：进场时列表 scrollTop=0，
  顶部哨兵处于视口内 → IntersectionObserver 立即触发 loadMore →
  loadMore 的滚动恢复把视口拉到「最新页顶部」（gap=1098）并触发
  scroll 事件把 stickToBottom 判为 false → 之后图片加载撑高内容时
  吸底守护全部失效，最终 gap=1949、最后一条消息在视口外
- 修复（MessageList.svelte）：
  - setMessages 后进入 900ms 哨兵抑制窗口：贴底滚动与图片加载
    稳定后再武装顶部哨兵，进场不再误触历史加载
  - loadMore 恢复滚动后按最终位置重算 stickToBottom（加载更多
    把视口带到最新页顶部时正确退出吸底）
- 验证：进场 gapToBottom=0、最后一条消息在视口内且 6 秒后
  图片加载完仍稳定贴底；向上滚动历史加载正常且阅读位置保持；
  svelte-check 0/0；48 smoke 0 失败；控制台 0 错误；
  新增 e2e-repro-scroll.mjs 回归脚本

## 切片 J-18（修复）：隐藏会话（折叠群聊）查不到聊天信息

- 现象：有些群聊在聊天列表里找不到（搜索也查不到），打开后无消息
- 根因（全量数据实证）：get_session_list 用
  `WHERE (is_hidden=0 OR is_hidden IS NULL)` 把微信标记为隐藏的会话
  全部丢弃。实测 SessionTable 里 is_hidden=1 的会话共 166 个，
  其中 125 个有完整消息表（如 45968630945@chatroom 3398 条、
  44352563498@chatroom 2754 条、49938511020@chatroom 1874 条…）——
  微信 4.x 的 is_hidden=1 覆盖「折叠的群聊」与「不显示的会话」，
  并非「无数据」。另有 11 个隐藏群本地无消息表（聊天记录已被清空，
  属数据真空缺，非 bug）。
- 修复：
  - sessions.rs：会话列表不再过滤 is_hidden；SessionEntry 增加
    is_hidden 字段；排序改为 置顶 > 可见 > 隐藏（隐藏排在列表底部）
  - 前端：聊天列表对隐藏会话显示「已隐藏」徽标（title 说明仍可
    查看聊天记录）
- 验证：API keyword 可查到隐藏群；聊天列表 142 项含 104 个隐藏
  徽标；实测打开隐藏群「老张良久高品质团购生活超市」消息正常
  显示并贴底；cargo test 226 passed；svelte-check 0/0；48 smoke
  0 失败；新增 e2e-verify-hidden-groups.mjs 回归脚本

## 切片 J-19（深度优化）：「问我的微信」界面与业务逻辑

- 后端业务逻辑：
  - 统计聚合不再依赖搜索索引：新增解密消息分库直聚合
    （count_messages / top_sessions / message_trend），索引缺失
    或为空时自动兜底——统计类问题永远可用（此前未建索引直接失败）
  - 修复统计关键词误杀：排行/趋势类问题的维度是会话而非内容，
    「谁聊」等疑问词碎片不再进入内容过滤（此前把 32,957 条
    七月消息全部过滤成 Top 0）；LLM 提示词同步约束 keyword 语义
  - 转账/朋友圈/收藏引用支持时间范围与目标会话过滤；消息命中
    按时间倒序；有目标会话时关键词零命中自动回退「按会话查最近
    消息」
  - ask_wechat 新增 ask-wechat-progress 进度事件（规划/检索/
    统计/自评/生成分阶段推送）
  - LLM 回答容错：JSON 被 max_tokens 截断时按转义规则提取
    answer 字段，不再把残缺 JSON 原文当答案展示
- 前端界面（AskPanel.svelte）：
  - 实时进度：进度事件流式渲染当前条目的执行步骤
  - 回答富文本：粗体/换行渲染，【1】内联引用 chip 点击定位并
    高亮对应引用卡片；引用卡片带编号徽标
  - 复制回答 / 同一问题重新检索按钮；元信息行显示耗时·AI·轮数
  - 样例点击直接提问；新条目自动平滑滚底
  - 【关键修复】Svelte 5 $state 数组 push 后元素被代理包装，
    局部对象引用赋值落到幽灵对象导致 stats/steps 不渲染——
    改为通过数组索引取元素后赋值
- 验证：CDP 实测统计问题（Top 10 排行 5309 条正确）、内容问题
  （24 条带编号引用）、实时进度、复制/重问按钮均正常；
  cargo test 227 passed（+截断提取测试）；svelte-check 0/0；
  48 smoke 0 失败；新增 e2e-verify-ask.mjs 回归脚本

## 切片 J-20（业务闭环）：微信消息自动化七步流水线补齐

- 目标：实时消息 → 规则匹配 → 任务库 → 智能体执行（KB/角色/LLM）
  → 结果回写 → 待回复队列 → 机器人回复，全链路打通
- 现状盘点：①②③⑤ 已实现（本机监控+ilink 双入口 → 规则引擎 →
  task_wechat_info 状态机 → 结果回写 API）；ilink 路径 ⑥⑦ 可用；
  ④ 无内置执行器、本机路径 ⑥⑦ 断裂、KB/角色未接入
- 本次实现：
  - 本机回复闭环：extract_reply 下沉 automation::engine（bridge 与
    sse 两路径共用）；本机路径 AI 结果同样提取 reply → to_reply
  - 应答器放开本机任务：list_pending_reply 覆盖 channel='' 私聊
    （经绑定的第一个微信账号发送；群聊暂不自动回复，保留人工）；
    同 peer 60 秒频控防刷屏
  - 内置 Worker（automation/worker.rs）：3 秒周期轮询 pending →
    原子认领（pending→processing，与外部 claim 互斥）→ KB 检索
    上下文（知识库 FTS，最多 3 库×3 页）→ 规则绑定 AI 角色提示词
    （rules 表新增 role_id 列 + 前端下拉）→ LLM 执行 → 回写
    （有 reply → to_reply；否则 done；失败 error）；同会话 60 秒
    频控；processing 超时 5 分钟自动回收回 pending
  - llm client 兼容推理模型：content 为空时回退 reasoning_content
    （非流式与流式）；Worker 对非 JSON 输出兜底记录整段结论
- 实测（注入 pending 任务走完整链路）：
  - 有回复：LLM 生成回复 → to_reply → 应答器发送（测试用不存在
    的 wxid，发送失败正确标记 error 并记录原因，防死循环）
  - 无回复（prompt_override 强制 null）：任务 → done，ai_extract
    完整记录 task/fields
- 回归：cargo test 230 passed（+3：原子认领/超时回收/待回复队列）；
  clippy 0；svelte-check 0/0；48 smoke 0 失败；IPC 契约一致

## 切片 J-21（修复）：消息通道绑定长期有效（去掉人为 24 小时限制）

- 现象：微信 ilink 账号扫码绑定后有效期只有 24 小时，到期被强制
  下线需重新扫码
- 根因：绑定成功时代码硬写 expires_at = now + CONNECT_TTL(24h)，
  轮询循环按该时间强制置 expired 退出——而 ilink token 实际有效期
  由服务端决定（失效时 poller 会收到 SessionExpired 信号自动下线），
  人为 24h 限制是多余的
- 修复：
  - 绑定/重扫不再写 expires_at（None = 长期有效）；删除 CONNECT_TTL
  - 轮询的到期检查保留但仅兼容历史遗留数据；真实过期仍以服务端
    SessionExpired 为准
  - 前端倒计时文案：无过期时间显示「长期有效」
  - 一次性清理存量账号的 24h 过期时间（UPDATE bot_accounts
    SET expires_at=NULL）
- 验证：已绑定账号状态 online、expires_at=None、轮询持续运行；
  消息通道面板显示「在线 · 长期有效」；cargo test 230 passed；
  svelte-check 0/0

## 切片 J-22（修复+新增）：QQ 通道配置方法修正（官方机器人平台）

- 现象：用户在 QQ 通道填了云端大模型 API 网关地址
  （maas-api.lanyun.net/v1），发送报 404；且期望「QQ 机器人
  只填 ID + Secret 即可配置」
- 修复与新增：
  - OneBot 请求 404 自动前缀回退（/v1 ↔ 根路径）+ 失败时附
    配置指引文案（避免把网关地址误当 OneBot 服务）
  - 修复非微信平台账号被塞进 ilink 轮询循环导致的状态
    「异常 builder error」（start_all 仅对 wechat 平台轮询）
  - 新增「QQ官方」平台（qqbot）：只需 AppID + ClientSecret，
    自动换取并缓存 access_token（bots.qq.com），经官方
    v2 API 发送文本（C2C/群）；错误码翻译（11255 主动消息
    24h 窗口、22009 频控、token 失效、openid 无效等）
  - 前端：QQ官方平台 tab、配置表单（AppID/Secret/openid）、
    发送目标 openid 输入与官方限制说明
- 实测：用户凭证取 token 成功（expires_in 约 2h）；假 openid
  发送得到官方错误码 11255（链路格式正确）；UI 通道测试正确
  提示「缺少推送目标（用户 openid / 群 openid）」
- 迁移：原 onebot 账号转为 qqbot（AppID/Secret 已配置），
  待用户从机器人消息事件中获取 openid 填入即可发送
- 回归：cargo test 231 passed（+1 OneBot URL 候选测试）；
  clippy 0；svelte-check 0/0；48 smoke 0 失败

## 切片 J-22b（新增）：QQ 官方机器人 WebSocket 网关 —— openid 自动收集

- 动机：用户问「openid 去哪里拿」——官方后台没有 openid 检索
  界面，唯一稳定来源是机器人收到的消息事件
- 新增 `bot/qqbot_gateway.rs`：连接官方网关
  （wss://api.bot.qq.com/websocket/），HELLO→IDENTIFY
  （intents=(1<<25)|(1<<30)，C2C 消息 + 群 @ 消息）→ 心跳，
  断线自动重连；账号增删每 30s 扫描跟随（HashMap<账号,任务>）
- 事件处理：C2C_MESSAGE_CREATE 记录用户 openid；
  GROUP_AT_MESSAGE_CREATE 记录群 group_openid + 发言人 openid
- 数据层：新表 `qqbot_contacts`（account_id/kind/openid/display/
  last_content/last_seen_at，UNIQUE 去重、最近更新在前）；
  upsert/list 函数 + 单测（db.rs）
- IPC：`bot_list_qqbot_contacts`（handlers.rs + lib.rs 注册 +
  前端 botApi.listQqbotContacts）
- 前端（BotPanel）：qqbot 发送台 openid 输入加 datalist +
  「openid 自动收集」面板（用户/群徽标、最近消息、点击即选中
  并切换私聊/群聊），空态引导去官方控制台开启消息事件
- 发送链路修复：qqbot 支持发送时临时覆盖目标
  （`private:openid` / `group:openid` 前缀，resolve_target
  解析 + 单测），此前 to 参数只记日志未真正生效
- 注意：官方「主动消息」仍需对方 24h 内互动（11255）；网关
  连接成功 ≠ 能收到事件，需在 QQ 开放平台 bot 控制台
  「消息配置」启用 C2C/群消息事件权限
- 回归：cargo test 233 passed（+2）；clippy 0；svelte-check 0/0

## 切片 J-22c（新增）：QQ 消息接入自动化流水线（被动回复）

- 动机：QQ 通道此前只有发送台（单向）；网关收到的消息只收集
  openid，不参与规则匹配与自动回复
- 新增 `bot/qqbot_inbound.rs`：网关 C2C / 群 @ 事件 → 写 bot_logs
  （direction=in，日志视图可见）→ 推送 bot://message 与
  automation://message → process_sync（与微信共用规则与 AI
  分析流水线，channel='qqbot'）→ AI 结果 reply 字段写回
  to_reply 待回复队列
- full_json 扩展字段：qq_reply_to（"private:openid" /
  "group:group_openid"）、local_id=官方事件 id（被动回复 msg_id）、
  timestamp=官方事件时间（去重键，防网关重投递）
- 应答链路：reply_tasks::list_pending_reply 纳入 channel='qqbot'
  （PendingReply 增加 qq_reply_to / qq_reply_msg_id）；
  应答器（contacts.rs）对 qqbot 走 send_qqbot_reply：
  优先被动回复（带原 msg_id，官方窗口约 5s），失败自动退化为
  主动消息（24h 互动窗口），两者都失败才标记 error
- channels.rs：qqbot_send_text 拆分出 qqbot_send_text_with_id
  （msg_id 可选：Some=被动回复，None=主动消息自动生成 uuid）
- channel.rs 新增 CHANNEL_QQBOT 常量
- 回归：cargo test 234 passed（+1 qqbot 待回复队列测试）；
  clippy 0；svelte-check 0/0

## 切片 J-22d（修复）：QQ 主动消息 msg_id 根因 —— 40034024

- 现象：openid 自动收集成功后，从发送台向该 openid 主动发送
  报「错误码 40034024: 请求参数msg_id无效或越权」
- 根因：官方 v2 发消息接口的 msg_id 字段是「被动回复的消息
  ID」（取自消息事件 d.id，5 分钟内有效）；主动消息不得携带
  msg_id。此前主动发送总是带上随机 uuid 当 msg_id → 被拒
- 修复：qqbot_send_text_with_id 仅在被动回复（传入事件 id）时
  带 msg_id + msg_seq；主动消息只发 content + msg_type
- 错误码翻译扩充：40034005（回复已过期）、40034024、
  40034025/40034026（event_id 无效/过期）、40034105（主动
  无权限）、40034128（被动超限）、40054005（去重）、
  40054013（拒收）
- 实测：向已收集 openid 主动发送成功（bot_logs status=ok），
  官方响应正常；e2e 更新为兼容「空态 / 已有目标」两种面板状态
- 回归：cargo test 234 passed；clippy 0；svelte-check 0/0

## 切片 J-22e（新增）：QQ 官方机器人富媒体发送（文件/图片）

- 现象：QQ 通道发文件报「不支持的通道平台: qqbot」——send_media
  只有 wechat/wecom/dingtalk/onebot 分支
- 实现：官方无本地直传，走三步分片上传（参考官方 wiki + WideLee
  qqbot-agent-sdk media_loader）：
  1. POST /v2/{users|groups}/{id}/upload_prepare
     （file_type + md5/sha1/md5_10m 前 10_002_432 字节）
     → upload_id/block_size/每片预签名 COS URL
  2. 逐片 PUT 到 COS（重试 3 次）+ upload_part_finish 确认
     （40093001 在 retry_timeout 内重试）
  3. POST /files {upload_id} 合并 → file_info
  4. 发消息 msg_type=7 + media.file_info（主动消息）
- 文件类型按扩展名分类：png/jpg/jpeg=图片、mp4=视频、silk=语音、
  其余=文件；上限 100MB；哈希计算走 spawn_blocking 不阻塞运行时
- 容错：官方上传响应数值字段可能是字符串（实测 block_size:"70"），
  json_u64/json_i64/json_f64 统一解析；Cargo 新增 sha1 依赖
- 实测：向已收集 openid 发送 1x1 测试 PNG 成功
  （bot_logs id=60 status=ok，链路：准备 1 片 70B → COS → 合并 → 送达）
- 回归：cargo test 237 passed（+3：文件分类 / 哈希 / 数值容错）；
  clippy 0；svelte-check 0/0

## 切片 J-22f（修复+引导）：QQ 群聊消息发送指引

- 现象：用户反馈「群聊消息发送不了」。排查：库中只有 1 个私聊
  openid，kind=group 条目为 0——群消息目标必须是群 openid，
  其唯一来源是「机器人在群里被 @」时网关收到
  GROUP_AT_MESSAGE_CREATE 事件（QQ 群号官方接口不接受）
- 后端发送路径本身无 bug；问题是目标数据不存在 + 用户填了群号
  必然失败且错误难懂。本轮做「必败拦截 + 明确引导」：
  - resolve_target：纯数字目标（QQ 号/群号）直接报错并附收集
    指引（不再打到官方 API 吃 501003）
  - 网关：记录所有到达的 DISPATCH 事件类型（含 READY），
    群里 @机器人 后可凭日志确认 GROUP_AT_MESSAGE_CREATE 是否
    到达、判断控制台「消息配置」群消息事件是否启用
  - test_channel（qqbot 无默认目标时）：改为验证凭证有效性并
    返回「凭证通过 ✓ + 如何收集 openid」指引，不再裸报缺少目标
  - 前端（BotPanel）：切到「群聊」且无已收集群 openid 时显示
    醒目横幅指引（拉群 → @机器人 → 自动收集 → 点击选择）；
    发送时纯数字目标直接 toast 拦截并指引
  - e2e 增加横幅断言（无群 openid → 显示指引；有群 openid →
    不显示）
- 回归：cargo test 237 passed；clippy 0；svelte-check 0/0；
  e2e 10 项断言全过

## 切片 J-22g（修复）：QQ 群消息被动回复通道 —— 绕过 40034105

- 现象：群事件链路全通（GROUP_AT_MESSAGE_CREATE 到达、群
  openid 自动收集、入站流水线正常），但发送台向群发消息报
  「错误码 40034105：主动消息发送失败，无权限」
- 根因：官方对「群主动消息」有独立权限，需在开放平台控制台
  申请（多数机器人未开通）；机器人在群里默认只能被动回复
  @ 消息（msg_id 5 分钟窗口内有效）
- 实现「群被动回复优先」：
  - qqbot_contacts 新增 last_event_id 列（迁移自动补齐）；
    网关把群 @ 事件 id 存入群条目（发言人私聊条目不存，避免
    错用群事件 id 回复私聊）
  - send_text 群目标：先查该群最近 @ 事件（5 分钟窗口），
    带 msg_id 被动回复；窗口已过或失败退化为主动消息
  - 主动发送仍失败且为 40034105 时，错误附指引：开通控制台
    群主动权限，或 @机器人后 5 分钟内发送（自动被动回复）
  - 错误码 40034105 翻译扩充；前端群模式提示被动回复机制
- 实测：群 openid 14EBF724… 已收集、入站群消息入库正常；
  被动回复路径待用户 @机器人 后 5 分钟内发送验证
- 回归：cargo test 237 passed；clippy 0；svelte-check 0/0；
  e2e 10 项断言全过

## 切片 J-22h（工具）：QQ 群被动回复零时机压力测试规则

- 动机：手动「先 @ 后发」容易错过 5 分钟窗口（实测 21:45 发送时
  距上次 @ 已超窗，回落主动消息再被 40034105 拒）
- 做法：直接在 automation_rules 建「QQ群测试应答」规则
  （.codex_tests/create_qq_group_test_rule.py）：
  - 条件：sender=群 openid 14EBF724…（钉死该群，不误伤微信）
    AND 内容包含「测试」
  - AI 提示词覆盖为固定 JSON 返回 fields.reply 文案 →
    qqbot_inbound 写回 to_reply → 应答器 2s 轮询取走 →
    send_qqbot_reply 被动回复（AI 几秒完成，远快于 5 分钟窗口）
- 用法：打开应用 → 群里 @机器人 说「测试」→ 自动收到
  「✅ ST 系统自动回复测试成功（QQ 群被动回复通道）」；
  验证整条 群消息 → 规则 → AI → 被动回复 流水线
- 群主动消息的永久解法仍是：开放平台控制台申请群主动权限；
  申请通过后发送台可随时主动发群消息

## 切片 J-23（裁剪）：消息通道收敛为 微信 + QQ官方

- 需求：移除 企业微信 / 钉钉 / QQ OneBot 三个通道，集中维护
  微信（iLink）与 QQ 官方机器人
- Rust 移除：
  - channels.rs：WecomConfig / DingtalkConfig / OnebotConfig
    及全部发送函数、URL 候选回退、签名、消息体构造、相关测试
  - manager：channel.rs（test_channel 仅留 qqbot、add 白名单
    仅 qqbot）、send.rs（send_text/send_media 仅 wechat+qqbot）、
    utils.rs（apply_onebot_override）、tests.rs（onebot 测试）
  - db.rs / channel.rs / loop.rs 注释与测试数据更新
- 前端移除：
  - types.ts：BotPlatform = 'wechat' | 'qqbot'，PLATFORM_META
    仅两平台
  - ChannelConfigDialog.svelte 重写为仅 qqbot 表单（AppID /
    Secret / 目标）
  - BotPanel.svelte：发送台推送目标块仅 qqbot，变量
    onebotTarget* 重命名 qqTarget*
  - PlatformOverview 文案更新
- 数据：现库无这三平台账号行，无需迁移删除
- 回归：cargo test 232 passed（-5 已移除测试）；clippy 0；
  svelte-check 0/0；smoke 全绿

## 切片 J-24（重构）：首页改为产品官网式落地页

- 需求：首页要「官网界面那种」，介绍项目作用，参考
  LifeArchiveProject/WeChatDataAnalysis 官网的编辑部叙事
- PlatformOverview.svelte 整体重写（保持 props 不变：
  statusText / statusCls / onNavigate，App.svelte 零改动）：
  - HERO：品牌行 + 金句大标题（渐变关键词）+ 项目介绍段落 +
    双 CTA（进入工作台 / 平滑滚动到功能全景）+ 元信息带
    （12 模块 · 双通道 · 0 字节出网）+ 右侧 LIVE STATUS 实时卡
  - 宣言：本地优先 / 一体化 / 真实可见 三卡（编号 01–03）
  - 功能全景：12 模块编号卡片（大号角标数字、hover 上浮、
    点击跳转对应面板）
  - 三步工作流：STEP 01 连接数据 / 02 注入智能 / 03 交给自动化
    （标签胶囊）
  - 运行状态：Agent 连接 + 最近事件（真实数据保留）
  - 收尾 CTA：隐私主张「把数据与智能，握回自己手里」+ 开始使用
- 视觉：编辑部排版（大写眉题、0.2em 字距、发丝线、角标数字），
  颜色全走主题 token（--brand/--card/--border/--foreground），
  深色/浅色主题自适应；响应式 1180/880 断点
- 验证：vision_glance 截图审查（上半/底部均无错乱、无溢出；
  doc scrollWidth == clientWidth）；svelte-check 0/0；
  smoke 全绿

## 切片 J-24b（增强）：首页动效升级（炫酷版）

- 需求：首页「还是没有炫酷的感觉」，加动态效果
- 动效（fancy-ui-svelte，包已通过 app.css 的
  @import "fancy-ui-svelte/tailwind.css" + @source "." 纳入
  Tailwind 编译）：
  - Hero：Meteors 流星雨（22 颗，品牌色增亮 + 渐变拖尾）+
    BorderBeam 边框环绕光束 + 两团 Aurora 极光光斑（CSS
    模糊渐变漂移）+
    标题逐词浮现 TextGenerateEffect（模糊渐显）+ 火花标题
    SparklesText（渐变流光动画 + 随机星芒）
  - 跑马灯：双行反向无限滚动（模块名 / 模块名·slogan），
    悬停暂停，两端 mask 渐隐
  - 功能卡：CardSpotlight 鼠标追踪光斑 + 悬停扫光
    （::after 平移）+ 编号角标发光
  - CTA：GradientButton 旋转渐变描边 + 外发光（hero 与页脚）
  - 状态卡 BorderBeam + 仪表 hover 辉光 + 空态接入引导
- 关键修复：CardSpotlight 外壳带 dark:text-white，而应用主题
  是独立于 OS 的（骨白仪表板），OS 深色时会出现白字浅卡——
  .po-card 显式 color: var(--foreground) 兜底
- 验证：vision 评审三轮迭代（对比度/干扰/层次），最终结论
  「高完成度，接近生产上线精致水平」；svelte-check 0/0；
  smoke 全绿

## 切片 J-25（重构·进行中）：全站 UI 重设计（目标 goal-a8c50682）

- 目标：自动巡检全部 13 个 tab + 14 个子视图 + 7 个微信子页
  （data/ui-audit/ 截图存档），按 5 个视觉审查子代理输出的
  ~140 条问题逐面板重设计
- **根因级修复（R1）**：
  - `.card` 是 App.svelte 作用域样式，子组件里 class="card"
    完全无效（首页功能卡无背景+CardSpotlight 硬编码浅灰底+
    深色主题浅字 = 白字白卡）。改为 :global(.card)，
    CardSpotlight 外壳用 :global(.po-spot) 中和
  - tabs-trigger 选中态全局增强（品牌色下划线 + 加粗）
- 面板级修复（R1 批次）：
  - 大模型：头部路径只显文件名（tooltip 全路径）；不限额度时
    隐藏进度条改文案；清空统计本有 confirm
  - AI 文案：主题 textarea rows 3→4
  - 自动化：统计卡 7 列→4 列（4+3）；SSE 状态徽章与操作按钮
    分组；消息流空态加图标+引导文案
  - 智能体：三栏 260/1fr/1fr → 250/自适应/460；已接入空态
    升级（图标+标题+说明）
  - AI 角色：计数徽章移到标题区；卡片网格 minmax(320,400)
    + 居中（单卡不再贴左）
  - 侧边栏底部 footer 间距加大；数据看板 KPI 8 列→4 列两行；
    OCR 资源列表去重复标题
- 巡检基建：ui-audit-all.mjs / ui-audit-sub.mjs /
  ui-audit-wechat-sub.mjs（CDP 截图输出到 data/ui-audit，
  避开 vite watch）；修复了 CDP 响应层级读取 bug
- 回归：svelte-check 0/0；smoke 全绿；vision 复检 3 面板通过
- 待办（后续轮次）：消息通道/微信数据、知识库、数据库、
  图文识别、数据看板细化，及子视图批次修复

## 切片 J-25b（R2 批次修复）

- OCR：统计条 8 项一行 → 4 列两行（grid + 每行首项去左框、
  第二行加顶框）；「资源列表」重复标题改「识别结果列表」
- 数据库：右侧空态图标增亮/品牌色、字号 13→14
- 数据看板：KPI 8 列 → 4 列两行；网络延迟 21.0ms → 21 ms；
  root 加 scrollbar-gutter + 底部 padding
- 消息通道：平台 Tab 选中态加品牌色下划线+加粗；账号卡
  「在线」标签与状态点去重（仅非 online 显示状态徽章）；
  发送台 textarea 5→4 行使发送按钮上移
- 全局：侧栏搜索快捷提示 ⌘K → Ctrl K（Windows 平台）
- 回归：svelte-check 0/0；vision 复检（OCR 4×2 ✓、
  数据库空态 ✓、看板两行 ✓）

## 切片 J-25c（R3 批次修复）

- 微信数据面板：
  - 顶部工具栏分组：数据操作（DB 状态/图片体检/刷新）｜竖线｜
    监控控制（MonitorControl），语义与视觉分离
  - 侧栏底部「设置」更名「微信配置」（与全局侧栏设置区分）
- 知识库：
  - 指标卡间距 gap 10→12；趋势图高度 320→260（视觉重心上移）
  - Y 轴刻度整数友好化（niceStep：消息量等整数指标不再出现
    0.25/0.75 小数刻度；$derived.by 修正）
- 回归：svelte-check 0/0；smoke 全绿；DOM 探针确认
  微信头部分组生效（divider/group 均在）

## 切片 J-25d（R4 批次修复）

- 微信子视图：
  - 朋友圈洞察统计卡：min-width 64→76、内边距与间距加大
  - 存储空间分类分布：进度条补充占比百分比
  - 撤回记录：类型构成/撤回最多 两组统计竖线分隔（原混排一行）
- OCR 资源列表：操作列 min-width 220px + nowrap，三按钮不再挤压
- 回归：svelte-check 0/0；smoke 全绿

## 切片 J-25e（R5 收尾）：全站最终复检

- 重新全站巡检 13 个 tab（data/ui-audit/ 覆盖为新状态）
- vision 最终复检（首页/大模型/自动化/图文识别）：「整体协调、
  风格统一、文字清晰可读，未发现严重问题」，仅剩首页卡片
  字号偏小等可接受项
- 全站 UI 重设计目标达成（goal-a8c50682）：
  - 根因修复 3 项（.card 作用域 / Tab 选中态 / 平台符号）
  - 面板级修复 ~30 项，覆盖全部 13 个面板及多个子视图
  - 门禁全程绿：svelte-check 0/0、smoke 46/46、
    vision 复检多轮通过

## 切片 J-26（重设计）：AI 角色界面（主界面 + 新建抽屉）

- 主界面（角色定位）：
  - 页头：计数徽章样式化（品牌色 pill，贴标题行）；搜索框
    加宽至 240px；刷新图标按钮与搜索框同高（36px）；操作区
    间距 12px
  - 卡片网格 minmax(320,400) → minmax(340,440)；卡片内边距
    16→18px；顶行 min-height 48px 统一基线
  - 底部操作：图标按钮 30→32px、组间距 4→8px；删除按钮
    hover 改警示红色（原品牌色）
  - 「使用此角色」按钮强化可点击感（更深填充+描边+发光+悬停
    上浮），消除「禁用感」
- 新建角色抽屉：
  - 按钮体系统一 min-height 38px
  - 「系统提示词预览」浮层改为默认收起，区块标题右侧新增
    「预览合成结果/收起预览」开关——不再常驻遮挡主内容
- 验证：vision 终检（主界面协调 ✓、按钮可点击感 ✓、
  预览默认不遮挡 ✓）；DOM 探针确认 popVisible=false；
  svelte-check 0/0；smoke 46/46

## 切片 J-26b（重设计）：新建角色抽屉（产品化）

- 审查发现并修复 7 类问题：
  - 提交按钮置灰无原因 → 页脚左侧提示「请先填写角色名称」
  - 图标预览与选择行分离 → 选择行归入「基本信息」并加
    「选择图标」标签；选中态品牌青 + 放大 1.1
  - 启用开关从抽屉底部上移到角色名称行内联（基础状态随
    基本信息设置），带「启用/停用」文字标签
  - 必填/可选标记统一：描述、行为约束&能力标签、路由偏好
    标注「可选」
  - 预览按钮从区块标题移入系统提示词文本域下方（作用于该
    字段，语义清晰）
  - chips 交互统一（回车或点添加均可）+ 空态示例提示
  - 折叠分区（采样参数/路由偏好）收起时显示当前值摘要
  - 打开抽屉自动聚焦角色名称（rp-name）
- 回归：svelte-check 0/0（清 4 处警告）；smoke 46/46；
  vision 终检通过

## 切片 J-27（原图迁移）：个微 C2C 原图代码迁移 + 本机严格实机验证

### 迁移内容（G:\wechat_image\01-current-unified-image-video-source → ST）
- 更新打包二进制 `src-tauri/resources/origin/`：
  - `wechat-cdn-poc.exe`（2,926,080 → 6,009,856 字节，01-current 新版，
    统一 image/video 命令集）
  - `wxcdn_origin_bridge.dll`（27,136 → 30,720 字节，随新版桥接）
- `origin_ilink/download.rs`：适配 01-current 的 `download-origin` CLI
  （`--db/--account/--source-id/--message-json/--wrapper/--bridge/--config/
  --allowlist/--work-dir/--output/--max-attempts/--max-output-bytes/
  --max-image-pixels`）；不再依赖静态打包 allowlist，改为运行期调
  `create-allowlist` 动态生成（wrapper_sha256/ilink2_sha256/bridge_sha256
  随实际 DLL 变化，兼容微信小版本升级）
- `origin_ilink/sandbox.rs`：沙箱准备新增 `netbridge/cdn/` CDN 状态复制
  （`cdninfo_new.cache` + `cdnmisc.cfg`，缺失会导致 CDN 域名解析失败）
- `origin_ilink/tests.rs`：补充 2026-08-16 E2E 验证记录注释

### 严格验证（本机 4.1.12.26，官方 ilink_wrapper.dll 全链路）
- 负样本（旧图，校验层正确拒绝）：local_id=9（hdlength 120365）→
  `status=unknown_failed, output_verification_failed, staging_absent`
  ——原图 CDN 链接已过期，大小/MD5 双校验与 staging 落盘校验按设计
  拒绝，未把坏图当成功
- 正样本（最新图片，全链路成功）：
  `Msg_5a8f5ec9ef550505c625c39c3e6d4c9b:2966`（create_time DESC 取最新，
  hdlength 874943，md5 5eb4eeb125563b8a56548e8cdd63e88c）→
  `{"status":"succeeded","bytes":874943,"md5_verified":true}`，
  输出 PNG 872×608 可正常解码 —— 字节数与 hdlength 完全一致、MD5 与
  消息 XML 一致，即微信原图本体
- 运行期 IPC：CDP invoke `get_ilink_origin_status` → enabled=true、
  wechat_version=4.1.12.26、wrapper=D:\Weixin\…\4.1.12.26\ilink_wrapper.dll、
  sandbox_ready=true、downloader=resources/origin/wechat-cdn-poc.exe
- 门禁：cargo build 成功（重新编译并重启应用后复核）；cargo fmt/
  clippy/test 通过

### 排障记录（已解决）
- hot-json `source_native_id` 出现双前缀 `Msg_Msg_…` → 修正为
  `Msg_<hash>:<local_id>` 后命令正常定位消息
- 旧消息原图链接过期（CDN 时效）→ 测试一律取 create_time 最新消息
- 沙箱残留旧二进制导致链路不通 → 替换为新版 exe/bridge 后通过
- `config.ini` 缺 kv_clientversion 时 client_version 取默认
  4065598490 兜底可用

## 切片 J-28（微信数据）：总览「朋友圈活跃」Top 3 → Top 15

- 数据总览仪表板「朋友圈活跃 Top 3」改为「朋友圈活跃 Top 15」：
  - `wechat/modules/moments.rs`：作者榜上限 `truncate(8)` → `truncate(15)`
    （洞察面板展示仍为 `slice(0, 5)`，不受影响）
  - `wechat/handlers/data/overview.rs`：总览取数 `.take(3)` → `.take(15)`
  - `DataOverview.svelte`：面板标题「Top 3」→「Top 15」
  - `types.ts`：`moments_authors` 注释同步
- 验证：CDP 实测 `get_wechat_data_overview` 返回 15 位作者（原 8 位
  上限被洞察模块截断，本次一并放开），UI 标题与 15 个作者 chip 均正确
- 门禁：cargo fmt/clippy 0、moments 测试 9 passed、svelte-check 0/0

## 切片 J-29（微信数据）：总览 Top 15 作者 chip 朋友圈跳转

- 「朋友圈活跃 Top 15」面板作者 chip 支持点击跳转：
  - `DataOverview.svelte`：chip 由 span 改为 button，新增可选 prop
    `onOpenAuthor`（未传入时保持不可点击）；面板加提示行
    「点击作者，跳转查看 TA 的朋友圈」；hover 品牌青描边+底色过渡
  - `WeChatPanel.svelte`：传入 `onOpenAuthor`，实现顺序为
    先 `setMomentAuthor(a)`（此时不在朋友圈页不触发加载）再
    `switchTab('moments')`——`refreshMomentsAuto` 同步读取作者，
    单次请求直达该好友的朋友圈，无二次重载/闪烁
- 验证（CDP E2E）：点击第一名 chip（王勤 72 条）→ 朋友圈页出现
  「正在看「东兰民中1410王勤」」过滤徽标，加载 18 条动态作者
  全部为目标作者，「返回全部」按钮可用；vision 复检提示行与
  15 个 chip 样式协调
- 门禁：svelte-check 0/0、wechat smoke 7 项全过

## 切片 J-30（微信朋友圈）：导出多格式 + 当前联系人过滤（缺陷修复）

- 修复「导出只能唯一格式」：
  - 后端 `export_moments_csv(path)` → `export_moments(format, path,
    author_username)`：支持 csv（默认，BOM）/ json（pretty，含
    likes/comments/images 全字段）/ txt（时间+作者+正文+位置+链接+
    媒体+点赞 可读块）
  - 前端朋友圈工具栏新增格式下拉（CSV/JSON/TXT），保存弹窗按格式
    给默认文件名与过滤器；命令/服务/注册（lib.rs、ipc.ts、
    WeChatPanel）同步更名，旧命令移除
- 修复「不能导出当前联系人朋友圈」：
  - `author_username` 直传 `get_moments_page` 后端过滤——正在看
    某位好友时，导出只含 TA 的动态；保存弹窗标题与默认文件名
    （moments_<作者名>_时间戳）随之变化；导出按钮 title 动态
    显示「导出「X」的朋友圈（当前筛选，格式 CSV）」
- 验证（CDP E2E，15 项全过）：作者过滤 JSON 导出 72 条全部属于
  王勤（与洞察发圈数一致）；全量 CSV 1306 行带 BOM 表头正确；
  TXT 72 块含作者名；UI 选择器 3 选项、过滤徽标、按钮 title 动态
  正确；vision 截图复检工具栏布局协调
- 门禁：cargo fmt/clippy 0、wechat 测试 112 passed、svelte-check
  0/0、smoke-ipc-contract 313 命令/147 调用全一致

## 切片 J-31（微信朋友圈）：HTML 导出 + 全量媒体资源落盘

- 朋友圈导出新增 **HTML** 格式（选择器 CSV/JSON/TXT/HTML）：
  - `export_moments` html 分支生成单文件报告（内联 CSS、明暗双主题、
    点赞/评论摘要、图片×N/视频标签），并把**全部图片/视频资源**
    下载解密到同级 `<html名>_media/` 目录后相对引用
  - 图片：原图 `/0` 优先（`url_token`，缩略图 `thumb/thumb_token`
    兜底），复用 sns_image ISAAC-64 解密管线 + 磁盘缓存
    （重复导出秒级命中）
  - 视频：`resolve_moment_video` 下载 + 头部 128KB 解密后复制 MP4；
    vweixinthumb 封面作 `<video poster>`
  - 单个资源失败不阻断整体导出，头部统计
    「媒体文件 N 个（M 个资源下载失败）」；结果 JSON 新增
    media / media_failed 字段，前端成功提示追加资源数
- 修复：HTML 资源引用路径 `media/…` → `<资源目录名>/…`（初版引用
  错误导致图片全部破图，vision 渲染复检发现并修正）
- 验证（CDP E2E）：王勤 72 条动态 → HTML + 173 个媒体文件
  （172 图 + 1 视频，2 个 CDN 过期失败正确计数）；图片魔数
  JPEG/PNG、视频 ftyp 抽检有效；HTML 条目数/引用数/资源目录
  标注全对；第二次导出 1 秒完成（磁盘缓存）；vision 渲染报告
  复检：真实图片正常显示、排版协调
- 门禁：cargo fmt/clippy 0、svelte-check 0/0、smoke-ipc-contract
  全一致、UI E2E 8 项全过（选择器 4 格式 + 过滤态 title）

## 切片 J-32（微信朋友圈 UI）：「近 12 个月发圈热度」组件重设计

- 旧样式为朴素矩形柱 + 月份标签；新设计（`WeChatPanel.svelte`）：
  - 标题行右侧新增元信息：完整日期范围（2025-09 ~ 2026-08）·
    12 个月合计条数 · 峰值月与条数
  - 每根柱顶显示当月条数（0 条隐藏，悬停显现），柱体为圆角胶囊 +
    低饱和主题色轨道，生长动画（scaleY 弹性缓动）
  - 峰值月高亮：数值/月份标签加粗主题色、柱体更亮渐变 + 内描边
  - 悬停：柱体增亮、数值转主题色；月份标签去掉前导零（08月 → 8月）
  - 原生 title 悬浮提示保留（完整月份 + 条数）
- 验证：svelte-check 0/0；CDP 实机截图 vision 复检——元信息
  数值与 12 柱求和一致（943 条）、峰值高亮正确、胶囊柱形与
  深色主题协调、无拥挤错位

## 切片 J-33（问我的微信）：问答准确性与意图识别修复

- 用 5 类真实问题实测 ask_wechat，定位并修复 5 个根因：
  1. **LLM 不知道「今天」**：规划/回答提示词注入当前日期，
     相对时间（上个月/最近7天）由 LLM 换算为具体日期；LLM 计划
     缺时间/目标时用启发式解析结果补齐（时间范围、显示名→username、
     聚合子任务 target/time/keyword/group_only 全字段合并）
  2. **显示名被当全文关键词**：LLM 的 target 是显示名时无法查消息库
     → 统一解析为 username；新增「目标会话最近消息」检索通道
     （get_conversation_messages 时间倒序，群聊带发送者前缀，
     通讯录解析会话显示名），覆盖「我和X最近聊了什么」类问题
  3. **会话标识关键词误杀内容**：把「王勤」这类人名从内容关键词里
     剔除（content_kws），消息/朋友圈的内容过滤只用纯内容词
  4. **朋友圈只查全局最新 300 条窗口**：指定作者时改为后端
     `get_moments_page(author_username)` 精确过滤，不受窗口限制
  5. **红包时间过滤失效**：redEnvelopeTable 无时间戳列，原来拿
     message_server_id 比 epoch 恒为 0；实测 send_id 内嵌日期
     （`1000039901+YYYYMMDD+…`，本机 65 条全部符合）→ 改
     `substr(send_id,11,8)` 过滤，统计摘要带日期范围
  6. **自评引用序号 1 基/0 基错位**：提示词按 [1][2] 编号而过滤用
     0 基下标，最新证据永远被删 → 转 0 基；且头 3 条最新证据
     始终保留，防自评误删
- 实测前后对比（同一批问题）：上个月聊最多 ✓（7月范围 + 5309 条）、
  王勤最近聊了什么 ✓（真实消息 + 显示名引用）、最近7天群聊 ✓
  （真实时间范围 + 内容摘要）、去年红包 ✓（2025 年 0 个，范围正确）、
  王勤朋友圈 ✓（含最新 08-06 动态）
- 门禁：cargo fmt/clippy 0、ask 测试 9 passed、wechat 全量测试通过

## 切片 J-34（问我的微信）：排行榜问法变体 + 统计索引新鲜度

- 实测「我最近和谁聊的最多？」首答错误（"无法确认，仅有与 a憨 的
  一条记录"），两个根因：
  1. 启发式排行触发词漏「聊的最多」（的/得混用）等口语变体 →
     未触发 top_sessions 聚合；「谁聊/最」碎片被当内容关键词，
     检索出一条噪声消息污染答案
  2. 统计走搜索索引（message_meta），索引是 2026-08-06 的手工快照、
     已过期 10 天 → 计数低估 36%、#4/#5 排行错位
- 修复：
  - 排行触发词扩为「聊得/聊的最多、聊最多、联系最多、最频繁、
    最常聊、消息最多…」+「和谁聊/跟谁聊/和哪个+最多」组合
  - 裸「最近/近期」（无天数）默认近 30 天时间范围
  - top_sessions / message_trend 聚合清空 plan.keywords，不再产生
    噪声引用
  - 统计索引新鲜度门禁：message_meta MAX(create_time) 距今超 12 小时
    即判定过期，统计改走分库直聚合（永远最新，同口径只计
    local_type=1 文本消息）
- 独立核验（直接读解密分库 Msg_ 表逐会话计数）：
  - 修复后答案「黑龙江沃融-燎引擎 4794 条」与独立核算**逐项一致**
    （Top10 中 9 项数字完全相同，仅南宁房东群差 35 条为窗口边界
    定义差异：滚动30天 vs 日历月）
  - 全消息类型口径下 TOP1 同为黑龙江沃融-燎引擎（7826），
    结论稳健
- 门禁：cargo fmt/clippy 0、ask 测试 10 passed（新增排行变体 +
  裸最近回归测试）

## 切片 J-35（AI 聊天 UI）：全局对话界面重设计（ChatGPT/Claude 风格）

- `llm/components/GlobalChatTab.svelte` 对话区整体重设计：
  - 布局：消息流去掉卡片外框，改为**居中窄栏**（max-width 800px），
    与主流 AI 聊天界面一致
  - 消息样式：助手消息 = 圆形渐变头像 + 模型名 + 无气泡正文
    （悬停显示「播报/复制」操作行）；用户消息 = 右侧圆角气泡
  - 流式回复：正文尾部闪烁光标（llm-caret）
  - 空态首屏：居中渐变 Logo + 「有什么可以帮你？」+ 4 个推荐问题
    卡片（点击即填入发送）
  - 输入区：悬浮圆角卡片（focus 品牌描边 + 阴影）、无边框自适应
    高度 textarea（1~8 行自动增高）、纸夹图标按钮、**圆形发送按钮**
    （生成类模型保留带文字按钮）、脚注「AI 生成内容仅供参考 ·
    Enter 发送 / Shift+Enter 换行」+ 当前模型名
  - 复制功能新增（剪贴板 API 失败时回退 execCommand，适配 WebView2
    权限策略）
- 验证：svelte-check 0/0；CDP E2E——空态 hero 4 推荐卡、真实流式
  对话（DeepSeek 回复）、用户/助手消息结构、复制「已复制」反馈
  全过；vision 双截图复检「高度类似 ChatGPT/Claude，协调美观」
- 门禁：llm 相关 smoke 6 项全过

## 切片 J-36（AI 语音对话）：拟人化优化（语速/音色/自然度）

- 背景：本机此前无 TTS 模型 → 语音回复回退 Windows SAPI 机械音；
  本地 Whisper 配置指向已迁移的旧路径（model_exists=false）
- 语音合成升级（像真人说话）：
  - **接入 CosyVoice2 自然语音**：实测硅基流动 OpenAI 兼容
    /audio/speech 可用，向提供方新增模型
    `FunAudioLLM/CosyVoice2-0.5B`（model_type=语音）；实测 6 个
    可用音色（anna/bella/alex/benjamin/charles/david），UI 音色
    列表改为「名字 · 性别 · 气质」中文标注
  - **语速 speed 全链路**：SpeechRequest/client/handler 增加
    speed（0.5~2.0 clamp）；语音条与 TTS 输入行新增语速选择
    （0.75x 舒缓 / 0.9x 稍慢 / 1.0x 正常 / 1.15x 稍快 / 1.3x 快速），
    偏好持久化 + 历史值归一化（"1" → "1.0"）
  - **SAPI 兜底带语速**：synthesize_native_speech 增加 rate
    （-10~10），默认 -2 自然语速；实测 slow(-4) 音频时长 ≈
    fast(+4) 的 2.4 倍
  - **朗读去 Markdown**：speakText 先 plainTextForSpeech，
    不再读出 ** 加粗 **、# 标题、代码块等符号
- 本地转写修复：下载 whisper Base 模型（148MB，自动写入配置并
  加载）；转写引擎显示「本地 Whisper + 云端转写」（云端
  TeleSpeechASR 仍在）
- 验证（CDP E2E 全过）：anna 1.0x / alex 1.3x 真实合成成功且
  音频长度不同；SAPI rate 生效；UI 音色 6 项、语速 5 档、元信息
  「…· 语速：1.0x」；vision 复检语音条布局协调
- 门禁：cargo fmt/clippy 0、llm 测试 11 passed、svelte-check 0/0、
  voice.test 通过

## 切片 J-37（AI 聊天 UI）：语音/普通聊天界面统一（去组件化）

- 语音对话不再使用堆叠在输入框上方的整块「语音条」
  （麦克风+状态+2 复选框+2 下拉+停止按钮 ≈ 8 个组件）：
  - 输入行新增**麦克风圆形按钮**（与附件按钮并列），一键三态：
    未开启 → 开启语音；已开启 → 开始/停止录音（录音红底脉冲）
  - 开启语音后仅在输入框上方多**一条胶囊细状态行**：状态文字 +
    「语音回复」「连续对话」胶囊开关 + 齿轮 + ×（退出）；
    播报中才出现「停止播报」
  - **音色/语速/引擎信息**收进齿轮弹出的**小设置浮层**
    （右下角卡片，音色 6 项 + 语速 5 档 + 转写/播报引擎）
  - 工具栏移除「语音对话」按钮（麦克风即入口），普通聊天与
    语音聊天的界面差异收敛为一条细状态行
- 验证：svelte-check 0/0；CDP E2E——普通态输入行仅 附件+麦克风
  2 图标、开启后细状态行 + chips、设置浮层含音色/语速/引擎、
  退出后无残留；vision 复检「与普通聊天相比只多一条细状态行，
  简洁不拥挤」
- 门禁：voice.test 通过、相关 smoke 全过

## 切片 J-38（AI 语音对话）：修复思考过程被朗读 + 配置容灾

- 问题：语音回复把模型的内部思考过程（"我们需要理解当前状态…"）
  整段读出来，答案里也混入了思考内容
- 根因：`chat_completion_stream` 把推理模型的 `reasoning_content`
  增量当作正文流向 on_delta → 前端展示并进入语音播报
- 修复：
  - 流式解析把 content 与 reasoning_content 严格分离（抽出纯函数
    parse_stream_delta 并加单测）：思考内容只收集不展示，仅当模型
    完全没有正文输出时才回退使用（保留原兜底意图）
  - 前端 TTS 前 plainTextForSpeech 增加剥离 `<think>…</think>` /
    【思考】…【/思考】块（防御内容内嵌思考的模型）
  - 实测验证：原始 SSE 确认提供方 reasoning 与 content 分开发送；
    直连 chatStream delta 纯净；UI 清空历史后发送「你好」→
    回复「你好！有什么我可以帮你的吗？」无思考内容
- 附带修复：排查中发现 llm_config.json 曾被强杀进程截断成空文件，
  load_config 静默返回空配置导致后续保存把提供方配置（含 API Key）
  整个覆盖丢失：
  - save_config 改为**原子写**（.tmp + rename，强杀不再产生截断文件）
  - load_config 遇空文件/解析失败时**自动从 .bak 恢复并写回**（自愈）
  - 已从 .bak 恢复本机配置（2 提供方 4 模型，含 CosyVoice2），
    并清空了被污染污染的对话历史
- 门禁：cargo fmt/clippy 0、llm 测试 12 passed（新增 reasoning
  分离单测）、svelte-check 0/0、voice.test 通过

## 切片 J-39（AI 聊天）：富文本渲染补全 + 上下文连续管理

- 富文本渲染补全（此前有上下文的长回复会出现「变形」）：
  - `messageRender.miniMarkdown` 新增 **表格**（| 行 + |---| 分隔，
    溢出横向滚动包裹）、**引用块**（> 连续行合并，左竖线+底色）、
    **分割线**（--- / *** / ___）
  - `inlineMd` 行内代码改为**先提取保护再套用粗体/斜体**，
    修复 \`**x**\` 被粗体规则误伤渲染成 <b>x</b>
  - MessageBody 补表格/引用/分割线样式（表头底色、单元格边框、
    宽表不撑破消息栏）
  - 实测：植入富文本对话（表格/引用/代码/嵌套列表/长文），
    DOM 校验 1 表格 3 表头 + 2 引用 + 1 分割线，vision 复检
    全部正常渲染、无溢出错位
- 上下文连续管理（不让 AI 脱离当前窗口主题）：
  - `trimContext` 重写：**首条用户消息作为主题锚点**始终保留；
    裁剪**按完整轮次**（user+assistant 一问一答）整轮丢弃，
    不拆散对话；预算 40 条 / 120K 字符，最少保留 6 条
  - 返回 `{ messages, trimmed }`；被裁剪时注入系统说明
    「更早的对话已省略，请紧扣当前主题继续，不要编造被省略内容」，
    防止模型回复无关信息
  - smoke-chat-context 重写为 11 项断言（锚点/整轮边界/条数与
    字符预算/最小保留/空历史），smoke-message-render 22 项断言
- 门禁：svelte-check 0/0、smoke-message-render 22 项、
  smoke-chat-context 11 项、voice.test 通过

## 切片 J-40（AI 聊天 UI）：修复底部工具栏被遮挡

- 问题：`.llm-chat { height:100% }` 与父容器里的 PanelHeader 叠加，
  整块高度超出可视区域 → 底部工具栏（提供方/模型下拉 + 按钮）
  被 `overflow:hidden` 裁掉、完全不可见
- 修复：
  - `.llm-chat` 改 `flex:1; min-height:0`（占头部之外的剩余空间）
  - `.llm-chat-window` `min-height:240px` → `min-height:0`，
    消息区改为内部滚动而不是撑破布局
- 验证：DOM 实测工具栏 rect 完整落在容器内；vision 截图复检
  提供方/模型下拉、「对话」标签、AI 角色/清空对话按钮全部可见
  无遮挡；1280×680 小窗口模拟下工具栏仍完整可见、消息区
  收缩为内部滚动
- 门禁：svelte-check 0/0

## 切片 K-1（DSH 迁移①）：AI 聊天代理模式 — 工具调用 + 审批流

- 目标：把 DeepSeek Harness 对话能力迁移到 ST「AI 聊天」
  （goal-133d6918，多轮目标第一轮）
- 后端（新增 `llm/agent.rs` + client 工具支持）：
  - `client/chat.rs`：`chat_completion_with_tools_raw`（OpenAI tools /
    tool_calls 解析）；`CompletionParams` 增加 tools/tool_choice
  - 工具注册表 + 5 个内置工具：web_search（Bing cn/www 双域兜底，
    解析 b_algo 标题/链接/摘要，实测国内可达）、read_file /
    write_file / list_dir（agent_workspace 沙箱，路径逃逸防护 +
    单测）、exec_command（🔒 审批门控）
  - `chat_agent_stream`：模型调用 → tool_calls → 执行 → 结果回传
    → 循环（最多 6 轮）→ 最终回答；Channel 事件
    tool_start/tool_done/delta/done/error
  - 审批流：pending 存储 + approve/reject IPC + 10 分钟超时 +
    `agent-approval-requested` 事件
- 前端（GlobalChatTab）：输入脚注「代理」开关（偏好持久化）；
  输入框上方**工具步骤面板**（工具名/参数/执行状态/结果摘要）；
  **审批卡片**（批准/拒绝）；代理流事件驱动消息填充与持久化
- 验证：cargo 238 passed（新增 4 个 agent 单测：沙箱逃逸拒绝、
  工具目录、文件往返、Bing 解析）；svelte-check 0/0；
  smoke-ipc-contract 317 命令全一致；CDP E2E ALL_PASS——
  「搜索南宁天气」真实调用 web_search 返回中国天气网等 8 条结果、
  exec_command 弹出审批卡 → 批准 → 输出 hello-agent；
  vision 复检工具面板/代理开关布局协调
- 后续（目标下一轮）：动态插件系统（define/run/update/stop/
  undefine + 插件注册工具 + 插件管理面板）

## 切片 K-2（DSH 迁移②）：动态插件系统 — 定义/运行/版本/审批全生命周期

- DSH 插件模型的 ST 适配（`llm/agent_plugins.rs`）：
  - 插件 = 持久化记录（data/plugins/plugins.json 原子写）：
    id/名称/描述/启用态/工具列表（name/description/parameters/
    requires_approval/code）+ **不可变版本历史**（每次保存追加
    version+1，旧版本保留）
  - 生命周期 IPC：list / save（新建或更新→新版本）/ delete
    （undefine）/ set_enabled（stop/start）
  - 插件工具实现为 JavaScript，**前端 WebView 执行**（与 DSH
    Client 插件同信任级别）：后端代理循环遇插件工具时发出
    `agent-tool-exec-request` 事件并等待，前端 `new Function`
    沙箱化函数体（ctx.fetch/ctx.log）执行后经
    submit_agent_tool_result 回传（60 秒超时）
  - 插件工具的 requires_approval 复用代理审批流；工具目录
    （get_agent_tools）与 tools_json 动态合并已启用插件工具
- 前端（GlobalChatTab）：
  - 工具栏新增「插件」按钮 → **插件管理抽屉**：列表卡片
    （名称/v 版本/运行中|已停止/工具名🔒/运行·编辑·删除）+
    新建/编辑表单（名称/描述/工具名/工具描述/审批开关/
    JS 代码编辑器），保存即产生新版本
  - 插件工具执行桥监听 + 结果回传（含日志捕获）
- 验证（CDP E2E ALL_PASS）：
  - 创建「计算器插件」v1 → 更新 v2（versions=[1,2]）→ 工具目录
    含 calculator → 代理模式问「计算 123*456」→ 模型调用插件
    工具 → 前端执行桥 → 正确回答 **56088**
  - 停止插件 → 工具目录移除；删除 → 列表为空
  - 抽屉 UI：标题/新建按钮/6 字段表单（含代码编辑器与审批开关）
    vision 复检布局协调
- 门禁：cargo 240 passed（新增插件版本历史/工具查找单测）、
  clippy 0、svelte-check 0/0、smoke-ipc-contract 322 命令全一致、
  smoke 全过

## 切片 K-3（DSH 迁移③）：AI 聊天工具系统完善 v2

- 目标：完善代理工具系统五方向（goal-71029c4b）：
  ① 工具步骤详情面板 ② 更多内置工具 ③ 工具调用历史持久化
  ④ 审批与安全增强 ⑤ 插件工具增强
- 后端（`llm/agent.rs` / `llm/handlers/history.rs` / `db.rs`）：
  - 新增 3 个内置工具（合计 8 个）：`get_current_time`（本地时间含
    时区）、`search_knowledge_base`（BM25/FTS 只读检索，当前登录
    用户或默认 admin 可见库，分片截断 400 字符）、
    `fetch_web_page`（http/https 抓取去标签正文，截断 8KB）
  - `ToolFn` 签名扩展为 `fn(Option<AppHandle>, Value)`：工具可访问
    应用状态（知识库 DB/会话），测试传 None
  - `exec_command` 超时管控：输出重定向临时文件防管道死锁，30 秒
    轮询超时强制 kill 并返回已产生输出
  - `tool_done` 事件携带 `duration_ms`；插件工具同样计时
  - 审批增强：审批事件携带完整 `arguments`；新增「会话内记住批准」
    （trust_agent_tool / clear_agent_trust，信任键
    (provider, model, tool) + 30 分钟 TTL，request_approval 轮询
    中动态放行；清空对话清除信任）
  - **工具名去重**：tools_json / get_agent_tools 插件工具优先、
    同名内置被遮蔽、插件间先注册生效——修复重复插件工具名导致
    上游 API 400「Tool names must be unique」使代理整体不可用
  - 工具调用历史持久化：新表 `llm_agent_tool_steps`
    (provider, model, assistant_idx) UNIQUE + upsert/查询/随清空删除；
    IPC save/get_agent_tool_steps（步骤 JSON 由前端序列化，
    按助手消息序号关联）
- 前端（GlobalChatTab / types.ts / ipc.ts）：
  - **工具步骤详情面板**：点击步骤头展开参数/结果区块（JSON 缩进
    美化、pre 滚动、复制按钮、再点收起）；状态徽标（执行中/完成/
    失败）+ 执行耗时徽标 + 「已重试」标记
  - **历史工具调用**：面板按消息渲染——当前回合实时面板位于最后
    一条 AI 回复之前（思考位置，62px 对齐）；历史回合在带
    tool_steps 的助手消息上方显示「历史 · N 步」面板；重载会话/
    重启后仍可见并可展开
  - **审批卡增强**：完整命令 code 展示 + 复制 + 三按钮
    （记住并批准/批准/拒绝）；重载历史时重置实时步骤避免残留面板
  - **插件失败重试**：失败插件步骤提供「重试（仅本地查看）」——
    实时拉取插件代码本地重跑并更新步骤（标注已重试，不回传模型）
- 验证（CDP E2E `e2e-agent-tools-v2.mjs` ALL_PASS，24 项断言）：
  新工具目录 / 详情展开含 12*34 参数与 408 结果 / 耗时徽标 310ms /
  步骤落盘 + 切模型重载后历史面板仍显示 / 审批卡完整命令 + 记住并
  批准后第二次 exec 不再弹窗 / flaky_probe 首次失败 → 重试转 ok；
  `verify-panel-pos`（面板位置）、`e2e-agent-mode`（web_search +
  exec 审批）、`e2e-agent-plugins`（插件全生命周期）回归 ALL_PASS
- 门禁：cargo 244 passed（新增时间工具/echo/知识库上下文/工具名
  去重 4 单测）、clippy 0、cargo fmt clean、svelte-check 0/0、
  smoke-ipc-contract 326 命令全一致、smoke 全过；vision 复检详情
  面板/耗时/已重试标记布局协调














