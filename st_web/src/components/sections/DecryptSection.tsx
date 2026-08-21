"use client";

/**
 * ACT 03 · Decrypt — 微信数据本地解密
 * 高仿微信对话演示 + 解密流程 + db 清单 + 真实截图
 */
import { useEffect, useRef } from "react";
import Image from "next/image";
import { ScrambleText } from "@/components/ui/ScrambleText";
import { useHlLines } from "@/lib/use-hl-lines";
import type { Locale } from "@/lib/i18n/locales";

const DBS = ["message_0.db", "contact.db", "session.db", "sns.db", "favorite.db", "media_0.db"];

export function DecryptSection({ locale }: { locale: Locale }) {
  const chatRef = useRef<HTMLDivElement | null>(null);
  const secRef = useRef<HTMLElement | null>(null);
  useHlLines(secRef);

  useEffect(() => {
    const el = chatRef.current;
    if (!el) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries[0]?.isIntersecting) return;
        io.disconnect();
        const rows = el.querySelectorAll("[data-bub]");
        rows.forEach((row, i) => {
          setTimeout(() => row.classList.add("in"), 300 + i * 260);
        });
      },
      { threshold: 0.25 },
    );
    io.observe(el);
    return () => io.disconnect();
  }, []);

  const t = locale === "zh";

  return (
    <section id="decrypt" ref={secRef} className="act relative overflow-hidden py-24 lg:py-32">
      <span className="hud-corner left-6 top-20" aria-hidden="true">DB — SQLCIPHER · READONLY</span>
      <span className="hud-corner right-6 top-20" aria-hidden="true">KEY — 64 HEX · MEMORY SCAN</span>
      <span className="hud-corner bottom-10 right-6" aria-hidden="true">RUNTIME — 100% LOCAL · 0 UPLOAD</span>

      <div className="relative mx-auto grid max-w-7xl items-center gap-14 px-5 lg:grid-cols-2 lg:px-8">
        {/* 文案侧 */}
        <div>
          <p className="sec-tag"><i className="tick" /> {t ? "WECHAT DATA — 本地解密" : "WECHAT DATA — local decryption"}</p>
          <h2 className="mt-6 font-display text-4xl font-extrabold leading-tight text-text sm:text-5xl">
            <span className="hl-line"><span className="hl-line-inner">{t ? "你的微信数据，" : "Your WeChat data,"}</span></span>
            <span className="hl-line"><span className="hl-line-inner">{t ? "由你" : ""}<em className="text-gradient">{t ? "自己留存" : "yours to keep"}</em></span></span>
          </h2>
          <p className="mt-6 max-w-xl text-[15px] leading-relaxed text-muted">
            {t
              ? "本地扫描定位数据库密钥，解密后以只读投影呈现：朋友圈洞察、撤回消息记录、存储空间分析、通讯录全库检索——无需联网，不留副本，卸载微信数据依然在。"
              : "Locate the database key with an in-memory scan, decrypt and present it as a read-only projection: moments insights, recalled-message records, storage analysis and full contact search — no network, no copies, and the data survives even if WeChat is uninstalled."}
          </p>
          <ul className="mt-8 flex flex-wrap gap-2">
            {DBS.map((db) => (
              <li key={db} className="rounded border border-border bg-surface px-2.5 py-1 font-mono text-[11px] text-muted">
                <span className="text-ok">●</span> {db}
              </li>
            ))}
          </ul>
          <div className="mt-8">
            <ScrambleText
              text={t ? "解密 · 浏览 · 搜索 · 导出 —— 全部离线完成" : "DECRYPT · BROWSE · SEARCH · EXPORT — ALL OFFLINE"}
              className="font-mono text-[13px] tracking-[0.12em] text-accent"
            />
          </div>
        </div>

        {/* 演示侧：高仿微信对话 + 真实截图 */}
        <div className="relative">
          <div className="glass mx-auto max-w-[460px] rounded-2xl p-5">
            <div ref={chatRef} className="wxchat mx-auto overflow-hidden rounded-xl border border-border bg-surface">
              <div className="wxchat__head">
                <span className="h-2 w-2 rounded-full bg-ok" />
                <span>{t ? "会话 · 微信数据已解密" : "Session · decrypted"}</span>
                <span className="ml-auto font-mono text-[9px] text-faint">message_0.db ✓</span>
              </div>
              <div className="wxchat__body">
                <p className="wx-time" data-bub>{t ? "昨天 21:47" : "Yesterday 21:47"}</p>
                <div className="wx-row" data-bub>
                  <span className="wx-avatar" style={{ background: "#5d6a75" }}>{t ? "友" : "F"}</span>
                  <div className="wx-bubble">{t ? "到了跟我说一声" : "Text me when you land"}</div>
                </div>
                <div className="wx-row wx-row--r" data-bub>
                  <span className="wx-avatar" style={{ background: "#07b75b" }}>{t ? "我" : "Me"}</span>
                  <div className="wx-bubble">{t ? "刚落地，还是老地方见" : "Landed, see you at the usual place"}</div>
                </div>
                <div className="wx-row" data-bub>
                  <span className="wx-avatar" style={{ background: "#5d6a75" }}>{t ? "友" : "F"}</span>
                  <div className="wx-bubble wx-voice">
                    <svg viewBox="0 0 32 32" fill="currentColor"><path d="M10.3 11.7l-1.8 1.8c.7.7 1.1 1.6 1.1 2.5s-.4 1.9-1.1 2.5l1.8 1.8c1.1-1.1 1.8-2.6 1.8-4.3s-.7-3.2-1.8-4.3zM15.2 6.7l-1.8 1.8c1.9 1.9 3 4.5 3 7.3s-1.2 5.4-3 7.3l1.8 1.8c2.3-2.3 3.8-5.5 3.8-9.1s-1.5-6.8-3.8-9.1z" /></svg>
                    <span>4″</span><i className="wx-unread" />
                  </div>
                </div>
                <div className="wx-row wx-row--r" data-bub>
                  <span className="wx-avatar" style={{ background: "#07b75b" }}>{t ? "我" : "Me"}</span>
                  <div className="wx-img">
                    <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" strokeWidth="1.4"><rect x="3" y="4" width="18" height="16" rx="2" /><circle cx="9" cy="10" r="1.6" /><path d="M3 17l5-5 4 4 3.5-3.5L21 18" /></svg>
                  </div>
                </div>
                <div className="wx-row" data-bub>
                  <span className="wx-avatar" style={{ background: "#5d6a75" }}>{t ? "友" : "F"}</span>
                  <div className="wx-rp">
                    <b>🧧 {t ? "恭喜发财，大吉大利" : "Lucky money!"}</b>
                    <i>{t ? "领取红包" : "Open"}</i>
                  </div>
                </div>
              </div>
            </div>
            <p className="mt-4 text-center font-mono text-[10px] tracking-[0.2em] text-faint">
              {t ? "演示 · 全部来自本地解密后的真实数据" : "Demo · rendered from locally decrypted data"}
            </p>
          </div>
          {/* 真实截图小卡 */}
          <div className="mt-5 grid grid-cols-2 gap-3 sm:grid-cols-4">
            {["wechat-moments", "wechat-recall", "wechat-graph", "wechat-storage"].map((s) => (
              <div key={s} className="glass overflow-hidden rounded-xl" data-testid="decrypt-shot">
                <Image src={`/screenshots/${s}.webp`} alt={s} width={640} height={320} loading="lazy" className="aspect-[2/1] w-full object-cover" />
              </div>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
