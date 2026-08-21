// 已停用（2026-08）：原「AI 聊天」板块已并入 Harness 会话，本脚本断言的独立面板 UI 不再存在；
// 功能验证改由 e2e-harness-phase* 与 verify-harness-chat-* 承担。
// ============================================================
// 端到端验证：真实应用内「添加模型 → 其他界面实时更新」
// 通过 WebView2 CDP 驱动：
//   1. 在 AI 聊天 / AI 文案 / 大模型管理面板读取模型计数徽标
//   2. 调用真实后端命令 add_llm_model 添加一个探针模型
//   3. 不刷新页面，等待数百毫秒后重新读取徽标，验证计数 +1
//   4. 再调用 remove_llm_model 删除探针模型，验证计数还原
// 运行：node st_control/.codex_tests/e2e-cdp.mjs
// ============================================================

const CDP_BASE = 'http://127.0.0.1:9222';

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function findTarget() {
  for (let i = 0; i < 90; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page');
      if (t) return t;
    } catch {
      /* 应用尚未就绪 */
    }
    await sleep(1000);
  }
  throw new Error('90 秒内未发现 CDP 页面目标');
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
        if (m.error) reject(new Error(JSON.stringify(m.error)));
        else resolve(m.result);
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
    const r = await this.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (r.exceptionDetails) {
      throw new Error('evaluate 异常: ' + JSON.stringify(r.exceptionDetails));
    }
    return r.result.value;
  }
}

const READ_BADGE = `(() => {
  const visible = [...document.querySelectorAll('.panel')].find((el) => !el.classList.contains('panel-hidden'));
  if (!visible) return '';
  const texts = [...visible.querySelectorAll('*')]
    .map((el) => (el.textContent || '').trim())
    .filter((t) => t.includes('个提供方') && t.includes('个模型'));
  return texts[0] || '';
})()`;

const CLICK_NAV = (title) =>
  `(() => { const b = document.querySelector('.nav-item[title="${title}"]'); if (!b) return false; b.click(); return true; })()`;

const parseCount = (badge) => {
  const m = badge.match(/(\d+)\s*个提供方\s*·\s*(\d+)\s*个模型/);
  return m ? { providers: Number(m[1]), models: Number(m[2]) } : null;
};

const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => {
  ws.onopen = res;
  ws.onerror = rej;
});
const cdp = new Cdp(ws);
await cdp.send('Runtime.enable');

let passed = 0;
const ok = (cond, msg) => {
  if (!cond) throw new Error('断言失败：' + msg);
  passed++;
  console.log('✓', msg);
};

// 取一个真实提供方 id
const cfg = await cdp.eval(
  `window.__TAURI_INTERNALS__.invoke('get_llm_config')`,
);
const providerId = cfg?.providers?.[0]?.id;
ok(!!providerId, `应用中存在提供方：${providerId ?? '无'}`);
if (!providerId) process.exit(1);

const probeModel = 'e2e-probe-' + Date.now();

// 面板 1：AI 聊天
ok(await cdp.eval(CLICK_NAV('AI 聊天')), '切换到 AI 聊天面板');
await sleep(600);
const beforeChat = parseCount(await cdp.eval(READ_BADGE));
ok(!!beforeChat, 'AI 聊天面板显示模型计数徽标');
console.log('  AI 聊天初始计数：', JSON.stringify(beforeChat));

// 真实后端命令：添加模型（走与 UI 完全相同的 IPC）
const added = await cdp.eval(
  `window.__TAURI_INTERNALS__.invoke('add_llm_model', { id: ${JSON.stringify(
    providerId,
  )}, model: ${JSON.stringify(probeModel)} })`,
);
ok(added && Array.isArray(added.models) && added.models.includes(probeModel), '后端 add_llm_model 返回包含新模型');

// 不刷新，等待事件广播 + store 刷新
await sleep(1000);

const afterChat = parseCount(await cdp.eval(READ_BADGE));
ok(
  afterChat && afterChat.models === beforeChat.models + 1,
  `AI 聊天面板无需刷新，模型计数 ${beforeChat.models} → ${afterChat.models}`,
);

// 面板 2：AI 文案
ok(await cdp.eval(CLICK_NAV('AI 文案')), '切换到 AI 文案面板');
await sleep(600);
const copyBadge = parseCount(await cdp.eval(READ_BADGE));
ok(
  copyBadge && copyBadge.models === beforeChat.models + 1,
  `AI 文案面板同步显示新模型（${copyBadge?.models} 个模型）`,
);

// 面板 3：大模型管理
ok(await cdp.eval(CLICK_NAV('大模型')), '切换到大模型管理面板');
await sleep(600);
const llmBadge = parseCount(await cdp.eval(READ_BADGE));
ok(
  llmBadge && llmBadge.models === beforeChat.models + 1,
  `大模型管理面板同步显示新模型（${llmBadge?.models} 个模型）`,
);

// 清理：删除探针模型，计数还原
await cdp.eval(
  `window.__TAURI_INTERNALS__.invoke('remove_llm_model', { id: ${JSON.stringify(
    providerId,
  )}, model: ${JSON.stringify(probeModel)} })`,
);
await sleep(1000);
ok(await cdp.eval(CLICK_NAV('AI 聊天')), '切回 AI 聊天面板');
await sleep(600);
const restored = parseCount(await cdp.eval(READ_BADGE));
ok(
  restored && restored.models === beforeChat.models,
  `删除探针模型后计数还原为 ${restored?.models}`,
);

console.log(`\n端到端验证全部通过：${passed} 项`);
ws.close();
process.exit(0);
