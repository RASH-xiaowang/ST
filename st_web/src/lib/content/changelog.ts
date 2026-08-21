import type { Bi } from "@/lib/i18n/locales";

/**
 * 更新日志（时间线）与路线图
 * 版本号与仓库一致（st_control/package.json、Cargo.toml 均为 1.0.0）；
 * 历史条目为功能里程碑（无独立版本号，按交付日期排序）。
 */

export type ChangelogEntry = {
  /** 真实发布版本号（仅当前版本条目有值）；历史里程碑不带版本号 */
  version?: string;
  date: string;
  tag: "major" | "minor" | "patch";
  title: Bi<string>;
  items: Bi<string[]>;
};

export const changelog: ChangelogEntry[] = [
  {
    version: "1.0.0",
    date: "2026-08-18",
    tag: "major",
    title: { zh: "ST Control 1.0.0 · 微信社交图谱与 HTTP API 就绪", en: "ST Control 1.0.0 · social graph & HTTP APIs" },
    items: {
      zh: [
        "微信数据新增社交关系图谱：群友圈子 / 群聊网络双模式，社区检测着色，亲密度榜 / 共同群榜 / 圈子概览，支持导出 SVG 矢量图与分享海报",
        "新增微信数据 HTTP API（127.0.0.1:5032）：会话 / 消息 / 联系人 / 群成员 / 媒体直链 / 监控状态 / 自动化任务，SSE 实时推送与 OpenAPI 自描述文档",
        "新增图文识别资源接收服务（/api/ocr/ingest）：下载 → 开源 OCR 预检 → 证件分类 → 归档 → OCR 入库的异步管线",
        "Harness 接入动态插件（plugin_*）与 run_code 代码执行工具（WebView 沙箱）",
        "全库 308 个单元测试全绿，svelte-check 0 错误 0 警告",
      ],
      en: [
        "New WeChat social relationship graph: dual modes (circles of friends / group network), auto-colored communities, intimacy / shared-group leaderboards and circle overviews; exportable as SVG or a shareable poster",
        "New WeChat data HTTP API (127.0.0.1:5032): sessions, messages, contacts, group members, media links, monitor status and automation tasks, plus SSE push and a self-describing OpenAPI document",
        "New OCR ingest service (/api/ocr/ingest): an async pipeline of download → open-source OCR precheck → document classification → archiving → OCR → persistence",
        "HARNESS gains dynamic plugins (plugin_*) and the run_code tool (WebView sandbox)",
        "308 unit tests green; svelte-check at 0 errors, 0 warnings",
      ],
    },
  },
  {
    date: "2026-08-12",
    tag: "minor",
    title: { zh: "会话自维护与工作区放大", en: "Session self-maintenance & workspace expansion" },
    items: {
      zh: ["新增 session_list / create / rename / clear / delete 五个自维护工具", "默认工作区放大到应用项目根，代理可读写自身源码", "会话侧栏新增「清空聊天记录」按钮"],
      en: ["New self-maintenance tools: session_list/create/rename/clear/delete", "Default workspace expanded to the app project root for source I/O", "New “clear chat” button in the session sidebar"],
    },
  },
  {
    date: "2026-07-28",
    tag: "minor",
    title: { zh: "真流式输出", en: "True streaming output" },
    items: {
      zh: ["工具循环切换为流式补全，逐字下发", "首 token 遥测改为真实 TTFT", "工具调用分片按 index 合并，保证关联"],
      en: ["Agent loop now uses streaming completion, token by token", "TTFT telemetry now measures real first-delta latency", "Tool-call fragments merged by index for reliable correlation"],
    },
  },
  {
    date: "2026-07-15",
    tag: "minor",
    title: { zh: "会话遥测统计条", en: "Session telemetry stats bar" },
    items: {
      zh: ["轮次/步数/LLM 墙钟/工具墙钟/首 token/缓存命中率", "harness_usage 表新增遥测列并自动迁移", "工具执行时间线显示位置重构"],
      en: ["Rounds/steps/LLM wall/tool wall/TTFT/cache-hit rate", "Telemetry columns added to harness_usage with auto-migration", "Execution timeline display redesigned"],
    },
  },
  {
    date: "2026-06-24",
    tag: "major",
    title: { zh: "AI 聊天并入 Harness 会话", en: "AI chat merged into Harness" },
    items: {
      zh: ["AI 角色注入迁移至会话级并落日志", "移除独立 AI 聊天板块，统一为单一对话界面", "概览页「AI 对话」直达合并后的会话"],
      en: ["Role injection moved to session level and logged", "Standalone AI chat panel removed — one unified chat", "Overview card “AI Dialogue” jumps straight to the merged session"],
    },
  },
  {
    date: "2026-05-30",
    tag: "minor",
    title: { zh: "扩展生态与协议连接器", en: "Extensions & protocol connectors" },
    items: {
      zh: ["技能 / 反馈 / 会话查询 / KV 存储 / 上下文溢写", "凭据引用（.env 注入）与 LSP 语言服务器", "MCP schema 透传、ACP 自动化入口与后台作业", "后台作业运行时与 job_list/output/kill"],
      en: ["Skills, feedback, session search, KV storage and context spill", "Credential references (.env injection) and LSP servers", "MCP schema passthrough, ACP automation entry and background jobs", "Background job runtime with job_list/output/kill"],
    },
  },
  {
    date: "2026-04-18",
    tag: "minor",
    title: { zh: "治理中心", en: "Governance center" },
    items: {
      zh: ["预设（工具作用域/超时/提示词分区）", "钩子桥 CC/Codex 方言 + PreToolUse 决策", "沙箱三模式与逐调用升级"],
      en: ["Presets (tool scopes/timeouts/prompt sections)", "Hook bridge with CC/Codex dialect + PreToolUse decisions", "Three-tier sandbox with per-call escalation"],
    },
  },
  {
    date: "2026-03-01",
    tag: "major",
    title: { zh: "Harness 迁移落地", en: "HARNESS migration lands" },
    items: {
      zh: ["会话日志投影架构（渲染与回放同源，模型可见即落日志）", "50+ 内置工具与审批流", "PTY 真终端与进程树终止"],
      en: ["Session log-projection architecture (render = replay; model-visible is logged)", "50+ built-in tools with approval flows", "PTY real terminal with process-tree termination"],
    },
  },
];

