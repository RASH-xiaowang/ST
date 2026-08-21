# DeepSeek Harness 前端（packages/client）待迁移功能精确清单

> 用途：st_control 把 DSH 前端完整迁移到 Svelte5 + Rust 时的功能验收对照。
> 范围：`E:\ST\deepseek-harness-master\packages\client\` 下 20+ 个 `ui-*` 包，只读精读源码产出。
> 契约来源：字段名/枚举值/像素/断点/默认值/中文文案均取自源码原文。
> 架构约定（迁移背景）：DSH 前端是 React+TS，业务组件为纯 props 展示层，数据经「四份 props 共享」（PropsRuntime / PropsRenderSlots / PropsStore / inject face）注入；UI 组合只通过 `ctx.slots.register({name,children,store,inject}, Component)`。Svelte5 迁移时这些 slot/注入机制需按 Rust+Svelte 自行落地，本文只负责「渲染内容 / 交互 / 字段」三件事的精确事实。

---

## 1. ui-conversation（chat 域）

### 组件/节点（渲染内容；交互；关键字段）

- **ChatView** — 会话默认对话视图：稳定 key 的父级列表按 `order` 遍历交给 `ChatNodeSeat`，顶部「加载更早」、历史加载中/失败提示、运行中 turn 信号 `TurnStatus`、`PendingSteeringBubble` 列表、右下「回到底部」。交互：`loadOlderAnchored()` 先记分页锚点再 loadOlder、prepend 后恢复锚行；滚动跟随阈值 `FOLLOW_THRESHOLD=24`；滚动记忆 `chatScroll.save/read`(anchorKey/anchorTop/scrollTop)；ResizeObserver 监听列/composer；「回到底部」仅 `!atBottom`。字段：`ChatViewSlotProps`(useSession/useSessions/useStore/renderSlot/sessionId/openFile/loadOlder/loadImage/inspectCall/chatScroll/forkAt/fileMentions/t)；数据源 `s.chat.order/nodes/timeline`、`s.queue`、`s.running/openState/openError/hasMore/loadingOlder`、`useStore(s=>s.selection?.callId)`。
- **TurnStatus**（私有）— turn 级活动标签，硬编码 `Deep diving...`，运行 ≥15s 追加 `formatRunDuration` 时钟。无交互。
- **ChatNodeSeat** — 订阅单 node key，按 `routedNode.kind` 分派给 `conversation.chat.node` 插槽；未注册 kind 回退 `JsonBlock`(message.unknownSurface)。DOM 带 `data-chat-anchor-key/flow-key/flow-kind`。
- **UserStyleBubble**（私有）— 右对齐气泡（user/steering 共用）：`contentParts` 拆成 text/images/rest；图片用 `ImageGallery`(align=end)；`showBubble = text!==''||rest.length>0`；`/name`/`@name` 渲染为 refChip(`data-ref-chip='subagent'|'skill'`)。交互：`actions?(text)` 注入。字段：`content`、`imageLoader`、`pending?`、`actions?`。
- **PendingSteeringBubble** — Host 权威的待入账 steering 气泡；仅复制（clock=start 无 time）。字段：`content`、`loadImage?`、`t`。
- **UserMessageNodeView**（memo）— `ChatNodeViewProps<'user'|'steering'>` → UserStyleBubble + time + 复制按钮(clock=start)。字段：`node.data.content/time`。
- **ContextMessageNodeView**（memo）— `'context'` → `ContextInjectionRow`(content/source/provenance/form)，展开/折叠委托。
- **CompactionNodeView**（memo）— `'compaction'` → `CompactionItem node={data}`。
- **RetryNodeView / ModelRetryItem** — `<details>` 披露：summary=`{label}（{retry}/{maximum}）· {seconds}s`，展开显示 delay(delayMs)、failure(failure.message)；倒计时 250ms 秒级更新。字段：`ModelRetryNode`(delayMs/mode('normal'|→'∞')/maxRetries/retry/retryState('scheduled'|'started'|'cancelled')/failure.message)。
- **TurnErrorNodeView / TurnErrorItem** — StateDot error + `message.turnError` + node.message + 可选 code；`role=status`。字段：`node.message/node.code?`。
- **TurnMaxTokensNodeView** — StateDot warning + `message.maxTokens` + `.hint`。无字段。
- **UnknownNodeView** — `JsonBlock` label=message.unknownSurface({type})，payload=`data.data`。
- **AssistantNodeView** — `'assistant-step'` → `AssistantMarkdown`(blocks, streaming=status==='running', interrupted=status==='interrupted')；turn 关闭且 finalNode.seq 与 turn-tail.closing.finalNode.seq 一致时解析 `fileMentions`。字段：`node.data.status/ blocks/finalNode`。
- **AssistantMarkdown** — 按序渲染 blocks：`text`→MarkdownText(带 fileMentions)；`reasoning`→ReasoningRow(running=streaming&&i===last)；`image`→连续块合并为单个 ImageGallery(align=start)；`tool-call`→跳过；其它→JsonBlock(unknownBlock)；`interrupted` 时末尾追加「已停止」。`hasVisible` 否则 null。字段：`blocks/streaming/interrupted?/loadImage?/mentions?/t`。
- **ReasoningRow** — Think 折叠披露：icon `IconThinkOutline14` + 硬编码标题 `Think`；collapsed 摘要=running?latestLine:firstLine(text)；running 时摘要横向滚动到行尾(3 帧节流)+隐藏态「运行中」。字段：`text/running/t`；`data-variant="think"`、`data-state='running'|'ok'`。
- **TurnTailNodeView** — 已完成 turn 页脚：`renderSlotChain('conversation.chat.turnTail')` 特性链 + MessageIconActions(复制+分支+时钟/指标) + `renderSlot('conversation.chat.assistant-actions',{messageId})`。复制=`assistantText(closing.blocks)`；分支=`forkAt(closing.finalNode.seq)`，`branchUnavailable=data.branchUnavailable||hasLaterChatNode`。字段：`TurnTailChatData`(turn/seq/time/closing/branchUnavailable/ttftMs?/tokensPerSecond?)；`runMs=turn.end.time-turn.start.time`。
- **CommandNodeView** — `'command'` → `renderSlot('conversation.chat.commandview', owner, {entryKey: command.name??'', fallback:<GenericCommandCard/>})`。字段：`CommandRowOwnerProps={node, compaction?}`。
- **ManualCompactionNodeView** — `'manual-compaction'` → `CompactionCommandCard`。字段：`ManualCompactionChatData{command, compaction|null}`。
- **CompactionCommandCard** — `/compact` 生命周期三分支：compaction!==undefined→CompactionItem(title=compact)；compaction===undefined&&outcome!==null→GenericCommandCard；running→GenericCommandCard(runningSummary=「正在压缩…」)。
- **CompactionItem** — 落地压缩标记行（默认折叠）：context 图标+展开箭头+标题+摘要；展开显示 `MarkdownText(summary)`；`expandable=summary!==null`。摘要三态：两项计数非空→「已压缩 {items} 条历史记录（约 {tokens} tokens）」；否则 fallbackSummary。字段：`CompactionSummaryNode`(summary/shadowedItemCount/shadowedTokenCount)。
- **GenericCommandCard** — 通用命令行：DisclosureRow，leading=error?StateDot error:IconApiOutline14，标题=`node.name??command.title`，collapsed 摘要=running?「执行中…」:outcome?.text??(kind==='error'?「命令失败」:「已完成」)；`body = text.includes('\n')?text:null`。字段：`CommandRowState='running'|'ok'|'error'`。
- **ContextInjectionRow** — 非用户消息折叠披露：icon IconBrowseOutline16，标题=`provenance.role==='recall'?「跨会话召回」:「上下文注入」`；collapsed=provenance.label + 可选 summary；展开=`contextBody(form,...)`。字段：`content/source/provenance/form/t`。
- **MessageIconActions** — 共享 IconActions 行：可选时钟+复制+extraActions+可选分支。复制成功 1s 内换成对勾；分支仅 onBranch 提供时渲染，branchUnavailable 时 aria-disabled+「仅可从已完成轮次的最后一条消息分支」；时钟 clock='start'(图标前)/'end'(图标后)，追加「· 用时 {duration}」「· 首 token {seconds}秒」「· {tps} tok/s」。字段：`text/time?/runMs?/ttftMs?/tokensPerSecond?/clock/onBranch?/branchUnavailable?/extraActions?/t`。

### 消息操作按钮全集

| 按钮 | 位置 | 动作 | 可用性 |
|---|---|---|---|
| 复制 | UserMessageNodeView(user/steering) | writeClipboard(拼接 text 块) | 始终 |
| 复制 | PendingSteeringBubble | 同上 | 始终 |
| 复制 | TurnTailNodeView | writeClipboard(assistantText(closing.blocks)) | 始终；closing===null 时无 |
| 分支(fork) | **仅** TurnTailNodeView | forkAt(closing.finalNode.seq) | onBranch 存在且 !branchUnavailable |
| extraActions | TurnTailNodeView 内 assistant-actions 插槽 | 第三方注册 | closing.finalNode.messageId 存在 |
| 重试(retry) | ❌ 无用户可触发按钮 | retry 仅为 model-retry 节点被动披露 | — |

### chat 节点定义（12 个 Definition + 1 视图构建器，match() 触发条件）

统一：`match(event)` 只读当前事件，返回 `{id, role:'start'|'update'}`；`buildViewNode` 产 `ChatNode<kind>`。

| Definition(kind) | match() 触发 | 渲染 |
|---|---|---|
| assistant(`assistant-step`) | `step/start` start(id `turn:step`)；`assistant/chunk` update；`assistant/message`(isAppendSurfaceEvent) update；`llm/retry` update | AssistantNodeView→AssistantMarkdown |
| command(`command`/`manual-compaction`) | `command/run` start(id commandId)；`command/done` update；`compactSource`(user/message+replacement+plugin='compact'+sourceCommandId) update；`compaction/start|summary|end`(sourceCommandId 非空) update | name==='compact'→manual-compaction，否则 command |
| compaction(`compaction`) | `compactSource`(sourceCommandId===undefined) update；`compaction/start` start、`summary|end` update | CompactionNodeView→CompactionItem |
| fallback(`unknown-surface`) | isAppendSurfaceEvent → start(id seq) | UnknownNodeView→JsonBlock |
| inbox(`inbox-next-turn`/`inbox-next-step`, 2 个) | `agent/inbox/spliced`(data.target===target) start(id seq) | 无视图节点(publication:'none')，产 InboxState 供 message 判定 |
| message(`input-message`) | `user/message`+append+!isCompactionCheckpoint start(id data.id) | source.kind!=='user'→context；user 且在 inbox.claimed→steering；否则 user |
| retry(`model-retry`) | `llm/retry`(retryId 非空；retry===1 start 否则 update)；`llm/retry-started` update | RetryNodeView→ModelRetryItem |
| tool(`tool-call`) | `tool/call` start(id callId)；`tool/result`(append) update(id message.source.callId)；`tool/code-dispatch-start`/`tool/code-dispatch` update | **渲染器在 ui-tool**；本包产 ToolChatData{root}(递归 subCalls, MAX_DEPTH=256) |
| turn-error(`turn-error`) | `turn/start` start；`turn/end`(reason.kind==='error') update；`llm/retry*` update(隐藏) | TurnErrorNodeView；`hidden`(turn 拥有 retry 链)时 visibility:'hidden' |
| turn-max-tokens(`turn-max-tokens`) | `turn/end`(reason.kind==='max-tokens') start(id turn) | TurnMaxTokensNodeView；锚点 noticeAnchor=closing.seq+0.05 或 turn/end seq |
| turn-tail(`turn-tail`) | `turn/start` start；`turn/end` update；`tool/call|result` update；`assistant/message|chunk`、`step/end`、`llm/retry` update | TurnTailNodeView；另发布为 `turn-tail` 位置数据 |

`CHAT_SYNTHETIC_SEQ_OFFSETS`：interruptedAssistant -0.9 / interruptedFollowup -0.8 / maxTokensNotice 0.05 / finalizedFollowup 0.1。

### ContextBody（上下文渲染体，8 项）

分发器 `contextBody(form, props)`，返回 `{rendered, summary, body}`；表单不可读回退 OpaqueBody。`KnownContextForm = instructions|catalog|snapshot|notice|relay|recall|null`：
- OpaqueBody（默认/null/未知）：ModelFacingContent(`<pre>` 保留换行 + 未知块 JsonBlock) + SourceFields。
- InstructionsBody：`instructionChanges(source)` → 文件 `<ul>`(path + 动作词 已载入/已新增/已更新/已移除)。
- CatalogBody：`catalogEntries(source)` → 条目 `<ul>`(MAX_ENTRIES=200，超出「…还有 {count} 条」)。
- SnapshotBody：`snapshotSections(source)` → `<dl>` 分区 + 「取代先前的快照」。
- NoticeBody：`noticeSummary(source)` → 仅 ModelFacingContent。
- RelayBody：`relaySender(source)` → 「来自会话 {session}」+ content。
- RecallBody：`recalledSessions(source)` → 召回 `<ul>`(label + 「保留 {retained} 条 · 省略 {omitted} 条」+ 可选「已截断」)。
常量：`MAX_CHARS=20_000`、`MAX_ENTRIES=200`。

### 辅助纯函数（关键 8 项）

`message-chrome.ts`(formatRunDuration/formatLatencySeconds/formatTokensPerSecond/formatMessageClock)、`turn-assistant.ts`(assistantText 拼 text 块)、`turn-metrics.ts`(deriveTurnMetrics：TTFT 取最小 step 首 token，tps=Σoutput/(ΣdecodeMs/1000))、`tool-node-reader.ts`(rootToolCall/findToolCall)、`image-labels.ts`(messageImageLabels)、`use-calendar-day.ts`、`use-throttled-visual-update.ts`(rAF 3 帧节流)、`common.ts`(chatNode/coordinate)。

### StatsLine（注册于 conversation.composer.dock id stats）

`|` 分隔分组（无数据整组消失，过长省略号+tooltip）：counts({turns}轮·{steps}步)/llm/toolCall/ttftAverage/tokensPerSecond/cacheHit/tokens。`formatTokens`(517/12.2K/1.2M)、`cacheHitPercent = cacheRead/(uncached+cacheRead+cacheWrite)`、`contextOccupancy = min(100, round(usedTokens/contextWindow*100))`。

### EnterBehaviorRow（settings.general.item id composer-enter）

选项：`queue`「排队发送」/`steer`「插话发送」；默认 `DEFAULT_BUSY_ENTER_BEHAVIOR='queue'`；存储 ns=`ui-conversation` 字段 `busyEnter`(schema default 'queue')。解析：`!running||!steeringAvailable→'queue'`；否则 plain Enter→preferred，Cmd/Ctrl+Enter→相反。

### 文案要点（chat 中文）

载入历史… / 历史加载失败 / 加载更早 / 回到底部；上下文注入 / 跨会话召回；已压缩 {items} 条历史记录（约 {tokens} tokens）/ 正在压缩… / 点击查看压缩摘要；未知 surface 事件 / 未知内容块 / 已停止；在新对话中分支 / 仅可从已完成轮次的最后一条消息分支；正在重试模型请求 / 等待重试模型请求 / 失败原因；本轮运行失败 / 已达到输出 token 上限 / 回答被截断…发送“继续”可让模型接着输出；命令失败 / 已完成 / 执行中…；图片 / 查看原图 / 图片加载中… / 图片加载失败，点击重试 / 原图预览 / 关闭原图预览；繁忙时 Enter 键行为 / 排队发送 / 插话发送。硬编码非本地化：`Deep diving...`、`Think`。

**共 54 项功能点**。

---

## 2. ui-conversation（skeleton）

### 组件/节点

- **ConversationRoot** — 常驻会话骨架：header 槽 + 滚动体(`data-conversation-scroll`) + 会话体槽 + 粘性 composer 座(`data-composer-seat`)；hero 态渲染 HeroGlow+HeroShell+工作区行。`renderSlotChain('conversation.composer', {interactions:pending, session}, {fallback: composerBar, overlay:true})` 选举 takeover(如 ApprovalPanel)。状态分支：`hero`/`settling`/`active`→`data-phase`；`inert`/`blocked`；`chipTitle` 解析链(pending 工作区→session 工作区→workspaceLabel(cwd)→占位)。槽位：conversation.session.header/session/composer(chain)/composer.bar/input.overlay/input.dock/composer.dock/input.left/input.right/hero.workspace/hero.agentPreset。ResizeObserver 写 CSS 变量 `--dsh-composer-height`。
- **ConversationSessionHeader** — 标题区(面包屑 ancestry + header.actions + header.utilities) + 视图 tab 列表(role=tablist)；`hideChrome=blank&&composerPhase==='blank'` 时隐藏整 header。`deriveAncestry` 沿 parentId 回溯遇 `origin!=='subagent'` 停。字段：`DEFAULT_VIEW_ID='chat'`、`ViewTab{id,label}`。
- **ConversationSession** — active view 区 `renderSlot('conversation.view', {inspect,onInspectDone}, {only:active.id})`；挂载时恢复持久化草稿，卸载 `releaseSessionImages`。
- **InputBar** — 见 §3。
- **ApprovalPanel** — amber 条「等待审批」+ reason 标题 + 灰码命令文本(可滚动 data-approval-scroll) + 右对齐动作行(拒绝/允许一次)。交互：`answer('allowed-once'|'rejected')` + 本地 `answered` 一次性锁，`pending.answer` 失败回滚。字段：`PendingApproval`(key/toolName/reason/callId)、`commandOf(call)` 从 argsRaw 解析 `args.command`。
- **ContextMeter** — 环形 `RADIUS=5.5`、viewBox 14×14、stroke 2px；`percent=min(100,round(usedTokens/contextWindow*100))`；`contextBreakdown` 字段 systemTokens/toolsTokens/messageTokens；figures `~{formatTokens(usedTokens)} / {formatTokens(contextWindow)}`。
- **PermissionSelect** — 见 §3 访问模式。
- **DetailsPanel** — 标题+关闭+「输入」区(CodeBlock JSON pretty)+「输出」区(conversation.details.tool 槽，fallback `<pre>` 或「运行中…」)。`materialFor` 用 `'kind' in found` 判别 settled/running。
- **EmptyHero** — WorkspaceChip(folder 图标+label+chevron)、HeroGlow(feGaussianBlur stdDeviation=50, ellipse fill=#6187D8 fillOpacity=0.08)、HeroShell(FishLogo 34+headline+preview 徽章)。
- **TodoPanel/TodoDock** — 读 `useProjection('todos')`，空返回 null；折叠默认 true；状态字形 completed(对勾)/in_progress(渐变旋转环)/pending(虚线环 dasharray 2.4 2.4)。

### 文案要点（skeleton 中文）

给智能体发消息 / 描述你的任务以生成计划 / 描述你想要构建的内容 / 选择一个工作区开始 / 会话不可用 / 父会话已离线…仍可停止当前运行 / Cmd/Ctrl+Enter 插话发送全部排队消息；发送消息 / 停止生成 / 访问模式，当前：{name}；仅支持 PNG、JPG、WebP、GIF 格式的图片 / 一条消息最多添加 {count} 张图片 / 单张图片不能超过 {size} / 图片总大小超过 {size} / 图片拖动到此处即可添加 / 待发送图片；上下文已用 {percent} / 系统提示词 / 工具 / 对话消息；等待审批 / 拒绝 / 允许一次 / 工具 {toolName} 请求越权执行；确认启用 Full access？/ 我已了解风险，并愿意继续 / 启用 Full access；探索未至之境 / 预览版 / 选择工作区；详情 / 点击消息流中的工具行查看详情 / 输入 / 输出 / 运行中…；任务 / {done} 已完成 / {active} 进行中 / {pending} 待处理。

**共 27 项功能点**（10 组件 + 9 组键盘命令 + 8 类交互）。

---

## 3. ui-conversation（input + InputBar 键盘/交互）

### InputBar 完整键盘命令集（onKeyDown 分支顺序）

1. `Enter`/`Space`（workspaceTrigger 态）：只读触发器 → onRequestWorkspace()。
2. `Shift+Enter`：无条件换行（置于 IME 守卫之前）。
3. `ArrowUp`/`ArrowDown`：`keyboard.arbitrate('up'|'down', composing)`，consumed 则 preventDefault。
4. `Escape`：先 dismissPopup；arbitrate('escape') consumed 则 preventDefault（无浮层时**不**释放 token，删除 token 唯一手势是退格）。
5. `Cmd/Ctrl+z` / `Cmd/Ctrl+y`（含大写/Shift）：preventDefault；redo=(key==='y'||shiftKey)；浏览器原生撤销禁用(chip 事务语义)。
6. `Space`：composing 时 return；`keyboard.space()` true 则 preventDefault（claim token 自带尾随分隔符）。
7. `Enter`（普通）：composing return → arbitrate('enter') !=='pass' 则 preventDefault+return → preventDefault → e.repeat return(长按不连发) → locked||machineBusy return → `accelerated = e.ctrlKey||e.metaKey` → accelerated&&canSteerQueue 则 steerQueue()，否则 `submit(resolveSubmitMode(running, accelerated?'accelerated':'enter', subagent===null))`。

平台差异：`metaKey`(macOS ⌘) 与 `ctrlKey`(Win/Linux) 等价。IME 守卫：composingRef + onCompositionStart/End(结束 10ms 延迟清除，兼容 Safari)+ legacy keyCode===229。**本文件不处理 Tab 补全**（属 ui-input-trigger 候选菜单）。

### 发送/停止按钮

`primaryStops = running && subagent===null`（普通会话运行中显示停止）；`interruptible = running && continuable`（可继续子会话保留 Send 主按钮+独立停止按钮）；`primaryLabel = primaryStops?'停止生成':'发送消息'`；主按钮 disabled=primaryStops?stop===undefined:empty||disabled||machineBusy；`empty = draft.trim()==='' && attachments.length===0`。

### 附件（图片）MIME 与限制

MIME `image/png|jpeg|webp|gif`；`ImageAttachmentLimits`(maxImageBytes/maxImagesPerMessage/maxMessageImageBytes/maxImagePixels/mediaTypes)。`intakeImages(files)` 预检顺序（**格式优先于数量/大小**）：① 格式不符→addImages(由其权威拒绝)；② 数量超限→tooMany；③ 单张超限→fileTooLarge；④ 总大小超限→totalTooLarge。拒绝经 `showToast({seq,text})`。

### 拖放 intakeImages

监听挂 `document`(dragenter/dragover/dragleave/drop)+`window`(dragend)，全页可拖入。`hasFiles(event)=dataTransfer?.types.includes('Files')`；`dropEffect = canAcceptDrop?'copy':'none'`；`dragDepthRef` 计数进/出归零 setDragActive(false)，离开视口边缘 reset；`canAcceptDrop = !locked && !machineBusy && addImages!==undefined`；`onDrop → intakeImages([...dataTransfer.files])`。粘贴：onPaste 从 clipboardData.items 过滤 `kind==='file'` → intakeImages + keyboard.pasteBegin。

### 访问模式 / plan / 模型座

- 访问模式：`permissions = useProjection('permissions')`；预设 `read-only`(盾✓)/`workspace-write`(盾✎)/`danger-full-access`(盾⚠, 常量 `FULL_ACCESS='danger-full-access'`)；选择 full-access 弹 RiskConfirmation，其它直接 `submit('/permission '+id)`；显示名 kebab→Title Case，Full access 强制 'Full access'。
- plan：`planActive = plan!==undefined && (plan.pending? !plan.active : plan.active)`；plan 座槽渲染 `{locked}`；planActive 时 placeholder 用 placeholder.plan。
- 模型座：槽 `conversation.input.model` 渲染 `{locked: modelSeatLocked}`；`modelSeatLocked = removed||inert||!live`（blocked **不**锁模型座）。

### ApprovalPanel 拒绝/允许一次

「拒绝」=outline、「允许一次」=primary，点击后 disabled={answered}(一次性锁)；面板在广播 approval/resolved 帧后离开，InputBar 返回。

### input 域类型（7 文件）

`input/blocks.ts`(ComposerBlock{reason})；`input/contract.ts`(InputState{draft,imageIds,draftRev,phase:'plain'|'adjudicating'|'claimed'|'submitting',claim?,occurrences,paste?,queue}、Occurrence{occurrenceId,source,ref,offset,label,clipboardText,invalid?}、InputEvent 16 类、InputEffect 4 类、InputMachineOptions{mergeWindowMs=1000})；`input/decorations.ts`(scanTextRefs 正则 `/(^|\s)([/@])([\w-]+)/g`、deriveDecorations)；`input/facade.ts`(SessionInputShell、submit(mode='queue')、consumeToken、bindMirror)；`input/hub.ts`(InputHub，sink/steerQueue，错误码 steer-unavailable/queue-item-not-found)；`input/machine.ts`(PLACEHOLDER='￼' U+FFFC、LOG_LIMIT=100、onEnter/onAdjudicated/onSubmitSettled、typing merge 1000ms)；`input/submission-policy.ts`(ComposerSubmissionPolicy、busyEnter、resolve)。上级 `contract/composer-submission.ts`：`BusyEnterBehavior='queue'|'steer'`、`ComposerSubmitGesture='enter'|'accelerated'`、`DEFAULT_BUSY_ENTER_BEHAVIOR='queue'`、`BUSY_ENTER_FIELD='busyEnter'`、`CONVERSATION_SETTINGS_NAMESPACE='ui-conversation'`。

**共 21 项功能点**。

---

## 4. ui-conversation（queue）

### 数据结构

- `QueueRow = QueuedMessage`：`id: MessageId`、`messageId`、`placement: 'queued'|'steering'|'context'`（**仅 queued 接受队列变更**）、`content: readonly ContentBlock[]`、`preview`(previewOf：文本拼接/非文本显示 `[type]`，截断 `QUEUE_PREVIEW_CHARS=200` 加 `…`)、`text: string|null`（**仅全 text 块可编辑**，否则 null）。
- `QueueAction = {kind:'edit';content:ContentBlock[]}|{kind:'remove'}|{kind:'steer'}`。

### QueueDock 交互

渲染：`conversation.input.dock` id `queue` order 20；过滤 queued 后 1 条直接渲染，多条折叠为计数 header「{n} 条排队消息」，空 null。
- 编辑：仅 `row.text!==null`；Escape 取消、Enter(!isComposing) 保存；`updateQueue(id,{kind:'edit',content:[{type:'text',text}]})`。
- 删除：`updateQueue(id,{kind:'remove'})`。
- 插话(steer)：`updateQueue(id,{kind:'steer'})`，**仅 running 可点**（否则「仅运行中可插话发送」）。
- 状态：`running = useSession(s=>s.running)`；`queueMutable = useSession(s=>s.subagent===null)`（子智能体会话锁定）。`applyAction` setBusy→await→失败 notify。

### 文案

{n} 条排队消息 / 编辑排队消息 / 包含非文本内容，暂不支持编辑 / 保存排队消息 / 取消编辑 / 删除排队消息 / 插话发送 / 仅运行中可插话发送 / 编辑失败：这条消息可能已经开始发送。 / 删除失败：这条消息可能已经开始发送。 / 插话发送失败，请重试。

**共 8 项功能点**。

---

## 5. ui-layout（三栏骨架/拖拽/窄屏）

- **AppFrame** — `gridTemplateColumns = ${sidebar}px minmax(0,1fr) ${details}px` 三栏；`ResizeObserver`(rAF 节流) 读 viewport；`narrow = viewport < SIDEBAR_AUTO_COLLAPSE`。
- **DragHandle(side: 'sidebar'|'details')** — onPointerDown 捕获指针+记 origin=clientX，onPointerMove rAF 节流上报 dx，onPointerUp 释放；`onSidebarDrag=setSidebar(sidebarBase+dx)`、`onDetailsDrag=setDetails(detailsBase-dx)`。
- **columns.ts** 常量：`CENTER_MIN=640`；`SIDEBAR_MIN=264 / MAX=420 / DEFAULT=280 / COLLAPSED=56`；`SIDEBAR_AUTO_COLLAPSE=1024`；`DETAILS_MIN=300 / MAX=520 / DEFAULT=360`。让渡三步：① s+d0+CENTER_MIN<=viewport 全按偏好；② 否则收缩 details 到 max(DETAILS_MIN, viewport-s-CENTER_MIN)；③ 否则自动关 details(派生 0，偏好不重写)。sidebar **永不退让**。
- **stores.ts**：`LayoutState{sidebar,details,narrow,narrowExpanded}`；init `{sidebar:280,details:0,narrow:false,narrowExpanded:false}`；actions setSidebar(clamp 264-420)/setDetails(clamp 300-520)/toggleSidebar(narrow 翻 narrowExpanded，宽态翻 sidebar 0↔280)/setNarrow(跨断点清 narrowExpanded)/openDetails(0→360)/closeDetails(→0)。
- **三栏渲染**：左=sidebar(折叠 56px rail)、中=conversation、右=details(宽度 0 仍挂载不卸载，仅视觉关闭)。拖拽手柄：sidebar 在右缘、details 在左缘；折叠 sidebar 无手柄、details>0 才有手柄。
- **⚠️ 宽度偏好不持久化**：store 注释「transient layout store」，纯内存瞬态，无持久化 key；关面板遗忘宽度，重开恢复契约默认(sidebar 280/details 360)。

**共 10 项功能点**（无产品中文文案，纯几何）。

---

## 6. ui-tool

### 三个渲染座
`tool.call.toolview`(keyed, scope=session) + `conversation.chat.node#tool-call` + `conversation.details.tool`。

