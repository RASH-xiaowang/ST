// 测量 .llm-chat 布局：各子元素 rect 与容器关系，确认工具栏是否被裁掉
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
const out = await cdp.eval(`JSON.stringify((() => {
  const chat = document.querySelector('.llm-chat');
  if (!chat) return { err: 'no .llm-chat' };
  const cr = chat.getBoundingClientRect();
  const cs = getComputedStyle(chat);
  const info = (el) => {
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { cls: el.className.split(' ')[0], top: Math.round(r.top), bottom: Math.round(r.bottom), h: Math.round(r.height), inView: r.bottom <= cr.bottom + 1 && r.top >= cr.top - 1 };
  };
  return {
    chat: { top: Math.round(cr.top), bottom: Math.round(cr.bottom), h: Math.round(cr.height), overflow: cs.overflow, overflowY: cs.overflowY },
    children: [...chat.children].map(info),
    scrollH: chat.scrollHeight, clientH: chat.clientHeight,
  };
})())`);
console.log('LAYOUT=' + out);
ws.close();
process.exit(0);
