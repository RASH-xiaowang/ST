# DSH 能力面盘点清单（deepseek-harness-master）

> 数据来源：`docs/tool-catalog.md`（权威工具目录，24 个工具包 / 约 53 个模型可见工具名）、各 `packages/<group>/README.md`。均为只读研究，未改任何文件。DSH 是"全插件化"的 Cordis harness：**一切能力都是插件**，通过 `ctx.<service>`（服务）、`ctx.on()/ctx.effect()`（事件/副作用）、`ctx.tools`（模型工具）、`Remote`（跨进程 RPC）暴露。

## 一、模型工具完整清单

按功能族分组（`工具名 | 用途 | 需审批 | 备注`）。

### 执行/进程族

| 工具名 | 用途 | 需审批 | 备注 |
|---|---|---|---|
| `bash` | 一次性 bash 命令（全新进程，`$DSH_*` 环境，可后台） | 否（可选文件沙箱） | `tool-bash`，后台走 `ctx.jobs` |
| `pwsh` | PowerShell 命令（Windows） | 否 | `tool-pwsh`，每次全新进程 |
| `bash`(持久) | 持久化 bash shell，状态跨调用保留 | 否 | `tool-bash-persistent`，需 PTY 后端 |
| `run_code` | 执行 TypeScript 程序，`await tools.name()` 调其它工具 | 否 | `dsh-tools`，code-mode 保留传输，子调用经完整工具管线 |
| `terminal_open` | 创建持久、owner 隔离的终端会话 | 否 | `tool-terminal` |
| `terminal_list` | 列出当前代理拥有的终端会话 | 否 | `tool-terminal` |
| `terminal_read` | 读取持久终端的有界输出页 | 否 | `tool-terminal` |
| `terminal_send` | 向持久终端发送文本（可后台） | 否 | `tool-terminal` |
| `terminal_signal` | 向前台进程组发送允许信号 | 否 | `tool-terminal` |
| `terminal_close` | 关闭持久终端并等待进程树退出 | 否 | `tool-terminal` |
| `job_list` | 列出后台作业（运行/完成） | 否 | `tool-jobs` |
| `job_output` | 读取后台作业输出（可 wait） | 否 | `tool-jobs` |
| `job_kill` | 取消后台作业 | 否 | `tool-jobs` |

### 文件/发现族

| 工具名 | 用途 | 需审批 | 备注 |
|---|---|---|---|
| `read` | 读 UTF-8 文本（行号，offset/limit） | 否 | `tool-fs` |
| `write` | 创建/整体覆写文件 | 否 | 受 read-before-write 策略 |
| `edit` | 字面替换（old_string→new_string） | 否 | `tool-fs` |
| `read_image` | 读图片返回图像本身 | 否（需图片可输入模型） | 需 `ctx.attachments` |
| `glob` | 文件发现（glob 模式） | 否 | `tool-fs-search`，内置 ripgrep 二进制 |
| `grep` | 内容搜索（正则） | 否 | `tool-fs-search` |
| `str_replace_editor` | view/create/str_replace/insert 四命令编辑器 | 否 | `tool-str-replace-editor` |

### 编排/多代理族

| 工具名 | 用途 | 需审批 | 备注 |
|---|---|---|---|
| `subagent` | 派发独立子任务（可后台，可续聊） | 否 | `tool-subagent`（默认名可配） |
| `subagent_fork` | 从父会话已完历史派生子代理 | 否 | fork 后端实例 |
| `interrupt_agent` | 请求取消子代理当前 turn | 否 | `tool-subagent-control` |
| `list_agents` | 列出可续聊后台子代理 | 否 | `tool-subagent-control` |
| `send_message` | 向后台子代理续发消息 | 否 | `tool-subagent-control` |
| `report` | 子→父结果回报通道 | 否 | `tool-subagent-report`（子作用域） |
| `workflow` | 运行 JS 编排脚本（agent/pipeline/parallel/phase） | 否 | `tool-workflow` |
| `ralph` | 固定"每轮全新子代理"迭代循环 | 否 | `tool-ralph` |
| `todo_write` | 结构化任务清单（整表替换） | 否 | `tool-todo` |

### 目标/计划/调度族

