// ============================================================
// 微信数据面板 · 数值化布局审计
// 前置：应用已启动且打开「微信数据」面板（CDP 9222）
// 检测项：console 错误 / 横向溢出 / 面板容器尺寸 / 零高元素 /
//         关键容器高度异常 / 截图存档
// 运行：node st_control/.codex_tests/audit-layout.mjs
// ============================================================
import { chromium } from 'playwright-core';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const OUT = path.resolve('C:/Users/28361/AppData/Local/Temp/st_ui_audit');
fs.mkdirSync(OUT, { recursive: true });

const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
const ctx = browser.contexts()[0];
const page = ctx.pages().find((p) => p.url().includes('localhost:1420')) ?? ctx.pages()[0];
await page.waitForTimeout(1200);

const issues = [];
let current = '(boot)';
page.on('console', (m) => { if (m.type() === 'error') issues.push(`[console] ${current}: ${m.text().slice(0, 300)}`); });
page.on('pageerror', (e) => issues.push(`[pageerror] ${current}: ${String(e).slice(0, 300)}`));
page.on('requestfailed', (r) => issues.push(`[reqfail] ${current}: ${r.url().slice(0, 140)} ${r.failure()?.errorText ?? ''}`));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function clickTab(label) {
  return page.evaluate((label) => {
    const els = [...document.querySelectorAll('button.wc-ihb, .wc-ihb')];
    const el = els.find((e) => {
      const t = (e.textContent || '').trim();
      return t === label && e.getClientRects().length > 0;
    });
    if (!el) return false;
    el.click();
    return true;
  }, label);
}

async function audit(name, label) {
  current = label;
  await sleep(1600);
  const m = await page.evaluate(() => {
    const d = document.documentElement;
    const body = document.body;
    const report = {
      innerW: window.innerWidth,
      innerH: window.innerHeight,
      docScrollW: d.scrollWidth,
      docScrollH: d.scrollHeight,
      bodyScrollW: body.scrollWidth,
    };
    // 主面板容器（wc- 前缀的最外层）
    const containers = [];
    for (const el of document.querySelectorAll('[class*="wc-"]')) {
      const cls = el.className.toString();
      const r = el.getBoundingClientRect();
      if (r.width < 10 || r.height < 10) continue;
      if (cls.split(' ').filter((c) => c.startsWith('wc-')).length > 0 && el.parentElement?.className?.toString().includes('wc-') === false) {
        containers.push({ cls: cls.slice(0, 60), w: Math.round(r.width), h: Math.round(r.height) });
      }
    }
    report.containers = containers.slice(0, 10);
    // 零高度但非空的 wc 元素
    const zero = [];
    for (const el of document.querySelectorAll('[class*="wc-"]')) {
      const r = el.getBoundingClientRect();
      if (r.width > 50 && r.height === 0 && (el.textContent || '').trim().length > 0) {
        zero.push((el.className || '').toString().slice(0, 50));
      }
    }
    report.zeroHeight = [...new Set(zero)].slice(0, 8);
    // 可见文本溢出视口（横向溢出元素）
    const overflow = [];
    for (const el of document.querySelectorAll('*')) {
      const r = el.getBoundingClientRect();
      if (r.width > 0 && (r.left < -2 || r.right > window.innerWidth + 2)) {
        const cs = getComputedStyle(el);
        if (cs.position === 'fixed') continue;
        const t = (el.textContent || '').trim().slice(0, 24);
        if (t) overflow.push({ t, cls: (el.className || '').toString().slice(0, 40), left: Math.round(r.left), right: Math.round(r.right) });
        if (overflow.length > 12) break;
      }
    }
    report.overflowEls = overflow;
    return report;
  });

  const flag = [];
  if (m.docScrollW > m.innerW + 2) flag.push(`横向溢出 ${m.docScrollW} > ${m.innerW}`);
  if (m.bodyScrollW > m.innerW + 2) flag.push(`body 横向溢出 ${m.bodyScrollW}`);
  for (const z of m.zeroHeight) flag.push(`零高元素 ${z}`);
  for (const o of m.overflowEls) flag.push(`元素出界 [${o.t}] ${o.cls} L${o.left} R${o.right}`);

  try {
    const buf = await page.screenshot();
    fs.writeFileSync(path.join(OUT, `${name}.png`), buf);
  } catch { /* 截图失败不阻断 */ }

  const line = `${name}(${label}): ${flag.length ? '⚠ ' + flag.join(' | ') : 'OK'} 容器: ${m.containers.map((c) => `${c.cls.split(' ')[0]}@${c.w}x${c.h}`).join(',') || 'none'}`;
  console.log(line);
  return flag;
}

// 面板 tab 序列（按导航组顺序）
const tabs = [
  ['t01-overview', '数据总览'],
  ['t02-chats', '聊天'],
  ['t03-ask', 'AI 问答'],
  ['t04-graph', '关系图谱'],
  ['t05-monitor', '群监控'],
  ['t06-contacts', '通讯录'],
  ['t07-moments', '朋友圈'],
  ['t08-favorites', '收藏'],
  ['t09-emoticons', '表情'],
  ['t10-files', '文件'],
  ['t11-records', '记录'],
  ['t12-storage', '存储空间'],
  ['t13-bizchats', '公众号'],
  ['t14-servicechats', '服务号'],
  ['t15-kefu', '客服'],
  ['t16-annual', '年度总结'],
  ['t17-dailysummary', '每日总结'],
  ['t18-revoked', '撤回记录'],
  ['t19-hook', '原图Hook'],
  ['t20-privacy', '隐私体检'],
  ['t21-backup', '备份管家'],
  ['t22-settings', '设置'],
];

const allFlags = [];
for (const [name, label] of tabs) {
  const ok = await clickTab(label);
  if (!ok) { console.log(`${name}: ! 未找到 tab ${label}`); continue; }
  const f = await audit(name, label);
  allFlags.push({ name, label, flags: f });
}

console.log('\n==== 汇总 ====');
let bad = 0;
for (const it of allFlags) {
  if (it.flags.length) { bad++; console.log(`⚠ ${it.label}: ${it.flags.join(' ; ')}`); }
}
console.log(`有问题 tab: ${bad}/${allFlags.length}`);
console.log('console 问题:', issues.length ? issues.slice(0, 20) : '无');
await browser.close();

