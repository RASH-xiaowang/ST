// 通过 Vite 动态导入前端服务，收集 chatStream 原始 delta
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
    if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
    return r.result.value;
  }
}
const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
const cdp = new Cdp(ws);
const out = await cdp.eval(`(async () => {
  try {
    const mod = await import('/@fs/E:/ST/st_control/src/lib/llm/services/ipc.ts');
    const cfg = await mod.llmApi.getConfig();
    const p = (cfg.providers ?? []).find((x) => (x.models ?? []).includes('deepseek-v4-flash'));
    if (!p) return 'ERROR: provider not found';
    const deltas = [];
    let done = null;
    await mod.llmApi.chatStream({
      provider_id: p.id,
      model: 'deepseek-v4-flash',
      role_id: null,
      messages: [{ role: 'user', content: '你好' }],
      max_tokens: null,
      temperature: 0.7,
      top_p: null,
      presence_penalty: null,
      frequency_penalty: null,
    }, (chunk) => {
      if (chunk.type === 'delta') deltas.push(chunk.content);
      else if (chunk.type === 'done') done = chunk;
    });
    const all = deltas.join('');
    return JSON.stringify({ deltaCount: deltas.length, deltaHead: all.slice(0, 150), deltaLen: all.length, doneHead: (done?.content ?? '').slice(0, 150) });
  } catch (e) {
    return 'ERROR: ' + String(e);
  }
})()`);
console.log('STREAM=' + out);
ws.close();
process.exit(0);
