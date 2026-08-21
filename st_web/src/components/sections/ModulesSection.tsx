"use client";

/**
 * ACT 04.5 · Modules — 全部功能模块矩阵
 * 数据源：应用内首页（PlatformOverview）的功能清单；模块已合并为 9 个
 * （AI 角色/AI 文案并入大模型，消息通道并入自动化，数据看板并入首页系统监控）。
 */
import Image from "next/image";
import { useRef } from "react";
import { Reveal } from "@/components/ui/Reveal";
import { useHlLines } from "@/lib/use-hl-lines";
import type { Locale } from "@/lib/i18n/locales";

type ModuleItem = {
  no: string;
  group: string;
  name: string;
  slogan: string;
  points: string[];
  src: string;
  alt: string;
};

const MODULES: ModuleItem[] = [
  {
    no: "01",
    group: "overview",
    name: "首页工作台",
    slogan: "实时状态 · 系统监控",
    points: ["服务器状态与仪表卡", "机架式快捷入口直达各功能", "系统监控：CPU/内存/磁盘/GPU/网络实时曲线"],
    src: "/screenshots/home-overview.webp",
    alt: "首页工作台",
  },
  {
    no: "02",
    group: "ai",
    name: "Harness",
    slogan: "智能代理会话，一个界面",
    points: ["真流式输出与工具时间线", "遥测统计条（TTFT/缓存命中）", "治理抽屉：预设/钩子/沙箱/插件"],
    src: "/screenshots/harness-session.webp",
    alt: "Harness 会话",
  },
  {
    no: "03",
    group: "ai",
    name: "大模型",
    slogan: "接入 · 角色 · 文案 一体",
    points: ["流量与成本 / 接入配置 / 模型管理", "AI 角色：提示词与能力标签管理", "AI 文案：场景模板一键生成"],
    src: "/screenshots/llm.webp",
    alt: "大模型",
  },
  {
    no: "04",
    group: "ai",
    name: "智能体",
    slogan: "远程客户端协同",
    points: ["st_agent 安全连接", "任务下发与状态实时监控"],
    src: "/screenshots/agents.webp",
    alt: "智能体",
  },
  {
    no: "05",
    group: "automation",
    name: "自动化",
    slogan: "消息驱动的规则引擎 · 含消息通道",
    points: ["实时消息监控规则 + AI 字段提取", "自动回复与待回复队列", "消息通道：微信 iLink / QQ 官方机器人"],
    src: "/screenshots/automation.webp",
    alt: "自动化与消息通道",
  },
  {
    no: "06",
    group: "data",
    name: "微信数据",
    slogan: "本地解密，完整掌控",
    points: ["朋友圈洞察 / 撤回记录 / 存储空间", "通讯录全库搜索与资料卡", "社交关系图谱与年度总结"],
    src: "/screenshots/wechat-home.webp",
    alt: "微信数据总览",
  },
  {
    no: "07",
    group: "data",
    name: "知识库",
    slogan: "个人 RAG 问答中枢",
    points: ["多格式文档导入与解析", "向量 + BM25 混合检索问答", "Wiki 知识图谱与自动提炼"],
    src: "/screenshots/kb.webp",
    alt: "知识库",
  },
  {
    no: "08",
    group: "data",
    name: "数据库",
    slogan: "SQLite 可视化工作台",
    points: ["多库浏览与 SQL 执行", "表详情 / 完整性 / 统计", "备份恢复与 CSV 导出"],
    src: "/screenshots/db-manager.webp",
    alt: "数据库",
  },
  {
    no: "09",
    group: "data",
    name: "图文识别",
    slogan: "批量 OCR 工作流",
    points: ["文本 / 表格 / 二维码识别", "批量任务与统计", "HTTP 投递与结果导出"],
    src: "/screenshots/ocr.webp",
    alt: "图文识别",
  },
];

const GROUPS: Record<string, { zh: string; en: string }> = {
  overview: { zh: "概览", en: "Overview" },
  ai: { zh: "AI 工作台", en: "AI Workbench" },
  automation: { zh: "自动化", en: "Automation" },
  data: { zh: "数据与识别", en: "Data & Recognition" },
};

