# 测试报告

## 1. 测试策略

| 层 | 工具 | 覆盖 |
|---|---|---|
| 单元 | Vitest + Testing Library | i18n 核心、主题系统、站内搜索、内容数据双语完整性 |
| E2E | Playwright（桌面/平板/移动三端） | 首屏与区块、导航锚点、主题/语言切换、搜索弹窗、FAQ、案例弹窗、文档/博客/日志/路线图、联系表单校验、404、SEO meta |
| 性能 | Lighthouse CI（lighthouserc.json） | performance ≥ 85（warn）、a11y/best-practices/SEO ≥ 90（error）、LCP ≤ 2.5s、CLS ≤ 0.1 |

## 2. 执行结果（2026-08-18）

### 单元测试（Vitest）

```
Test Files  4 passed (4)
     Tests 18 passed (18)
  Duration  ~1s
```

- `i18n.test.ts`：locale 识别/回退/pick 取值
- `search.test.ts`：索引覆盖域、多词 AND、英文检索、空查询
- `content.test.ts`：定价/对比表/文档/博客/日志/路线图/FAQ/案例双语完整性
- `theme.test.tsx`：跟随系统、切换持久化、html data-theme 同步

### E2E（Playwright）

三端设备（Desktop Chromium 1440×900 / 平板 Chromium 1024×768 / 移动 Chromium iPhone 13 视口）执行同一套核心用例：**42/42 全过**。关键覆盖：
- 首屏 H1/CANVAS（或 2D 降级主视觉）/CTA、六大区块可见
- 主题切换 + 刷新持久化（含水合竞态的轮询断言）；语言切换 zh⇄en（含 `<html lang>` 与 hreflang）
- Ctrl+K 搜索弹窗、FAQ 手风琴与过滤、案例筛选 + 详情弹窗
- 文档/博客/更新日志/路线图/联系表单（校验 + 成功态）/404
- sitemap.xml 与 robots.txt 可访问、canonical/hreflang 输出

### Lighthouse

- 配置：`lighthouserc.json`（performance ≥ 85 warn、a11y/best-practices/SEO ≥ 90 error、LCP ≤ 2.5s、CLS ≤ 0.1），CI 中 `lhci autorun` 自动执行并上传报告。
- 本机 Windows 环境下 Lighthouse CLI 因浏览器启动限制未能完成实测（chrome-launcher 与本机代理环境冲突）；分数以 CI 结果为准。静态导出 + 零位图资源 + 可变字体子集化（@fontsource unicode-range）+ 首屏共享 JS ≈103kB，为性能预算预留了充足余量。

## 3. 已知限制

- Playwright 未覆盖 WebGL 像素级渲染（不同驱动差异大）；3D 以「画布存在 + 降级组件出现」二选一断言，视觉正确性由人工走查。
- 跨浏览器（Firefox/Safari）建议在 CI 中按需扩展对应 project（配置已预留多项目结构）。
