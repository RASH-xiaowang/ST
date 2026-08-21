"use client";

/** FAQ：手风琴 + 搜索过滤 + JSON-LD（FAQPage 结构化数据由页面注入） */

import { useMemo, useState } from "react";
import { Reveal } from "@/components/ui/Reveal";
import { SectionHeading } from "@/components/ui/SectionHeading";
import { Accordion } from "@/components/ui/Accordion";
import { faqs, faqCats } from "@/lib/content/faq";
import { pick, type Locale } from "@/lib/i18n/locales";

export function FaqSection({ locale }: { locale: Locale }) {
  const [q, setQ] = useState("");
  const [cat, setCat] = useState<string>("all");

  const items = useMemo(
    () =>
      faqs
        .filter((f) => (cat === "all" ? true : f.cat === cat))
        .filter((f) => {
          const needle = q.trim().toLowerCase();
          if (!needle) return true;
          return `${f.q[locale]} ${f.a[locale]}`.toLowerCase().includes(needle);
        })
        .map((f) => ({
          id: f.q.en,
          q: f.q[locale],
          a: f.a[locale],
          tag: pick(faqCats[f.cat], locale),
        })),
    [q, cat, locale],
  );

  return (
    <section id="faq" className="relative scroll-mt-20 py-24 lg:py-32">
      <div className="mx-auto max-w-4xl px-5 lg:px-8">
        <Reveal>
          <SectionHeading
            index="08"
            eyebrow={locale === "zh" ? "常见问题" : "FAQ"}
            title={locale === "zh" ? "你可能想问的" : "Answers, up front"}
          />
        </Reveal>

        <Reveal delay={60}>
          <div className="mt-10 flex flex-col gap-3 sm:flex-row">
            <div className="flex flex-1 items-center gap-2.5 rounded-xl border border-border bg-surface px-4 py-2.5">
              <svg viewBox="0 0 24 24" className="h-4 w-4 text-accent" fill="none" aria-hidden="true">
                <circle cx="11" cy="11" r="6.5" stroke="currentColor" strokeWidth="1.7" />
                <path d="m16 16 4.5 4.5" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
              </svg>
              <input
                value={q}
                onChange={(e) => setQ(e.target.value)}
                placeholder={locale === "zh" ? "搜索问题…" : "Search questions…"}
                className="flex-1 bg-transparent text-sm text-text outline-none placeholder:text-faint"
                aria-label={locale === "zh" ? "搜索问题" : "Search questions"}
                data-testid="faq-search"
              />
            </div>
            <div className="flex gap-2">
              {["all", ...Object.keys(faqCats)].map((c) => (
                <button
                  key={c}
                  onClick={() => setCat(c)}
                  aria-pressed={cat === c}
                  className={`rounded-xl border px-3.5 py-2 text-xs font-semibold transition ${
                    cat === c ? "border-accent bg-accent/15 text-accent" : "border-border text-muted hover:text-text"
                  }`}
                >
                  {c === "all" ? (locale === "zh" ? "全部" : "All") : pick(faqCats[c as keyof typeof faqCats], locale)}
                </button>
              ))}
            </div>
          </div>
        </Reveal>

        <Reveal delay={100}>
          <div className="mt-8">
            {items.length > 0 ? (
              <Accordion items={items} />
            ) : (
              <p className="py-10 text-center text-sm text-muted">
                {locale === "zh" ? "没有匹配的问题" : "No matching questions"}
              </p>
            )}
          </div>
        </Reveal>
      </div>
    </section>
  );
}
