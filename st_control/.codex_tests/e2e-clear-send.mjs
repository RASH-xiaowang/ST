// 已停用（2026-08）：原「AI 聊天」板块已并入 Harness 会话，本脚本断言的独立面板 UI 不再存在；
// 功能验证改由 e2e-harness-phase* 与 verify-harness-chat-* 承担。
// 清空对话 → UI 发送「你好」→ 只检查这条新回复
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
  async waitFor(expression, timeoutMs = 90000, stepMs = 500) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        const v = await this.eval(expression);
        if (v && v !== 'false' && v !== 'null' && v !== 'undefined') return v;
      } catch {}
      await sleep(stepMs);
    }
    return null;
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

// 进入 AI 聊天 + 切对话模型
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('.hns-viewbar button')].find((el) => (el.textContent || '').trim() === 'AI 聊天' && el.offsetParent !== null) ?? (() => { const n = [...document.querySelectorAll('button.nav-item')].find((el) => el.title === 'Harness'); if (n) n.click(); return null; })();
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
await sleep(1500);
await cdp.eval(`(async () => {
  const cfg = await window.__TAURI_INTERNALS__.invoke('get_llm_config');
  const chatP = (cfg.providers ?? []).find((x) => (x.models ?? []).includes('deepseek-v4-flash'));
  if (!chatP) return;
  const setSelect = (el, val) => {
    if (!el || ![...el.options].some((o) => o.value === val)) return false;
    el.value = val;
    el.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  };
  setSelect([...document.querySelectorAll('.llm-chat-toolbar select')][0], chatP.id);
  await new Promise((r) => setTimeout(r, 700));
  const sels = [...document.querySelectorAll('.llm-chat-toolbar select')];
  setSelect(sels[0], chatP.id);
  setSelect(sels[1], 'deepseek-v4-flash');
  await new Promise((r) => setTimeout(r, 800));
})()`);
await sleep(1000);

// 清空对话
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.llm-chat-toolbar button')].find((el) => el.textContent.includes('清空对话'));
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
await sleep(800);

// 发送「你好」
await cdp.eval(`(() => {
  const ta = document.querySelector('.llm-chat-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, '你好');
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'true';
})()`);
await sleep(400);
const clicked = await cdp.eval(`(() => { const b = document.querySelector('.llm-send-btn'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
console.log('SENT=' + clicked);

const reply = await cdp.waitFor(`(() => {
  const msgs = document.querySelectorAll('.llm-msg-bot .llm-msg-bubble');
  const last = msgs[msgs.length - 1];
  if (!last) return 'false';
  const caret = !!document.querySelector('.llm-caret');
  const text = last.textContent.trim();
  return !caret && text.length > 0 ? text : 'false';
})()`, 90000);
console.log('REPLY=' + JSON.stringify(reply));
check(!!reply, '收到回复');
const leakPatterns = ['我们需要回应', '用户可能是在测试', '决定回应', '我们之前', '保持友好', '注意用户'];
const leaked = leakPatterns.filter((p) => (reply ?? '').includes(p));
check(leaked.length === 0, `回复不含思考过程（泄露: ${JSON.stringify(leaked)}）`);

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