### ToolCallTree
递归渲染 `node.data.root` 的 subCalls 树（**本身不折叠**，折叠下沉到 ToolRow）；`ToolCall` 分派 `renderSlot('tool.call.toolview', owner, {entryKey: toolName, fallback:<GenericToolCard/>})`；subCalls 缩进 22px(`data-subcalls`)。`callName(node)='kind' in node ? call?.name??'' : node.name`。

### ToolRow（单行摘要行 + 展开体）
- 字段/Props：`variant: ToolRowVariant`、`toolName?`、`icon`、`title`、`summary`、`summarySuffix?(不随省略)`、`body(string|null 输入)`、`output?`、`errorSummary?`、`terminal?/diff?/read?/search?/web?`(互斥 card)、`state: ToolRowState`、`filePath?`、`onOpenFile?`、`inspect?`。
- 状态：`ToolRowState = 'running'|'ok'|'error'|'stopped'`；leading 替换 error→StateDot error、stopped→StateDot warning，默认 icon；`stateStatus` 隐藏文本 运行中/失败/已停止。
- 折叠：DisclosureRow + `expandable = body!==null||output!==null||card!==null`；整行点击展开(expandOnRowClick)、keepContentWhenOpen；**所有卡片默认折叠**；展开体 max-height 内部滚动。
- card 优先级（互斥）：terminal→diff→read→search→web→否则 IO 卡(IN/OUT gutter，code variant 走 CodeBlock 显示 program)。
- 文件工具摘要=可点路径链接(onOpenFile，stopPropagation 独立手势)；error 行摘要=失败首行(error 色)。
- 展开体右下 hover 显示 `Inspect` 按钮(IconInspectOutline12)。

