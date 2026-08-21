// E2E：H3 会话级互斥（用户回合与定时任务并发写同一会话 → 事件不交错）
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

// 0) 进入 Harness 并新建会话（等待列表就绪）
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

// 审批看门狗：exec_command 需审批
let approveStop = false;
const approveLoop = (async () => {
  while (!approveStop) {
    await cdp.eval(`(() => { const btns = [...document.querySelectorAll('.hns-approve')]; const b = btns[btns.length - 1]; if (b) { b.click(); return 't'; } return 'f'; })()`).catch(() => {});
    await sleep(150);
  }
})();

// 1) 创建定时任务（绑定同一会话，prompt 简短）
const sch = await invoke('save_harness_schedule', {
  schedule: { id: '', name: 'E2E并发', session_id: sid, prompt: '请只回复：SCHED_CONC_1', interval_minutes: 5, enabled: true, next_run_at: 0, last_run_at: null, created_at: '' },
});
const schId = sch?.id;
check(!!schId, `定时任务创建（${schId}）`);

// 2) 启动一个长用户回合（exec_command Start-Sleep 8 → 约 8-15 秒）。
//    发送必须确认成功（按钮可能偶发不可用——重试）
let sent = 'false';
for (let attempt = 0; attempt < 3 && sent !== 'true'; attempt++) {
  await cdp.eval(`(() => {
    const ta = document.querySelector('.hns-input textarea');
    const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
    setter.call(ta, '请调用 exec_command 执行：Start-Sleep 8，然后回复：TURN_CONC_DONE');
    ta.dispatchEvent(new Event('input', { bubbles: true }));
    return 'typed';
  })()`);
  await sleep(600);
  const inputLen = await cdp.eval(`(() => (document.querySelector('.hns-input textarea')?.value || '').length)()`);
  sent = await cdp.eval(`(() => { const b = document.querySelector('.hns-send'); if (b && !b.disabled && (document.querySelector('.hns-input textarea')?.value || '').length > 0) { b.click(); return 'true'; } return 'false'; })()`);
  console.log(`SEND_ATTEMPT=${attempt} inputLen=${inputLen} sent=${sent}`);
  if (sent !== 'true') await sleep(1000);
}
check(sent === 'true', '用户消息已发送');
// 等用户回合启动（第 1 条 user_message 落日志）
let userTurnStarted = false;
for (let i = 0; i < 20; i++) {
  await sleep(500);
  const evs = await invoke('harness_session_events', { id: sid, afterSeq: 0 });
  if ((evs ?? []).some(([, e]) => e?.type === 'user_message')) { userTurnStarted = true; break; }
}
check(userTurnStarted, '用户回合已启动（user_message 落日志）');
await sleep(1500); // 等用户回合进入执行

// 3) 回合进行中：立即触发定时任务（应被会话锁阻塞等待）
const runStarted = Date.now();
const runRes = await invoke('run_harness_schedule_now', { id: schId });
const runElapsed = Date.now() - runStarted;
console.log('SCHEDULE_RUN_ELAPSED_MS=' + runElapsed);

// 4) 等用户回合完成（模型可能不逐字回显标记——以「新助手消息出现」为准）
let userDone = false;
let userTurnAssistant = "";
for (let i = 0; i < 120; i++) {
  await sleep(1000);
  const msgs = await invoke('harness_display_messages', { id: sid });
  const ass = (msgs ?? []).filter((m) => m.role === "assistant");
  if (ass.length >= 1) {
    userDone = true;
    userTurnAssistant = String(ass[ass.length - 1].content ?? "").slice(0, 60);
    break;
  }
}
check(userDone, `用户回合完成（${userTurnAssistant}）`);
// 调度回合在后台被会话锁阻塞，用户回合结束后才执行——轮询等待事件日志
// 出现第 2 条 user_message 且其后有 assistant_message
let schedDone = false;
for (let i = 0; i < 40; i++) {
  await sleep(1000);
  const evs2 = await invoke('harness_session_events', { id: sid, afterSeq: 0 });
  const types2 = (evs2 ?? []).map(([, e]) => e?.type ?? '?');
  const umIdxs = [];
  types2.forEach((t, i) => { if (t === 'user_message') umIdxs.push(i); });
  if (umIdxs.length >= 2) {
    const last = umIdxs[umIdxs.length - 1];
    if (types2.slice(last + 1).some((t) => t === 'assistant_message')) { schedDone = true; break; }
  }
}
check(schedDone, '定时任务回合随后执行完成（SCHED_CONC_1）');

// 5) 事件序列校验：用户回合的所有事件必须先于定时任务的 user_message
const events = await invoke('harness_session_events', { id: sid, afterSeq: 0 });
const types = (events ?? []).map(([, e]) => e?.type ?? '?');
console.log('EVENT_SEQUENCE=' + JSON.stringify(types));
const userMsgIdxs = [];
types.forEach((t, i) => { if (t === 'user_message') userMsgIdxs.push(i); });
check(userMsgIdxs.length >= 2, `事件含 2 条用户消息（用户回合 + 定时任务）（${userMsgIdxs.length}）`);
// 用户回合的 user_message 是第 1 条；定时任务的是第 2 条
const firstUserIdx = userMsgIdxs[0];
const schedUserIdx = userMsgIdxs[1];
// 第 1 条 user_message 之后、第 2 条之前的片段应只属于用户回合
const between = types.slice(firstUserIdx + 1, schedUserIdx);
console.log('BETWEEN_TURNS=' + JSON.stringify(between));
check(between.some((t) => t === 'assistant_message' || t === 'assistant_chunk'), '用户回合在定时任务前完整结束（无交错）');
// 定时任务的事件在用户回合之后且完整
const schedTail = types.slice(schedUserIdx);
check(schedTail.some((t) => t === 'assistant_chunk' || t === 'assistant_message'), '定时任务回合随后执行完成（事件完整）');

// 6) 清理
await invoke('delete_harness_schedule', { id: schId });
await invoke('harness_delete_session', { id: sid });
approveStop = true;

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
