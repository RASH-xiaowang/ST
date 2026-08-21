// E2E：Harness 阶段 9（credentials / lsp / acp 语义 / code-runtime 映射）
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

// ─── 1) credentials：存储 + .env 提供者 + 掩码 ───
const credKey = `E2E_TOKEN_${Date.now().toString(36).toUpperCase()}`;
await invoke('harness_credential_put', { key: credKey, value: 'super-secret-value-123456' });
const credList = await invoke('harness_credential_list');
const credView = (credList ?? []).find((c) => c.key === credKey);
check(!!credView && credView.masked.includes('*') && !credView.masked.includes('super-secret-value-123456'), `凭据掩码展示（${credView?.masked}）`);
await invoke('harness_credential_put', { key: 'E2E_ENV_TOKEN', value: 'env-secret', storeInEnv: true });
const envCred = (await invoke('harness_credential_list') ?? []).find((c) => c.key === 'E2E_ENV_TOKEN');
check(!!envCred, '.env 提供者凭据可见（掩码）');
// 子进程注入：钩子读取环境变量（turn_end 钩子输出凭据）
await invoke('save_harness_hooks', {
  hooks: [{
    id: 'hook-cred-e2e', event: 'turn_end', enabled: true,
    command: `Write-Output ("cred:" + $env:HARNESS_CREDENTIAL_${credKey})`,
  }],
});
// 触发一个回合
const s1 = await rpc('session.create');
await rpc('session.chat', { session_id: s1.result.id, content: '请只回复：CRED_TURN_OK' });
await sleep(2500);
// 子进程注入验证：shell 子进程读取 HARNESS_CREDENTIAL_<KEY>
const hookCheck = await invoke('harness_shell_run', {
  command: `if ($env:HARNESS_CREDENTIAL_${credKey}) { 'ENV_INJECT_OK' } else { 'ENV_MISSING' }`,
});
check(hookCheck.ok === true && String(hookCheck.output).includes('ENV_INJECT_OK'), '凭据注入子进程环境（shell 可读）');
await invoke('save_harness_hooks', { hooks: [] });
await invoke('harness_credential_delete', { key: credKey });
await invoke('harness_credential_delete', { key: 'E2E_ENV_TOKEN' });
const credGone = (await invoke('harness_credential_list') ?? []).some((c) => c.key === credKey);
check(!credGone, '凭据删除生效');

// ─── 2) lsp：PowerShell 测试服务器 + lsp_hover 工具 ───
await invoke('save_harness_lsp_servers', {
  servers: [{
    id: 'lsp-echo', name: 'Echo LSP', enabled: true,
    command: 'powershell.exe',
    args: ['-NoProfile', '-File', 'C:\\Users\\28361\\Desktop\\ST\\st_control\\.codex_tests\\lsp-echo-server.ps1'],
  }],
});
const lspList = await invoke('list_harness_lsp_servers');
check((lspList ?? []).some((s) => s.id === 'lsp-echo'), 'LSP 服务器已保存');
// lsp_hover 工具：经 harness_execute_tool 派发（需要有效会话）
const session = await invoke('harness_create_session');
const hoverRes = await invoke('harness_execute_tool', {
  sessionId: session.id,
  name: 'lsp_hover',
  arguments: JSON.stringify({ file: 'lsp-test.txt', line: 3, column: 5 }),
});
console.log('HOVER=' + JSON.stringify(hoverRes).slice(0, 200));
check(hoverRes.ok === true && String(hoverRes.result).includes('hover-info:line=3,col=5'), `LSP hover 查询回显（${String(hoverRes.result).slice(0, 60)}）`);
const tools9 = await invoke('get_harness_tools');
check((tools9 ?? []).some((t) => t.name === 'lsp_hover'), 'lsp_hover 工具注册进目录');
await invoke('save_harness_lsp_servers', { servers: [] });
// 未配置服务器时优雅报错
const hoverNone = await invoke('harness_execute_tool', {
  sessionId: session.id,
  name: 'lsp_hover',
  arguments: JSON.stringify({ file: 'x.txt', line: 0, column: 0 }),
});
check(hoverNone.ok === false && String(hoverNone.result).includes('未配置'), '未配置 LSP 时优雅报错');

// ─── 3) acp 语义（SDK） ───
const acpNew = await rpc('session/new', { goal: 'ACP 目标' });
check(!!acpNew.result?.id, `session/new 创建（${acpNew.result?.id}）`);
const acpPrompt = await rpc('session/prompt', {
  session_id: acpNew.result.id,
  prompt: '请只回复：ACP_OK',
});
console.log('ACP_PROMPT=' + JSON.stringify(acpPrompt).slice(0, 200));
check(
  acpPrompt.result?.stopReason === 'end_turn' && (acpPrompt.result?.content ?? '').includes('ACP_OK'),
  'session/prompt 返回 end_turn 与回答',
);
const acpState = await rpc('session.state', { session_id: acpNew.result.id });
check((acpState.result?.goal ?? '').includes('ACP 目标'), 'session/new 的 goal 已落状态');
const acpCancel = await rpc('session/cancel', { session_id: acpNew.result.id });
check(acpCancel.result?.cancelled === true, 'session/cancel 语义返回（cancelled + 中断说明）');
await invoke('harness_delete_session', { id: acpNew.result.id });
await invoke('harness_delete_session', { id: session.id });

// ─── 4) UI：凭据/LSP 标签 ───
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
await sleep(1200);
// 先关闭可能遗留的抽屉，再点击治理打开（触发 openDrawer 全量刷新）
await cdp.eval(`(() => { const c = document.querySelector('.hns-drawer-close'); if (c) { c.click(); return 'true'; } return 'false'; })()`);
await sleep(400);
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.hns-bar-icon')].find((x) => (x.title || '').includes('设置 / 钩子'));
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
await sleep(800);
const tabs = await cdp.eval(`(() => JSON.stringify([...document.querySelectorAll('.hns-drawer-tabs button')].map((x) => x.textContent.trim())))()`);
const tabList = JSON.parse(tabs);
check(tabList.includes('凭据') && tabList.includes('LSP'), `治理抽屉含凭据/LSP 标签（${tabs}）`);
// 关闭抽屉，避免影响后续探针的开关语义
await cdp.eval(`(() => { const c = document.querySelector('.hns-drawer-close'); if (c) { c.click(); return 'true'; } return 'false'; })()`);
await sleep(300);

// ─── 5) 截图 ───
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../data/ui-audit/llm-harness-phase9.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
