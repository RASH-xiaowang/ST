// E2E：Harness 阶段 11（审计缺口补齐验证）
// 覆盖：jobs / fs(edit,glob,grep,read_image) / workspace / 三模式沙箱 /
// terminal 模型工具 / schedule 模型工具 / goal 生命周期 / subagent /
// ask_user_question / slash 命令 / session_search / session_trace /
// spill 溢写 / feedback 消息级 / skill frontmatter 门控 / ACP 补全。
// 前置：app 运行中（CDP 9222）+ Vite 1420。
const CDP_BASE = 'http://127.0.0.1:9222';
const SDK_BASE = 'http://127.0.0.1:4770';
import fs from 'node:fs';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const watchdog = setTimeout(() => {
  console.error('WATCHDOG: 脚本疑似卡死，强制退出');
  process.exit(2);
}, 480000);
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
    ws.onclose = () => {
      for (const { reject } of [...this.pending.values()]) reject(new Error('CDP 连接已断开'));
      this.pending.clear();
    };
  }
  send(method, params = {}, timeoutMs = 60000) {
    return new Promise((resolve, reject) => {
      const id = ++this.id;
      const timer = setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new Error(`CDP 请求超时（${method}）`));
        }
      }, timeoutMs);
      this.pending.set(id, {
        resolve: (v) => { clearTimeout(timer); resolve(v); },
        reject: (e) => { clearTimeout(timer); reject(e); },
      });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }
  async eval(expression, timeoutMs = 60000) {
    const r = await this.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, timeoutMs);
    if (r.exceptionDetails) throw new Error('evaluate 异常: ' + JSON.stringify(r.exceptionDetails));
    return r.result.value;
  }
  async evalNoWait(expression) {
    const r = await this.send('Runtime.evaluate', { expression, awaitPromise: false, returnByValue: true });
    if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
    return true;
  }
  async waitFor(expression, timeoutMs = 120000, stepMs = 250) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        const v = await this.eval(expression, Math.min(30000, timeoutMs));
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
  cdp.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`, 120000)
    .catch((e) => ({ __err: String(e) }));
async function rpc(method, params = {}) {
  try {
    const res = await fetch(`${SDK_BASE}/rpc`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
      signal: AbortSignal.timeout(120000),
    });
    return await res.json();
  } catch (e) {
    return { __err: String(e) };
  }
}
const TS = Date.now().toString(36);
const execTool = (sid, name, args) =>
  invoke('harness_execute_tool', { sessionId: sid, name, arguments: JSON.stringify(args) });

// 导航到 Harness
await cdp.waitFor(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness'); if (b) { b.click(); return 'true'; } return 'false'; })()`, 15000);
await cdp.waitFor(`document.querySelector('.hns') ? 'true' : 'false'`, 10000);

