// 已停用（2026-08）：原「AI 聊天」板块已并入 Harness 会话，本脚本断言的独立面板 UI 不再存在；
// 功能验证改由 e2e-harness-phase* 与 verify-harness-chat-* 承担。
// E2E：AI 聊天工具系统 v2（工具详情面板 / 新内置工具 / 历史持久化 / 审批增强 / 插件重试）
// 前置：app 运行中（CDP 9222）+ Vite 1420；代理模式用 deepseek-v4-flash。
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

const invoke = (cmd, args = {}) =>
  cdp.eval(
    `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`,
  );

// ─── 0) 新内置工具目录 ───
const tools = await invoke('get_agent_tools');
console.log('TOOLS=' + tools.map((t) => `${t.name}${t.requires_approval ? "🔒" : ""}`).join(','));
check(tools.some((t) => t.name === 'get_current_time'), '内置工具含 get_current_time');
check(tools.some((t) => t.name === 'search_knowledge_base'), '内置工具含 search_knowledge_base');
check(tools.some((t) => t.name === 'fetch_web_page'), '内置工具含 fetch_web_page');
check(tools.some((t) => t.name === 'exec_command' && t.requires_approval), 'exec_command 仍需审批');

// ─── 插件准备：calculator（确定性）+ flaky_probe（第一次必失败，用于重试） ───
const mkPlugin = (name, toolName, desc, code, approval = false) => ({
  id: '', name, description: desc, enabled: true,
  tools: [{ name: toolName, description: desc, parameters: { type: 'object', properties: {}, required: [] }, requires_approval: approval, code }],
  versions: [], created_at: '', updated_at: '',
});
const existing = await invoke('list_agent_plugins');
// 清理上次中断运行残留的探针插件（避免重名工具干扰）
for (const p of existing.filter((x) => x.name === '探针插件')) {
  try {
    await invoke('delete_agent_plugin', { id: p.id });
  } catch {}
}
const existingAfter = await invoke('list_agent_plugins');
const hadCalc = existingAfter.some((p) => (p.tools ?? []).some((t) => t.name === 'calculator'));
const createdPluginIds = [];
if (!hadCalc) {
  const p = await invoke('save_agent_plugin', { plugin: mkPlugin('计算器插件', 'calculator', '计算表达式，参数 expression', 'return String(eval(args.expression));') });
  createdPluginIds.push(p.id);
}
const flaky = await invoke('save_agent_plugin', {
  plugin: mkPlugin('探针插件', 'flaky_probe', '探针工具，参数 x；第一次调用必然失败', 'globalThis.__flakyN = (globalThis.__flakyN || 0) + 1;\nif (globalThis.__flakyN === 1) { throw new Error(\'第一次必失败\'); }\nreturn \'ok:\' + (args.x || \'\');'),
});
createdPluginIds.push(flaky.id);
check(flaky.id.startsWith('plugin-'), '探针插件已创建');

// ─── 1) 进入 AI 聊天 + 代理模式 ───
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('.hns-viewbar button')].find((el) => (el.textContent || '').trim() === 'AI 聊天' && el.offsetParent !== null) ?? (() => { const n = [...document.querySelectorAll('button.nav-item')].find((el) => el.title === 'Harness'); if (n) n.click(); return null; })();
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
await sleep(1500);
const cfg = await invoke('get_llm_config');
const chatP = (cfg.providers ?? []).find((x) => (x.models ?? []).includes('deepseek-v4-flash'));
const chatModels = (chatP?.models ?? []).filter((m) => m !== 'deepseek-v4-flash');
// 清掉上一轮 probe 可能残留的「记住批准」信任记录，保证审批流可重复验证
if (chatP) {
  try {
    await invoke('clear_agent_trust', { providerId: chatP.id, model: 'deepseek-v4-flash' });
  } catch {}
}
await cdp.eval(`(async () => {
  const setSelect = (el, val) => {
    if (!el || ![...el.options].some((o) => o.value === val)) return false;
    el.value = val; el.dispatchEvent(new Event('change', { bubbles: true })); return true;
  };
  const sels = [...document.querySelectorAll('.llm-chat-toolbar select')];
  setSelect(sels[0], ${JSON.stringify(chatP?.id ?? '')});
  await new Promise((r) => setTimeout(r, 700));
  const sels2 = [...document.querySelectorAll('.llm-chat-toolbar select')];
  setSelect(sels2[0], ${JSON.stringify(chatP?.id ?? '')});
  setSelect(sels2[1], 'deepseek-v4-flash');
  await new Promise((r) => setTimeout(r, 800));
})()`);
await sleep(1000);
await cdp.eval(`(() => { const b = document.querySelector('.llm-agent-toggle'); if (b && !b.classList.contains('on')) b.click(); return 'true'; })()`);
await cdp.waitFor(`(() => document.querySelector('.llm-agent-toggle')?.classList.contains('on') ? 'true' : 'false')()`, 10000);

