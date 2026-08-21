"use client";

/** 滚动进入时文字乱码还原（自实现，尊重 reduced-motion） */
import { useEffect, useRef } from "react";

const CHARS = "!<>-_\\/[]{}—=+*^?#01";

export function ScrambleText({
  text,
  className = "",
  as: Tag = "span",
}: {
  text: string;
  className?: string;
  as?: "span" | "p" | "h3";
}) {
  const ref = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const reduced =
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduced) {
      el.textContent = text;
      return;
    }
    let raf = 0;
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries[0]?.isIntersecting) return;
        io.disconnect();
        const start = performance.now();
        const dur = 850;
        const tick = (now: number) => {
          const p = Math.min(1, (now - start) / dur);
          const reveal = Math.floor(p * text.length);
          let out = "";
          for (let i = 0; i < text.length; i++) {
            out += i < reveal ? text[i] : CHARS[Math.floor(Math.random() * CHARS.length)];
          }
          el.textContent = out;
          if (p < 1) raf = requestAnimationFrame(tick);
          else el.textContent = text;
        };
        raf = requestAnimationFrame(tick);
      },
      { threshold: 0.4 },
    );
    io.observe(el);
    return () => {
      io.disconnect();
      cancelAnimationFrame(raf);
    };
  }, [text]);

  return (
    <Tag ref={ref as never} className={className} aria-label={text}>
      {text}
    </Tag>
  );
}
