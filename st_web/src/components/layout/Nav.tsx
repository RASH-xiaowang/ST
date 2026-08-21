"use client";

/**
 * 导航栏：滚动悬浮玻璃态 + 移动端抽屉 + 主题/语言/搜索入口。
 */
import { useEffect, useState } from "react";
import { usePathname } from "next/navigation";
import { Logo } from "@/components/ui/Logo";
import { ThemeToggle } from "@/components/layout/ThemeToggle";
import { LocaleSwitch } from "@/components/layout/LocaleSwitch";
import { useSmoothScroll } from "@/components/layout/SmoothScroll";
import { useSearch } from "@/components/search/SearchContext";
import { nav } from "@/lib/content/site";
import { pick } from "@/lib/i18n/locales";

export function Nav({ locale }: { locale: string }) {
  const [scrolled, setScrolled] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const pathname = usePathname();
  const { scrollTo } = useSmoothScroll();
  const { open: openSearch } = useSearch();

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 24);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  useEffect(() => {
    setMenuOpen(false);
  }, [pathname]);

  const go = (href: string) => {
    setMenuOpen(false);
    if (href.startsWith("/#")) {
      const id = href.slice(2);
      if (pathname === `/${locale}` || pathname === `/${locale}/`) {
        scrollTo(`#${id}`);
      } else {
        window.location.href = `/${locale}/${href.replace(/^\//, "")}`;
      }
    } else {
      // 站内跳转走路由
      window.location.href = `/${locale}${href.startsWith("/") ? href : `/${href}`}`;
    }
  };

  const labels = { dark: "切换到亮色模式", light: "切换到暗黑模式" };

  return (
    <header
      className={`fixed inset-x-0 top-0 z-50 transition-all duration-300 ${
        scrolled || menuOpen
          ? "glass shadow-[0_12px_40px_-20px_rgba(0,0,0,.45)]"
          : "border-b border-transparent bg-transparent"
      }`}
      style={scrolled || menuOpen ? { background: "var(--nav-bg)" } : undefined}
    >
      <nav
        className="mx-auto flex h-16 max-w-7xl items-center gap-6 px-5 lg:px-8"
        aria-label={locale === "zh" ? "主导航" : "Main navigation"}
      >
        <a
          href={`/${locale}/`}
          className="shrink-0"
          aria-label="HARNESS home"
          data-testid="nav-logo"
        >
          <Logo locale={locale} compact />
        </a>

        <ul className="hidden flex-1 items-center justify-center gap-1 lg:flex">
          {nav.items.map((item) => (
            <li key={item.id}>
              <button
                onClick={() => go(item.href)}
                className="rounded-lg px-3 py-2 text-[13.5px] text-muted transition hover:bg-surface-3 hover:text-text"
              >
                {pick(item.label, locale as "zh" | "en")}
              </button>
            </li>
          ))}
        </ul>

        <div className="ml-auto flex items-center gap-2.5 lg:ml-0">
          <button
            onClick={openSearch}
            aria-label={locale === "zh" ? "站内搜索" : "Search"}
            title={locale === "zh" ? "站内搜索" : "Search"}
            className="hidden h-9 w-9 place-items-center rounded-lg border border-border text-muted transition hover:border-border-2 hover:text-text sm:grid"
            data-testid="search-open"
          >
            <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" aria-hidden="true">
              <circle cx="11" cy="11" r="6.5" stroke="currentColor" strokeWidth="1.7" />
              <path d="m16 16 4.5 4.5" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
            </svg>
          </button>
          <ThemeToggle label={labels} />
          <LocaleSwitch locale={locale} />
          <a
            href={`/${locale}/contact/`}
            className="hidden rounded-lg bg-gradient-to-r from-accent via-accent-2 to-accent-3 px-4 py-2 text-[13px] font-semibold text-white shadow-[0_10px_30px_-12px_var(--glow)] transition hover:brightness-110 md:inline-flex"
            data-testid="nav-cta"
          >
            {pick(nav.cta, locale as "zh" | "en")}
          </a>
          <button
            onClick={() => setMenuOpen(!menuOpen)}
            aria-expanded={menuOpen}
            aria-label={locale === "zh" ? "菜单" : "Menu"}
            className="grid h-9 w-9 place-items-center rounded-lg border border-border text-text lg:hidden"
          >
            {menuOpen ? "✕" : "☰"}
          </button>
        </div>
      </nav>

      {/* 移动端抽屉 */}
      {menuOpen && (
        <div className="glass border-t border-border px-5 pb-6 pt-2 lg:hidden">
          <ul className="flex flex-col gap-1">
            {nav.items.map((item) => (
              <li key={item.id}>
                <button
                  onClick={() => go(item.href)}
                  className="w-full rounded-lg px-3 py-2.5 text-left text-[15px] text-text transition hover:bg-surface-3"
                >
                  {pick(item.label, locale as "zh" | "en")}
                </button>
              </li>
            ))}
            <li className="mt-2">
              <button
                onClick={() => go("/search/")}
                className="w-full rounded-lg px-3 py-2.5 text-left text-[15px] text-text transition hover:bg-surface-3"
              >
                {locale === "zh" ? "站内搜索" : "Search"}
              </button>
            </li>
          </ul>
        </div>
      )}
    </header>
  );
}
