import type { Locale } from "@/lib/i18n/locales";

/** 各语言根布局共享的站点级 meta（静态，无需 params） */
export const META: Record<Locale, { title: string; description: string; keywords: string[] }> = {
  zh: {
    title: "ST Control — 一体化 AI 智能控制台",
    description:
      "本地优先的一体化控制台：微信数据、知识库、大模型管理与 Harness 智能代理运行时。流式对话、工具执行时间线、治理与审计一体，把大模型变成可靠、可审计、可自维护的数字员工。",
    keywords: [
      "ST Control",
      "智能控制台",
      "微信数据管理",
      "知识库",
      "大模型管理",
      "智能代理",
      "本地优先 AI",
      "工具执行",
    ],
  },
  en: {
    title: "ST Control — The All-in-one AI Control Console",
    description:
      "A local-first all-in-one console: WeChat data, knowledge base, LLM management and the HARNESS agent runtime. Streaming chat, execution timeline, governance and audit — turning LLMs into reliable, auditable, self-maintaining digital workers.",
    keywords: [
      "ST Control",
      "control console",
      "WeChat data",
      "knowledge base",
      "LLM management",
      "agent runtime",
      "local-first AI",
      "tool execution",
    ],
  },
};
