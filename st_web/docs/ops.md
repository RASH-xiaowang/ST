# 运维手册

## 1. 架构与运行模型

纯静态站点：无应用服务端。运行时风险面 = 静态托管层 + 客户端浏览器环境。

## 2. 监控

- **错误监控（Sentry）**：预留 `NEXT_PUBLIC_SENTRY_DSN`。接入步骤：
  1. `.env` 配置 DSN 并在构建时注入；
  2. 创建 `src/lib/telemetry.ts`，按需引入 `@sentry/nextjs`（静态导出模式下使用 `@sentry/browser` 的 `Sentry.init` + `captureException`）；
  3. 在 `app/(zh|en)/layout.tsx` 挂一个客户端 `TelemetryInit` 组件即可。
- **前端异常兜底**：3D 引擎 WebGL 上下文丢失/不支持会自动降级 2D 静态视觉（`useSceneCanvas.failed`），不影响内容访问；建议在 Sentry 中为 `webgl-context-lost` 自定义事件建告警。

## 3. 分析

- Google Analytics：预留 `NEXT_PUBLIC_GA_ID`，构建时注入后挂载 GA4 gtag 脚本（`app/(zh|en)` 布局或独立 `<Script>`）。
- 百度统计：`NEXT_PUBLIC_BAIDU_ID`，国内部署建议启用。
- 隐私提示：当前站点零埋点、零遥测默认开启，符合「本地优先」品牌叙事。

## 4. 日志与缓存

- 静态托管层（Nginx/CDN）访问日志即全部日志；`docker/nginx.conf` 已启用 gzip 与 `_next/static` 长缓存（immutable，内容哈希）。
- 发版后如需立即生效：HTML 默认 `must-revalidate`（5 分钟），静态资源永久缓存由哈希保证安全。

## 5. 备份与回滚

- 站点 = 纯文件：备份 `out/`（或 Docker 镜像 tag）即可全量回滚；建议每次发版保留上一版本产物。
- 内容数据（docs/blog/FAQ 等）在 `src/lib/content/*.ts`，进入 Git 版本管理，天然可审计。

## 6. 常见故障处理

| 现象 | 处理 |
|---|---|
| 页面白屏 | 查看浏览器控制台；多为 CDN 缓存旧 chunk——清缓存或等待 5 分钟 HTML 过期 |
| 3D 不显示 | 正常降级（WebGL 不可用/减动效/低端档位显示 2D 静态视觉）；如需确认档位可在控制台执行 `localStorage.clear()` 后重载 |
| 表单提交失败 | 检查 `NEXT_PUBLIC_FORM_ENDPOINT` 可达性与 CORS；缺省演示模式不会失败 |
| 语言跳转异常 | 清除 `ST Control-locale` localStorage；根路径 `/` 的 meta refresh 是无 JS 兜底 |
