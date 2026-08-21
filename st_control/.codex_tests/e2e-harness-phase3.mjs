// E2E：Harness 阶段 3（guard 工具超时 / hooks 钩子桥 / preset 组合与会话作用域 / telemetry 用量）
// 前置：app 运行中（CDP 9222）+ Vite 1420。
const CDP_BASE = 'http://127.0.0.1:9222';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
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
  async waitFor(expression, timeoutMs = 120000, stepMs = 250) {
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
  cdp.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);

// ─── A) 预设 / 设置 / 钩子（IPC 准备） ───
const preset = await invoke('save_harness_preset', {
  preset: {
    id: '', name: 'E2E治理预设', description: '禁用联网搜索 + exec 超时 1 秒 + 提示词分区',
    disabled_tools: ['web_search'],
    overrides: { exec_command: { timeout_secs: 1 } },
    prompt_sections: [{ order: 10, title: 'e2e', content: '你是测试预设下的助手。' }],
    created_at: '', updated_at: '',
  },
});
check(!!preset.id && preset.id.startsWith('preset-'), `预设已创建（${preset.id}）`);
const presetId = preset.id;

const settings = await invoke('save_harness_settings', {
  settings: {
    last_provider_id: '', last_model: '',
    tool_timeout_secs: 10, max_agent_rounds: 3, preset_id: presetId,
  },
});
check(settings.preset_id === presetId && settings.tool_timeout_secs === 10, '设置已保存（超时 10s / 轮次 3 / 应用预设）');

const scope = await invoke('get_harness_scope');
console.log('SCOPE=' + JSON.stringify(scope));
check(scope.preset_name === 'E2E治理预设', `作用域应用预设（${scope.preset_name}）`);
check(scope.disabled_tools.includes('web_search'), '作用域禁用 web_search');

await invoke('save_harness_hooks', {
  hooks: [{
    id: 'hook-e2e-1', event: 'turn_end', enabled: true,
    command: 'Write-Output ("hook-ok " + $env:HARNESS_EVENT + " " + $env:HARNESS_SESSION)',
  }],
});
const hooksList = await invoke('list_harness_hooks');
check(hooksList.length === 1 && hooksList[0].enabled, '钩子已保存（turn_end → 命令）');

