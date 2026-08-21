"use client";

/** 主题切换按钮：日/月图标，title 双语 */

import { useTheme } from "@/lib/theme";

export function ThemeToggle({ label }: { label: { dark: string; light: string } }) {
  const { resolved, toggle } = useTheme();
  const isDark = resolved === "dark";

  return (
    <button
      onClick={toggle}
      aria-label={isDark ? label.light : label.dark}
      title={isDark ? label.light : label.dark}
      className="grid h-9 w-9 place-items-center rounded-lg border border-border text-muted transition hover:border-border-2 hover:text-text"
      data-testid="theme-toggle"
    >
      {isDark ? (
        /* 太阳 */
        <svg viewBox="0 0 24 24" className="h-[18px] w-[18px]" fill="none" aria-hidden="true">
          <circle cx="12" cy="12" r="4" stroke="currentColor" strokeWidth="1.6" />
          <path
            d="M12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.6 4.6l1.8 1.8M17.6 17.6l1.8 1.8M19.4 4.6l-1.8 1.8M6.4 17.6l-1.8 1.8"
            stroke="currentColor"
            strokeWidth="1.6"
            strokeLinecap="round"
          />
        </svg>
      ) : (
        /* 月亮 */
        <svg viewBox="0 0 24 24" className="h-[18px] w-[18px]" fill="none" aria-hidden="true">
          <path
            d="M20 13.5A8 8 0 0 1 10.5 4a8 8 0 1 0 9.5 9.5Z"
            stroke="currentColor"
            strokeWidth="1.6"
            strokeLinejoin="round"
          />
        </svg>
      )}
    </button>
  );
}
