// ============================================================
// 验证应用内 get_ilink_origin_status 返回 enabled=true
// 运行：node st_control/.codex_tests/probe_origin_status.mjs
// ============================================================

const CDP_BASE = 'http://127.0.0.1:9222';

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function findTarget() {
  for (let i = 0; i < 30; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
      if (t) return t;
    } catch {
      /* 应用尚未就绪 */
    }
    await sleep(1000);
  }
  throw new Error('30 秒内未发现 CDP 页面目标');
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
    const r = await this.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (r.exceptionDetails) {
      throw new Error('evaluate 异常: ' + JSON.stringify(r.exceptionDetails));
    }
    return r.result.value;
  }
}

const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});
const cdp = new Cdp(ws);

const expr = `(async () => {
  try {
    const status = await window.__TAURI_INTERNALS__.invoke('get_ilink_origin_status');
    return JSON.stringify(status);
  } catch (e) {
    return 'ERROR: ' + String(e);
  }
})()`;
const result = await cdp.eval(expr);
console.log('ILINK_ORIGIN_STATUS=' + result);
const parsed = JSON.parse(result);
const ok = parsed && parsed.enabled === true;
console.log(ok ? 'PASS: enabled=true' : 'FAIL: enabled !== true');
ws.close();
process.exit(ok ? 0 : 1);
