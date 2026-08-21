// 已停用（2026-08）：原「AI 聊天」板块已并入 Harness 会话，本脚本断言的独立面板 UI 不再存在；
// 功能验证改由 e2e-harness-phase* 与 verify-harness-chat-* 承担。
// 验证工具面板位置：内嵌在对话流（AI 思考位置，紧跟最后一条消息）
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
  async waitFor(expression, timeoutMs = 120000, stepMs = 700) {
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

// 确保插件存在（上一轮可能被清理）；记录是否由本探针创建，结束时清理
let createdCalcId = null;
await cdp.eval(`(async () => {
  const list = await window.__TAURI_INTERNALS__.invoke('list_agent_plugins');
  const has = list.some((p) => (p.tools ?? []).some((t) => t.name === 'calculator'));
  if (!has) {
    const saved = await window.__TAURI_INTERNALS__.invoke('save_agent_plugin', { plugin: {
      id: '', name: '计算器插件', description: '演示', enabled: true,
      tools: [{ name: 'calculator', description: '计算表达式', parameters: { type: 'object', properties: { expression: { type: 'string' } }, required: ['expression'] }, requires_approval: false, code: 'return String(eval(args.expression));' }],
      versions: [], created_at: '', updated_at: '',
    }});
    window.__createdCalcId = saved.id;
  }
})()`);
createdCalcId = await cdp.eval(`window.__createdCalcId ?? null`);

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
    el.value = val; el.dispatchEvent(new Event('change', { bubbles: true })); return true;
  };
  setSelect([...document.querySelectorAll('.llm-chat-toolbar select')][0], chatP.id);
  await new Promise((r) => setTimeout(r, 700));
  const sels = [...document.querySelectorAll('.llm-chat-toolbar select')];
  setSelect(sels[0], chatP.id); setSelect(sels[1], 'deepseek-v4-flash');
  await new Promise((r) => setTimeout(r, 800));
})()`);
await sleep(1000);
await cdp.eval(`(() => { const b = document.querySelector('.llm-agent-toggle'); if (b && !b.classList.contains('on')) b.click(); })()`);
await cdp.waitFor(`(() => document.querySelector('.llm-agent-toggle')?.classList.contains('on') ? 'true' : 'false')()`, 10000);
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.llm-chat-toolbar button')].find((el) => el.textContent.includes('清空对话'));
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
await sleep(800);

await cdp.eval(`(() => {
  const ta = document.querySelector('.llm-chat-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, '请务必实际调用 calculator 工具计算 12*34。');
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'true';
})()`);
await sleep(400);
await cdp.eval(`(() => { const b = document.querySelector('.llm-send-btn'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);

const panel = await cdp.waitFor(`(() => document.querySelector('.llm-agent-panel') ? 'true' : 'false')()`, 90000);
check(panel === 'true', '工具面板出现');

const pos = await cdp.eval(`JSON.stringify((() => {
  const p = document.querySelector('.llm-agent-panel');
  if (!p) return { err: 'no panel' };
  const col = document.querySelector('.llm-chat-col');
  const msgs = [...document.querySelectorAll('.llm-chat-col > .llm-msg')];
  const last = msgs[msgs.length - 1];
  const prev = msgs.length > 1 ? msgs[msgs.length - 2] : null;
  const cr = p.getBoundingClientRect();
  const lr = last ? last.getBoundingClientRect() : null;
  const pr = prev ? prev.getBoundingClientRect() : null;
  return {
    insideCol: !!col && col.contains(p),
    lastIsAssistant: last ? last.classList.contains('llm-msg-bot') : false,
    prevIsUser: prev ? prev.classList.contains('llm-msg-user') : false,
    beforeLastMessage: lr ? cr.bottom <= lr.top + 2 : false,
    afterPrevMessage: pr ? cr.top >= pr.bottom - 2 : true,
    align: Math.round(cr.left - (col ? col.getBoundingClientRect().left : 0)),
  };
})())`);
console.log('POS=' + pos);
const r = JSON.parse(pos);
check(r.insideCol, '面板内嵌在对话流（.llm-chat-col 内）');
check(r.lastIsAssistant, '面板位于 AI 回复之前');
check(r.prevIsUser, '面板位于用户消息之后');
check(r.beforeLastMessage && r.afterPrevMessage, `面板位于用户消息与 AI 回复之间（思考位置，回复的前面）`);
// 对齐值 = 列内边距 20 + 缩进 42 = 62（相对列 border-box）
check(r.align === 62, `左缩进对齐 AI 正文（${r.align}px = 列内边距20 + 缩进42）`);

// 等回复完成截图
await cdp.waitFor(`(() => {
  const msgs = document.querySelectorAll('.llm-msg-bot .llm-msg-bubble');
  const last = msgs[msgs.length - 1];
  return last && !document.querySelector('.llm-agent-running') && last.textContent.trim().includes('408') ? 'true' : 'false';
})()`, 120000);
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve('E:/ST/st_control/data/ui-audit/llm-agent-inline.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
// 清理本探针创建的插件，避免残留重复工具名
if (createdCalcId) {
  try {
    await cdp.eval(`window.__TAURI_INTERNALS__.invoke('delete_agent_plugin', { id: ${JSON.stringify(createdCalcId)} })`);
  } catch {}
}
ws.close();
process.exit(failures === 0 ? 0 : 1);
