"use client";

/** ACT 06 · Machine — 隐私 × 引擎室：0 字节出网 + 技术栈 + 守卫卡 + 规格表 */

import { useRef } from "react";
import { ScrambleText } from "@/components/ui/ScrambleText";
import { Reveal } from "@/components/ui/Reveal";
import { useHlLines } from "@/lib/use-hl-lines";
import { architecture } from "@/lib/content/site";
import { pick, type Locale } from "@/lib/i18n/locales";

export function MachineSection({ locale }: { locale: Locale }) {
  const t = locale === "zh";
  const secRef = useRef<HTMLElement | null>(null);
  useHlLines(secRef);

  const guards = t
    ? [
        { b: "本地运行", s: "密钥与数据全程不出本机，不存在“云端”" },
        { b: "零数据出境", s: "模型请求仅发往你配置的端点；默认 0 遥测" },
        { b: "沙箱三模式", s: "只读 / 工作区写 / 全权，逐调用升级需审批" },
        { b: "审计日志", s: "模型可见即落日志，渲染与回放同源" },
      ]
    : [
        { b: "Runs locally", s: "Keys and data never leave this machine — there is no “cloud”" },
        { b: "Zero egress", s: "Model requests go only to endpoints you configure; zero telemetry by default" },
        { b: "Three-tier sandbox", s: "Read-only / workspace-write / full, with approved per-call escalation" },
        { b: "Audit log", s: "Model-visible facts are logged; render and replay share one source" },
      ];

  return (
    <section id="machine" ref={secRef} className="act relative overflow-hidden py-24 lg:py-32">
      <span className="hud-corner left-6 top-20" aria-hidden="true">THE MACHINE — 引擎 × 隐私</span>
      <div className="relative mx-auto max-w-7xl px-5 lg:px-8">
        <header className="max-w-3xl">
          <p className="sec-tag"><i className="tick" /> {t ? "THE MACHINE — 本机 · 引擎 × 隐私" : "THE MACHINE — engine × privacy"}</p>
          <h2 className="mt-6 font-display text-4xl font-extrabold leading-tight text-text sm:text-5xl">
            <span className="hl-line"><span className="hl-line-inner">{t ? "解密、浏览、导出" : "Decrypt, browse, export"}</span></span>
            <span className="hl-line"><span className="hl-line-inner">{t ? "全程" : "all"}&nbsp;<em className="text-gradient">{t ? "不出这台电脑" : "on this machine"}</em></span></span>
          </h2>
        </header>

        <div className="mt-12 grid gap-10 lg:grid-cols-[1fr_1.1fr] lg:items-start">
          {/* 0 出网大数字 + 仪表 */}
          <div className="glass rounded-2xl p-8 text-center">
            <p className="font-mono text-[11px] tracking-[0.24em] text-faint">
              {t ? "字节出网 · BYTES UPLOADED" : "BYTES UPLOADED"}
            </p>
            <p className="big-zero mt-4 text-[120px]">0</p>
            <p className="mt-2 font-mono text-xs tracking-[0.2em] text-ok">
              ✓ {t ? "AUDIT PASSED · 0 EGRESS" : "AUDIT PASSED · 0 EGRESS"}
            </p>
            <div className="mt-8 flex flex-col gap-3">
              {[
                { k: "PROCESSED ON THIS MACHINE", v: "100%", cls: "text-accent" },
                { k: "UPLOADED", v: "0 B", cls: "text-ok" },
              ].map((m) => (
                <div key={m.k} className="flex items-center justify-between rounded-xl border border-border bg-surface px-5 py-3.5">
                  <span className="font-mono text-[10px] tracking-[0.16em] text-faint">{m.k}</span>
                  <b className={`font-mono text-lg ${m.cls}`}>{m.v}</b>
                </div>
              ))}
            </div>
          </div>

          {/* 守卫 + 技术栈 + 规格 */}
          <div>
            <div className="grid gap-3 sm:grid-cols-2">
              {guards.map((g) => (
                <div key={g.b} className="guard">
                  <span className="mt-1 h-2 w-2 shrink-0 rounded-full bg-ok" />
                  <div>
                    <b>{g.b}</b>
                    <p className="mt-1 text-xs leading-relaxed text-muted">{g.s}</p>
                  </div>
                </div>
              ))}
            </div>

            <div className="glass mt-6 rounded-2xl p-6">
              <p className="term-line"><span className="k">ENGINE</span> <span className="d">—</span> <span className="v">Rust · Tauri 2 · SQLite (WAL) · Svelte 5 · Three.js</span></p>
              <p className="term-line mt-1"><span className="k">TOOLS</span> <span className="d">—</span> <span className="v">{t ? "30+ 内置工具 · MCP · LSP · 钩子桥" : "30+ built-in tools · MCP · LSP · hooks"}</span></p>
              <p className="term-line mt-1"><span className="k">PLATFORM</span> <span className="d">—</span> <span className="v">{t ? "Windows 桌面原生" : "Windows desktop native"}</span></p>
            </div>

            <div className="mt-6">
              <ScrambleText
                text={t ? "仅限处理你自己的数据 · 请遵守当地法律法规" : "FOR YOUR OWN DATA ONLY · FOLLOW LOCAL LAWS"}
                className="font-mono text-[11px] tracking-[0.18em] text-faint"
              />
            </div>
          </div>
        </div>

        {/* 技术规格表 */}
        <Reveal delay={60}>
          <div className="glass mt-14 overflow-hidden rounded-2xl">
            <h3 className="border-b border-border px-6 py-4 font-display text-lg font-bold text-text sm:px-8">
              {pick(architecture.specs.title, locale)}
            </h3>
            <div className="grid divide-y divide-border sm:grid-cols-2 sm:divide-x">
              {architecture.specs.rows.map((r) => (
                <div key={r.k.en} className="grid gap-1 px-6 py-3.5 sm:grid-cols-[150px_1fr] sm:gap-4">
                  <dt className="text-sm font-semibold text-text">{r.k[locale]}</dt>
                  <dd className="text-sm text-muted">{r.v[locale]}</dd>
                </div>
              ))}
            </div>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
