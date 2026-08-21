import type { NextConfig } from "next";

// 静态导出（SSG）：SEO 友好、可部署到任意静态托管 / Nginx / CDN。
// 国际化使用显式 [locale] 路由（zh / en）；静态导出不支持 middleware，
// 根路由由客户端按存储/系统语言重定向。
const nextConfig: NextConfig = {
  output: "export",
  trailingSlash: true,
  images: { unoptimized: true },
  reactStrictMode: true,
};

export default nextConfig;
