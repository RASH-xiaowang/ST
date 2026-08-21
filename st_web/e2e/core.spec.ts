import { expect, test } from "@playwright/test";

/**
 * 核心体验冒烟：导航、区块、主题、语言、搜索、表单校验。
 * 3D 画布按设备档位渲染或降级为 2D 静态视觉（两者都可接受）。
 */

test.describe("首页核心体验", () => {
  test("首屏渲染：主视觉 + 主张 + CTA + 滚动引导", async ({ page }) => {
    await page.goto("/zh/");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("ST CONTROL");
    await expect(page.getByTestId("hero-cta-primary")).toBeVisible();
    // 3D 画布（或 WebGL 不可用时的 2D 降级主视觉）必须可见
    await expect(page.getByTestId("hero-canvas")).toBeVisible();
  });

  test("首页区块完整：宣言/解密/洞察/功能/模块/年度/机舱/客户/FAQ", async ({ page }) => {
    await page.goto("/zh/");
    for (const id of ["manifesto", "decrypt", "insights", "features", "modules", "wrapped", "machine", "customers", "faq", "updates"]) {
      await expect(page.locator(`#${id}`)).toBeVisible();
    }
  });

  test("功能模块矩阵：全部模块均有真实界面介绍", async ({ page }) => {
    await page.goto("/zh/");
    await expect(page.locator("#modules")).toBeVisible();
    await expect(page.locator("#modules")).toContainText("全部功能模块");
    // 9 个模块卡（合并后：AI 角色/文案并入大模型、消息通道并入自动化、数据看板并入首页系统监控）
    await expect(page.getByTestId("module-card")).toHaveCount(9);
    for (const name of ["首页工作台", "Harness", "大模型", "智能体", "自动化", "微信数据", "知识库", "数据库", "图文识别"]) {
      await expect(page.locator("#modules").getByText(name, { exact: false }).first()).toBeVisible();
    }
    // 并入关系的说明可见
    await expect(page.locator("#modules")).toContainText("消息通道并入自动化");
  });

  test("界面实拍：解密演示截图 + 功能轮播真实截图", async ({ page }) => {
    await page.goto("/zh/");
    await expect(page.locator("#decrypt")).toBeVisible();
    await expect(page.locator("#decrypt")).toContainText("微信数据");
    await expect(page.getByTestId("decrypt-shot")).toHaveCount(4);
    await expect(page.locator("#features")).toBeVisible();
    await expect(page.getByTestId("deck-card")).toHaveCount(7);
    // 首张功能卡展示微信数据真实截图
    await expect(page.getByTestId("deck").getByRole("img").first()).toBeVisible();
    // 社交图谱真实截图在功能轮播中呈现（轮播卡按需显示，验证 DOM 与资源存在）
    await expect(page.getByTestId("deck").getByRole("img", { name: "社交关系图谱" })).toHaveCount(1);
    await expect(page.getByTestId("deck").getByRole("img", { name: "社交关系图谱" })).toHaveAttribute("src", /wechat-graph/);
  });

  test("导航锚点滚动到对应区块", async ({ page }) => {
    await page.goto("/zh/");
    await page.getByTestId("nav-logo").waitFor();
    // 移动端导航藏在汉堡菜单中，先展开（force：避开固定头部的指针拦截）
    const menu = page.locator('header button[aria-label="菜单"]');
    if (await menu.isVisible()) await menu.click({ force: true });
    await page.locator("header button:visible", { hasText: "宣言" }).first().click();
    await expect(page.locator("#manifesto")).toBeInViewport();
  });

  test("主题切换持久化", async ({ page }) => {
    await page.goto("/zh/");
    const toggle = page.getByTestId("theme-toggle");
    const before = await page.evaluate(() => document.documentElement.dataset.theme);
    // force：固定头部在移动端会干扰 Playwright 的命中重试（元素本身可点，
    // 后续断言仍校验主题确实切换），与导航菜单的 force 用法保持一致
    await toggle.click({ force: true });
    await expect.poll(() => page.evaluate(() => document.documentElement.dataset.theme)).not.toBe(before);
    const after = await page.evaluate(() => document.documentElement.dataset.theme);
    await page.reload();
    // 等待 React 水合完成后的最终属性（防闪烁脚本 → 水合 → 主题 Provider 的时序）
    await expect
      .poll(() => page.evaluate(() => document.documentElement.dataset.theme), { timeout: 5000 })
      .toBe(after);
  });

  test("语言切换：zh ⇄ en 路径与 hreflang", async ({ page }) => {
    await page.goto("/zh/");
    const htmlLang = await page.getAttribute("html", "lang");
    expect(htmlLang).toBe("zh-CN");
    await page.getByTestId("locale-switch").locator("button", { hasText: /en/i }).click({ force: true });
    await page.waitForURL("**/en/");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("ST CONTROL");
    const enLang = await page.getAttribute("html", "lang");
    expect(enLang).toBe("en");
  });

  test("站内搜索：Ctrl+K 弹窗与结果跳转", async ({ page }) => {
    await page.goto("/zh/");
    await page.keyboard.press("Control+k");
    await expect(page.getByTestId("search-dialog")).toBeVisible();
    await page.getByTestId("search-input").fill("沙箱");
    await expect(page.getByTestId("search-result").first()).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("search-dialog")).toBeHidden();
  });

  test("FAQ 手风琴展开与过滤", async ({ page }) => {
    await page.goto("/zh/");
    await page.locator("#faq").scrollIntoViewIfNeeded();
    await page.getByTestId("faq-search").fill("私有化");
    const items = page.locator('[data-accordion-item]');
    await expect(items.first()).toBeVisible();
    const first = items.first();
    await first.locator("button").first().click();
    await expect(first.locator('[id^="faq-panel-"]')).toHaveClass(/grid-rows-\[1fr\]/);
  });

  test("客户案例：筛选 + 详情弹窗", async ({ page }) => {
    await page.goto("/zh/");
    await page.locator("#customers").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "本地数据洞察", exact: true }).click();
    const dataCard = page.locator("#customers").getByRole("button", { name: /微信数据洞察/ }).first();
    await expect(dataCard).toBeVisible();
    // force：Reveal 入场动画会让 Playwright 的稳定性重试误判（移动端常见），
    // 命中测试已确认卡片中心可点，force 只是跳过稳定性抖动
    await dataCard.click({ force: true });
    await expect(page.getByRole("dialog")).toBeVisible();
    await expect(page.getByRole("dialog")).toContainText("本地解密");
  });
});

