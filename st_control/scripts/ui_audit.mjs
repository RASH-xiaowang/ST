// ST 控制台 · 微信数据 UI 层自动化审计
// 前置：应用以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 启动
// 运行：node scripts/ui_audit.mjs
import { chromium } from 'playwright-core';
import fs from 'node:fs';
import path from 'node:path';

const CDP = 'http://127.0.0.1:9222';
const SHOT_DIR = path.resolve('E:/ST/.codex_shots/ui_audit');
fs.mkdirSync(SHOT_DIR, { recursive: true });

const TAB_LABELS = [
  '聊天', 'AI 问答', '关系图谱', '群监控',
  '通讯录', '朋友圈', '收藏', '表情', '文件', '记录',
  '公众号', '服务号', '客服',
  '年度总结', '每日总结',
  '原图Hook', '隐私体检', '备份管家',
];

const browser = await chromium.connectOverCDP(CDP);
const ctx = browser.contexts()[0];
const page = ctx.pages().find((p) => p.url().includes('localhost:1420')) ?? ctx.pages()[0];

const issues = [];
const errorsByTab = new Map();
let currentTab = '(boot)';

page.on('console', (m) => {
  if (m.type() === 'error') {
    const t = m.text().slice(0, 400);
    issues.push(`[console:error] ${currentTab}: ${t}`);
    if (!errorsByTab.has(currentTab)) errorsByTab.set(currentTab, []);
    errorsByTab.get(currentTab).push(t);
  }
});
page.on('pageerror', (e) => {
  const t = String(e).slice(0, 400);
  issues.push(`[pageerror] ${currentTab}: ${t}`);
  if (!errorsByTab.has(currentTab)) errorsByTab.set(currentTab, []);
  errorsByTab.get(currentTab).push(t);
});
page.on('requestfailed', (r) => {
  const t = `${r.method()} ${r.url().slice(0, 160)} ${r.failure()?.errorText ?? ''}`;
  issues.push(`[reqfail] ${currentTab}: ${t}`);
});
page.on('response', (r) => {
  if (r.status() >= 400) {
    issues.push(`[http ${r.status()}] ${currentTab}: ${r.url().slice(0, 160)}`);
  }
});

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function clickText(label, scope = 'body') {
  return page.evaluate(
    ({ label, scope }) => {
      const root = scope === 'body' ? document.body : document.querySelector(scope);
      if (!root) return false;
      const els = [...root.querySelectorAll('button, [role="button"], a, li, div, span')];
      const el = els.find((e) => {
        const t = (e.textContent || '').trim();
        return t === label && e.children.length <= 4 && e.offsetParent !== null;
      });
      if (!el) return false;
      el.click();
      return true;
    },
    { label, scope },
  );
}

async function snapshot(name) {
  const data = await page.evaluate(() => {
    const text = (document.body?.innerText || '').trim();
    const toasts = [...document.querySelectorAll('[data-sonner-toast], [data-type="error"], [role="alert"]')]
      .map((e) => (e.textContent || '').trim().slice(0, 200))
      .filter(Boolean)
      .slice(0, 6);
    const imgs = [...document.querySelectorAll('img')];
    const imgsOk = imgs.filter((i) => i.complete && i.naturalWidth > 0).length;
    const imgsBad = imgs.filter((i) => i.complete && i.naturalWidth === 0 && i.src).length;
    return { text: text.slice(0, 1200), toasts, imgs: imgs.length, imgsOk, imgsBad };
  });
  await page.screenshot({ path: path.join(SHOT_DIR, `${name}.png`) }).catch(() => {});
  return data;
}

const report = [];
function add(name, status, detail) {
  report.push({ name, status, detail });
  console.log(`${status.padEnd(5)} ${name.padEnd(10)} ${detail.slice(0, 110)}`);
}

// ── 进入微信数据 ──
await page.goto('http://localhost:1420/', { waitUntil: 'domcontentloaded', timeout: 30000 }).catch(() => {});
await page.waitForTimeout(4000);
currentTab = 'boot';
const entered = await clickText('微信数据');
add('进入微信数据', entered ? 'PASS' : 'FAIL', entered ? '侧栏按钮点击成功' : '未找到侧栏按钮');
await page.waitForTimeout(6000);

// 等待微信主面板出现（启动页可能先出现）
let panelReady = false;
for (let i = 0; i < 12; i++) {
  const ready = await page.evaluate(() => {
    const t = document.body?.innerText || '';
    return t.includes('聊天') && t.includes('通讯录') && t.includes('朋友圈');
  });
  if (ready) { panelReady = true; break; }
  await sleep(2500);
}
add('微信面板就绪', panelReady ? 'PASS' : 'WARN', panelReady ? '主面板已渲染' : '未检测到主面板（可能停在启动页）');
await snapshot('00_wechat_panel');

