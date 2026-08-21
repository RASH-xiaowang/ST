// 截取 ST 控制台更多功能界面：微信数据（重点）+ 知识库 + 数据看板
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
// 按可见按钮文本点击（微信数据子界面）
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

// ── 微信数据（重点）──
await clickNav('微信数据');
// 等待启动页完成进入主界面
let wxReady = false;
for (let i = 0; i < 40; i++) {
  const has = await ev(`(() => {
    const txt = document.body.innerText || '';
    return (txt.includes('朋友圈') || txt.includes('撤回') || txt.includes('存储空间')) ? 'true' : 'false';
  })()`);
  if (has === 'true') { wxReady = true; break; }
  await sleep(1000);
}
if (wxReady) {
  await sleep(1500);
  await shot('wechat-home');
  // 朋友圈洞察
  if (await clickByText('朋友圈')) {
    await sleep(2000);
    await shot('wechat-moments');
  }
  // 撤回消息记录
  await clickNav('微信数据');
  await sleep(2500);
  if (await clickByText('撤回')) {
    await sleep(2000);
    await shot('wechat-recall');
  }
  // 存储空间分析
  await clickNav('微信数据');
  await sleep(2500);
  if (await clickByText('存储空间')) {
    await sleep(2000);
    await shot('wechat-storage');
  }
} else {
  console.log('WECHAT NOT READY');
  await shot('wechat-bootstrap');
}

// ── 知识库 ──
await clickNav('知识库');
await sleep(2500);
await shot('kb');

// ── 数据看板 ──
await clickNav('数据看板');
await sleep(3000);
await shot('dashboard');

// 回到 Harness，保持应用状态干净
await clickNav('Harness');
process.exit(0);
