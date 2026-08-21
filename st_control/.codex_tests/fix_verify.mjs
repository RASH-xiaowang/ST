// 修复后复检：重新截图关键 tab + 视觉模型验证
import { chromium } from 'playwright-core';
import fs from 'node:fs';
import path from 'node:path';

const KEY = 'sk-mxdftdttxxzldbzxmkphmlifcnsnkpuzesnahvsoxhgnqxvm';
const OUT = 'E:/ST/.codex_shots/wechat_ui/fix';
fs.mkdirSync(OUT, { recursive: true });

const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
const ctx = browser.contexts()[0];
const page = ctx.pages().find((p) => p.url().includes('localhost:1420')) ?? ctx.pages()[0];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function clickTab(label) {
  return page.evaluate((label) => {
    const el = [...document.querySelectorAll('button.wc-ihb, .wc-ihb')].find((e) => (e.textContent || '').trim() === label && e.getClientRects().length > 0);
    if (!el) return false;
    el.click();
    return true;
  }, label);
}

const tabs = [['数据总览', 'f01-overview'], ['朋友圈', 'f02-moments'], ['文件', 'f03-files'], ['撤回记录', 'f04-revoked']];
for (const [label, name] of tabs) {
  await clickTab(label);
  await sleep(2200);
  const buf = await page.screenshot();
  fs.writeFileSync(path.join(OUT, name + '.png'), buf);
  console.log('shot', name);
}

// 复检
const question = '你是严格的 UI 审查员。看这张界面截图，只列出"确定存在"的布局问题（错位/截断/贴边/间距异常），每条一句。若基本协调，回答"协调"。';
const files = fs.readdirSync(OUT).filter((f) => f.endsWith('.png')).sort();
const results = [];
let i = 0;
async function worker() {
  while (i < files.length) {
    const f = files[i++];
    const img = fs.readFileSync(path.join(OUT, f));
    const b64 = img.toString('base64');
    const resp = await fetch('https://api.siliconflow.cn/v1/chat/completions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + KEY },
      body: JSON.stringify({
        model: 'Qwen/Qwen3-VL-32B-Instruct', max_tokens: 500,
        messages: [{ role: 'user', content: [
          { type: 'image_url', image_url: { url: 'data:image/png;base64,' + b64 } },
          { type: 'text', text: question },
        ]}],
      }),
    });
    const j = await resp.json();
    results.push({ f, v: j?.choices?.[0]?.message?.content ?? '(空)' });
    console.log('REVIEW', f);
  }
}
await Promise.all([worker(), worker()]);
for (const r of results) {
  console.log('\n===== ' + r.f + ' =====\n' + r.v);
}
await browser.close();
