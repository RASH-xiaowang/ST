"use client";

/** 搜索页：全页检索体验（复用全局索引与检索逻辑） */

import { useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { searchAll, type SearchEntry } from "@/lib/search";
import type { Locale } from "@/lib/i18n/locales";

const TYPE_COLORS: Record<string, string> = {
  doc: "var(--accent)",
  blog: "var(--accent-2)",
  faq: "var(--gold)",
  case: "var(--accent-3)",
  changelog: "var(--ok)",
  scenario: "var(--warn)",
  page: "var(--muted)",
};

export function SearchPage({ locale }: { locale: Locale }) {
  const [q, setQ] = useState("");
  const router = useRouter();
  const results: SearchEntry[] = useMemo(
    () => (q.trim() ? searchAll(q, locale, 40) : []),
    [q, locale],
  );

  const t = locale === "zh";

  return (
    <div className="mx-auto max-w-3xl px-5 pb-24 pt-32 lg:px-8">
      <header>
        <p className="font-mono text-xs uppercase tracking-[0.3em] text-accent">
          {t ? "站内搜索" : "Search"}
        </p>
        <h1 className="mt-4 font-display text-4xl font-extrabold text-text">
          {t ? "搜遍文档、博客与案例" : "Search docs, blog and cases"}
        </h1>
      </header>

      <div className="mt-10 flex items-center gap-3 rounded-xl border border-border bg-surface px-4 py-3">
        <svg viewBox="0 0 24 24" className="h-4 w-4 text-accent" fill="none" aria-hidden="true">
          <circle cx="11" cy="11" r="6.5" stroke="currentColor" strokeWidth="1.7" />
          <path d="m16 16 4.5 4.5" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
        </svg>
        <input
          autoFocus
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder={t ? "输入关键词（空格分隔多词）…" : "Type keywords (space-separated)…"}
          className="flex-1 bg-transparent text-[15px] text-text outline-none placeholder:text-faint"
          aria-label={t ? "搜索" : "Search"}
          data-testid="search-page-input"
        />
      </div>

      <div className="mt-8">
        {q.trim() === "" ? (
          <p className="py-14 text-center text-sm text-muted">
            {t ? "支持文档 / 博客 / FAQ / 案例 / 更新日志 / 路线图检索" : "Covers docs, blog, FAQ, cases, changelog and roadmap"}
          </p>
        ) : results.length === 0 ? (
          <p className="py-14 text-center text-sm text-muted">{t ? "没有匹配结果" : "No results"}</p>
        ) : (
          <>
            <p className="font-mono text-xs text-faint">
              {t ? `${results.length} 条结果` : `${results.length} results`}
            </p>
            <ul className="mt-4 flex flex-col gap-3">
              {results.map((r) => (
                <li key={`${r.type}-${r.href}-${r.title}`}>
                  <button
                    onClick={() => router.push(r.href)}
                    className="glass card-hover w-full rounded-xl p-5 text-left"
                    data-testid="search-page-result"
                  >
                    <span className="flex items-center gap-2">
                      <span
                        className="rounded px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider"
                        style={{ color: TYPE_COLORS[r.type] ?? "var(--muted)", background: "var(--surface-3)" }}
                      >
                        {r.typeLabel}
                      </span>
                      <span className="text-sm font-semibold text-text">{r.title}</span>
                    </span>
                    <span className="mt-1.5 line-clamp-2 block text-[13px] leading-relaxed text-muted">
                      {r.snippet}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </>
        )}
      </div>
    </div>
  );
}
