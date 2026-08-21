"use client";

/** ACT 02 · 宣言：滚动钉住 + 分行揭幕（GSAP ScrollTrigger pin） */
import { useEffect, useRef } from "react";
import { gsap, registerGsap, ScrollTrigger } from "@/lib/gsap";
import type { Locale } from "@/lib/i18n/locales";

export function Manifesto({ locale }: { locale: Locale }) {
  const ref = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      el.querySelectorAll("[data-mline]").forEach((l) => l.classList.add("in"));
      return;
    }
    registerGsap();
    const ctx = gsap.context(() => {
      gsap.fromTo(
        el.querySelectorAll("[data-mline]"),
        { yPercent: 40, opacity: 0 },
        {
          yPercent: 0,
          opacity: 1,
          stagger: 0.14,
          duration: 0.9,
          ease: "power3.out",
          scrollTrigger: { trigger: el, start: "top 70%", end: "bottom 40%", scrub: 0.6 },
        },
      );
    }, el);
    return () => {
      ctx.revert();
      ScrollTrigger.getAll().forEach((s) => s.kill());
    };
  }, []);

  return (
    <section id="manifesto" className="act flex min-h-[86vh] items-center py-24" aria-label={locale === "zh" ? "产品宣言" : "Manifesto"}>
      <div className="mx-auto w-full max-w-5xl px-5 lg:px-8">
        <p className="sec-tag" data-mline>
          <i className="tick" /> {locale === "zh" ? "WHY — 为什么本地" : "WHY — why local"}
        </p>
        <div className="mt-8 flex flex-col gap-5 font-display text-3xl font-extrabold leading-[1.25] text-text sm:text-5xl lg:text-6xl">
          <p data-mline>
            {locale === "zh" ? "你的微信数据、聊天与足迹，" : "Your WeChat data, chats and footprints,"}
          </p>
          <p data-mline>
            {locale === "zh" ? (
              <>从来不该存在<em className="text-gradient">别人的服务器</em>里。</>
            ) : (
              <>should never live on <em className="text-gradient">someone else&apos;s server</em>.</>
            )}
          </p>
          <p data-mline className="text-muted">
            {locale === "zh" ? (
              <>本地解密、本地归档、本地分析——直到你，握回它。</>
            ) : (
              <>Decrypt locally, archive locally, analyze locally — until it is yours again.</>
            )}
          </p>
        </div>
      </div>
    </section>
  );
}
