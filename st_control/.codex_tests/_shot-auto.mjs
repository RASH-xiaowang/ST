// 自动化面板各视图截图
import fs from 'node:fs';
const CDP_BASE = 'http://127.0.0.1:9222';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const OUT = 'C:/Users/28361/Desktop/ST/.codex_shots/automation';
fs.mkdirSync(OUT, { recursive: true });
async function findTarget() {
  for (let i = 0; i < 40; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
      if (t) return t;
    } catch {}
    await sleep(500);
  }
  throw new Error('no cdp target');
}
const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0;
const pending = new Map();
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) {
    const { resolve, reject } = pending.get(m.id);
    pending.delete(m.id);
    m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result);
  }
};
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const i = ++id;
  pending.set(i, { resolve, reject });
  ws.send(JSON.stringify({ id: i, method, params }));
});
const evalJs = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error('evaluate 异常: ' + JSON.stringify(r.exceptionDetails));
  return r.result.value;
};
const clickSidebar = (label) => evalJs(`(() => {
  const els = Array.from(document.querySelectorAll('button, [role="tab"], [role="button"], a'));
  const exact = els.filter(el => (el.textContent || '').trim() === ${JSON.stringify(label)} && el.getBoundingClientRect().width > 0 && el.getBoundingClientRect().x < 260);
  const t = exact.find(el => el.tagName === 'BUTTON' || el.getAttribute('role') === 'tab') || exact[0];
  if (t) { t.click(); return true; } return false;
})()`);
const clickTab = (label) => evalJs(`(() => {
  const tr = Array.from(document.querySelectorAll('button[role="tab"]'));
  const t = tr.find(b => b.textContent.replace(/\\s+/g,'').includes(${JSON.stringify(label)}));
  if (t) { t.click(); return true; } return false;
})()`);
await clickSidebar('自动化');
await sleep(1800);
const shots = [['overview', null], ['rules', '规则管理'], ['tasks', '消息与任务'], ['robot', '回复机器人'], ['channels', '消息通道']];
for (const [name, tab] of shots) {
  if (tab) { await clickTab(tab); await sleep(1400); }
  const r = await send('Page.captureScreenshot', { format: 'png', fromSurface: true, captureBeyondViewport: false });
  fs.writeFileSync(`${OUT}/${name}.png`, Buffer.from(r.data, 'base64'));
  console.log('saved', `${OUT}/${name}.png`);
}
ws.close();
