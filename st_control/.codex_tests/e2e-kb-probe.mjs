// 验证：后端广播后，kb_list_models（知识库/智能体模型下拉的数据源）
// 立即包含新添加的模型。
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

const cfg = await ev(`window.__TAURI_INTERNALS__.invoke('get_llm_config')`);
const providerId = cfg.providers[0].id;
const probeModel = 'e2e-kb-probe-' + Date.now();
const INVOKE = (cmd, args) =>
  `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`;

// 1) 变更前：kb_list_models 不含探针模型
let before = await ev(INVOKE('kb_list_models', {}));
const beforeHas = Array.isArray(before) && before.some((m) => m.model === probeModel);
console.log('变更前 kb_list_models 不含探针模型：', !beforeHas ? '✓' : '✗');
if (beforeHas) process.exit(1);

// 2) 后端添加模型（触发广播）
await ev(INVOKE('add_llm_model', { id: providerId, model: probeModel }));

// 3) 变更后：立即（不刷新任何界面）查询数据源
await new Promise((r) => setTimeout(r, 500));
let after = await ev(INVOKE('kb_list_models', {}));
const afterHas = Array.isArray(after) && after.some((m) => m.model === probeModel);
console.log('变更后 kb_list_models 立即包含新模型：', afterHas ? '✓' : '✗');
if (!afterHas) process.exit(1);

// 4) 清理
await ev(INVOKE('remove_llm_model', { id: providerId, model: probeModel }));
await new Promise((r) => setTimeout(r, 300));
after = await ev(INVOKE('kb_list_models', {}));
const cleaned = !Array.isArray(after) || !after.some((m) => m.model === probeModel);
console.log('删除探针模型后数据源还原：', cleaned ? '✓' : '✗');

ws.close();
process.exit(cleaned ? 0 : 1);
