// 诊断：当前会话最新回合的工具步骤状态 + 最近事件
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
const ev = (expression) => new Promise((resolve, reject) => { pend.set(++id, { resolve, reject }); ws.send(JSON.stringify({ id, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); });
const invoke = (cmd, args = {}) => ev(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`).catch((e) => ({ __err: String(e) }));

const stepsRes = await ev(`JSON.stringify([...document.querySelectorAll('.hns-tool-step')].map((s) => ({ cls: s.className, name: s.querySelector('.hns-tool-name')?.textContent?.trim(), status: s.querySelector('.hns-tool-status')?.textContent?.trim() })))`);
console.log('STEPS=' + stepsRes?.result?.value);
const sessionsRes = await invoke('harness_list_sessions');
const sessions = sessionsRes?.result?.value ?? sessionsRes;
const sid = sessions?.[0]?.id;
console.log('SID=' + sid);
const eventsRes = await invoke('harness_session_events', { id: sid, afterSeq: 0 });
const events = eventsRes?.result?.value ?? eventsRes;
const toolCalls = (events ?? []).filter(([, e2]) => e2?.type === 'assistant_tool_calls' || e2?.type === 'tool_result').slice(-8);
for (const [seq, e2] of toolCalls) {
  console.log(`#${seq} ${JSON.stringify(e2).slice(0, 180)}`);
}
process.exit(0);
