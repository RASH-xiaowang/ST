// 复核 Q2：会话显示名 + 最近消息内容
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
const raw = await cdp.eval(`(async () => {
  const r = await window.__TAURI_INTERNALS__.invoke('ask_wechat', { question: '我和东兰民中1410王勤最近聊了什么？', limit: 24, history: null });
  return JSON.stringify(r);
})()`);
const r = JSON.parse(raw);
console.log('answer:', (r.answer ?? '').slice(0, 200));
console.log('target:', r.plan?.target);
console.log('citation names:', [...new Set((r.citations ?? []).map((c) => c.name))]);
console.log('first snippets:', JSON.stringify((r.citations ?? []).slice(0, 2).map((c) => c.snippet.slice(0, 40))));
ws.close();
process.exit(0);
