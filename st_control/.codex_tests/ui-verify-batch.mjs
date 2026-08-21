// 改动面板复检截图
import { writeFileSync, mkdirSync } from 'node:fs';
const CDP_BASE = 'http://127.0.0.1:9222';
const OUT = 'E:/ST/st_control/data/ui-audit';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const STEPS = [['大模型', '流量与成本'], ['自动化', '概览'], ['智能体', '已接入 Agent'], ['数据看板', null], ['AI 角色', null], ['首页', null]];

const list = await (await fetch(`${CDP_BASE}/json/list`)).json();
const t = list.find((x) => x.type === 'page');
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0; const pending = new Map();
ws.onmessage = (ev) => { const m = JSON.parse(ev.data); if (m.id && pending.has(m.id)) { const h = pending.get(m.id); pending.delete(m.id); if (m.error) h.reject(new Error(JSON.stringify(m.error))); else h.resolve(m.result); } };
const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; pending.set(i, { resolve: res, reject: rej }); ws.send(JSON.stringify({ id: i, method, params })); });
await send('Runtime.enable');
async function evalp(expression) {
  for (let a = 0; a < 4; a++) {
    const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (r && r.result && r.result.value !== undefined) return r.result.value;
    await sleep(700);
  }
  return undefined;
}
mkdirSync(OUT, { recursive: true });
for (const [tab, sub] of STEPS) {
  let ok = false;
  for (let a = 0; a < 4 && !ok; a++) {
    ok = (await evalp(`(() => { const b = document.querySelector('.nav-item[title="${tab}"]'); if (!b) return false; b.click(); return true; })()`)) === true;
    if (!ok) await sleep(800);
  }
  await sleep(1500);
  if (sub) {
    await evalp(`(() => { const btns = [...document.querySelectorAll('button')].filter(b => b.offsetParent !== null); const norm = (t) => (t || '').replace(/\\s+/g,' ').trim(); const b = btns.find(x => norm(x.textContent).includes('${sub}')); if (!b) return false; b.click(); return true; })()`);
    await sleep(1400);
  }
  const shot = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(`${OUT}/复检-${tab}${sub ? '-' + sub : ''}.png`, Buffer.from(shot.data, 'base64'));
  console.log('✓', tab, sub || '');
}
process.exit(0);
