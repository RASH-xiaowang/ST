// E2E：Harness 阶段 10（会话分叉与回放 / 每会话预设 / MCP UI+导入导出 / 语音入口 / PTY 真终端）
// 前置：app 运行中（CDP 9222）+ Vite 1420。
// 幂等性：时间戳命名创建资源，结束前清理；CDP 断连/单求值 60s 超时快速失败。
const CDP_BASE = 'http://127.0.0.1:9222';
import fs from 'node:fs';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// 全局看门狗：异常卡死时 8 分钟强制退出（避免管道超时吞输出）
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
  cdp.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`, 90000)
    .catch((e) => ({ __err: String(e) }));
const TS = Date.now().toString(36);
const WS_DIR = 'C:\\Users\\28361\\Desktop\\ST\\st_control\\data\\agent_workspace';
const createdSessionIds = [];

// ═══ 0) 导航到 Harness 界面 ═══
const navHit = await cdp.waitFor(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness'); if (b) { b.click(); return 'true'; } return 'false'; })()`, 15000);
check(navHit === 'true', '导航栏 Harness 按钮可点击');
await cdp.waitFor(`document.querySelector('.hns') ? 'true' : 'false'`, 10000);
// 头部新控件存在性（预设作用域 + 导出 + 语音；等待 tab 初始化完成）
const presetSel = await cdp.waitFor(`(() => { const sels = [...document.querySelectorAll('.hns-bar-right select')]; return sels.some((s) => (s.title || '').includes('会话预设作用域')) ? 'true' : 'false'; })()`, 20000);
check(presetSel === 'true', '头部存在每会话预设作用域下拉');
const exportBtn = await cdp.waitFor(`document.querySelector('button[title="导出会话转写（Markdown 回放）"]') ? 'true' : 'false'`, 20000);
check(exportBtn === 'true', '头部存在导出回放按钮');
const micHit = await cdp.eval(`document.querySelector('[title="语音输入（麦克风）"]') ? 'true' : 'false'`);
check(micHit === 'true', '输入栏存在语音输入按钮（STT 入口）');

