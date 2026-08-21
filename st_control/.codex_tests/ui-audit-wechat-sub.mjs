// 微信数据面板内部子页签截图（输出到 data/ui-audit/）
import { writeFileSync, mkdirSync } from 'node:fs';

const CDP_BASE = 'http://127.0.0.1:9222';
const OUT_DIR = 'E:/ST/st_control/data/ui-audit';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SUBS = ['朋友圈', '存储空间', '撤回记录', '通讯录', '收藏', '表情', '文件', '概览'];

const list = await (await fetch(`${CDP_BASE}/json/list`)).json();
const t = list.find((x) => x.type === 'page');
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0;
const pending = new Map();
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) {
    const { resolve, reject } = pending.get(m.id);
    pending.delete(m.id);
    if (m.error) reject(new Error(JSON.stringify(m.error)));
    else resolve(m.result);
  }
};
const send = (method, params = {}) => new Promise((res, rej) => {
  const i = ++id;
  pending.set(i, { resolve: res, reject: rej });
  ws.send(JSON.stringify({ id: i, method, params }));
});
await send('Runtime.enable');
async function evalp(expression) {
  for (let a = 0; a < 4; a++) {
    const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (r && r.result && r.result.value !== undefined) return r.result.value;
    await sleep(700);
  }
  return undefined;
}

mkdirSync(OUT_DIR, { recursive: true });

// 进入微信数据
let ok = false;
for (let a = 0; a < 5 && !ok; a++) {
  ok = (await evalp(`(() => { const b = document.querySelector('.nav-item[title="微信数据"]'); if (!b) return false; b.click(); return true; })()`)) === true;
  if (!ok) await sleep(900);
}
await sleep(2200);

for (const sub of SUBS) {
  const clicked = await evalp(`(() => {
    const btns = [...document.querySelectorAll('button')].filter(b => b.offsetParent !== null);
    const norm = (t) => (t || '').replace(/\\s+/g, ' ').trim();
    const b = btns.find(x => norm(x.textContent) === '${sub}');
    if (!b) return false;
    b.click();
    return true;
  })()`);
  await sleep(1600);
  const shot = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(`${OUT_DIR}/微信数据-${sub}.png`, Buffer.from(shot.data, 'base64'));
  console.log(`✓ 微信数据-${sub}（命中: ${clicked}）`);
}
console.log('完成');
process.exit(0);
