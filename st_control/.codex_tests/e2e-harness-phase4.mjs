// E2E：Harness 阶段 4（todo / plan 守卫 / goal / subagent / schedule / workflow）
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

// ─── 0) 进入 Harness 并通过 UI 新建会话（保证 UI 选中该会话） ───
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
await sleep(1500);
await cdp.eval(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(800);
const listSessions = async () => invoke('harness_list_sessions');
// 新会话 = 最新创建（created_at 最大；列表按 order_index 升序，
// list[0] 不一定是新会话——脏库下会错选到旧会话）
const all0 = await listSessions();
const sid = all0.reduce((a, b) => (a.created_at >= b.created_at ? a : b)).id;
check(!!sid, `会话创建（${sid}）`);

// ─── 1) todo：人工命令派发 todo_write ───
const todoRes = await invoke('harness_execute_tool', {
  sessionId: sid,
  name: 'todo_write',
  arguments: JSON.stringify({
    items: [
      { content: '第一件事：调研', status: 'in_progress' },
      { content: '第二件事：实现', status: 'pending' },
    ],
  }),
});
check(todoRes.ok === true, '人工命令派发 todo_write 成功');
const state1 = await invoke('harness_session_state', { id: sid });
check(state1.todos.length === 2, `待办列表投影（${state1.todos.length} 项）`);

// ─── 2) plan：进入计划模式 + goal ───
await invoke('harness_execute_tool', {
  sessionId: sid, name: 'plan_enter', arguments: JSON.stringify({ plan: '先调研后实现' }),
});
await invoke('harness_execute_tool', {
  sessionId: sid, name: 'goal_set', arguments: JSON.stringify({ objective: 'E2E 目标：验证编排能力' }),
});
const state2 = await invoke('harness_session_state', { id: sid });
check(state2.plan_mode === true, '计划模式已开启（日志投影）');
check(state2.goal.includes('E2E 目标'), `目标已设置（${state2.goal}）`);

// ─── 3) UI：横幅与待办卡（重新选中当前会话以刷新投影；不切到列表首项） ───
await cdp.eval(`(() => {
  const active = document.querySelector('.hns-session.active') || document.querySelector('.hns-session');
  if (active) { active.click(); return 'true'; }
  return 'false';
})()`);
await sleep(900);
const todoCard = await cdp.waitFor(`(() => {
  const t = document.querySelector('.hns-todos');
  return t ? t.textContent.trim() : 'false';
})()`, 15000);
check(!!todoCard && todoCard.includes('第一件事'), `待办卡显示（${String(todoCard).slice(0, 60)}）`);
const planBanner = await cdp.eval(`(() => document.querySelector('.hns-plan') ? 'true' : 'false')()`);
check(planBanner === 'true', '计划模式横幅显示');
const goalBanner = await cdp.eval(`(() => document.querySelector('.hns-goal') ? 'true' : 'false')()`);
check(goalBanner === 'true', '目标横幅显示');

// 模型选择（deepseek）
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

// ─── 4) 计划模式守卫：模型调用 exec_command 被拦截（全程不弹审批） ───
const before1 = await botCount(cdp);
await sendPrompt(cdp, '请务必直接调用 exec_command 工具执行命令：echo plan-guard-test。绝对不要调用 plan_exit 或任何其他工具，只调用 exec_command。');
let sawCard = false;
{
  const t0 = Date.now();
  while (Date.now() - t0 < 180000) {
    const card = await cdp.eval(`!!document.querySelector('.hns-approval')`);
    if (card) {
      sawCard = true;
      break;
    }
    const done = await cdp.eval(`(() => {
      const bots = document.querySelectorAll('.hns-msg-bot .hns-bubble');
      if (bots.length <= ${before1}) return false;
      const running = !!document.querySelector('.hns-tool-running');
      const hint = !!document.querySelector('.hns-stream-hint');
      const t = bots[bots.length - 1].textContent.trim();
      return !running && !hint && t.length > 3;
    })()`);
    if (done) break;
    await sleep(400);
  }
}
check(!sawCard, '计划模式下 exec_command 不弹审批（守卫拦截）');
const reply1 = await cdp.waitFor(`(() => {
  const bots = document.querySelectorAll('.hns-msg-bot .hns-bubble');
  if (bots.length <= ${before1}) return 'false';
  const last = bots[bots.length - 1];
  return last.textContent.trim().length > 3 ? last.textContent.trim().slice(0, 300) : 'false';
})()`, 60000, 400);
check(!!reply1, `计划模式回合完成（${reply1?.slice(0, 80) ?? ''}）`);
const msgs1 = await invoke('harness_display_messages', { id: sid });
const lastTools1 = msgs1.slice().reverse().find((m) => m.role === 'assistant' && (m.tools?.length ?? 0) > 0)?.tools ?? [];
check(
  lastTools1.some((t) => t.name === 'exec_command' && (t.result ?? '').includes('计划模式')),
  '计划模式守卫拦截日志（结果含「计划模式」）',
);

// 退出计划模式
await invoke('harness_execute_tool', {
  sessionId: sid, name: 'plan_exit', arguments: JSON.stringify({}),
});
const state3 = await invoke('harness_session_state', { id: sid });
check(state3.plan_mode === false, 'plan_exit 后计划模式关闭');

// ─── 5) subagent：task 工具委派子代理 ───
const before2 = await botCount(cdp);
await sendPrompt(cdp, '请使用 task 工具委派子代理完成：计算 123*456 并只回复结果数字，然后把结果告诉我。');
const reply2 = await waitTurnDone(cdp, before2);
check(!!reply2, `子代理回合完成（${reply2?.slice(0, 80) ?? ''}）`);
const msgs2 = await invoke('harness_display_messages', { id: sid });
const lastTools2 = msgs2.slice().reverse().find((m) => m.role === 'assistant' && (m.tools?.length ?? 0) > 0)?.tools ?? [];
check(
  lastTools2.some((t) => t.name === 'task' && (t.result ?? '').includes('56088')),
  '子代理结论返回 56088',
);

// ─── 6) schedule：定时任务立即运行 ───
const sch = await invoke('save_harness_schedule', {
  schedule: {
    id: '', name: 'E2E定时', session_id: sid, prompt: '请只回复：SCHEDULE_TICK_OK',
    interval_minutes: 5, enabled: true, next_run_at: 0, last_run_at: null, created_at: '',
  },
});
check(!!sch.id && sch.id.startsWith('sch-'), `定时任务已创建（${sch.id}）`);
await invoke('run_harness_schedule_now', { id: sch.id });
const tick = await cdp.waitFor(`(async () => {
  const msgs = await window.__TAURI_INTERNALS__.invoke('harness_display_messages', { id: ${JSON.stringify(sid)} });
  const flat = JSON.stringify(msgs);
  return flat.includes('SCHEDULE_TICK_OK') ? 'true' : 'false';
})()`, 90000, 1000);
check(tick === 'true', '定时任务运行并产生回复（SCHEDULE_TICK_OK 落日志）');
await invoke('delete_harness_schedule', { id: sch.id });

// ─── 7) workflow：两阶段运行 ───
const wf = await invoke('save_harness_workflow', {
  workflow: {
    id: '', name: 'E2E工作流', description: '',
    stages: [
      { name: '阶段一', prompt: '请只回复：STAGE_ONE_DONE' },
      { name: '阶段二', prompt: '请只回复：STAGE_TWO_DONE' },
    ],
    created_at: '', updated_at: '',
  },
});
check(!!wf.id && wf.id.startsWith('wf-'), `工作流已创建（${wf.id}）`);
const wfRun = await invoke('run_harness_workflow', { workflowId: wf.id, sessionId: sid });
check(wfRun.stages.length === 2 && wfRun.stages.every((s) => s.ok), '工作流两阶段全部成功');
const msgs3 = await invoke('harness_display_messages', { id: sid });
const flat3 = JSON.stringify(msgs3);
check(flat3.includes('STAGE_ONE_DONE') && flat3.includes('STAGE_TWO_DONE'), '工作流阶段输出落会话日志');
await invoke('delete_harness_workflow', { id: wf.id });

// ─── 8) 治理抽屉新标签 ───
// close-then-open 强制刷新：前序探针/回合遗留的打开态会让点击变成关闭
// （打开按钮 = 头部 .hns-bar-icon「设置 / 钩子 / 预设」，旧 .hns-tools-btn 已随重设计移除）
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
await sleep(500);
const tabs = await cdp.eval(`(() => JSON.stringify([...document.querySelectorAll('.hns-drawer-tabs button')].map((x) => x.textContent.trim())))()`);
check(JSON.parse(tabs).includes('定时') && JSON.parse(tabs).includes('工作流'), `治理抽屉含定时/工作流标签（${tabs}）`);
await cdp.eval(`(() => { const b = document.querySelector('.hns-drawer-close'); if (b) b.click(); return 'true'; })()`);

// ─── 9) 截图 + 清理 ───
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../data/ui-audit/llm-harness-phase4.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);
await invoke('harness_delete_session', { id: sid });

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
