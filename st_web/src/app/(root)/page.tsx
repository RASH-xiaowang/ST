"use client";

/**
 * 根路由 /：按 localStorage 语言 / 浏览器语言重定向到 /zh 或 /en。
 * 无 JS 环境由 meta refresh 兜底。
 */
import { useEffect } from "react";

const STORAGE_KEY = "harness-locale";

export default function RootRedirectPage() {
  useEffect(() => {
    let target: string | null = null;
    try {
      target = window.localStorage.getItem(STORAGE_KEY);
    } catch {
      /* ignore */
    }
    if (target !== "zh" && target !== "en") {
      target =
        typeof navigator !== "undefined" &&
        navigator.language?.toLowerCase().startsWith("zh")
          ? "zh"
          : "en";
    }
    window.location.replace(`/${target}/`);
  }, []);

  return (
    <main className="flex min-h-screen items-center justify-center bg-[#04060d] text-white">
      <noscript>
        <meta httpEquiv="refresh" content="0;url=/zh/" />
      </noscript>
      <div className="flex flex-col items-center gap-4">
        <div className="h-10 w-10 animate-spin rounded-full border-2 border-cyan-400/30 border-t-cyan-400" />
        <p className="font-mono text-sm text-cyan-200/70">HARNESS · redirecting…</p>
      </div>
    </main>
  );
}
