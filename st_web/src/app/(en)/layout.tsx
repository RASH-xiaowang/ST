import type { Metadata, Viewport } from "next";
import "../globals.css";
import { themeInitScript } from "@/lib/theme-script";
import { LocaleChrome } from "@/components/layout/LocaleChrome";
import { siteUrl, type Locale } from "@/lib/i18n/locales";
import { META } from "../(zh)/meta";

export const metadata: Metadata = {
  metadataBase: new URL(process.env.NEXT_PUBLIC_SITE_URL ?? "https://st-control.dev"),
  title: {
    default: META.en.title,
    template: `%s · HARNESS`,
  },
  description: META.en.description,
  keywords: META.en.keywords,
  alternates: {
    canonical: siteUrl("en"),
    languages: { zh: siteUrl("zh"), en: siteUrl("en") },
  },
  openGraph: {
    title: META.en.title,
    description: META.en.description,
    url: siteUrl("en"),
    siteName: "HARNESS",
    locale: "en_US",
    alternateLocale: "zh_CN",
    type: "website",
    images: [{ url: "/og.png", width: 1200, height: 630, alt: "HARNESS" }],
  },
  twitter: {
    card: "summary_large_image",
    title: META.en.title,
    description: META.en.description,
    images: ["/og.png"],
  },
  robots: { index: true, follow: true },
};

export const viewport: Viewport = {
  themeColor: "#04060d",
  width: "device-width",
  initialScale: 1,
};

function jsonLd(locale: Locale) {
  return [
    {
      "@context": "https://schema.org",
      "@type": "Organization",
      name: "ST Control",
      url: siteUrl(locale),
      logo: siteUrl(locale, "favicon.ico"),
      description: META[locale].description,
      sameAs: ["https://github.com/harness-dev"],
    },
    {
      "@context": "https://schema.org",
      "@type": "SoftwareApplication",
      name: "ST Control",
      applicationCategory: "DeveloperApplication",
      operatingSystem: "Windows",
      offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
      aggregateRating: { "@type": "AggregateRating", ratingValue: "4.8", ratingCount: "126" },
    },
  ];
}

export default function EnRootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeInitScript() }} />
      </head>
      <body className="antialiased">
        {jsonLd("en").map((obj, i) => (
          <script
            key={i}
            type="application/ld+json"
            dangerouslySetInnerHTML={{ __html: JSON.stringify(obj) }}
          />
        ))}
        <LocaleChrome locale="en">{children}</LocaleChrome>
      </body>
    </html>
  );
}
