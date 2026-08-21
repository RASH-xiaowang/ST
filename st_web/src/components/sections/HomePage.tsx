/** 首页组装：ACT 单页叙事结构（区块各自为客户端组件） */

import type { Locale } from "@/lib/i18n/locales";
import { Hero } from "./Hero";
import { Manifesto } from "./Manifesto";
import { DecryptSection } from "./DecryptSection";
import { WechatInsights } from "./WechatInsights";
import { DeckSection } from "./DeckSection";
import { ModulesSection } from "./ModulesSection";
import { WrappedSection } from "./WrappedSection";
import { MachineSection } from "./MachineSection";
import { Customers } from "./Customers";
import { FaqSection } from "./FaqSection";
import { UpdatesSection } from "./UpdatesSection";
import { ContactCta } from "./ContactCta";
import { ActRail } from "@/components/ui/ActRail";

const ACT_IDS = ["manifesto", "decrypt", "insights", "features", "wrapped", "machine"];

export function HomePage({ locale }: { locale: Locale }) {
  return (
    <>
      <Hero locale={locale} />
      <Manifesto locale={locale} />
      <DecryptSection locale={locale} />
      <WechatInsights locale={locale} />
      <DeckSection locale={locale} />
      <ModulesSection locale={locale} />
      <WrappedSection locale={locale} />
      <MachineSection locale={locale} />
      <Customers locale={locale} />
      <FaqSection locale={locale} />
      <UpdatesSection locale={locale} />
      <ContactCta locale={locale} />
      <ActRail ids={ACT_IDS} total={7} />
    </>
  );
}
