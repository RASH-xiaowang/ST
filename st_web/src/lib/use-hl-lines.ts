/**
 * 分行揭幕标题（.hl-line）触发：区块进入视口后给所有 .hl-line 加 .in，
 * 由 globals.css 的过渡完成 translateY 揭幕。尊重 prefers-reduced-motion。
 */
import { useEffect, type RefObject } from "react";

export function useHlLines(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const reveal = () => {
      el.querySelectorAll<HTMLElement>(".hl-line").forEach((l) => l.classList.add("in"));
    };
    const reduced =
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduced) {
      reveal();
      return;
    }
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            reveal();
            io.disconnect();
            break;
          }
        }
      },
      { threshold: 0.15, rootMargin: "0px 0px -10% 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [ref]);
}
