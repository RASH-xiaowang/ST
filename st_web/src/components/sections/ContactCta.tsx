/** 首页联系 CTA 横幅 */

import { Reveal } from "@/components/ui/Reveal";
import { contact, brand } from "@/lib/content/site";
import { pick, type Locale } from "@/lib/i18n/locales";

export function ContactCta({ locale }: { locale: Locale }) {
  return (
    <section className="relative py-20 lg:py-28">
      <div className="mx-auto max-w-5xl px-5 lg:px-8">
        <Reveal>
          <div className="glow-ring glass relative overflow-hidden rounded-3xl px-8 py-14 text-center sm:px-14">
            <div
              className="absolute left-1/2 top-0 h-64 w-[560px] -translate-x-1/2 rounded-full opacity-50 blur-3xl"
              style={{ background: "radial-gradient(circle, var(--glow), transparent 70%)" }}
              aria-hidden="true"
            />
            <div className="relative">
              <h2 className="font-display text-3xl font-extrabold text-text sm:text-4xl">
                <span className="text-gradient">{pick(brand.slogan, locale)}</span>
              </h2>
              <p className="mx-auto mt-4 max-w-xl text-[15px] leading-relaxed text-muted">
                {pick(contact.subtitle, locale)}
              </p>
              <div className="mt-8 flex flex-wrap items-center justify-center gap-4">
                <a
                  href={`/${locale}/docs/`}
                  className="rounded-xl bg-gradient-to-r from-accent via-accent-2 to-accent-3 px-7 py-3.5 text-[15px] font-semibold text-white shadow-[0_18px_50px_-16px_var(--glow)] transition hover:brightness-110"
                >
                  {pick(brand.ctaPrimary, locale)}
                </a>
                <a
                  href={`/${locale}/contact/`}
                  className="glass rounded-xl px-7 py-3.5 text-[15px] font-semibold text-text transition hover:border-border-2"
                >
                  {locale === "zh" ? "联系我们" : "Contact us"}
                </a>
              </div>
            </div>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
