// 修正本地 STT 配置：指向新数据目录已有的 ggml-small.bin
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
  const st = await window.__TAURI_INTERNALS__.invoke('get_local_stt_status');
  const newPath = 'E:\\\\ST\\\\st_control\\\\data\\\\models\\\\ggml-small.bin';
  if (st.model_exists) return JSON.stringify({ skipped: true, st });
  const r = await window.__TAURI_INTERNALS__.invoke('set_local_stt_config', {
    config: { enabled: st.enabled, model_path: newPath, language: st.language, translate: false, model_size: 'small' },
  });
  return JSON.stringify({ updated: true, model_exists: r.model_exists, model_loaded: r.model_loaded, path: r.model_path });
})()`);
console.log('FIX=' + out);
ws.close();
process.exit(0);
