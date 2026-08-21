// 已停用（2026-08）：原「AI 聊天」板块已并入 Harness 会话，本脚本断言的独立面板 UI 不再存在；
// 功能验证改由 e2e-harness-phase* 与 verify-harness-chat-* 承担。
// E2E：代理模式（工具调用 + 审批流）
const CDP_BASE = 'http://127.0.0.1:9222';
import fs from 'node:fs';
import path from 'node:path';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function findTarget() {
  for (let i = 0; i < 40; i++) {
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

// 0) 清理 calculator 插件（避免模型被无关工具干扰，保证 web_search 被选用）
await cdp.eval(`(async () => {
  const list = await window.__TAURI_INTERNALS__.invoke('list_agent_plugins');
  for (const p of list) {
    if ((p.tools ?? []).some((t) => t.name === 'calculator')) {
      await window.__TAURI_INTERNALS__.invoke('delete_agent_plugin', { id: p.id });
    }
  }
})()`);

// 1) 工具目录
const toolsRaw = await cdp.eval(`(async () => {
  const t = await window.__TAURI_INTERNALS__.invoke('get_agent_tools');
  return JSON.stringify(t);
})()`);
const tools = JSON.parse(toolsRaw);
console.log('TOOLS=' + tools.map((t) => `${t.name}${t.requires_approval ? "🔒" : ""}`).join(','));
check(tools.some((t) => t.name === 'web_search'), '工具目录含 web_search');
check(tools.some((t) => t.name === 'exec_command' && t.requires_approval), 'exec_command 需审批');

// 1) 进入 AI 聊天（Harness → 「AI 聊天」子视图）+ 切对话模型 + 开代理模式
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.title === 'Harness');
  if (b) b.click();
  return 'true';
})()`, 20000);
await sleep(1000);
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('.hns-viewbar button')].find((el) => (el.textContent || '').trim() === 'AI 聊天');
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
await cdp.eval(`(() => {
  const b = document.querySelector('.llm-agent-toggle');
  if (b && !b.classList.contains('on')) b.click();
  return 'true';
})()`);
const toggled = await cdp.waitFor(`(() => document.querySelector('.llm-agent-toggle')?.classList.contains('on') ? 'true' : 'false')()`, 10000);
check(toggled === 'true', '代理模式已开启');

// 清空对话，保证从干净会话开始（历史气泡不干扰本轮探测）
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.llm-chat-toolbar button')].find((el) => el.textContent.includes('清空对话'));
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
await sleep(800);

// 2) 发送一个需要联网搜索的问题（指令式，要求必须调用工具）
const q = '请务必实际调用 web_search 工具（不要凭空回答）：搜索关键词「南宁天气」，然后把搜索到的结果转述给我。';
const botsBefore = await cdp.eval(`document.querySelectorAll('.llm-msg-bot').length`);
await cdp.eval(`(() => {
  const ta = document.querySelector('.llm-chat-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, ${JSON.stringify(q)});
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'true';
})()`);
await sleep(400);
await cdp.eval(`(() => { const b = document.querySelector('.llm-send-btn'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
console.log('SENT');

// 等待工具面板出现（仅看实时面板，排除历史面板的旧步骤）
const toolPanel = await cdp.waitFor(`(() => {
  const p = document.querySelector('.llm-agent-panel:not(.llm-agent-panel-history)');
  if (!p) return 'false';
  const steps = [...p.querySelectorAll('.llm-agent-step-name')].map((e) => e.textContent.trim());
  return steps.length ? JSON.stringify(steps) : 'false';
})()`, 90000);
console.log('STEPS=' + toolPanel);
check(!!toolPanel && toolPanel.includes('web_search'), `工具面板出现且含 web_search（${toolPanel}）`);

// 等待最终回复（本轮新增的助手气泡：消息数需多于发送前）
const reply = await cdp.waitFor(`(() => {
  const bots = document.querySelectorAll('.llm-msg-bot .llm-msg-bubble');
  if (bots.length <= ${botsBefore}) return 'false';
  const last = bots[bots.length - 1];
  const caret = !!document.querySelector('.llm-caret');
  const sending = !!document.querySelector('.llm-agent-running');
  const typing = !!document.querySelector('.llm-typing');
  const text = last.textContent.trim();
  return !caret && !sending && !typing && text.length > 5 ? text.slice(0, 200) : 'false';
})()`, 120000);
console.log('REPLY=' + JSON.stringify(reply));
check(!!reply, '收到代理最终回复');

// 截图
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve('E:/ST/st_control/data/ui-audit/llm-agent.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);

// 3) 审批流：清空对话后让模型执行命令（避免历史里的旧结果干扰）
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.llm-chat-toolbar button')].find((el) => el.textContent.includes('清空对话'));
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
await sleep(800);
const botsBefore2 = await cdp.eval(`document.querySelectorAll('.llm-msg-bot').length`);
const q2 = '请用 exec_command 工具执行命令：echo hello-agent';
await cdp.eval(`(() => {
  const ta = document.querySelector('.llm-chat-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, ${JSON.stringify(q2)});
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'true';
})()`);
await sleep(400);
const q2Sent = await cdp.eval(`(() => { const b = document.querySelector('.llm-send-btn'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
console.log('SENT_Q2=' + q2Sent);
check(q2Sent === 'true', '第二轮发送按钮可用并已点击');
const approval = await cdp.waitFor(`(() => {
  const a = document.querySelector('.llm-agent-approval');
  return a ? a.textContent.trim() : 'false';
})()`, 90000);
console.log('APPROVAL=' + approval);
check(!!approval && approval.includes('exec_command'), `审批卡片出现（${approval}）`);

// 点击批准（跳过「记住并批准」，取最后一个 approve 即普通「批准」）
await cdp.eval(`(() => {
  const btns = [...document.querySelectorAll('.llm-agent-approve')];
  const b = btns[btns.length - 1];
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
console.log('APPROVED');
const reply2 = await cdp.waitFor(`(() => {
  const bots = document.querySelectorAll('.llm-msg-bot .llm-msg-bubble');
  if (bots.length <= ${botsBefore2}) return 'false';
  const last = bots[bots.length - 1];
  if (!last) return 'false';
  const sending = !!document.querySelector('.llm-agent-running');
  const typing = !!document.querySelector('.llm-typing');
  const text = last.textContent.trim();
  return !sending && !typing && text.length > 3 ? text.slice(0, 200) : 'false';
})()`, 120000);
console.log('REPLY2=' + JSON.stringify(reply2));
check(!!reply2, '审批通过后收到回复');

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
