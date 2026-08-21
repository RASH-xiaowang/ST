"use client";

/**
 * ACT 04 · Features — 卡片堆叠轮播（真实截图 + MCP 代码卡）+ 3D 模型
 */
import { useEffect, useMemo, useRef, useState } from "react";
import Image from "next/image";
import { gsap, registerGsap } from "@/lib/gsap";
import { ProductViewer, type Hotspot } from "@/components/three/ProductViewer";
import type { Locale } from "@/lib/i18n/locales";

type DeckItem = {
  name: string;
  desc: string;
  meta: string;
  src?: string;
  alt?: string;
  code?: string;
  lang?: string;
};

export function DeckSection({ locale }: { locale: Locale }) {
  const t = locale === "zh";
  const [idx, setIdx] = useState(0);
  const stackRef = useRef<HTMLDivElement | null>(null);

  const items: DeckItem[] = useMemo(
    () =>
      t
        ? [
            { name: "微信数据 · 本地解密", desc: "朋友圈洞察、撤回消息记录、存储空间分析、通讯录全库检索，全部离线。", meta: "朋友圈 · 撤回 · 存储 · 通讯录", src: "/screenshots/wechat-home.webp", alt: "微信数据总览" },
            { name: "聊天记录 1:1 复刻", desc: "文本、图片、语音、表情、引用、红包……逐一还原，界面尽可能与微信一致。", meta: "高仿界面 · 时间轴 · 全消息类型", src: "/screenshots/wechat-moments.webp", alt: "朋友圈洞察" },
            { name: "社交图谱 · 关系网络", desc: "群友圈子 + 群聊网络双模式，社区检测自动着色，亲密度与共同群一目了然，可导出 SVG / 海报。", meta: "力导向 · 社区着色 · 双模式", src: "/screenshots/wechat-graph.webp", alt: "社交关系图谱" },
            { name: "Harness 智能代理", desc: "真流式对话、工具执行时间线、遥测统计条——大模型变成可靠的数字员工。", meta: "流式 · 工具 · 统计条", src: "/screenshots/harness-session.webp", alt: "Harness 会话" },
            { name: "治理与审计", desc: "预设、钩子、沙箱三模式与审批卡，模型可见即落日志，一切可追溯。", meta: "治理中心 · 审批 · 审计", src: "/screenshots/harness-governance.webp", alt: "治理中心" },
            { name: "知识库 RAG", desc: "多格式文档导入，向量 + BM25 混合检索问答，答案带来源。", meta: "导入 · 混合检索", src: "/screenshots/kb.webp", alt: "知识库" },
            { name: "给 AI 的会话协议", desc: "ACP JSON-RPC，一行命令把聊天档案交给 AI 提问。", meta: "ACP · 127.0.0.1:4770", code: `# ACP · JSON-RPC
$ curl -X POST http://127.0.0.1:4770/acp \\
  -d '{"method":"session/new","params":{"goal":"分析依赖"}}
› tools: session_search · session_trace · …
✓ 你的数据，可以被 AI 提问`, lang: "bash" },
          ]
        : [
            { name: "WeChat Data · Decrypted", desc: "Moments insights, recalled messages, storage analysis, full contact search — all offline.", meta: "moments · recall · storage · contacts", src: "/screenshots/wechat-home.webp", alt: "WeChat overview" },
            { name: "Chats, 1:1 recreated", desc: "Text, images, voice, stickers, red packets — recreated to match WeChat itself.", meta: "faithful UI · timeline", src: "/screenshots/wechat-moments.webp", alt: "Moments insights" },
            { name: "Social graph · your network", desc: "Two modes — circles of friends and group network — with auto-colored communities, intimacy & shared-group rankings, exportable as SVG or poster.", meta: "force-directed · communities · dual mode", src: "/screenshots/wechat-graph.webp", alt: "Social relationship graph" },
            { name: "HARNESS Agent Runtime", desc: "True streaming, an execution timeline, telemetry — LLMs as dependable workers.", meta: "streaming · tools · telemetry", src: "/screenshots/harness-session.webp", alt: "HARNESS session" },
            { name: "Governance & Audit", desc: "Presets, hooks, a three-tier sandbox and approval cards; model-visible is logged.", meta: "governance · approvals · audit", src: "/screenshots/harness-governance.webp", alt: "Governance center" },
            { name: "Knowledge Base RAG", desc: "Multi-format imports with hybrid vector + BM25 Q&A, answers cite sources.", meta: "import · hybrid search", src: "/screenshots/kb.webp", alt: "Knowledge base" },
            { name: "A session protocol for AI", desc: "ACP JSON-RPC — hand your data to an AI in one line.", meta: "ACP · 127.0.0.1:4770", code: `# ACP · JSON-RPC
$ curl -X POST http://127.0.0.1:4770/acp \\
  -d '{"method":"session/new","params":{"goal":"analyze deps"}}
› tools: session_search · session_trace · …
✓ your data, answerable by AI`, lang: "bash" },
          ],
    [t],
  );

  // 轮播：下一张
  useEffect(() => {
    if (items.length === 0) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const timer = setInterval(() => setIdx((i) => (i + 1) % items.length), 3600);
    return () => clearInterval(timer);
  }, [items.length]);

  // GSAP 卡片堆叠入场/切换动画
  useEffect(() => {
    const stack = stackRef.current;
    if (!stack) return;
    registerGsap();
    const cards = stack.querySelectorAll("[data-card]");
    const ctx = gsap.context(() => {
      cards.forEach((card, i) => {
        const c = card as HTMLElement;
        gsap.set(c, { opacity: i === 0 ? 1 : 0, scale: 1 - i * 0.05, y: i * 14, zIndex: 100 - i });
      });
    }, stack);
    return () => ctx.revert();
  }, []);

  useEffect(() => {
    const stack = stackRef.current;
    if (!stack) return;
    const cards = stack.querySelectorAll("[data-card]");
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      cards.forEach((c, i) => {
        const el = c as HTMLElement;
        el.style.opacity = i === idx ? "1" : "0";
        el.style.transform = `scale(${1 - idx * 0.05})`;
      });
      return;
    }
    registerGsap();
    const ctx = gsap.context(() => {
      cards.forEach((c, i) => {
        const el = c as HTMLElement;
        const active = i === idx;
        const targetZ = 100 - i;
        if (active) {
          gsap.to(el, { opacity: 1, scale: 1, y: 0, zIndex: targetZ, duration: 0.6, ease: "power3.out" });
        } else {
          const diff = (i - idx + items.length) % items.length;
          gsap.to(el, { opacity: 0, scale: 1 - diff * 0.05, y: diff * 14, zIndex: 100 - diff, duration: 0.6, ease: "power3.out" });
        }
      });
    }, stack);
    return () => ctx.revert();
  }, [idx, items.length]);

  const cur = items[idx];

  const hotspots: Hotspot[] = t
    ? [
        { id: "core", title: "代理核心", desc: "模型循环中枢：流式补全、工具循环、轮次守卫与重复调用提醒。", position: [0, 0, 0] },
        { id: "ring", title: "执行轨道", desc: "30+ 内置工具与子代理并行执行，结果进入追加式日志。", position: [1.5, 0.9, 0.6] },
        { id: "shell", title: "治理壳层", desc: "沙箱三模式、审批卡与决策钩子，把每次调用都约束在策略之内。", position: [-1.4, -0.6, 1.0] },
      ]
    : [
        { id: "core", title: "Agent Core", desc: "The model-loop hub: streaming, tool loop, round guard and reminders.", position: [0, 0, 0] },
        { id: "ring", title: "Execution Orbit", desc: "30+ built-in tools and parallel subagents; every result is logged.", position: [1.5, 0.9, 0.6] },
        { id: "shell", title: "Governance Shell", desc: "Three-tier sandbox, approval cards and hooks keep calls in policy.", position: [-1.4, -0.6, 1.0] },
      ];
  const viewerLabels = t
    ? { rotate: "产品模型（旋转/缩放/平移）", explode: "爆炸视图", section: "剖面", scheme: "配色", auto: "自动旋转", reset: "重置" }
    : { rotate: "Product model (rotate/zoom/pan)", explode: "Explode", section: "Section", scheme: "Scheme", auto: "Auto-rotate", reset: "Reset" };

  return (
    <section id="features" className="act relative overflow-hidden py-24 lg:py-32">
      <span className="hud-corner left-6 top-20" aria-hidden="true">FEATURES · 功能全景</span>
      <div className="relative mx-auto grid max-w-7xl items-center gap-12 px-5 lg:grid-cols-2 lg:px-8">
        <div>
          <p className="sec-tag"><i className="tick" /> {t ? "FEATURES · 功能全景" : "FEATURES · panorama"}</p>
          <div className="mt-6 flex items-baseline gap-4">
            <span className="font-mono text-5xl font-bold text-gradient">{String(idx + 1).padStart(2, "0")}</span>
            <h2 className="font-display text-3xl font-extrabold text-text sm:text-4xl">{cur.name}</h2>
          </div>
          <p className="mt-4 max-w-xl text-[15px] leading-relaxed text-muted">{cur.desc}</p>
          <p className="deck__meta mt-5">{cur.meta}</p>
          <div className="mt-8 flex items-center gap-3">
            {items.map((_, i) => (
              <button
                key={i}
                onClick={() => setIdx(i)}
                aria-label={t ? `第 ${i + 1} 项` : `Item ${i + 1}`}
                aria-pressed={idx === i}
                className={`h-1.5 rounded-full transition-all ${idx === i ? "w-8 bg-accent" : "w-3 bg-border hover:bg-muted"}`}
              />
            ))}
          </div>
        </div>

        <div ref={stackRef} className="deck__stack" data-testid="deck">
          {items.map((item, i) => (
            <div key={item.name} className="deck__card" data-card data-testid="deck-card">
              {item.src ? (
                <Image src={item.src} alt={item.alt ?? item.name} fill sizes="(min-width:1024px) 50vw, 100vw" loading={i === 0 ? "eager" : "lazy"} />
              ) : (
                <pre className="code-shell h-full overflow-auto p-6 font-mono text-[12.5px] leading-relaxed text-[#dbe7ff]">
                  <code>{item.code}</code>
                </pre>
              )}
              <span className="absolute left-4 top-4 rounded bg-black/55 px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-white/85">
                {item.meta.split(" · ")[0]}
              </span>
            </div>
          ))}
        </div>
      </div>

      {/* 3D 模型 */}
      <div className="mx-auto mt-20 max-w-4xl px-5 lg:px-8">
        <div className="glass rounded-2xl p-6 sm:p-8">
          <p className="sec-tag mb-5"><i className="tick" /> {t ? "THE CORE — 代理核心 3D" : "THE CORE — agent core 3D"}</p>
          <ProductViewer hotspots={hotspots} labels={viewerLabels} />
        </div>
      </div>
    </section>
  );
}
