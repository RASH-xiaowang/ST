import type { Metadata, Viewport } from "next";
import "../globals.css";
import { themeInitScript } from "@/lib/theme-script";

export const metadata: Metadata = {
  title: "HARNESS",
  description: "Local-first AI agent runtime",
  robots: { index: false, follow: false },
};

export const viewport: Viewport = {
  themeColor: "#04060d",
  width: "device-width",
  initialScale: 1,
};

export default function RootRedirectLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="zh-CN" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeInitScript() }} />
      </head>
      <body className="antialiased">{children}</body>
    </html>
  );
}
