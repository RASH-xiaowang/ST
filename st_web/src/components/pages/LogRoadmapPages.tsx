/** 更新日志（时间线）与路线图页面 */

import { changelog, roadmap, type RoadmapItem } from "@/lib/content/changelog";
import { Reveal } from "@/components/ui/Reveal";
import type { Locale } from "@/lib/i18n/locales";

export function ChangelogPage({ locale }: { locale: Locale }) {
  return (
    <div className="mx-auto max-w-4xl px-5 pb-24 pt-32 lg:px-8">
      <Reveal>
        <header>
          <p className="font-mono text-xs uppercase tracking-[0.3em] text-accent">Changelog</p>
          <h1 className="mt-4 font-display text-4xl font-extrabold text-text">
            {locale === "zh" ? "更新日志" : "Changelog"}
          </h1>
        </header>
      </Reveal>

      <ol className="mt-14 flex flex-col">
        {changelog.map((e, i) => (
          <li key={`${e.date}-${e.version ?? i}`} className="relative flex gap-5 pb-10 last:pb-0">
            {i < changelog.length - 1 && (
              <span className="absolute left-[6px] top-6 h-full w-px bg-gradient-to-b from-accent/60 to-border" aria-hidden="true" />
            )}
            <span className="relative mt-2 h-[13px] w-[13px] shrink-0 rounded-full border-2 border-accent bg-bg" />
            <Reveal className="flex-1">
              <div className="glass rounded-2xl p-6">
                <div className="flex flex-wrap items-center gap-3">
                  {e.version && (
                    <span className="rounded bg-accent/15 px-2.5 py-0.5 font-mono text-xs font-bold text-accent">
                      v{e.version}
                    </span>
                  )}
                  <span
                    className={`rounded px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider ${
                      e.tag === "major"
                        ? "bg-accent-3/15 text-accent-3"
                        : e.tag === "minor"
                          ? "bg-accent-2/15 text-accent-2"
                          : "bg-ok/15 text-ok"
                    }`}
                  >
                    {e.tag}
                  </span>
                  <span className="ml-auto font-mono text-xs text-faint">{e.date}</span>
                </div>
                <h2 className="mt-3 font-display text-lg font-bold text-text">{e.title[locale]}</h2>
                <ul className="mt-3 flex flex-col gap-1.5">
                  {e.items[locale].map((item) => (
                    <li key={item} className="flex items-start gap-2.5 text-sm text-muted">
                      <span className="mt-2 h-1 w-1 shrink-0 rounded-full bg-accent" />
                      {item}
                    </li>
                  ))}
                </ul>
              </div>
            </Reveal>
          </li>
        ))}
      </ol>
    </div>
  );
}

function ItemRow({ item, locale }: { item: RoadmapItem; locale: Locale }) {
  const dot =
    item.status === "done" ? "bg-ok" : item.status === "active" ? "bg-accent animate-pulse" : "bg-faint";
  const label =
    item.status === "done"
      ? locale === "zh" ? "已交付" : "Shipped"
      : item.status === "active"
        ? locale === "zh" ? "进行中" : "In progress"
        : locale === "zh" ? "规划中" : "Planned";
  return (
    <li className="flex items-start gap-3 rounded-xl border border-border bg-surface px-4 py-3.5">
      <span className={`mt-1.5 h-2 w-2 shrink-0 rounded-full ${dot}`} />
      <div className="min-w-0 flex-1">
        <p className="text-sm font-semibold text-text">{item.title[locale]}</p>
        <p className="mt-0.5 text-xs text-muted">{item.desc[locale]}</p>
      </div>
      <span className="shrink-0 font-mono text-[10px] uppercase tracking-wider text-faint">{label}</span>
    </li>
  );
}

export function RoadmapPage({ locale }: { locale: Locale }) {
  return (
    <div className="mx-auto max-w-5xl px-5 pb-24 pt-32 lg:px-8">
      <Reveal>
        <header className="max-w-2xl">
          <p className="font-mono text-xs uppercase tracking-[0.3em] text-accent">Roadmap</p>
          <h1 className="mt-4 font-display text-4xl font-extrabold text-text">
            {locale === "zh" ? "产品路线图" : "Product roadmap"}
          </h1>
          <p className="mt-4 text-[15px] leading-relaxed text-muted">
            {locale === "zh"
              ? "路线图反映当前计划，可能随时调整。想影响优先级？在社区投票。"
              : "The roadmap reflects current plans and may change. Want to influence it? Vote in the community."}
          </p>
        </header>
      </Reveal>
      <div className="mt-12 grid gap-6 lg:grid-cols-3">
        {roadmap.map((ph, i) => (
          <Reveal key={ph.quarter} delay={i * 80}>
            <section className="glass h-full rounded-2xl p-6">
              <div className="flex items-center justify-between">
                <h2 className="font-display text-lg font-bold text-text">{ph.name[locale]}</h2>
                <span className="font-mono text-xs text-accent">{ph.quarter}</span>
              </div>
              <ul className="mt-5 flex flex-col gap-2.5">
                {ph.items.map((item) => (
                  <ItemRow key={item.title.en} item={item} locale={locale} />
                ))}
              </ul>
            </section>
          </Reveal>
        ))}
      </div>
    </div>
  );
}
