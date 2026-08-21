"use client";

/** ACT 05 · Wrapped — 年度数据叙事（滚动计数 + 截图轮播） */
import { useEffect, useRef, useState } from "react";
import Image from "next/image";
import { Counter } from "@/components/ui/Counter";
import { useHlLines } from "@/lib/use-hl-lines";
import type { Locale } from "@/lib/i18n/locales";

export function WrappedSection({ locale }: { locale: Locale }) {
  const t = locale === "zh";
  const secRef = useRef<HTMLElement | null>(null);
  useHlLines(secRef);
  const [frame, setFrame] = useState(0);
  const shots = [
    { src: "/screenshots/dashboard.webp", alt: t ? "数据看板" : "Dashboard" },
    { src: "/screenshots/wechat-graph.webp", alt: t ? "社交关系图谱" : "Social graph" },
    { src: "/screenshots/wechat-storage.webp", alt: t ? "存储空间分析" : "Storage analysis" },
    { src: "/screenshots/wechat-home.webp", alt: t ? "微信数据总览" : "WeChat overview" },
  ];
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    timerRef.current = setInterval(() => setFrame((f) => (f + 1) % shots.length), 3400);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [shots.length]);

  const stats = t
    ? [
        { to: 137842, suffix: "", label: "条消息，被完整找回" },
        { to: 370, suffix: "", label: "个会话 · 全部可回放" },
        { to: 3772, suffix: "", label: "位好友 · 全库检索" },
        { to: 19.5, suffix: " GB", label: "媒体占用 · 分类分析", decimals: 1 },
      ]
    : [
        { to: 137842, suffix: "", label: "messages recovered" },
        { to: 370, suffix: "", label: "sessions, fully replayable" },
        { to: 3772, suffix: "", label: "contacts, full-text searchable" },
        { to: 19.5, suffix: " GB", label: "media, analyzed by type", decimals: 1 },
      ];

  return (
    <section id="wrapped" ref={secRef} className="act relative overflow-hidden py-24 lg:py-32">
      <span className="hud-corner right-6 top-20" aria-hidden="true">YOUR DATA, WRAPPED</span>
      <div className="relative mx-auto max-w-7xl px-5 lg:px-8">
        <header className="max-w-3xl">
          <p className="sec-tag"><i className="tick" /> {t ? "ANNUAL WRAPPED — 数据叙事" : "ANNUAL WRAPPED — data story"}</p>
          <h2 className="mt-6 font-display text-4xl font-extrabold leading-tight text-text sm:text-5xl">
            <span className="hl-line"><span className="hl-line-inner">{t ? "而这一年，" : "And this year,"}</span></span>
            <span className="hl-line"><span className="hl-line-inner">{t ? "值得被数据" : "deserves to be"}</span></span>
            <span className="hl-line"><span className="hl-line-inner"><em className="text-gradient">{t ? "温柔复述" : "gently retold"}</em></span></span>
          </h2>
        </header>

        <div className="mt-12 grid gap-8 lg:grid-cols-[1fr_1.2fr] lg:items-center">
          {/* 截图轮播 */}
          <div className="glass relative overflow-hidden rounded-2xl">
            <div className="flex items-center justify-between border-b border-border px-4 py-2.5 font-mono text-[10px] tracking-[0.2em] text-faint">
              <span>{t ? "DATA FILM" : "DATA FILM"}</span>
              <span>FRAME {String(frame + 1).padStart(2, "0")} / {String(shots.length).padStart(2, "0")}</span>
            </div>
            <div className="relative aspect-[16/9]">
              {shots.map((s, i) => (
                <div
                  key={s.src}
                  className="absolute inset-0 transition-opacity duration-700"
                  style={{ opacity: i === frame ? 1 : 0 }}
                >
                  <Image src={s.src} alt={s.alt} fill sizes="(min-width:1024px) 50vw, 100vw" loading="lazy" />
                </div>
              ))}
            </div>
          </div>

          {/* 统计 */}
          <div className="grid grid-cols-2 gap-4">
            {stats.map((s) => (
              <div key={s.label} className="glass rounded-2xl p-6">
                <p className={`font-mono font-bold text-gradient ${s.decimals ? "text-3xl" : "text-4xl"}`}>
                  <Counter to={s.to} suffix={s.suffix} decimals={s.decimals ?? 0} className="text-4xl" />
                </p>
                <p className="mt-2 text-[13px] text-muted">{s.label}</p>
              </div>
            ))}
          </div>
        </div>

        <p className="mt-8 font-mono text-[11px] tracking-[0.18em] text-faint">
          {t ? "真实数据 · 本地统计 · 可逐条溯源" : "REAL DATA · LOCAL STATS · TRACEABLE"}{" "}
          <span className="text-accent">→</span>
        </p>
      </div>
    </section>
  );
}
