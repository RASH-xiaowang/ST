/** 动态内容预览：最新博客 + 更新日志摘要 + 路线图进度条 */

import { Reveal } from "@/components/ui/Reveal";
import { SectionHeading } from "@/components/ui/SectionHeading";
import { posts } from "@/lib/content/blog";
import { changelog, roadmap } from "@/lib/content/changelog";
import { pick, type Locale } from "@/lib/i18n/locales";

export function UpdatesSection({ locale }: { locale: Locale }) {
  const latest = posts.slice(0, 3);
  const latestLog = changelog.slice(0, 3);
  const progress = roadmap.reduce(
    (acc, ph) => {
      const done = ph.items.filter((i) => i.status === "done").length;
      acc.done += done;
      acc.total += ph.items.length;
      return acc;
    },
    { done: 0, total: 0 },
  );
  const pct = progress.total ? Math.round((progress.done / progress.total) * 100) : 0;

  return (
    <section id="updates" className="relative scroll-mt-20 py-24 lg:py-32">
      <div className="mx-auto max-w-7xl px-5 lg:px-8">
        <Reveal>
          <SectionHeading
            index="09"
            eyebrow={locale === "zh" ? "博客 · 日志 · 路线图" : "Blog · Changelog · Roadmap"}
            title={locale === "zh" ? "保持更新" : "Stay in the loop"}
          />
        </Reveal>

        {/* 博客 */}
        <div className="mt-12 grid gap-5 md:grid-cols-3">
          {latest.map((p, i) => (
            <Reveal key={p.slug} delay={i * 80}>
              <a
                href={`/${locale}/blog/${p.slug}/`}
                className="glass card-hover flex h-full flex-col rounded-2xl p-6"
              >
                <div className="flex items-center gap-2 font-mono text-[10px] uppercase tracking-wider">
                  <span className="rounded bg-accent/15 px-2 py-0.5 text-accent">{pick(p.tag, locale)}</span>
                  <span className="text-faint">{p.date}</span>
                </div>
                <h3 className="mt-4 font-display text-[17px] font-bold leading-snug text-text">
                  {pick(p.title, locale)}
                </h3>
                <p className="mt-2 line-clamp-3 flex-1 text-[13px] leading-relaxed text-muted">
                  {pick(p.excerpt, locale)}
                </p>
                <span className="mt-4 text-xs font-semibold text-accent">
                  {locale === "zh" ? "阅读全文 →" : "Read more →"}
                </span>
              </a>
            </Reveal>
          ))}
        </div>

        <div className="mt-14 grid gap-5 lg:grid-cols-2">
          {/* 更新日志摘要 */}
          <Reveal>
            <div className="glass h-full rounded-2xl p-6 sm:p-7">
              <h3 className="font-display text-lg font-bold text-text">
                {locale === "zh" ? "最近更新" : "Recent updates"}
              </h3>
              <ol className="mt-5 flex flex-col">
                {latestLog.map((e, i) => (
                  <li key={e.version} className="relative flex gap-4 pb-5 last:pb-0">
                    {i < latestLog.length - 1 && (
                      <span className="absolute left-[5px] top-4 h-full w-px bg-border" aria-hidden="true" />
                    )}
                    <span className="relative mt-1.5 h-[11px] w-[11px] shrink-0 rounded-full border-2 border-accent bg-bg" />
                    <div>
                      <p className="font-mono text-xs font-bold text-accent">v{e.version}</p>
                      <p className="mt-0.5 text-sm font-semibold text-text">{pick(e.title, locale)}</p>
                    </div>
                  </li>
                ))}
              </ol>
              <a href={`/${locale}/changelog/`} className="mt-5 inline-block text-xs font-semibold text-accent">
                {locale === "zh" ? "全部更新日志 →" : "Full changelog →"}
              </a>
            </div>
          </Reveal>

          {/* 路线图 */}
          <Reveal delay={80}>
            <div className="glass h-full rounded-2xl p-6 sm:p-7">
              <div className="flex items-center justify-between">
                <h3 className="font-display text-lg font-bold text-text">
                  {locale === "zh" ? "产品路线图" : "Product roadmap"}
                </h3>
                <span className="font-mono text-xs text-faint">{pct}%</span>
              </div>
              <div className="mt-4 h-2 overflow-hidden rounded-full bg-surface-3">
                <div
                  className="h-full rounded-full bg-gradient-to-r from-accent to-accent-2 transition-all"
                  style={{ width: `${Math.max(8, pct)}%` }}
                />
              </div>
              <div className="mt-5 flex flex-col gap-3.5">
                {roadmap.map((ph) => (
                  <div key={ph.quarter}>
                    <p className="font-mono text-[11px] uppercase tracking-wider text-faint">
                      {pick(ph.name, locale)} · {ph.quarter}
                    </p>
                    <ul className="mt-1.5 flex flex-col gap-1">
                      {ph.items.slice(0, 2).map((item) => (
                        <li key={item.title.en} className="flex items-center gap-2 text-[13px] text-muted">
                          <span
                            className={`h-1.5 w-1.5 rounded-full ${
                              item.status === "done" ? "bg-ok" : item.status === "active" ? "bg-accent" : "bg-faint"
                            }`}
                          />
                          {pick(item.title, locale)}
                        </li>
                      ))}
                    </ul>
                  </div>
                ))}
              </div>
              <a href={`/${locale}/roadmap/`} className="mt-5 inline-block text-xs font-semibold text-accent">
                {locale === "zh" ? "完整路线图 →" : "Full roadmap →"}
              </a>
            </div>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
