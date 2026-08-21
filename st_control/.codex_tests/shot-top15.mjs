// 截取数据总览「朋友圈活跃 Top 15」面板 → data/ui-audit/top15-jump.png
const CDP_BASE = 'http://127.0.0.1:9222';
import fs from 'node:fs';
import path from 'node:path';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function findTarget() {
  for (let i = 0; i < 30; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
      if (t) return t;
    } catch {}
    await sleep(1000);
  }
  throw new Error('no CDP target');
}

class Cdp {
  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.pending = new Map();
    ws.onmessage = (ev) => {
      const m = JSON.parse(ev.data);
      if (m.id && this.pending.has(m.id)) {
        const { resolve, reject } = this.pending.get(m.id);
        this.pending.delete(m.id);
        if (m.error) reject(new Error(JSON.stringify(m.error)));
        else resolve(m.result);
      }
    };
  }
  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++this.id;
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }
  async eval(expression) {
    const r = await this.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error('eval: ' + JSON.stringify(r.exceptionDetails));
    return r.result.value;
  }
}

const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
const cdp = new Cdp(ws);

// 打开数据总览
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('button')].find((el) => el.textContent.trim() === '数据总览');
  if (b) { b.click(); return true; }
  return false;
})()`);
await sleep(2500);

// 滚动到作者面板并截图
await cdp.eval(`(() => {
  const sec = [...document.querySelectorAll('.ov-panel')].find((el) => el.textContent.includes('朋友圈活跃 Top 15'));
  if (sec) sec.scrollIntoView({ block: 'center' });
})()`);
await sleep(800);

const shot = await cdp.send('Page.captureScreenshot', { format: 'png' });
const out = path.resolve('E:/ST/st_control/data/ui-audit/top15-jump.png');
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);
ws.close();
