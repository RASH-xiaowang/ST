// E2E：B2 workflow JS 编排（DSH workflow 组合子 agent/parallel/pipeline）
// 前置：隔离环境（CDP 9222 + Vite 1420 + ST_WECHAT_APP_DIR=.e2e/app）
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
  cdp.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`).catch((e) => ({ __err: String(e) }));

// 0) 进入 Harness 并新建会话
await cdp.eval(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(1500);
await cdp.eval(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(800);
const sessList = await invoke('harness_list_sessions');
const sid = (sessList ?? []).slice().sort((a, b) => (b.created_at || '').localeCompare(a.created_at || ''))[0]?.id;
check(!!sid, `会话取得（${sid}）`);

// 1) workflow_run_js 工具注册
const tools = await invoke('get_harness_tools');
check((tools ?? []).some((t) => t.name === 'workflow_run_js' && t.requires_approval), '工具目录含 workflow_run_js（需审批）');

// 2) 审批看门狗：workflow_run_js 需审批
let approveStop = false;
const approveLoop = (async () => {
  while (!approveStop) {
    await cdp.eval(`(() => { const btns = [...document.querySelectorAll('.hns-approve')]; const b = btns[btns.length - 1]; if (b) { b.click(); return 't'; } return 'f'; })()`).catch(() => {});
    await sleep(150);
  }
})();

// 3) 派发 workflow_run_js：脚本用 ctx.agent 并行派生两个子代理并合并结论
const TS = Date.now().toString(36);
const script = [
  "const [a, b] = await ctx.parallel([",
  "  () => ctx.agent('请只回复：WF_A_' + args.tag),",
  "  () => ctx.agent('请只回复：WF_B_' + args.tag),",
  "]);",
  "return JSON.stringify({ a, b });",
].join('\n');
const dispatched = await invoke('harness_execute_tool', {
  sessionId: sid,
  name: 'workflow_run_js',
  arguments: JSON.stringify({ code: script, args: { tag: TS } }),
});
console.log('WF_RESULT=' + JSON.stringify(dispatched).slice(0, 400));
const wfText = JSON.stringify(dispatched ?? '');
check(dispatched?.ok === true && wfText.includes(`WF_A_${TS}`) && wfText.includes(`WF_B_${TS}`), 'ctx.parallel 双子代理结论合并（WF_A/WF_B）');

// 4) pipeline 组合子：逐阶段对数组做映射
const script2 = [
  "const out = await ctx.pipeline([1, 2, 3],",
  "  async (x) => (await ctx.agent('请只回复数字：' + (x * 10))).trim(),",
  ");",
  "return JSON.stringify(out);",
].join('\n');
const dispatched2 = await invoke('harness_execute_tool', {
  sessionId: sid,
  name: 'workflow_run_js',
  arguments: JSON.stringify({ code: script2 }),
});
console.log('PIPELINE_RESULT=' + JSON.stringify(dispatched2).slice(0, 400));
check(dispatched2?.ok === true, `ctx.pipeline 流水线执行（${String(dispatched2?.result ?? dispatched2?.__err).slice(0, 120)}）`);

// 5) 子代理血缘：workflow 派生会话可从父会话溯源
const catalog = await invoke('harness_subagent_catalog', { sessionId: sid });
check(Array.isArray(catalog) && catalog.length >= 1, `子代理目录含 workflow 派生的子代理（${(catalog ?? []).length} 个）`);

// 6) run_code ctx.tools（B23）：脚本内调用其它 Harness 工具。
// run_code 需审批——看门狗仍在运行（approveStop 在 6 之后才置位）
const script3 = [
  "const t = await ctx.tools.get_current_time();",
  "await ctx.tools.write_file({ path: 'runcode-' + args.tag + '.txt', content: 'RUNC_TOOLS_OK' });",
  "const r = await ctx.tools.read_file({ path: 'runcode-' + args.tag + '.txt' });",
  "return JSON.stringify({ time: String(t).slice(0, 24), read: r });",
].join('\n');
const d3 = await invoke('harness_execute_tool', {
  sessionId: sid,
  name: 'run_code',
  arguments: JSON.stringify({ code: script3, args: { tag: TS } }),
});
console.log('RUNC_TOOLS=' + JSON.stringify(d3).slice(0, 300));
const d3Text = JSON.stringify(d3 ?? '');
check(d3?.ok === true && d3Text.includes('RUNC_TOOLS_OK'), 'run_code ctx.tools 调其它工具（写→读回显）');
// 清理脚本写的文件
await invoke('harness_fs_delete', { path: `runcode-${TS}.txt` }).catch(() => {});
approveStop = true; // 全部审批敏感测试结束

// 6) 清理本探针会话
await invoke('harness_delete_session', { id: sid });

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
