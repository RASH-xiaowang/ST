// 已停用（2026-08）：原「AI 聊天」板块已并入 Harness 会话，本脚本断言的独立面板 UI 不再存在；
// 功能验证改由 e2e-harness-phase* 与 verify-harness-chat-* 承担。
// 验证：发送「你好」→ 回复不应包含模型思考过程
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

// 进入 AI 聊天
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('.hns-viewbar button')].find((el) => (el.textContent || '').trim() === 'AI 聊天' && el.offsetParent !== null) ?? (() => { const n = [...document.querySelectorAll('button.nav-item')].find((el) => el.title === 'Harness'); if (n) n.click(); return null; })();
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
await sleep(1500);

// 切换到对话模型（deepseek-v4-flash；配置未就绪时重试）
const sw = await cdp.waitFor(`(async () => {
  try {
    const cfg = await window.__TAURI_INTERNALS__.invoke('get_llm_config');
    const ps = cfg?.providers ?? [];
    if (!ps.length) return 'false';
    const chatP = ps.find((x) => (x.models ?? []).includes('deepseek-v4-flash')) ?? ps[0];
    const chatM = (chatP.models ?? []).includes('deepseek-v4-flash') ? 'deepseek-v4-flash' : (chatP.models ?? [])[0];
    if (!chatM) return 'false';
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
    setSelect(sels[1], chatM);
    await new Promise((r) => setTimeout(r, 800));
    return 'true';
  } catch (e) {
    return 'false';
  }
})()`, 30000);
check(sw === 'true', '已切换到对话模型');
await sleep(1000);

// 发送「你好」
const beforeCount = await cdp.eval(`(() => document.querySelectorAll('.llm-msg-bot').length)()`);
console.log('BOT_BEFORE=' + beforeCount);
await cdp.eval(`(() => {
  const ta = document.querySelector('.llm-chat-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, '你好');
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'true';
})()`);
await sleep(400);
await cdp.eval(`(() => { const b = document.querySelector('.llm-send-btn'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
console.log('SENT');

// 等待新回复出现并完成（bot 消息数增加 + 无流式光标）
const reply = await cdp.waitFor(`(() => {
  const msgs = document.querySelectorAll('.llm-msg-bot .llm-msg-bubble');
  const last = msgs[msgs.length - 1];
  const caret = !!document.querySelector('.llm-caret');
  const countNow = document.querySelectorAll('.llm-msg-bot').length;
  if (countNow <= ${typeof beforeCount === 'number' ? beforeCount : 0}) return 'false';
  if (!last) return 'false';
  const text = last.textContent.trim();
  return !caret && text.length > 0 ? text : 'false';
})()`, 90000);
console.log('REPLY=' + JSON.stringify(reply));
check(!!reply, '收到回复');

// 思考过程泄露特征检测（推理内容不应出现在答案里）
const leakPatterns = [
  '我们需要理解当前状态',
  '之前用户多次发送',
  '用户可能是在测试',
  '考虑到之前的',
  '决定回应',
  '我们之前已经介绍过',
  '保持友好',
  '注意用户现在',
];
const leaked = leakPatterns.filter((p) => (reply ?? '').includes(p));
check(leaked.length === 0, `回复不含思考过程片段（泄露: ${JSON.stringify(leaked)}）`);
console.log('ANSWER_LEN=' + (reply ?? '').length);

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
