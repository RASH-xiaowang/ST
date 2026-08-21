# ST Control — 产品官方网站

> 本地优先 AI 代理运行时 ST Control 的商业级产品官网：深色科技风、Three.js 3D 视觉、中英双语、静态导出（SSG）。

## 技术栈

| 层 | 选型 |
|---|---|
| 框架 | Next.js 15（App Router，静态导出 SSG） |
| 语言 | TypeScript（strict） |
| 样式 | Tailwind CSS 4（CSS-first 设计令牌）+ CSS 变量主题 |
| 3D | Three.js（原生引擎，自适应质量档位） |
| 动画 | Lenis 平滑滚动 + IntersectionObserver 滚动进场 + rAF 统一循环 |
| 测试 | Vitest（单元）+ Playwright（E2E 三端设备）+ Lighthouse CI |
| 部署 | 纯静态产物 → Nginx / 任意 CDN / Docker |

## 快速开始

```bash
npm install
npm run dev        # http://localhost:3000 → 根路径按语言重定向 /zh /en
npm run test       # 单元测试
npm run build      # 生成 OG 图 + next build（产物 out/）
npm run serve      # 静态产物本地预览 http://localhost:3000
npx playwright install chromium && npm run test:e2e   # E2E（需先 build）
```

## 目录结构

```
src/
  app/
    (root)/          # / 语言重定向页（无 JS 时 meta refresh 兜底）
    (zh)/ (en)/      # 两个根布局：静态输出正确的 <html lang> 与 hreflang
      zh/ en/        # 各语言路由（首页/定价/文档/博客/日志/路线图/联系/搜索）
    sitemap.ts robots.ts
  components/
    sections/        # 首页 10 个区块
    three/           # 3D 引擎：自适应质量 / 背景场景 / 产品查看器 / 数据可视化
    pages/           # 子页面组件
    layout/ ui/ search/
  lib/
    i18n/            # locale 类型与双语结构
    content/         # 全部内容数据（zh/en 双语齐备）
    search.ts        # 站内搜索索引（内容源即索引）
    theme.tsx        # 明暗主题（跟随系统 + localStorage 记忆 + 防闪烁脚本）
docs/                # 设计系统 / 3D / 动画 / 部署 / 运维 / 测试报告
docker/              # Nginx 配置
e2e/                 # Playwright 用例
scripts/             # OG 图生成（sharp）
```

## 环境变量

见 `.env.example`：`NEXT_PUBLIC_SITE_URL`（canonical/hreflang/sitemap 基础 URL）、`NEXT_PUBLIC_FORM_ENDPOINT`（联系表单转发，缺省演示模式）、`NEXT_PUBLIC_GA_ID` / `NEXT_PUBLIC_SENTRY_DSN`（可选）。

## 部署

```bash
npm run build                      # 产物 out/
docker compose up -d --build       # 或 Docker 一键部署（:8080）
```

详细说明见 `docs/deployment.md`；CI 流水线见 `.github/workflows/ci.yml`（lint → 单测 → 构建 → Playwright → Lighthouse CI）。
