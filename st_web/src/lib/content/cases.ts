import type { Bi } from "@/lib/i18n/locales";

/** 典型应用场景：以真实产品能力组织的使用模式（非客户证言） */

export type CaseCategory = "data" | "dev" | "automation" | "knowledge";

export const caseCategories: Record<CaseCategory, Bi<string>> = {
  data: { zh: "本地数据洞察", en: "Local Data Insights" },
  dev: { zh: "研发自维护", en: "Developer Self-Maintenance" },
  automation: { zh: "自动化与执行", en: "Automation & Execution" },
  knowledge: { zh: "知识管理与学习", en: "Knowledge & Learning" },
};

export const logoWall: { name: string; initials: string; hue: number }[] = [
  { name: "微信数据", initials: "微", hue: 215 },
  { name: "知识库", initials: "知", hue: 150 },
  { name: "Harness", initials: "H", hue: 190 },
  { name: "自动化", initials: "自", hue: 35 },
  { name: "OCR", initials: "O", hue: 330 },
  { name: "语音对话", initials: "语", hue: 265 },
  { name: "系统仪表", initials: "仪", hue: 280 },
  { name: "DSH 迁移", initials: "迁", hue: 170 },
];

export type Case = {
  id: string;
  category: CaseCategory;
  logo: string;
  hue: number;
  name: string;
  metric: Bi<string>;
  title: Bi<string>;
  summary: Bi<string>;
  detail: Bi<string[]>;
  tags: Bi<string>[];
};

