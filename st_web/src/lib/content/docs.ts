import type { Bi } from "@/lib/i18n/locales";

/** 文档中心：导航结构 + 各指南页内容（zh/en 双语） */

export type DocSection = {
  heading: Bi<string>;
  body: Bi<string[]>;
  code?: { title: Bi<string>; source: string };
};

export type DocPage = {
  slug: string;
  title: Bi<string>;
  summary: Bi<string>;
  order: number;
  group: Bi<string>;
  sections: DocSection[];
};

export const docGroups: Bi<string>[] = [
  { zh: "快速开始", en: "Getting Started" },
  { zh: "核心概念", en: "Core Concepts" },
  { zh: "开发者", en: "Developers" },
  { zh: "法律与政策", en: "Legal & Policy" },
];

export const docPages: DocPage[] = [
  {
    slug: "getting-started",
    group: { zh: "快速开始", en: "Getting Started" },
    order: 1,
    title: { zh: "快速开始", en: "Quick Start" },
    summary: {
      zh: "十分钟内完成安装、配置模型并发出第一条流式对话。",
      en: "Install, configure a model and send your first streaming message in ten minutes.",
    },
    sections: [
      {
        heading: { zh: "安装", en: "Installation" },
        body: {
          zh: [
            "下载对应平台的安装包（Windows 优先），双击完成安装。应用为桌面原生（Tauri 2），无需额外运行时。",
            "系统要求：Windows 10 1809+ / 8GB 内存 / 任意支持 WebGL2 的现代浏览器内核。",
          ],
          en: [
            "Download the installer for your platform (Windows first) and double-click. The app is a native desktop binary (Tauri 2) — no extra runtime needed.",
            "Requirements: Windows 10 1809+, 8GB RAM, any modern WebView2-capable browser engine.",
          ],
        },
      },
      {
        heading: { zh: "配置模型", en: "Configure a model" },
        body: {
          zh: [
            "打开「大模型 → 接入配置」，添加提供方（OpenAI 兼容 / Azure / Ollama / 自定义），填入端点与密钥，保存后在 Harness 顶部选择提供方与模型。",
          ],
          en: [
            "Open Models → Providers, add a provider (OpenAI-compatible / Azure / Ollama / custom), fill in endpoint and key, then pick provider and model in the Harness header.",
          ],
        },
        code: {
          title: { zh: "Ollama 本地模型示例", en: "Local model via Ollama" },
          source: `# 本地启动 Ollama
ollama serve

# 拉取模型
ollama pull qwen2.5:7b

# 在 Harness 中新增提供方：
# 类型 = 自定义 · 端点 = http://127.0.0.1:11434/v1`,
        },
      },
      {
        heading: { zh: "第一句话", en: "First message" },
        body: {
          zh: [
            "进入 Harness 会话，输入消息按 Enter 发送。回复逐字流式呈现；若模型调用工具，工具执行时间线会显示在回复上方，点击步骤可查看参数与结果。",
          ],
          en: [
            "Open the Harness session, type a message and press Enter. Replies stream token-by-token; when the model calls tools, an execution timeline appears above the reply — click any step to inspect args and results.",
          ],
        },
      },
    ],
  },
  {
    slug: "api",
    group: { zh: "开发者", en: "Developers" },
    order: 3,
    title: { zh: "本地 JSON-RPC 与 ACP 协议", en: "Local JSON-RPC & ACP Protocol" },
    summary: {
      zh: "通过本地 JSON-RPC（127.0.0.1:4770 /rpc，仅本机）驱动会话、工具与用量；ACP 自动化入口复用同一通道。",
      en: "Drive sessions, tools and usage through local JSON-RPC (127.0.0.1:4770 /rpc, loopback only); the ACP automation entry reuses the same channel.",
    },
    sections: [
      {
        heading: { zh: "端点与传输", en: "Endpoint & transport" },
        body: {
          zh: [
            "SDK 服务随应用启动监听 127.0.0.1:4770（无鉴权、仅本机）。健康检查 GET /health 返回 ok；所有方法经 POST /rpc 以 JSON-RPC 2.0 请求。",
          ],
          en: [
            "The SDK service listens on 127.0.0.1:4770 at app startup (no auth, loopback only). GET /health returns ok; all methods go through POST /rpc as JSON-RPC 2.0.",
          ],
        },
        code: {
          title: { zh: "JSON-RPC 请求", en: "JSON-RPC request" },
          source: `curl -X POST http://127.0.0.1:4770/rpc \\
  -H "Content-Type: application/json" \\
  -d '{"jsonrpc":"2.0","id":1,"method":"sessions.list","params":{}}'

# 同步执行一轮对话（返回最终回答）
{"jsonrpc":"2.0","id":2,"method":"session.chat",
 "params":{"session_id":"<sid>","prompt":"列出过期的依赖"}}`,
        },
      },
      {
        heading: { zh: "方法总览", en: "Method overview" },
        body: {
          zh: [
            "会话：sessions.list（列表）、session.create（新建）、session.display（消息投影）、session.state（运行状态）、session.chat（同步执行一轮对话并返回最终回答）。",
            "工具与用量：tool.execute（不经模型直接派发一次工具调用）、usage.get（会话用量遥测）。",
            "ACP 语义：initialize / authenticate（握手与鉴权探测）、session/new（带 goal 创建）、session/prompt（发送提示词，stopReason=end_turn 返回完整回答）、session/update（同步模式下的流式等价：返回 agent_message_chunk）、session/cancel（中断进行中回合）、session/request_permission（approve / reject 一次性决策）。",
          ],
          en: [
            "Sessions: sessions.list, session.create, session.display (message projection), session.state (run state) and session.chat (runs one synchronous turn and returns the final answer).",
            "Tools & usage: tool.execute dispatches a tool call without the model; usage.get returns session telemetry.",
            "ACP semantics: initialize / authenticate (handshake), session/new (create with a goal), session/prompt (send a prompt; returns the full answer at stopReason=end_turn), session/update (the synchronous equivalent of streaming: returns agent_message_chunk), session/cancel (interrupts an in-flight turn) and session/request_permission (one-shot approve / reject).",
          ],
        },
      },
      {
        heading: { zh: "ACP 自动化入口", en: "ACP automation entry" },
        body: {
          zh: [
            "同一 /rpc 通道提供 ACP 语义：initialize 返回能力声明（loadSession/prompt/cancel/stream/requestPermission）、authenticate 返回本机免鉴权；session/new（含 goal）创建自动化会话、session/prompt 发送提示词并在 stopReason=end_turn 时返回完整回答、session/update 返回同步的 agent_message_chunk、session/cancel 中断进行中的回合、session/request_permission 对审批请求做一次性 approve / reject（当前为同步模式）。",
          ],
          en: [
            "The same /rpc channel exposes ACP semantics: initialize advertises capabilities (loadSession/prompt/cancel/stream/requestPermission), authenticate confirms loopback-only access; session/new (with goal) creates an automation session, session/prompt sends a prompt and returns the full answer at stopReason=end_turn, session/update returns a synchronous agent_message_chunk, session/cancel interrupts an in-flight turn and session/request_permission answers an approval request with a one-shot approve / reject (synchronous mode for now).",
          ],
        },
        code: {
          title: { zh: "ACP 会话示例", en: "ACP session example" },
          source: `# 创建自动化会话并设定目标
{"jsonrpc":"2.0","id":3,"method":"session/new","params":{"goal":"分析依赖"}}

# 发送提示词（end_turn 返回完整回答）
{"jsonrpc":"2.0","id":4,"method":"session/prompt",
 "params":{"session_id":"<sid>","prompt":"列出过期的依赖"}}`,
        },
      },
    ],
  },
  {
    slug: "wechat-data",
    group: { zh: "核心概念", en: "Core Concepts" },
    order: 2,
    title: { zh: "微信数据能力", en: "WeChat Data Capabilities" },
    summary: {
      zh: "只读分析本机解密后的微信数据库：会话、消息、朋友圈、撤回、转账/红包与存储构成，图片多通道解析。",
      en: "Read-only analysis of your decrypted local WeChat databases: sessions, messages, moments, recalls, transfers/red packets, storage composition and multi-channel image resolution.",
    },
    sections: [
      {
        heading: { zh: "数据来源与安全", en: "Data sources & safety" },
        body: {
          zh: [
            "应用只读访问解密副本（message/message_*.db、session/session.db、contact.db、general.db、sns.db、bizchat 等），所有密钥与解密过程都在本机完成，绝不写入原始数据。",
            "旧版 `%APPDATA%\\st_result` 数据可一键迁移到统一 data 目录；启动时幂等执行 legacy 迁移并备份旧目录。",
          ],
          en: [
            "The app only reads decrypted copies (message/message_*.db, session/session.db, contact.db, general.db, sns.db, bizchat, …). Keys and decryption stay local; original data is never written.",
            "Legacy `%APPDATA%\\st_result` data migrates to the unified data directory on startup (idempotent) with the old directory kept as a backup.",
          ],
        },
      },
      {
        heading: { zh: "记录与统计", en: "Records & statistics" },
        body: {
          zh: [
            "会话/群聊/好友/朋友圈浏览与全文检索；撤回消息缓存、转账/红包记录、视频号直播、小程序、好友验证（general.db 记录类查询）。",
            "年度总结（消息量/活跃天数/热力图/词频/表情）、每日总结（群聊 × 成员定时生成）、存储空间构成与隐私扫描（导出记录扫描）。",
          ],
          en: [
            "Browse and full-text search sessions, groups, contacts and moments; recalled-message cache, transfer/red-packet records, finder streams, mini-programs and friend verifications (general.db record queries).",
            "Annual summaries (volume/active days/heatmap/phrases/emoji), scheduled daily summaries per group × members, storage composition and a privacy export-records scan.",
          ],
        },
      },
      {
        heading: { zh: "图片多通道解析", en: "Multi-channel image resolution" },
        body: {
          zh: [
            "本地 DAT/解密解码优先；CDN 原图回退（本地 AES-ECB 或服务端解密）；ilink 官方通道回退（版本护栏 + 隔离沙箱）；朋友圈 ISAAC-64 XOR 解密；HEVC(wxgf) 经 Media Foundation 转 JPEG。",
            "每个通道失败都优雅降级，绝不阻塞消息浏览；原图按 fileid/md5 落盘缓存。",
          ],
          en: [
            "Local DAT/decode first; CDN original-image fallback (local AES-ECB or server-side decrypt); ilink official-channel fallback (version guard + isolated sandbox); moments ISAAC-64 XOR decryption; HEVC(wxgf) to JPEG via Media Foundation.",
            "Every channel degrades gracefully without blocking message browsing; originals are cached by fileid/md5 on disk.",
          ],
        },
      },
    ],
  },
  {
    slug: "kb",
    group: { zh: "核心概念", en: "Core Concepts" },
    order: 3,
    title: { zh: "知识库与 RAG", en: "Knowledge Base & RAG" },
    summary: {
      zh: "多格式文档导入、分块、向量 + BM25 混合检索与流式问答；Wiki、FAQ、ACL 与异步作业。",
      en: "Multi-format import, chunking, vector + BM25 hybrid retrieval and streaming Q&A, plus Wiki, FAQ, ACLs and async jobs.",
    },
    sections: [
      {
        heading: { zh: "文档与分块", en: "Documents & chunking" },
        body: {
          zh: [
            "docs 支持上传/多版本/恢复与目录管理，解析涵盖 PDF（含 OCR 回退）、Word、xlsx、Markdown 等；分块与重处理以异步作业执行，按分片策略生成可检索 chunk。",
            "访问控制按知识库/文档 ACL（成员/角色）管理，检索结果只返回有权限的片段。",
          ],
          en: [
            "Docs support upload, versioning/restore and directory management; parsing covers PDF (with OCR fallback), Word, xlsx, Markdown and more. Chunking and reprocessing run as async jobs with a configurable sharding strategy.",
            "Access is governed by per-KB/doc ACLs (members/roles); retrieval only returns fragments you can read.",
          ],
        },
      },
      {
        heading: { zh: "检索与问答", en: "Retrieval & Q&A" },
        body: {
          zh: [
            "chunks 双路检索：BM25 全文 + 向量相似度，RAG 流式回答并给出引用来源；FAQ 精确匹配与检索历史沉淀常用问答。",
            "Wiki 提供页面 CRUD、链接图、自动提炼与全文索引，让知识从文档沉淀为可复用条目。",
          ],
          en: [
            "Dual-path chunk retrieval: BM25 full-text plus vector similarity; RAG streams answers with cited sources; FAQ exact-match and search history capture recurring Q&A.",
            "Wiki adds page CRUD, link graphs, auto-extraction and BM25 indexing to distill reusable entries from documents.",
          ],
        },
      },
    ],
  },
  {
    slug: "harness-sessions",
    group: { zh: "核心概念", en: "Core Concepts" },
    order: 4,
    title: { zh: "会话、日志与回放", en: "Sessions, Logs & Replay" },
    summary: {
      zh: "追加式事件日志是唯一上下文来源：渲染、回放、分叉与导出同源；会话级预设与 AI 角色注入。",
      en: "The append-only event log is the single source of context: render, replay, fork and export all project from it; sessions carry per-session presets and role injection.",
    },
    sections: [
      {
        heading: { zh: "事件日志与投影", en: "Event log & projection" },
        body: {
          zh: [
            "每个会话一条追加式事件流：用户消息、助手分片/消息、工具调用与结果、计划/目标/待办、角色注入、压缩与清空事件。UI 不单独存消息，任何状态都能从日志重建。",
            "模型可见即落日志：进入模型请求的内容必然来自日志投影，这是一条运行时不变量。",
          ],
          en: [
            "Each session is an append-only event stream: user messages, assistant chunks/messages, tool calls and results, plan/goal/todo updates, role injection, compaction and clear events. The UI never stores messages separately — any state rebuilds from the log.",
            "“Model-visible is logged”: anything reaching a model request must project from the log; this is a runtime invariant.",
          ],
        },
      },
      {
        heading: { zh: "分叉、导出与自维护", en: "Fork, export & self-maintenance" },
        body: {
          zh: [
            "会话可按边界 seq 分叉（SessionForked 落日志可溯源）、导出为 Markdown 转写；支持清空/删除/重命名与全文检索。",
            "模型具备 session_list/create/rename/clear/delete 自维护工具；每会话可指定预设覆盖与 AI 角色（原「AI 聊天」角色注入迁移）。",
          ],
          en: [
            "Fork a session at a boundary seq (SessionForked is logged for lineage), export Markdown transcripts; clear/delete/rename and full-text search included.",
            "The agent has session_list/create/rename/clear/delete self-maintenance tools; each session can carry a preset override and an AI role (migrated from the original AI chat roles).",
          ],
        },
      },
    ],
  },
  {
    slug: "harness-tools",
    group: { zh: "核心概念", en: "Core Concepts" },
    order: 5,
    title: { zh: "工具、审批与守卫", en: "Tools, Approvals & Guards" },
    summary: {
      zh: "50+ 内置工具 + 动态扩展；作用域注册表、守卫管道、审批卡与计划模式只读守卫。",
      en: "50+ built-in tools plus dynamic extensions; a scoped registry, guarded pipeline, approval cards and a plan-mode read-only guard.",
    },
    sections: [
      {
        heading: { zh: "工具目录", en: "Tool catalog" },
        body: {
          zh: [
            "内置工具覆盖：文件（read/write/list/glob/grep/edit）、命令（exec_command 前台/后台）、网络（web_search/fetch_web_page）、会话（todo/plan/goal/session_*）、编排（task 子代理、workflow、schedule、job_*）、终端（terminal_*）、LSP（hover/definition/references/implementation）、技能（skill_list/load）、动态插件与 run_code。",
            "工具目录带参数 Schema 与搜索；预设可禁用工具、覆盖审批与超时、注入提示词分区。",
          ],
          en: [
            "Built-ins span files (read/write/list/glob/grep/edit), commands (exec_command foreground/background), web (web_search/fetch_web_page), sessions (todo/plan/goal/session_*), orchestration (task subagent, workflow, schedule, job_*), terminals, LSP (hover/definition/references/implementation), skills (skill_list/load), dynamic plugins and run_code.",
            "The catalog is schema-aware and searchable; presets can disable tools, override approvals/timeouts and inject prompt sections.",
          ],
        },
      },
      {
        heading: { zh: "守卫与审批", en: "Guards & approvals" },
        body: {
          zh: [
            "工具调用依次经过：会话级拦截 → 计划模式/只读守卫 → PreToolUse 决策钩子（deny/ask）→ 审批门控（会话内信任 30 分钟 TTL）→ 超时守卫执行。",
            "计划模式仅放行只读工具；沙箱「只读」模式同样拦截执行类工具；越界访问需逐调用升级审批。",
          ],
          en: [
            "A call passes through: session-level interception → plan-mode/read-only guard → PreToolUse decision hooks (deny/ask) → approval gate (30-min per-session trust TTL) → timeout-guarded execution.",
            "Plan mode allows read-only tools only; the read-only sandbox blocks mutating tools too; out-of-bounds access requires per-call escalation approval.",
          ],
        },
      },
    ],
  },
  {
    slug: "harness-execution",
    group: { zh: "核心概念", en: "Core Concepts" },
    order: 6,
    title: { zh: "执行世界与编排", en: "Execution World & Orchestration" },
    summary: {
      zh: "shell / fs / ConPTY 终端与沙箱策略；子代理、工作流、定时任务与后台作业。",
      en: "Shell, fs, ConPTY terminals and sandbox policy; subagents, workflows, schedules and background jobs.",
    },
    sections: [
      {
        heading: { zh: "执行世界", en: "The execution world" },
        body: {
          zh: [
            "shell 经临时文件防死锁执行并支持超时终止；fs 读写受工作区沙箱约束（默认工作区 = 应用项目根）；ConPTY 真终端保持 cwd 与进程树终止，旧系统自动降级为非 PTY 状态保持模式。",
            "沙箱三模式（只读/工作区写/全权）统一约束 shell、fs 与终端；凭证经 HARNESS_CREDENTIAL_<KEY> 注入子进程（.env 提供者）。",
          ],
          en: [
            "Shell executes via temp-file redirection (deadlock-free) with timeout kill; fs reads/writes are constrained by the workspace sandbox (default root = the app project); ConPTY terminals keep cwd and process-tree termination, degrading to stateful non-PTY mode on older systems.",
            "The three-tier sandbox (read-only / workspace-write / full) uniformly constrains shell, fs and terminals; credentials inject into child processes as HARNESS_CREDENTIAL_<KEY> (with a .env provider).",
          ],
        },
      },
      {
        heading: { zh: "编排能力", en: "Orchestration" },
        body: {
          zh: [
            "task 子代理（全新上下文 + 独立工具循环）、workflow 分阶段流水线、schedule 定时任务（30 秒调度器）、exec_command 后台作业（job_list/output/kill）、goal/plan/todo 会话状态机。",
            "所有编排动作都落追加式日志（GoalSet/PlanEnter/TodoUpdate/workflow_run 等事件），UI 从日志投影横幅与待办卡。",
          ],
          en: [
            "The task subagent (fresh context + its own tool loop), staged workflows, schedule jobs (30-second scheduler), background exec jobs (job_list/output/kill) and the goal/plan/todo session state machine.",
            "Every orchestration action lands in the append-only log (GoalSet/PlanEnter/TodoUpdate/workflow_run events); the UI projects banners and todo cards from it.",
          ],
        },
      },
    ],
  },
  {
    slug: "harness-extensions",
    group: { zh: "核心概念", en: "Core Concepts" },
    order: 7,
    title: { zh: "扩展生态：技能、插件与连接器", en: "Extensions: Skills, Plugins & Connectors" },
    summary: {
      zh: "技能、动态插件（extensions）、run_code（code-runtime）、MCP、LSP、钩子、凭据、存储与反馈。",
      en: "Skills, dynamic plugins (extensions), run_code (code-runtime), MCP, LSP, hooks, credentials, storage and feedback.",
    },
    sections: [
      {
        heading: { zh: "技能与动态插件", en: "Skills & dynamic plugins" },
        body: {
          zh: [
            "技能：data/harness/skills/<id>/SKILL.md 目录约定，skill_list/skill_load 供模型读取执行说明。",
            "动态插件（DSH extensions）：模型可经 plugin_list/plugin_define 定义、plugin_enable/disable 启停、plugin_delete 移除；工具代码在前端 WebView 沙箱执行（async 函数体 + ctx.fetch/ctx.log）。",
            "run_code（DSH code-runtime）：模型编写 async 函数体，前端沙箱运行并回传日志与返回值。",
          ],
          en: [
            "Skills follow a data/harness/skills/<id>/SKILL.md directory convention; skill_list/skill_load let the agent read and apply them.",
            "Dynamic plugins (DSH extensions): the model defines them via plugin_list/plugin_define, toggles with plugin_enable/disable and removes with plugin_delete; tool code runs in the WebView sandbox (async body + ctx.fetch/ctx.log).",
            "run_code (DSH code-runtime): the model writes an async function body that runs in the frontend sandbox and returns logs plus the return value.",
          ],
        },
      },
      {
        heading: { zh: "连接器与治理", en: "Connectors & governance" },
        body: {
          zh: [
            "MCP stdio 客户端（initialize/tools/list/call）把外部工具注册为 mcp_<server>_<tool>；LSP stdio 客户端提供 hover/定义/引用；钩子桥在 turn_start/turn_end/tool_executed 触发外部命令。",
            "配置束可整体导出/导入（预设 + 技能 + MCP + LSP + 钩子）；反馈（好/差评 + 评论）与 KV 存储（SQLite）落地；上下文溢写在压缩前把完整转录写盘。",
          ],
          en: [
            "The MCP stdio client (initialize/tools/list/call) registers external tools as mcp_<server>_<tool>; the LSP stdio client offers hover/definition/references; the hook bridge fires external commands at turn_start/turn_end/tool_executed.",
            "Config bundles export/import presets + skills + MCP + LSP + hooks in one file; feedback (like/dislike + comments) and KV storage persist locally; context spill writes full transcripts before compaction.",
          ],
        },
      },
    ],
  },
  {
    slug: "sdk-cli",
    group: { zh: "开发者", en: "Developers" },
    order: 2,
    title: { zh: "SDK 与 CLI", en: "SDK & CLI" },
    summary: {
      zh: "本地 JSON-RPC SDK、Harness CLI 与配置束工具；自动化集成十分钟起步。",
      en: "The local JSON-RPC SDK, the HARNESS CLI and bundle tooling — automation integration in ten minutes.",
    },
    sections: [
      {
        heading: { zh: "SDK 方法", en: "SDK methods" },
        body: {
          zh: [
            "POST /rpc 提供 sessions.list、session.create、session.display、session.state、session.chat、tool.execute、usage.get；GET /health 健康检查。",
            "ACP 语义（initialize / authenticate、session/new、session/prompt、session/update、session/cancel、session/request_permission）复用同一通道，供外部自动化程序驱动代理。",
          ],
          en: [
            "POST /rpc exposes sessions.list, session.create, session.display, session.state, session.chat, tool.execute and usage.get; GET /health for health checks.",
            "ACP semantics (initialize / authenticate, session/new, session/prompt, session/update, session/cancel, session/request_permission) reuse the same channel for external automation.",
          ],
        },
      },
      {
        heading: { zh: "CLI 与配置束", en: "CLI & bundles" },
        body: {
          zh: [
            "Harness CLI：sessions list / session create / session chat <id> <文本> / session show <id> / tools list / usage <id>。",
            "配置束导出到文件或剪贴板，按 id 合并导入（同 id 覆盖），一条命令迁移整套 Harness 配置。",
          ],
          en: [
            "The HARNESS CLI: sessions list / session create / session chat <id> <text> / session show <id> / tools list / usage <id>.",
            "Export a config bundle to file or clipboard and import it by merging on id (same-id overwrites) — migrate an entire HARNESS setup in one command.",
          ],
        },
      },
    ],
  },
  {
    slug: "architecture",
    group: { zh: "核心概念", en: "Core Concepts" },
    order: 8,
    title: { zh: "架构与日志投影", en: "Architecture & Log Projection" },
    summary: {
      zh: "「模型可见即落日志」：渲染、回放与导出共享同一份追加式事件流。",
      en: "“Model-visible is logged”: render, replay and export share one append-only event stream.",
    },
    sections: [
      {
        heading: { zh: "事件日志", en: "The event log" },
        body: {
          zh: [
            "每个会话对应一条追加式事件流：用户消息、助手流式块、工具调用、工具结果、计划/目标/待办更新、角色注入与清空事件。UI 从不单独保存消息——它从日志投影，任何状态都能重建。",
          ],
          en: [
            "Each session is an append-only event stream: user messages, assistant chunks, tool calls and results, plan/goal/todo updates, role injection and clear events. The UI never stores messages separately — it projects from the log, so any state can be rebuilt.",
          ],
        },
      },
      {
        heading: { zh: "工具管道", en: "The tool pipeline" },
        body: {
          zh: [
            "模型发出工具调用后依次经过：会话级拦截（会话维护/子代理）→ 计划模式与只读守卫 → 决策钩子（PreToolUse）→ 审批门控 → 超时守卫执行。结果落日志并注入下一轮上下文。",
          ],
          en: [
            "A tool call passes through: session-level interception (session maintenance / subagents) → plan-mode & read-only guard → decision hooks (PreToolUse) → approval gate → timeout-guarded execution. Results land in the log and feed the next round.",
          ],
        },
      },
    ],
  },
  {
    slug: "governance",
    group: { zh: "核心概念", en: "Core Concepts" },
    order: 9,
    title: { zh: "治理中心指南", en: "Governance Center Guide" },
    summary: {
      zh: "预设、钩子、沙箱模式与审批卡：控制模型能做什么。",
      en: "Presets, hooks, sandbox modes and approval cards: control what the model can do.",
    },
    sections: [
      {
        heading: { zh: "沙箱三模式", en: "Three-tier sandbox" },
        body: {
          zh: [
            "只读：仅允许只读工具；工作区写：可在工作区内读写（默认工作区=应用项目根）；全权：越界访问，需逐调用升级审批。",
          ],
          en: [
            "Read-only: read-only tools only. Workspace-write: read/write within the workspace (default root = the app project). Full: out-of-bounds access with per-call approved escalation.",
          ],
        },
      },
      {
        heading: { zh: "预设与钩子", en: "Presets & hooks" },
        body: {
          zh: [
            "预设组合工具禁用清单、逐工具超时、提示词分区与审批覆盖。钩子以 CC/Codex 方言在事件点执行命令，PreToolUse 支持 deny/ask 决策拦截。",
          ],
          en: [
            "Presets combine disabled-tool lists, per-tool timeouts, prompt sections and approval overrides. Hooks run commands at event points (CC/Codex dialect); PreToolUse supports deny/ask decisions.",
          ],
        },
      },
    ],
  },
  {
    slug: "wechat-http-api",
    group: { zh: "开发者", en: "Developers" },
    order: 4,
    title: { zh: "微信数据 HTTP API", en: "WeChat Data HTTP API" },
    summary: {
      zh: "本机微信数据只读 HTTP 接口（默认 127.0.0.1:5032）：会话 / 消息 / 联系人 / 群成员 / 媒体直链 / 监控 / 自动化任务，附 SSE 实时推送与 OpenAPI 自描述文档。",
      en: "A loopback read-only HTTP API (127.0.0.1:5032 by default) for your decrypted WeChat data: sessions, messages, contacts, group members, media links, monitoring and automation tasks, plus SSE push and a self-describing OpenAPI document.",
    },
    sections: [
      {
        heading: { zh: "端点与启用", en: "Endpoint & enabling" },
        body: {
          zh: [
            "服务仅监听 127.0.0.1（默认端口 5032，可在微信配置中修改，端口热更新平滑迁移）。需先在设置中开启「HTTP API」总开关；GET /health 与 /api/v1/health 不经过开关，始终可用。",
            "鉴权：未配置令牌时免鉴权；配置后支持三种传递方式——Authorization: Bearer <token>、查询参数 ?access_token=、请求体 access_token 字段。",
            "CORS 仅放行应用自身来源（localhost:1420 / tauri://localhost），外部网页无法跨域读取本地数据。",
          ],
          en: [
            "The service listens on 127.0.0.1 only (port 5032 by default; configurable in WeChat settings with graceful port hot-swap). Enable the HTTP API master switch in settings first; GET /health and /api/v1/health bypass the switch and always work.",
            "Auth: with no token configured, requests are unauthenticated; with a token, three forms are accepted — Authorization: Bearer <token>, ?access_token= query parameter, or an access_token field in the body.",
            "CORS allows only the app's own origins (localhost:1420 / tauri://localhost), so external web pages cannot read your local data cross-origin.",
          ],
        },
      },
      {
        heading: { zh: "数据接口", en: "Data endpoints" },
        body: {
          zh: [
            "GET/POST /api/v1/sessions：会话列表（keyword / limit / offset 过滤分页）。",
            "GET/POST /api/v1/messages：会话消息（talker 必填；cursor 游标分页、时间范围与关键词过滤；时间参数支持秒/毫秒时间戳与 YYYYMMDD）。",
            "GET /api/v1/sessions/{id}/messages：按 since 时间戳增量拉取，sync 分页块返回。",
            "GET/POST /api/v1/contacts：联系人列表（category / keyword 过滤）；GET/POST /api/v1/group-members：群成员列表（chatroomId 必填）。",
          ],
          en: [
            "GET/POST /api/v1/sessions: session list (keyword / limit / offset filtering & paging).",
            "GET/POST /api/v1/messages: session messages (talker required; cursor paging, time-range and keyword filters; timestamps accept seconds, milliseconds or YYYYMMDD).",
            "GET /api/v1/sessions/{id}/messages: incremental pull since a timestamp with sync paging blocks.",
            "GET/POST /api/v1/contacts: contact list (category / keyword); GET/POST /api/v1/group-members: group members (chatroomId required).",
          ],
        },
      },
      {
        heading: { zh: "媒体接口", en: "Media endpoints" },
        body: {
          zh: [
            "GET /api/v1/media/{username}/{local_id}：图片按需即时解密（含 wxgf 转码）；/api/v1/media/video/{username}/{local_id} 与 /thumb：视频与封面。",
            "GET /api/v1/sns/video/{file_key}：朋友圈视频（本地解密 MP4，支持 Range 断点）；/api/v1/emoticon/{md5}：表情图。",
            "GET /api/v1/file/image/{md5}、/api/v1/file/video/{md5}、/api/v1/file/video/thumb/{md5}：按 md5 取文件媒体。",
          ],
          en: [
            "GET /api/v1/media/{username}/{local_id}: on-demand image decryption (incl. wxgf transcoding); /api/v1/media/video/{username}/{local_id} and /thumb: videos and covers.",
            "GET /api/v1/sns/video/{file_key}: moments videos (locally decrypted MP4, Range supported); /api/v1/emoticon/{md5}: stickers.",
            "GET /api/v1/file/image/{md5}, /api/v1/file/video/{md5} and /api/v1/file/video/thumb/{md5}: file media by md5.",
          ],
        },
      },
      {
        heading: { zh: "监控与实时推送", en: "Monitoring & live push" },
        body: {
          zh: [
            "GET /api/v1/monitor/status：消息监控运行状态、uptime、WebSocket 端口与指标（pendingAcks / sentTotal / 延迟分桶）。",
            "GET /api/v1/push/messages：SSE 实时推送新消息，事件名 message.new / message.batch / message.revoke；支持 Last-Event-ID 头或 since_ack 参数断线补推，15 秒 keepalive。",
          ],
          en: [
            "GET /api/v1/monitor/status: monitor running state, uptime, WebSocket port and metrics (pendingAcks / sentTotal / latency buckets).",
            "GET /api/v1/push/messages: SSE live push with message.new / message.batch / message.revoke events; replays missed events via Last-Event-ID or since_ack, with 15-second keepalive.",
          ],
        },
      },
      {
        heading: { zh: "自动化任务与自描述文档", en: "Automation tasks & self-description" },
        body: {
          zh: [
            "GET /api/v1/automation/tasks：自动化任务列表；POST /api/v1/automation/tasks/claim、/start、/complete：领取、启动与完成（如消息监控驱动的任务流）。",
            "GET /api/v1/openapi.json：OpenAPI 3.0 描述（标题「ST 控制台 · 微信数据 HTTP API」，含全部路径摘要），应用内「API 文档」界面即由此动态渲染。",
          ],
          en: [
            "GET /api/v1/automation/tasks: task list; POST /api/v1/automation/tasks/claim, /start and /complete: claim, start and finish tasks (e.g. flows driven by the message monitor).",
            "GET /api/v1/openapi.json: an OpenAPI 3.0 description (title “ST Console · WeChat Data HTTP API”) covering every path; the in-app “API docs” view renders from it.",
          ],
        },
      },
      {
        heading: { zh: "调用示例", en: "Example calls" },
        body: {
          zh: [
            "以下命令假设已开启 API 且未配置令牌（配置令牌时加 -H \"Authorization: Bearer <token>\"）。",
          ],
          en: [
            "The commands below assume the API is enabled without a token (add -H \"Authorization: Bearer <token>\" when configured).",
          ],
        },
        code: {
          title: { zh: "微信数据 HTTP API 示例", en: "WeChat HTTP API examples" },
          source: `# 健康检查
curl http://127.0.0.1:5032/health

# 会话列表（含关键词与分页）
curl "http://127.0.0.1:5032/api/v1/sessions?keyword=项目&limit=20&offset=0"

# 指定会话的消息（talker 必填，cursor 分页）
curl "http://127.0.0.1:5032/api/v1/messages?talker=<username>&cursor=0&limit=50"

# 增量拉取（since 秒级时间戳）
curl "http://127.0.0.1:5032/api/v1/sessions/<id>/messages?since=1753718400"

# 群成员（chatroomId 必填）
curl "http://127.0.0.1:5032/api/v1/group-members?chatroomId=<id>"

# OpenAPI 自描述文档（应用内「API 文档」同源）
curl http://127.0.0.1:5032/api/v1/openapi.json`,
        },
      },
    ],
  },
  {
    slug: "ocr-http-api",
    group: { zh: "开发者", en: "Developers" },
    order: 5,
    title: { zh: "图文识别 HTTP API", en: "OCR Ingest HTTP API" },
    summary: {
      zh: "图文识别资源接收服务（默认 0.0.0.0:9787）：POST /api/ocr/ingest 投递图片/PDF，异步管线完成预检、分类、OCR 与入库。",
      en: "The OCR ingest service (0.0.0.0:9787 by default): POST /api/ocr/ingest hands over images/PDFs, and an async pipeline runs precheck, classification, OCR and persistence.",
    },
    sections: [
      {
        heading: { zh: "端点与启用", en: "Endpoint & enabling" },
        body: {
          zh: [
            "默认绑定 0.0.0.0:9787（bind_host / port 均可配置）。注意默认 0.0.0.0 表示监听所有网卡——部署在可信网络时请务必配置访问令牌，或将绑定改回 127.0.0.1。",
            "鉴权：配置令牌后接受 Authorization: Bearer <token> 或请求体 access_token 字段；CORS 仅放行应用自身来源，拒绝任意站点跨域投递。",
            "GET /api/ocr/health：返回服务状态（configured 是否已配凭据）与入库统计（总数 / 按状态分布）。",
          ],
          en: [
            "Binds to 0.0.0.0:9787 by default (both bind_host and port are configurable). Note that 0.0.0.0 listens on every interface — on a trusted network, configure an access token or switch the bind back to 127.0.0.1.",
            "Auth: with a token configured, Authorization: Bearer <token> or an access_token body field is accepted; CORS allows only the app's own origins, blocking cross-origin delivery from arbitrary sites.",
            "GET /api/ocr/health: service status (configured: whether credentials exist) and ingest statistics (total / by status).",
          ],
        },
      },
      {
        heading: { zh: "投递资源", en: "Ingesting a resource" },
        body: {
          zh: [
            "POST /api/ocr/ingest，JSON 必填字段：sender_username（发送者）、session_type（会话类型）、timestamp、username（资源归属）、mediaUrl（媒体地址）。",
            "mediaUrl 支持：http(s) 下载、data: URL（含 base64）、file:// 或本地路径、builtin://（内置测试图）；单资源上限 10MB。",
            "成功返回 202 Accepted：{ \"id\": <入库ID>, \"status\": \"pending\" }，处理异步进行，状态经事件推送可追踪。",
          ],
          en: [
            "POST /api/ocr/ingest with JSON required fields: sender_username, session_type, timestamp, username and mediaUrl.",
            "mediaUrl accepts http(s) downloads, data: URLs (incl. base64), file:// or local paths, and builtin:// (embedded test images); single resources are capped at 10MB.",
            "Success returns 202 Accepted: { \"id\": <id>, \"status\": \"pending\" }; processing runs asynchronously and progress is tracked via events.",
          ],
        },
      },
      {
        heading: { zh: "处理管线", en: "Processing pipeline" },
        body: {
          zh: [
            "下载/读取资源 → 存入 incoming 暂存 → 开源 OCR 预检（图片无有效文本则直接过滤归档，省去付费分类调用）→ TextIn 证件分类 → 按分类目录归档 → 调用对应分类的 OCR 接口 → 结果入库（成功 / ocr_failed / filtered 状态）→ 事件推送。",
            "未配置某分类的 OCR 接口时仅完成分类归档；OCR 失败保留可重试状态，不影响分类结果。",
          ],
          en: [
            "Fetch the resource → stage under incoming → open-source OCR precheck (images without usable text are filtered and archived directly, saving paid classification calls) → TextIn document classification → archive by category → call the category's OCR endpoint → persist the result (success / ocr_failed / filtered) → emit events.",
            "If a category has no OCR endpoint configured, only classification and archiving happen; OCR failures keep a retryable state without losing the classification.",
          ],
        },
      },
      {
        heading: { zh: "调用示例", en: "Example call" },
        body: {
          zh: [
            "投递一张本地图片并查询服务状态。",
          ],
          en: [
            "Ingest a local image and check service health.",
          ],
        },
        code: {
          title: { zh: "OCR 投递示例", en: "OCR ingest example" },
          source: `# 服务状态
curl http://127.0.0.1:9787/api/ocr/health

# 投递资源（本地路径 / http(s) / data: URL 均可）
curl -X POST http://127.0.0.1:9787/api/ocr/ingest \\
  -H "Content-Type: application/json" \\
  -d '{
    "sender_username": "user1",
    "session_type": "group",
    "timestamp": "1753718400",
    "username": "user1",
    "mediaUrl": "file:///D:/tmp/scan.jpg"
  }'

# → 202 {"id": 42, "status": "pending"}`,
        },
      },
    ],
  },
  {
    slug: "community",
    group: { zh: "开发者", en: "Developers" },
    order: 6,
    title: { zh: "社区与贡献", en: "Community & Contributing" },
    summary: {
      zh: "开源内核、贡献指南与社区渠道。",
      en: "Open core, contribution guidelines and community channels.",
    },
    sections: [
      {
        heading: { zh: "贡献指南", en: "Contributing" },
        body: {
          zh: [
            "后端为 Rust（clippy 零警告、fmt 通过、单测 308），前端为 Svelte 5（svelte-check 0 错误 0 警告）。提交需附测试与变更说明，并保持全部门禁绿。",
          ],
          en: [
            "The backend is Rust (zero clippy warnings, fmt clean, 308 unit tests); the frontend is Svelte 5 (svelte-check at 0 errors / 0 warnings). PRs include tests and a change description and keep every gate green.",
          ],
        },
        code: {
          title: { zh: "本地开发", en: "Local development" },
          source: `# 仓库根目录（ST 工作区）
cd st_control && npm install
npm run dev          # Vite :1420（配合 src-tauri 桌面壳）
cd src-tauri
cargo test --lib --no-default-features`,
        },
      },
    ],
  },
  {
    slug: "privacy",
    group: { zh: "法律与政策", en: "Legal & Policy" },
    order: 1,
    title: { zh: "隐私政策", en: "Privacy Policy" },
    summary: { zh: "我们如何处理你的数据。", en: "How we handle your data." },
    sections: [
      {
        heading: { zh: "数据本地化", en: "Data locality" },
        body: {
          zh: [
            "Harness 默认不收集任何遥测。对话内容、文件、会话日志与用量统计全部保存在本机。模型请求仅发往你配置的提供方端点。",
            "微信数据分析只读访问本机解密副本，密钥与解密都在本机完成；图片原图回退（CDN / ilink）仅在用户开启且需要时发起网络请求。",
          ],
          en: [
            "HARNESS collects no telemetry by default. Conversations, files, session logs and usage stats stay on your machine. Model requests go only to endpoints you configure.",
            "WeChat analysis reads only local decrypted copies; keys and decryption stay on this machine. Original-image fallbacks (CDN / ilink) only make network requests when enabled and needed.",
          ],
        },
      },
    ],
  },
  {
    slug: "terms",
    group: { zh: "法律与政策", en: "Legal & Policy" },
    order: 2,
    title: { zh: "服务条款", en: "Terms of Service" },
    summary: { zh: "软件许可与服务条款。", en: "License and terms of service." },
    sections: [
      {
        heading: { zh: "许可", en: "License" },
        body: {
          zh: [
            "当前版本面向本地个人使用，安装即表示同意本条款；团队协作能力仍在路线图中，届时另行发布对应许可条款。",
          ],
          en: [
            "The current release is for local personal use; installing it constitutes acceptance of these terms. Team collaboration capabilities remain on the roadmap, with their own licensing terms to follow.",
          ],
        },
      },
    ],
  },
];
