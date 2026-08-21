import type { MetadataRoute } from "next";
import { docPages } from "@/lib/content/docs";
import { posts } from "@/lib/content/blog";

export const dynamic = "force-static";

const BASE = process.env.NEXT_PUBLIC_SITE_URL ?? "https://st-control.dev";

function entry(url: string, priority: number, changeFrequency: MetadataRoute.Sitemap[number]["changeFrequency"]): MetadataRoute.Sitemap[number] {
  return { url: `${BASE}${url}`, lastModified: new Date(), changeFrequency, priority };
}

export default function sitemap(): MetadataRoute.Sitemap {
  const locales = ["zh", "en"];
  const out: MetadataRoute.Sitemap = [];

  out.push(entry("/", 0.6, "monthly"));
  for (const l of locales) {
    out.push(entry(`/${l}/`, 1, "weekly"));
    out.push(entry(`/${l}/docs/`, 0.8, "weekly"));
    out.push(entry(`/${l}/blog/`, 0.7, "weekly"));
    out.push(entry(`/${l}/changelog/`, 0.6, "weekly"));
    out.push(entry(`/${l}/roadmap/`, 0.6, "monthly"));
    out.push(entry(`/${l}/contact/`, 0.7, "monthly"));
    out.push(entry(`/${l}/search/`, 0.3, "monthly"));
    for (const d of docPages) out.push(entry(`/${l}/docs/${d.slug}/`, 0.7, "monthly"));
    for (const p of posts) out.push(entry(`/${l}/blog/${p.slug}/`, 0.6, "monthly"));
  }
  return out;
}
