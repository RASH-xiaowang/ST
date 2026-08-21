// 截图：Harness 重设计（治理中心 + 工具目录 + 会话主界面）
import { writeFileSync, mkdirSync } from 'node:fs';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const list = await (await fetch('http://127.0.0.1:9222/json/list')).json();
const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
if (!t) { console.log('NO PAGE'); process.exit(1); }
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
const OUT = 'E:/ST/st_control/data/ui-audit/redesign';
mkdirSync(OUT, { recursive: true });
const shot = async (name) => {
  const s = await send('Page.captureScreenshot', { format: 'png', fromSurface: true });
  const f = `${OUT}/${name}.png`;
  writeFileSync(f, Buffer.from(s.data, 'base64'));
  console.log('SAVED=' + f);
};

// 进入 Harness
await ev(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((x) => x.title === 'Harness'); if (b) b.click(); return 'true'; })()`);
await sleep(2500);
await shot('01-main');

// 工具目录
await ev(`(() => { const b = [...document.querySelectorAll('.hns-tools-btn')].find((x) => (x.textContent || '').includes('工具')); if (b) b.click(); return 'true'; })()`);
await sleep(800);
await shot('02-tools');
// 展开一个 schema
await ev(`(() => { const b = document.querySelector('.hns-tool-main'); if (b) b.click(); return 'true'; })()`);
await sleep(400);
await shot('03-tools-schema');

// 治理中心
await ev(`(() => { const b = document.querySelector('.hns-drawer-close'); if (b) b.click(); return 'true'; })()`);
await sleep(400);
await ev(`(() => { const b = [...document.querySelectorAll('.hns-tools-btn')].find((x) => (x.textContent || '').includes('治理')); if (b) b.click(); return 'true'; })()`);
await sleep(800);
await shot('04-governance');
// 切到预设 tab
await ev(`(() => { const b = [...document.querySelectorAll('.hns-drawer-tabs button')].find((x) => (x.textContent || '').trim() === '预设'); if (b) b.click(); return 'true'; })()`);
await sleep(600);
await shot('05-governance-presets');
// 切到 MCP tab
await ev(`(() => { const b = [...document.querySelectorAll('.hns-drawer-tabs button')].find((x) => (x.textContent || '').trim() === 'MCP'); if (b) b.click(); return 'true'; })()`);
await sleep(600);
await shot('06-governance-mcp');
// 关闭
await ev(`(() => { const b = document.querySelector('.hns-drawer-close'); if (b) b.click(); return 'true'; })()`);
process.exit(0);
