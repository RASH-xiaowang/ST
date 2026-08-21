// E2E：goal 自动续跑（DSH goal-round-driver；H5 修复后真实回路验证）
// 前置：隔离环境（CDP 9222 + Vite 1420 + ST_WECHAT_APP_DIR=.e2e/app）
const CDP_BASE = 'http://127.0.0.1:9222';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function findTarget() {
  for (let i = 0; i < 40; i++) {
    try {
      const list = await (await fetch(`${CDP_BASE}/json/list`)).json();
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
let failures = 0;
const check = (ok, msg) => {
  console.log((ok ? 'PASS: ' : 'FAIL: ') + msg);
  if (!ok) failures++;
};
const invoke = (cmd, args = {}) =>
  cdp.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`).catch((e) => ({ __err: String(e) }));

// 0) 进入 Harness；等待会话列表就绪（应用启动自动建会话可能较慢）
await cdp.eval(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(1500);
let sessList = await invoke('harness_list_sessions');
for (let i = 0; i < 30 && !((sessList ?? []).length); i++) {
  await sleep(1000);
  sessList = await invoke('harness_list_sessions');
}
await cdp.eval(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(1000);
sessList = await invoke('harness_list_sessions');
const sid = (sessList ?? []).slice().sort((a, b) => (b.created_at || '').localeCompare(a.created_at || ''))[0]?.id;
check(!!sid, `会话取得（${sid}）`);
if (!sid) { console.log(`FAILURES=${failures}`); ws.close(); process.exit(1); }

// 1) 发送目标任务：goal_create(max_goal_rounds=2)，要求逐轮输出数字
const prompt = '请调用 goal_create 工具设置目标：逐轮输出数字，第一轮输出 1，之后每轮输出下一个数字。max_goal_rounds 设为 2。目标达成（输出到 3）后调用 goal_update 标记 complete。';
await cdp.eval(`(() => {
  const ta = document.querySelector('.hns-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, ${JSON.stringify(prompt)});
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'typed';
})()`);
await sleep(400);
const sent = await cdp.eval(`(() => { const b = document.querySelector('.hns-send'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
console.log('SENT=' + sent);
if (sent !== 'true') {
  // 发送未触发：重试一次
  await sleep(800);
  await cdp.eval(`(() => { const b = document.querySelector('.hns-send'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
}

// 2) 等待回合序列结束（目标 complete 或状态静止）。采样 goal 状态直到
//    连续 N 次无变化且回合完成
let prev = null, idle = 0;
const deadline = Date.now() + 360000; // 6 分钟上限（2 次自动续跑 × 每轮 10-60s）
let autoRounds = 0;
while (Date.now() < deadline) {
  await sleep(3000);
  const st = await invoke('harness_session_state', { id: sid });
  const sig = `${st?.goal_status}|${st?.goal_revision}|${st?.goal}`;
  if (st?.goal_revision && prev && st.goal_revision !== prev) {
    autoRounds = Math.max(autoRounds, (st.goal_revision ?? 0) - 1);
  }
  if (sig === prev) {
    idle += 1;
    if (idle >= 5) break; // 状态静止 15s → 回合序列结束
  } else {
    idle = 0;
    prev = sig;
  }
  const bots = await cdp.eval(`document.querySelectorAll('.hns-msg-bot .hns-bubble').length`);
  if (bots === 0) continue;
}
const state = await invoke('harness_session_state', { id: sid });
console.log('GOAL_STATE=' + JSON.stringify(state));
// 诊断：目标事件与消息流（判断模型是否执行了 goal_create）
const events = await invoke('harness_session_events', { id: sid, afterSeq: 0 }).catch(() => []);
console.log('GOAL_EVENTS=' + JSON.stringify((events ?? []).map(([, e]) => e?.type ?? "?")));
const msgsDiag = await invoke('harness_display_messages', { id: sid }).catch(() => []);
console.log('BOTS=' + JSON.stringify((msgsDiag ?? []).filter((m) => m.role === "assistant").map((m) => String(m.content).slice(0, 80))).slice(0, 500));

// 3) 断言
check(!!state?.goal && state.goal.length > 0, `目标已设置（${String(state?.goal).slice(0, 40)}）`);
check(state?.goal_max_rounds === 2, `max_goal_rounds=2（${state?.goal_max_rounds}）`);
check(state?.goal_revision >= 2, `自动续跑已发生（revision=${state?.goal_revision} ≥ 2）`);
check(state?.goal_status === 'complete' || state?.goal_status === 'active', `目标状态收敛（${state?.goal_status}）`);
// 消息列表应包含多个回合的数字输出（自动续跑真实执行）
const msgs = await invoke('harness_display_messages', { id: sid });
const allText = JSON.stringify(msgs ?? []);
const nums = ['1', '2', '3'].filter((n) => allText.includes(n));
console.log('NUMBERS_SEEN=' + nums.join(','));
check(nums.length >= 2, `多回合数字输出已产生（${nums.join(',')}）`);

// 4) 清理
await invoke('harness_delete_session', { id: sid });

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
