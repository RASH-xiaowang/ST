import type { Bi } from "@/lib/i18n/locales";

/** 博客文章（双语，Markdown 风格正文段落） */

export type BlogPost = {
  slug: string;
  date: string;
  readMinutes: number;
  tag: Bi<string>;
  title: Bi<string>;
  excerpt: Bi<string>;
  author: { name: string; role: Bi<string> };
  body: Bi<string[]>;
};

export const posts: BlogPost[] = [
  {
    slug: "why-local-first-agents",
    date: "2026-08-08",
    readMinutes: 6,
    tag: { zh: "观点", en: "Perspective" },
    title: {
      zh: "为什么我们坚持本地优先的 AI 代理",
      en: "Why we bet on local-first AI agents",
    },
    excerpt: {
      zh: "当大模型开始替你做决定时，数据和审计权就不该在别人手里。",
      en: "When models start making decisions for you, data and audit rights shouldn't live in someone else's hands.",
    },
    author: { name: "Harness Team", role: { zh: "产品团队", en: "Product Team" } },
    body: {
      zh: [
        "过去两年，AI 助手的默认假设是云端：你的每一次提问、每一份文件、每一条审计轨迹都被上传到某个遥远的数据中心。这个假设对消费级聊天也许成立，但对真正做事的代理系统并不成立。",
        "当代理开始读写你的代码、执行你的命令、维护你的会话时，「数据不出境」就不再是一句口号，而是合规底线与信任基石。本地优先意味着：模型可见的事实全部落在你自己的追加式日志里，渲染与回放同源，任何决策都能被重建与审计。",
        "我们因此把 Harness 设计成一个本地运行时：模型只对你配置的端点说话，沙箱三模式约束它的手脚，审批卡守住危险操作。性能并没有为此妥协——真流式输出、并发子代理、PTY 真终端都在本地完成，首 token 延迟由统计条实时可见，随提供方与网络波动。",
        "本地优先不是回到单机时代，而是把智能的所有权还给使用者。",
      ],
      en: [
        "For the past two years, the default assumption for AI assistants has been cloud: every question, every file, every audit trail uploaded to a distant data center. That assumption may hold for consumer chat — it does not hold for agents that actually do work.",
        "Once an agent reads your source, runs your commands and maintains your sessions, “no data egress” stops being a slogan and becomes a compliance baseline and a foundation of trust. Local-first means every model-visible fact lands in your own append-only log, render and replay share one source, and any decision can be rebuilt and audited.",
        "That is why HARNESS is a local runtime: the model only talks to endpoints you configure, a three-tier sandbox constrains its reach, and approval cards guard dangerous operations. Performance doesn't suffer for it — true streaming, parallel subagents and a real PTY terminal all run locally, and time-to-first-token is visible live on the stats bar, varying with provider and network.",
        "Local-first isn't a return to the single-machine era. It's giving ownership of intelligence back to the people who use it.",
      ],
    },
  },
  {
    slug: "designing-the-execution-timeline",
    date: "2026-07-22",
    readMinutes: 8,
    tag: { zh: "设计", en: "Design" },
    title: {
      zh: "工具执行时间线：让代理的动作可以被看见",
      en: "The execution timeline: making agent actions visible",
    },
    excerpt: {
      zh: "先工具、后回复——显示位置应当忠实于真实时序。",
      en: "Tools first, reply second — display should honor real chronology.",
    },
    author: { name: "Harness Team", role: { zh: "设计团队", en: "Design Team" } },
    body: {
      zh: [
        "传统的聊天界面把工具调用折叠成一行小字，或者干脆不展示。用户只看到最终回答，却不知道代理读了什么文件、执行了什么命令、失败了几次。",
        "Harness 的做法是把工具执行渲染成一条垂直时间线，放在回复气泡上方：每个步骤一个状态节点——完成是绿色对勾、失败是红色叉、执行中是脉冲光点——点击即可展开完整参数与结果。",
        "位置与真实时序严格一致：工具先执行，回复后到来。失败与重试序列被如实保留，这正是审计所需要的透明度。",
        "我们相信，可视化不是锦上添花，而是信任机制的一部分。",
      ],
      en: [
        "Traditional chat UIs collapse tool calls into a line of tiny text — or hide them entirely. Users see only the final answer, with no idea which files were read, which commands ran, or how many times things failed.",
        "HARNESS renders tool execution as a vertical timeline above the reply bubble: each step gets a state node — green check for done, red cross for failure, pulsing dot while running — click to expand full args and results.",
        "Placement strictly follows real chronology: tools execute first, the reply arrives after. Failure-and-retry sequences are preserved faithfully — exactly the transparency auditing needs.",
        "We believe visualization isn't decoration; it's part of the trust mechanism.",
      ],
    },
  },
  {
    slug: "agent-runtime-what-we-measure",
    date: "2026-06-30",
    readMinutes: 5,
    tag: { zh: "工程", en: "Engineering" },
    title: {
      zh: "我们如何度量一个代理运行时",
      en: "How we measure an agent runtime",
    },
    excerpt: {
      zh: "轮次、步数、墙钟、缓存命中率——统计条背后是哪些遥测。",
      en: "Rounds, steps, wall-clock, cache-hit rate — the telemetry behind the stats bar.",
    },
    author: { name: "Harness Team", role: { zh: "平台团队", en: "Platform Team" } },
    body: {
      zh: [
        "代理系统的性能不是单一数字。我们在每个会话顶部展示一条统计条：轮次与步数、LLM 墙钟与工具墙钟、首 token 平均、tokens/秒、缓存命中率、输入输出总量。",
        "这些指标的采集发生在请求级：模型客户端记录墙钟与首字节延迟（非流式下的 TTFT 代理），工具管道记录每步耗时；会话聚合时，步数与工具墙钟直接从事件日志投影，保证展示与审计同源。",
        "缓存命中率来自 OpenAI prompt_tokens_details.cached_tokens 与 DeepSeek prompt_cache_hit_tokens 的兼容解析。对本地优先的部署，这一项直接决定长期运行成本。",
        "好的度量让优化有方向：统计条上的每一次变化——无论是首 token 缩短还是缓存命中率上升——都是可验证的发布说明。",
      ],
      en: [
        "Agent-system performance is not a single number. We show a stats bar atop every session: rounds and steps, LLM and tool wall-clock, average time-to-first-token, tokens/sec, cache-hit rate, and total in/out tokens.",
        "These metrics are captured at request level: the model client records wall-clock and first-byte latency (a TTFT proxy in non-streaming mode), the tool pipeline records per-step durations; when aggregating a session, steps and tool time project straight from the event log, keeping display and audit from one source.",
        "Cache-hit rate parses OpenAI prompt_tokens_details.cached_tokens and DeepSeek prompt_cache_hit_tokens compatibly. For local-first deployments, this one number drives long-run cost.",
        "Good metrics give optimization a direction: every change on the stats bar — shorter time-to-first-token, a rising cache-hit rate — becomes a verifiable release note.",
      ],
    },
  },
];

export function postBySlug(slug: string): BlogPost | undefined {
  return posts.find((p) => p.slug === slug);
}
