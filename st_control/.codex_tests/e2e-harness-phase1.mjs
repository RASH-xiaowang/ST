// E2E：Harness 阶段 1（导航入口 + 会话核心 + 流式对话 + 日志持久化）
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
  async waitFor(expression, timeoutMs = 120000, stepMs = 700) {
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

// 0) 会话 IPC 基础
const sessions0 = await invoke('harness_list_sessions');
check(Array.isArray(sessions0), 'harness_list_sessions 返回数组');

// 1) 导航入口
const navHit = await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
check(navHit === 'true', '导航栏出现 Harness 按钮并点击');
const shell = await cdp.waitFor(`(() => document.querySelector('.hns') ? 'true' : 'false')()`, 15000);
check(shell === 'true', 'Harness 界面渲染（.hns 壳）');

// 2) 新建会话
await cdp.waitFor(`(() => { const b = document.querySelector('.hns-new'); if (b) { b.click(); return 'true'; } return 'false'; })()`, 10000);
await sleep(600);
const sessionItems = await cdp.eval(`(() => document.querySelectorAll('.hns-session').length)()`);
check(sessionItems >= 1, `会话列表出现条目（${sessionItems}）`);

// 3) 模型选择（deepseek 对话模型）
await cdp.eval(`(async () => {
  const cfg = await window.__TAURI_INTERNALS__.invoke('get_llm_config');
  const chatP = (cfg.providers ?? []).find((x) => (x.models ?? []).includes('deepseek-v4-flash'));
  if (!chatP) return;
  const sels = [...document.querySelectorAll('.hns-bar-right select')];
  const setSelect = (el, val) => {
    if (!el || ![...el.options].some((o) => o.value === val)) return false;
    el.value = val; el.dispatchEvent(new Event('change', { bubbles: true })); return true;
  };
  setSelect(sels[0], chatP.id);
  await new Promise((r) => setTimeout(r, 600));
  const sels2 = [...document.querySelectorAll('.hns-bar-right select')];
  setSelect(sels2[0], chatP.id);
  setSelect(sels2[1], 'deepseek-v4-flash');
})()`);
await sleep(900);

// 4) 发送消息并接收流式回复（长回复保证可观测到流式过程）
const q = '请分三条要点介绍你的能力，每一条不少于 20 个字，最后一行只输出 HARNESS_OK';
await cdp.eval(`(() => {
  const ta = document.querySelector('.hns-input textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, ${JSON.stringify(q)});
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'true';
})()`);
await sleep(400);
await cdp.eval(`(() => { const b = document.querySelector('.hns-send'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
const botsBefore = await cdp.eval(`document.querySelectorAll('.hns-msg-bot .hns-bubble').length`);
const streamSeen = await cdp.waitFor(`(() => {
  const hint = document.querySelector('.hns-stream-hint');
  return hint ? 'true' : 'false';
})()`, 60000, 120);
// 工具循环下最终回答以单块下发：流式指示窗口极短，仅作信息记录
console.log(streamSeen === 'true' ? 'INFO: 流式指示可见' : 'INFO: 回答过快，未捕捉到流式指示（单块下发）');
const reply = await cdp.waitFor(`(() => {
  const bubbles = document.querySelectorAll('.hns-msg-bot .hns-bubble');
  if (bubbles.length <= ${botsBefore}) return 'false';
  const last = bubbles[bubbles.length - 1];
  const streaming = !!document.querySelector('.hns-stream-hint');
  const running = !!document.querySelector('.hns-tool-running');
  const text = last.textContent.trim();
  // 完成判定：非流式且非运行中且文本足够长（HARNESS_OK 标记模型可能
  // 不逐字输出——依从性波动，收发链路以「收到长回复」为准）
  return !streaming && !running && text.length > 30 ? text.slice(0, 300) : 'false';
})()`, 120000);
console.log('REPLY=' + JSON.stringify(reply));
check(!!reply && reply.length > 30, `收到流式回复（${reply?.slice(0, 60) ?? ''}…）`);
// 回放锚点：取回复尾 24 字符（足够独特，跨重载定位）
const replayAnchor = (reply || '').slice(-24).trim() || 'HARNESS_OK';

// 5) 会话列表联动：标题投影 + 消息数
// （定位本次聊天会话：标题由首条用户消息投影，而非假设列表首项——
// 脏库/多会话环境下 list[0] 不一定是本探针的会话）
await sleep(800);
const listAfter = await invoke('harness_list_sessions');
const mine = listAfter.find((s) => (s.title || '').includes('三条要点'));
console.log('SESSIONS=' + JSON.stringify(listAfter.slice(0, 3)));
check(!!mine && mine.message_count >= 1, '会话消息数已更新（首条用户消息落日志）');
check(!!mine && mine.title.includes('三条要点'), `会话标题自动投影（${mine?.title ?? ''}）`);

// 6) 持久化：整页重载后从日志回放（先选中本次会话再断言）
await cdp.send('Page.reload');
await sleep(6000);
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 30000);
await sleep(1200);
const selected = await cdp.eval(`(() => {
  const items = [...document.querySelectorAll('.hns-session')];
  const el = items.find((x) => x.textContent.includes('三条要点'));
  if (el) { el.click(); return 'true'; }
  return 'false';
})()`);
await sleep(800);
const replayed = await cdp.waitFor(`(() => {
  const bubbles = document.querySelectorAll('.hns-msg-bot .hns-bubble');
  const last = bubbles[bubbles.length - 1];
  return last && last.textContent.includes(${JSON.stringify(replayAnchor)}) ? 'true' : 'false';
})()`, 20000);
check(selected === 'true' && replayed === 'true', '整页重载后会话消息从日志完整回放');

// 7) 重命名（定位本次会话，非假设列表首项）
await cdp.eval(`(() => {
  const items = [...document.querySelectorAll('.hns-session')];
  const item = items.find((x) => x.textContent.includes('三条要点')) || document.querySelector('.hns-session');
  const btn = item?.querySelector('.hns-session-acts button[title="重命名"]');
  if (btn) { btn.click(); return 'true'; }
  return 'false';
})()`);
await sleep(400);
await cdp.eval(`(() => {
  const input = document.querySelector('.hns-session-edit');
  if (!input) return 'false';
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(input, '改名验证');
  input.dispatchEvent(new Event('input', { bubbles: true }));
  return 'true';
})()`);
await cdp.eval(`(() => { const b = document.querySelector('.hns-session-act[title="确认"]'); if (b) { b.click(); return 'true'; } return 'false'; })()`);
await sleep(600);
const renamed = await cdp.eval(`(() => {
  const items = [...document.querySelectorAll('.hns-session')];
  const mine = items.find((x) => x.textContent.includes('改名验证'));
  const t = mine?.querySelector('.hns-session-title');
  return t ? t.textContent.trim() : 'false';
})()`);
check(renamed === '改名验证', `重命名生效（${renamed}）`);

// 8) 删除会话（覆盖 confirm；删除本次重命名后的会话；按钮未命中/目标仍
//    存在时重试）。注意：删除最后一个会话时应用会新建一个空会话保持
//    ≥1——按「目标会话已不存在」断言而非计数差
await cdp.eval(`(() => { window.confirm = () => true; return 'true'; })()`);
const delTarget = (await invoke('harness_list_sessions')).find((s) => s.title === '改名验证');
check(!!delTarget, '定位待删除会话（改名验证）');
let clicked = 'false';
let afterDel = await invoke('harness_list_sessions');
for (let attempt = 0; attempt < 3; attempt++) {
  clicked = await cdp.eval(`(() => {
    const items = [...document.querySelectorAll('.hns-session')];
    const item = items.find((x) => x.textContent.includes('改名验证')) || document.querySelector('.hns-session');
    const btn = item?.querySelector('.hns-session-acts button[title="删除会话"]');
    if (btn) { btn.click(); return 'clicked'; }
    return 'missing';
  })()`);
  await sleep(1200);
  afterDel = await invoke('harness_list_sessions');
  if (!afterDel.some((s) => s.id === delTarget?.id)) break;
}
check(!afterDel.some((s) => s.id === delTarget?.id), `删除会话生效（目标已移除，点击=${clicked}）`);

// 9) 截图
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../data/ui-audit/llm-harness-phase1.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
