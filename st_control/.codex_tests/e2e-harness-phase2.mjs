// E2E：Harness 阶段 2（工具循环 / 审批 / 会话内信任 / 工具历史回放 / 设置持久化）
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

// 0) 工具目录
const tools = await invoke('get_harness_tools');
console.log('TOOLS=' + tools.map((t) => `${t.name}${t.requires_approval ? "🔒" : ""}`).join(','));
check(tools.some((t) => t.name === 'get_current_time'), 'Harness 工具目录含 get_current_time');
check(tools.some((t) => t.name === 'exec_command' && t.requires_approval), 'exec_command 需审批');
check(tools.some((t) => t.name === 'search_knowledge_base'), '工具目录含 search_knowledge_base');
check(tools.some((t) => t.name === 'fetch_web_page'), '工具目录含 fetch_web_page');

// 1) 进入 Harness 并新建会话
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
await sleep(1200);
await cdp.eval(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(600);
const sessionId = await cdp.eval(`(() => {
  const s = [...document.querySelectorAll('.hns-session')][0];
  return s ? s.dataset.sid ?? null : null;
})()`);
// 经 IPC 获取当前会话 id（取最新创建；列表按 order_index 升序，
// [0] 不一定是新会话——脏库下会错选旧会话）
const sessionsNow = await invoke('harness_list_sessions');
const sid = sessionsNow.reduce((a, b) => (a.created_at >= b.created_at ? a : b)).id ?? sessionId;
console.log('SESSION=' + sid);
check(!!sid && sid.startsWith('h-'), `会话已创建（${sid}）`);

// 2) 模型选择（deepseek）——ModelSelect 自定义组件（提供方/模型/推理等级
// 三级菜单；旧原生 select 已被模型座替代，仅剩 AI 角色选择器）
await cdp.eval(`(async () => {
  const cfg = await window.__TAURI_INTERNALS__.invoke('get_llm_config');
  const chatP = (cfg.providers ?? []).find((x) => (x.models ?? []).includes('deepseek-v4-flash'));
  if (!chatP) return;
  const openSeat = () => { const s = document.querySelector('.hns-model-seat-btn'); if (s) { s.click(); return true; } return false; };
  const pickRow = (text) => {
    const rows = [...document.querySelectorAll('.hns-model-row')];
    const r = rows.find((x) => x.textContent.includes(text));
    if (r) { r.click(); return true; }
    return false;
  };
  if (!openSeat()) return;
  await new Promise((r) => setTimeout(r, 300));
  pickRow(chatP.name); // 提供方
  await new Promise((r) => setTimeout(r, 500));
  if (!openSeat()) return;
  await new Promise((r) => setTimeout(r, 300));
  pickRow('deepseek-v4-flash'); // 模型
})()`);
await sleep(900);

// 3) 工具调用：get_current_time（确定性、免审批）
let before = await botCount(cdp);
await sendPrompt(cdp, '请务必实际调用 get_current_time 工具获取当前时间，然后告诉我现在几点。');
const toolStep = await cdp.waitFor(`(() => {
  const names = [...document.querySelectorAll('.hns-tool-name')].map((e) => e.textContent.trim());
  return names.includes('get_current_time') ? 'true' : 'false';
})()`, 90000);
check(toolStep === 'true', '工具步骤出现在会话流（get_current_time）');
const toolDone = await cdp.waitFor(`(() => {
  const step = [...document.querySelectorAll('.hns-tool-step')].find((s) => s.classList.contains('ok'));
  return step ? 'true' : 'false';
})()`, 30000);
check(toolDone === 'true', '工具步骤完成（状态 ok）');
const reply1 = await waitTurnDone(cdp, before);
check(!!reply1 && /现在|时间|:/.test(reply1), `代理回复引用工具结果（${reply1?.slice(0, 60) ?? ''}）`);

// 工具步骤可展开详情（改版后详情区 = .hns-tool-detail 内嵌 ToolCard）
await cdp.eval(`(() => {
  const head = document.querySelector('.hns-tool-head');
  if (head) { head.click(); return 'true'; }
  return 'false';
})()`);
await sleep(400);
const detail = await cdp.eval(`(() => {
  const d = document.querySelector('.hns-tool-detail');
  return d ? d.textContent.trim().slice(0, 120) : 'false';
})()`);
check(!!detail && detail !== 'false', `工具详情可展开（${String(detail).slice(0, 40)}）`);
await cdp.eval(`(() => { const h = document.querySelector('.hns-tool-head'); if (h) h.click(); return 'true'; })()`);

// 4) 审批流：exec_command → 审批卡 → 批准
before = await botCount(cdp);
await sendPrompt(cdp, '请用 exec_command 工具执行命令：echo harness-phase2-ok');
const approval = await cdp.waitFor(`(() => {
  const a = document.querySelector('.hns-approval');
  return a ? a.textContent.trim() : 'false';
})()`, 90000);
console.log('APPROVAL=' + String(approval).slice(0, 140));
check(!!approval && approval.includes('exec_command'), '审批卡出现（exec_command + 完整参数）');
const approveBtns = await cdp.eval(`(() => {
  const a = document.querySelector('.hns-approval');
  return JSON.stringify([...a.querySelectorAll('button')].map((b) => b.textContent.trim()));
})()`);
const btns = JSON.parse(approveBtns);
check(btns.includes('记住并批准') && btns.includes('批准') && btns.includes('拒绝'), `审批卡三按钮（${btns.join('/')}）`);
// 点普通「批准」（最后一个 approve）；若模型多轮重复调用则逐张批准
await cdp.eval(`(() => {
  const btns = [...document.querySelectorAll('.hns-approve')];
  const b = btns[btns.length - 1];
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
await approveAnyCards(cdp);
const reply2 = await waitTurnDone(cdp, before);
check(!!reply2 && reply2.includes('harness-phase2-ok'), `批准后命令执行成功（${reply2?.slice(0, 80) ?? ''}）`);

// 5) 会话内信任（M8 参数指纹）：记住并批准 → 同参数第三次不再弹审批，
//    不同参数的命令仍需审批
before = await botCount(cdp);
await sendPrompt(cdp, '请再次用 exec_command 工具执行命令：echo trust-check');
const approval2 = await cdp.waitFor(`(() => {
  const a = document.querySelector('.hns-approval');
  return a ? a.textContent.trim() : 'false';
})()`, 90000);
check(!!approval2 && approval2.includes('exec_command'), '第二次 exec_command 弹出审批卡');
// 点「记住并批准」（第一个 approve）
await cdp.eval(`(() => { const b = document.querySelector('.hns-approve'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await approveAnyCards(cdp);
const reply3 = await waitTurnDone(cdp, before);
check(!!reply3 && reply3.includes('trust-check'), `记住并批准生效（${reply3?.slice(0, 60) ?? ''}）`);
// 第三次：SAME 参数（echo trust-check）→ 信任命中，不再弹审批
before = await botCount(cdp);
await sendPrompt(cdp, '请第三次用 exec_command 工具执行命令：echo trust-check');
const noApproval = await cdp.waitFor(`(() => {
  const a = document.querySelector('.hns-approval');
  return a ? 'true' : 'false';
})()`, 12000);
check(noApproval !== 'true', '记住批准后同会话同参数 exec_command 不再弹审批');
const reply4 = await waitTurnDone(cdp, before);
check(!!reply4 && reply4.includes('trust-check'), `同参数第三次直接执行成功（${reply4?.slice(0, 60) ?? ''}）`);
// 第四次：DIFFERENT 参数（echo diff-check）→ 参数指纹不匹配，仍需审批
before = await botCount(cdp);
await sendPrompt(cdp, '请第四次用 exec_command 工具执行命令：echo diff-check');
const approval4 = await cdp.waitFor(`(() => {
  const a = document.querySelector('.hns-approval');
  return a ? 'true' : 'false';
})()`, 90000);
check(approval4 === 'true', '不同参数命令仍弹审批（M8 参数指纹）');
await approveAnyCards(cdp);
const reply5 = await waitTurnDone(cdp, before);
check(!!reply5 && reply5.includes('diff-check'), `不同参数批准后执行成功（${reply5?.slice(0, 60) ?? ''}）`);

// 6) 工具历史回放：整页重载后工具步骤仍随回复展示
await cdp.send('Page.reload');
await sleep(6000);
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 30000);
await sleep(1500);
// 重载后选中本次会话（列表按 order_index 升序，需按标题定位）
await cdp.eval(`(() => {
  const items = [...document.querySelectorAll('.hns-session')];
  const el = items.find((x) => x.textContent.includes('get_current_time')) || document.querySelector('.hns-session');
  if (el) { el.click(); return 'true'; }
  return 'false';
})()`);
await sleep(1000);
const replayed = await cdp.waitFor(`(() => {
  const names = [...document.querySelectorAll('.hns-tool-name')].map((e) => e.textContent.trim());
  return names.includes('exec_command') && names.includes('get_current_time') ? 'true' : 'false';
})()`, 20000);
check(replayed === 'true', '重载后历史工具步骤从日志回放');

// 7) 设置持久化：提供方/模型选择记忆
const settings = await invoke('get_harness_settings');
console.log('SETTINGS=' + JSON.stringify(settings));
check(
  !!settings.last_provider_id && !!settings.last_model,
  `提供方/模型选择已持久化（${settings.last_provider_id}/${settings.last_model}）`,
);

// 8) 截图 + 清理本探针会话
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../data/ui-audit/llm-harness-phase2.png');
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
/** 等待本轮新增的助手气泡完成（按数量增长判定，避免读到上一轮旧气泡） */
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
/** 逐张批准可能残留的审批卡（模型多轮重复调用工具时） */
async function approveAnyCards(cdp) {
  for (let i = 0; i < 4; i++) {
    const card = await cdp.waitFor(`(() => {
      const a = document.querySelector('.hns-approval');
      return a ? 'true' : 'false';
    })()`, 6000, 200);
    if (card !== 'true') break;
    await cdp.eval(`(() => {
      const btns = [...document.querySelectorAll('.hns-approve')];
      const b = btns[btns.length - 1];
      if (b) { b.click(); return 'true'; }
      return 'false';
    })()`);
    await sleep(800);
  }
}
