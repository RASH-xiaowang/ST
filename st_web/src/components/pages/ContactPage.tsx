"use client";

/**
 * 联系与支持：商务咨询表单（客户端校验 + 蜜罐防刷 + 限流节流）。
 * 提交成功/失败本地反馈；生产可配置 FORM_ENDPOINT 转发。
 */
import { useState } from "react";
import { Reveal } from "@/components/ui/Reveal";
import { contact } from "@/lib/content/site";
import { pick, type Locale } from "@/lib/i18n/locales";

export type FormState = "idle" | "submitting" | "success" | "error";

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/;

export function ContactPage({ locale }: { locale: Locale }) {
  const [state, setState] = useState<FormState>("idle");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [honeypot, setHoneypot] = useState("");
  const [lastSubmit, setLastSubmit] = useState(0);

  const t = locale === "zh";

  const validate = (fd: FormData): Record<string, string> => {
    const errs: Record<string, string> = {};
    const name = String(fd.get("name") ?? "").trim();
    const email = String(fd.get("email") ?? "").trim();
    const message = String(fd.get("message") ?? "").trim();
    const agree = fd.get("agree");
    if (name.length < 2) errs.name = t ? "请填写姓名（至少 2 个字符）" : "Name is required (min 2 chars)";
    if (!EMAIL_RE.test(email)) errs.email = t ? "请输入有效的邮箱地址" : "Enter a valid email address";
    if (message.length < 10) errs.message = t ? "请描述你的需求（至少 10 个字符）" : "Describe your need (min 10 chars)";
    if (!agree) errs.agree = t ? "请同意隐私政策" : "Please accept the privacy policy";
    return errs;
  };

  const onSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    // 蜜罐：机器人会填写隐藏字段
    if (honeypot) return;
    // 节流：60 秒内只允许一次提交
    const now = Date.now();
    if (now - lastSubmit < 60_000) {
      setErrors({ rate: t ? "提交过于频繁，请稍后再试" : "Too many submissions, please wait" });
      return;
    }
    const fd = new FormData(e.currentTarget);
    const errs = validate(fd);
    setErrors(errs);
    if (Object.keys(errs).length > 0) return;

    setState("submitting");
    setLastSubmit(now);
    try {
      const endpoint = process.env.NEXT_PUBLIC_FORM_ENDPOINT;
      if (endpoint) {
        const res = await fetch(endpoint, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            name: fd.get("name"),
            email: fd.get("email"),
            company: fd.get("company"),
            topic: fd.get("topic"),
            message: fd.get("message"),
          }),
        });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
      } else {
        // 无端点：本地模拟提交（静态站点演示模式）
        await new Promise((r) => setTimeout(r, 900));
      }
      setState("success");
    } catch {
      setState("error");
    }
  };

  return (
    <div className="mx-auto max-w-6xl px-5 pb-24 pt-32 lg:px-8">
      <div className="grid gap-10 lg:grid-cols-[1fr_1.2fr]">
        <Reveal>
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.3em] text-accent">
              {t ? "联系与支持" : "Contact & Support"}
            </p>
            <h1 className="mt-4 font-display text-4xl font-extrabold text-text">{pick(contact.title, locale)}</h1>
            <p className="mt-4 max-w-md text-[15px] leading-relaxed text-muted">{pick(contact.subtitle, locale)}</p>
          </div>
          <div className="mt-10 flex flex-col gap-4">
            {contact.channels.map((c) => (
              <div key={c.name.en} className="glass flex items-center gap-4 rounded-2xl p-5">
                <span className="grid h-11 w-11 place-items-center rounded-xl border border-border text-accent">
                  {c.icon === "mail" ? "✉" : c.icon === "support" ? "☎" : "⌖"}
                </span>
                <div>
                  <p className="font-mono text-[11px] uppercase tracking-wider text-faint">{pick(c.name, locale)}</p>
                  <p className="mt-0.5 text-sm font-semibold text-text">{pick(c.value as { zh: string; en: string }, locale)}</p>
                </div>
              </div>
            ))}
            <div className="glass rounded-2xl p-5">
              <p className="font-mono text-[11px] uppercase tracking-wider text-faint">
                {t ? "反馈方式" : "Feedback"}
              </p>
              <p className="mt-1 text-sm text-text">
                {t
                  ? "应用内 Harness 会话的「好/差评 + 评论」按钮，评论会进入本地反馈记录。"
                  : "Use the like/dislike buttons with comments in HARNESS sessions; they land in the local feedback store."}
              </p>
            </div>
          </div>
        </Reveal>

        <Reveal delay={100}>
          <form onSubmit={onSubmit} noValidate className="glass rounded-2xl p-7 sm:p-9" data-testid="contact-form">
            {state === "success" ? (
              <div className="flex min-h-[380px] flex-col items-center justify-center gap-4 text-center">
                <span className="grid h-14 w-14 place-items-center rounded-full bg-ok/15 text-2xl text-ok">✓</span>
                <h2 className="font-display text-xl font-bold text-text">
                  {t ? "已收到你的消息！" : "Message received!"}
                </h2>
                <p className="max-w-sm text-sm text-muted">
                  {t ? "你的反馈已记录（本地）。感谢你帮助改进产品！" : "Your feedback was recorded locally. Thanks for helping improve the product!"}
                </p>
                <button
                  type="button"
                  onClick={() => setState("idle")}
                  className="mt-2 rounded-lg border border-border px-4 py-2 text-sm text-muted transition hover:text-text"
                >
                  {t ? "再发一条" : "Send another"}
                </button>
              </div>
            ) : (
              <>
                <div className="grid gap-5 sm:grid-cols-2">
                  <label className="flex flex-col gap-1.5">
                    <span className="text-xs font-semibold text-muted">{t ? "姓名 *" : "Name *"}</span>
                    <input
                      name="name"
                      required
                      aria-invalid={!!errors.name}
                      className={`rounded-lg border bg-surface px-3.5 py-2.5 text-sm text-text outline-none transition focus:border-accent ${
                        errors.name ? "border-err" : "border-border"
                      }`}
                      placeholder={t ? "你的姓名" : "Your name"}
                    />
                    {errors.name && <span className="text-xs text-err">{errors.name}</span>}
                  </label>
                  <label className="flex flex-col gap-1.5">
                    <span className="text-xs font-semibold text-muted">{t ? "邮箱 *" : "Email *"}</span>
                    <input
                      name="email"
                      type="email"
                      required
                      aria-invalid={!!errors.email}
                      className={`rounded-lg border bg-surface px-3.5 py-2.5 text-sm text-text outline-none transition focus:border-accent ${
                        errors.email ? "border-err" : "border-border"
                      }`}
                      placeholder="you@company.com"
                    />
                    {errors.email && <span className="text-xs text-err">{errors.email}</span>}
                  </label>
                </div>
                <div className="mt-5 grid gap-5 sm:grid-cols-2">
                  <label className="flex flex-col gap-1.5">
                    <span className="text-xs font-semibold text-muted">{t ? "公司" : "Company"}</span>
                    <input
                      name="company"
                      className="rounded-lg border border-border bg-surface px-3.5 py-2.5 text-sm text-text outline-none transition focus:border-accent"
                      placeholder={t ? "（可选）" : "(optional)"}
                    />
                  </label>
                  <label className="flex flex-col gap-1.5">
                    <span className="text-xs font-semibold text-muted">{t ? "咨询类型" : "Topic"}</span>
                    <select
                      name="topic"
                      className="rounded-lg border border-border bg-surface px-3.5 py-2.5 text-sm text-text outline-none transition focus:border-accent"
                      defaultValue="sales"
                    >
                      <option value="sales">{t ? "商务咨询" : "Sales inquiry"}</option>
                      <option value="deploy">{t ? "部署支持" : "Deployment support"}</option>
                      <option value="feedback">{t ? "产品反馈" : "Product feedback"}</option>
                      <option value="other">{t ? "其他" : "Other"}</option>
                    </select>
                  </label>
                </div>
                {/* 蜜罐（对用户隐藏） */}
                <div className="absolute -left-[9999px]" aria-hidden="true">
                  <label>
                    {t ? "请勿填写" : "Do not fill"}
                    <input name="website" tabIndex={-1} autoComplete="off" value={honeypot} onChange={(e) => setHoneypot(e.target.value)} />
                  </label>
                </div>
                <label className="mt-5 flex flex-col gap-1.5">
                  <span className="text-xs font-semibold text-muted">{t ? "需求描述 *" : "Message *"}</span>
                  <textarea
                    name="message"
                    rows={5}
                    required
                    aria-invalid={!!errors.message}
                    className={`rounded-lg border bg-surface px-3.5 py-2.5 text-sm text-text outline-none transition focus:border-accent ${
                      errors.message ? "border-err" : "border-border"
                    }`}
                    placeholder={t ? "告诉我们你的场景与需求…" : "Tell us about your use case…"}
                  />
                  {errors.message && <span className="text-xs text-err">{errors.message}</span>}
                </label>
                <label className="mt-4 flex items-center gap-2.5 text-sm text-muted">
                  <input type="checkbox" name="agree" className="h-4 w-4 accent-[var(--accent)]" />
                  {t ? "我同意" : "I agree to the"}{" "}
                  <a href={`/${locale}/docs/privacy/`} className="text-accent hover:underline">
                    {t ? "《隐私政策》" : "Privacy Policy"}
                  </a>
                </label>
                {errors.agree && <p className="mt-1 text-xs text-err">{errors.agree}</p>}
                {errors.rate && <p className="mt-2 text-xs text-err">{errors.rate}</p>}
                {state === "error" && (
                  <p className="mt-2 text-xs text-err">{t ? "提交失败，请稍后重试" : "Submission failed, please retry"}</p>
                )}
                <button
                  type="submit"
                  disabled={state === "submitting"}
                  className="mt-6 w-full rounded-xl bg-gradient-to-r from-accent via-accent-2 to-accent-3 px-6 py-3.5 text-sm font-bold text-white shadow-[0_16px_44px_-16px_var(--glow)] transition hover:brightness-110 disabled:opacity-60"
                  data-testid="contact-submit"
                >
                  {state === "submitting"
                    ? t ? "提交中…" : "Sending…"
                    : t ? "发送消息" : "Send message"}
                </button>
              </>
            )}
          </form>
        </Reveal>
      </div>
    </div>
  );
}
