/** 文档中心：列表页 + 详情页 */

import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { docPages } from "@/lib/content/docs";
import { CodeBlock } from "@/components/ui/CodeBlock";
import { Reveal } from "@/components/ui/Reveal";
import type { Locale } from "@/lib/i18n/locales";

export function DocsIndexPage({ locale }: { locale: Locale }) {
  const groups = [...new Set(docPages.map((d) => d.group[locale]))];
  return (
    <div className="mx-auto max-w-6xl px-5 pb-24 pt-32 lg:px-8">
      <Reveal>
        <header className="max-w-2xl">
          <p className="font-mono text-xs uppercase tracking-[0.3em] text-accent">
            {locale === "zh" ? "文档中心" : "Documentation"}
          </p>
          <h1 className="mt-4 font-display text-4xl font-extrabold text-text">
            {locale === "zh" ? "从零开始，十分钟上手" : "From zero to running in ten minutes"}
          </h1>
          <p className="mt-4 text-[15px] leading-relaxed text-muted">
            {locale === "zh"
              ? "安装、配置模型、发出第一条流式对话——以及架构、治理、API 与合规的全部细节。"
              : "Install, configure a model and send your first streaming message — plus everything on architecture, governance, APIs and compliance."}
          </p>
        </header>
      </Reveal>

      {groups.map((g) => (
        <section key={g} className="mt-14">
          <h2 className="font-mono text-xs font-bold uppercase tracking-[0.24em] text-faint">{g}</h2>
          <div className="mt-4 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {docPages
              .filter((d) => d.group[locale] === g)
              .sort((a, b) => a.order - b.order)
              .map((d) => (
                <a
                  key={d.slug}
                  href={`/${locale}/docs/${d.slug}/`}
                  className="glass card-hover rounded-2xl p-6"
                >
                  <h3 className="font-display text-[16px] font-bold text-text">{d.title[locale]}</h3>
                  <p className="mt-2 text-[13px] leading-relaxed text-muted">{d.summary[locale]}</p>
                  <span className="mt-4 inline-block text-xs font-semibold text-accent">
                    {locale === "zh" ? "阅读 →" : "Read →"}
                  </span>
                </a>
              ))}
          </div>
        </section>
      ))}
    </div>
  );
}

export function docMetadata(slug: string, locale: Locale): Metadata {
  const doc = docPages.find((d) => d.slug === slug);
  if (!doc) return {};
  return {
    title: doc.title[locale],
    description: doc.summary[locale],
    alternates: {
      canonical: `/${locale}/docs/${slug}/`,
      languages: { zh: `/zh/docs/${slug}/`, en: `/en/docs/${slug}/` },
    },
  };
}

export function DocDetailPage({ slug, locale }: { slug: string; locale: Locale }) {
  const doc = docPages.find((d) => d.slug === slug);
  if (!doc) notFound();

  return (
    <article className="mx-auto max-w-3xl px-5 pb-24 pt-32 lg:px-8">
      <Reveal>
        <header>
          <p className="font-mono text-xs uppercase tracking-[0.3em] text-accent">
            {locale === "zh" ? "文档" : "Docs"} · {doc.group[locale]}
          </p>
          <h1 className="mt-4 font-display text-4xl font-extrabold text-text">{doc.title[locale]}</h1>
          <p className="mt-4 text-[15px] leading-relaxed text-muted">{doc.summary[locale]}</p>
        </header>
      </Reveal>

      {doc.sections.map((s, i) => (
        <section key={i} className="mt-12">
          <Reveal>
            <h2 className="font-display text-2xl font-bold text-text">{s.heading[locale]}</h2>
            {s.body[locale].map((para, j) => (
              <p key={j} className="mt-4 text-[15px] leading-relaxed text-muted">
                {para}
              </p>
            ))}
            {s.code && (
              <div className="mt-5">
                <CodeBlock code={s.code.source} title={s.code.title[locale]} />
              </div>
            )}
          </Reveal>
        </section>
      ))}

      <nav className="mt-16 flex items-center justify-between border-t border-border pt-6">
        <a href={`/${locale}/docs/`} className="text-sm font-semibold text-accent">
          ← {locale === "zh" ? "返回文档中心" : "Back to docs"}
        </a>
        <a href={`/${locale}/contact/`} className="text-sm font-semibold text-muted hover:text-text">
          {locale === "zh" ? "还有问题？联系我们" : "Still stuck? Contact us"} →
        </a>
      </nav>
    </article>
  );
}
