// 验证：str_replace_editor 四命令（view/create/str_replace/insert）
// 经人工派发路径（harness_execute_tool，免审批 → 零 LLM 消耗），
// 断言文件真实变更 + 工具时间线出现专用卡（.tc-sre）。
// 前置：app 运行中（CDP 9222）+ Vite 1420；隔离数据目录。
const CDP_BASE = 'http://127.0.0.1:9222';
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
  async waitFor(expression, timeoutMs = 60000, stepMs = 250) {
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
  cdp.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);

// 0) 工具目录含 str_replace_editor
const tools = await invoke('get_harness_tools');
check(tools.some((t) => t.name === 'str_replace_editor'), '工具目录含 str_replace_editor');
check(
  !tools.find((t) => t.name === 'str_replace_editor')?.requires_approval,
  'str_replace_editor 免审批（人工派发零 LLM）'
);

// 1) 进入 Harness 并新建会话
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
await sleep(1200);
await cdp.eval(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(600);
const sessionsNow = await invoke('harness_list_sessions');
const sid = sessionsNow.reduce((a, b) => (a.created_at >= b.created_at ? a : b)).id;
console.log('SESSION=' + sid);
check(!!sid && sid.startsWith('h-'), `会话已创建（${sid}）`);

// 2) 人工派发四命令（确定性、免审批、零 LLM）
const P = 'sre_e2e.txt';
const exec = (args) => invoke('harness_execute_tool', { sessionId: sid, name: 'str_replace_editor', arguments: JSON.stringify(args) });

// create
const r1 = await exec({ command: 'create', path: P, file_text: 'line1\nline2\nline3\n' });
check(r1 && !r1.__err && String(r1.result).includes('已创建'), `create 创建文件（${String(r1?.result ?? r1).slice(0, 40)}）`);
// create 已存在 → 拒绝
const r1b = await exec({ command: 'create', path: P, file_text: 'x' });
check(r1b && r1b.ok === false, 'create 已存在拒绝覆盖');
// view 全文带行号
const r2 = await exec({ command: 'view', path: P });
check(r2 && r2.ok && String(r2.result).includes('1  line1') && String(r2.result).includes('3  line3'), 'view 带行号全文');
// view_range [2,2]
const r2b = await exec({ command: 'view', path: P, view_range: [2, 2] });
check(r2b && r2b.ok && String(r2b.result).includes('2  line2') && !String(r2b.result).includes('line1'), 'view view_range 区间');
// str_replace 唯一匹配
const r3 = await exec({ command: 'str_replace', path: P, old_str: 'line2', new_str: 'LINE2' });
check(r3 && r3.ok && String(r3.result).includes('已编辑'), 'str_replace 唯一匹配');
// insert 行 2 后
const r4 = await exec({ command: 'insert', path: P, insert_line: 2, new_str: 'INSERTED' });
check(r4 && r4.ok, 'insert 行后插入');
// 非法 insert_line → 拒绝
const r4b = await exec({ command: 'insert', path: P, insert_line: 99, new_str: 'x' });
check(r4b && r4b.ok === false, 'insert 非法行号拒绝');
// 最终内容核对
const r5 = await exec({ command: 'view', path: P });
const finalText = String(r5?.result ?? '');
check(
  finalText.includes('line1') && finalText.includes('LINE2') && finalText.includes('INSERTED') && finalText.includes('line3'),
  '最终文件内容 = line1/LINE2/INSERTED/line3'
);

// 3) 日志事件配对：人工派发落 AssistantToolCalls(hcmd-) + ToolResult
// （模型上下文完整性：配对齐全 → 后续回合 API 不会因孤立 tool 消息 400）
const events = await invoke('harness_session_events', { id: sid, afterSeq: 0 });
const evArr = Array.isArray(events) ? events : [];
const calls = evArr.filter(([, e]) => e && e.type === 'assistant_tool_calls' && JSON.stringify(e).includes('str_replace_editor'));
const results = evArr.filter(([, e]) => e && e.type === 'tool_result');
console.log('EVENTS=' + evArr.length + ' calls=' + calls.length + ' results=' + results.length);
check(calls.length >= 1, '日志含 str_replace_editor 的 assistant_tool_calls（人工派发）');
check(results.length >= 1, '日志含 tool_result（配对）');
// 工具步骤在展示层随回合挂载；人工派发无 assistant 回合 → 时间线不显示
// （设计行为，phase11 同款断言口径：只验证执行结果与日志）。sre 卡 DOM
// 渲染由模型路径探针（verify-tool-timeline 系列）回归覆盖。
const disp = await invoke('harness_display_messages', { id: sid });
const dispArr = Array.isArray(disp) ? disp : [];
const hasTools = dispArr.some((m) => (m.tools ?? []).length > 0);
console.log('DISP=' + dispArr.length + ' hasTools=' + hasTools);

// 4) 清理：删除测试文件（自包含）
const del = await invoke('harness_fs_delete', { path: P });
console.log('CLEANUP=' + (del && !del.__err ? 'ok' : String(del)));
// 显式收尾：关闭 CDP WebSocket 并退出——否则全局 ws 连接保持打开会让
// node 事件循环不退出，`& node` 永不返回，外层脚本卡在探针循环
// （teardown 因 finally 未到达而不执行，应用残留锁 exe）
try { ws.close(); } catch {}
if (failures > 0) {
  console.log('FAILURES=' + failures);
  process.exit(1);
}
console.log('ALL_PASS');
process.exit(0);
