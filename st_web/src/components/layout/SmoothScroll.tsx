"use client";

/**
 * 平滑滚动（Lenis）：统一管理 rAF 与锚点导航。
 * 尊重 prefers-reduced-motion；SSR 安全。
 */
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import Lenis from "lenis";

type SmoothScrollCtx = {
  scrollTo: (target: string | number) => void;
};

const Ctx = createContext<SmoothScrollCtx>({ scrollTo: () => {} });

export function useSmoothScroll() {
  return useContext(Ctx);
}

export function SmoothScrollProvider({ children }: { children: ReactNode }) {
  const [api, setApi] = useState<SmoothScrollCtx>({ scrollTo: () => {} });

  useEffect(() => {
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduced) {
      setApi({
        scrollTo: (target) => {
          if (typeof target === "number") {
            window.scrollTo({ top: target });
          } else {
            const el = document.querySelector(target);
            el?.scrollIntoView({ block: "start" });
          }
        },
      });
      return;
    }
    const lenis = new Lenis({
      duration: 1.1,
      easing: (t) => Math.min(1, 1.001 - Math.pow(2, -10 * t)),
      smoothWheel: true,
    });
    let raf = 0;
    const loop = (time: number) => {
      lenis.raf(time);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    setApi({
      scrollTo: (target) => {
        if (typeof target === "number") {
          lenis.scrollTo(target);
        } else {
          const el = document.querySelector(target);
          if (el) lenis.scrollTo(el as HTMLElement, { offset: -72 });
          else lenis.scrollTo(0);
        }
      },
    });
    return () => {
      cancelAnimationFrame(raf);
      lenis.destroy();
    };
  }, []);

  return <Ctx.Provider value={api}>{children}</Ctx.Provider>;
}