### ToolDetails（详情面板输出体，七分支优先级）
terminal→read→diff→search→web→running(`details.running`)→raw(`<pre data-error>` resultText)。terminal 带 description；search 带 recovery；web 带 raw body(`<pre>` 当 'kind' in block)。

### tool-call-model
- `ToolRowVariant = 'search'|'read'|'bash'|'write'|'edit'|'code'|'others'`。
- `VARIANT_TITLES`：Search/Read/Bash/Write/Edit/Code/Tool call。
- `TOOL_VARIANTS` 映射：bash→bash、pwsh→bash、read→read、web_fetch→read、web_search→search、grep→search、glob→search、write→write、edit→edit、run_code→code、cordis_package_inspect→read、cordis_runtime_inspect→read、cordis_run/stop/undefine→others。
- `TOOL_TITLES`：cordis_package_inspect/runtime_inspect→'Inspect'、cordis_run→'Run Cordis Plugin'、cordis_stop→'Stop Cordis Plugin'、cordis_undefine→'Remove Cordis Plugin'、pwsh→'Pwsh'。
- `SUMMARY_KEYS`：bash[description,command]、read[path,file_path,url]、search[query,pattern,url]、write/edit[path,file_path]、code[description]。
- `toolRowModel`：state=!done?running:error.code==='interrupted'?stopped:isError?error:ok；summary 相对 cwd 显示；errorSummary=state==='error'?firstLine(output):null。

