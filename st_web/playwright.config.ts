import { defineConfig, devices } from "@playwright/test";

/**
 * E2E：先 `npm run build` 并启动静态服务器（npx serve out -l 3000），
 * 或使用 `webServer` 自动启动。三档设备覆盖桌面 / 平板 / 移动。
 */
export default defineConfig({
  testDir: "./e2e",
  timeout: 45_000,
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: "http://127.0.0.1:3000",
    trace: "on-first-retry",
    screenshot: "only-on-failure",
  },
  projects: [
    { name: "desktop", use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 900 } } },
    { name: "tablet", use: { browserName: "chromium", viewport: { width: 1024, height: 768 } } },
    { name: "mobile", use: { ...devices["iPhone 13"], browserName: "chromium" } },
  ],
  webServer: {
    command: "npx serve out -l 3000",
    url: "http://127.0.0.1:3000/zh/",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
