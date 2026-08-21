/** 博客：列表页 + 详情页（含 Article JSON-LD） */

import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { posts, postBySlug } from "@/lib/content/blog";
import { Reveal } from "@/components/ui/Reveal";
import type { Locale } from "@/lib/i18n/locales";

export function BlogIndexPage({ locale }: { locale: Locale }) {
  return (
    <div className="mx-auto max-w-5xl px-5 pb-24 pt-32 lg:px-8">
      <Reveal>
        <header className="max-w-2xl">
          <p className="font-mono text-xs uppercase tracking-[0.3em] text-accent">Blog</p>
          <h1 className="mt-4 font-display text-4xl font-extrabold text-text">
            {locale === "zh" ? "来自 Harness 团队的思考" : "Thinking from the HARNESS team"}
          </h1>
        </header>
      </Reveal>
      <div className="mt-12 flex flex-col gap-5">
        {posts.map((p, i) => (
          <Reveal key={p.slug} delay={i * 60}>
            <a
              href={`/${locale}/blog/${p.slug}/`}
              className="glass card-hover flex flex-col gap-4 rounded-2xl p-7 sm:flex-row sm:items-center"
            >
              <div className="flex flex-1 flex-col gap-3">
                <div className="flex items-center gap-3 font-mono text-[11px] uppercase tracking-wider">
                  <span className="rounded bg-accent/15 px-2 py-0.5 text-accent">{p.tag[locale]}</span>
                  <span className="text-faint">{p.date}</span>
                  <span className="text-faint">{p.readMinutes} min</span>
                </div>
                <h2 className="font-display text-xl font-bold leading-snug text-text">{p.title[locale]}</h2>
                <p className="line-clamp-2 text-sm leading-relaxed text-muted">{p.excerpt[locale]}</p>
              </div>
              <span className="shrink-0 text-xs font-semibold text-accent">
                {locale === "zh" ? "阅读全文 →" : "Read more →"}
              </span>
            </a>
          </Reveal>
        ))}
      </div>
    </div>
  );
}

export function blogMetadata(slug: string, locale: Locale): Metadata {
  const post = postBySlug(slug);
  if (!post) return {};
  return {
    title: post.title[locale],
    description: post.excerpt[locale],
    alternates: {
      canonical: `/${locale}/blog/${slug}/`,
      languages: { zh: `/zh/blog/${slug}/`, en: `/en/blog/${slug}/` },
    },
    openGraph: {
      type: "article",
      title: post.title[locale],
      description: post.excerpt[locale],
      publishedTime: post.date,
      authors: [post.author.name],
    },
  };
}

export function BlogDetailPage({ slug, locale }: { slug: string; locale: Locale }) {
  const post = postBySlug(slug);
  if (!post) notFound();

  const articleJsonLd = {
    "@context": "https://schema.org",
    "@type": "BlogPosting",
    headline: post.title[locale],
    description: post.excerpt[locale],
    datePublished: post.date,
    author: { "@type": "Person", name: post.author.name },
    inLanguage: locale === "zh" ? "zh-CN" : "en",
  };

  return (
    <article className="mx-auto max-w-3xl px-5 pb-24 pt-32 lg:px-8">
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(articleJsonLd) }}
      />
      <Reveal>
        <header>
          <div className="flex items-center gap-3 font-mono text-[11px] uppercase tracking-wider">
            <span className="rounded bg-accent/15 px-2 py-0.5 text-accent">{post.tag[locale]}</span>
            <span className="text-faint">{post.date}</span>
            <span className="text-faint">{post.readMinutes} min</span>
          </div>
          <h1 className="mt-5 font-display text-3xl font-extrabold leading-tight text-text sm:text-4xl">
            {post.title[locale]}
          </h1>
          <p className="mt-4 text-[15px] leading-relaxed text-muted">{post.excerpt[locale]}</p>
          <div className="mt-6 flex items-center gap-3 border-y border-border py-4">
            <span className="grid h-10 w-10 place-items-center rounded-full bg-gradient-to-br from-accent to-accent-2 font-mono text-sm font-bold text-white">
              {post.author.name[0]}
            </span>
            <div>
              <p className="text-sm font-semibold text-text">{post.author.name}</p>
              <p className="font-mono text-[11px] text-faint">{post.author.role[locale]}</p>
            </div>
          </div>
        </header>
      </Reveal>
      <div className="mt-10 flex flex-col gap-6">
        {post.body[locale].map((para, i) => (
          <Reveal key={i} delay={i * 40}>
            <p className="text-[15.5px] leading-[1.9] text-text/90">{para}</p>
          </Reveal>
        ))}
      </div>
      <nav className="mt-14 flex items-center justify-between border-t border-border pt-6">
        <a href={`/${locale}/blog/`} className="text-sm font-semibold text-accent">
          ← {locale === "zh" ? "返回博客" : "Back to blog"}
        </a>
        <a href={`/${locale}/changelog/`} className="text-sm font-semibold text-muted hover:text-text">
          {locale === "zh" ? "查看更新日志" : "See the changelog"} →
        </a>
      </nav>
    </article>
  );
}