export function ModulesSection({ locale }: { locale: Locale }) {
  const t = locale === "zh";
  const secRef = useRef<HTMLElement | null>(null);
  useHlLines(secRef);
  return (
    <section id="modules" ref={secRef} className="act relative overflow-hidden py-24 lg:py-32">
      <span className="hud-corner left-6 top-20" aria-hidden="true">ALL MODULES · 功能全景</span>
      <div className="relative mx-auto max-w-7xl px-5 lg:px-8">
        <header className="max-w-3xl">
          <p className="sec-tag"><i className="tick" /> {t ? "MODULES · 全部功能模块" : "MODULES · every feature"}</p>
          <h2 className="mt-6 font-display text-4xl font-extrabold leading-tight text-text sm:text-5xl">
            <span className="hl-line"><span className="hl-line-inner">{t ? "从数据到智能到执行，" : "From data to intelligence to execution,"}</span></span>
            <span className="hl-line"><span className="hl-line-inner"><em className="text-gradient">{t ? "一个界面全部掌控" : "one console, fully in control"}</em></span></span>
          </h2>
          <p className="mt-4 text-[15px] leading-relaxed text-muted">
            {t
              ? "九个功能模块覆盖微信数据、知识库、大模型、自动化与消息通道；AI 角色与文案并入大模型，消息通道并入自动化，数据看板并入首页系统监控。以下均为真实运行界面截图。"
              : "Nine modules span WeChat data, knowledge base, LLMs, automation and messaging; AI roles & copywriting live in Models, channels live in Automation, and system monitoring lives in the home view. Every shot below is a real running interface."}
          </p>
        </header>

        {Object.entries(GROUPS).map(([key, label]) => (
          <div key={key} className="mt-14">
            <p className="font-mono text-[11px] uppercase tracking-[0.3em] text-faint">
              {label[locale]} <span className="text-accent">/</span> {key.toUpperCase()}
            </p>
            <div className="mt-5 grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
              {MODULES.filter((m) => m.group === key).map((m, i) => (
                <Reveal key={m.no} delay={i * 60}>
                  <article className="glass group flex h-full flex-col overflow-hidden rounded-2xl transition hover:border-border-2" data-testid="module-card">
                    <div className="relative aspect-[16/9] overflow-hidden border-b border-border">
                      <Image
                        src={m.src}
                        alt={m.alt}
                        fill
                        sizes="(min-width:1024px) 33vw, (min-width:640px) 50vw, 100vw"
                        loading="lazy"
                        className="object-cover transition duration-500 group-hover:scale-[1.04]"
                      />
                      <span className="absolute left-3 top-3 rounded bg-black/55 px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-white/85">
                        {m.no} · {m.name}
                      </span>
                    </div>
                    <div className="flex flex-1 flex-col gap-2 p-5">
                      <h3 className="font-display text-lg font-bold text-text">
                        {m.name}
                        <span className="ml-2 align-middle font-mono text-[11px] font-normal text-faint">{m.slogan}</span>
                      </h3>
                      <ul className="mt-auto flex flex-col gap-1">
                        {m.points.map((p) => (
                          <li key={p} className="flex items-start gap-2 text-[13px] text-muted">
                            <span className="mt-[7px] h-1 w-1 shrink-0 rounded-full bg-accent" />
                            {p}
                          </li>
                        ))}
                      </ul>
                    </div>
                  </article>
                </Reveal>
              ))}
            </div>
          </div>
        ))}

        <p className="mt-10 font-mono text-[11px] tracking-[0.18em] text-faint">
          {t ? "另有：全局搜索（Ctrl+K）· API 文档（OpenAPI 自描述）· 设置（模型/OCR/通道配置）" : "Also: global search (Ctrl+K) · API docs (OpenAPI) · settings (models / OCR / channels)"}{" "}
          <span className="text-accent">→</span>
        </p>
      </div>
    </section>
  );
}