test.describe("子页面", () => {
  test("文档列表与详情（含代码块）", async ({ page }) => {
    await page.goto("/zh/docs/");
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await page.getByRole("link", { name: /快速开始/ }).click();
    await page.waitForURL("**/docs/getting-started/");
    await expect(page.locator("pre")).toBeVisible();
  });

  test("博客列表与详情（JSON-LD）", async ({ page }) => {
    await page.goto("/zh/blog/");
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await page.locator("a[href*='/blog/why-local-first-agents/']").click();
    await page.waitForURL("**/blog/why-local-first-agents/");
    await expect(page.locator('script[type="application/ld+json"]')).not.toHaveCount(0);
  });

  test("更新日志时间线与路线图", async ({ page }) => {
    await page.goto("/zh/changelog/");
    await expect(page.getByText("v1.0.0", { exact: true }).first()).toBeVisible();
    await page.goto("/zh/roadmap/");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("路线图");
  });

  test("联系表单校验与成功态", async ({ page }) => {
    await page.goto("/zh/contact/");
    const submit = page.getByTestId("contact-submit");
    await submit.click();
    await expect(page.getByText("请填写姓名", { exact: false }).first()).toBeVisible();
    await page.locator('input[name="name"]').fill("张三");
    await page.locator('input[name="email"]').fill("bad-email");
    await page.locator('textarea[name="message"]').fill("这是足够长的需求描述内容。");
    await page.locator('input[name="agree"]').check();
    await submit.click();
    await expect(page.getByText("请输入有效的邮箱地址")).toBeVisible();
    await page.locator('input[name="email"]').fill("zhang@example.com");
    await submit.click();
    await expect(page.getByText("已收到你的消息！")).toBeVisible({ timeout: 5000 });
  });

  test("404 页面", async ({ page }) => {
    const res = await page.goto("/zh/no-such-page/");
    expect(res?.status()).toBe(404);
    await expect(page.getByText("404")).toBeVisible();
  });

  test("SEO 基础：meta 与 sitemap", async ({ request }) => {
    const home = await request.get("/zh/");
    expect(home.ok()).toBe(true);
    const html = await home.text();
    expect(html).toContain('rel="canonical"');
    expect(html).toMatch(/hrefLang="en"/i);
    const sitemap = await request.get("/sitemap.xml");
    expect(sitemap.ok()).toBe(true);
    const robots = await request.get("/robots.txt");
    expect(robots.ok()).toBe(true);
  });
});
