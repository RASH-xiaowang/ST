# Harness 会话能力清单（与 AI 助手能力对照）

> 状态：2026-08-20 第 96 轮快照（新周期第 46 轮）。本文档把 Harness 会话（ST 应用内 AI 工作台）
> 的能力对照当前 AI 助手逐项列出，并标注验证方式。迁移背景见
> `docs/harness-migration-plan*.md`，维护记录见 `docs/harness-maintenance-log.md`。

## 能力对照表

| AI 助手能力 | Harness 会话实现 | 验证 |
|---|---|---|
| 多轮对话上下文 | 追加式事件日志 → 模型上下文投影（`session.rs::derive_model_messages`） | e2e phase1/2/3、verify-streaming |
| 持久化会话存储与恢复 | SQLite `harness_sessions`/`harness_events`，整页重载回放同源 | e2e phase1（重载回放）、verify-no-duplicate |
| 读文件 | `read_file`（64KB 上限，字符边界安全截断） | phase2、verify-session-maintain |
| 写文件/编辑 | `write_file` / `edit_file`（字面替换，先读后写）/ `str_replace_editor`（DSH 四命令：view 带行号+view_range、create 不覆盖、str_replace 唯一匹配、insert 行后插入；ToolCard 专用卡按命令 diff/视图展示） | phase5（shell→fs 世界统一）、单测（第 21 轮 +2）、**verify-sre-editor 14/14**（第 23 轮人工派发零 LLM 实测） |
| 目录/搜索 | `list_dir` / `glob`（**/\*/? 段匹配）/ `grep`（regex file:line） | phase11 |
| 图像读取 | `read_image`（base64 视觉引用；模型需声明「视觉」标签） | phase11（拒绝非图片） |
| 执行命令 | `exec_command`（PowerShell，工作区锚定，审批门控，超时可配置）+ `run_in_background` 后台作业 | phase2/3/11 |
| 后台任务 | `job_list`/`job_output`/`job_kill`（会话隔离、进程树终止、1h 惰性清理） | phase11 |
| Web 搜索/抓取 | `web_search`（提供商缝：settings 默认 bing、可选 deepseek；Bing 双域兜底 8 条）/ `fetch_web_page`（http/https 白名单，去标签 8KB 截断） | 静态审查确认（第 19 轮，含解析单测） |
| 目标跟踪 | `goal_create`/`goal_get`/`goal_update` + 状态机（active/paused/blocked/complete）+ **自动续跑**（max_goal_rounds） | phase11、phase4 |
| 待办 | `todo_write` + TodoUpdate 事件 + UI 待办卡 | phase4 |
| 计划模式 | `plan_enter`/`plan_exit` + 方案评审流（只读守卫，非只读工具拦截） | phase4（守卫实测） |
| 子代理 | `subagent`（fork 子会话，后台/前台）+ `send_message`/`interrupt_agent`/`subagent_list`/`subagent_output` + 目录树 | phase4（结论 56088） |
| 工作流（固定阶段） | `workflow_list`/`workflow_run`（阶段流水线，前序输出注入） | phase4 |
| **workflow JS 编排（B2）** | `workflow_run_js`：模型编写 JS 脚本，ctx.agent/parallel/pipeline 组合子（前端沙箱执行，子代理经 `harness_workflow_agent` 原语） | phase-b2（parallel 双子代理合并、pipeline 流水线实测） |
| 循环迭代 | `ralph`（固定轮次全新子代理：每轮全新上下文、共享工作区记忆，「已完成/已阻塞」提前结束；子代理经 stub 拒绝无法递归再启） | 静态审查确认（第 18 轮） |
| 提问 | `ask_user_question`（选项/多选/10 分钟超时，可被停止中断） | phase11、verify-tool-timeline |
| 技能 | `skill_list`/`skill_load`（frontmatter 门控 + /skill 手势） | phase11、phase78 |
| 会话管理（自维护） | `session_list/create/rename/clear/delete`（模型可自清） | verify-session-maintain |
| 会话查询/血缘 | `session_search`/`session_trace`/`session_event_*`（5 工具） | phase11 |
| 上下文压缩 | 预算触发自动压缩 + Compaction 事件（跨回合持久）+ 工具结果剪枝 + spill 溢写 | phase6、单测 |
| 附件/多模态 | 文件附加 + 图片注入模型（sha256 内容寻址，最多 4 张） | phase6（附件） |
| MCP/LSP | `mcp_<server>_<tool>` 动态工具、schema 透传、env/cwd 配置；`lsp_hover/definition/references/implementation` | phase6/9 |
| 动态插件/代码运行 | `plugin_*` 5 工具 + `run_code`（前端 WebView 沙箱执行）+ **ctx.tools 无锁桥**（`harness_execute_tool_nolock`，仅前端执行桥用，外层派发已持锁防死锁） | phase-b2（写→读回显 826ms） |
| **会话标题 LLM 生成（B19）** | `harness_generate_title` IPC + 会话行「✨」按钮（手动触发） | 实测（标题→「智能代理工具简介」） |
| 审批/信任 | 审批卡三按钮 + 记住并批准（**参数指纹**：仅同参数免审）+ 逐调用升级审批 | phase2（M8 实测） |
| 沙箱三模式 | read-only / workspace-write / danger-full-access + 越界审批 | phase11（越界列目录） |
| 遥测统计条 | 轮/步/LLM 墙钟/首 token/tok/s/缓存命中/成本 | verify-chat-func |
| 真流式 | SSE 逐 delta 渲染 + Think 推理行 | verify-streaming（31 快照） |
| 语音 | 麦克风输入（电平 VAD 自动停止→STT 云端/本地 Whisper 双引擎）+ 回复播报（TTS 提供方→SAPI 离线兜底） | 静态审查确认（第 20 轮，含空录音拒绝单测） |
| 外部协议 | SDK JSON-RPC（127.0.0.1:4770）+ ACP 7 方法 + CLI | phase6/9/11 |
| 钩子/凭据 | CC 方言 hooks（deny/ask 决策）+ 凭据引用注入 | phase3/9 |

## 工程质量

| 项 | 状态 |
|---|---|
| `cargo test --lib` | **416 passed / 0 failed / 22 ignored** |
| `cargo fmt --check` | 0 diff |
| `svelte-check` | 0 errors / 0 warnings |
| E2E（隔离环境，真实 LLM） | **19/19 探针 ALL_PASS**（含 phase-b2、phase-concurrency、verify-no-duplicate、verify-sre-editor 14/14） |
| 数据安全 | 测试库隔离（cfg!(test)→临时文件）+ E2E 隔离目录（ST_WECHAT_APP_DIR），真实库零污染 |
| 已修复缺陷 | 审查报告 H1-H5、M1-M10、L1-L11 全部落地（见 maintenance-log） |

## 使用方式

- 运行应用：`npm run tauri dev`（或已构建 exe）
- 隔离 E2E：`powershell -File scripts/run-e2e-isolated.ps1 -Probes "phase1,phase4,phase-b2"`
- 数据库备份：`data/backup-control-*.zip` 目录（含 WAL 合并）

## 已知边界 / 后续方向

- B2 workflow JS 编排结果展示：已由 ToolCard 专用卡（tc-workflow 结构化
  JSON + 日志前缀剥离）覆盖（第 22 轮）；完整 WorkflowRunPanel 形态
  （阶段节点点击打开成员会话）未做（等价降级）
- e2b 云沙箱 / telemetry otel / session-log ZIP（明确不迁移）