// 用 UI 新建会话并取得其 id（问题卡/slash 均按 UI 激活会话过滤）
await cdp.eval(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(800);
const sessList0 = await invoke('harness_list_sessions');
const newest = (sessList0 ?? []).slice().sort((a, b) => (b.created_at || '').localeCompare(a.created_at || ''))[0];
const sid = newest?.id;
check(!!sid, `UI 会话取得 id（${sid}）`);
// M8 参数指纹语义：不再支持「整体信任 exec_command」（每次不同命令都需
// 审批）。后台看门狗自动点「批准」，保证 exec_command 派发不被挂起；
// 挂起会留下未配对 tool_calls 事件，污染后续模型回合（API 400）。
let approveStop = false;
const approveLoop = (async () => {
  while (!approveStop) {
    await cdp
      .eval(`(() => { const btns = [...document.querySelectorAll('.hns-approve')]; const b = btns[btns.length - 1]; if (b) { b.click(); return 't'; } return 'f'; })()`)
      .catch(() => {});
    await sleep(150);
  }
})();

// ═══ 0) slash 命令（最早执行：此时 UI 无其他回合干扰） ═══
const slashText = `/goal SG_${TS}`;
await cdp.eval(`(() => {
  const ta = document.querySelector('.hns-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, ${JSON.stringify(slashText)});
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'true';
})()`);
await sleep(300);
await cdp.eval(`(() => { const b = document.querySelector('.hns-send'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
const slashReply = await cdp.waitFor(`(() => { const bubbles = [...document.querySelectorAll('.hns-msg-bot .hns-bubble')]; if (!bubbles.length) return 'false'; return bubbles[bubbles.length - 1].textContent.includes('SG_${TS}') ? 'true' : 'false'; })()`, 30000);
check(slashReply === 'true', 'slash 命令 /goal 返回命令回复');
const sstate0 = await invoke('harness_session_state', { id: sid });
check(String(sstate0?.goal ?? '').includes(`SG_${TS}`), 'slash /goal 落状态');

// ═══ 1) jobs 后台作业 ═══
const bg = await execTool(sid, 'exec_command', { command: `Write-Output BG11_${TS}; Start-Sleep 2`, run_in_background: true });
const jobId = String(bg?.result || '').match(/job-[a-z0-9]+/)?.[0];
check(bg?.ok === true && !!jobId, `后台作业启动（${bg?.result}）`);
if (jobId) {
  const l1 = await invoke('harness_job_list', { sessionId: sid });
  check((l1 ?? []).some((j) => j.id === jobId), 'job_list 见运行中作业');
  await sleep(4000);
  const out = await invoke('harness_job_output', { id: jobId });
  check(String(out).includes(`BG11_${TS}`), 'job_output 取回完整输出');
  const l2 = await invoke('harness_job_list', { sessionId: sid });
  check((l2 ?? []).some((j) => j.id === jobId && j.status === 'done'), '作业完成状态 done');
  // kill 路径
  const bg2 = await execTool(sid, 'exec_command', { command: 'Start-Sleep 120', run_in_background: true });
  const jobId2 = String(bg2?.result || '').match(/job-[a-z0-9]+/)?.[0];
  if (jobId2) {
    await invoke('harness_job_kill', { id: jobId2 });
    await sleep(2000);
    const l3 = await invoke('harness_job_list', { sessionId: sid });
    check((l3 ?? []).some((j) => j.id === jobId2 && j.status === 'killed'), 'job_kill 终止作业');
  }
}

// ═══ 2) fs 工具 edit / glob / grep / read_image ═══
await execTool(sid, 'write_file', { path: `r11_${TS}.txt`, content: 'alpha\nbeta\nalpha\n' });
const ed = await execTool(sid, 'edit_file', { path: `r11_${TS}.txt`, old_string: 'alpha', new_string: 'omega', replace_all: true });
check(ed?.ok === true && String(ed.result).includes('2'), `edit_file 替换（${ed?.result}）`);
const edBad = await execTool(sid, 'edit_file', { path: `r11_${TS}.txt`, old_string: 'omega', new_string: 'x' });
check(edBad?.ok === false && String(edBad.result).includes('出现'), `edit_file 歧义报错（${edBad?.result}）`);
const gl = await execTool(sid, 'glob', { pattern: `r11_${TS}.txt` });
check(gl?.ok === true && String(gl.result).includes(`r11_${TS}.txt`), `glob 发现（${gl?.result}）`);
const gr = await execTool(sid, 'grep', { pattern: 'omega', path: `r11_${TS}.txt` });
check(gr?.ok === true && String(gr.result).includes('omega'), `grep 命中（${gr?.result}）`);
const ri = await execTool(sid, 'read_image', { path: `r11_${TS}.txt` });
check(ri?.ok === false && String(ri.result).includes('格式'), `read_image 拒绝非图片（${ri?.result}）`);

// ═══ 3) workspace 注册表 + 三模式沙箱 ═══
const wl = await execTool(sid, 'workspace_list', {});
check(wl?.ok === true && String(wl.result).includes('default'), 'workspace_list');
const wc = await execTool(sid, 'workspace_create', { title: `E2E-WS-${TS}` });
const wsId = String(wc?.result || '').match(/ws-[a-z0-9]+/)?.[0];
check(wc?.ok === true && !!wsId, `workspace_create（${wc?.result}）`);
await execTool(sid, 'workspace_switch', { id: wsId });
const pwd = await execTool(sid, 'exec_command', { command: 'Write-Output (Get-Location).Path' });
check(pwd?.ok === true && String(pwd.result).includes(wsId), `exec_command 锚定工作区（${String(pwd?.result).slice(0, 90)}）`);
approveStop = true; // 看门狗任务结束（后续 exec_command 已无）
await invoke('save_harness_settings', { settings: { last_provider_id: '', last_model: '', tool_timeout_secs: 30, max_agent_rounds: 6, preset_id: null, allow_workspace_escape: false, sandbox_mode: 'read-only', workspace_id: wsId, context_budget_tokens: 24000, enable_compaction: true } });
const ro = await execTool(sid, 'write_file', { path: 'ro.txt', content: 'x' });
check((ro?.ok === false && String(ro.result).includes('只读')) || String(ro?.__err ?? '').includes('只读'), `read-only 拦截写入（${ro?.result ?? ro?.__err}）`);
await invoke('save_harness_settings', { settings: { last_provider_id: '', last_model: '', tool_timeout_secs: 30, max_agent_rounds: 6, preset_id: null, allow_workspace_escape: false, sandbox_mode: 'danger-full-access', workspace_id: wsId, context_budget_tokens: 24000, enable_compaction: true } });
const esc = await execTool(sid, 'list_dir', { path: 'C:\\Users\\28361\\Desktop\\ST\\st_control' });
check(esc?.ok === true, 'danger-full-access 越界列目录');
await invoke('save_harness_settings', { settings: { last_provider_id: '', last_model: '', tool_timeout_secs: 30, max_agent_rounds: 6, preset_id: null, allow_workspace_escape: false, sandbox_mode: 'workspace-write', workspace_id: '', context_budget_tokens: 24000, enable_compaction: true } });

// ═══ 4) terminal 模型工具 ═══
const to = await execTool(sid, 'terminal_open', { name: `E2E-T-${TS}` });
const termId = String(to?.result || '').match(/term-[a-z0-9]+/)?.[0];
check(to?.ok === true && !!termId, `terminal_open（${to?.result}）`);
if (termId) {
  const ts = await execTool(sid, 'terminal_send', { id: termId, input: `Write-Output T11_${TS}` });
  check(ts?.ok === true && String(ts.result).includes(`T11_${TS}`), `terminal_send（${String(ts?.result).slice(0, 80)}）`);
  const tr = await execTool(sid, 'terminal_read', { id: termId });
  check(tr?.ok === true && String(tr.result).includes(`T11_${TS}`), 'terminal_read 读日志');
  const tl = await execTool(sid, 'terminal_list', {});
  check(tl?.ok === true && String(tl.result).includes(termId), 'terminal_list');
  const tc = await execTool(sid, 'terminal_close', { id: termId });
  check(tc?.ok === true, 'terminal_close');
}

// ═══ 5) schedule 模型工具 ═══
const sc = await execTool(sid, 'schedule_create', { name: `E2E-S-${TS}`, prompt: '请回复 SCH_OK', after_seconds: 86400 });
const schId = String(sc?.result || '').match(/sch-[a-z0-9]+/)?.[0];
check(sc?.ok === true && !!schId, `schedule_create 一次性（${sc?.result}）`);
const sl = await execTool(sid, 'schedule_list', {});
check(sl?.ok === true && String(sl.result).includes(schId ?? 'sch-'), 'schedule_list');
if (schId) await execTool(sid, 'schedule_delete', { id: schId });

// ═══ 6) goal 生命周期 ═══
const gc = await execTool(sid, 'goal_create', { objective: `G11_${TS}`, max_goal_rounds: 5 });
check(gc?.ok === true && String(gc.result).includes('active'), `goal_create（${gc?.result}）`);
const gg = await execTool(sid, 'goal_get', {});
check(gg?.ok === true && String(gg.result).includes(`G11_${TS}`) && String(gg.result).includes('active'), 'goal_get');
const gb = await execTool(sid, 'goal_update', { action: 'blocked', blocked_reason: 'E2E 阻塞原因' });
check(gb?.ok === true && String(gb.result).includes('blocked'), 'goal_update blocked');
const gg2 = await execTool(sid, 'goal_get', {});
check(gg2?.ok === true && String(gg2.result).includes('E2E 阻塞原因') && String(gg2.result).includes('修订：') && String(gg2.result).includes('blocked'), `goal_get 状态机（${String(gg2.result).slice(0, 120)}）`);
const gr2 = await execTool(sid, 'goal_update', { action: 'resume' });
check(gr2?.ok === true, 'goal_update resume');
const ge = await execTool(sid, 'goal_update', { action: 'complete' });
check(ge?.ok === true && String(ge.result).includes('complete'), 'goal_update complete');

// ═══ 7) session_search / session_trace / spill / feedback ═══
const ss = await execTool(sid, 'session_search', { query: `G11_${TS}` });
check(ss?.ok === true, `session_search（${String(ss?.result).slice(0, 80)}）`);
const st = await execTool(sid, 'session_trace', {});
check(st?.ok === true && String(st.result).includes('血缘'), `session_trace（${String(st?.result).slice(0, 80)}）`);
const big = 'B'.repeat(4000);
await execTool(sid, 'write_file', { path: `sp11_${TS}.txt`, content: big });
// spill 作用于超限的工具结果：读取大文件 → 结果超 2000 字符 → 溢写
const sp = await execTool(sid, 'read_file', { path: `sp11_${TS}.txt` });
check(sp?.ok === true && String(sp.result).includes('已溢写'), `spill 溢写超限结果（${String(sp?.result).slice(0, 80)}）`);
const loc = String(sp?.result || '').match(/locator:\s*([^\n]+)/)?.[1];
if (loc) {
  const sread = await execTool(sid, 'spill_read', { locator: loc });
  check(sread?.ok === true && String(sread.result).includes('B'.repeat(50)), 'spill_read 取回完整值');
}
const fb = await invoke('harness_submit_feedback', { sessionId: sid, rating: 'good', comment: `F11_${TS}`, messageSeq: 3 });
check(!fb?.__err, 'feedback 提交（消息级）');
const fl = await invoke('harness_list_feedback');
check((fl ?? []).some((f) => f.comment === `F11_${TS}` && f.message_seq === 3), 'feedback 记录含 message_seq');

// ═══ 8) skill frontmatter 门控 ═══
const skillId = `skill-gate-${TS}`;
await invoke('save_harness_skill', { skill: { id: skillId, name: 'gate', description: 'd', content: '---\nname: 门控技能\ndisable-model-invocation: true\n---\n\n# 门控\n\n内容。', model_invocable: true } });
const skList = await execTool(sid, 'skill_list', {});
check(skList?.ok === true && !String(skList.result).includes(skillId), 'skill_list 不展示禁模型调用技能');
const skLoad = await execTool(sid, 'skill_load', { name: skillId });
check(skLoad?.ok === false && String(skLoad.result).includes('禁用模型调用'), `skill_load 拒绝（${skLoad?.result}）`);
await invoke('delete_harness_skill', { id: skillId });

// ═══ 9) ask_user_question（UI 问题卡 + 回答） ═══
await cdp.evalNoWait(`window.__TAURI_INTERNALS__.invoke('harness_execute_tool', ${JSON.stringify({ sessionId: sid, name: 'ask_user_question', arguments: JSON.stringify({ question: `Q11_${TS}`, options: ['选项甲', '选项乙'] }) })}).catch(() => {});`);
const qCard = await cdp.waitFor(`(() => { const el = [...document.querySelectorAll('.hns-approval-text')].find((x) => (x.textContent || '').includes('Q11_${TS}')); return el ? 'true' : 'false'; })()`, 15000);
check(qCard === 'true', '问题卡渲染（user-questions 接缝）');
const answered = await cdp.eval(`(() => { const btn = [...document.querySelectorAll('.hns-approve')].find((b) => b.textContent === '选项甲'); if (btn) { btn.click(); return 'true'; } return 'false'; })()`);
check(answered === 'true', '点击选项回答');
await sleep(1500);
const qResult = await invoke('harness_session_events', { id: sid, afterSeq: 0 });
const qTool = (qResult ?? []).filter(([, e]) => e?.type === 'tool_result').map(([, e]) => e.result || '');
check(qTool.some((r) => String(r).includes('选项甲')), '工具拿到用户回答');

// ═══ 10) ACP 补全（SDK；显式提供方/模型避免设置重置影响） ═══
const cfg = await invoke('get_llm_config');
const chatP = (cfg?.providers ?? []).find((x) => (x.models ?? []).includes('deepseek-v4-flash'));
const ini = await rpc('initialize', {});
check(ini?.result?.agentCapabilities?.stream === true, 'ACP initialize 能力声明');
const upd = await rpc('session/update', { session_id: sid, provider_id: chatP?.id, model: 'deepseek-v4-flash', prompt: `请只回复：U11_${TS}` });
check(upd?.result?.stopReason === 'end_turn' && String(upd?.result?.content).includes(`U11_${TS}`), `ACP session/update（${String(upd?.result?.content ?? upd?.error?.message ?? upd?.__err).slice(0, 60)}）`);
const perm = await rpc('session/request_permission', { id: 'p1', approve: true });
check(perm?.result?.outcome === 'approved', 'ACP request_permission approve');

// ═══ 清理 ═══
await invoke('harness_fs_delete', { path: `r11_${TS}.txt` });
await invoke('harness_fs_delete', { path: `sp11_${TS}.txt` });
if (wsId) await invoke('delete_harness_workspace', { id: wsId });
await invoke('harness_delete_session', { id: sid });

clearTimeout(watchdog);
console.log(failures === 0 ? 'ALL_PASS' : `FAILURES: ${failures}`);
process.exit(failures === 0 ? 0 : 1);
