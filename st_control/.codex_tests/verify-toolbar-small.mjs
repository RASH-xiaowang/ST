// 小窗口高度下复检：工具栏必须仍可见（消息区内部滚动）
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
let failures = 0;
const check = (ok, msg) => {
  console.log((ok ? 'PASS: ' : 'FAIL: ') + msg);
  if (!ok) failures++;
};

// 模拟小窗口（1280x680 CSS）
await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 680, deviceScaleFactor: 1, mobile: false });
await sleep(800);
const out = await cdp.eval(`JSON.stringify((() => {
  const chat = document.querySelector('.llm-chat');
  const win = document.querySelector('.llm-chat-window');
  const input = document.querySelector('.llm-chat-input');
  const tb = document.querySelector('.llm-chat-toolbar');
  if (!chat || !tb) return { err: 'missing' };
  const cr = chat.getBoundingClientRect();
  const tr = tb.getBoundingClientRect();
  const wr = win.getBoundingClientRect();
  return {
    vh: window.innerHeight,
    chatBottom: Math.round(cr.bottom),
    tbTop: Math.round(tr.top), tbBottom: Math.round(tr.bottom),
    tbVisible: tr.top >= cr.top && tr.bottom <= Math.min(cr.bottom, window.innerHeight) + 1,
    winH: Math.round(wr.height),
    winScrollable: win.scrollHeight >= win.clientHeight,
  };
})())`);
console.log('SMALL=' + out);
const r = JSON.parse(out);
check(r.tbVisible, `小窗口下工具栏仍完整可见（tb ${r.tbTop}-${r.tbBottom}，窗口底 ${r.chatBottom}）`);
check(r.winH < r.vh, `消息区收缩为内部滚动（高度 ${r.winH} < 视口 ${r.vh}）`);

// 恢复窗口尺寸
await cdp.send('Emulation.clearDeviceMetricsOverride');
await sleep(600);
console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
