// 验证表格渲染 + 滚动到表格消息截图
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
  throw new Error('no target');
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
        m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result);
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
    if (r.exceptionDetails) throw new Error('evaluate 异常: ' + JSON.stringify(r.exceptionDetails));
    return r.result.value;
  }
}
const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
const cdp = new Cdp(ws);

const dom = await cdp.eval(`JSON.stringify({
  tables: document.querySelectorAll('.llm-md-table table').length,
  thCount: document.querySelectorAll('.llm-md-table th').length,
  firstTh: document.querySelector('.llm-md-table th')?.textContent?.trim() ?? '',
  quotes: document.querySelectorAll('.llm-md blockquote').length,
  hrs: document.querySelectorAll('.llm-md hr').length,
  overflow: (() => { const t = document.querySelector('.llm-md-table'); return t ? t.scrollWidth + '/' + t.clientWidth : 'none'; })(),
})`);
console.log('DOM=' + dom);

// 滚动到表格消息并截图
await cdp.eval(`(() => {
  const t = document.querySelector('.llm-md-table');
  if (t) t.scrollIntoView({ block: 'center' });
  return 'true';
})()`);
await sleep(600);
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve('E:/ST/st_control/data/ui-audit/llm-table.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);
ws.close();
process.exit(0);
