"use client";

/**
 * ACT 01 · Hero — 3D 背景 + 终端 HUD 美学 + 乱码还原副标 + 跑马灯
 */
import { useEffect } from "react";
import { BackgroundScene } from "@/components/three/BackgroundScene";
import { ScrambleText } from "@/components/ui/ScrambleText";
import { brand } from "@/lib/content/site";
import { pick, type Locale } from "@/lib/i18n/locales";
import { useSmoothScroll } from "@/components/layout/SmoothScroll";

const TICKER = [
  "100% 本地运行",
  "微信数据解密",
  "知识库混合检索",
  "Harness 智能代理",
  "沙箱三模式",
  "0 字节出网",
];

export function Hero({ locale }: { locale: Locale }) {
  const { scrollTo } = useSmoothScroll();

  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      window.__hnsPointerX = (e.clientX / window.innerWidth) * 2 - 1;
      window.__hnsPointerY = (e.clientY / window.innerHeight) * 2 - 1;
    };
    window.addEventListener("pointermove", onMove, { passive: true });
    return () => window.removeEventListener("pointermove", onMove);
  }, []);

  const t = (bi: { zh: string; en: string }) => pick(bi, locale);

  return (
    <section className="relative flex min-h-[100svh] flex-col overflow-hidden" aria-label={t(brand.slogan)}>
      <BackgroundScene className="absolute inset-0" />

      {/* 终端 HUD 角标 */}
      <span className="hud-corner left-6 top-24" aria-hidden="true">ST CONTROL — LOCAL FIRST</span>
      <span className="hud-corner right-6 top-24" aria-hidden="true">RUNTIME — 0 UPLOAD</span>
      <span className="hud-corner bottom-24 right-6 hidden sm:block" aria-hidden="true">PLATFORM — WINDOWS NATIVE</span>
      <span className="hud-corner bottom-24 left-6 hidden sm:block" aria-hidden="true">WECHAT × LLM × AUTOMATION</span>

      <div className="relative z-10 mx-auto flex w-full max-w-7xl flex-1 flex-col items-center justify-center px-5 pb-24 pt-36 text-center">
        <div className="inline-flex items-center gap-2 rounded-full border border-border bg-surface px-4 py-1.5 font-mono text-[11px] uppercase tracking-[0.2em] text-muted">
          <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-ok" />
          {t(brand.heroBadge)}
        </div>

        <h1 className="mt-8 max-w-4xl text-balance font-display text-5xl font-extrabold leading-[1.06] tracking-tight text-text sm:text-6xl lg:text-7xl">
          <span className="text-metal">{t(brand.slogan)}</span>
          <br />
          <span className="outline-text">ST CONTROL</span>
        </h1>

        <ScrambleText
          text={t(brand.heroSub)}
          className="mt-7 block max-w-2xl text-base leading-relaxed text-muted sm:text-lg"
          as="p"
        />

        <div className="mt-10 flex flex-col items-center justify-center gap-3 sm:flex-row sm:gap-4">
          <a
            href={`/${locale}/docs/`}
            className="w-full rounded-xl bg-gradient-to-r from-accent via-accent-2 to-accent-3 px-7 py-3.5 text-center text-[15px] font-semibold text-white shadow-[0_18px_50px_-16px_var(--glow)] transition hover:brightness-110 sm:w-auto"
            data-testid="hero-cta-primary"
          >
            {t(brand.ctaPrimary)}
          </a>
          <a
            href={`/${locale}/#modules`}
            className="glass w-full rounded-xl px-7 py-3.5 text-center text-[15px] font-semibold text-text transition hover:border-border-2 sm:w-auto"
          >
            {t(brand.ctaSecondary)}
          </a>
        </div>

        {/* 跑马灯 */}
        <div className="ticker mt-14 w-full max-w-4xl opacity-80" aria-hidden="true">
          <div className="ticker__row">
            {[...TICKER, ...TICKER].map((item, i) => (
              <span key={i} className="flex items-center gap-7">
                {item}<i>◆</i>
              </span>
            ))}
          </div>
        </div>
      </div>

      {/* 滚动引导 */}
      <button
        onClick={() => scrollTo("#manifesto")}
        className="glass absolute bottom-8 left-1/2 z-10 flex -translate-x-1/2 flex-col items-center gap-2 rounded-full px-4 py-2 text-muted transition hover:text-accent"
        aria-label={t(brand.scrollHint)}
      >
        <span className="font-mono text-[10px] uppercase tracking-[0.3em]">{t(brand.scrollHint)}</span>
        <span className="flex h-9 w-5 items-start justify-center rounded-full border border-current p-1">
          <span className="h-2 w-1 animate-bounce rounded-full bg-current" />
        </span>
      </button>
    </section>
  );
}
