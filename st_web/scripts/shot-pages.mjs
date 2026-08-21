// 视觉验收截图：zh/en 首页、文档、移动端视口
import { chromium } from "@playwright/test";

const browser = await chromium.launch();
const shots = [
  { url: "http://127.0.0.1:3000/zh/", name: "01-zh-home", viewport: { width: 1440, height: 900 }, dark: true },
  { url: "http://127.0.0.1:3000/en/", name: "02-en-home", viewport: { width: 1440, height: 900 }, dark: true },
  { url: "http://127.0.0.1:3000/zh/docs/", name: "03-zh-docs", viewport: { width: 1440, height: 900 }, dark: true },
  { url: "http://127.0.0.1:3000/zh/docs/api/", name: "04-zh-api", viewport: { width: 1440, height: 900 }, dark: true },
  { url: "http://127.0.0.1:3000/zh/", name: "05-zh-mobile", viewport: { width: 390, height: 844 }, dark: true },
  { url: "http://127.0.0.1:3000/zh/", name: "06-zh-light", viewport: { width: 1440, height: 900 }, dark: false },
];
for (const s of shots) {
  const page = await browser.newPage({ viewport: s.viewport });
  if (!s.dark) {
    await page.addInitScript(() => {
      localStorage.setItem("harness-theme", "light");
      localStorage.setItem("harness-locale", "zh");
    });
  } else {
    await page.addInitScript(() => {
      localStorage.setItem("harness-theme", "dark");
    });
  }
  await page.goto(s.url, { waitUntil: "networkidle", timeout: 60000 });
  await page.waitForTimeout(2500);
  await page.screenshot({ path: `data/ui-audit/${s.name}.png`, fullPage: s.name.includes("mobile") ? false : false });
  console.log("SAVED=" + s.name);
  await page.close();
}
await browser.close();
console.log("DONE");
