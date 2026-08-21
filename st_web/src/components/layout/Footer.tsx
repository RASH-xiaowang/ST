/** 页脚：品牌 + 三列链接 + 版权；静态组件 */

import { Logo } from "@/components/ui/Logo";
import { footer } from "@/lib/content/site";
import { pick, type Locale } from "@/lib/i18n/locales";

export function Footer({ locale }: { locale: Locale }) {
  return (
    <footer className="relative border-t border-border">
      {/* 顶部发光线 */}
      <div className="glow-hr absolute inset-x-0 top-0" aria-hidden="true" />
      <div className="mx-auto grid max-w-7xl gap-12 px-5 py-16 sm:grid-cols-2 lg:grid-cols-5 lg:px-8">
        <div className="flex flex-col gap-4 lg:col-span-2">
          <Logo locale={locale} />
          <p className="max-w-sm text-sm leading-relaxed text-muted">{pick(footer.tagline, locale)}</p>
          <div className="mt-2 flex gap-3">
            {["GitHub", "X", "Discord"].map((s) => (
              <a
                key={s}
                href="#"
                aria-label={s}
                className="grid h-9 w-9 place-items-center rounded-lg border border-border text-muted transition hover:border-border-2 hover:text-text"
              >
                {s[0]}
              </a>
            ))}
          </div>
        </div>
        {footer.columns.map((col) => (
          <nav key={col.title.en} aria-label={pick(col.title, locale)}>
            <h3 className="mb-4 font-mono text-xs uppercase tracking-[0.24em] text-faint">
              {pick(col.title, locale)}
            </h3>
            <ul className="flex flex-col gap-2.5">
              {col.links.map((l) => (
                <li key={l.href + l.label.en}>
                  <a
                    href={l.href.startsWith("/#") ? `/${locale}${l.href}` : `/${locale}${l.href}`}
                    className="text-sm text-muted transition hover:text-accent"
                  >
                    {pick(l.label, locale)}
                  </a>
                </li>
              ))}
            </ul>
          </nav>
        ))}
      </div>
      <div className="border-t border-border">
        <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-3 px-5 py-5 lg:px-8">
          <p className="font-mono text-xs text-faint">
            {pick(footer.copyright, locale).replace("{year}", String(new Date().getFullYear()))}
          </p>
          <p className="font-mono text-xs text-faint">{locale === "zh" ? "构建于本地" : "built locally"}</p>
        </div>
      </div>
    </footer>
  );
}
