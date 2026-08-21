# 部署文档

## 1. 本地开发

```bash
# 环境要求：Node.js ≥ 20
npm install
npm run dev          # http://localhost:3000（根路径自动重定向到 /zh 或 /en）
```

主题/语言选择保存在 localStorage，不随构建变化。

## 2. 构建与产物

```bash
npm run build        # 1) 生成 OG 分享图（scripts/generate-assets.mjs → public/og.png）
                     # 2) next build → 静态导出 out/（output: "export"）
npm run serve        # npx serve out -l 3000 本地预览静态产物
```

产物特点：**纯静态**（41 个预渲染页面 + sitemap.xml + robots.txt + JSON-LD 内联），无服务端运行时；`trailingSlash: true` 目录式路由。

## 3. 部署方式

### A. Nginx / 任意静态托管

```bash
npm run build
# 将 out/ 上传到站点根目录即可；Nginx 参考配置见 docker/nginx.conf
# （gzip、_next/static 长缓存、目录路由回退 index.html）
```

### B. Docker

```bash
docker compose up -d --build        # http://localhost:8080
# 或
docker build --build-arg NEXT_PUBLIC_SITE_URL=https://your.domain -t ST Control-web .
docker run -p 8080:80 ST Control-web
```

多阶段构建：`node:20-alpine` 构建 → `nginx:1.27-alpine` 托管，含健康检查。

### C. CDN / 边缘平台（Vercel/Netlify/Cloudflare Pages）

直接部署 `out/`（构建命令 `npm run build`，输出目录 `out`）。`NEXT_PUBLIC_SITE_URL` 需在构建时设置为生产域名。

## 4. 环境变量

| 变量 | 必填 | 说明 |
|---|---|---|
| `NEXT_PUBLIC_SITE_URL` | 建议 | canonical / hreflang / sitemap / OG 的基础 URL（默认 https://ST Control.dev） |
| `NEXT_PUBLIC_FORM_ENDPOINT` | 否 | 联系表单 POST 转发端点；缺省时表单以本地演示模式运行（模拟成功） |
| `NEXT_PUBLIC_GA_ID` / `NEXT_PUBLIC_BAIDU_ID` | 否 | 统计（预留位，接入见 ops.md） |
| `NEXT_PUBLIC_SENTRY_DSN` | 否 | 错误监控（预留位） |

> 静态站点：所有 `NEXT_PUBLIC_*` 在构建时内联，修改后需重新构建。

## 5. CI/CD

`.github/workflows/ci.yml`：`lint → vitest → build → Playwright（三端设备）→ Lighthouse CI（lighthouserc.json 阈值断言）`；产物经 artifact 传递。合并到 main 后可将 out/ 同步到托管（按目标平台追加部署步骤）。
