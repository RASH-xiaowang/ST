/**
 * 站内搜索索引与检索（内容源即索引：纯客户端构建，无构建脚本依赖）
 */

import type { Bi, Locale } from "@/lib/i18n/locales";
import { docPages } from "@/lib/content/docs";
import { posts } from "@/lib/content/blog";
import { faqs } from "@/lib/content/faq";
import { cases } from "@/lib/content/cases";
import { changelog, roadmap } from "@/lib/content/changelog";
import { scenarios, overview, architecture, brand } from "@/lib/content/site";

export type SearchEntry = {
  type: "doc" | "blog" | "faq" | "case" | "changelog" | "scenario" | "page";
  typeLabel: string;
  href: string;
  title: string;
  snippet: string;
  keywords: string;
};

function buildIndex(locale: Locale): SearchEntry[] {
  const out: SearchEntry[] = [];
  const t = <T,>(l: Bi<T>) => l[locale];

  out.push({
    type: "page",
    typeLabel: locale === "zh" ? "页面" : "Page",
    href: `/${locale}/`,
    title: `${brand.name} — ${t(brand.product)}`,
    snippet: t(brand.slogan) + " · " + t(brand.heroSub),
    keywords: "harness agent runtime 智能代理 本地优先",
  });

  for (const p of docPages) {
    out.push({
      type: "doc",
      typeLabel: locale === "zh" ? "文档" : "Docs",
      href: `/${locale}/docs/${p.slug}/`,
      title: t(p.title),
      snippet: t(p.summary),
      keywords: [t(p.title), ...p.sections.flatMap((s) => [t(s.heading), ...t(s.body)])].join(" "),
    });
  }

  for (const p of posts) {
    out.push({
      type: "blog",
      typeLabel: locale === "zh" ? "博客" : "Blog",
      href: `/${locale}/blog/${p.slug}/`,
      title: t(p.title),
      snippet: t(p.excerpt),
      keywords: t(p.title) + " " + t(p.body).join(" "),
    });
  }

  for (const f of faqs) {
    out.push({
      type: "faq",
      typeLabel: locale === "zh" ? "常见问题" : "FAQ",
      href: `/${locale}/#faq`,
      title: t(f.q),
      snippet: t(f.a).slice(0, 160),
      keywords: t(f.q) + " " + t(f.a),
    });
  }

  for (const c of cases) {
    out.push({
      type: "case",
      typeLabel: locale === "zh" ? "客户案例" : "Case",
      href: `/${locale}/#customers`,
      title: `${c.name} — ${t(c.title)}`,
      snippet: t(c.summary),
      keywords: `${c.name} ${t(c.title)} ${t(c.summary)} ${t(c.detail).join(" ")}`,
    });
  }

  for (const e of changelog) {
    out.push({
      type: "changelog",
      typeLabel: locale === "zh" ? "更新日志" : "Changelog",
      href: `/${locale}/changelog/`,
      title: `v${e.version} ${t(e.title)}`,
      snippet: t(e.items)[0] ?? "",
      keywords: `v${e.version} ${t(e.title)} ${t(e.items).join(" ")}`,
    });
  }

  for (const s of scenarios.list) {
    out.push({
      type: "scenario",
      typeLabel: locale === "zh" ? "应用场景" : "Scenario",
      href: `/${locale}/#features`,
      title: t(s.name),
      snippet: t(s.solve),
      keywords: `${t(s.name)} ${t(s.pain)} ${t(s.solve)} ${s.tags.map(t).join(" ")}`,
    });
  }

  for (const f of overview.features) {
    out.push({
      type: "page",
      typeLabel: locale === "zh" ? "特性" : "Feature",
      href: `/${locale}/#features`,
      title: f[locale],
      snippet: f[locale],
      keywords: f[locale],
    });
  }

  for (const m of architecture.metrics) {
    out.push({
      type: "page",
      typeLabel: locale === "zh" ? "指标" : "Metric",
      href: `/${locale}/#machine`,
      title: `${m.name[locale]} ${m.value}${m.unit}`,
      snippet: m.note[locale],
      keywords: m.name[locale] + " " + m.note[locale],
    });
  }

  for (const item of roadmap.flatMap((ph) => ph.items)) {
    out.push({
      type: "page",
      typeLabel: locale === "zh" ? "路线图" : "Roadmap",
      href: `/${locale}/roadmap/`,
      title: item.title[locale],
      snippet: item.desc[locale],
      keywords: item.title[locale] + " " + item.desc[locale],
    });
  }

  return out;
}

const cache = new Map<Locale, SearchEntry[]>();

export function searchIndex(locale: Locale): SearchEntry[] {
  const hit = cache.get(locale);
  if (hit) return hit;
  const idx = buildIndex(locale);
  cache.set(locale, idx);
  return idx;
}

/** 多词 AND 模糊检索：标题/正文/关键词全命中，按类型加权排序 */
export function searchAll(query: string, locale: Locale, limit = 24): SearchEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const terms = q.split(/\s+/).filter(Boolean);
  const scored = searchIndex(locale)
    .map((e) => {
      const hay = `${e.title} ${e.snippet} ${e.keywords}`.toLowerCase();
      let score = 0;
      for (const term of terms) {
        if (!hay.includes(term)) return null;
        score += e.title.toLowerCase().includes(term) ? 3 : 1;
      }
      const typeBoost =
        e.type === "doc" ? 2 : e.type === "faq" ? 1.5 : e.type === "page" ? 1 : 0.8;
      return { e, score: score * typeBoost };
    })
    .filter((x): x is { e: SearchEntry; score: number } => x !== null)
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map((x) => x.e);
  return scored;
}