// ── 逐 Tab 点检 ──
for (const label of TAB_LABELS) {
  currentTab = label;
  const clicked = await clickText(label);
  await page.waitForTimeout(label === '年度总结' || label === '关系图谱' || label === '隐私体检' ? 6000 : 2200);
  const snap = await snapshot(`tab_${label}`);
  const textOk = snap.text.replace(/\s+/g, '').length > 30;
  const hasErrorToast = snap.toasts.some((t) => /失败|错误|异常/.test(t));
  const tabErrors = (errorsByTab.get(label) || []).filter((e) => !/GL_INVALID|WebGL/.test(e));
  let status = !clicked ? 'FAIL' : hasErrorToast || tabErrors.length > 0 ? 'WARN' : textOk ? 'PASS' : 'WARN';
  const detailBits = [];
  if (!clicked) detailBits.push('未找到导航项');
  detailBits.push(`正文${textOk ? snap.text.length : 0}字`);
  if (snap.imgs > 0) detailBits.push(`图片 ${snap.imgsOk}/${snap.imgs}`);
  if (hasErrorToast) detailBits.push(`错误提示: ${snap.toasts.join(' | ')}`);
  if (tabErrors.length) detailBits.push(`控制台错误: ${tabErrors[0].slice(0, 120)}`);
  if (status === 'PASS' && !textOk) status = 'WARN';
  add(`Tab ${label}`, status, detailBits.join(' · '));
}

// ── 聊天深度检查：打开第一个会话 ──
currentTab = '聊天-打开会话';
await clickText('聊天');
await page.waitForTimeout(1500);
const firstSession = await page.evaluate(() => {
  const els = [...document.querySelectorAll('[class*="session"], [class*="conv"], [class*="chat-item"], [class*="chat_item"], li, [role="button"]')];
  const el = els.find((e) => {
    const t = (e.textContent || '').trim();
    return t.length > 0 && t.length < 40 && e.querySelectorAll('*').length < 12 && e.offsetParent !== null && /[^\s]/.test(t);
  });
  return el ? el.textContent.trim().slice(0, 40) : null;
});
if (firstSession) {
  const opened = await page.evaluate(() => {
    const els = [...document.querySelectorAll('[class*="session"], [class*="conv"], [class*="chat-item"], [class*="chat_item"], li, [role="button"]')];
    const el = els.find((e) => {
      const t = (e.textContent || '').trim();
      return t.length > 0 && t.length < 40 && e.querySelectorAll('*').length < 12 && e.offsetParent !== null;
    });
    if (el) el.click();
    return true;
  });
  await page.waitForTimeout(2500);
  const snap = await snapshot('chat_open');
  const bubbles = await page.evaluate(() => {
    const text = document.body.innerText;
    const imgs = [...document.querySelectorAll('img')];
    return {
      msgCount: (text.match(/\d{1,2}:\d{2}/g) || []).length,
      imgs: imgs.length,
      imgsOk: imgs.filter((i) => i.complete && i.naturalWidth > 0).length,
    };
  });
  add('聊天-会话详情', opened && bubbles.msgCount > 0 ? 'PASS' : 'WARN',
    `样本: ${firstSession} · 时间戳消息 ${bubbles.msgCount} · 图片 ${bubbles.imgsOk}/${bubbles.imgs}`);
} else {
  add('聊天-会话详情', 'WARN', '未找到会话列表项');
}

// ── 关系图谱深度检查 ──
currentTab = '关系图谱-画布';
await clickText('关系图谱');
await page.waitForTimeout(5000);
const graph = await page.evaluate(() => {
  const canvas = [...document.querySelectorAll('canvas')].filter((c) => c.width > 200 && c.height > 200);
  const text = document.body.innerText;
  const nodeMatch = text.match(/(\d+)\s*个?节点/);
  return { canvases: canvas.length, canvasSizes: canvas.map((c) => `${c.width}x${c.height}`), nodeMatch: nodeMatch ? nodeMatch[1] : null };
});
add('关系图谱-画布', graph.canvases > 0 ? 'PASS' : 'WARN',
  `canvas ${graph.canvases} 个（${graph.canvasSizes.join(', ')}）· 节点文本 ${graph.nodeMatch ?? '无'}`);
await snapshot('graph_view');

// ── 汇总 ──
console.log('\n===== 问题清单 =====');
for (const i of issues.slice(0, 60)) console.log(i);
console.log(`\n问题总数: ${issues.length}`);

const summary = {
  generatedAt: new Date().toISOString(),
  report,
  issueCount: issues.length,
  issues: issues.slice(0, 100),
  screenshots: SHOT_DIR,
};
fs.writeFileSync(path.resolve('E:/ST/st_control/scripts/ui_audit_report.json'), JSON.stringify(summary, null, 2), 'utf-8');
await browser.close();
