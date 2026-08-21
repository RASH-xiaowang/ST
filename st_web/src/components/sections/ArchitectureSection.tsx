"use client";

/** 架构与性能：分层架构 SVG 图 + 3D 指标柱状图 + 技术规格表 */

import { Reveal } from "@/components/ui/Reveal";
import { SectionHeading } from "@/components/ui/SectionHeading";
import { DataViz3D, type VizBar } from "@/components/three/DataViz3D";
import { architecture } from "@/lib/content/site";
import { pick, type Locale } from "@/lib/i18n/locales";

const BAR_COLORS = ["#22d3ee", "#8b5cf6", "#ec4899", "#34d399", "#f5c33b", "#60a5fa"];

export function ArchitectureSection({ locale }: { locale: Locale }) {
  const bars: VizBar[] = architecture.metrics.map((m, i) => ({
    id: m.key,
    label: m.name[locale],
    value: Math.min(100, m.value),
    unit: m.unit,
    color: BAR_COLORS[i % BAR_COLORS.length],
  }));

  return (
    <section id="architecture" className="relative scroll-mt-20 py-24 lg:py-32">
      {/* 背景网格 */}
      <div className="grid-overlay absolute inset-0 opacity-40" aria-hidden="true" />
      <div className="relative mx-auto max-w-7xl px-5 lg:px-8">
        <Reveal>
          <SectionHeading
            index="03"
            eyebrow={locale === "zh" ? "架构与性能" : "Architecture & Performance"}
            title={pick(architecture.title, locale)}
            subtitle={pick(architecture.subtitle, locale)}
          />
        </Reveal>

        <div className="mt-14 grid items-start gap-10 lg:grid-cols-2">
          {/* 分层架构图 */}
          <Reveal>
            <div className="glass rounded-2xl p-6 sm:p-8" role="img" aria-label={locale === "zh" ? "分层架构图" : "Layered architecture"}>
              <div className="flex flex-col gap-3">
                {architecture.layers.map((l, i) => (
                  <div key={l.key} className="relative">
                    <div className="group flex items-center gap-4 rounded-xl border border-border bg-surface px-5 py-4 transition hover:border-accent/50">
                      <span className="font-mono text-[11px] font-bold uppercase tracking-wider text-accent">
                        L{i}
                      </span>
                      <div className="min-w-0">
                        <p className="text-sm font-bold text-text">{l.name[locale]}</p>
                        <p className="mt-0.5 text-xs text-muted">{l.desc[locale]}</p>
                      </div>
                      <span className="ml-auto hidden font-mono text-[10px] text-faint sm:inline">
                        {["→", "⇄", "⇄", "⇄"][i] ?? "→"}
                      </span>
                    </div>
                    {i < architecture.layers.length - 1 && (
                      <span className="mx-auto block h-3 w-px bg-gradient-to-b from-accent/60 to-transparent" aria-hidden="true" />
                    )}
                  </div>
                ))}
              </div>
            </div>
          </Reveal>

          {/* 3D 数据可视化 */}
          <Reveal delay={120}>
            <div className="glass rounded-2xl p-6 sm:p-8">
              <h3 className="font-display text-lg font-bold text-text">
                {locale === "zh" ? "性能指标" : "Performance metrics"}
              </h3>
              <DataViz3D
                bars={bars}
                labels={{ hint: locale === "zh" ? "悬停高亮 · 点击查看详情 · 拖拽旋转" : "Hover to highlight · click for details · drag to rotate" }}
              />
            </div>
          </Reveal>
        </div>

        {/* 技术规格表 */}
        <Reveal delay={80}>
          <div className="glass mt-14 overflow-hidden rounded-2xl">
            <h3 className="border-b border-border px-6 py-4 font-display text-lg font-bold text-text sm:px-8">
              {pick(architecture.specs.title, locale)}
            </h3>
            <div className="divide-y divide-border">
              {architecture.specs.rows.map((r) => (
                <div key={r.k.en} className="grid gap-1 px-6 py-3.5 sm:grid-cols-[240px_1fr] sm:gap-6 sm:px-8">
                  <dt className="text-sm font-semibold text-text">{r.k[locale]}</dt>
                  <dd className="text-sm text-muted">{r.v[locale]}</dd>
                </div>
              ))}
            </div>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
