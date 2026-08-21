"use client";

/**
 * 语言根布局的客户端壳：主题 + 平滑滚动 + 搜索 + 导航 + 页脚。
 * （(zh)/(en) 两个根布局共用，保证 <html lang> 静态正确输出。）
 */
import { ThemeProvider } from "@/lib/theme";
import { SmoothScrollProvider } from "@/components/layout/SmoothScroll";
import { SearchProvider } from "@/components/search/SearchContext";
import { SearchDialog } from "@/components/search/SearchDialog";
import { Nav } from "@/components/layout/Nav";
import { Footer } from "@/components/layout/Footer";
import type { Locale } from "@/lib/i18n/locales";

export function LocaleChrome({
  locale,
  children,
}: {
  locale: Locale;
  children: React.ReactNode;
}) {
  return (
    <ThemeProvider>
      <SmoothScrollProvider>
        <SearchProvider>
          <Nav locale={locale} />
          <main id="main">{children}</main>
          <Footer locale={locale} />
          <SearchDialog locale={locale} />
        </SearchProvider>
      </SmoothScrollProvider>
    </ThemeProvider>
  );
}
