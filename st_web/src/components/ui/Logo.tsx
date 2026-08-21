/** 品牌标识：六边形核心 + 轨道环 + 字标 */

export function LogoMark({ className = "h-8 w-8" }: { className?: string }) {
  return (
    <svg viewBox="0 0 48 48" className={className} aria-hidden="true">
      <defs>
        <linearGradient id="hns-logo" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="var(--accent)" />
          <stop offset="0.55" stopColor="var(--accent-2)" />
          <stop offset="1" stopColor="var(--accent-3)" />
        </linearGradient>
      </defs>
      {/* 轨道环 */}
      <ellipse
        cx="24"
        cy="24"
        rx="20"
        ry="8.5"
        fill="none"
        stroke="url(#hns-logo)"
        strokeWidth="1.4"
        transform="rotate(-24 24 24)"
        opacity="0.75"
      />
      {/* 六边形核心 */}
      <path
        d="M24 8.5 37.4 16.25v15.5L24 39.5 10.6 31.75v-15.5Z"
        fill="color-mix(in oklab, var(--accent) 16%, transparent)"
        stroke="url(#hns-logo)"
        strokeWidth="1.6"
        strokeLinejoin="round"
      />
      {/* 核心发光点 */}
      <circle cx="24" cy="23.9" r="5" fill="url(#hns-logo)" />
      <circle cx="24" cy="23.9" r="9" fill="none" stroke="url(#hns-logo)" strokeWidth="0.8" opacity="0.5" />
    </svg>
  );
}

export function Logo({
  locale,
  compact = false,
}: {
  locale: string;
  compact?: boolean;
}) {
  return (
    <span className="inline-flex items-center gap-2.5 select-none">
      <LogoMark className="h-8 w-8" />
      {!compact && (
        <span className="flex flex-col leading-none">
          <span className="font-display text-[15px] font-bold tracking-[0.18em] text-text">
            ST CONTROL
          </span>
          <span className="mt-1 font-mono text-[9px] uppercase tracking-[0.28em] text-faint">
            {locale === "zh" ? "智能控制台" : "Control Console"}
          </span>
        </span>
      )}
    </span>
  );
}
