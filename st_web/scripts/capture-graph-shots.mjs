// 截取 ST 控制台「微信社交关系图谱」真实界面（群友圈子 + 群聊网络两种模式）
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
const clickByText = async (text) => {
  const r = await ev(`(() => {
    const btns = [...document.querySelectorAll('button')].filter((b) => b.offsetParent !== null);
    const b = btns.find((x) => (x.textContent || '').trim().includes(${JSON.stringify(text)}));
    if (b) { b.click(); return 'true'; }
    return 'false';
  })()`);
  await sleep(1600);
  return r === 'true';
};
const bodyText = () => ev(`(() => document.body.innerText || '')()`);

// ── 微信数据 → 关系图谱 ──
await clickNav('微信数据');
let wxReady = false;
for (let i = 0; i < 40; i++) {
  const txt = await bodyText();
  if (txt.includes('朋友圈') || txt.includes('撤回') || txt.includes('存储空间')) { wxReady = true; break; }
  await sleep(1000);
}
if (!wxReady) { console.log('WECHAT NOT READY'); process.exit(1); }
await sleep(1500);

// 进入关系图谱页签
await clickByText('关系图谱');
// 等待图谱构建完成：标题 + 统计（连线数）出现（缓存命中时秒级，全量扫描时最多等 240s）
let graphReady = false;
for (let i = 0; i < 240; i++) {
  const r = await ev(`(() => {
    const txt = document.body.innerText || '';
    return (txt.includes('社交关系图谱') && (txt.includes('连线') || txt.includes('圈子'))) ? 'true' : 'false';
  })()`);
  if (r === 'true') { graphReady = true; break; }
  await sleep(1000);
}
if (!graphReady) { console.log('GRAPH NOT READY'); await shot('wechat-graph-timeout'); process.exit(1); }
// 等力导向布局稳定几秒
await sleep(6000);
await shot('wechat-graph');

// 切换到「群聊网络」模式
await clickByText('群聊网络');
await sleep(6000);
await shot('wechat-graph-groups');

// 回到 Harness，保持应用状态干净
await clickNav('Harness');
console.log('DONE');
process.exit(0);
