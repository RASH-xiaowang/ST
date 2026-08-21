/** 站点级内容：品牌、导航、首屏、总览、特性、指标、场景、联系、页脚 */

export const brand = {
  name: "ST Control",
  product: { zh: "一体化 AI 智能控制台", en: "All-in-one AI Control Console" },
  slogan: {
    zh: "把智能装进每一台机器",
    en: "Intelligence, deployed everywhere",
  },
  heroSub: {
    zh: "ST Control 是一款本地优先的一体化控制台：微信数据、知识库、大模型管理与智能代理运行时（Harness）尽收一处。流式对话、工具执行时间线、治理与审计一体，把大模型变成能在你自己机器上可靠执行任务、可审计、可自维护的数字员工。",
    en: "ST Control is a local-first, all-in-one console: WeChat data, knowledge base, LLM management and the Harness agent runtime in one place. Streaming chat, an execution timeline, governance and audit — turning LLMs into reliable, auditable, self-maintaining digital workers on your own machine.",
  },
  heroBadge: {
    zh: "本地优先 · 零数据出境 · 桌面原生",
    en: "Local-first · Zero data egress · Desktop native",
  },
  ctaPrimary: { zh: "开始使用", en: "Get Started" },
  ctaSecondary: { zh: "探索功能", en: "Explore Features" },
  scrollHint: { zh: "向下滚动", en: "Scroll to explore" },
};

export const nav = {
  items: [
    { id: "manifesto", href: "/#manifesto", label: { zh: "宣言", en: "Manifesto" } },
    { id: "decrypt", href: "/#decrypt", label: { zh: "微信数据", en: "WeChat Data" } },
    { id: "features", href: "/#features", label: { zh: "核心功能", en: "Features" } },
    { id: "machine", href: "/#machine", label: { zh: "隐私与架构", en: "Privacy & Arch" } },
    { id: "customers", href: "/#customers", label: { zh: "客户案例", en: "Customers" } },
    { id: "docs", href: "/docs/", label: { zh: "文档", en: "Docs" } },
    { id: "blog", href: "/blog/", label: { zh: "博客", en: "Blog" } },
  ],
  cta: { zh: "开始使用", en: "Get Started" },
};

export const footer = {
  tagline: {
    zh: "本地优先的一体化智能控制台：微信数据、知识库、大模型管理与 Harness 智能代理运行时，把前沿 AI 能力安全地装进你自己的机器。",
    en: "A local-first all-in-one console — WeChat data, knowledge base, LLM management and the HARNESS agent runtime — putting frontier AI safely inside your own machine.",
  },
  columns: [
    {
      title: { zh: "产品", en: "Product" },
      links: [
        { label: { zh: "宣言", en: "Manifesto" }, href: "/#manifesto" },
        { label: { zh: "微信数据", en: "WeChat Data" }, href: "/#decrypt" },
        { label: { zh: "核心特性", en: "Features" }, href: "/#features" },
        { label: { zh: "路线图", en: "Roadmap" }, href: "/roadmap/" },
        { label: { zh: "更新日志", en: "Changelog" }, href: "/changelog/" },
      ],
    },
    {
      title: { zh: "资源", en: "Resources" },
      links: [
        { label: { zh: "文档", en: "Docs" }, href: "/docs/" },
        { label: { zh: "API 参考", en: "API Reference" }, href: "/docs/api/" },
        { label: { zh: "博客", en: "Blog" }, href: "/blog/" },
        { label: { zh: "常见问题", en: "FAQ" }, href: "/#faq" },
        { label: { zh: "站内搜索", en: "Search" }, href: "/search/" },
      ],
    },
    {
      title: { zh: "支持", en: "Support" },
      links: [
        { label: { zh: "联系我们", en: "Contact" }, href: "/contact/" },
        { label: { zh: "开发者社区", en: "Community" }, href: "/docs/community/" },
        { label: { zh: "状态页", en: "Status" }, href: "/changelog/" },
        { label: { zh: "隐私政策", en: "Privacy" }, href: "/docs/privacy/" },
        { label: { zh: "服务条款", en: "Terms" }, href: "/docs/terms/" },
      ],
    },
  ],
  copyright: {
    zh: "© {year} ST Control. 保留所有权利。本地优先，数据不出境。",
    en: "© {year} ST Control. All rights reserved. Local-first, data never leaves.",
  },
};

