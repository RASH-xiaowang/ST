// 已停用（2026-08）：原「AI 聊天」板块已并入 Harness 会话，本脚本断言的独立面板 UI 不再存在；
// 功能验证改由 e2e-harness-phase* 与 verify-harness-chat-* 承担。
// ============================================================
// E2E：AI 聊天重设计验证
//   1. 进入 AI 聊天 → 空态 hero（logo/标题/推荐问题）截图
//   2. 发送一条消息 → 等待流式回复 → 校验消息行结构
//   3. 截图（对话态）→ data/ui-audit/llm-chat-*.png
// 运行：node st_control/.codex_tests/e2e-llm-chat-redesign.mjs
// ============================================================

import fs from 'node:fs';
import path from 'node:path';

const CDP_BASE = 'http://127.0.0.1:9222';
const OUT_DIR = 'E:/ST/st_control/data/ui-audit';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function findTarget() {
  for (let i = 0; i < 30; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
      if (t) return t;
    } catch {}
    await sleep(1000);
  }
  throw new Error('no CDP target');
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
  async waitFor(expression, timeoutMs = 30000, stepMs = 500) {
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
  async shot(name) {
    const r = await this.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
    const out = path.join(OUT_DIR, name);
    fs.writeFileSync(out, Buffer.from(r.data, 'base64'));
    console.log('SAVED=' + out);
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

// 1) 进入 AI 聊天（已整合进 Harness：Harness 面板 → 「AI 聊天」子视图）
const navOk = await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.title === 'Harness');
  if (b) b.click();
  return 'true';
})()`, 20000);
check(navOk === 'true', '进入 Harness 面板');
await sleep(1000);
const subOk = await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('.hns-viewbar button')].find((el) => (el.textContent || '').trim() === 'AI 聊天');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
check(subOk === 'true', '点击 Harness「AI 聊天」子视图');
await sleep(1500);

const chatReady = await cdp.waitFor(`document.querySelector('.llm-chat') ? 'true' : 'false'`, 15000);
check(chatReady === 'true', 'AI 聊天面板已渲染');

// 2) 空态检查 + 截图
const hero = await cdp.eval(`JSON.stringify({
  hasHero: !!document.querySelector('.llm-hero'),
  hasMsg: document.querySelectorAll('.llm-msg').length,
  sugCount: document.querySelectorAll('.llm-hero-sug').length,
  sendBtn: !!document.querySelector('.llm-send-btn'),
  inputFoot: document.querySelector('.llm-input-foot span')?.textContent?.trim() ?? '',
})`);
console.log('STATE=' + hero);
const st = JSON.parse(hero);
check(st.hasHero, '空态 hero 存在');
check(st.sugCount === 4, `推荐问题 4 条（实际 ${st.sugCount}）`);
check(st.sendBtn, '圆形发送按钮存在');
check(!!st.inputFoot && st.inputFoot.includes('仅供参考'), `输入框脚注: ${st.inputFoot}`);
if (st.hasHero) await cdp.shot('llm-chat-hero.png');

// 3) 发送一条消息（若模型可用）
const sendable = await cdp.eval(`(() => {
  const ta = document.querySelector('.llm-chat-input textarea');
  if (!ta) return 'false';
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, '你好，请用一句话介绍你自己');
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return 'true';
})()`);
await sleep(400);
const canClick = await cdp.eval(`(() => {
  const btn = document.querySelector('.llm-send-btn');
  return btn && !btn.disabled ? 'true' : 'false';
})()`);
if (sendable === 'true' && canClick === 'true') {
  await cdp.eval(`(() => { const b = document.querySelector('.llm-send-btn'); if (b && !b.disabled) { b.click(); return 'true'; } return 'false'; })()`);
  console.log('SENT');
  // 等待流式回复完成（助手消息有内容且发送结束）
  const done = await cdp.waitFor(`(() => {
    const msgs = document.querySelectorAll('.llm-msg-bot .llm-msg-bubble');
    const last = msgs[msgs.length - 1];
    const sending = !!document.querySelector('.llm-caret');
    if (!last) return 'false';
    const text = last.textContent.trim();
    return !sending && text.length > 10 ? text.slice(0, 80) : 'false';
  })()`, 90000);
  console.log('REPLY=' + (done ? JSON.stringify(done) : 'timeout'));
  check(!!done, '收到流式回复');
  await sleep(400);
  const struct = await cdp.eval(`JSON.stringify({
    userMsgs: document.querySelectorAll('.llm-msg-user').length,
    botMsgs: document.querySelectorAll('.llm-msg-bot').length,
    botAvatar: !!document.querySelector('.llm-msg-bot .llm-msg-avatar'),
    botName: document.querySelector('.llm-msg-bot .llm-msg-name')?.textContent?.trim() ?? '',
    actions: document.querySelectorAll('.llm-msg-actions').length,
    copyBtns: [...document.querySelectorAll('.llm-msg-act')].map((b) => b.textContent.trim()),
    caret: !!document.querySelector('.llm-caret'),
  })`);
  console.log('STRUCT=' + struct);
  const s2 = JSON.parse(struct);
  check(s2.userMsgs >= 1, `用户消息 ${s2.userMsgs} 条`);
  check(s2.botMsgs >= 1, `助手消息 ${s2.botMsgs} 条`);
  check(s2.botAvatar && s2.botName.length > 0, `助手头像+名称（${s2.botName}）`);
  check(s2.copyBtns.includes('复制'), '复制按钮存在');
  check(!s2.caret, '流式光标已结束');
  await cdp.shot('llm-chat-conversation.png');

  // 4) 点击复制按钮验证
  await cdp.eval(`(() => { const b = [...document.querySelectorAll('.llm-msg-act')].find((x) => x.textContent.includes('复制')); if (b) b.click(); })()`);
  await sleep(500);
  const copied = await cdp.eval(`(() => [...document.querySelectorAll('.llm-msg-act')].some((b) => b.textContent.includes('已复制')) ? 'true' : 'false')()`);
  check(copied === 'true', '复制按钮反馈「已复制」');
} else {
  console.log('（当前无可用发送状态，跳过对话测试）');
}

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
