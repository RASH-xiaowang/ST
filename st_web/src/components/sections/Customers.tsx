"use client";

/** 客户案例：Logo 墙 + 分类筛选 + 案例卡片 + 详情弹窗 */

import { useMemo, useState } from "react";
import { Reveal } from "@/components/ui/Reveal";
import { SectionHeading } from "@/components/ui/SectionHeading";
import { cases, caseCategories, logoWall, type CaseCategory, type Case } from "@/lib/content/cases";
import { pick, type Locale } from "@/lib/i18n/locales";

export function Customers({ locale }: { locale: Locale }) {
  const [filter, setFilter] = useState<CaseCategory | "all">("all");
  const [selected, setSelected] = useState<Case | null>(null);

  const visible = useMemo(
    () => (filter === "all" ? cases : cases.filter((c) => c.category === filter)),
    [filter],
  );

  return (
    <section id="customers" className="relative scroll-mt-20 py-24 lg:py-32">
      <div className="mx-auto max-w-7xl px-5 lg:px-8">
        <Reveal>
          <SectionHeading
            index="05"
            eyebrow={locale === "zh" ? "典型应用场景" : "Scenarios"}
            title={locale === "zh" ? "用 Harness 能做什么" : "What HARNESS can do"}
            subtitle={
              locale === "zh"
                ? "四个方向的真实能力落地：本地数据洞察、研发自维护、自动化执行与知识管理。"
                : "Four real capability areas: local data insights, developer self-maintenance, automation and knowledge management."
            }
          />
        </Reveal>

        {/* Logo 墙 */}
        <Reveal delay={60}>
          <div className="mt-12 grid grid-cols-2 gap-3 sm:grid-cols-4 lg:grid-cols-8">
            {logoWall.map((l) => (
              <div
                key={l.name}
                className="glass flex items-center justify-center gap-2 rounded-xl px-3 py-4 opacity-80 transition hover:opacity-100"
                title={l.name}
              >
                <span
                  className="grid h-8 w-8 place-items-center rounded-lg font-mono text-xs font-bold text-white"
                  style={{ background: `linear-gradient(135deg, hsl(${l.hue} 80% 55%), hsl(${l.hue + 40} 80% 45%))` }}
                >
                  {l.initials}
                </span>
                <span className="hidden truncate text-xs font-semibold text-muted xl:inline">{l.name}</span>
              </div>
            ))}
          </div>
        </Reveal>

        {/* 筛选 */}
        <div className="mt-10 flex flex-wrap justify-center gap-2" role="group" aria-label={locale === "zh" ? "案例筛选" : "Case filter"}>
          <button
            onClick={() => setFilter("all")}
            aria-pressed={filter === "all"}
            className={`rounded-full border px-4 py-1.5 text-sm transition ${
              filter === "all" ? "border-accent bg-accent/15 text-accent" : "border-border text-muted hover:text-text"
            }`}
          >
            {locale === "zh" ? "全部" : "All"}
          </button>
          {(Object.keys(caseCategories) as CaseCategory[]).map((cat) => (
            <button
              key={cat}
              onClick={() => setFilter(cat)}
              aria-pressed={filter === cat}
              className={`rounded-full border px-4 py-1.5 text-sm transition ${
                filter === cat ? "border-accent bg-accent/15 text-accent" : "border-border text-muted hover:text-text"
              }`}
            >
              {pick(caseCategories[cat], locale)}
            </button>
          ))}
        </div>

        {/* 案例卡片 */}
        <div className="mt-8 grid gap-5 md:grid-cols-2 lg:grid-cols-3">
          {visible.map((c, i) => (
            <Reveal key={c.id} delay={i * 60}>
              <button
                onClick={() => setSelected(c)}
                className="glass card-hover flex h-full w-full flex-col rounded-2xl p-6 text-left"
                aria-haspopup="dialog"
              >
                <div className="flex items-center gap-3">
                  <span
                    className="grid h-10 w-10 place-items-center rounded-xl font-mono text-sm font-bold text-white"
                    style={{ background: `linear-gradient(135deg, hsl(${c.hue} 80% 55%), hsl(${c.hue + 40} 80% 45%))` }}
                  >
                    {c.logo}
                  </span>
                  <div>
                    <p className="text-sm font-bold text-text">{c.name}</p>
                    <p className="font-mono text-[10px] uppercase tracking-wider text-faint">
                      {pick(caseCategories[c.category], locale)}
                    </p>
                  </div>
                </div>
                <p className="mt-4 font-mono text-lg font-bold text-gradient">{pick(c.metric, locale)}</p>
                <p className="mt-2 text-sm font-semibold text-text">{pick(c.title, locale)}</p>
                <p className="mt-2 line-clamp-3 text-[13px] leading-relaxed text-muted">{pick(c.summary, locale)}</p>
              </button>
            </Reveal>
          ))}
        </div>
      </div>

      {/* 详情弹窗 */}
      {selected && (
        <div
          className="fixed inset-0 z-[80] flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm"
          onClick={() => setSelected(null)}
          role="dialog"
          aria-modal="true"
          aria-label={selected.name}
        >
          <div
            className="glass w-full max-w-2xl overflow-hidden rounded-2xl shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-start justify-between gap-4 border-b border-border px-6 py-5">
              <div className="flex items-center gap-3">
                <span
                  className="grid h-10 w-10 place-items-center rounded-xl font-mono text-sm font-bold text-white"
                  style={{ background: `linear-gradient(135deg, hsl(${selected.hue} 80% 55%), hsl(${selected.hue + 40} 80% 45%))` }}
                >
                  {selected.logo}
                </span>
                <div>
                  <h3 className="font-display text-lg font-bold text-text">{selected.name}</h3>
                  <p className="text-sm text-accent">{pick(selected.metric, locale)}</p>
                </div>
              </div>
              <button
                onClick={() => setSelected(null)}
                aria-label={locale === "zh" ? "关闭" : "Close"}
                className="grid h-9 w-9 place-items-center rounded-lg border border-border text-muted transition hover:text-text"
              >
                ✕
              </button>
            </div>
            <div className="max-h-[60vh] overflow-y-auto px-6 py-5">
              <p className="text-[15px] font-semibold text-text">{pick(selected.title, locale)}</p>
              <ul className="mt-4 flex flex-col gap-3.5">
                {pick(selected.detail, locale).map((para, i) => (
                  <li key={i} className="flex gap-3 text-sm leading-relaxed text-muted">
                    <span className="mt-0.5 grid h-5 w-5 shrink-0 place-items-center rounded-full bg-accent/15 font-mono text-[10px] text-accent">
                      {i + 1}
                    </span>
                    {para}
                  </li>
                ))}
              </ul>
              <div className="mt-5 flex flex-wrap gap-2">
                {selected.tags.map((tag) => (
                  <span key={tag.en} className="rounded-full bg-surface px-3 py-1 font-mono text-[11px] text-muted">
                    #{pick(tag, locale)}
                  </span>
                ))}
              </div>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
