// 从运行中的 ST 控制台（CDP 9222）截取真实界面截图
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

// 1) Harness 会话界面（制造一次带工具时间线的对话展示真实产品）
await ev(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((x) => x.title === 'Harness'); if (b) b.click(); return 'true'; })()`);
await sleep(2500);
const ready = await ev(`(async () => {
  for (let i = 0; i < 30; i++) {
    if (document.querySelector('.hns-input textarea')) return 'true';
    await new Promise((r) => setTimeout(r, 500));
  }
  return 'false';
})()`);
if (ready === 'true') {
  // 新会话
  await ev(`(() => { const b = document.querySelector('.hns-new'); if (b) b.click(); return 'true'; })()`);
  await sleep(600);
  // 发一条触发工具时间线的消息
  await ev(`(() => {
    const ta = document.querySelector('.hns-input textarea');
    const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
    setter.call(ta, '请调用 get_current_time 工具获取当前时间，然后简要告诉我现在几点，用一句话回答');
    ta.dispatchEvent(new Event('input', { bubbles: true }));
    return 'typed';
  })()`);
  await sleep(300);
  await ev(`(() => { const ta = document.querySelector('.hns-input textarea'); ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); return 'sent'; })()`);
  // 等待完成
  for (let i = 0; i < 90; i++) {
    await sleep(1000);
    const txt = await ev(`(() => [...document.querySelectorAll('.hns-msg-bot')].map((x) => x.textContent).join('\\n'))()`);
    if (String(txt).includes('现在') || String(txt).includes('时间') || String(txt).includes('Current')) break;
  }
  await sleep(1200);
  await shot('harness-session');
}

// 2) 治理中心
await ev(`(() => { const b = [...document.querySelectorAll('.hns-tools-btn')].find((x) => (x.textContent || '').includes('治理')); if (b) b.click(); return 'true'; })()`);
await sleep(900);
await shot('harness-governance');
// 治理中心 → 预设 tab
await ev(`(() => { const b = [...document.querySelectorAll('.hns-drawer-tabs button')].find((x) => (x.textContent || '').trim() === '预设'); if (b) b.click(); return 'true'; })()`);
await sleep(600);
await shot('harness-presets');
// 关闭治理
await ev(`(() => { const b = document.querySelector('.hns-drawer-close'); if (b) b.click(); return 'true'; })()`);
await sleep(400);

// 3) 工具目录
await ev(`(() => { const b = [...document.querySelectorAll('.hns-tools-btn')].find((x) => (x.textContent || '').includes('工具')); if (b) b.click(); return 'true'; })()`);
await sleep(700);
// 展开第一个工具的 schema
await ev(`(() => { const b = document.querySelector('.hns-tool-main'); if (b) b.click(); return 'true'; })()`);
await sleep(500);
await shot('harness-tools');
await ev(`(() => { const b = document.querySelector('.hns-tools-btn.on'); if (b) b.click(); return 'true'; })()`);
await sleep(300);

// 4) 大模型（模型管理界面）
await ev(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((x) => x.title === '大模型'); if (b) b.click(); return 'true'; })()`);
await sleep(2000);
await shot('llm-config');
process.exit(0);
