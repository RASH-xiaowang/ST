// ============================================================
// 微信数据面板 · 全屏界面截图审计
// 前置：应用以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 启动
// 运行：node st_control/.codex_tests/audit-shots.mjs
// 逐个切换微信数据面板的导航 tab 并全窗口截图，供人工检查布局。
// ============================================================
import { chromium } from 'playwright-core';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const OUT = path.resolve('E:/ST/.codex_shots/wechat_ui');
fs.mkdirSync(OUT, { recursive: true });

const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
const ctx = browser.contexts()[0];
const page = ctx.pages().find((p) => p.url().includes('localhost:1420')) ?? ctx.pages()[0];
await page.bringToFront().catch(() => {});
await page.waitForTimeout(2500);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function shot(name) {
  const buf = await page.screenshot();
  fs.writeFileSync(path.join(OUT, `${name}.png`), buf);
  console.log('shot:', name);
}

async function clickNav(label) {
  const ok = await page.evaluate((label) => {
    const els = [...document.querySelectorAll('button, [role="button"], a, li, span, div')];
    const el = els.find((e) => {
      const t = (e.textContent || '').trim();
      return t === label && e.children.length <= 2 && e.offsetParent !== null && e.getClientRects().length > 0;
    });
    if (!el) return false;
    el.click();
    return true;
  }, label);
  return ok;
}

async function dumpNav() {
  return page.evaluate(() => {
    const out = [];
    for (const el of document.querySelectorAll('button, [role="button"], [class*="nav"], [class*="tab"]')) {
      const t = (el.textContent || '').trim().slice(0, 40);
      if (t && el.getClientRects().length > 0 && el.offsetParent !== null) out.push(t);
    }
    return [...new Set(out)].slice(0, 80);
  });
}

console.log('URL:', page.url());
console.log('TITLE:', await page.title().catch(() => '(no title)'));
console.log('NAV candidates:', JSON.stringify(await dumpNav(), null, 0));
await shot('00-boot');

// 微信数据面板导航（NAV_GROUPS 语义）
const tabs = [
  ['01-overview', '数据总览'],
  ['02-chats', '聊天'],
  ['03-ask', 'AI 问答'],
  ['04-graph', '关系图谱'],
  ['05-monitor', '群监控'],
  ['06-contacts', '通讯录'],
  ['07-moments', '朋友圈'],
  ['08-favorites', '收藏'],
  ['09-emoticons', '表情'],
  ['10-files', '文件'],
  ['11-records', '记录'],
  ['12-bizchats', '公众号'],
  ['13-servicechats', '服务号'],
  ['14-kefu', '客服'],
  ['15-annual', '年度总结'],
  ['16-dailysummary', '每日总结'],
  ['17-revoked', '撤回记录'],
  ['18-storage', '存储空间'],
  ['19-privacy', '隐私体检'],
  ['20-backup', '备份管家'],
  ['21-hook', '原图Hook'],
  ['22-settings', '设置'],
];

for (const [name, label] of tabs) {
  const ok = await clickNav(label);
  await sleep(1400);
  await shot(name + (ok ? '' : '-NOTFOUND'));
  if (!ok) console.log('  ! 未找到导航项:', label);
}

await browser.close();
console.log('done ->', OUT);