export const cases: Case[] = [
  {
    id: "wechat-data-insights",
    category: "data",
    logo: "微",
    hue: 215,
    name: "微信数据洞察",
    metric: { zh: "本地解密 · 零出境", en: "Local decrypt · zero egress" },
    title: { zh: "把微信档案变成可检索、可统计的本地数据", en: "Turn your WeChat archive into searchable, local-first data" },
    summary: {
      zh: "在解密后的本地数据库中浏览会话/群聊/好友/朋友圈，统计撤回、转账、红包与存储构成，原图经 CDN / ilink / ISAAC 多通道解析。",
      en: "Browse sessions, groups, contacts and moments in the decrypted local database; analyze recalled messages, transfers, red packets and storage; resolve originals through CDN / ilink / ISAAC channels.",
    },
    detail: {
      zh: [
        "数据来源：应用内只读访问解密副本（message/message_*.db、session/session.db、contact.db、general.db、sns.db 等），密钥与解密全在本机完成。",
        "能力：会话与消息全文检索、朋友圈洞察与月度热力、撤回记录（谁撤回了什么）、存储空间构成、年度/每日总结、转账/红包/视频号/小程序/好友验证记录、隐私扫描。",
        "图片链路：本地 DAT 解码、CDN 原图回退、ilink 官方通道回退、朋友圈 ISAAC-64 解密、HEVC(wxgf) 转 JPEG，任一通道失败均优雅降级。",
      ],
      en: [
        "Data source: the app only reads decrypted copies (message/message_*.db, session/session.db, contact.db, general.db, sns.db, …); keys and decryption stay on this machine.",
        "Capabilities: full-text session & message search, moments insights with monthly heatmaps, recalled-message records, storage composition, annual/daily summaries, transfer/red-packet/finder/mini-program/friend-verification records and privacy scans.",
        "Image pipeline: local DAT decode, CDN fallback, ilink official-channel fallback, ISAAC-64 moments decryption and HEVC(wxgf)-to-JPEG — every channel degrades gracefully.",
      ],
    },
    tags: [{ zh: "本地解密", en: "Local decryption" }, { zh: "全文检索", en: "Full-text search" }, { zh: "多通道原图", en: "Multi-channel originals" }],
  },
  {
    id: "dev-self-maintenance",
    category: "dev",
    logo: "H",
    hue: 190,
    name: "研发自维护",
    metric: { zh: "50+ 工具 · 会话自维护", en: "50+ tools · self-maintaining sessions" },
    title: { zh: "工作区直通项目根，代理自己维护自己", en: "Workspace rooted at your repo — the agent maintains itself" },
    summary: {
      zh: "模型读写源码、跑命令、维护会话；计划/目标/待办与子代理协同，执行时间线全程可回放。",
      en: "The model edits source, runs commands and maintains sessions; plans, goals, todos and subagents collaborate behind a fully replayable execution timeline.",
    },
    detail: {
      zh: [
        "执行世界：shell 与 ConPTY 真终端保持 cwd，fs 读写受工作区沙箱约束，后台作业与定时任务把长任务移出回合。",
        "编排：plan_enter/exit 计划模式守卫、goal_set 目标、todo_write 待办、task 子代理、workflow 分阶段流水线，全部落入追加式日志。",
        "自维护：session_list/create/rename/clear/delete 让模型管理自身会话；分叉与 Markdown 导出便于追溯与归档。",
      ],
      en: [
        "Execution world: shell and a ConPTY terminal keep cwd; fs reads/writes are constrained by the workspace sandbox; background jobs and schedules move long work out of the turn.",
        "Orchestration: plan_enter/exit mode guard, goal_set, todo_write, the task subagent and staged workflows — all recorded in the append-only log.",
        "Self-maintenance: session_list/create/rename/clear/delete let the agent manage its own sessions; forking and Markdown export aid traceability.",
      ],
    },
    tags: [{ zh: "源码读写", en: "Source I/O" }, { zh: "计划模式", en: "Plan mode" }, { zh: "子代理", en: "Subagents" }],
  },
  {
    id: "automation-execution",
    category: "automation",
    logo: "自",
    hue: 35,
    name: "自动化与执行",
    metric: { zh: "定时 + 作业 + 审计", en: "Schedules + jobs + audit" },
    title: { zh: "定时任务、后台作业与可审计的执行链", en: "Schedules, background jobs and an auditable execution chain" },
    summary: {
      zh: "每 30 秒调度器触发一轮代理对话，后台作业并行执行命令，钩子桥把事件推给外部脚本，全部调用落日志。",
      en: "A 30-second scheduler fires agent turns, background jobs run commands in parallel, the hook bridge pushes events to external scripts — and every call lands in the log.",
    },
    detail: {
      zh: [
        "调度：schedule_create 支持周期/延时两种触发，到点自动执行一轮对话并落 workflow_run 事件；「立即运行」手动触发。",
        "作业：exec_command run_in_background=true 启动后台作业，job_list/output/kill 管理生命周期，输出可随时取回。",
        "治理：沙箱三模式 + 逐调用越界审批 + PreToolUse 决策钩子，进程树级终止；会话遥测记录每轮墙钟与 token 用量。",
      ],
      en: [
        "Schedules: schedule_create supports interval and one-shot triggers; a 30-second scheduler fires the turn and logs workflow_run events; “run now” triggers manually.",
        "Jobs: exec_command with run_in_background=true starts background jobs; job_list/output/kill manage their lifecycle and retrieve output anytime.",
        "Governance: three-tier sandbox with per-call escalation approval and PreToolUse decision hooks, process-tree termination, and per-turn wall-clock/token telemetry.",
      ],
    },
    tags: [{ zh: "定时任务", en: "Schedules" }, { zh: "后台作业", en: "Jobs" }, { zh: "审计日志", en: "Audit log" }],
  },
  {
    id: "kb-learning",
    category: "knowledge",
    logo: "知",
    hue: 150,
    name: "知识管理与学习",
    metric: { zh: "混合检索 RAG", en: "Hybrid retrieval RAG" },
    title: { zh: "个人知识库：导入、分块、检索与流式问答", en: "A personal knowledge base: import, chunk, retrieve and stream Q&A" },
    summary: {
      zh: "多格式文档导入、向量 + BM25 混合检索、Wiki 与 FAQ 沉淀，OCR 让扫描件也可检索。",
      en: "Multi-format document import, vector + BM25 hybrid retrieval, Wiki and FAQ capture — and OCR makes scanned documents searchable too.",
    },
    detail: {
      zh: [
        "知识库：docs 上传/多版本/ACL，分块与重处理任务异步执行，chunks 支持全文与向量双路检索，RAG 流式回答带引用。",
        "Wiki 与 FAQ：页面 CRUD、链接图、提炼命令与全文索引（BM25），自动提炼与实体提取沉淀可复用知识。",
        "辅助能力：Windows OCR 把扫描版 PDF/图片转文本；语音对话（TTS 朗读 / STT 输入）让学习与检索双手不离开键盘。",
      ],
      en: [
        "Knowledge base: document upload with versions and ACLs, async chunking/reprocessing jobs, and dual full-text + vector retrieval behind streaming RAG answers with citations.",
        "Wiki & FAQ: page CRUD, link graphs, summarization commands and BM25 indexing; auto-extraction distills reusable knowledge.",
        "Helpers: Windows OCR turns scanned PDFs/images into text; voice chat (TTS read-aloud / STT input) keeps hands on the keyboard.",
      ],
    },
    tags: [{ zh: "RAG", en: "RAG" }, { zh: "Wiki", en: "Wiki" }, { zh: "OCR", en: "OCR" }],
  },
];
