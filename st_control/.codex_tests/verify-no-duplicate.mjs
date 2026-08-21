// 验证：修复助手消息重复显示（每条回复只投影一条）
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
// 新建会话：避免上一个探针的会话/回复污染（探针自包含）
await ev(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(900);

// 发送一轮，等待回复
const KEY = 'DUP_' + Date.now().toString(36);
await ev(`(() => {
  const ta = document.querySelector('.hns-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, '请只回复：${KEY}');
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'typed';
})()`);
await sleep(300);
const sent = await ev(`(() => { const b = document.querySelector('.hns-send'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
let reply = '';
for (let i = 0; i < 150; i++) {
  await sleep(1000);
  const text = await ev(`(() => [...document.querySelectorAll('.hns-msg-bot')].map((x) => x.textContent).join('\\n'))()`);
  if (String(text).includes(KEY)) { reply = String(text); break; }
  // 模型未逐字回显关键词：记录收到的任意回复（收尾校验用）
  if (String(text).length > 0 && !reply) reply = String(text).slice(0, 300);
}
// 发送未触发（按钮不可用）或模型无回复：重试一次
if (!reply && !sent) {
  await ev(`(() => { const b = document.querySelector('.hns-send'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
  for (let i = 0; i < 150; i++) {
    await sleep(1000);
    const text = await ev(`(() => [...document.querySelectorAll('.hns-msg-bot')].map((x) => x.textContent).join('\\n'))()`);
    if (String(text).includes(KEY)) { reply = String(text); break; }
    if (String(text).length > 0 && !reply) reply = String(text).slice(0, 300);
  }
}
check(reply.length > 0, `收到回复${reply.includes(KEY) ? '（含关键词）' : '（模型未逐字回显）'}`);

// 去重校验片段：优先关键词；否则用回复尾段（唯一性足够）
const dedupKey = reply.includes(KEY) ? KEY : reply.slice(-30).trim() || reply;

// 1) UI 消息列表：回复只出现一次
const uiCount = await ev(`(() => [...document.querySelectorAll('.hns-msg-bot')].filter((x) => x.textContent.includes(${JSON.stringify(dedupKey)})).length)()`);
check(uiCount === 1, `UI 中回复仅一条（实际 ${uiCount}）`);

// 2) 日志投影：assistant 消息只有一条（取最新会话 = 本次聊天会话）
const _sess = await invoke('harness_list_sessions');
const sid = _sess.reduce((a, b) => (a.created_at >= b.created_at ? a : b)).id;
const msgs = await invoke('harness_display_messages', { id: sid });
const ass = (msgs ?? []).filter((m) => m.role === 'assistant' && String(m.content ?? '').includes(dedupKey));
check(ass.length === 1, `日志投影 assistant 消息仅一条（实际 ${ass.length}）`);

// 3) 整页重载后回放仍只有一条（修复后重载路径）
await send('Page.reload', { ignoreCache: true });
await sleep(5000);
await ev(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((x) => x.title === 'Harness'); if (b) b.click(); return 'true'; })()`);
await sleep(2500);
// 重载后选中本次聊天会话（列表按 order_index 升序；会话标题 = 首条用户
// 消息（含 KEY），故用 KEY 定位；回复内容校验用 dedupKey）
await ev(`(() => {
  const items = [...document.querySelectorAll('.hns-session')];
  const el = items.find((x) => x.textContent.includes(${JSON.stringify(KEY)})) || document.querySelector('.hns-session');
  if (el) { el.click(); return 'true'; }
  return 'false';
})()`);
await sleep(1000);
const replayCount = await ev(`(() => {
  const root = [...document.querySelectorAll('.panel')].find((el) => !el.classList.contains('panel-hidden'));
  return root ? [...root.querySelectorAll('.hns-msg-bot')].filter((x) => x.textContent.includes(${JSON.stringify(dedupKey)})).length : -1;
})()`);
check(replayCount === 1, `整页重载回放仍仅一条（实际 ${replayCount}）`);

console.log(fails === 0 ? 'ALL_PASS' : `FAILURES=${fails}`);
process.exit(fails === 0 ? 0 : 1);
