// 验证 + 截图：工具执行时间线（先工具 → 后回复）+ 实时回合合并
import { writeFileSync, mkdirSync } from 'node:fs';
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
const OUT = 'C:/Users/28361/Desktop/ST/st_control/data/ui-audit/redesign';
mkdirSync(OUT, { recursive: true });
const shot = async (name) => {
  const s = await send('Page.captureScreenshot', { format: 'png', fromSurface: true });
  const f = `${OUT}/${name}.png`;
  writeFileSync(f, Buffer.from(s.data, 'base64'));
  console.log('SAVED=' + f);
};

await ev(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((x) => x.title === 'Harness'); if (b) b.click(); return 'true'; })()`);
await sleep(2500);
const ready = await ev(`(async () => {
  for (let i = 0; i < 30; i++) {
    if (document.querySelector('.hns-input textarea')) return 'true';
    await new Promise((r) => setTimeout(r, 500));
  }
  return 'false';
})()`);
check(ready === 'true', '输入区就绪');
// 新建会话：避免上一个探针的会话/回复污染（探针自包含）
await ev(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(900);

// 发送需要工具调用的回合（get_current_time 免审批确定性工具）
const KEY = 'TL_' + Date.now().toString(36);
await ev(`(() => {
  const ta = document.querySelector('.hns-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, '请先调用 get_current_time 工具获取当前时间，再调用一次获取同一时间，然后告诉我现在几点，最后一行输出 ${KEY}');
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'typed';
})()`);
await sleep(300);
await ev(`(() => { const ta = document.querySelector('.hns-input textarea'); ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); return 'sent'; })()`);

// 等待回复完成
let reply = '';
for (let i = 0; i < 180; i++) {
  await sleep(1000);
  const text = await ev(`(() => [...document.querySelectorAll('.hns-msg-bot')].map((x) => x.textContent).join('\\n'))()`);
  if (String(text).includes(KEY)) { reply = String(text); break; }
}
check(reply.includes(KEY), '回复完成（含关键词）');
await sleep(1200);
await shot('07-tool-timeline');

// 结构断言：最新回合的时间线在气泡上方（DOM 顺序）
const order = await ev(`(() => {
  const cols = [...document.querySelectorAll('.hns-msg-bot .hns-bot-col')];
  const col = cols[cols.length - 1];
  if (!col) return 'no-col';
  return JSON.stringify([...col.children].map((k) => k.className.split(' ')[0]));
})()`);
check(order.includes('hns-tool-timeline'), `回合结构含时间线（${order}）`);
const orderArr = order !== 'no-col' ? JSON.parse(order) : [];
const tlPos = orderArr.indexOf('hns-tool-timeline');
const bubblePos = orderArr.indexOf('hns-bubble');
check(tlPos >= 0 && bubblePos > tlPos, '时间线位于回复气泡上方（先工具后回复）');
// 工具步骤状态：时间线步骤有完成与失败状态（重试序列如实展示）
const stepOk = await ev(`(() => {
  const steps = [...document.querySelectorAll('.hns-tool-step')];
  return steps.length >= 1 && steps.some((s) => s.classList.contains('ok')) ? 'true' : 'false';
})()`);
check(stepOk === 'true', '工具步骤状态展示（含完成）');
const stepErr = await ev(`(() => {
  const steps = [...document.querySelectorAll('.hns-tool-step')];
  return steps.some((s) => s.classList.contains('err')) ? 'true' : 'false';
})()`);
// 失败状态节点依赖模型真实触发失败工具（本次提示词仅确定性 get_current_time，
// 通常全部成功）——失败时如实展示即可，不硬性要求
console.log('STEP_ERR=' + stepErr + (stepErr === 'true' ? '（失败重试序列可见）' : '（本次回合无失败步骤，状态节点=完成）'));
if (stepErr === 'true') check(true, '失败步骤以红色状态节点展示（重试序列可见）');
const expandable = await ev(`(() => {
  const head = document.querySelector('.hns-tool-head');
  if (!head) return 'false';
  head.click();
  return new Promise((res) => setTimeout(() => {
    // 改版后详情区 = .hns-tool-detail（内嵌 ToolCard），不再有 .hns-tool-pre
    const d = document.querySelector('.hns-tool-detail');
    res(d && d.textContent.trim().length > 0 ? 'true' : 'false');
  }, 300));
})()`);
check(expandable === 'true', '时间线步骤可展开详情');
await ev(`(() => { const h = document.querySelector('.hns-tool-head'); if (h) h.click(); return 'true'; })()`);

// 实时回合合并：ask_user_question（免审批、阻塞等待用户回答，
// 「执行中」窗口稳定可捕获）
await ev(`(() => {
  const ta = document.querySelector('.hns-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, '请调用 ask_user_question 工具，询问用户是否继续，选项「继续」「停止」；用户回答后回复 OK_LIVE');
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'typed';
})()`);
await sleep(300);
await ev(`(() => { const ta = document.querySelector('.hns-input textarea'); ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); return 'sent'; })()`);
// 问题卡出现 = ask_user_question 正在执行 → 采样「时间线 + 气泡 + 执行中」同容器
const liveMerged = await ev(`(async () => {
  for (let i = 0; i < 200; i++) {
    const bots = [...document.querySelectorAll('.hns-msg-bot')];
    const last = bots[bots.length - 1];
    if (last && last.querySelector('.hns-tool-timeline') && last.querySelector('.hns-bubble')) {
      if (last.querySelector('.hns-tool-running')) return 'merged-running';
      return 'merged';
    }
    await new Promise((r) => setTimeout(r, 120));
  }
  return 'false';
})()`);
check(liveMerged === 'merged-running' || liveMerged === 'merged', `实时回合：工具时间线与回复气泡同一容器（${liveMerged}）`);
if (liveMerged === 'merged-running') await shot('08-live-turn');
// 回答问题卡（选「继续」）
await ev(`(async () => {
  for (let i = 0; i < 60; i++) {
    const btn = [...document.querySelectorAll('.hns-approve')].find((b) => b.textContent.trim() === '继续');
    if (btn) { btn.click(); return 'answered'; }
    await new Promise((r) => setTimeout(r, 300));
  }
  return 'no-btn';
})()`);
// 等完成
let liveDone = false;
for (let i = 0; i < 120; i++) {
  await sleep(1000);
  const text = await ev(`(() => [...document.querySelectorAll('.hns-msg-bot')].map((x) => x.textContent).join('\\n'))()`);
  if (String(text).includes('OK_LIVE')) { liveDone = true; break; }
}
check(liveDone, '实时回合完成后回复落消息列表');

console.log(fails === 0 ? 'ALL_PASS' : `FAILURES=${fails}`);
process.exit(fails === 0 ? 0 : 1);
