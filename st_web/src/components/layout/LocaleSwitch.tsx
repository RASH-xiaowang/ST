"use client";

/** 语言切换器：zh / en 双向，持久化选择，保持当前路径语义 */

import { usePathname, useRouter } from "next/navigation";

const STORAGE_KEY = "harness-locale";

export function LocaleSwitch({ locale }: { locale: string }) {
  const pathname = usePathname();
  const router = useRouter();

  const switchTo = (next: string) => {
    if (next === locale) return;
    try {
      window.localStorage.setItem(STORAGE_KEY, next);
    } catch {
      /* ignore */
    }
    // /zh/<rest> -> /en/<rest>
    const rest = pathname.replace(new RegExp(`^/${locale}`), "");
    router.push(`/${next}${rest || "/"}`);
  };

  return (
    <div
      className="flex items-center rounded-lg border border-border p-0.5 font-mono text-[11px]"
      role="group"
      aria-label="Language / 语言"
      data-testid="locale-switch"
    >
      {(["zh", "en"] as const).map((code) => (
        <button
          key={code}
          onClick={() => switchTo(code)}
          aria-pressed={locale === code}
          className={`rounded-md px-2 py-1 uppercase tracking-wider transition ${
            locale === code
              ? "bg-accent/15 text-accent"
              : "text-muted hover:text-text"
          }`}
        >
          {code}
        </button>
      ))}
    </div>
  );
}
