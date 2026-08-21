"use client";

/**
 * 全局搜索弹窗：Ctrl/Cmd+K 唤起，多词检索文档/博客/FAQ/案例/日志。
 * 键盘导航：↑↓ 选择、Enter 跳转、Esc 关闭。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { useSearch } from "@/components/search/SearchContext";
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

export function SearchDialog({ locale }: { locale: Locale }) {
  const { isOpen, close, open } = useSearch();
  const [q, setQ] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const router = useRouter();

  const results: SearchEntry[] = useMemo(
    () => (q.trim() ? searchAll(q, locale, 18) : []),
    [q, locale],
  );

  useEffect(() => {
    if (isOpen) {
      setQ("");
      setActive(0);
      setTimeout(() => inputRef.current?.focus(), 30);
      document.documentElement.style.overflow = "hidden";
    } else {
      document.documentElement.style.overflow = "";
    }
    return () => {
      document.documentElement.style.overflow = "";
    };
  }, [isOpen]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        if (isOpen) close();
        else open();
      }
      if (e.key === "Escape" && isOpen) close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isOpen, close, open]);

  const go = useCallback(
    (href: string) => {
      close();
      router.push(href);
    },
    [close, router],
  );

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(a + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter" && results[active]) {
      e.preventDefault();
      go(results[active].href);
    }
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-[90] flex items-start justify-center bg-black/55 px-4 pt-[12vh] backdrop-blur-sm"
      onClick={close}
      role="dialog"
      aria-modal="true"
      aria-label={locale === "zh" ? "站内搜索" : "Search"}
    >
      <div
        className="glass w-full max-w-xl overflow-hidden rounded-2xl shadow-[0_40px_120px_-30px_rgba(0,0,0,.8)]"
        onClick={(e) => e.stopPropagation()}
        data-testid="search-dialog"
      >
        <div className="flex items-center gap-3 border-b border-border px-4 py-3.5">
          <svg viewBox="0 0 24 24" className="h-4 w-4 text-accent" fill="none" aria-hidden="true">
            <circle cx="11" cy="11" r="6.5" stroke="currentColor" strokeWidth="1.7" />
            <path d="m16 16 4.5 4.5" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
          </svg>
          <input
            ref={inputRef}
            value={q}
            onChange={(e) => {
              setQ(e.target.value);
              setActive(0);
            }}
            onKeyDown={onKeyDown}
            placeholder={locale === "zh" ? "搜索文档、博客、案例、FAQ…" : "Search docs, blog, cases, FAQ…"}
            className="flex-1 bg-transparent text-[15px] text-text outline-none placeholder:text-faint"
            data-testid="search-input"
          />
          <kbd className="rounded border border-border px-1.5 py-0.5 font-mono text-[10px] text-faint">ESC</kbd>
        </div>

        <div className="max-h-[52vh] overflow-y-auto p-2">
          {q.trim() === "" ? (
            <p className="px-3 py-8 text-center text-sm text-muted">
              {locale === "zh"
                ? "输入关键词开始检索 · 支持多词空格分隔"
                : "Type to search · space-separated multi-term"}
            </p>
          ) : results.length === 0 ? (
            <p className="px-3 py-8 text-center text-sm text-muted">
              {locale === "zh" ? "没有匹配结果" : "No results"}
            </p>
          ) : (
            <ul>
              {results.map((r, i) => (
                <li key={`${r.type}-${r.href}-${r.title}`}>
                  <button
                    onClick={() => go(r.href)}
                    onMouseEnter={() => setActive(i)}
                    className={`flex w-full flex-col gap-0.5 rounded-lg px-3 py-2.5 text-left transition ${
                      i === active ? "bg-surface-3" : ""
                    }`}
                    data-testid="search-result"
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
                    <span className="line-clamp-1 pl-0 text-xs text-muted">{r.snippet}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="flex items-center gap-3 border-t border-border px-4 py-2 font-mono text-[10px] text-faint">
          <span>↑↓ {locale === "zh" ? "选择" : "select"}</span>
          <span>↵ {locale === "zh" ? "打开" : "open"}</span>
          <span className="ml-auto">Ctrl K</span>
        </div>
      </div>
    </div>
  );
}