### 7 个 toolview 注册 key 与渲染
- **bash**（key `bash`，独立注册，未复用 ToolRow）：icon+「Bash · {description}」，TerminalBlock(maxLines=Infinity) 或通用 IO 错误卡；error 摘要=失败首行。
- **read**（key `read`）：icon Browse+「Read · {path}」，ReadBlock；summary path 为可打开 host 链接。
- **file-mutation**（key `edit`+`write`）：icon Edit+「Edit/Write · {path}」，DiffBlock；error 走 Output 段+首行摘要。
- **search**（key `grep`+`glob`）：icon Search+标题「Grep/Glob」+`search?.title??summary`，SearchBlock(+recovery footer)。
- **web**（key `web_search`+`web_fetch`）：icon(web_search=Globe/web_fetch=Browse)+标题「Search/Fetch」，WebBlock。
- **todo**（key `todo_write`）：icon Checklist+`todo.rowTitle`；summary=「{done}/{total} 已完成」+active 项，summarySuffix=`+{extra}`(并行 active 数)；planSummary 解析。
- **ask-question**（key `ask_user_question`）：icon Question+`ask.rowTitle`；summary 按 code 分支 ASK_CANCELLED→「已取消」/ASK_ABORTED→「已中断」(state=stopped)/running→「等待回复」/ok→「已回复 {answered}/{total}」。
- **GenericToolCard**（fallback）：`VARIANT_ICONS` 7 变体图标；`singleFile` 时 body=null 且 path 为链接；summary 优先级 terminal.description > search.title > model.summary。

### 6 个 card-model 字段
- diff：`{card:{diffs:[{path,oldText,newText}]}}`。
- read：`{label, lines:[{number,text}], totalLines, lang}`。
- search：`{card:{kind:'matches'|'paths', ..., truncated, total}, title, recovery}`。
- terminal：`{card:{command,cwd,output,exitCode,signal,running}, description}`。
- web：`{kind:'search'|'fetch'}`（WebBlock props）。
- tool-call：ToolRowVariant/State/Model + TOOL_VARIANTS/TOOL_TITLES。
常量：`CHAT_{DIFF,READ,SEARCH}_MAX_LINES=8`；terminal 在 chat 行 maxLines=Infinity(输出内部滚 224px)。

**共 19 项功能点**。

---

## 7. ui-trajectory

### 虚拟行（TrajectoryTable）
常量：`CONTENT_ROW_HEIGHT=30`、`COLLAPSED_SUMMARY_HEIGHT=20`、`TERMINAL_BOUNDARY_HEIGHT=9`、overscan=12、虚拟化阈值=100、初始视口 600px、底部跟随阈值 2px、加载更早阈值 48px。`useVirtualizer`(getScrollElement=tablePaneRef)。`flattenRecords` 展开 turn→group→cell；`foldTurns`(summarizeTurn)/`foldAssistantTools`(summarizeAssistantTools) 生成折叠摘要行。`RecordState='complete'|'running'|'error'`。

### TrajectoryCell 字段（TrajectoryCellProps）
`index`(1-based 显示 `#N`)、`recordId?`、`kind: 'system'|'user'|'context'|'compacted'|'message'|'tool'|'subtool'`、`text`、`previewMarkdown?`、`opensTurn?`、`sourceSeq?`、`messageSource?`、`requestOnly?`、`inputDetail?`、`promptDetail?`、`previousPromptDetail?`、`outputDetail?`、`thinkingDetail?`、`sourceBlocks?`、`outputBlocks?`、`schemaDetail?`、`assistantMetrics?`、`result?`、`resultPreviewMarkdown?`、`callId?`、`isError?`、`timeSeconds(number|null)`、`startedAt?`、`input?/cacheRead?/cacheWrite?/output?/think?`(token 计数)、`selected?`。
- `KIND_LABEL`：System/User/Context/Compacted/Message/Tool/Sub（tag 色 class 区分）。
- 渲染：`#index` + kind tag + text + trailing(message 才显示 input/output/think 三指标)+ `formatElapsedSeconds(timeSeconds)`(未知显示 `—`，整数毫秒千分位)。
- `AssistantMetricDetail`：timingRecorded/stepStartTime/firstTokenTime/completedTime/usageProvided/outputTokens。
- `trajectoryRecordId`：recordId→`${kind}\0call\0${callId}`→`${kind}\0seq\0${sourceSeq}`→`${kind}\0index\0${index}`。

### 详情标签（DetailTab，共 13 个）
`'system-prompt'|'tools'|'overview'|'rendered'|'raw'|'source'|'input'|'output'|'schema'|'options'|'usage'|'timing'|'diff'`。
- `SYSTEM_PROMPT_TABS`：System Prompt / Tools。
- `SYSTEM_UPDATE_TABS`：Diff + System Prompt + Tools。
- `REQUEST_TABS`（请求检查器）：Summary / Options / Usage / Timing。
- compacted：Summary / Raw Output。
- markdown 记录(user/context/message)：Summary / Preview / Raw / (+Source)。
- tool 记录：Summary / Payload(input) / Result(output) / Schema / Timing。
- 注意：请求检查器用 8 个注册 Definition 中的 request-header 记录（`promptDetail`/`previousPromptDetail`/`options`/`usage`/`timing`），并非任务描述里的"5 类"。

### TrajectoryTimeline（Chrome-Network 式瀑布）
- 3 泳道 `LaneLabels`：**Input / Model / Tools**（按 kind，非 turn/step 错开）；并行请求同泳道按时重叠。
- 4 模式 `TrajectoryTimelineMode`：sequence（等宽块，无时间轴）/ duration（按记录时长比例）/ time（等宽 duration 块，保留 idle 间隙=实际时钟）/ actual（真实墙钟）。`data-equal-duration={mode==='time'}`。
- 交互：左键拖拽选区(range)、右键拖拽平移(pan，MINIMUM_DRAG_PX=3)、滚轮缩放(sequence 下 MINIMUM_ZOOM_OPERATIONS=4 步、否则 20)、双击/Escape 复位 range、点击块 onRecordSelect、点击空白 onRecordFocus(最近记录)、hover Tooltip(500ms，显示 kind/Started→/Total/TTFT/Decoding)、`data-search-match`/`data-selected`/`data-error`/`data-current`。边缘平移 `EDGE_PAN_ZONE_FRACTION=0.08`、`EDGE_PAN_STEP_FRACTION=0.025`、`MAXIMUM_EDGE_PAN_PX=32`。底部 `…` 按钮加载更早历史。
- 空态「No timing data」。`timelineTooltipLabel` 用 `formatRecordedTime`(HH:MM:SS.mmm)。

