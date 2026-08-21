// 验证：会话维护能力（session_* 工具 + 模型自清空）+ 工作路径放大（项目根）
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const list = await (await fetch('http://127.0.0.1:9222/json/list')).json();
const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
if (!t) { console.log('NO PAGE'); process.exit(1); }
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0; const pend = new Map();
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pend.has(m.id)) { const { resolve, reject } = pend.get(m.id); pend.delete(m.id); m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result); }
};
const send = (method, params = {}) => new Promise((resolve, reject) => {
  pend.set(++id, { resolve, reject });
  ws.send(JSON.stringify({ id, method, params }));
});
const ev = (expression) => send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
  .then((r) => { if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails)); return r.result?.value; });
const invoke = (cmd, args = {}) =>
  ev(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`)
    .catch((e) => ({ __err: String(e) }));
let fails = 0;
const check = (ok, msg) => { console.log((ok ? 'PASS: ' : 'FAIL: ') + msg); if (!ok) fails++; };

// 进入 Harness 会话
await ev(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((x) => x.title === 'Harness'); if (b) b.click(); return 'true'; })()`);
await sleep(2500);
const ready = await ev(`(async () => {
  for (let i = 0; i < 30; i++) {
    if (document.querySelector('.hns-input textarea')) return 'true';
    await new Promise((r) => setTimeout(r, 500));
  }
  return 'false';
})()`);
check(ready === 'true', 'Harness 会话输入区就绪');

// ─── 1) 工作路径放大：默认工作区 = 应用项目根 ───
// （取最新会话；列表按 order_index 升序，[0] 不一定是新会话）
const _newest = async () => {
  const l = await invoke('harness_list_sessions');
  return l.reduce((a, b) => (a.created_at >= b.created_at ? a : b)).id;
};
// M8 参数指纹语义下 exec_command 需逐次审批——后台看门狗自动点「批准」
let approveStop = false;
const approveLoop = (async () => {
  while (!approveStop) {
    await ev(`(() => { const btns = [...document.querySelectorAll('.hns-approve')]; const b = btns[btns.length - 1]; if (b) { b.click(); return 't'; } return 'f'; })()`).catch(() => {});
    await sleep(150);
  }
})();
const pwd = await invoke('harness_execute_tool', {
  sessionId: await _newest(),
  name: 'exec_command',
  arguments: JSON.stringify({ command: 'Write-Output (Get-Location).Path' }),
});
check(String(pwd?.result ?? pwd?.__err ?? '').includes('st_control'), `exec_command 锚定项目根（${String(pwd?.result ?? '').slice(0, 90)}）`);
// fs 可读自身源码（自维护）
const selfRead = await invoke('harness_execute_tool', {
  sessionId: await _newest(),
  name: 'read_file',
  arguments: JSON.stringify({ path: 'package.json' }),
});
check(String(selfRead?.result ?? '').includes('"name"'), 'fs 读取自身源码 package.json（自维护路径放大）');
// 工作区外仍被拒（新边界 = 项目根）
const outRead = await invoke('harness_fs_read', { path: 'C:/Windows/System32/drivers/etc/hosts' });
check(typeof outRead === 'object' && outRead.__err, '项目根之外仍被沙箱拒绝');

// ─── 2) 会话维护工具注册 ───
const tools = await invoke('get_harness_tools');
const tnames = (tools ?? []).map((x) => x.name);
for (const n of ['session_list', 'session_create', 'session_rename', 'session_clear', 'session_delete']) {
  check(tnames.includes(n), `工具目录含 ${n}`);
}

// ─── 3) UI：会话侧栏「清空聊天记录」按钮 ───
const clearBtn = await ev(`(() => {
  const b = [...document.querySelectorAll('.hns-session-act')].find((x) => (x.title || '').includes('清空聊天记录'));
  return b ? 'true' : 'false';
})()`);
check(clearBtn === 'true', '会话侧栏有「清空聊天记录」按钮');

// ─── 4) 模型驱动：让模型清空当前会话聊天记录 ───
// 先制造历史消息。种子消息经 UI 发送进「当前选中会话」，其不一定是列表
// 最新/最旧——用显示投影扫描定位实际会话，保证后续断言针对同一会话。
const seedKey = 'PRE_SEED_' + Date.now().toString(36);
const seedPrompt = '请只回复：' + seedKey;
await ev(`(() => {
  const ta = document.querySelector('.hns-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, ${JSON.stringify(seedPrompt)});
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'typed';
})()`);
await sleep(300);
await ev(`(() => { const ta = document.querySelector('.hns-input textarea'); ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); return 'sent'; })()`);
let seeded = false;
for (let i = 0; i < 120; i++) {
  await sleep(1000);
  const text = await ev(`(() => [...document.querySelectorAll('.hns-msg-bot')].map((x) => x.textContent).join('\\n'))()`);
  if (String(text).includes(seedKey)) { seeded = true; break; }
}
check(seeded, '预置历史消息完成');
// 定位种子消息所在会话（UI 当前选中 = 回复出现处）
const _all = await invoke('harness_list_sessions');
let sid = null;
for (const s of _all) {
  const msgs = await invoke('harness_display_messages', { id: s.id });
  if (JSON.stringify(msgs ?? []).includes(seedKey)) { sid = s.id; break; }
}
if (!sid) sid = _all.reduce((a, b) => (a.created_at >= b.created_at ? a : b)).id;
console.log('CHAT_SID=' + sid);

// 让模型调用 session_clear 清空当前会话
const askKey = 'CLEAR_OK_' + Date.now().toString(36);
await ev(`(() => {
  const ta = document.querySelector('.hns-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, '请调用 session_clear 工具清空当前会话的聊天记录，清空完成后只回复：${askKey}');
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'typed';
})()`);
await sleep(300);
await ev(`(() => { const ta = document.querySelector('.hns-input textarea'); ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); return 'sent'; })()`);
let cleared = false;
for (let i = 0; i < 150; i++) {
  await sleep(1000);
  const text = await ev(`(() => [...document.querySelectorAll('.hns-msg-bot')].map((x) => x.textContent).join('\\n'))()`);
  if (String(text).includes(askKey)) { cleared = true; break; }
}
check(cleared, '模型完成清空并回复');
await sleep(1200);

// 验证：预置消息已从日志消失；清空事件落日志；UI 消息列表同步
const msgs = await invoke('harness_display_messages', { id: sid });
const allText = JSON.stringify(msgs ?? []);
check(!allText.includes(seedKey), '预置历史消息已清空（日志投影无残留）');
const events = await invoke('harness_session_events', { id: sid, afterSeq: 0 });
const hasCleared = (events ?? []).some(([, e2]) => e2?.type === 'session_cleared');
check(hasCleared, '清空事件已落日志（session_cleared）');
const uiText = await ev(`(() => [...document.querySelectorAll('.hns-msg')].map((x) => x.textContent).join('\\n'))()`);
check(!String(uiText).includes(seedKey), 'UI 消息列表已同步（无预置残留）');
check(String(uiText).includes(askKey), 'UI 显示清空确认回复');

console.log(fails === 0 ? 'ALL_PASS' : `FAILURES=${fails}`);
process.exit(fails === 0 ? 0 : 1);
