"use client";

/** 年度总结预览：8 帧轮播卡片（示意数据），自动播放 + 手动切换 + 微可视化 */

import { useEffect, useRef, useState } from "react";
import { wechatInsights } from "@/lib/content/wechat";
import { pick, type Bi, type Locale } from "@/lib/i18n/locales";

type Frame = (typeof wechatInsights.annual.frames)[number];

const WEEKDAYS_ZH = ["一", "二", "三", "四", "五", "六", "日"];
const WEEKDAYS_EN = ["M", "T", "W", "T", "F", "S", "S"];

function barColor(v: number, max: number): string {
  const t = max > 0 ? v / max : 0;
  return `linear-gradient(90deg, var(--accent), var(--accent-3) ${t * 100}%)`;
}

function FrameVisual({ frame, locale }: { frame: Frame; locale: Locale }) {
  const v = frame.visual;

  if (v.kind === "stat" || v.kind === "chars") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2">
        <span className="font-mono text-4xl font-bold text-gradient sm:text-5xl">
          {locale === "zh" ? v.primary : v.primaryEn}
        </span>
        <span className="text-sm text-muted">{locale === "zh" ? v.secondary : v.secondaryEn}</span>
      </div>
    );
  }

  if (v.kind === "clock") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3">
        <div className="flex items-center gap-3">
          <span className="font-mono text-5xl font-bold text-text">{v.latest}</span>
          <span className="text-sm text-muted">
            {locale === "zh" ? "最晚的一次夜聊" : "latest late-night chat"}
          </span>
        </div>
        <div className="flex items-center gap-3 text-xs text-muted">
          <span className="font-mono">{v.earliest}</span>
          <span>{locale === "zh" ? "第一句消息" : "first message"}</span>
        </div>
      </div>
    );
  }

  if (v.kind === "heatmap") {
    const matrix = buildHeatmap();
    const max = 9;
    const labels = locale === "zh" ? WEEKDAYS_ZH : WEEKDAYS_EN;
    return (
      <div className="flex h-full flex-col justify-center gap-1.5">
        {matrix.map((row, w) => (
          <div key={w} className="flex items-center gap-1.5">
            <span className="w-3 text-[10px] text-faint">{labels[w]}</span>
            {row.map((val, h) => (
              <span
                key={h}
                title={`${labels[w]} ${String(h).padStart(2, "0")}:00 · ${val}`}
                className="h-2.5 flex-1 rounded-[3px]"
                style={{ background: `rgba(7,193,96,${0.06 + (val / max) * 0.82})` }}
              />
            ))}
          </div>
        ))}
        <div className="mt-1 flex items-center justify-between text-[10px] text-faint">
          <span>00:00</span>
          <span>{locale === "zh" ? "热力峰值" : "peak hour"}</span>
          <span>23:00</span>
        </div>
      </div>
    );
  }

  if (v.kind === "bars") {
    const max = Math.max(...v.items.map((i) => i.value));
    return (
      <div className="flex h-full flex-col justify-center gap-2.5">
        {v.items.map((item, i) => (
          <div key={i} className="flex items-center gap-3">
            <span className="w-16 truncate text-xs text-muted">{pick(item.label as Bi<string>, locale)}</span>
            <div className="h-3 flex-1 overflow-hidden rounded-full bg-surface-3">
              <div
                className="h-full rounded-full"
                style={{ width: `${Math.max(8, (item.value / max) * 100)}%`, background: barColor(item.value, max) }}
              />
            </div>
            <span className="w-12 text-right font-mono text-xs text-accent">{item.value.toLocaleString()}</span>
          </div>
        ))}
      </div>
    );
  }

  if (v.kind === "phrases") {
    return (
      <div className="flex h-full flex-wrap content-center items-center justify-center gap-2.5">
        {v.items.map((p, i) => (
          <span
            key={i}
            className={`rounded-full border px-3.5 py-1.5 text-[13px] ${
              i % 3 === 0
                ? "border-accent/50 bg-accent/10 text-accent"
                : i % 3 === 1
                  ? "border-border bg-surface-3 text-text"
                  : "border-border bg-white/[0.03] text-muted"
            }`}
          >
            {pick(p as Bi<string>, locale)}
          </span>
        ))}
      </div>
    );
  }

  // months
  const max = Math.max(...v.values);
  return (
    <div className="flex h-full items-end justify-center gap-2 pb-1">
      {v.values.map((val, i) => (
        <div key={i} className="flex h-full flex-1 flex-col items-center justify-end gap-1.5">
          <span className="text-[10px] text-faint">{val}</span>
          <div
            className="w-full rounded-t-[3px]"
            style={{ height: `${Math.max(6, (val / max) * 78)}%`, background: barColor(val, max) }}
          />
          <span className="text-[10px] text-faint">{i + 1}</span>
        </div>
      ))}
    </div>
  );
}

