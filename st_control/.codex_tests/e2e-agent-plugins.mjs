// 已停用（2026-08）：原「AI 聊天」板块已并入 Harness 会话，本脚本断言的独立面板 UI 不再存在；
// 功能验证改由 e2e-harness-phase* 与 verify-harness-chat-* 承担。
// E2E：动态插件系统（创建/运行/更新/停止/删除 + 代理调用插件工具）
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
const inv = (cmd, args) => cdp.eval(`(async () => {
  try { return JSON.stringify(await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args ?? {})})); }
  catch (e) { return JSON.stringify({ __err: String(e) }); }
})()`).then((s) => JSON.parse(s));

// 1) 创建插件（v1）
const created = await inv('save_agent_plugin', {
  plugin: {
    id: "",
    name: "计算器插件",
    description: "四则运算工具（插件演示）",
    enabled: true,
    tools: [{
      name: "calculator",
      description: "计算数学表达式，参数 expression 为字符串表达式",
      parameters: { type: "object", properties: { expression: { type: "string" } }, required: ["expression"] },
      requires_approval: false,
      code: "return String(eval(args.expression));",
    }],
    versions: [],
    created_at: "",
    updated_at: "",
  },
});
check(!!created.id && created.versions?.length === 1, `插件创建成功（id=${created.id} v${created.versions?.[0]?.version}）`);
const pluginId = created.id;

// 2) 更新插件（v2，工具描述变更）
const updated = await inv('save_agent_plugin', {
  plugin: {
    id: pluginId,
    name: "计算器插件",
    description: "四则运算工具（v2 描述）",
    enabled: true,
    tools: [{
      name: "calculator",
      description: "计算数学表达式（v2）",
      parameters: { type: "object", properties: { expression: { type: "string" } }, required: ["expression"] },
      requires_approval: false,
      code: "return String(eval(args.expression));",
    }],
    versions: [],
    created_at: "",
    updated_at: "",
  },
});
check(updated.versions?.length === 2 && updated.versions?.[1]?.version === 2, `更新生成新版本（versions=${updated.versions?.length}，最新 v${updated.versions?.[1]?.version}）`);

// 3) 工具目录含插件工具
const tools = await inv('get_agent_tools');
check(tools.some((t) => t.name === "calculator"), '工具目录包含插件工具 calculator');

// 4) 进入 AI 聊天 + 开代理模式 → 让模型调用插件工具
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
await cdp.eval(`(() => {
  const b = document.querySelector('.llm-agent-toggle');
  if (b && !b.classList.contains('on')) b.click();
  return 'true';
})()`);
await cdp.waitFor(`(() => document.querySelector('.llm-agent-toggle')?.classList.contains('on') ? 'true' : 'false')()`, 10000);
// 清空对话避免历史干扰
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.llm-chat-toolbar button')].find((el) => el.textContent.includes('清空对话'));
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
await sleep(800);

const q = '请务必实际调用 calculator 工具（不要自己口算）：计算 123*456 的值。';
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

const steps = await cdp.waitFor(`(() => {
  const p = document.querySelector('.llm-agent-panel');
  if (!p) return 'false';
  const names = [...p.querySelectorAll('.llm-agent-step-name')].map((e) => e.textContent.trim());
  return names.length ? JSON.stringify(names) : 'false';
})()`, 90000);
console.log('STEPS=' + steps);
check(!!steps && steps.includes('calculator'), `工具面板含 calculator（${steps}）`);

const reply = await cdp.waitFor(`(() => {
  const msgs = document.querySelectorAll('.llm-msg-bot .llm-msg-bubble');
  const last = msgs[msgs.length - 1];
  if (!last) return 'false';
  const running = !!document.querySelector('.llm-agent-running');
  const text = last.textContent.trim();
  return !running && text.length > 5 ? text.slice(0, 300) : 'false';
})()`, 120000);
console.log('REPLY=' + JSON.stringify(reply));
check(!!reply && reply.includes('56088'), `回复含正确计算结果 56088（${reply ? reply.slice(0, 60) + '…' : ''}）`);

// 截图（工具面板 + 插件调用结果）
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve('E:/ST/st_control/data/ui-audit/llm-plugin.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);

// 5) 停止（disable）→ 工具目录不含 calculator
await inv('set_agent_plugin_enabled', { id: pluginId, enabled: false });
const tools2 = await inv('get_agent_tools');
check(!tools2.some((t) => t.name === "calculator"), '停止插件后工具目录不再含 calculator');

// 6) 删除（undefine）→ 列表为空
await inv('delete_agent_plugin', { id: pluginId });
const list = await inv('list_agent_plugins');
check(!list.some((p) => p.id === pluginId), '删除插件后列表为空');

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