### TrajectoryToolbar（sticky）
按钮：① `toolbar.duration`「Duration」(**英文原样**，时钟图标) aria-pressed=actualDuration，title 切换「Use actual duration / Use equal-width operations」；② `toolbar.actualTime`「实际时间」开关 role=switch，**hidden**；③ `toolbar.turns`「Turns」(**英文**)折叠/展开全部(icon ⊟/⊞，aria/title「Expand/Collapse turns」)；④ `toolbar.calls`「Calls」(**英文**)折叠/展开全部(icon ⊟/⊞)；⑤ 搜索框 placeholder「搜索」、aria「搜索轨迹」。
> ⚠️ zh 字典中仅 `轨迹`/`轨迹工具栏`/`实际时间`/`搜索轨迹`/`搜索` 为中文；`Duration`/`Turns`/`Calls` 及展开/折叠文案保持英文。

### Inspect 跳转机制
点击行 → 用 `record.cell.callId`（或 recordId/seq）匹配定位 → `scrollToIndex` 回到聊天节点；`onOpenCall(callId)`/`recordSelection`/`recordFocus` props。

### 实际耗时展示
duration 来自 epoch-ms 差（`timeSeconds` 秒），`durationSeconds` 派生；显示 `formatElapsedSeconds`(千分位 ms 或 `—`)；Timeline tooltip 显示 `Total`/`TTFT`/`Decoding` 分段。

### 8 个注册 Definition（非 5 类）
`trajectory-inbox-next-step`(message-defs, publication:none) / `trajectory-input-message`(→node) / `trajectory-request-header`(→request-header, change:initial/system/tools/system-and-tools) / `trajectory-assistant-step`(→assistant) / `trajectory-turn-end`(→turn-end) / `trajectory-tool-call`(→tool, 子调用树 MAX_DEPTH=256) / `trajectory-compaction`(→compaction) / `trajectory-session-end`(→session-end)。

### 请求检查器字段（request-header 详情）
- **Overview**：Status / Purpose(compaction 时) / Provider / Model / Tool calls / Subtool calls / Error / Retry(`Scheduled N [of M]`) / Retry delay / Result(跳 Assistant Message 或 Compacted)。
- **Options/Usage/Timing** 预览节；Usage 面板分「This request / Session cumulative」，字段 Input/Cached/Cache created/Other/Output/Reasoning/Content；Timing 用 `AssistantTimingPanel`(Started/Total duration/TTFT/Generation/Throughput) 或 `RequestTiming`(Started/Duration/Timing source)。

### 其它关键常量
`PREVIEW_SOURCE_CHARACTERS=2048`、`PREVIEW_OUTPUT_CHARACTERS=512`(trajectory-preview)；详情栏 `DETAILS_MIN_WIDTH=320/MAX=720`、`TABLE_MIN_WIDTH=280`、`DETAILS_RESIZE_STEP=16`、`TOOL_REQUEST_SHARE=0.58`；`formatDurationMillis`(千分位 `N ms`，未知 `—`)。台账 KIND_LABEL(大写 ASSISTANT/SUBTOOL)与遗留 TrajectoryCell 的 KIND_LABEL(User/Message/Sub) 是两套；台账图标 KIND_ICON(system=Settings/user=User/context=Info/compacted=Compacted/message=Sparkle/tool=Wrench/subtool=Wrench)。

**共 27 项功能点**。

---

## 8. ui-settings-models

### 组件/节点
- **ModelsSection** — 模型页主列：标题/介绍/只读提示/保存成功提示(role=status)/提供方行/添加块/删除确认弹窗。加载态 load()；删除二次确认；关闭 setup 卡记录 dismissedSetup。
- **ProviderEditor** — 单提供方编辑卡(标题+API key+折叠"自定义设置"+页脚)；输入 staged 到 draft，Apply 以 path ops 写回；三布局 `deepseek`/`pi-ai`/`unknown`。
- **CustomProviderCard** — 创建自定义提供方(ns=`llm-pi-ai`)；成功锁定非 key 字段。
- **DeepSeekModelsEditor** — 官方模型目录编辑器(id/name+折叠容量)；行增删；容量 K/M 后缀；重置恢复默认。
- **ModelListEditor** — pi-ai 模型列表 + "获取可用模型"(discoverModels→候选勾选弹窗)。
- **EditorFooter** — 每卡底部取消/提交；提交时禁用取消+换 busy 文案。
- **DeepSeekOnboardingDialog** — 首启引导：官方凭据输入(复用 ProviderEditor credentialOnly)；非 credential-missing 自动 complete。
- **OnboardingModal** — 共享阻塞式 modal(inert 锁根、headless、无关闭按钮)。
- **WelcomeNotice** — 内测声明弹窗(版本化)；点击「继续」acknowledge 后消失。

### 提供方配置字段
- **ProviderEditor API key**：`keyDraft`(本地 state)；`<input type=password autoComplete=off>`；经 `credentials.set({ref,value})`（不写 settings 段）；keyRef=`apiKeyEnv` 或 `deriveKeyRef`=`PROVIDER_API_KEY`(大写、非字母数字→`_`)；校验 `apiKeyFailure`；placeholder 按 writable/keyStored/pi-ai 切换。
- **折叠"自定义设置"**：`displayName`(可选，仅 ownsIdentity)、`baseURL`(deepseek 占位 `https://api.deepseek.com`)、`api`(协议枚举，来自 schema probe.api union)、`models`(DeepSeek 或 ModelList 编辑器)。
- **⚠️ 不存在 reasoningEfforts/temperature/top_p 字段**：reasoning effort 是 per-MODEL 能力，由 composer 模型选择器按模型提供；其余归 settings.yaml(advancedHint 提示)。
- 提交：最小 path ops + `settings.mutate({ns,ops,expectedRevision})`；冲突码 settings-conflict；禁用条件 readOnly||busy||layout==='unknown'||modelFailure||keyFailure||(credentialRequired&&keyEmpty)。

### CustomProviderCard 字段（profile 键）
route(`ROUTE_PATTERN=/^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/` 必填)、displayName(可选)、baseURL(必填)、api(协议枚举)、apiKey(经 apiKeyEnv+credentials)、models(必填≥1)。

### API key 校验（apiKey.ts）
空串 OK；纯空白 keyBlank；`/^[A-Z][A-Z0-9_]*=[^=]/`(粘贴 NAME=value) 或引号包裹或非 `/^[\x21-\x7E]+$/` → keyIllegalCharacters。

### 模型目录字段/校验
`CatalogField`：id/name/contextWindow/maxTokens；id trim 非空+唯一；name 若存在须非空 string；contextWindow/maxTokens 正整数。`parseCapacity`：`/^(\d+(?:\.\d+)?)([km])?$/i`，K=1000/M=1e6；`CAPACITY_HINT={contextWindow:'256K',maxTokens:'32K'}`。

### DeepSeek onboarding 流程
两个独立顺序注册的 onboarding 槽：① welcome-notice(order -100 内测声明) ② deepseek-official(order 0 单步凭据录入)。`onboardingReadiness` 5 态：loading/adapter-absent/provider-ready/unavailable/credential-missing(仅此显示弹窗)。unavailable reason：load-failed/provider-inactive/credentials-unavailable/settings-read-only/credential-read-only。弹窗：取消「稍后配置」、提交「保存并继续」、提交中「保存中…」。

### WelcomeNotice
出现：status 非 idle/loading 且 acknowledged===false；`acknowledged = (settings 'ui-onboarding'.welcomeNoticeVersion === '2026-08-13.1')` 精确相等；持久化 `connection.isLoopback?'host':'memory'`。标题「内测声明」，正文两段，按钮「继续」。

### 文案
模型 / 填入各提供方的 API 密钥即可使用其模型。 / 编辑 / 删除 / 添加提供方 / 添加自定义提供方 / 删除 {provider}？ / API 密钥 / 输入 API 密钥 / 已配置——输入新值可替换 / 由启动环境提供（只读）/ 该 API 密钥格式错误，请检查。 / 模型目录 / 恢复默认模型 / 模型 ID / 显示名称 / 上下文窗口 / 最大输出 token 数 / 容量 / 添加模型 / 获取可用模型 / 选择要添加的模型 / 添加所选 / Provider ID / API 协议 / 创建提供方 / 添加一个 API Key 开始使用 / 配置 DeepSeek 官方模型，即可开始使用。

**共 14 项功能点**。

---

## 9. ui-settings-plugins

### 组件
- **PluginsSettingsSection** — 标题/介绍+标签栏+懒挂载面板；键盘导航 ←/→/Home/End；tab 首次选中才 mount。
- **ConfigurablePluginsTab** — 渲染 settings.plugin.item 卡片列表；无卡 empty。
- **PluginCard** — 可折叠头(名称+说明+未保存徽标)+只读提示+控件+保存/放弃脚注；脏时标记「未保存」。
- **BashCard / AgentLoopCard / WebSearchCard** — 见下。
- **fields.tsx** — `ValueField`(标签+覆盖徽标+重置+输入+hint)、`SecretField`(密码框+配置状态徽标)。
- **card-form.ts** — 共享表单模型：staged 编辑、save 统一写、revision 界定；`CardShell`(available/writable/dirty/invalid/saving/failed)；`CardFieldState`(text/overridden/invalid)；`numberField`(空→clear、Number.isFinite→set、否则 invalid)、`textField`(trim)、`CardSecretSpec`(write credentials)；`CardActions`(edit/resetField/save/discard)。

### BashCard 字段（ns `shell`）
`timeoutMs`(number 可选，空=继承，前台命令超时毫秒)、`maxOutputBytes`(number 可选，单流内存输出上限字节，超出转存临时文件)。

### AgentLoopCard 字段（ns `agent-loop`）
`maxParallelToolCalls`(number 可选，空=继承，同一步并行工具调用数上限)。**⚠️ 只有这一个字段**，不存在 maxIterations/timeout。