// ─── B) UI：治理抽屉 ───
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
await sleep(1200);
await cdp.eval(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(500);
await cdp.eval(`(() => {
  const close = document.querySelector('.hns-drawer-close');
  if (close) { close.click(); return 'closed'; }
  return 'none';
})()`);
await sleep(400);
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.hns-bar-icon')].find((x) => (x.title || '').includes('设置 / 钩子'));
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
await sleep(600);
const drawerShown = await cdp.eval(`(() => document.querySelector('.hns-drawer') ? 'true' : 'false')()`);
check(drawerShown === 'true', '治理抽屉打开');
await cdp.eval(`(() => {
  const tabs = [...document.querySelectorAll('.hns-drawer-tabs button')];
  const p = tabs.find((x) => x.textContent.trim() === '预设');
  if (p) { p.click(); return 'true'; }
  return 'false';
})()`);
await sleep(400);
const presetVisible = await cdp.eval(`(() => {
  const items = [...document.querySelectorAll('.hns-preset-item')];
  return items.some((x) => x.textContent.includes('E2E治理预设')) ? 'true' : 'false';
})()`);
check(presetVisible === 'true', '预设列表显示「E2E治理预设」（禁用 1 个工具）');

// 模型选择（deepseek）
await cdp.eval(`(() => { const b = document.querySelector('.hns-drawer-close'); if (b) b.click(); return 'true'; })()`);
await cdp.eval(`(async () => {
  const cfg = await window.__TAURI_INTERNALS__.invoke('get_llm_config');
  const chatP = (cfg.providers ?? []).find((x) => (x.models ?? []).includes('deepseek-v4-flash'));
  if (!chatP) return;
  const sels = [...document.querySelectorAll('.hns-bar-right select')];
  const setSelect = (el, val) => {
    if (!el || ![...el.options].some((o) => o.value === val)) return false;
    el.value = val; el.dispatchEvent(new Event('change', { bubbles: true })); return true;
  };
  setSelect(sels[0], chatP.id);
  await new Promise((r) => setTimeout(r, 600));
  const sels2 = [...document.querySelectorAll('.hns-bar-right select')];
  setSelect(sels2[0], chatP.id);
  setSelect(sels2[1], 'deepseek-v4-flash');
})()`);
await sleep(900);

// ─── C) 普通回合：钩子触发 + 用量统计 ───
const before1 = await botCount(cdp);
await sendPrompt(cdp, '请回复：PHASE3_TURN1_OK');
const reply1 = await waitTurnDone(cdp, before1);
check(!!reply1 && reply1.includes('PHASE3_TURN1_OK'), `回合 1 完成（${reply1?.slice(0, 60) ?? ''}）`);
await sleep(1500);
const _all1 = await invoke('harness_list_sessions');
const usage = await invoke('harness_usage_summary', { id: _all1.reduce((a, b) => (a.created_at >= b.created_at ? a : b)).id });
console.log('USAGE=' + JSON.stringify(usage));
check(usage.turns >= 1 && usage.prompt_tokens > 0, `telemetry 用量已记录（${usage.turns} 轮 / ${usage.prompt_tokens} prompt tokens）`);
// 钩子触发记录（前端事件日志；打开按钮 = 头部 .hns-bar-icon，旧 .hns-tools-btn 已随重设计移除）
await cdp.eval(`(() => {
  const close = document.querySelector('.hns-drawer-close');
  if (close) { close.click(); return 'closed'; }
  return 'none';
})()`);
await sleep(300);
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.hns-bar-icon')].find((x) => (x.title || '').includes('设置 / 钩子'));
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
await sleep(400);
await cdp.eval(`(() => {
  const tabs = [...document.querySelectorAll('.hns-drawer-tabs button')];
  const h = tabs.find((x) => x.textContent.trim() === '钩子');
  if (h) { h.click(); return 'true'; }
  return 'false';
})()`);
await sleep(400);
const hookFired = await cdp.waitFor(`(() => {
  const logs = [...document.querySelectorAll('.hns-hook-log')].map((x) => x.textContent);
  return logs.some((x) => x.includes('hook-ok') && x.includes('turn_end')) ? 'true' : 'false';
})()`, 20000);
check(hookFired === 'true', '钩子触发记录显示（turn_end → hook-ok）');
await cdp.eval(`(() => { const b = document.querySelector('.hns-drawer-close'); if (b) b.click(); return 'true'; })()`);

// ─── D) 工具超时守卫：preset 覆盖 exec_command 超时 1 秒 ───
const before2 = await botCount(cdp);
await sendPrompt(cdp, '请用 exec_command 工具执行命令：Start-Sleep 3');
const approval = await cdp.waitFor(`(() => {
  const a = document.querySelector('.hns-approval');
  return a ? 'true' : 'false';
})()`, 90000);
check(approval === 'true', 'exec_command 弹审批卡');
await cdp.eval(`(() => {
  const btns = [...document.querySelectorAll('.hns-approve')];
  const b = btns[btns.length - 1];
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
// 模型同一响应可能多次调用 exec_command → 多张审批卡。持续批准直至
// 连续两轮无新卡（防止延迟出现的卡在退出后才到达导致回合挂起 10 分钟）
let idleRounds = 0;
for (let i = 0; i < 12 && idleRounds < 2; i++) {
  const card = await cdp.waitFor(`(() => {
    const a = document.querySelector('.hns-approval');
    return a ? 'true' : 'false';
  })()`, 4000, 250);
  if (card !== 'true') {
    idleRounds += 1;
    continue;
  }
  idleRounds = 0;
  await cdp.eval(`(() => {
    const btns = [...document.querySelectorAll('.hns-approve')];
    const b = btns[btns.length - 1];
    if (b) { b.click(); return 'true'; }
    return 'false';
  })()`);
}
const reply2 = await waitTurnDone(cdp, before2);
check(!!reply2, `超时回合完成（${reply2?.slice(0, 80) ?? ''}）`);
// 工具步骤结果应包含超时（guard 生效）
const _all2 = await invoke('harness_list_sessions');
const msgs = await invoke('harness_display_messages', { id: _all2.reduce((a, b) => (a.created_at >= b.created_at ? a : b)).id });
const lastTools = msgs
  .slice()
  .reverse()
  .find((m) => m.role === 'assistant' && (m.tools?.length ?? 0) > 0)?.tools ?? [];
console.log('TOOLS_LAST=' + JSON.stringify(lastTools.slice(-2)));
check(
  lastTools.some((t) => t.name === 'exec_command' && (t.result ?? '').includes('超时')),
  '工具超时守卫生效（exec_command 1 秒超时，结果含「超时」）',
);

// ─── E) 清理 ───
await invoke('delete_harness_preset', { id: presetId });
await invoke('save_harness_settings', {
  settings: { last_provider_id: '', last_model: '', tool_timeout_secs: null, max_agent_rounds: null, preset_id: null },
});
await invoke('save_harness_hooks', { hooks: [] });
const scopeAfter = await invoke('get_harness_scope');
check(scopeAfter.preset_name === '' && !scopeAfter.disabled_tools.includes('web_search'), '清理后作用域恢复全局');

// ─── F) 截图 + 清理本探针会话（防止残留/挂起审批污染后续运行） ───
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../data/ui-audit/llm-harness-phase3.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);
const _clean3 = await invoke('harness_list_sessions');
await invoke('harness_delete_session', { id: _clean3.reduce((a, b) => (a.created_at >= b.created_at ? a : b)).id });

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);

// ─── 辅助 ───
async function botCount(cdp) {
  return await cdp.eval(`document.querySelectorAll('.hns-msg-bot .hns-bubble').length`);
}
async function sendPrompt(cdp, q) {
  await cdp.eval(`(() => {
    const ta = document.querySelector('.hns-input textarea');
    const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
    setter.call(ta, ${JSON.stringify(q)});
    ta.dispatchEvent(new Event('input', { bubbles: true }));
    return 'true';
  })()`);
  await sleep(400);
  const ok = await cdp.eval(`(() => { const b = document.querySelector('.hns-send'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
  console.log('SENT=' + ok + ' ' + q.slice(0, 40));
  return ok;
}
async function waitTurnDone(cdp, beforeCount) {
  return await cdp.waitFor(`(() => {
    const bots = document.querySelectorAll('.hns-msg-bot .hns-bubble');
    if (bots.length <= ${beforeCount}) return 'false';
    const last = bots[bots.length - 1];
    const running = !!document.querySelector('.hns-tool-running');
    const hint = !!document.querySelector('.hns-stream-hint');
    const text = last.textContent.trim();
    return !running && !hint && text.length > 3 ? text.slice(0, 300) : 'false';
  })()`, 180000, 300);
}
