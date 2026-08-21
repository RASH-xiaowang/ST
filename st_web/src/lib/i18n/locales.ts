/** 站点国际化核心：locale 类型、候选列表与工具函数 */

export type Locale = "zh" | "en";

export const LOCALES: Locale[] = ["zh", "en"];

export const LOCALE_META: Record<
  Locale,
  { code: Locale; label: string; native: string; dir: "ltr" }
> = {
  zh: { code: "zh", label: "Chinese", native: "中文", dir: "ltr" },
  en: { code: "en", label: "English", native: "English", dir: "ltr" },
};

export function isLocale(v: string | undefined | null): v is Locale {
  return v === "zh" || v === "en";
}

export function fallbackLocale(v: string | undefined | null): Locale {
  return isLocale(v) ? v : "zh";
}

/** 双语内容结构：所有内容字段都按 locale 提供 */
export type Bi<T> = { zh: T; en: T };

export function pick<T>(bi: Bi<T>, locale: Locale): T {
  return bi[locale];
}

export function siteUrl(locale: Locale, path = ""): string {
  const base = process.env.NEXT_PUBLIC_SITE_URL ?? "https://st-control.dev";
  const p = path ? `/${path.replace(/^\//, "")}` : "";
  return locale === "zh" ? `${base}/zh${p}` : `${base}/en${p}`;
}