async function clearChat() {
  await cdp.eval(`(() => {
    const b = [...document.querySelectorAll('.llm-chat-toolbar button')].find((el) => el.textContent.includes('清空对话'));
    if (b) { b.click(); return 'true'; }
    return 'false';
  })()`);
  await sleep(1000);
}
async function sendPrompt(q) {
  await cdp.eval(`(() => {
    const ta = document.querySelector('.llm-chat-input textarea');
    const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
    setter.call(ta, ${JSON.stringify(q)});
    ta.dispatchEvent(new Event('input', { bubbles: true }));
    return 'true';
  })()`);
  await sleep(400);
  await cdp.eval(`(() => { const b = document.querySelector('.llm-send-btn'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
}
async function waitTurnDone() {
  return await cdp.waitFor(`(() => {
    const msgs = document.querySelectorAll('.llm-msg-bot .llm-msg-bubble');
    const last = msgs[msgs.length - 1];
    if (!last) return 'false';
    const sending = !!document.querySelector('.llm-agent-running');
    const text = last.textContent.trim();
    return !sending && text.length > 3 ? text.slice(0, 300) : 'false';
  })()`, 180000);
}

// ─── 2) 工具步骤详情面板 + 耗时 ───
await clearChat();
await sendPrompt('请务必实际调用 calculator 工具计算 12*34，然后回复计算结果数字。');
const stepsRaw = await cdp.waitFor(`(() => {
  const p = document.querySelector('.llm-agent-panel');
  if (!p) return 'false';
  const names = [...p.querySelectorAll('.llm-agent-step-name')].map((e) => e.textContent.trim());
  return names.includes('calculator') ? 'true' : 'false';
})()`, 90000);
check(stepsRaw === 'true', 'calculator 步骤出现在面板');
// 耗时徽标（tool_done 带 duration_ms）
const dur = await cdp.waitFor(`(() => {
  const d = document.querySelector('.llm-agent-step-dur');
  return d && d.textContent.trim() ? d.textContent.trim() : 'false';
})()`, 30000);
check(!!dur && dur !== 'false', `步骤显示执行耗时（${dur}）`);
// 展开详情（选择参数含 12*34 的步骤：模型可能先带空参数调用一次）
const expanded = await cdp.waitFor(`(() => {
  const heads = [...document.querySelectorAll('.llm-agent-step-head')];
  const hit = heads.find((h) => {
    const a = h.querySelector('.llm-agent-step-args');
    return a && a.textContent.includes('12');
  }) ?? heads[0];
  if (!hit) return 'false';
  hit.click();
  return 'true';
})()`, 10000);
await sleep(600);
const detail = await cdp.eval(`(() => {
  const d = document.querySelector('.llm-agent-step-detail');
  if (!d) return 'false';
  const pres = [...d.querySelectorAll('.llm-agent-step-pre')].map((e) => e.textContent);
  return JSON.stringify({ pres, hasCopy: d.querySelectorAll('.llm-agent-step-copy').length });
})()`);
console.log('DETAIL=' + detail);
const detailObj = JSON.parse(detail);
check(detailObj.pres?.length >= 2, '详情展开后显示参数与结果两个区块');
check(detailObj.pres?.[0]?.includes('12') && detailObj.pres?.[0]?.includes('34'), '参数区块含 12/34');
check(detailObj.pres?.[1]?.includes('408'), '结果区块含 408');
check(detailObj.hasCopy >= 1, '详情提供复制按钮');
// 收起（点击展开中步骤的头部）
await cdp.eval(`(() => {
  const step = document.querySelector('.llm-agent-step-detail')?.closest('.llm-agent-step');
  const head = step?.querySelector('.llm-agent-step-head');
  if (head) { head.click(); return 'true'; }
  return 'false';
})()`);
await sleep(500);
const collapsed = await cdp.eval(`(() => document.querySelector('.llm-agent-step-detail') ? 'false' : 'true')()`);
check(collapsed === 'true', '再次点击可收起详情');
const reply = await waitTurnDone();
check(!!reply && reply.includes('408'), `代理回复含计算结果（${reply?.slice(0, 60) ?? ''}）`);

// ─── 3) 工具调用历史持久化 ───
const savedSteps = await invoke('get_agent_tool_steps', { providerId: chatP.id, model: 'deepseek-v4-flash' });
console.log('SAVED_STEPS=' + JSON.stringify(savedSteps).slice(0, 300));
check(Array.isArray(savedSteps) && savedSteps.length >= 1, '工具步骤已落盘（get_agent_tool_steps 非空）');
const lastBatch = savedSteps?.[savedSteps.length - 1]?.[1] ?? [];
check(lastBatch.some((s) => s.name === 'calculator'), '落盘步骤含 calculator');
check(lastBatch.some((s) => s.name === 'calculator' && typeof s.duration_ms === 'number'), '落盘步骤含耗时字段');
// 模拟重新打开会话：切换模型再切回（触发 loadHistory）
if (chatModels.length > 0) {
  await cdp.eval(`(async () => {
    const setSelect = (el, val) => {
      if (!el || ![...el.options].some((o) => o.value === val)) return false;
      el.value = val; el.dispatchEvent(new Event('change', { bubbles: true })); return true;
    };
    setSelect([...document.querySelectorAll('.llm-chat-toolbar select')][1], ${JSON.stringify(chatModels[0])});
    await new Promise((r) => setTimeout(r, 900));
    setSelect([...document.querySelectorAll('.llm-chat-toolbar select')][1], 'deepseek-v4-flash');
    await new Promise((r) => setTimeout(r, 900));
  })()`);
} else {
  await cdp.send('Page.reload');
  await sleep(6000);
  await cdp.waitFor(`(() => {
    const b = [...document.querySelectorAll('.hns-viewbar button')].find((el) => (el.textContent || '').trim() === 'AI 聊天' && el.offsetParent !== null) ?? (() => { const n = [...document.querySelectorAll('button.nav-item')].find((el) => el.title === 'Harness'); if (n) n.click(); return null; })();
    if (b) { b.click(); return 'true'; }
    return 'false';
  })()`, 20000);
  await sleep(1500);
}
const histPanel = await cdp.waitFor(`(() => {
  const p = document.querySelector('.llm-agent-panel-history');
  if (!p) return 'false';
  const names = [...p.querySelectorAll('.llm-agent-step-name')].map((e) => e.textContent.trim());
  return names.includes('calculator') ? 'true' : 'false';
})()`, 30000);
check(histPanel === 'true', '重新打开会话后历史工具调用面板仍显示 calculator');
const histDetail = await cdp.eval(`(() => {
  const p = document.querySelector('.llm-agent-panel-history');
  if (!p) return 'false';
  const head = p.querySelector('.llm-agent-step-head');
  if (!head) return 'false';
  head.click();
  return 'true';
})()`);
await sleep(600);
const histPre = await cdp.eval(`(() => {
  const pre = document.querySelector('.llm-agent-panel-history .llm-agent-step-pre');
  return pre ? pre.textContent : 'false';
})()`);
check(!!histPre && histPre.includes('12'), '历史步骤可展开查看参数');

// ─── 4) 审批增强：完整命令展示 + 记住并批准 ───
await clearChat();
await sendPrompt('请用 exec_command 工具执行命令：echo hello-trust-probe');
const approvalTxt = await cdp.waitFor(`(() => {
  const a = document.querySelector('.llm-agent-approval');
  return a ? a.textContent.trim() : 'false';
})()`, 90000);
console.log('APPROVAL=' + approvalTxt);
check(!!approvalTxt && approvalTxt.includes('exec_command'), '审批卡片出现（exec_command）');
const approvalArgs = await cdp.eval(`(() => {
  const code = document.querySelector('.llm-agent-approval-args');
  return code ? code.textContent : 'false';
})()`);
check(!!approvalArgs && approvalArgs !== 'false' && (approvalArgs.includes('echo') || approvalArgs.includes('hello-trust-probe')), `审批卡显示完整命令（${String(approvalArgs).slice(0, 80)}）`);
const btnTexts = await cdp.eval(`(() => {
  const a = document.querySelector('.llm-agent-approval');
  return JSON.stringify([...a.querySelectorAll('button')].map((b) => b.textContent.trim()));
})()`);
const btns = JSON.parse(btnTexts);
check(btns.includes('记住并批准') && btns.includes('批准') && btns.includes('拒绝'), `审批卡含三按钮（${btns.join('/')}）`);
// 点击「记住并批准」（第一个 approve 按钮）
await cdp.eval(`(() => { const b = document.querySelector('.llm-agent-approve'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
const reply3 = await waitTurnDone();
check(!!reply3 && reply3.includes('hello-trust-probe'), `记住并批准后命令执行成功（${reply3?.slice(0, 80) ?? ''}）`);
// 第二次 exec（同一会话内，不清空）：信任应生效，不再弹审批
await sendPrompt('请用 exec_command 工具执行命令：echo hello-trust-2');
const approvalAgain = await cdp.waitFor(`(() => {
  const a = document.querySelector('.llm-agent-approval');
  return a ? 'true' : 'false';
})()`, 15000);
check(approvalAgain !== 'true', '记住批准后第二次 exec_command 不再弹审批');
const reply4 = await waitTurnDone();
check(!!reply4 && reply4.includes('hello-trust-2'), `第二次命令直接执行成功（${reply4?.slice(0, 80) ?? ''}）`);

// ─── 5) 插件工具失败重试 ───
await clearChat();
// 重置探针计数器（globalThis 状态跨 probe 运行残留）
await cdp.eval(`(() => { globalThis.__flakyN = 0; return 'true'; })()`);
await sendPrompt('请调用 flaky_probe 工具，参数 x 填 "t"。如果工具报错，请直接告诉我错误信息，不要重试。');
const errStep = await cdp.waitFor(`(() => {
  const p = document.querySelector('.llm-agent-panel');
  if (!p) return 'false';
  const step = [...p.querySelectorAll('.llm-agent-step')].find((s) => s.classList.contains('err'));
  return step ? 'true' : 'false';
})()`, 90000);
check(errStep === 'true', 'flaky_probe 第一次执行失败（步骤置为 err）');
// 展开并点击重试（找不到面板时给出明确失败而非崩溃）
await cdp.eval(`(() => {
  const p = document.querySelector('.llm-agent-panel');
  const head = p?.querySelector('.llm-agent-step-head');
  if (head) head.click();
  return head ? 'true' : 'false';
})()`);
await sleep(500);
const retryBtn = await cdp.waitFor(`(() => {
  const b = document.querySelector('.llm-agent-step-retry');
  return b ? 'true' : 'false';
})()`, 10000);
check(retryBtn === 'true', '失败插件步骤提供重试按钮');
await cdp.eval(`(() => { const b = document.querySelector('.llm-agent-step-retry'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
const retriedOk = await cdp.waitFor(`(() => {
  const p = document.querySelector('.llm-agent-panel');
  if (!p) return 'false';
  const chip = p.querySelector('.llm-agent-step-retried');
  if (!chip) return 'false';
  const step = chip.closest('.llm-agent-step');
  return step && step.classList.contains('ok') ? 'true' : 'false';
})()`, 20000);
check(retriedOk === 'true', '重试后该步骤转为 ok 且带「已重试」标记');
const retryResult = await cdp.eval(`(() => {
  const chip = document.querySelector('.llm-agent-panel .llm-agent-step-retried');
  const step = chip?.closest('.llm-agent-step');
  const pres = step?.querySelectorAll('.llm-agent-step-pre');
  const pre = pres?.[pres.length - 1]; // 最后一个 pre = 结果区块
  return pre ? pre.textContent : 'false';
})()`);
check(!!retryResult && retryResult.includes('ok:'), `重试结果含 ok:（${String(retryResult).slice(0, 60)}）`);
await waitTurnDone();

// ─── 截图 ───
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve('E:/ST/st_control/data/ui-audit/llm-agent-tools-v2.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);

// ─── 清理：删除本探针创建的插件 ───
for (const pid of createdPluginIds) {
  try {
    await invoke('delete_agent_plugin', { id: pid });
  } catch {}
}

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