/** 产品总览：三张核心价值卡 + 关键特性清单 */
export const overview = {
  title: { zh: "一套运行时，三种能力", en: "One runtime, three superpowers" },
  subtitle: {
    zh: "从对话到执行再到治理，Harness 把大模型的能力收敛为一个可靠的本地运行时。",
    en: "From conversation to execution to governance — Harness distills LLM capability into one dependable local runtime.",
  },
  pillars: [
    {
      icon: "chat",
      title: { zh: "流式智能对话", en: "Streaming Chat" },
      desc: {
        zh: "逐字流式输出、AI 角色注入、语音输入与播报、多模态附件与历史持久化。",
        en: "Token-by-token streaming, role injection, voice in/out, multimodal attachments and persistent history.",
      },
    },
    {
      icon: "wrench",
      title: { zh: "可靠工具执行", en: "Reliable Tool Execution" },
      desc: {
        zh: "文件读写、命令执行、终端、子代理与定时任务，执行时间线全程可视化、可回放。",
        en: "File I/O, shell, PTY terminals, subagents and schedules — a visualized, replayable execution timeline.",
      },
    },
    {
      icon: "shield",
      title: { zh: "治理与审计", en: "Governance & Audit" },
      desc: {
        zh: "沙箱三模式、审批卡、钩子桥与追加式日志：模型可见即落日志，一切可追溯。",
        en: "Three-tier sandbox, approval cards, hook bridges and append-only logs: what the model sees is logged.",
      },
    },
  ],
  features: [
    { zh: "微信数据本地解密：朋友圈洞察 / 撤回记录 / 存储空间 / 通讯录检索", en: "Local WeChat decryption: moments insights, recalled messages, storage & contacts" },
    { zh: "个人知识库：多格式导入 + 向量 / BM25 混合检索问答", en: "Personal knowledge base: multi-format import + hybrid retrieval Q&A" },
    { zh: "真流式逐字输出，首 token 延迟 < 1s", en: "True streaming output, <1s time-to-first-token" },
    { zh: "50+ 内置工具，工具目录带参数 Schema", en: "50+ built-in tools with schema-aware catalog" },
    { zh: "执行时间线：步骤状态、耗时、参数与结果可展开", en: "Execution timeline with expandable args, results and timings" },
    { zh: "会话遥测统计条：轮次/步数/墙钟/缓存命中率", en: "Session telemetry: rounds, steps, wall-clock, cache-hit rate" },
    { zh: "沙箱三模式 + 逐调用越界审批", en: "Three-tier sandbox with per-call escalation approval" },
    { zh: "微信数据本地解密：年度/每日总结、转账/红包/视频号记录、隐私扫描与多通道原图", en: "Local WeChat decryption: annual/daily summaries, transfers/red packets/finder records, privacy scan and multi-channel originals" },
    { zh: "个人知识库 RAG：多格式导入、向量 + BM25 混合检索、流式问答与 Wiki", en: "Personal KB RAG: multi-format import, vector + BM25 hybrid retrieval, streaming Q&A and Wiki" },
    { zh: "Windows OCR 与语音对话（TTS 朗读 / STT 输入）", en: "Windows OCR plus voice chat (TTS read-aloud / STT input)" },
    { zh: "动态插件与 run_code：模型自修改运行时 + 前端沙箱代码执行", en: "Dynamic plugins and run_code: model self-modification plus sandboxed code execution" },
    { zh: "凭据引用（.env 注入）、会话分叉与 Markdown 导出", en: "Credential references (.env injection), session forking and Markdown export" },
  ],
};