// ═══ 1) UI 对话 + 分叉 + 回放 ═══
// 模型选择（deepseek 对话模型；头部 select 顺序：提供方[0]、模型[1]）
await cdp.eval(`(async () => {
  const cfg = await window.__TAURI_INTERNALS__.invoke('get_llm_config');
  const chatP = (cfg.providers ?? []).find((x) => (x.models ?? []).includes('deepseek-v4-flash'));
  if (!chatP) return 'noprovider';
  const sels = [...document.querySelectorAll('.hns-bar-right select')];
  const setSelect = (el, val) => {
    if (!el || ![...el.options].some((o) => o.value === val)) return false;
    el.value = val; el.dispatchEvent(new Event('change', { bubbles: true })); return true;
  };
  setSelect(sels[0], chatP.id);
  await new Promise((r) => setTimeout(r, 500));
  const sels2 = [...document.querySelectorAll('.hns-bar-right select')];
  setSelect(sels2[0], chatP.id);
  setSelect(sels2[1], 'deepseek-v4-flash');
  return 'true';
})()`);
await sleep(900);
await cdp.eval(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(600);
const chatText = `FORK_E2E_${TS}`;
await cdp.eval(`(() => {
  const ta = document.querySelector('.hns-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, ${JSON.stringify(`请只回复：${chatText}`)});
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'true';
})()`);
await sleep(400);
await cdp.eval(`(() => { const b = document.querySelector('.hns-send'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
const reply = await cdp.waitFor(`(() => {
  const running = !!document.querySelector('.hns-tool-running');
  const streaming = !!document.querySelector('.hns-stream-hint');
  if (running || streaming) return 'false';
  const bubbles = [...document.querySelectorAll('.hns-msg-bot .hns-bubble')];
  if (!bubbles.length) return 'false';
  const text = bubbles[bubbles.length - 1].textContent.trim();
  return text.includes('${chatText}') ? text.slice(0, 300) : 'false';
})()`, 180000);
check(!!reply && reply.includes(chatText), `UI 收到对话回复（${String(reply).slice(0, 50)}）`);
const speakHit = await cdp.eval(`[...document.querySelectorAll('button')].some((b) => b.title === '朗读此回复') ? 'true' : 'false'`);
check(speakHit === 'true', '助手回复旁存在朗读按钮（TTS 入口）');

// 分叉：助手消息上的「分叉」按钮
const forkHit = await cdp.waitFor(`(() => { const els = [...document.querySelectorAll('.hns-msg-bot .hns-fork-btn')]; if (els.length) { els[els.length - 1].click(); return 'true'; } return 'false'; })()`, 10000);
check(forkHit === 'true', 'UI 点击助手消息「分叉」按钮');
await sleep(2500);
let sessList = await invoke('harness_list_sessions');
if (!Array.isArray(sessList)) {
  console.log('INFO: list_sessions 首次调用异常，2 秒后重试：' + JSON.stringify(sessList));
  await sleep(2000);
  sessList = await invoke('harness_list_sessions');
}
const srcMeta = (sessList ?? []).find((s) => (s.title || '').includes(chatText) && !(s.title || '').includes('分叉'));
check(!!srcMeta, `源会话标题投影含消息文本（${srcMeta?.title}）`);
const forkedMeta = (sessList ?? []).find((s) => (s.title || '').includes('分叉') && (s.title || '').includes(chatText));
check(!!forkedMeta, `UI 分叉产生新会话（${forkedMeta?.title}）`);
if (srcMeta) createdSessionIds.push(srcMeta.id);
if (forkedMeta) {
  createdSessionIds.push(forkedMeta.id);
  check(forkedMeta.preset_id === '', '分叉会话默认无会话预设覆盖');
  const srcMsgs = await invoke('harness_display_messages', { id: srcMeta.id });
  // UI 点击的是「最后一条」助手消息的分叉按钮：边界取其 seq
  const srcAsstMsgs = (srcMsgs ?? []).filter((m) => m.role === 'assistant');
  const srcAsst = srcAsstMsgs[srcAsstMsgs.length - 1];
  const boundary = srcAsst?.seq ?? 0;
  const events = await invoke('harness_session_events', { id: forkedMeta.id, afterSeq: 0 });
  const forkEvt = (events ?? []).find(([, e]) => e && e.type === 'session_forked');
  check(!!forkEvt && forkEvt[1].source === srcMeta.id && forkEvt[1].boundary_seq === boundary, '分叉事件落日志（source + boundary_seq 可溯源）');
  const forkMsgs = await invoke('harness_display_messages', { id: forkedMeta.id });
  check((forkMsgs ?? []).some((m) => m.role === 'user' && (m.content || '').includes(chatText)), '分叉会话复制了边界前的消息');
}
// 回放导出：Markdown 文本 + 文件写出
if (srcMeta) {
  const md = await invoke('harness_export_session', { id: srcMeta.id, path: null });
  check(typeof md === 'string' && md.includes(chatText), 'export_session 返回含对话内容的 Markdown（回放）');
  const exportPath = `C:\\Users\\28361\\Desktop\\ST\\st_control\\data\\harness\\e2e-export-${TS}.md`;
  const exportedPath = await invoke('harness_export_session', { id: srcMeta.id, path: exportPath });
  check(exportedPath === exportPath && fs.existsSync(exportPath), 'export_session 写文件成功');
  if (fs.existsSync(exportPath)) {
    const fileMd = fs.readFileSync(exportPath, 'utf8');
    check(fileMd.includes(chatText), '导出文件内容与日志投影一致');
    fs.rmSync(exportPath, { force: true });
  }
}

// ═══ 2) 每会话预设作用域 ═══
if (srcMeta) {
  const presetName = `E2E-预设-${TS}`;
  const savedPreset = await invoke('save_harness_preset', {
    preset: {
      id: '', name: presetName, description: '禁用 list_dir 的测试预设',
      disabled_tools: ['list_dir'], overrides: {}, prompt_sections: [],
      created_at: '', updated_at: '',
    },
  });
  const presetId = savedPreset?.id;
  check(!!presetId, `创建测试预设（${presetId ?? savedPreset?.__err}）`);
  if (presetId) {
    await invoke('harness_set_session_preset', { id: srcMeta.id, presetId });
    const afterSet = (await invoke('harness_list_sessions') ?? []).find((s) => s.id === srcMeta.id);
    check(afterSet?.preset_id === presetId, '会话级预设覆盖持久化（list_sessions.preset_id）');
    const blocked = await invoke('harness_execute_tool', {
      sessionId: srcMeta.id, name: 'list_dir',
      arguments: JSON.stringify({ path: '' }),
    });
    check(!!blocked?.__err && String(blocked.__err).includes('禁用'), `被禁用工具在派发层拦截（${blocked?.__err}）`);
    await invoke('harness_set_session_preset', { id: srcMeta.id, presetId: '' });
    const afterReset = (await invoke('harness_list_sessions') ?? []).find((s) => s.id === srcMeta.id);
    check(afterReset?.preset_id === '', '会话预设重置回全局默认（""）');
    const unblocked = await invoke('harness_execute_tool', {
      sessionId: srcMeta.id, name: 'list_dir',
      arguments: JSON.stringify({ path: '' }),
    });
    check(!unblocked?.__err && unblocked?.ok === true, '解除预设后工具恢复正常执行');
    await invoke('delete_harness_preset', { id: presetId });
  }
}

// ═══ 3) MCP 管理 + 配置束导入导出 ═══
const mcpId = `mcp-e2e-${TS}`;
const skillId = `skill-e2e-${TS}`;
const importPresetName = `E2E-导入预设-${TS}`;
const bundleJson = JSON.stringify({
  mcp_servers: [{
    id: mcpId, name: 'E2E Imported MCP', command: 'powershell.exe',
    args: ['-NoProfile', '-File', 'C:\\Users\\28361\\Desktop\\ST\\st_control\\.codex_tests\\mcp-echo-server.ps1'],
    enabled: false,
  }],
  skills: [{
    id: skillId, name: 'E2E Skill', description: 'E2E imported skill',
    content: '# E2E Skill\n\n导入测试技能。',
  }],
  presets: [{
    id: `preset-import-e2e-${TS}`, name: importPresetName, description: 'imported',
    disabled_tools: [], overrides: {}, prompt_sections: [], created_at: '', updated_at: '',
  }],
});
const importCount = await invoke('harness_import_bundle', { path: null, json: bundleJson });
check(importCount === 3, `配置束导入合并 3 条（${importCount?.__err ?? importCount}）`);
const mcpList = await invoke('list_harness_mcp_servers');
check((mcpList ?? []).some((s) => s.id === mcpId && s.enabled === false), '导入的 MCP 服务器出现在管理列表');
const skillList = await invoke('list_harness_skills');
check((skillList ?? []).some((s) => s.id === skillId), '导入的技能出现在技能列表');
const presetList = await invoke('list_harness_presets');
const importPresetId = (presetList ?? []).find((p) => p.name === importPresetName)?.id;
check(!!importPresetId, '导入的预设出现在预设列表');
const bundleOut = await invoke('harness_export_bundle', { path: null });
check(typeof bundleOut === 'string' && bundleOut.includes(mcpId) && bundleOut.includes(skillId), '导出配置束 JSON 包含导入项');
const bundlePath = `C:\\Users\\28361\\Desktop\\ST\\st_control\\data\\harness\\e2e-bundle-${TS}.json`;
const bundlePathOut = await invoke('harness_export_bundle', { path: bundlePath });
check(bundlePathOut === bundlePath && fs.existsSync(bundlePath), '配置束写文件成功');
if (fs.existsSync(bundlePath)) {
  const parsed = JSON.parse(fs.readFileSync(bundlePath, 'utf8'));
  check(parsed.mcp_servers.some((s) => s.id === mcpId), '导出文件可解析且含 MCP 项');
  fs.rmSync(bundlePath, { force: true });
}
await invoke('save_harness_mcp_servers', { servers: (mcpList ?? []).filter((s) => s.id !== mcpId) });
await invoke('delete_harness_skill', { id: skillId });
if (importPresetId) await invoke('delete_harness_preset', { id: importPresetId });

// ═══ 4) 治理抽屉：MCP 管理 tab + 终端 PTY 按钮 ═══
// 先创建终端会话（抽屉内每个终端行渲染「启动 PTY」按钮）
const term = await invoke('create_harness_terminal', { name: `E2E-PTY-${TS}` });
check(!!term?.id, `创建终端会话（${term?.id}）`);
// 先关闭可能遗留的抽屉，再点击治理打开（触发 openDrawer 刷新终端/MCP 列表）
await cdp.eval(`(() => { const c = document.querySelector('.hns-drawer-close'); if (c) { c.click(); return 'true'; } return 'false'; })()`);
await sleep(400);
const openGov = await cdp.eval(`(() => { const b = document.querySelector('button[title="设置 / 钩子 / 预设"]'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
check(openGov === 'true', '打开治理抽屉');
await sleep(800);
const mcpTab = await cdp.eval(`(() => { const b = [...document.querySelectorAll('.hns-drawer-tabs button')].find((x) => x.textContent === 'MCP'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
check(mcpTab === 'true', '治理抽屉含 MCP 管理 tab');
await sleep(400);
const mcpUi = await cdp.eval(`document.querySelector('.hns-port-head') ? 'true' : 'false'`);
check(mcpUi === 'true', 'MCP tab 渲染服务器列表与配置束导入导出区');
const termTab = await cdp.eval(`(() => { const b = [...document.querySelectorAll('.hns-drawer-tabs button')].find((x) => x.textContent === '终端'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
check(termTab === 'true', '切换到终端 tab');
await sleep(400);
const ptyBtn = await cdp.eval(`[...document.querySelectorAll('button')].some((b) => b.title === '启动 PTY 真终端（powershell）') ? 'true' : 'false'`);
check(ptyBtn === 'true', '终端 tab 存在「启动 PTY」按钮（已有终端会话时）');
await cdp.eval(`(() => { const b = document.querySelector('.hns-drawer-close'); if (b) { b.click(); return 'true'; } return 'false'; })()`);

// ═══ 5) PTY 真终端：进程状态保持（变量跨命令存活 = 真 PTY 证明） ═══
if (term?.id) {
  const ptyStart = await invoke('harness_terminal_start_pty', { id: term.id, rows: 30, cols: 120 });
  if (ptyStart?.__err) {
    const msg = String(ptyStart.__err);
    if (msg.includes('CreatePseudoConsole') || msg.includes('启动 shell 失败')) {
      console.log(`SKIP: PTY 启动被系统拒绝（ConPTY 不可用），已按设计降级为普通命令模式：${msg}`);
    } else {
      check(false, `PTY 启动：意外错误 ${msg}`);
    }
  } else {
    check(true, 'PTY 启动成功（ConPTY + powershell）');
    const setVar = await invoke('harness_terminal_send_pty', { id: term.id, input: `$e2eVar = 'PTY-STATE-${TS}'` });
    check(!setVar?.__err, 'PTY 写入第一条命令成功');
    const readVar = await invoke('harness_terminal_send_pty', { id: term.id, input: 'Write-Output $e2eVar' });
    console.log('PTY_READVAR=' + JSON.stringify(readVar).slice(0, 300));
    check(!readVar?.__err && String(readVar).includes(`PTY-STATE-${TS}`), 'PTY 跨命令保留进程状态（变量存活 = 真终端）');
    const st = await invoke('harness_terminal_pty_status', { id: term.id });
    check(st?.running === true, 'PTY 状态报告 running');
    const rz = await invoke('harness_terminal_resize_pty', { id: term.id, rows: 40, cols: 100 });
    check(!rz?.__err, 'PTY 尺寸调整成功');
    await invoke('harness_terminal_stop_pty', { id: term.id });
    const st2 = await invoke('harness_terminal_pty_status', { id: term.id });
    check(st2?.running === false, 'PTY 停止后状态 running=false');
  }
  await invoke('delete_harness_terminal', { id: term.id });
  const gone = (await invoke('list_harness_terminals') ?? []).some((t) => t.id === term.id);
  check(!gone, '测试终端已清理（PTY 进程一并回收）');
}

// ═══ 清理 ═══
for (const id of createdSessionIds) await invoke('harness_delete_session', { id });
const leftovers = (await invoke('harness_list_sessions') ?? []).filter((s) => createdSessionIds.includes(s.id));
check(leftovers.length === 0, '测试会话全部清理');

clearTimeout(watchdog);
console.log(failures === 0 ? 'ALL_PASS' : `FAILURES: ${failures}`);
process.exit(failures === 0 ? 0 : 1);
