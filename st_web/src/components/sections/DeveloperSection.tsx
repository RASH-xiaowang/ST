/** 开发者区块：ACP 代码示例 + API 方法总览 */

import { Reveal } from "@/components/ui/Reveal";
import { SectionHeading } from "@/components/ui/SectionHeading";
import { CodeBlock } from "@/components/ui/CodeBlock";
import { developer } from "@/lib/content/site";
import { pick, type Locale } from "@/lib/i18n/locales";

export function DeveloperSection({ locale }: { locale: Locale }) {
  return (
    <section id="developers" className="relative scroll-mt-20 py-24 lg:py-32">
      <div className="mx-auto max-w-7xl px-5 lg:px-8">
        <Reveal>
          <SectionHeading
            index="06"
            eyebrow={locale === "zh" ? "开发者" : "Developers"}
            title={pick(developer.title, locale)}
            subtitle={pick(developer.subtitle, locale)}
          />
        </Reveal>

        <div className="mt-14 grid items-start gap-8 lg:grid-cols-[1.15fr_1fr]">
          <Reveal>
            <CodeBlock
              code={developer.sample}
              title={locale === "zh" ? "ACP 会话协议 · JSON-RPC" : "ACP session protocol · JSON-RPC"}
              lang="bash"
            />
          </Reveal>
          <Reveal delay={100}>
            <div className="glass rounded-2xl p-6 sm:p-7">
              <h3 className="font-display text-lg font-bold text-text">
                {locale === "zh" ? "API 参考概览" : "API overview"}
              </h3>
              <ul className="mt-5 flex flex-col divide-y divide-border">
                {developer.api.map((m) => (
                  <li key={m.name} className="flex flex-col gap-1 py-3.5">
                    <code className="font-mono text-[13px] font-bold text-accent">{m.name}</code>
                    <span className="text-[13px] text-muted">{pick(m.desc, locale)}</span>
                  </li>
                ))}
              </ul>
              <a
                href={`/${locale}/docs/api/`}
                className="mt-5 inline-flex items-center gap-2 rounded-lg border border-border px-4 py-2 text-sm font-semibold text-text transition hover:border-accent/50 hover:text-accent"
              >
                {locale === "zh" ? "阅读完整文档" : "Read the full docs"} →
              </a>
            </div>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
