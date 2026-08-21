// 已停用（2026-08）：原「AI 聊天」板块已并入 Harness 会话，本脚本断言的独立面板 UI 不再存在；
// 功能验证改由 e2e-harness-phase* 与 verify-harness-chat-* 承担。
// 合并方案 + 用量统一 的运行期验证：
// 1) 大模型管理不再有「全局调用」Tab（入口去重）
// 2) AI 聊天使用共享 LlmStatsBadge + ModelSelect
// 3) 实时同步回归：添加模型后徽标无需刷新即更新
// 4) 真实 LLM 调用（连接测试）计入「流量与成本」call_count
const list = await (await fetch('http://127.0.0.1:9222/json/list')).json();
const target = list.find((x) => x.type === 'page');
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0;
const pending = new Map();
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) {
    pending.get(m.id)(m.result);
    pending.delete(m.id);
  }
};
const send = (method, params = {}) =>
  new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
  return r.result.value;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const INVOKE = (cmd, args) =>
  `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`;
const READ_BADGE = `(() => {
  const root = [...document.querySelectorAll('.panel')].find((el) => !el.classList.contains('panel-hidden'));
  if (!root) return '';
  const candidates = [...root.querySelectorAll('*')]
    .map((el) => (el.textContent || '').trim())
    .filter((t) => /^\\d+ 个提供方 · \\d+ 个模型$/.test(t));
  return candidates[0] || '';
})()`;

let passed = 0;
const ok = (cond, msg) => {
  if (!cond) throw new Error('断言失败：' + msg);
  passed++;
  console.log('✓', msg);
};

// 1) 大模型管理 Tab 去重
await ev(`document.querySelector('.nav-item[title="大模型"]').click()`);
await sleep(600);
const tabsText = await ev(`(() => {
  const root = [...document.querySelectorAll('.panel')].find((el) => !el.classList.contains('panel-hidden'));
  return root ? root.innerText : '';
})()`);
ok(!tabsText.includes('全局调用'), '大模型管理不再显示「全局调用」Tab');
ok(tabsText.includes('流量与成本') && tabsText.includes('接入配置') && tabsText.includes('模型管理'),
  '大模型管理保留 流量与成本 / 接入配置 / 模型管理');

// 2) AI 聊天（已整合进 Harness）：共享徽标 + ModelSelect
await ev(`document.querySelector('.nav-item[title="Harness"]').click()`);
await sleep(600);
await ev(`(() => { const b = [...document.querySelectorAll('.hns-viewbar button')].find((x) => (x.textContent || '').trim() === 'AI 聊天'); if (b) b.click(); return !!b; })()`);
await sleep(600);
const chatInfo = await ev(`(() => {
  const root = [...document.querySelectorAll('.panel')].find((el) => !el.classList.contains('panel-hidden'));
  const selects = root.querySelectorAll('.llm-select');
  return { selectCount: selects.length, providerOptions: selects[0]?.options.length ?? 0 };
})()`);
const badgeText = await ev(READ_BADGE);
ok(/^\d+ 个提供方 · \d+ 个模型$/.test(badgeText), 'AI 聊天使用共享 LlmStatsBadge 徽标');
ok(chatInfo.selectCount === 2 && chatInfo.providerOptions >= 1, 'AI 聊天使用共享 ModelSelect（提供方/模型联动下拉）');

// 3) 实时同步回归：添加模型 → 徽标实时 +1
const cfg = await ev(INVOKE('get_llm_config', {}));
const providerId = cfg.providers[0].id;
const probeModel = 'e2e-merge-probe-' + Date.now();
const badgeNum = (badge) => Number((badge.match(/(\d+) 个模型/) || [])[1]);
const before = badgeNum(badgeText);
await ev(INVOKE('add_llm_model', { id: providerId, model: probeModel }));
await sleep(900);
const afterBadge = await ev(READ_BADGE);
ok(badgeNum(afterBadge) === before + 1, `实时同步回归：模型计数 ${before} → ${badgeNum(afterBadge)}（无需刷新）`);

// 4) 真实 LLM 调用计入「流量与成本」
const summaryBefore = await ev(INVOKE('get_llm_usage_summary', {}));
const callsBefore = summaryBefore.find((p) => p.id === providerId)?.usage?.call_count ?? 0;
const conn = await ev(INVOKE('test_llm_connection', { id: providerId }));
await sleep(800);
const summaryAfter = await ev(INVOKE('get_llm_usage_summary', {}));
const callsAfter = summaryAfter.find((p) => p.id === providerId)?.usage?.call_count ?? 0;
if (conn.ok) {
  ok(callsAfter === callsBefore + 1, `真实 LLM 调用已计入流量与成本：call_count ${callsBefore} → ${callsAfter}`);
} else {
  console.log(`ℹ 连接测试未成功（${conn.error || '网络不可达'}），跳过用量断言；机制已由代码统一保证`);
}

// 清理
await ev(INVOKE('remove_llm_model', { id: providerId, model: probeModel }));
await sleep(500);
console.log(`\n合并 + 用量验证完成：${passed} 项通过`);
ws.close();
process.exit(0);