### WebSearchCard 字段（ns `web-search-deepseek`）
`apiKey`(write-only，经 credentials.set，ref=apiKeyEnv 或 `DEEPSEEK_API_KEY`)、`baseURL`(string 可选)、`maxUses`(number 可选)。apiKeyConfigured 徽标；credential 刷新订阅 credentials/updated。

### 文案
插件 / 配置和查看本部署已安装的插件。 / 插件配置 / 本部署没有开放任何插件设置。 / 已覆盖 / 恢复默认 / 本部署的设置为只读。 / 保存 / 放弃修改 / 未保存 / 请填数字；留空表示使用默认值。 / 终端 / 限制 agent 运行的每一条命令。 / 命令超时（毫秒）/ 单流输出上限（字节）/ 超出部分会转存到临时文件，而不是被丢弃。 / Agent 循环 / 并行工具调用数 / 网页搜索 / API Key / 不写入设置文件。留空表示保持当前密钥。 / 已配置密钥。 / 未配置密钥；配置之前搜索不可用。 / 接口地址 / 单次请求最多搜索次数。

**共 8 项功能点**。

---

## 10. ui-goal

- **GoalBar** — 输入框上方目标条；不渲染条件 `goal===undefined||null||phase==='complete'||id===clearedGoalId`。
- 阶段标签：`PHASE_LABELS={active:'进行中的目标', paused:'已暂停的目标', blocked:'受阻的目标'}`（complete 无标签，因整体不渲染）。
- 非编辑态：IconGoalOutline16 + 阶段标签 + objective(截断)；blocked 时整条 title=blockedReason.message；actionError role=alert。
- 暂停/恢复：active→暂停按钮 IconPause、paused→恢复按钮 IconPlay，均 disabled={pending}、Tooltip side=bottom delayMs=500。
- 编辑：IconEdit → 预填 draft=goal.objective；input autoFocus，Enter 保存、Escape 取消；保存 IconCheck(disabled=pending||draft.trim()==='')、取消 IconClose；空 trim 直接 return。
- 清除：IconTrash → handleClear(goal.id)；成功本地记 clearedGoalId 墓碑立即消失。
- 防重入 pendingRef + 失败渲染 `"{message} ({code})"`；goalId 变化重置 editing/actionError/clearedGoalId。
- **GoalCommandInputView** — 右对齐 `/goal` 命令气泡(role=group, MessageText)，只展示无交互；文本 `'/'+name+(args??'').trimEnd()`；match `command/run`+name==='goal'；anchorSeq=seq-0.1。
- 字段：`GoalBarActions{onEdit(objective),onPause,onResume,onClear}`、`GoalActionResult=RemoteResult`、`GoalCommandInputData{commandId,text,time}`。

**共 14 项功能点**。

---

## 11. ui-workflow-run

- **WorkflowRunPanel** — `data-workflow-run data-run-status`；`totalMembers=Σphases[i].members.length`；`requiresExpansion = status!=='completed'||phases.some(phaseRequiresExpansion)`；`navigable=useSessions(navigableMembers)`。
- **RunHeader** — title=`run.title`({name})；折叠态右侧分隔符+`memberCount(count)`+`statusTail`(StateDot(dotState(status))+状态文案)。
- **PhaseSection** — title=`readablePhase(phase.phase)`；折叠态 phaseCount+phaseStatusSummary。
- **MemberRow** — StateDot(member.status)+成员名+状态文案；navigable→`<button>` openSession(childId)，否则 `<div>`。
- 字段：`WorkflowRunStatus='running'|'completed'|'failed'|'cancelled'|'interrupted'`；`WorkflowRunMemberData{seq,label,childId,status}`；`WorkflowRunPhaseData{key,phase,members}`；`WorkflowRunChatData{name,status,phases}`。
- 状态→点：running→ongoing、completed→done、failed→error、cancelled→warning、interrupted→warning；文案 运行中/已完成/失败/已取消/已中断。
- 回退：readablePhase null→「未分阶段」、''→「空阶段名」；readableMember ''→「空成员名」。
- 阶段状态汇总 phaseStatusSummary：多状态 ` · ` 连接。
- 展开：requiresExpansion=false→ManualDisclosure(可折叠)；true→DisclosureRow open expandable=false(强制展开)。
- navigableMembers：仅 status==='running' 且 childId∈sessions.ids、origin==='subagent'、parentId===parentId、running 才可点击跳转。
- 事件折叠 workflowRunDefinition：tool-workflow/run-start→start、agent-start/agent-end/run-end→update。

**共 10 项功能点**。

---

## 12. ui-subagent

- **SubagentCatalogAction** — 会话头触发按钮+树目录弹层。
- 触发按钮：aria-haspopup="tree"；`descendantCount`；`runningCount>0` 显示 StateDot ongoing+「{count} 个子代理，正在运行」，否则「{count} 个子代理」；`visible = presentedCatalog!==undefined && (state==='error'||entries.length>0||descendantCount>0)` 否则 null。
- 节点字段：`entry.id`、`entry.label?`(回退 id)、`entry.mode:'one-shot'|'continuable'`、`entry.activity:'running'|'inactive'`、`entry.hasChildren`、`entry.kind:'child'|'diagnostic'`。
- StateDot：activity==='running'?'ongoing':'done'（子节点只用蓝/绿；diagnostic 固定 error）。
- secondary = [summary.title, mode文案(一次性/可继续), activity文案(正在运行/当前未运行)].join(' · ')。
- token：`tokenTotal=uncachedInputTokens+outputTokens+cacheReadTokens+cacheWriteTokens`；`formatTokens`(<1000 原值/<1e6 K/否则 M)；显示 `{n} tok`。
- 耗时：`activityDuration`；`formatDuration` 精度递减(年/月/天/时:分:秒/分:秒/秒)；指标拼接 `{token} · {durationExact}`。
- 展开/折叠：非叶 disclosure chevron，`toggleBranch`；展开后递归 CatalogRows(level+1)，未加载先 CatalogLoadingRows(aria-disabled)。
- 行点击 open=openChild({parentSessionId,childSessionId,mode})+closeCatalog；键盘 Enter/空格打开、ArrowRight 展开、ArrowLeft 折叠；全局 Escape 关闭回焦、Home/End/↑/↓ 移动焦点；外部 pointerdown 关闭；runningCount>0 且 open 每秒刷新耗时。
- 错误态：显示 error.message+重试(refresh)。
- **SubagentReadOnlyComposer** — 只读输入框替代：`matched:{reason:'one-shot'|'parent-unavailable'}`；one-shot→「一次性子代理记录」+「一次性任务不支持后续消息，可在这里查看完整执行记录」；parent-unavailable→「此子代理暂时只读」+「父会话当前不在线，重新打开父会话后即可继续发送消息」。

**共 12 项功能点**。

---

## 13. ui-jobs

- **JobListAction** — 会话头操作，`jobs.length===0` 返回 null(无入口)。
- 列表项字段 `JobView`：`id/kind/label/status/detail/startedAt/finishedAt?`。
- 状态：`'running'|'stopping'|'completed'|'killed'|'failed'`；StateDot 映射 running→ongoing、stopping→warning、completed→done、killed→warning、failed→error；文案 运行中/正在停止/已完成/已取消/已失败。
- 列表项：StateDot + kind + label(title) + status(=`detail??statusLabel`)+ duration；`live=status==='running'|'stopping'`；`elapsed=live?now-startedAt:(finishedAt??startedAt)-startedAt`；formatDuration 时/分/秒。
- 排序：live 在前按 startedAt 升序，结算按 finishedAt??startedAt 降序。
- 触发按钮：liveCount>0 显示 StateDot ongoing+「{count} 个后台任务运行中」，否则「{count} 个后台任务」；open&&liveCount>0 每秒 setNow。
- 关闭：外部 pointerdown、Escape 回焦、jobs.length===0&&open 自动关。

**共 9 项功能点**。

---

## 14. ui-directory-picker-browse

- **DirectoryBrowser** — 680×500 目录选择对话框(Miller 列横向滚动、列纵向滚动)。
- 本地 state：`parent/selected/child`(列)、`loading/slowScan/scanWindow/error/pathDraft/showHidden/folderDraft/creatingFolder/createError`；常量 `SLOW_SCAN_DELAY_MS=300/PARENT_LEG_WAIT_MS=200/DRAFT_PREVIEW_DEBOUNCE_MS=250`。
- **Miller 列**：左列 parent.entries(onPick=select)，选中渲染分隔线+右列 child.entries(onPick=advance 下钻一层)；`twoPane=selected!==null`；行 `<button aria-current>`，选中 IconFolderOpen 否则 IconFolderClose，行尾 chevron。
- **路径编辑**：面包屑模式(pathDraft===null)显示 displayCrumbs(home 子树内折叠为 Home 面包屑)+右侧铅笔按钮进编辑；编辑态 autoFocus 输入，Enter(非 IME)→navigate，Escape/失焦取消；250ms 防抖实时跟随(解析 directory+tail，前缀过滤大小写不敏感 startsWith)。
- **导航落点** land(path)：launchListing(AbortController+seq)；父列 PARENT_LEG_WAIT_MS 内到达则一次双窗格帧落地，超时先单窗格再升级。
- **新建文件夹**：底部「新建文件夹」按钮→嵌套 Modal(显示「在 {name} 内创建」+输入框)，Enter 确认/Escape 关闭；创建成功后 launchListing 重列+select 选中。
- **显示隐藏文件**：底部按钮 `browser.showHidden`，aria-pressed，开启末尾 IconCheck；纯客户端过滤，每次 open 重置 false；`visibleEntries` 选中项永不滤、`showHidden||!entry.hidden`、dot 前缀(needle.startsWith('.'))临时揭示隐藏项。
- **打开/取消**：Open disabled=targetPath===null||loading||parentInert||draftPending，onOpen(targetPath=selected?.path??parent?.path)；busy 冻结。
- 加载/错误/截断：loading&&slowScan(300ms) 浮动 Loading；truncated 提示；error role=alert。
- open/close 副作用：open 重置+navigate(home)；close supersede 终止在途请求。

