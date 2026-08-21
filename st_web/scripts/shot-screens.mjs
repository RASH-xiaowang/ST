// 截取界面实拍区块（滚动到 #features 功能轮播 / #decrypt 解密演示）
import { chromium } from "@playwright/test";
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 1200 } });
await page.goto("http://127.0.0.1:3000/zh/", { waitUntil: "networkidle", timeout: 60000 });
await page.locator("#features").scrollIntoViewIfNeeded();
await page.waitForTimeout(1200);
await page.screenshot({ path: "data/ui-audit/07-screens-section.png" });
// 解密演示区块：微信真实截图小卡 + 高仿聊天动画
await page.locator("#decrypt").scrollIntoViewIfNeeded();
await page.waitForTimeout(1200);
await page.screenshot({ path: "data/ui-audit/08-screens-wechat.png" });
console.log("DONE");
await browser.close();
