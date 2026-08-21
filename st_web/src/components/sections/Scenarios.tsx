"use client";

/** 应用场景：行业 Tab 切换（痛点 → 方案）+ 标签 */

import { useState } from "react";
import { Reveal } from "@/components/ui/Reveal";
import { SectionHeading } from "@/components/ui/SectionHeading";
import { scenarios } from "@/lib/content/site";
import { pick, type Locale } from "@/lib/i18n/locales";

export function Scenarios({ locale }: { locale: Locale }) {
  const [active, setActive] = useState(scenarios.list[0].id);
  const cur = scenarios.list.find((s) => s.id === active) ?? scenarios.list[0];

  return (
    <section id="scenarios" className="relative scroll-mt-20 py-24 lg:py-32">
      <div className="mx-auto max-w-7xl px-5 lg:px-8">
        <Reveal>
          <SectionHeading
            index="04"
            eyebrow={locale === "zh" ? "应用场景" : "Scenarios"}
            title={pick(scenarios.title, locale)}
            subtitle={pick(scenarios.subtitle, locale)}
          />
        </Reveal>

        <Reveal delay={80}>
          <div
            className="mt-10 flex flex-wrap justify-center gap-2"
            role="tablist"
            aria-label={locale === "zh" ? "应用场景" : "Scenarios"}
          >
            {scenarios.list.map((s) => (
              <button
                key={s.id}
                role="tab"
                aria-selected={active === s.id}
                onClick={() => setActive(s.id)}
                className={`rounded-full border px-5 py-2 text-sm font-semibold transition ${
                  active === s.id
                    ? "border-accent bg-accent/15 text-accent"
                    : "border-border text-muted hover:text-text"
                }`}
              >
                {pick(s.name, locale)}
              </button>
            ))}
          </div>
        </Reveal>

        <Reveal delay={120}>
          <div className="glass mt-8 grid overflow-hidden rounded-2xl lg:grid-cols-2">
            {/* 痛点 → 方案 */}
            <div className="flex flex-col justify-center gap-6 p-7 sm:p-10">
              <div className="rounded-xl border border-warn/40 bg-warn/10 p-5">
                <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-warn">
                  {locale === "zh" ? "痛点" : "Pain"}
                </p>
                <p className="mt-2 text-[15px] leading-relaxed text-text">{pick(cur.pain, locale)}</p>
              </div>
              <span className="mx-auto grid h-9 w-9 place-items-center rounded-full border border-accent/50 text-accent" aria-hidden="true">
                ↓
              </span>
              <div className="rounded-xl border border-accent/40 bg-accent/10 p-5">
                <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-accent">
                  {locale === "zh" ? "方案" : "Solution"}
                </p>
                <p className="mt-2 text-[15px] leading-relaxed text-text">{pick(cur.solve, locale)}</p>
              </div>
              <div className="flex flex-wrap gap-2">
                {cur.tags.map((tag) => (
                  <span key={tag.en} className="rounded-full bg-surface px-3 py-1 font-mono text-[11px] text-muted">
                    #{pick(tag, locale)}
                  </span>
                ))}
              </div>
            </div>
            {/* 场景视觉（SVG 装饰） */}
            <div className="relative hidden min-h-[320px] overflow-hidden border-l border-border lg:block" aria-hidden="true">
              <div className="grid-overlay absolute inset-0" />
              <svg viewBox="0 0 480 360" className="absolute inset-0 h-full w-full opacity-70">
                <defs>
                  <linearGradient id="sc-g" x1="0" y1="0" x2="1" y2="1">
                    <stop offset="0" stopColor="var(--accent)" />
                    <stop offset="1" stopColor="var(--accent-2)" />
                  </linearGradient>
                </defs>
                <g fill="none" stroke="url(#sc-g)" strokeWidth="1.2">
                  <circle cx="240" cy="180" r="90" opacity="0.7" />
                  <circle cx="240" cy="180" r="130" opacity="0.35" strokeDasharray="4 8" />
                  <path d="M240 90v180M150 180h180" opacity="0.3" />
                  {[...Array(12)].map((_, i) => {
                    const a = (i / 12) * Math.PI * 2;
                    return (
                      <circle
                        key={i}
                        cx={240 + Math.cos(a) * 110}
                        cy={180 + Math.sin(a) * 110}
                        r={5}
                        fill="var(--accent-3)"
                        stroke="none"
                        opacity="0.8"
                      />
                    );
                  })}
                </g>
              </svg>
            </div>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
