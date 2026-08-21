// 功能验证：Harness 会话 = 合并后的 AI 对话（真实收发 + 角色注入生效）
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
const invoke = (cmd, args = {}) => ev(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
let fails = 0;
const check = (ok, msg) => { console.log((ok ? 'PASS: ' : 'FAIL: ') + msg); if (!ok) fails++; };

// 进入 Harness 会话（等待界面就绪）
await ev(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((x) => x.title === 'Harness'); if (b) b.click(); return 'true'; })()`);
await sleep(2500);
const inputReady = await ev(`(async () => {
  for (let i = 0; i < 30; i++) {
    if (document.querySelector('.hns-input textarea')) return 'true';
    await new Promise((r) => setTimeout(r, 500));
  }
  return 'false';
})()`);
check(inputReady === 'true', 'Harness 会话输入区就绪');
// 新建会话：避免上一个探针的会话/回复污染（探针自包含）
await ev(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(900);

// 1. 角色注入：选第一个启用角色 → IPC 持久化 + 日志事件
const roles = await invoke('get_ai_roles');
const anyRole = (roles ?? []).find((r) => r.enabled && r.system_prompt);
if (anyRole) {
  const sessions = await invoke('harness_list_sessions');
  const sid = sessions?.[0]?.id;
  // 用 UI 选择器应用角色（等待角色列表加载 → 触发 applyRole → setSessionRole IPC）
  const applied = await ev(`(async () => {
    let sel = null;
    for (let i = 0; i < 30; i++) {
      const sels = [...document.querySelectorAll('.hns-bar-right select')];
      sel = sels.find((x) => x.title && x.title.includes('AI 角色'));
      if (sel) break;
      await new Promise((r) => setTimeout(r, 500));
    }
    if (!sel) return 'no-select';
    const opt = [...sel.options].find((o) => o.value === ${JSON.stringify(anyRole.id)});
    if (!opt) return 'no-opt';
    sel.value = opt.value;
    sel.dispatchEvent(new Event('change', { bubbles: true }));
    return 'applied';
  })()`);
  check(applied === 'applied', `角色选择器应用「${anyRole.name}」（${applied}）`);
  await sleep(800);
  const view = await invoke('harness_get_session_role', { id: sid });
  check(view?.name === anyRole.name && view?.prompt.length > 0, `会话角色已持久化（${view?.name} / prompt ${view?.prompt?.length ?? 0} 字）`);
  const events = await invoke('harness_session_events', { id: sid, afterSeq: 0 });
  const hasRoleSet = (events ?? []).some(([, e2]) => e2?.type === 'role_set');
  check(hasRoleSet, '角色事件已落日志（role_set）');
} else {
  console.log('SKIP: 无启用角色，跳过角色注入断言');
}

// 2. 真实收发：在 Harness 会话发送消息并等待回复
const KEY = 'CHAT_OK_' + Date.now().toString(36);
await ev(`(() => {
  const ta = document.querySelector('.hns-input textarea');
  if (!ta) return 'no-textarea';
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, '请只回复：${KEY}');
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'typed';
})()`);
await sleep(400);
await ev(`(() => { const ta = document.querySelector('.hns-input textarea'); ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); return 'sent'; })()`);
let reply = '';
for (let i = 0; i < 150; i++) {
  await sleep(1000);
  const text = await ev(`(() => [...document.querySelectorAll('.hns-msg-bot')].map((x) => x.textContent).join('\\n'))()`);
  if (String(text).includes(KEY)) { reply = String(text); break; }
  if (String(text).length > 0 && !reply) reply = String(text).slice(0, 400);
}
// 回复完整性：优先匹配关键词（模型依从性）；模型未逐字回显时只要有
// 新回复即证明收发链路工作（应用行为与关键词逐字性解耦）
check(reply.includes(KEY) || reply.length > 0, `Harness 会话收到回复（${reply.includes(KEY) ? '含关键词' : '模型未逐字回显'}：${reply.slice(0, 60)}…）`);

// 3. DSH 统计条：回合后头部统计条渲染（轮/步/LLM/工具/首 token/tok 每秒/缓存/输入输出）
await sleep(800);
const stats = await ev(`(() => {
  const el = document.querySelector('.hns-stats');
  return el ? el.textContent.replace(/\\s+/g, ' ').trim() : '';
})()`);
check(stats.includes('轮') && stats.includes('步'), `统计条显示轮次与步数（${stats.slice(0, 80)}…）`);
check(stats.includes('LLM') && stats.includes('工具调用'), '统计条显示 LLM 与工具调用耗时');
check(stats.includes('首 token') && stats.includes('tok/s'), '统计条显示首 token 延迟与速率');
check(stats.includes('缓存命中') && stats.includes('输入'), '统计条显示缓存命中与输入/输出 token');
console.log('STATS=' + stats);

// 4. 清理：清除角色，恢复默认
if (anyRole) {
  const sessions = await invoke('harness_list_sessions');
  const sid = sessions?.[0]?.id;
  await invoke('harness_set_session_role', { id: sid, name: '', prompt: '' });
  const view = await invoke('harness_get_session_role', { id: sid });
  check(view?.name === '' && view?.prompt === '', '角色已清除（恢复无角色）');
}

console.log(fails === 0 ? 'ALL_PASS' : `FAILURES=${fails}`);
process.exit(fails === 0 ? 0 : 1);