**共 14 项功能点**。

---

## 15. ui-attachment

- **AttachmentRail** — 草稿附件缩略图横栏(role=group，横向滚动隐藏滚动条)；字段 `items:[{id,previewUrl,alt,removeLabel}]`、`labels:{group,open,scrollLeft,scrollRight}`、`onOpen/onRemove`。左右边缘箭头(1px 容差)；缩略图按钮点击 onOpen、删除按钮 hover/focus 揭示；滚轮纵向→横向平移(独占消费，LINE×16/PAGE×clientWidth，单 tick clamp 60px)；分页 scrollBy(clientWidth-64, 至少 200)，prefers-reduced-motion→auto 否则 smooth；新项滚末尾；ResizeObserver 重算边缘。
- **DropOverlay** — 全视口拖放邀请遮罩(createPortal body，role=status，pointer-events:none)；**自身无拖拽监听**，由 owner 文件拖进页面时挂载/卸载；Props `disabled`+`labels:{title,desc?}`；disabled 灰色卡片+UploadDisabledIllustration(隐藏 desc)，否则倾斜照片卡 UploadIllustration。
- **ImageLightbox** — 原图预览弹窗(createPortal body，role=dialog aria-modal)；Props `src/alt/labels:{dialog,close}/onClose`；背景点击/Escape/关闭按钮三路关闭；挂载聚焦关闭按钮、卸载恢复焦点；**无翻页、无缩放**。
- **MessageImage** — 单图缩略图；Props `attachment/load:ImageLoader=(a)=>Promise<string>/variant:'single'|'tile'/labels`。load：live 守卫+setError/setSrc，失败渲染 error 按钮(labels.loadFailed 点击重试 attempt+1)。singleFit：长边 240px、宽高比 clamp[0.25,4]、scale=min(1,...) 绝不放大、objectPosition 按比例 center top/left center/center；tile 固定 64px 方瓦。src 就绪点击打开 ImageLightbox。
- **ImageGallery** — Props `images:[{attachment}]/load/align:'start'|'end'/labels`；`images.length===0→null`；`variant=length===1?'single':'tile'`；`<div.gallery data-align>`；align 仅作 data-align 属性。
- labels 全量：`{image,open,openNamed(label),loading,loadFailed,lightbox:{dialog,close}}`。

**共 13 项功能点**。

---

## 16. ui-commands

- **PopupSelectView** — 弹层卡片(`aria-label`=`/{command} 选项`，maxHeight=`useAnchoredMaxHeight(cardRef, MAX_HEIGHT=320, state)` 钳到输入框上方)：搜索框(`placeholder`「搜索…」、`aria-label`「筛选选项」、readOnly=submitting)、错误条(role=alert，failed 追加「重试」)、状态行(pending「正在加载选项…」/submitting「正在应用…」/ready 且空「无选项」)、选项列表(role=listbox，`aria-label`=`/{command} 匹配项`，每行 role=option + aria-selected + option.label/detail + active 勾选)。
- 交互：搜索框占焦点；↑↓ 环绕高亮(`(active+dir+rows.length)%rows.length`)、Enter 确认、Esc 取消还焦、←→ 保留光标、外部 pointerdown 捕获关闭(目标接走焦点)；active 行 scrollIntoView。
- 风险确认：`option.confirmation!==undefined` 时 select 只置 `confirming/acknowledged=false`，`confirm()` 需 acknowledged===true 才 settle。
- `PopupState`：open/command/status('pending'|'ready'|'failed')/options/search/active/submitting/confirming/acknowledged/error。
- `CommandDirectory`：`Entry{state:'cold'|'pending'|'ready'|'failed', commands, epoch, lastError, waiters}`；epoch 每次拉取自增，仅最新拉取发布；方法 status/resolve/invalidateAll(软失效)/resetConnected(硬重置)/warm/refresh/ensureReady。
- 决策表（slash 源 `name:'command'`）：`dispatch`(menu：contribution/decorated-host→popup；host 有 input→claim；host bare→detached execute)、`matchSpace`(仅 host leadingInput claim)、`matchEnter`(bare token→popup/exec)；`leadingClaim` token=`/{name} `。
- 契约类型：`SelectOption{id,label,detail?,active?,confirmation?}`、`SelectConfirmation{title,description,acknowledgeLabel,cancelLabel,confirmLabel}`、`CommandUiSpec{kind:'popupSelect',options,onSelect}`、`CommandContribution{name,description,available,ui}`、`CommandDescriptor{name,description,input.hint}`。

**共 12 项功能点**。

---

## 17. ui-message-feedback

- **MessageFeedbackActions**（conversation.chat.assistant-actions id `feedback` order 10，位于复制与分支之间）— 点赞(IconLikeOutline16)/点踩(IconDislikeOutline16)按钮：已选态 tooltip/aria 为「取消标记」，未选为「好的回答」/「有问题的回答」，`aria-pressed`、`data-active`、`disabled={pending}`；备注入口文本 `item?.note===undefined?'补充说明':item.note`；备注编辑器 textarea(aria-label「反馈说明」、placeholder「这条回答哪里好，或哪里有问题？（可选）」、rows=2)+「保存」/「取消」。
- 交互流程：懒加载（首次 hover/focus 才 `ensure()`）；`onRate`→`toggle`（同值=撤销/删除）；`onSaveNote`（空=clearNote 删备注，非空=rate 带 note）；settle：version-conflict→「这条反馈已在别处改动，已显示最新状态」，否则失败「反馈保存失败」；loadFailed→「反馈状态加载失败」。
- controller 方法：`ensure/refresh/resync(重连排队重读)/rate/toggle/clearNote/clear`；`MessageFeedbackStatus='cold'|'loading'|'ready'|'error'`；put/delete 都带 `ifVersion`，`version-conflict` 回执带权威 `current` 对账。
- 持久化字段 `MessageFeedbackItem{messageId,rating:'positive'|'negative',note?,version,createdAt,updatedAt}`（version 为等值比较令牌，createdAt/updatedAt 为 Unix 毫秒）。
- 错误码：session-not-found/target-not-found/version-conflict(带 current)/note-blank/note-too-large(带 maxBytes/actualBytes)。

**共 8 项功能点**。

---

## 18. ui-deliverables

- **ProducedFiles** — 产物行：标签「产物」+ chip 列表；每 chip `title=完整路径`、`aria-label`=`打开 {name}`、显示 `basename(path)`；超出显示「+ 1 个文件」/「+ {count} 个文件」；`fitProducedFiles(available,gap,chipWidths,moreWidths)` 测量 chip 宽度求最大适配前缀 + ResizeObserver 自适应。交互：chip 点击 `openFile(path)`(Host 打开器)；`hidden>0 && canOpenPath(isLoopback && hostDescription.canOpenPath===true)` 时显示「在文件夹中显示」→ `openFile('.')`。
- **chatFileMentions 服务**：`forClosing(owner)`→无产物返回 undefined，否则 `producedFileMentions(paths, owner.openFile, label)`；`MarkdownFileMentions.resolve(value)`：精确匹配 `paths.includes(value)`，否则仅当「恰有一个产物 basename===value」命中(`onlyPathWithBasename`，同名则惰性不解析避免开错文件)；命中返回 `{open,label,title}`。
- **deliverablesDefinition**：`producedPaths(view)` 取 card==='diff' 或 (generic && kind==='edit') 的 `locations[].path`（读/删/终端/失败调用不贡献）；match turn/start(start)+tool/call(update)+tool/result(append)；发布 turn 级 `{produced}`。

**共 7 项功能点**。

---

## 19. ui-permission-presets

- **PermissionRow**（settings.general.item id `permission` order -20）标题「权限」/描述「选择新会话的默认权限模式」；Menu 下拉(selectedId=currentValue, align=end, portal)；选 `danger-full-access` 先弹 RiskConfirmation 再 `select(id)`；`busy=loading||saving||confirmingFullAccess`。
- **默认预设表**（host `dsh-permission-presets` Config.presets）：`workspace-write`(sandbox `workspace-write` + approval `ask`)、`danger-full-access`(sandbox `danger-full-access` + approval `never`)；`CUSTOM_PRESET='custom'` 为派生态（当前 knob 组合不匹配任何表项时），**永不作为切换目标**，弹窗/行过滤掉。
- 设置写回：`settings.mutate({ns:'permission', ops:[{op:'set', path:['defaultPreset'], value:preset}], expectedRevision})`；`permissionDefaultOf(view)` 从 schema union/const 解析 currentValue+options。
- `/permission` 命令：`command.decorate({name:'permission', ui:{kind:'popupSelect', options, onSelect}})`；`optionsOf` 过滤 `value!=='custom'`，full-access 项带 `confirmation`；onSelect 执行 `command('/permission '+id)`。
- 显示名：`displayPermissionPreset`：`danger-full-access`→'Full access'，否则 kebab→Title Case(`workspace-write`→'Workspace write')。
- /permission popup 风险确认文案(accessZh)：title「确认启用 Full access？」、description「启用 Full access 后，agent 将减少确认步骤，并且可以直接执行更多操作，包括敏感操作、文件修改或外部命令。仅建议在你信任当前任务时使用。」、acknowledge「我已了解风险，并愿意继续」、cancel「取消」、enable「启用 Full access」。设置行版 description 改为「…仅建议在你信任后续任务时使用。」

**共 7 项功能点**。

---

## 20. ui-agent-preset