/** 技术架构 */
export const architecture = {
  title: { zh: "架构与性能", en: "Architecture & Performance" },
  subtitle: {
    zh: "分层运行时：前端投影、模型循环、工具管道与存储全部解耦。",
    en: "A layered runtime: UI projection, model loop, tool pipeline and storage — fully decoupled.",
  },
  layers: [
    { key: "ui", name: { zh: "界面投影层", en: "UI Projection" }, desc: { zh: "渲染与回放同源，任何状态都能从日志重建", en: "Render and replay share one source; any state rebuilds from the log" } },
    { key: "agent", name: { zh: "模型循环层", en: "Agent Loop" }, desc: { zh: "流式补全 + 工具循环 + 轮次守卫 + 重复提醒", en: "Streaming completion + tool loop + round guard + repeat reminders" } },
    { key: "tools", name: { zh: "工具管道层", en: "Tool Pipeline" }, desc: { zh: "审批门控、超时守卫、决策钩子、会话级拦截", en: "Approval gate, timeout guard, decision hooks, session interception" } },
    { key: "store", name: { zh: "存储与遥测层", en: "Storage & Telemetry" }, desc: { zh: "SQLite 追加式日志、用量遥测、向量与全文检索", en: "SQLite append-only log, usage telemetry, vector & FTS search" } },
  ],
  metrics: [
    { key: "ttft", name: { zh: "首 token 延迟", en: "Time to first token" }, value: 0.9, unit: "s", note: { zh: "本地直连平均", en: "avg, direct connect" } },
    { key: "tps", name: { zh: "输出速率", en: "Output rate" }, value: 100, unit: "tok/s", note: { zh: "流式解码", en: "streaming decode" } },
    { key: "tools", name: { zh: "内置工具", en: "Built-in tools" }, value: 50, unit: "+", note: { zh: "目录带参数 Schema", en: "schema-aware catalog" } },
    { key: "sandbox", name: { zh: "沙箱模式", en: "Sandbox modes" }, value: 3, unit: "", note: { zh: "只读 / 工作区写 / 全权", en: "read-only / workspace / full" } },
    { key: "privacy", name: { zh: "数据出境", en: "Data egress" }, value: 0, unit: "B", note: { zh: "全部本地处理", en: "processed locally" } },
  ],
  specs: {
    title: { zh: "技术规格", en: "Technical Specifications" },
    rows: [
      { k: { zh: "运行时", en: "Runtime" }, v: { zh: "Rust (Tauri 2) · 桌面原生", en: "Rust (Tauri 2) · desktop native" } },
      { k: { zh: "模型接口", en: "Model APIs" }, v: { zh: "OpenAI 兼容 / Azure / Ollama / 自定义", en: "OpenAI-compatible / Azure / Ollama / custom" } },
      { k: { zh: "存储", en: "Storage" }, v: { zh: "SQLite (WAL) · 追加式事件日志", en: "SQLite (WAL) · append-only event log" } },
      { k: { zh: "终端", en: "Terminal" }, v: { zh: "ConPTY 真终端 · 进程树终止", en: "ConPTY real terminal · process-tree kill" } },
      { k: { zh: "沙箱", en: "Sandbox" }, v: { zh: "只读 / 工作区写 / 全权（逐调用升级）", en: "Read-only / workspace-write / full (per-call escalation)" } },
      { k: { zh: "审计", en: "Audit" }, v: { zh: "会话事件日志 · 用量遥测 · 反馈", en: "Session event log · usage telemetry · feedback" } },
    ],
  },
};

