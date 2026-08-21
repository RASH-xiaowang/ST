import type { Bi } from "@/lib/i18n/locales";

/** FAQ：手风琴 + 搜索过滤 */

export type Faq = {
  q: Bi<string>;
  a: Bi<string>;
  cat: "product" | "tech" | "privacy";
};

export const faqCats: Record<Faq["cat"], Bi<string>> = {
  product: { zh: "产品", en: "Product" },
  tech: { zh: "技术", en: "Technical" },
  privacy: { zh: "隐私与合规", en: "Privacy & Compliance" },
};

export const faqs: Faq[] = [
  {
    q: { zh: "Harness 是什么？", en: "What is HARNESS?" },
    a: {
      zh: "Harness 是一个本地优先的 AI 代理运行时平台：把大模型变成能在你自己机器上可靠执行任务的数字员工。它提供流式对话、50+ 内置工具、治理中心（预设/钩子/沙箱）、追加式审计日志与自维护工作区。",
      en: "HARNESS is a local-first AI agent runtime platform: it turns LLMs into dependable digital workers that execute tasks on your own machine. It ships streaming chat, 50+ built-in tools, a governance center (presets/hooks/sandbox), an append-only audit log and a self-maintaining workspace.",
    },
    cat: "product",
  },
  {
    q: { zh: "我的数据会上传到云端吗？", en: "Does my data ever leave my machine?" },
    a: {
      zh: "不会。模型请求只发送到你自己配置的模型提供方（也可以使用本地 Ollama），会话日志、文件与遥测全部保存在本地 SQLite。你可以用沙箱「只读」模式进一步限制代理的文件访问。",
      en: "No. Model requests go only to providers you configure (local Ollama works too). Sessions, files and telemetry stay in a local SQLite database. The read-only sandbox mode can further restrict file access.",
    },
    cat: "privacy",
  },
  {
    q: { zh: "支持哪些模型？", en: "Which models are supported?" },
    a: {
      zh: "任何 OpenAI 兼容接口，包括 Azure OpenAI、Ollama 与自定义端点；支持流式输出、工具调用与用量统计。推理模型（含 reasoning_content）自动兼容。",
      en: "Any OpenAI-compatible endpoint, including Azure OpenAI, Ollama and custom servers — with streaming, tool calls and usage telemetry. Reasoning models (reasoning_content) are handled automatically.",
    },
    cat: "tech",
  },
  {
    q: { zh: "工具执行安全吗？", en: "Is tool execution safe?" },
    a: {
      zh: "Harness 提供三层防护：沙箱三模式（只读/工作区写/全权，逐调用可升级且需审批）、危险工具审批卡、进程树级终止。所有工具调用与结果进入追加式日志，可审计可回放。",
      en: "Three layers: a three-tier sandbox (read-only / workspace-write / full, with per-call approved escalation), approval cards for dangerous tools, and process-tree termination. Every call and result lands in the append-only log.",
    },
    cat: "tech",
  },
  {
    q: { zh: "可以私有化部署吗？", en: "Can I deploy on-premises?" },
    a: {
      zh: "应用本身就是本地优先：安装即部署在自己的机器上，数据全部落在本地 data/ 目录。完全离线（air-gapped）部署、SSO/RBAC 与审计归档等规模化能力已列入团队版路线图。",
      en: "The app is local-first by design: it installs on your own machine and keeps all data under the local data/ directory. Fully air-gapped deployment, SSO/RBAC and audit retention are on the team-edition roadmap.",
    },
    cat: "product",
  },
  {
    q: { zh: "如何扩展自定义工具？", en: "How do I add custom tools?" },
    a: {
      zh: "四种方式：动态插件（模型自己用 plugin_define 定义，代码在前端沙箱执行）、run_code（运行模型编写的程序）、MCP 外部服务器（inputSchema 透传）、LSP 语言服务器与钩子桥（CC/Codex 方言）。新工具会出现在带参数 Schema 的工具目录中。",
      en: "Four ways: dynamic plugins (the model defines them with plugin_define, code runs in the frontend sandbox), run_code (execute model-written programs), MCP external servers (inputSchema passthrough), plus LSP servers and hook bridges (CC/Codex dialect). New tools appear in the schema-aware catalog.",
    },
    cat: "tech",
  },
  {
    q: { zh: "会话记录可以清空或导出吗？", en: "Can I clear or export sessions?" },
    a: {
      zh: "可以。会话支持清空聊天记录（保留会话）、删除、重命名与 Markdown 导出；模型本身也具备 session_clear 等自维护工具，可以自己管理会话。",
      en: "Yes. Sessions support clearing (keeping the session), deletion, rename and Markdown export. The agent itself has self-maintenance tools like session_clear to manage sessions for you.",
    },
    cat: "product",
  },
];