| 工具名 | 用途 | 需审批 | 备注 |
|---|---|---|---|
| `create_goal` | 创建会话内长期目标 | 部分（需直接人类根授权） | `tool-goal` |
| `get_goal` | 读取当前目标 | 否 | `tool-goal` |
| `update_goal` | 更新目标（edit/pause/resume/complete/blocked） | 部分（edit/pause/resume 需人类根授权） | `tool-goal` |
| `exit_plan_mode` | 计划模式：呈交计划供人工审批后退出 | 人工审查 | `plan-mode` |
| `schedule_create` | 创建会话内定时提醒 | 否 | `schedule` |
| `schedule_delete` | 删除提醒 | 否 | `schedule` |
| `schedule_list` | 列出提醒 | 否 | `schedule` |

### 知识/检索/问答族

| 工具名 | 用途 | 需审批 | 备注 |
|---|---|---|---|
| `skill` | 加载技能完整指令 | 否 | `tool-skill` |
| `session_event_read` | 读取单个完整事件 | 否 | `tool-session-query`（工作区授权，只读） |
| `session_event_search` | 搜索单会话事件 | 否 | `tool-session-query` |
| `session_event_trace` | 追踪事件替换/关系链 | 否 | `tool-session-query` |
| `session_search` | 跨会话搜索最强匹配事件 | 否 | `tool-session-query` |
| `session_trace` | 读取会话谱系 | 否 | `tool-session-query` |
| `lsp` | 语言服务器导航（goToDefinition/findReferences/goToImplementation/hover） | 否 | `tool-lsp` |
| `web_search` | 网页搜索 | 否 | `tool-web`（后端可换） |
| `web_fetch` | 抓取 HTTP(S) 内容 | 否 | `tool-web` |
| `ask_user_question` | 向用户提问 | 人工交互（暂停等 UI 答案） | `tool-ask-user` |

### 运行时自修改族（opt-in，不在默认树）

| 工具名 | 用途 | 需审批 | 备注 |
|---|---|---|---|
| `cordis_define` | 定义不可变 Cordis Package | 否 | `tool-cordis` |
| `cordis_inspect_list` | 列出 Inspect Provider | 否 | `tool-cordis` |
| `cordis_inspect_query` | 只读查询 Provider 方法 | 否 | `tool-cordis` |
| `cordis_inspect_self` | 检查自身插件/包 | 否 | `tool-cordis` |
| `cordis_run` | 激活一个 Package | 可能需审批（Client 包未授权→awaiting-approval） | `tool-cordis` |
| `cordis_stop` | 停止当前 Run | 否 | `tool-cordis` |
| `cordis_undefine` | 永久删除插件 | 否 | `tool-cordis` |

### 动态工具

- MCP 客户端把外部 MCP 服务器工具桥接为 `mcp__<server>__<tool>`（运行时发现，非静态目录）。
- 动态 cordis 插件运行期可追加注册新工具。

## 二、Host 服务能力清单（packages/host）

| 包 | 核心能力 | 关键服务/端点 |
|---|---|---|
| `webserver` | `node:http` 服务器，精确/前缀路由、升级路由、单一 fallback 席位、index.html tap 变换 | `ctx.webServer`；仅 `127.0.0.1`/`0.0.0.0` |
| `apiproxy` | 共享 API 网关与线缆契约（四象限 wire 联合） | `ctx.apiProxy` |
| `frontend-static` | SPA dist 服务（fallback 席位，SPA 路由回退 index.html，防目录穿越 403） | 消费 `ctx.webServer` |
| `directory-picker` | 目录选择能力缝 | `ctx.directoryPicker`，`capability()` 返回 native/browse |
| `directory-picker-native` | 原生 OS 选择器（osascript / zenity+kdialog / Windows koffi COM IFileOpenDialog） | 注册 `ctx.directoryPicker` |
| `directory-picker-browse` | 应用内目录浏览（单层列举+建目录） | 注册 `ctx.directoryPicker` |
| `directory-picker-auto` | 启动时探测宿主，自动挂 native/browse 后端 | 挂载后端 |
| `plugin-inventory` | 只读 Loader 树投影 | Remote `pluginInventory/list` |

### API 网关 RPC 域（apiproxy）

