// 复制按钮反馈复核（含剪贴板回退后）
const CDP_BASE = 'http://127.0.0.1:9222';
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
await sleep(1500); // 等 HMR 应用
// 点击最后一个复制按钮
await cdp.eval(`(() => {
  const bs = [...document.querySelectorAll('.llm-msg-act')].filter((b) => b.textContent.includes('复制'));
  if (bs.length) bs[bs.length - 1].click();
  return 'true';
})()`);
await sleep(600);
const out = await cdp.eval(`JSON.stringify({
  copied: [...document.querySelectorAll('.llm-msg-act')].some((b) => b.textContent.includes('已复制')),
  labels: [...document.querySelectorAll('.llm-msg-act')].map((b) => b.textContent.trim()),
})`);
console.log('COPY=' + out);
ws.close();
process.exit(JSON.parse(out).copied ? 0 : 1);
