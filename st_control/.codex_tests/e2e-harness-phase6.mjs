// E2E：Harness 阶段 6（sdk JSON-RPC / compaction / attachment / mcp）
// 前置：app 运行中（CDP 9222）+ Vite 1420。
const CDP_BASE = 'http://127.0.0.1:9222';
const SDK_BASE = 'http://127.0.0.1:4770';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function findTarget() {
  for (let i = 0; i < 40; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
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
  async waitFor(expression, timeoutMs = 120000, stepMs = 250) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        const v = await this.eval(expression);
        if (v && v !== 'false' && v !== 'null' && v !== 'undefined') return v;
      } catch {}
      await sleep(stepMs);
    }
    return null;
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
  cdp.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`)
    .catch((e) => ({ __err: String(e) }));
async function rpc(method, params = {}) {
  try {
    const res = await fetch(`${SDK_BASE}/rpc`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
    });
    return await res.json();
  } catch (e) {
    return { __err: String(e) };
  }
}

// ─── 1) SDK / JSON-RPC ───
const health = await (await fetch(`${SDK_BASE}/health`)).text();
check(health === 'ok', 'SDK 健康检查（127.0.0.1:4770）');
const listRes = await rpc('sessions.list');
check(Array.isArray(listRes.result), 'SDK sessions.list 返回数组');
const createRes = await rpc('session.create');
const sdkSid = createRes.result?.id;
check(!!sdkSid && sdkSid.startsWith('h-'), `SDK session.create（${sdkSid}）`);
const chatRes = await rpc('session.chat', { session_id: sdkSid, content: '请只回复：SDK_TURN_OK' });
console.log('SDK_CHAT=' + JSON.stringify(chatRes).slice(0, 200));
check(
  (chatRes.result?.content ?? '').includes('SDK_TURN_OK'),
  `SDK session.chat 返回最终回答（${String(chatRes.result?.content).slice(0, 60)}）`,
);
const dispRes = await rpc('session.display', { session_id: sdkSid });
check(Array.isArray(dispRes.result) && dispRes.result.length >= 2, 'SDK session.display 投影消息');

// ─── 2) compaction：低预算 + 长历史触发压缩事件 ───
await invoke('save_harness_settings', {
  settings: {
    last_provider_id: '', last_model: '', tool_timeout_secs: null, max_agent_rounds: null,
    preset_id: null, allow_workspace_escape: false,
    context_budget_tokens: 4000, enable_compaction: true,
  },
});
// 用 SDK 连续写入长消息轮次，使历史超过预算
const longText = '这是一段用于撑大上下文的重复文本。'.repeat(300); // ~5400 字 ≈ 2700 token
await rpc('session.chat', { session_id: sdkSid, content: longText });
await rpc('session.chat', { session_id: sdkSid, content: longText + ' 第二段' });
await rpc('session.chat', { session_id: sdkSid, content: '请只回复：COMPACT_TURN_OK' });
const events = await invoke('harness_session_events', { id: sdkSid, afterSeq: 0 });
const compactEvt = events.find(([, ev]) => ev.type === 'compaction');
console.log('COMPACT_EVT=' + JSON.stringify(compactEvt ?? null).slice(0, 200));
check(!!compactEvt && compactEvt[1].removed_messages > 0, `compaction 事件落日志（移除 ${compactEvt?.[1]?.removed_messages} 条）`);
await invoke('save_harness_settings', {
  settings: {
    last_provider_id: '', last_model: '', tool_timeout_secs: null, max_agent_rounds: null,
    preset_id: null, allow_workspace_escape: false,
    context_budget_tokens: null, enable_compaction: true,
  },
});

// ─── 3) attachment：附加文件 + 投影 ───
const shWrite = await invoke('harness_shell_run', {
  command: "Set-Content -Path 'att-e2e.txt' -Value '附件内容：E2E 测试'",
});
check(shWrite.ok === true, '工作区写入附件源文件');
const att = await invoke('harness_attach_file', {
  sessionId: sdkSid,
  // 相对路径：解析到应用 CWD = 工作区根（真实环境 = 项目根，
  // 隔离环境 = .e2e/app），环境无关
  sourcePath: 'att-e2e.txt',
});
check(!!att.id && att.kind === 'text', `附件已附加（${att.name} / ${att.kind}）`);
const atts = await invoke('harness_list_attachments', { sessionId: sdkSid });
check(Array.isArray(atts) && atts.some((a) => a.name === 'att-e2e.txt'), '附件列表投影（日志同源）');
await invoke('harness_fs_delete', { path: 'att-e2e.txt' });

// ─── 4) MCP：外部服务器工具注册 + 调用 ───
const mcpRes = await invoke('save_harness_mcp_servers', {
  servers: [{
    id: 'echo1', name: 'Echo 服务器', enabled: true,
    command: 'powershell.exe',
    args: ['-NoProfile', '-File', 'C:\\Users\\28361\\Desktop\\ST\\st_control\\.codex_tests\\mcp-echo-server.ps1'],
  }],
});
check(Array.isArray(mcpRes) && mcpRes.length === 1, 'MCP 服务器配置保存（工具注册刷新）');
const tools = await invoke('get_harness_tools');
const mcpTool = (tools ?? []).find((t) => t.name === 'mcp_echo1_echo');
check(!!mcpTool, `MCP 工具注册进 Harness 目录（${mcpTool?.name ?? '缺失'}）`);
const mcpCall = await invoke('harness_execute_tool', {
  sessionId: sdkSid,
  name: 'mcp_echo1_echo',
  arguments: JSON.stringify({ text: 'hello-mcp' }),
});
console.log('MCP_CALL=' + JSON.stringify(mcpCall).slice(0, 200));
check(mcpCall.ok === true && (mcpCall.result ?? '').includes('echo:hello-mcp'), 'MCP 工具调用（echo:hello-mcp 回显）');
await invoke('save_harness_mcp_servers', { servers: [] });

// ─── 5) 清理 + 截图 ───
await invoke('harness_delete_session', { id: sdkSid });
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../data/ui-audit/llm-harness-phase6.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
