"use client";

/** 微信数据洞察：三步流程 + AI 提问 + 年度总结预览 + 隐私引擎 + 洞察清单 + 真实查询 */

import { Reveal } from "@/components/ui/Reveal";
import { SectionHeading } from "@/components/ui/SectionHeading";
import { CodeBlock } from "@/components/ui/CodeBlock";
import { AnnualWrapped } from "@/components/sections/AnnualWrapped";
import { wechatInsights } from "@/lib/content/wechat";
import { pick, type Locale } from "@/lib/i18n/locales";

export function WechatInsights({ locale }: { locale: Locale }) {
  const t = wechatInsights;

  return (
    <section id="insights" className="relative scroll-mt-20 py-24 lg:py-32">
      <div className="mx-auto max-w-7xl px-5 lg:px-8">
        <Reveal>
          <SectionHeading
            index="02"
            eyebrow={locale === "zh" ? "微信数据洞察" : "WeChat Data Insights"}
            title={pick(t.title, locale)}
            subtitle={pick(t.subtitle, locale)}
          />
        </Reveal>

        {/* 三步本地流程 */}
        <div className="mt-14 grid gap-5 md:grid-cols-3">
          {t.steps.items.map((s, i) => (
            <Reveal key={s.no} delay={i * 90}>
              <article className="glass card-hover group relative h-full overflow-hidden rounded-2xl p-7">
                <div
                  className="absolute -right-10 -top-10 h-36 w-36 rounded-full opacity-0 blur-3xl transition-opacity duration-500 group-hover:opacity-40"
                  style={{ background: "radial-gradient(circle, var(--glow), transparent 70%)" }}
                  aria-hidden="true"
                />
                <div className="flex items-center justify-between">
                  <span className="font-mono text-sm font-bold uppercase tracking-[0.25em] text-accent">
                    STEP {s.no}
                  </span>
                  <span className="font-mono text-4xl font-bold text-faint transition-colors duration-500 group-hover:text-accent/30">
                    {s.no}
                  </span>
                </div>
                <h3 className="mt-6 font-display text-xl font-bold text-text">{pick(s.name, locale)}</h3>
                <p className="mt-3 text-sm leading-relaxed text-muted">{pick(s.desc, locale)}</p>
              </article>
            </Reveal>
          ))}
        </div>

        {/* AI 提问 */}
        <div className="mt-24">
          <Reveal>
            <SectionHeading
              index="A"
              eyebrow={locale === "zh" ? "AI 提问" : "Ask your archive"}
              title={pick(t.ask.title, locale)}
              subtitle={pick(t.ask.subtitle, locale)}
            />
          </Reveal>
          <div className="mt-14 grid items-start gap-8 lg:grid-cols-[1fr_1.15fr]">
            <Reveal>
              <ul className="flex flex-col gap-3">
                {t.ask.bullets.map((b, i) => (
                  <li key={i} className="glass flex items-start gap-3 rounded-xl p-4 text-sm leading-relaxed text-text">
                    <span className="mt-0.5 grid h-5 w-5 shrink-0 place-items-center rounded-full bg-accent/15 text-[11px] text-accent">
                      ✓
                    </span>
                    {pick(b, locale)}
                  </li>
                ))}
              </ul>
              <p className="mt-4 text-xs text-faint">{pick(t.ask.note, locale)}</p>
            </Reveal>
            <Reveal delay={120}>
              <CodeBlock
                code={pick(t.ask.sample.code, locale)}
                title={pick(t.ask.sample.title, locale)}
                lang="json"
              />
            </Reveal>
          </div>
        </div>

        {/* 年度总结预览 */}
        <div className="mt-24">
          <Reveal>
            <SectionHeading
              index="B"
              eyebrow={locale === "zh" ? "年度总结" : "Annual Wrapped"}
              title={pick(t.annual.title, locale)}
              subtitle={pick(t.annual.subtitle, locale)}
            />
          </Reveal>
          <Reveal delay={80} className="mt-14">
            <AnnualWrapped locale={locale} />
          </Reveal>
        </div>

        {/* 隐私引擎 */}
        <div className="mt-24">
          <Reveal>
            <SectionHeading
              index="C"
              eyebrow={locale === "zh" ? "隐私引擎" : "The Machine"}
              title={pick(t.privacy.title, locale)}
              subtitle={pick(t.privacy.subtitle, locale)}
            />
          </Reveal>

          <div className="mt-14 grid items-stretch gap-5 lg:grid-cols-[1fr_1.6fr]">
            <Reveal>
              <div className="glass flex h-full flex-col items-center justify-center rounded-2xl p-8 text-center">
                <span className="font-mono text-7xl font-bold text-gradient">{t.privacy.stat.value}</span>
                <span className="mt-1 font-mono text-xl font-bold text-accent">{t.privacy.stat.unit}</span>
                <p className="mt-4 max-w-[220px] text-sm text-muted">{pick(t.privacy.stat.label, locale)}</p>
              </div>
            </Reveal>
            <div className="grid gap-5 sm:grid-cols-2">
              {t.privacy.facts.map((f, i) => (
                <Reveal key={f.title.zh} delay={(i % 2) * 80}>
                  <article className="glass card-hover h-full rounded-2xl p-6">
                    <span className="grid h-10 w-10 place-items-center rounded-xl border border-border bg-white/[0.03] text-lg">
                      {f.icon}
                    </span>
                    <h4 className="mt-4 font-display text-base font-bold text-text">{pick(f.title, locale)}</h4>
                    <p className="mt-2 text-[13px] leading-relaxed text-muted">{pick(f.desc, locale)}</p>
                  </article>
                </Reveal>
              ))}
            </div>
          </div>

          <Reveal delay={60}>
            <div className="mt-5 grid gap-px overflow-hidden rounded-2xl border border-border bg-border sm:grid-cols-3">
              {t.privacy.engine.rows.map((r) => (
                <div key={r.k.zh} className="bg-surface px-5 py-4">
                  <p className="text-[11px] uppercase tracking-[0.2em] text-faint">{pick(r.k, locale)}</p>
                  <p className="mt-1.5 text-sm font-semibold text-text">{pick(r.v, locale)}</p>
                </div>
              ))}
            </div>
          </Reveal>
          <p className="mt-4 text-center text-xs text-faint">{pick(t.privacy.legal, locale)}</p>
        </div>

        {/* 真实查询示例 */}
        <div className="mt-24 grid items-center gap-8 lg:grid-cols-[1fr_1.15fr]">
          <Reveal>
            <h3 className="font-display text-2xl font-bold text-text sm:text-3xl">{pick(t.sample.title, locale)}</h3>
            <p className="mt-4 max-w-lg text-[15px] leading-relaxed text-muted">{pick(t.sample.caption, locale)}</p>
            <ul className="mt-6 flex flex-col gap-2.5 text-sm text-text">
              {(locale === "zh"
                ? ["字段与 ST 内部结构一一对应", "只读打开解密副本，原库零改动", "聚合全部本地完成，可复现"]
                : ["Fields map 1:1 to ST internals", "Read-only access to the decrypted copy, originals untouched", "Aggregation runs locally and is fully reproducible"]
              ).map((text, i) => (
                <li key={i} className="flex items-start gap-3">
                  <span className="mt-1 grid h-5 w-5 shrink-0 place-items-center rounded-full bg-accent/15 text-[11px] text-accent">✓</span>
                  {text}
                </li>
              ))}
            </ul>
          </Reveal>
          <Reveal delay={120}>
            <CodeBlock
              code={pick(t.sample.code, locale)}
              title={locale === "zh" ? "年度总结聚合 · SQLite" : "Annual summary aggregation · SQLite"}
              lang="sql"
            />
          </Reveal>
        </div>

        {/* 洞察能力清单 */}
        <div className="mt-24">
          <Reveal>
            <h3 className="text-center font-display text-2xl font-bold text-text sm:text-3xl">
              {pick(t.insights.title, locale)}
            </h3>
          </Reveal>
          <div className="mt-12 grid gap-5 md:grid-cols-3">
            {t.insights.items.map((item, i) => (
              <Reveal key={item.key} delay={(i % 3) * 80}>
                <article className="glass card-hover flex h-full items-start gap-4 rounded-2xl p-6">
                  <span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl border border-border bg-white/[0.03] text-lg">
                    {item.icon}
                  </span>
                  <div>
                    <h4 className="font-display text-base font-bold text-text">{pick(item.name, locale)}</h4>
                    <p className="mt-2 text-[13px] leading-relaxed text-muted">{pick(item.desc, locale)}</p>
                  </div>
                </article>
              </Reveal>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
