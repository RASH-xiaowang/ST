"use client";

/**
 * 2D 静态降级视觉：WebGL 不可用 / 低端设备 / 减动效偏好时使用。
 * 纯 CSS + SVG，保证内容与氛围不缺失。
 */

export function StaticHero({ className = "" }: { className?: string }) {
  return (
    <div
      className={`pointer-events-none absolute inset-0 overflow-hidden ${className}`}
      aria-hidden="true"
      data-testid="hero-canvas"
    >
      {/* 光晕 */}
      <div className="absolute left-1/2 top-1/3 h-[520px] w-[520px] -translate-x-1/2 -translate-y-1/2 rounded-full opacity-60 blur-3xl"
        style={{ background: "radial-gradient(circle, color-mix(in oklab, var(--accent) 45%, transparent) 0%, transparent 65%)" }} />
      <div className="absolute right-[8%] top-[12%] h-72 w-72 rounded-full opacity-40 blur-3xl"
        style={{ background: "radial-gradient(circle, color-mix(in oklab, var(--accent-3) 40%, transparent), transparent 70%)" }} />
      {/* 网格 */}
      <div className="grid-overlay absolute inset-0" />
      {/* 六边形徽标 */}
      <svg viewBox="0 0 400 400" className="absolute left-1/2 top-[42%] w-[440px] -translate-x-1/2 -translate-y-1/2 opacity-30">
        <g fill="none" stroke="var(--accent)" strokeWidth="1">
          <path d="M200 40 340 120v160L200 360 60 280V120Z" />
          <path d="M200 90 295 145v110L200 310 105 255V145Z" opacity=".6" />
          <circle cx="200" cy="200" r="52" stroke="var(--accent-2)" />
          <circle cx="200" cy="200" r="86" stroke="var(--accent-3)" opacity=".55" />
          <path d="M200 52v44M340 120l-38 22M200 348v-44M60 120l38 22" opacity=".4" />
        </g>
      </svg>
    </div>
  );
}

export function StaticProduct({ className = "" }: { className?: string }) {
  return (
    <div className={`grid place-items-center ${className}`} aria-hidden="true">
      <svg viewBox="0 0 360 360" className="h-full max-h-[420px] w-auto">
        <defs>
          <linearGradient id="sp-a" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" stopColor="var(--accent)" />
            <stop offset="1" stopColor="var(--accent-2)" />
          </linearGradient>
        </defs>
        <g fill="none" strokeWidth="1.4">
          <ellipse cx="180" cy="292" rx="132" ry="30" stroke="var(--accent)" opacity=".5" />
          <ellipse cx="180" cy="180" rx="118" ry="118" stroke="url(#sp-a)" opacity=".8" />
          <ellipse cx="180" cy="180" rx="150" ry="52" stroke="var(--accent-2)" opacity=".5" transform="rotate(-24 180 180)" />
          <ellipse cx="180" cy="180" rx="150" ry="52" stroke="var(--accent-3)" opacity=".4" transform="rotate(36 180 180)" />
          <path d="M180 92 256 136v88l-76 44-76-44v-88Z" stroke="url(#sp-a)" strokeWidth="2" />
          <circle cx="180" cy="180" r="34" stroke="url(#sp-a)" strokeWidth="2" />
          <circle cx="180" cy="180" r="8" fill="url(#sp-a)" stroke="none" />
        </g>
      </svg>
    </div>
  );
}

export function StaticChart({ className = "" }: { className?: string }) {
  const bars = [42, 66, 50, 78, 58, 88, 64, 92, 72, 84];
  return (
    <div className={`flex items-end justify-center gap-3 ${className}`} aria-hidden="true">
      {bars.map((h, i) => (
        <div key={i} className="flex w-8 flex-col items-center gap-2">
          <div
            className="w-full rounded-t-md"
            style={{
              height: `${h * 2.4}px`,
              background: `linear-gradient(180deg, var(--accent), var(--accent-2) ${
                60 + i * 3
              }%, transparent)`,
              opacity: 0.85,
            }}
          />
          <span className="h-1.5 w-1.5 rounded-full bg-faint" />
        </div>
      ))}
    </div>
  );
}