/** 应用场景 */
export const scenarios = {
  title: { zh: "应用场景", en: "Scenarios" },
  subtitle: {
    zh: "从开发者自维护到企业合规审计，一套运行时覆盖多行业需求。",
    en: "From developer self-maintenance to enterprise compliance audit — one runtime across industries.",
  },
  list: [
    {
      id: "dev",
      name: { zh: "研发自维护", en: "Developer Self-maintenance" },
      pain: { zh: "工程重复劳动：改代码、跑测试、修 CI 全靠人肉", en: "Repetitive engineering: edits, tests and CI fixes are all manual" },
      solve: { zh: "工作路径直通项目根：模型读写源码、跑命令、清会话，自己维护自己", en: "Workspace rooted at your repo: the agent edits source, runs commands and maintains itself" },
      tags: [{ zh: "读写源码", en: "Source I/O" }, { zh: "命令执行", en: "Shell" }, { zh: "会话维护", en: "Session ops" }],
    },
    {
      id: "ops",
      name: { zh: "运维自动化", en: "Ops Automation" },
      pain: { zh: "告警响应慢，脚本散落各处，过程不可审计", en: "Slow alert response, scattered scripts, no audit trail" },
      solve: { zh: "定时任务 + 后台作业 + 真终端，每一步执行都落在追加式日志里", en: "Schedules, background jobs and real terminals — every step lands in the append-only log" },
      tags: [{ zh: "定时任务", en: "Schedules" }, { zh: "后台作业", en: "Jobs" }, { zh: "PTY 终端", en: "PTY" }],
    },
    {
      id: "fin",
      name: { zh: "金融合规", en: "Financial Compliance" },
      pain: { zh: "敏感数据不能出网，模型行为必须可追溯", en: "Sensitive data cannot egress; model behavior must be traceable" },
      solve: { zh: "本地推理 + 沙箱三模式 + 审批卡 + 模型可见即落日志", en: "Local inference, three-tier sandbox, approval cards and logged model-visible facts" },
      tags: [{ zh: "本地优先", en: "Local-first" }, { zh: "审批流", en: "Approvals" }, { zh: "审计日志", en: "Audit log" }],
    },
    {
      id: "edu",
      name: { zh: "科研教育", en: "Research & Education" },
      pain: { zh: "实验环境复杂，学生上手成本高", en: "Complex lab setups with steep onboarding" },
      solve: { zh: "一键工作区 + 技能注入 + 中英双语文档，几分钟跑通实验", en: "One-click workspaces, skill injection and bilingual docs — labs run in minutes" },
      tags: [{ zh: "工作区", en: "Workspaces" }, { zh: "技能", en: "Skills" }, { zh: "双语", en: "Bilingual" }],
    },
  ],
};

/** 开发者/API */
export const developer = {
  title: { zh: "为开发者而生", en: "Built for developers" },
  subtitle: {
    zh: "本地 JSON-RPC SDK、ACP 自动化协议、CLI 与配置束导入导出，十分钟完成集成。",
    en: "Local JSON-RPC SDK, the ACP automation protocol, a CLI and bundle import/export — integrate in ten minutes.",
  },
  sample: `# 本地 JSON-RPC（127.0.0.1:4770，仅本机）
curl -X POST http://127.0.0.1:4770/rpc \\
  -H "Content-Type: application/json" \\
  -d '{"jsonrpc":"2.0","id":1,"method":"sessions.list","params":{}}'

# ACP 自动化入口：创建会话并设定目标
curl -X POST http://127.0.0.1:4770/rpc \\
  -d '{"jsonrpc":"2.0","id":2,"method":"session/new","params":{"goal":"分析仓库依赖"}}'`,
  api: [
    { name: "sessions.list / session.create", desc: { zh: "列出 / 新建会话", en: "List / create sessions" } },
    { name: "session.display / session.state", desc: { zh: "读取事件流与运行状态", en: "Read the event log and run state" } },
    { name: "session.chat", desc: { zh: "同步执行一轮对话，返回最终回答", en: "Run one synchronous turn, returns the answer" } },
    { name: "tool.execute", desc: { zh: "不经模型直接派发一次工具调用", en: "Dispatch one tool call without the model" } },
    { name: "usage.get", desc: { zh: "会话用量遥测", en: "Session usage telemetry" } },
    { name: "session/new · prompt · cancel", desc: { zh: "ACP 自动化会话协议", en: "ACP automation session protocol" } },
  ],
};

/** 联系 */
export const contact = {
  title: { zh: "联系与支持", en: "Contact & Support" },
  subtitle: {
    zh: "产品反馈、问题排查与使用建议，欢迎通过应用内反馈或社区交流。",
    en: "Feedback, troubleshooting and suggestions — via the in-app feedback or community channels.",
  },
  channels: [
    { icon: "mail", name: { zh: "应用内反馈", en: "In-app feedback" }, value: { zh: "Harness 会话内「好/差评 + 评论」", en: "Like/dislike plus comments in HARNESS sessions" } },
    { icon: "support", name: { zh: "帮助文档", en: "Docs" }, value: { zh: "本站文档中心与站内搜索", en: "This site's docs and search" } },
    { icon: "map", name: { zh: "数据位置", en: "Data location" }, value: { zh: "全部本地 data/ 目录，零数据出境", en: "Local data/ directory, zero data egress" } },
  ],
};