/** 路线图：阶段 + 条目状态 */
export type RoadmapItem = { title: Bi<string>; desc: Bi<string>; status: "done" | "active" | "planned" };
export type RoadmapPhase = { name: Bi<string>; quarter: string; items: RoadmapItem[] };

export const roadmap: RoadmapPhase[] = [
  {
    name: { zh: "执行纵深", en: "Execution depth" },
    quarter: "2026 Q3",
    items: [
      { title: { zh: "沙箱进程隔离（Windows 作业对象）", en: "Sandboxed process isolation (Windows Job Objects)" }, desc: { zh: "内存/CPU/网络配额", en: "Memory/CPU/network quotas" }, status: "active" },
      { title: { zh: "多代理编排", en: "Multi-agent orchestration" }, desc: { zh: "子代理互发消息与并行任务图", en: "Subagent messaging and parallel task graphs" }, status: "planned" },
      { title: { zh: "e2b 远程沙箱（可选）", en: "e2b remote sandbox (optional)" }, desc: { zh: "把执行世界放到云端 Linux 沙箱", en: "Place the execution world in a cloud Linux sandbox" }, status: "planned" },
      { title: { zh: "Python SDK 绑定", en: "Python SDK bindings" }, desc: { zh: "面向外部集成的官方客户端", en: "Official client for external integration" }, status: "planned" },
    ],
  },
  {
    name: { zh: "智能与上下文", en: "Intelligence & context" },
    quarter: "2026 Q4",
    items: [
      { title: { zh: "长期记忆图谱", en: "Long-term memory graph" }, desc: { zh: "跨会话知识沉淀与检索", en: "Cross-session knowledge capture and retrieval" }, status: "planned" },
      { title: { zh: "技能市场", en: "Skill marketplace" }, desc: { zh: "社区共享 SKILL.md 模板", en: "Community-shared SKILL.md templates" }, status: "planned" },
      { title: { zh: "目标自动续跑驱动", en: "Goal round driver" }, desc: { zh: "目标完成前自动续轮（goal_set 已支持 max_goal_rounds）", en: "Auto-continue until goals complete (goal_set already supports max_goal_rounds)" }, status: "done" },
    ],
  },
  {
    name: { zh: "平台与生态", en: "Platform & ecosystem" },
    quarter: "2027 Q1",
    items: [
      { title: { zh: "插件市场", en: "Plugin marketplace" }, desc: { zh: "第三方工具/预设/技能分发", en: "Distribution for third-party tools, presets and skills" }, status: "planned" },
      { title: { zh: "团队版协作", en: "Team collaboration" }, desc: { zh: "共享会话与审计归档", en: "Shared sessions and audit archiving" }, status: "planned" },
    ],
  },
];