- `trust:'system'|'user'`（内置/自定义）；内置预设 id 与中文：`standard`=标准模式、`code`=PTC 模式、`minimal`=极简模式、`cordis`=创造模式。
- 四表面：**AgentPresetSeat**（新会话 staged→apply + introduce 逐字符动效，常量 `INTRO_TEXT_DELAY_MS=150/INTRO_CHAR_STAGGER_MS=40/INTRO_TEXT_REVEAL_MS=200/INTRO_CHAR_FADE_MS=400`）、**AgentPresetLabel**（只读 header 标签）、**AgentPresetRow**（General 默认，settings ns `agent-presets` 字段 `default`）、**AgentPresetSection**（分节管理页：内置/自定义分组、查看/复制/删除/设默认/定位目录/创造模式）、**PresetMenu**（共享选择器，user 项加「· 自定义」标注）。
- `PresetRow` 字段 id/name?/description?/trust/isDefault/broken?；复制对话框字段「标识符」(placeholder `my-agent`)+「名称」，校验 draftBlocker 返回 idRequired/idInvalid/idTaken；`PRESET_ID=/^[a-z0-9][a-z0-9-]*$/`。
- 内置预设描述（locales）：standard「功能完整的编码 Agent，支持文件编辑、Shell、文件与网页检索、Skills、计划、目标、子代理和工作流。」、code「…通过 Code Mode SDK 呈现工具，让模型用一个 TypeScript 程序组合多步操作。」、minimal「仅提供持久 bash 与 str_replace_editor 的双工具编码 Agent。」、cordis「用于创建自定义 Agent preset：…运行时检查、插件实验和 preset 创作指导。」
- 删除确认 Modal：「删除该预设？」+「预设目录将被删除。已在其上运行的会话不受影响；新会话将无法再选择它。」

**共 12 项功能点**。

---

## 21. ui-plan

- **PlanChip**（PlanModeControl）— 读 plan 投影，有效目标 `target = pending ? !active : active`；chip 字面 'Plan' + 关闭图标；点击执行 `/plan off`；locked/leaving 禁用；错误文案英文 'failed to exit plan mode'。

**共 4 项功能点**。

---

## 22. ui-skill

- **SkillRow**（tool.call.toolview key `skill`）— `data-tool="skill"`、`data-state`；行头：leading(error→StateDot error/stopped→StateDot warning/否则 IconSkillOutline16 + hover 展开 chevron) + 隐藏状态文案(正在加载 skill/skill 加载失败/skill 加载已中止) + 固定标题「Skill」+ 摘要(`errorSummary ?? name`)；展开体=「说明」区块(`<section aria-label="说明">` + `<pre data-error>` output)+ 可选 Inspect。
- 状态推导 `skillRowModel`：`settled='kind' in block`；未结算→running；`error.code==='interrupted'`→stopped；`isError`→error；否则 ok。名称 `skillName(argsRaw,callId)`：解析 args JSON 的 `name` 字段(firstLine 回退)，失败用 firstLine(argsRaw)，空用 callId。`SkillRowState='running'|'ok'|'error'|'stopped'`；`SkillRowModel{name,output,errorSummary,state}`。
- '/' skill 源(name `skill` order 2)：`fetchCatalog` per-session 单飞缓存(`skills.list`→value.skills，失败删 key，subagent 地址非空返回 [])；`candidates` 按 `skill.name.startsWith(query)` 过滤，`modelInvocable=false` 时 description 前加「仅用户 · 」；`onPick` 返回 `{text:'/${name} '}`；失效 agent-preset/selected→invalidate、connection/reset→clearAll。
- `SkillEntry` 字段 name/description/modelInvocable。

**共 8 项功能点**。

---

## 附录：共享原语（ui-primitives，多包复用）

- **Button** — `variant:'primary'|'ghost'|'outline'|'toolbar'`(默认 ghost)、`size:'md'(36px)|'sm'(28px)`、`icon?`；native button 属性透传。
- **Modal** — 全视口 dialog(createPortal body)；props `open/onClose/title/closeLabel='Close'/description?/children/footer/contentClassName/headless`；Escape/遮罩点击关闭。
- **Menu** — props `open/compact?/portal?/align/side/anchor/items(MenuEntry{id,label})/onSelect/onClose/getAnchorRect`。
- **DisclosureRow** — 24px 折叠 chrome；props `icon/title/open/expandable/onToggle/expandOnRowClick?/previewChevron?/keepContentWhenOpen?/collapsedContent/children`；hover 图标→chevron 预览；expandOnRowClick 时整行 Enter/Space 触发。
- **StateDot** — `state:'done'|'warning'|'ongoing'|'error'`、`size=10`；ongoing=3×3 像素追逐动画(8 格顺时针，animationDelay=(index-8)*125ms)，其它=CSS 双伪元素(光晕 10%+实心核 6/10)。
- **RiskConfirmation** — props `open/title/description/acknowledgeLabel/cancelLabel/confirmLabel/acknowledged/disabled?/onAcknowledgedChange/onCancel/onConfirm`；主按钮 disabled=disabled||!acknowledged。
- **DiffBlock** — `diffs:DiffHunk[{path,oldText:null|string,newText}]`、`maxLines=16`；路径头+`-`删行(error 色)+`+`增行(success 色)；超出中部折叠「… 其余 {hidden} 行」；footer `└ +A -R · N file(s)`(去重路径计数)；复制按钮；不软换行。
- **TerminalBlock** — `command/cwd?/home?/output?/exitCode?/signal?/running?/maxLines=16/labels`；prompt 行(StateDot+cwd 折叠 `~`/末段+命令)；ANSI 解析(ansi.ts)；状态 Pill(信号 X/退出码 X)；无输出「无输出」；复制原始 output；中部折叠。
- **ReadBlock** — `label?/lines:[{number,text}]/totalLines/lang?/maxLines=16`；banner(label+`显示 N / M 行`+lang+复制)；行号 gutter(文件真实行号，窗口不重排)；shiki 语法高亮(懒加载 grammar 后重渲染)；中部折叠。
- **SearchBlock** — kind `'matches'|'paths'`；matches 按文件分组(path 头可折叠+`lineNumber: line`)、paths 平铺路径；`truncated/total` → banner「显示 X / 共 N 处匹配 · K 个文件 / 个路径」；尾部切在匹配中间时补文件头；复制结构化全文；中部折叠；空「无结果」。
- **WebBlock** — kind `'search'|'fetch'`；search=可选 answer(MarkdownText)+有序来源列表(安全外链仅 http(s)，title 或 hostname 标签，snippet/publishedAt)，`truncated`→「来源列表已截断」；fetch=链接+HTTP status+「内容已截断」；空「未找到结果」。
- **JsonTree** — 只读可复制 JSON 树；props `data/label='JSON'/copyable=true/expandTopLevel=true/labels`；预览限 OBJECT_PREVIEW_LIMIT=4/ARRAY_PREVIEW_LIMIT=5/PREVIEW_DEPTH_LIMIT=2；行 hover 显示复制按钮，右键菜单 Copy value/JSON/property path/pretty/compact；键盘 ←→ 展开折叠、↑↓ 移动焦点。
- 其它：Pill、Tooltip(side/delayMs)、Toast、HoverCard、Input、MessageText/MarkdownText(markdown 渲染)、CodeBlock(shiki)、clipboard.ts(writeClipboard)、use-copy-feedback.ts、head-tail-cap.ts、ansi.ts。

---

## 汇总：功能点计数

| 包 | 功能点 |
|---|---|
| ui-conversation (chat) | 54 |
| ui-conversation (skeleton) | 27 |
| ui-conversation (input) | 21 |
| ui-conversation (queue) | 8 |
| ui-layout | 10 |
| ui-tool | 19 |
| ui-trajectory | 27 |
| ui-settings-models | 14 |
| ui-settings-plugins | 8 |
| ui-goal | 14 |
| ui-workflow-run | 10 |
| ui-subagent | 12 |
| ui-jobs | 9 |
| ui-directory-picker-browse | 14 |
| ui-attachment | 13 |
| ui-commands | 12 |
| ui-message-feedback | 8 |
| ui-deliverables | 7 |
| ui-permission-presets | 7 |
| ui-agent-preset | 12 |
| ui-plan | 4 |
| ui-skill | 8 |
| **合计** | **318** |

### 迁移实现最需锁定的硬编码值（速查）

- 断点/列宽：`SIDEBAR_AUTO_COLLAPSE=1024`、`CENTER_MIN=640`、sidebar 264–420(默认 280)/collapsed 56、details 300–520(默认 360)；宽度偏好**不持久化**。
- 键盘：`Cmd/Ctrl+Enter=加速发送`、`Shift+Enter=换行`、`Esc=关浮层`、`Cmd/Ctrl+z/y=撤销/重做`(禁用原生)。
- 图片：MIME `png/jpeg/webp/gif`；拖放判定 `dataTransfer.types.includes('Files')`、`dropEffect copy/none`。
- 权限预设：`read-only / workspace-write / danger-full-access`。
- 队列：`placement=queued/steering/context`、`QueueAction=edit/remove/steer`、preview 200 字符。
- 输入状态机：`phase=plain/adjudicating/claimed/submitting`、`BusyEnterBehavior=queue/steer`(默认 queue)、undo 环 100、typing merge 1000ms、占位符 `U+FFFC`。
- trajectory：行高 30/折叠摘要 20/边界 9、overscan 12、虚拟化阈值 100、Timeline 3 泳道 Input/Model/Tools、4 模式 sequence/duration/time/actual、13 DetailTab、REQUEST_TABS=[Summary/Options/Usage/Timing]。
- tool：`ToolRowVariant` 7 值、`ToolRowState` 4 值、`CHAT_{DIFF,READ,SEARCH}_MAX_LINES=8`、terminal 展开体 maxLines=Infinity、subCalls 缩进 22px、卡片默认折叠、DiffBlock/TerminalBlock/ReadBlock/SearchBlock maxLines=16。
- settings：path-op + `expectedRevision` 写回、API key 走独立 credentials 域(不回显)、"空即继承"(undefined=删除覆盖层)、`welcomeNoticeVersion='2026-08-13.1'`、`PRESET_ID=/^[a-z0-9][a-z0-9-]*$/`。
