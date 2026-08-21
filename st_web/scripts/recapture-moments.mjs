// 重截朋友圈洞察（等待列表加载完成）
import { writeFileSync } from 'node:fs';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const list = await (await fetch('http://127.0.0.1:9222/json/list')).json();
const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0; const pend = new Map();
ws.onmessage = (ev) => { const m = JSON.parse(ev.data); if (m.id && pend.has(m.id)) { const p = pend.get(m.id); pend.delete(m.id); m.error ? p.reject(new Error(JSON.stringify(m.error))) : p.resolve(m.result); } };
const send = (method, params = {}) => new Promise((resolve, reject) => { pend.set(++id, { resolve, reject }); ws.send(JSON.stringify({ id, method, params })); });
const ev = (expression) => send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }).then((r) => r.result?.value);
const clickNav = async (title) => {
  await ev(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((x) => x.title === ${JSON.stringify(title)}); if (b) b.click(); return 'true'; })()`);
  await sleep(2600);
};
await clickNav('微信数据');
await sleep(2000);
await ev(`(() => {
  const btns = [...document.querySelectorAll('button')].filter((b) => b.offsetParent !== null);
  const b = btns.find((x) => (x.textContent || '').trim().includes('朋友圈'));
  if (b) b.click();
  return 'true';
})()`);
// 等待列表加载完成（不再出现"加载中"）
for (let i = 0; i < 40; i++) {
  await sleep(1000);
  const still = await ev(`(() => (document.body.innerText || '').includes('加载中') ? 'true' : 'false')()`);
  if (still !== 'true') break;
}
await sleep(1500);
const s = await send('Page.captureScreenshot', { format: 'png', fromSurface: true });
writeFileSync('E:/ST/st_web/public/screenshots/wechat-moments.png', Buffer.from(s.data, 'base64'));
console.log('SHOT=wechat-moments (retry)');
process.exit(0);