/** 示意热力矩阵：工作日白天/夜间与周末高峰的确定性形态 */
function buildHeatmap(): number[][] {
  const m: number[][] = [];
  for (let w = 0; w < 7; w++) {
    const row: number[] = [];
    for (let h = 0; h < 24; h++) {
      const weekend = w >= 5;
      const day = h >= 9 && h <= 18;
      const night = h >= 20 || h <= 1;
      let val = 1;
      if (weekend && (day || night)) val = 7;
      else if (night) val = 5;
      else if (day) val = 3;
      val += ((w * 13 + h * 7) % 5) - 2;
      row.push(Math.max(0, Math.min(9, val)));
    }
    m.push(row);
  }
  return m;
}

export function AnnualWrapped({ locale }: { locale: Locale }) {
  const frames = wechatInsights.annual.frames;
  const [index, setIndex] = useState(0);
  const [paused, setPaused] = useState(false);
  const frameRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (paused) return;
    const id = setInterval(() => setIndex((i) => (i + 1) % frames.length), 4000);
    return () => clearInterval(id);
  }, [paused, frames.length]);

  const frame = frames[index];

  return (
    <div
      ref={frameRef}
      className="mx-auto max-w-3xl"
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocusCapture={() => setPaused(true)}
      onBlurCapture={() => setPaused(false)}
    >
      <div className="mb-4 flex items-center justify-between">
        <span className="font-mono text-xs uppercase tracking-[0.3em] text-faint">
          {locale === "zh" ? "年度总结 · 示意预览" : "Annual Wrapped · illustrative preview"}
        </span>
        <span className="font-mono text-xs text-accent">
          FRAME {String(index + 1).padStart(2, "0")} / {String(frames.length).padStart(2, "0")}
        </span>
      </div>

      <div
        role="region"
        aria-label={locale === "zh" ? "年度总结卡片轮播" : "Annual wrapped card carousel"}
        className="glass relative overflow-hidden rounded-3xl p-6 sm:p-8"
      >
        <div
          className="pointer-events-none absolute -right-16 -top-16 h-48 w-48 rounded-full blur-3xl"
          style={{ background: "radial-gradient(circle, var(--glow), transparent 70%)" }}
          aria-hidden="true"
        />
        <div className="relative min-h-[280px] sm:min-h-[260px]">
          <div key={frame.key} className="flex h-full flex-col">
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-3">
                <span className="grid h-10 w-10 place-items-center rounded-xl border border-border bg-white/[0.03] text-lg">
                  {frame.icon}
                </span>
                <div>
                  <h4 className="font-display text-lg font-bold text-text">{pick(frame.name, locale)}</h4>
                  <code className="font-mono text-[11px] text-accent">{frame.field}</code>
                </div>
              </div>
              <span className="rounded-full border border-border bg-white/[0.03] px-2.5 py-1 text-[10px] uppercase tracking-wider text-faint">
                {locale === "zh" ? "示意" : "Sample"}
              </span>
            </div>

            <div className="mt-6 flex-1">
              <FrameVisual frame={frame} locale={locale} />
            </div>

            <p className="mt-6 text-[13px] leading-relaxed text-muted">{pick(frame.hint, locale)}</p>
          </div>
        </div>
      </div>

      <div className="mt-5 flex items-center justify-center gap-4">
        <button
          onClick={() => setIndex((index - 1 + frames.length) % frames.length)}
          aria-label={locale === "zh" ? "上一张" : "Previous frame"}
          className="grid h-9 w-9 place-items-center rounded-full border border-border text-muted transition hover:border-accent/50 hover:text-accent"
        >
          ←
        </button>
        <div className="flex items-center gap-2" role="tablist" aria-label={locale === "zh" ? "年度卡片" : "Frames"}>
          {frames.map((f, i) => (
            <button
              key={f.key}
              role="tab"
              aria-selected={i === index}
              aria-label={pick(f.name, locale)}
              onClick={() => setIndex(i)}
              className={`h-2 rounded-full transition-all duration-300 ${
                i === index ? "w-6 bg-accent" : "w-2 bg-surface-3 hover:bg-border"
              }`}
            />
          ))}
        </div>
        <button
          onClick={() => setIndex((index + 1) % frames.length)}
          aria-label={locale === "zh" ? "下一张" : "Next frame"}
          className="grid h-9 w-9 place-items-center rounded-full border border-border text-muted transition hover:border-accent/50 hover:text-accent"
        >
          →
        </button>
      </div>

      <p className="mt-4 text-center text-xs text-faint">{pick(wechatInsights.annual.disclaimer, locale)}</p>
    </div>
  );
}
