"use client";

/** 代码块：终端壳样式 + 语言标签 + 复制按钮 */

import { useState } from "react";

export function CodeBlock({
  code,
  title,
  lang = "bash",
}: {
  code: string;
  title?: string;
  lang?: string;
}) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      /* clipboard unavailable */
    }
  };

  return (
    <figure className="code-shell overflow-hidden rounded-xl font-mono text-[13px] leading-relaxed shadow-[0_24px_70px_-30px_rgba(0,0,0,.55)]">
      <figcaption className="flex items-center gap-2 border-b border-white/10 px-4 py-2.5">
        <span className="flex gap-1.5" aria-hidden="true">
          <i className="h-2.5 w-2.5 rounded-full bg-[#ff5f57]/80" />
          <i className="h-2.5 w-2.5 rounded-full bg-[#febc2e]/80" />
          <i className="h-2.5 w-2.5 rounded-full bg-[#28c840]/80" />
        </span>
        {title && <span className="text-xs text-white/60">{title}</span>}
        <span className="ml-auto rounded border border-white/15 px-1.5 py-0.5 text-[10px] uppercase text-white/50">
          {lang}
        </span>
        <button
          onClick={copy}
          className="rounded border border-white/15 px-2 py-0.5 text-[10px] text-white/70 transition hover:bg-white/10"
        >
          {copied ? "✓" : "copy"}
        </button>
      </figcaption>
      <pre className="overflow-x-auto px-4 py-4">
        <code>{code}</code>
      </pre>
    </figure>
  );
}
