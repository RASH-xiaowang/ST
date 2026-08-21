"use client";

/** 产品总览：三大支柱卡 + 关键特性清单 + 交互式 3D 产品亮点 */

import { Reveal } from "@/components/ui/Reveal";
import { SectionHeading } from "@/components/ui/SectionHeading";
import { ProductViewer, type Hotspot } from "@/components/three/ProductViewer";
import { overview } from "@/lib/content/site";
import { pick, type Locale } from "@/lib/i18n/locales";

const ICONS: Record<string, React.ReactNode> = {
  chat: (
    <svg viewBox="0 0 24 24" className="h-6 w-6" fill="none">
      <path d="M4 5h16a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H9l-4.5 4v-4H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1Z" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
      <path d="M7 9h6M7 12h4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  ),
  wrench: (
    <svg viewBox="0 0 24 24" className="h-6 w-6" fill="none">
      <path d="M14.7 6.3a4.5 4.5 0 0 0-6 5.4L3 17.4V21h3.6l5.7-5.7a4.5 4.5 0 0 0 5.4-6L14.9 12l-2.9-2.9 2.7-2.8Z" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
    </svg>
  ),
  shield: (
    <svg viewBox="0 0 24 24" className="h-6 w-6" fill="none">
      <path d="M12 3 5 5.8v5.4c0 4.3 3 7.9 7 9.8 4-1.9 7-5.5 7-9.8V5.8L12 3Z" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
      <path d="m9 12 2 2 4-4.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
};

export function Overview({ locale }: { locale: Locale }) {
  const hotspots: Hotspot[] = locale === "zh"
    ? [
        { id: "core", title: "代理核心", desc: "模型循环中枢：流式补全、工具循环、轮次守卫与重复调用提醒。", position: [0, 0, 0] },
        { id: "ring", title: "执行轨道", desc: "50+ 内置工具与子代理并行执行，结果进入追加式日志。", position: [1.5, 0.9, 0.6] },
        { id: "shell", title: "治理壳层", desc: "沙箱三模式、审批卡与决策钩子，把每次调用都约束在策略之内。", position: [-1.4, -0.6, 1.0] },
      ]
    : [
        { id: "core", title: "Agent Core", desc: "The model-loop hub: streaming completion, tool loop, round guard and repeat reminders.", position: [0, 0, 0] },
        { id: "ring", title: "Execution Orbit", desc: "50+ built-in tools and parallel subagents; every result lands in the append-only log.", position: [1.5, 0.9, 0.6] },
        { id: "shell", title: "Governance Shell", desc: "Three-tier sandbox, approval cards and decision hooks keep every call inside policy.", position: [-1.4, -0.6, 1.0] },
      ];

  const viewerLabels = locale === "zh"
    ? { rotate: "产品模型（旋转/缩放/平移）", explode: "爆炸视图", section: "剖面", scheme: "配色", auto: "自动旋转", reset: "重置" }
    : { rotate: "Product model (rotate/zoom/pan)", explode: "Explode", section: "Section", scheme: "Scheme", auto: "Auto-rotate", reset: "Reset" };

  return (
    <section id="overview" className="relative scroll-mt-20 py-24 lg:py-32">
      <div className="mx-auto max-w-7xl px-5 lg:px-8">
        <Reveal>
          <SectionHeading
            index="01"
            eyebrow={locale === "zh" ? "产品总览" : "Overview"}
            title={pick(overview.title, locale)}
            subtitle={pick(overview.subtitle, locale)}
          />
        </Reveal>

        <div className="mt-14 grid gap-5 md:grid-cols-3">
          {overview.pillars.map((p, i) => (
            <Reveal key={p.icon} delay={i * 90}>
              <article className="glass card-hover group relative h-full overflow-hidden rounded-2xl p-7">
                <div
                  className="absolute -right-10 -top-10 h-36 w-36 rounded-full opacity-0 blur-3xl transition-opacity duration-500 group-hover:opacity-40"
                  style={{ background: "radial-gradient(circle, var(--glow), transparent 70%)" }}
                  aria-hidden="true"
                />
                <div className="grid h-12 w-12 place-items-center rounded-xl border border-border text-accent">
                  {ICONS[p.icon]}
                </div>
                <h3 className="mt-5 font-display text-xl font-bold text-text">{pick(p.title, locale)}</h3>
                <p className="mt-3 text-sm leading-relaxed text-muted">{pick(p.desc, locale)}</p>
              </article>
            </Reveal>
          ))}
        </div>

        {/* 交互式产品亮点 */}
        <div className="mt-20 grid items-center gap-10 lg:grid-cols-2">
          <Reveal>
            <h3 className="font-display text-2xl font-bold text-text sm:text-3xl">
              {locale === "zh" ? "把运行时的每一层拆开看" : "Take every layer apart"}
            </h3>
            <p className="mt-4 max-w-lg text-[15px] leading-relaxed text-muted">
              {locale === "zh"
                ? "旋转、缩放、平移这个「代理核心」：爆炸视图展开执行轨道，剖面切开治理壳层，热点标注解释每一层职责。"
                : "Rotate, zoom and pan the agent core: explode the execution orbit, cut the governance shell, and read the hotspots that explain each layer."}
            </p>
            <ul className="mt-6 flex flex-col gap-2.5">
              {overview.features.slice(0, 4).map((f, i) => (
                <li key={i} className="flex items-start gap-3 text-sm text-text">
                  <span className="mt-1 grid h-5 w-5 shrink-0 place-items-center rounded-full bg-accent/15 text-[11px] text-accent">✓</span>
                  {f[locale]}
                </li>
              ))}
            </ul>
          </Reveal>
          <Reveal delay={120}>
            <div className="glass rounded-2xl p-5 sm:p-7">
              <ProductViewer hotspots={hotspots} labels={viewerLabels} />
            </div>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