- 四象限 wire：`ClientRequest`(`POST /api/<method>`)、`ServerResponse`、`ServerRequest`(SSE 帧)、`ClientResponse`(`POST /api/respond`)。
- 业务 RPC 域：
  - `session.*`：history / fork / models / selectModel / prompt / rename / search / updateQueue / cancel / create / export / list
  - `host.*`：pickDirectory / listDirectory / createDirectory / openPath / describe
  - `workspace.*`：create / insertBefore / delete / list / archiveSession
  - `agentPreset.*`：list / select / read / copy / openDocument / remove
  - `command.*`：list / execute
  - `skill.*`：list
  - `settings.*`：describe / update / replace / mutate / openDocument
  - `credentials.*`：describe / set
  - `llm.*`：models / listConfigurableProviders
  - `subagent.prompt`

### HTTP 端点汇总

- `POST /api/<method>`（业务 RPC）
- `POST /api/respond`（服务端请求应答）
- `GET|HEAD /api/session.export`（流式 ZIP 会话日志导出）
- SPA 静态资源 + SSE/升级 WebSocket（由 client/connection 与 hmr 插件挂载）

## 三、LLM 提供商与模型能力（packages/llm）

| 包 | 核心能力 |
|---|---|
| `llm`(`ctx.llm`) | 提供商中立服务定义+消费方。`registerAdapter`/`listProviders`/`listModels`/`discoverModels`/`resolveModelInfo`/`resolveCallConfig`/`prepareCall`/`stream`；流式 chunk 协议（block-start/text-delta/reasoning-delta/tool-call-delta/block-end/usage/finish）；事件 `llm/stream`(waterfall) |
| `llm-deepseek` | 直接 DeepSeek chat-completions 适配器（fetch+SSE/eventsource-parser），路由 `deepseek-official` |
| `llm-pi-ai` | 通用多提供商适配器（基于 `@earendil-works/pi-ai`） |
| `llm-retry` | 提供商级重试策略执行器（监听 `agent/request-error`） |
| `token-meter`(`ctx.tokenMeter`) | 可重放的 token 计量 |

### 内容块与采样

- 内容块：text / reasoning / tool-call / tool-result（多模态需插件扩展，无核心图/音块）。
- 采样：temperature / maxTokens / stop（无 tool_choice / top_p / penalty）。
- 错误码：AUTH / QUOTA / RATE_LIMIT / CONTEXT_WINDOW_EXCEEDED / EMPTY_RESPONSE / INVALID_CREDENTIAL / MISSING_CREDENTIAL 等。

### llm-deepseek 要点

- 配置：`apiKeyEnv`(`DEEPSEEK_API_KEY`)、`baseURL`、`thinking`、`reasoningEffort`(off/high/max)、`maxTokens`(默认 256000)、`defaultContextWindow`(默认 1000000)、`models` 目录（默认 v4-flash/v4-pro）、`retryPolicy`。
- 动态配置走 `ctx.settings`(llm-deepseek 命名空间)+`ctx.credentials`（每操作解析、trim+格式校验）。
- 请求头：User-Agent 归属、`x-deepseek-harness-user-id`/`x-deepseek-harness-session-id`、compaction 专用头；reasoning_content 回传规则、cache 计数。

### llm-pi-ai 要点

- 协议：openai-completions、openai-responses、anthropic、deepseek 等；`supportedProtocols()` 仅含可用 key+endpoint+headers 完整描述的协议（Bedrock/Vertex/Azure/Codex 被排除）。
- 目录机制：安装目录路由默认继承 provider 端点/协议/模型目录；`models` 列表整体替换、`modelOverrides` 单模型整形；hand-declared 路由需 `api`+`baseURL`+`models`。
- 模型发现 `/models`：openai-completions/openai-responses 的 `GET /models`（bearer 认证）；目录路由本地回答、非目录路由网络探测。
- 凭据：`apiKeyEnv` 引用 + `ctx.credentials`，每操作解析、trim+格式校验。
- 输入模态：`defaultInput`/entry `input`（text/image 等）。
- 推理：`reasoningEfforts`（off/minimal/low/medium/high/xhigh/max，可映射 wire 拼写）+ `compat.thinkingFormat`。
- 其它配置字段：headers / transport / timeoutMs / thinkingBudgets / cacheRetention 等。

## 四、各能力包一览

`包 | 核心能力 | 关键服务/事件/工具`

