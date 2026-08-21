// 截取 ST 控制台全部功能模块界面（补齐官网「功能模块」矩阵用）
import { writeFileSync, mkdirSync } from 'node:fs';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const list = await (await fetch('http://127.0.0.1:9222/json/list')).json();
const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
if (!t) { console.log('NO TARGET'); process.exit(1); }
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0; const pend = new Map();
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pend.has(m.id)) { const { resolve, reject } = pend.get(m.id); pend.delete(m.id); m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result); }
};
const send = (method, params = {}) => new Promise((resolve, reject) => {
  pend.set(++id, { resolve, reject });
  ws.send(JSON.stringify({ id, method, params }));
});
const ev = (expression) => send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
  .then((r) => { if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails)); return r.result?.value; });
const OUT = 'E:/ST/st_web/public/screenshots';
mkdirSync(OUT, { recursive: true });
const shot = async (name) => {
  const s = await send('Page.captureScreenshot', { format: 'png', fromSurface: true });
  writeFileSync(`${OUT}/${name}.png`, Buffer.from(s.data, 'base64'));
  console.log('SHOT=' + name);
};
const clickNav = async (title) => {
  await ev(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((x) => x.title === ${JSON.stringify(title)}); if (b) { b.click(); return 'true'; } return 'false'; })()`);
  await sleep(2600);
};

// ── 首页工作台（PlatformOverview 落地页）──
await clickNav('首页');
await sleep(3500);
await shot('home-overview');

// ── AI 工作台 ──
await clickNav('AI 文案'); await sleep(2800); await shot('ai-copy');
await clickNav('智能体'); await sleep(3000); await shot('agents');
await clickNav('AI 角色'); await sleep(2800); await shot('ai-roles');
await clickNav('大模型'); await sleep(3000); await shot('llm');

// ── 自动化 ──
await clickNav('自动化'); await sleep(3000); await shot('automation');
await clickNav('消息通道'); await sleep(3000); await shot('bot-channels');

// ── 数据与识别 ──
await clickNav('数据看板'); await sleep(3000); await shot('data-dashboard');
await clickNav('数据库'); await sleep(3000); await shot('db-manager');
await clickNav('图文识别'); await sleep(3000); await shot('ocr');

// ── 设置弹窗 ──
await ev(`(() => { const b = [...document.querySelectorAll('button.footer-action')].find((x) => (x.textContent||'').includes('设置')); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(2000);
await shot('settings');

// 回首页保持状态干净
await clickNav('首页');
console.log('DONE');
process.exit(0);
