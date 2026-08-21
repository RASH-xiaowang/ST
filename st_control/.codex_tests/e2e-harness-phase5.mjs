// E2E：Harness 阶段 5（shell 能力 / fs 能力 / 终端会话 / 受限执行世界）
// 前置：app 运行中（CDP 9222）+ Vite 1420。
const CDP_BASE = 'http://127.0.0.1:9222';
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

// ─── 1) shell 能力 ───
const sh1 = await invoke('harness_shell_run', { command: 'Write-Output shell-phase5-ok' });
check(sh1.ok === true && (sh1.output ?? '').includes('shell-phase5-ok'), `shell 执行 echo（${sh1.output?.slice(0, 60)}）`);
// 受限执行世界：cwd 越界被拒绝
const sh2 = await invoke('harness_shell_run', { command: 'Get-Location', cwd: 'C:/Windows' });
check(sh2.ok === false && (sh2.output ?? '').includes('超出'), `受限世界拒绝工作区外 cwd（${sh2.output?.slice(0, 60)}）`);

// ─── 2) fs 能力（写→读→列→删） ───
const shWrite = await invoke('harness_shell_run', {
  command: "Set-Content -Path 'hfs-e2e.txt' -Value 'fs-roundtrip-ok'",
});
check(shWrite.ok === true, '工作区内写入文件（shell → fs 世界统一）');
const fsRead = await invoke('harness_fs_read', { path: 'hfs-e2e.txt' });
check(typeof fsRead === 'string' && fsRead.includes('fs-roundtrip-ok'), `fs 读取（${String(fsRead).slice(0, 60)}）`);
// 越界读取被拒绝
const fsEscape = await invoke('harness_fs_read', { path: 'C:/Windows/System32/drivers/etc/hosts' });
check(typeof fsEscape === 'object' && fsEscape.__err, 'fs 越界读取被拒绝');
await invoke('harness_fs_delete', { path: 'hfs-e2e.txt' });
const fsGone = await invoke('harness_fs_read', { path: 'hfs-e2e.txt' });
check(typeof fsGone === 'object' && fsGone.__err, 'fs 删除生效');

// ─── 3) 终端会话（cwd 状态保持；默认工作区 = 应用项目根） ───
const term = await invoke('create_harness_terminal', { name: 'E2E终端' });
check(!!term.id && term.id.startsWith('term-'), `终端创建（${term.id}）`);
const t1 = await invoke('harness_terminal_send', { id: term.id, input: 'Get-Location' });
check(typeof t1 === 'string' && t1.includes('st_control'), `终端初始 cwd = 工作区（项目根）（${t1.slice(0, 60)}）`);
await invoke('harness_terminal_send', {
  id: term.id,
  input: "New-Item -ItemType Directory -Name t1 -Force | Out-Null; Set-Location t1",
});
const t2 = await invoke('harness_terminal_send', { id: term.id, input: 'Get-Location' });
check(typeof t2 === 'string' && t2.includes('t1'), `终端 cwd 状态保持（Set-Location t1 → ${t2.slice(0, 80)}）`);
const logs = await invoke('harness_terminal_logs', { id: term.id });
check(Array.isArray(logs) && logs.length >= 3, `终端日志记录（${logs?.length} 条）`);
const terms = await invoke('list_harness_terminals');
check(terms.some((t) => t.id === term.id && t.cwd.includes('t1')), '终端会话 cwd 持久化');

// ─── 4) UI：终端标签与内容 ───
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
await sleep(1200);
await cdp.eval(`(() => {
  const close = document.querySelector('.hns-drawer-close');
  if (close) { close.click(); return 'closed'; }
  return 'none';
})()`);
await sleep(400);
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.hns-bar-icon')].find((x) => (x.title || '').includes('设置 / 钩子'));
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
await sleep(500);
await cdp.eval(`(() => {
  const tabs = [...document.querySelectorAll('.hns-drawer-tabs button')];
  const t = tabs.find((x) => x.textContent.trim() === '终端');
  if (t) { t.click(); return 'true'; }
  return 'false';
})()`);
await sleep(600);
const terminalUi = await cdp.waitFor(`(() => {
  const t = document.querySelector('.hns-terminal');
  return t ? t.textContent.trim() : 'false';
})()`, 15000);
check(!!terminalUi && terminalUi.includes('E2E终端'), `终端 UI 显示会话（${String(terminalUi).slice(0, 60)}）`);
check(String(terminalUi).includes('Get-Location') || String(terminalUi).includes('st_control'), '终端 UI 显示日志');

// ─── 5) 清理 + 截图（t1 为终端 cwd 测试目录，落在工作区内，删除避免残留） ───
await invoke('harness_shell_run', { command: "Remove-Item -Recurse -Force 't1' -ErrorAction SilentlyContinue" });
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../data/ui-audit/llm-harness-phase5.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);
await invoke('delete_harness_terminal', { id: term.id });

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