| 包 | 核心能力 | 关键服务/事件/工具 |
|---|---|---|
| `core/` | 产品 API 脊柱 | session(`ctx.sessions` 事件溯源日志)、system-prompt(`ctx.systemPrompt`)、tools(`ctx.tools`)、agent(`ctx.agents`)、agent-loop(`ctx.agentLoop`)、agent-default-model |
| `shell/` | bash 执行缝 | `ctx.shell`；bash-local/bash-sandbox/pwsh-local；shell-env(`ctx.shellEnv`)；tool-bash/tool-pwsh |
| `subprocess/` | 子进程基座 | `ctx.subprocess`；subprocess-local |
| `fs/` | 文件系统缝 | `ctx.fs`；fs-local/fs-sandbox/fs-e2b；fs-observation-policy；tool-fs/tool-fs-search |
| `terminal/` | 持久 PTY | `ctx.terminals`；terminal-bash；tool-terminal |
| `sandbox/` | 进程隔离缝 | `ctx.sandbox`；sandbox-local(bwrap/Landlock/Seatbelt)；sandbox-policy(`ctx.sandboxPolicy`) |
| `lsp/` | LSP 缝（4 操作） | `ctx.lsp`；lsp-stdio；tool-lsp |
| `mcp/` | MCP 客户端桥 | mcp-client（外部工具→`mcp__<server>__<tool>`） |
| `web/` | web 缝 | `ctx.web`；web-search-exa/perplexity/deepseek、web-fetch-http；tool-web |
| `jobs/` | 后台作业 | `ctx.jobs`；jobs-local；tool-jobs |
| `workflow/` | 动态编排 | `ctx.workflowEngine`；workflow-worker-thread；tool-workflow/tool-ralph |
| `goal/` | 会话目标 | `ctx.goals`；goal-round-driver；tool-goal/command-goal |
| `schedule/` | 会话内提醒 | 无公开服务，`schedule/change` 事件驱动 |
| `plan/` | 计划协作状态 | `ctx.planMode`，`plan/mode` fold |
| `todo/` | 会话 todo 列表 | tool-todo，`todo/write` 事件 |
| `subagent/` | 子代理缝 | `ctx.subagents`；inprocess/spawn/fork/acp/codex/claude-code/dsh-sdk 提供者；tool-subagent/control/report |
| `interaction/` | 人机协作面 | commands(`ctx.commands`)、user-approval(`ctx.approval`)、permission-presets(`ctx.permissionPresets`)、user-questions(`ctx.userQuestions`)、tool-ask-user |
| `session-query/` | 会话检索 | `ctx.sessionQuery`；session-query-sqlite(FTS)；session-log-export；tool-session-query |
| `compaction/` | 压缩缝 | `ctx.compaction`；compaction-basic；compaction-tool-result-pruner；command-compact |
| `attachment/` | 持久附件 | `ctx.attachments`（内容寻址存储）；attachment-local |
| `skill/` | 技能缝 | `ctx.skills`；skill-filesystem/skill-badge；tool-skill |
| `hooks/` | hook 桥 | hook-protocol；hooks-claude-code/hooks-codex |
| `credentials/` | 凭据引用缝 | `ctx.credentials`；credentials-local(env/.env) |
| `settings/` | 用户设置缝 | `ctx.settings`（命名空间+分层解析+热提交）；settings-file |
| `storage/` | 非会话存储 | `ctx.storage`；storage-json/sqlite；storage-domain(`ctx.storageDomain`) |
| `preset/` | 每会话代理组合 | `ctx.agentPresets`（agent.cordis.yml 目录）；persona |
| `context/` | 请求上下文插件 | session-reference/time-context/tmux-context/agent-instructions |
| `session/` | 持久化数据面 | session-persistence(+jsonl/sqlite)、session-checkpoint-policy、session-projection(+cache/stats)、session-title(+llm)、session-telemetry(+otel) |
| `identity/` | 匿名身份 | anonymous-user-id（无认证） |
| `api/` | Remote API 层 | remotes(BFF 策略)+gateway(Typert RPC 分发) |
| `typert/` | 类型图+运行时注册表 | registry/loader/generator |
| `extensions/` | 运行时自修改 | tool-cordis、cordis-host-runner(`ctx.dynamicCordisRunner`，node:vm 沙箱)、cordis-client-runner、ui-cordis |
| `code-runtime/` | 代码执行缝 | `ctx.codeRuntime`；code-runtime-worker(worker-thread) |
| `guard/` | 循环卫生守卫 | repeat-tool-reminder、timeout-policy（`tools/execute` 截止） |
| `feedback/` | 人类反馈 | command-feedback(`feedback/record`)+message-feedback(`ctx.messageFeedback`) |
| `e2b/` | E2B 远程执行（POC） | e2b/fs-e2b/subprocess-e2b |
| `spill/` | 超限结果落盘 | spill/spill-local/spill-policy（供 glob/grep 超限） |
| `boot/` | 应用启动胶水 | app-boot/cmdline |
| `client/` | 浏览器半（ui-*、槽位、连接、主题） | 本次未逐包展开 |

