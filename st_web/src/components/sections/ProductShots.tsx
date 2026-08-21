"use client";

/**
 * 界面实拍：ST Control 真实运行截图（从应用内捕获），按功能分组，点击放大。
 */
import { useEffect, useState } from "react";
import Image from "next/image";
import { Reveal } from "@/components/ui/Reveal";
import { SectionHeading } from "@/components/ui/SectionHeading";
import type { Locale } from "@/lib/i18n/locales";

type Shot = {
  src: string;
  alt: string;
  caption: string;
  span?: boolean;
};

type Group = { key: string; title: string; desc: string; shots: Shot[] };

export function ProductShots({ locale }: { locale: Locale }) {
  const [zoom, setZoom] = useState<Shot | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setZoom(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const groups: Group[] = locale === "zh"
    ? [
        {
          key: "wechat",
          title: "微信数据 · 本地解密，完整掌控",
          desc: "朋友圈洞察、撤回消息记录、存储空间分析、通讯录全库搜索——你的微信档案，全部离线保存在这台电脑上。",
          shots: [
            { src: "/screenshots/wechat-home.webp", alt: "微信数据总览：会话/群聊/好友/朋友圈统计", caption: "微信数据总览：会话 · 群聊 · 好友 · 朋友圈 · 存储构成", span: true },
            { src: "/screenshots/wechat-moments.webp", alt: "朋友圈洞察：活跃作者排行与月度热力", caption: "朋友圈洞察：活跃作者排行 · 月度热力" },
            { src: "/screenshots/wechat-recall.webp", alt: "撤回消息记录：谁撤回了什么，可查可导出", caption: "撤回消息记录：类型构成 · 撤回最多发送者 · 明细" },
            { src: "/screenshots/wechat-storage.webp", alt: "存储空间分析：分类分布与会话占用排行", caption: "存储空间分析：媒体分类 · 会话/发送者占用排行" },
          ],
        },
        {
          key: "harness",
          title: "智能代理运行时 · Harness",
          desc: "流式对话、工具执行时间线、治理中心与多模型管理——把大模型变成可靠的数字员工。",
          shots: [
            { src: "/screenshots/harness-session.webp", alt: "Harness 会话界面：流式回复与工具执行时间线", caption: "Harness 会话：真流式回复 · 工具执行时间线 · 遥测统计条", span: true },
            { src: "/screenshots/harness-governance.webp", alt: "治理中心：预设/钩子/沙箱/审批", caption: "治理中心：预设 · 钩子 · 沙箱三模式 · 审批流" },
            { src: "/screenshots/harness-tools.webp", alt: "工具目录：搜索与参数 Schema", caption: "工具目录：搜索 · 分组 · 参数 Schema 可展开" },
            { src: "/screenshots/llm-config.webp", alt: "大模型管理：接入配置与用量统计", caption: "大模型管理：多提供方接入 · 流量与成本" },
          ],
        },
        {
          key: "more",
          title: "知识库与数据看板",
          desc: "个人 RAG 问答中枢与全局运行数据仪表。",
          shots: [
            { src: "/screenshots/kb.webp", alt: "知识库：文档导入与混合检索问答", caption: "知识库：多格式导入 · 向量 + BM25 混合检索" },
            { src: "/screenshots/dashboard.webp", alt: "数据看板：运行指标仪表", caption: "数据看板：实时运行指标总览" },
          ],
        },
      ]
    : [
        {
          key: "wechat",
          title: "WeChat Data · Decrypted locally, fully yours",
          desc: "Moments insights, recalled-message records, storage analysis and full contact search — your WeChat archive, kept offline on this machine.",
          shots: [
            { src: "/screenshots/wechat-home.webp", alt: "WeChat overview: chats, groups, friends and moments stats", caption: "WeChat overview: chats · groups · friends · moments · storage", span: true },
            { src: "/screenshots/wechat-moments.webp", alt: "Moments insights: top authors and monthly heat", caption: "Moments insights: top authors · monthly heat" },
            { src: "/screenshots/wechat-recall.webp", alt: "Recalled messages: who recalled what, searchable and exportable", caption: "Recalled messages: type mix · top recallers · details" },
            { src: "/screenshots/wechat-storage.webp", alt: "Storage analysis: media breakdown and top sessions", caption: "Storage analysis: media categories · session/sender ranking" },
          ],
        },
        {
          key: "harness",
          title: "Agent Runtime · HARNESS",
          desc: "Streaming chat, an execution timeline, a governance center and multi-model management — turning LLMs into dependable digital workers.",
          shots: [
            { src: "/screenshots/harness-session.webp", alt: "HARNESS session: streaming replies and the execution timeline", caption: "HARNESS session: true streaming · timeline · telemetry", span: true },
            { src: "/screenshots/harness-governance.webp", alt: "Governance center: presets, hooks, sandbox", caption: "Governance center: presets · hooks · sandbox · approvals" },
            { src: "/screenshots/harness-tools.webp", alt: "Tool catalog with search and schemas", caption: "Tool catalog: search · grouping · expandable schemas" },
            { src: "/screenshots/llm-config.webp", alt: "LLM management: providers and usage", caption: "LLM management: multi-provider · traffic & cost" },
          ],
        },
        {
          key: "more",
          title: "Knowledge Base & Dashboard",
          desc: "A personal RAG hub and a live console dashboard.",
          shots: [
            { src: "/screenshots/kb.webp", alt: "Knowledge base: imports and hybrid retrieval Q&A", caption: "Knowledge base: multi-format import · hybrid retrieval" },
            { src: "/screenshots/dashboard.webp", alt: "Dashboard: live metrics", caption: "Dashboard: live runtime metrics" },
          ],
        },
      ];

  return (
    <section id="features" className="relative scroll-mt-20 py-20 lg:py-28">
      <div className="mx-auto max-w-7xl px-5 lg:px-8">
        <Reveal>
          <SectionHeading
            index="●"
            eyebrow={locale === "zh" ? "界面实拍" : "Product in action"}
            title={locale === "zh" ? "不是效果图，是真实运行画面" : "Not mockups — the real product"}
            subtitle={
              locale === "zh"
                ? "以下截图来自 ST Control 桌面应用的真实界面：微信数据、Harness 智能代理与知识库，所见即所得。"
                : "Captured from the ST Control desktop app — WeChat data, the HARNESS agent runtime and the knowledge base, what you see is what you get."
            }
          />
        </Reveal>

        {groups.map((g) => (
          <div key={g.key} className="mt-14 first:mt-12">
            <Reveal>
              <div className="flex items-center gap-3">
                <span
                  className={`h-2 w-2 rounded-full ${
                    g.key === "wechat" ? "bg-gold" : g.key === "harness" ? "bg-accent" : "bg-accent-2"
                  }`}
                />
                <h3 className="font-display text-xl font-bold text-text">{g.title}</h3>
              </div>
              <p className="mt-2 max-w-2xl text-sm text-muted">{g.desc}</p>
            </Reveal>
            <div className="mt-6 grid gap-5 md:grid-cols-2">
              {g.shots.map((s, i) => (
                <Reveal key={s.src + i} delay={i * 60} className={s.span ? "md:col-span-2" : ""}>
                  <button
                    onClick={() => setZoom(s)}
                    className="glass card-hover group relative block w-full overflow-hidden rounded-2xl text-left"
                    aria-label={s.alt}
                    data-testid="product-shot"
                  >
                    <Image
                      src={s.src}
                      alt={s.alt}
                      loading="lazy"
                      width={1600}
                      height={800}
                      className="aspect-[2/1] w-full object-cover transition-transform duration-500 group-hover:scale-[1.02]"
                    />
                    <span className="absolute inset-x-0 bottom-0 flex items-center gap-2 bg-gradient-to-t from-black/70 to-transparent px-5 pb-4 pt-10 text-sm font-semibold text-white">
                      <span className="grid h-6 w-6 place-items-center rounded-full bg-white/15 text-[11px]">🔍</span>
                      {s.caption}
                    </span>
                  </button>
                </Reveal>
              ))}
            </div>
          </div>
        ))}
      </div>

      {/* 放大查看 */}
      {zoom && (
        <div
          className="fixed inset-0 z-[95] flex items-center justify-center bg-black/80 p-6 backdrop-blur-sm"
          onClick={() => setZoom(null)}
          role="dialog"
          aria-modal="true"
          aria-label={zoom.alt}
        >
          <figure className="relative max-h-full max-w-6xl" onClick={(e) => e.stopPropagation()}>
            <Image
              src={zoom.src}
              alt={zoom.alt}
              width={1600}
              height={800}
              className="h-auto max-h-[82vh] w-auto rounded-xl shadow-2xl"
            />
            <figcaption className="mt-3 flex items-center justify-between gap-4">
              <span className="text-sm text-white/85">{zoom.caption}</span>
              <button
                onClick={() => setZoom(null)}
                className="rounded-lg border border-white/25 px-3 py-1.5 text-xs text-white/80 transition hover:bg-white/10"
              >
                ESC {locale === "zh" ? "关闭" : "Close"}
              </button>
            </figcaption>
          </figure>
        </div>
      )}
    </section>
  );
}
