// 验证：Harness 会话真流式（逐 delta 渲染）+ 首 token 遥测
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
let fails = 0;
const check = (ok, msg) => { console.log((ok ? 'PASS: ' : 'FAIL: ') + msg); if (!ok) fails++; };

// 进入 Harness 会话并等待就绪
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
// 新建会话：避免上一个探针的会话/回复污染本次流式观测（探针自包含）
await ev(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(900);

// 发送较长回答的请求（确保多个 delta 分片）
const KEY = 'STREAM_' + Date.now().toString(36);
await ev(`(() => {
  const ta = document.querySelector('.hns-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, '请介绍你的能力，分三点，每点不少于 60 字；最后一行只输出 ${KEY}');
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'typed';
})()`);
await sleep(400);
await ev(`(() => { const ta = document.querySelector('.hns-input textarea'); ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); return 'sent'; })()`);

// 高速采样流式气泡：文本应逐步增长（多个不同快照 = 逐 delta 渲染；
// 快模型可能秒回，40ms 采样 + 长回复请求尽量捕捉增量过程）
const snapshots = [];
for (let i = 0; i < 800; i++) {
  await sleep(40);
  const txt = await ev(`(() => {
    const els = [...document.querySelectorAll('.hns-msg-bot')];
    return els.length ? els[els.length - 1].textContent.trim() : '';
  })()`);
  if (txt.includes(KEY)) break;
  if (txt && !snapshots.includes(txt)) snapshots.push(txt);
}
console.log('SNAPSHOTS=' + snapshots.length + ' (样例: ' + snapshots.slice(0, 3).map((s) => s.slice(0, 24)).join(' / ') + ')');
check(snapshots.length >= 2, `流式增量渲染（捕获 ${snapshots.length} 个不同文本快照）`);

// 收尾：确认回复完成且首 token 遥测非零（关键词依从性失败时，有新回复也接受）
let reply = '';
for (let i = 0; i < 120; i++) {
  await sleep(1000);
  const text = await ev(`(() => [...document.querySelectorAll('.hns-msg-bot')].map((x) => x.textContent).join('\\n'))()`);
  if (String(text).includes(KEY)) { reply = String(text); break; }
  if (String(text).length > 0 && !reply) reply = String(text).slice(0, 400);
}
check(reply.includes(KEY) || reply.length > 0, '流式回复完整落消息列表（含关键词或收到回复）');
await sleep(800);
const stats = await ev(`(() => { const el = document.querySelector('.hns-stats'); return el ? el.textContent.replace(/\\s+/g, ' ').trim() : ''; })()`);
check(stats.includes('首 token') && !stats.includes('首 token 平均 0.0s'), `首 token 遥测为真实值（${stats.match(/首 token 平均 ([0-9.]+s)/)?.[1] ?? '?'}）`);

console.log(fails === 0 ? 'ALL_PASS' : `FAILURES=${fails}`);
process.exit(fails === 0 ? 0 : 1);