## 五、SDK / ACP / 协议面

### SDK（packages/sdk）

- `protocol`：换行分隔 JSON-RPC 2.0 线协议。方法：`initialize`（serverInfo.name=`deepseek-harness-sdk-runtime`）、`session/prompt`（返回 messageId 入队收据）、`shutdown`；通知：`session.event`、`session.status`、`subagent.started`、`subagent.finished`。
- `server`(`ctx.agents`)：stdio JSON-RPC 服务器，stdout 纯协议帧，`shutdown` 后 dispose 根上下文退出；`initialize.maxTokens` 为 SDK 代理输出上限。
- `client`：TS 客户端（`DeepSeekHarness` 高层 run() + `HarnessClient` 低层）。launch 显式 command/args；stdin-EOF→SIGTERM→SIGKILL 回收阶梯；错误类型 JsonRpcResponseError/RequestTimeoutError/SdkProtocolError/TransportClosedError。另有 Python SDK（python/，镜像形状）。

### ACP（packages/acp）

自动化专用 Agent Client Protocol 服务器（JSON-RPC stdio）。方法：

- `initialize`：基线能力协商（无图/音/嵌入）。
- `authenticate`：no-op（无认证方法）。
- `session/new`：创建新鲜代理（单 cwd）。
- `session/prompt`：等 agent idle，返回 end_turn/cancelled。
- `session/cancel`：取消指定代理。
- `session/update`：提交消息块（agent_message_chunk）。
- `session/request_permission`：一次性 allow/reject。

限制：仅新鲜会话、仅文本、仅提交后消息。

## 六、不确定项标注

1. `vision_glance/vision_ground/...` 等视觉工具**不在本 DSH 仓库**（grep 无匹配），来自部署/技能层（`vision-tools` skill），迁移时需另行溯源，不计入 DSH 静态工具目录。
2. 工具目录的完整性守护覆盖 `packages/*/tool-*`，但 `schedule_*` 三个工具来自 `packages/schedule/schedule`（非 tool-* 命名），属目录手工纳入项——迁移盘点时应按"注册工具名"而非"包名 glob"兜底。
3. MCP 与动态 cordis 插件的工具为**运行时生成**，无法静态穷举；清单中的工具数是"默认组合的静态面"。
4. `client/` 浏览器半（ui-*、槽位、连接、主题）本次仅确认存在，未逐包展开；如迁移目标含 Web GUI 需单独盘点该组。
5. `llm-pi-ai` 的完整协议清单依赖 `@earendil-works/pi-ai` 版本；本报告据其 README 归纳（openai-completions/openai-responses/anthropic/deepseek 等），精确 `supportedProtocols()` 需读源码确认。

## 概览总结

DSH 是一个"全插件化 + 能力缝（Service Definition/Provider/Consumer 三分）"的代理 harness：模型侧暴露约 53 个工具（执行/文件/编排/检索/运行时自修改五大族，MCP 与动态插件再动态扩面）；Host 侧是 HTTP+SSE+WebSocket 的 API 网关（session/workspace/settings/credentials/llm/preset/command/skill 全域 RPC）与 stdio JSON-RPC/ACP 两条跨进程自动化通道；LLM 层以 `ctx.llm` 统一流式词汇，双适配器直连 DeepSeek 或经 pi-ai 聚合多提供商（openai/anthropic/deepseek 等），凭据引用、目录、/models 发现、推理 effort、图像模态、token 计量齐备。迁移到 Rust+Tauri 时，需在 Tauri 侧复刻的核心接缝是：事件溯源会话日志 + 工具注册表/执行管线 + LLM 流式适配器 + 子进程/沙箱/文件系统执行世界 + 持久化/检索/编排服务，以及 HTTP 与 JSON-RPC/ACP 两类对外协议面。
