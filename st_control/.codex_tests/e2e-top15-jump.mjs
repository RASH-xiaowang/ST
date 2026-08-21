// ============================================================
// E2E：数据总览「朋友圈活跃 Top 15」作者 chip → 跳转该好友朋友圈
//   1. 打开数据总览，读取第一个作者 chip 的名字
//   2. 点击 chip → 应切到朋友圈页并带「正在看「name」」过滤徽标
//   3. 等待动态加载，校验卡片作者全部等于目标作者
// 运行：node st_control/.codex_tests/e2e-top15-jump.mjs
// ============================================================

const CDP_BASE = 'http://127.0.0.1:9222';

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function findTarget() {
  for (let i = 0; i < 30; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
      if (t) return t;
    } catch {
      /* 应用尚未就绪 */
    }
    await sleep(1000);
  }
  throw new Error('30 秒内未发现 CDP 页面目标');
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

  /** 轮询表达式直到返回真值（字符串 'false' 视为假） */
  async waitFor(expression, timeoutMs = 20000, stepMs = 500) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        const v = await this.eval(expression);
        if (v && v !== 'false' && v !== 'null' && v !== 'undefined') return v;
      } catch {
        /* 页面重载间隙 */
      }
      await sleep(stepMs);
    }
    return null;
  }
}

const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});
const cdp = new Cdp(ws);

// 强制刷新页面，确保拿到 HMR 后的最新模块
await cdp.send('Page.reload', { ignoreCache: true });
await cdp.waitFor(`document.readyState === 'complete' ? 'ready' : 'false'`, 15000);
await sleep(2000);

// 1) 打开数据总览
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('button')].find((el) => el.textContent.trim() === '数据总览');
  if (b) { b.click(); return true; }
  return false;
})()`);

// 2) 等待作者 chip 出现并读取第一个作者
const first = await cdp.waitFor(`(() => {
  const chips = document.querySelectorAll('.ov-author.ov-author-link');
  if (!chips.length) return 'false';
  const name = chips[0].querySelector('.ov-author-name')?.textContent?.trim() || '';
  const posts = chips[0].querySelector('.ov-author-posts')?.textContent?.trim() || '';
  return JSON.stringify({ name, posts });
})()`, 20000);
if (!first) {
  console.log('FAIL: 数据总览未出现可点击的作者 chip');
  process.exit(1);
}
const targetAuthor = JSON.parse(first);
console.log('TARGET_AUTHOR=' + first);

// 3) 点击 chip，应跳转到该好友的朋友圈
const clicked = await cdp.eval(`(() => {
  const chip = document.querySelector('.ov-author.ov-author-link');
  if (!chip) return 'false';
  chip.click();
  return 'true';
})()`);
if (clicked !== 'true') {
  console.log('FAIL: chip 点击失败');
  process.exit(1);
}

// 4) 校验朋友圈页过滤徽标
const badge = await cdp.waitFor(`(() => {
  const el = document.querySelector('.wc-moments-filtered');
  return el ? el.textContent.trim() : 'false';
})()`, 15000);
console.log('BADGE=' + JSON.stringify(badge));
if (!badge || !badge.includes(targetAuthor.name)) {
  console.log(`FAIL: 未出现「正在看「${targetAuthor.name}」」徽标，实际=${JSON.stringify(badge)}`);
  process.exit(1);
}
console.log('PASS: 已跳转到朋友圈并带作者过滤徽标');

// 5) 等待动态卡片，校验所有可见卡片作者均为目标作者
const cardCheck = await cdp.waitFor(`(() => {
  const cards = document.querySelectorAll('.wc-moment-card');
  if (!cards.length) return 'false';
  const authors = [...document.querySelectorAll('.wc-moment-author')].map((el) => el.textContent.trim());
  const others = authors.filter((n) => n !== ${JSON.stringify(targetAuthor.name)});
  return JSON.stringify({ total: cards.length, authors: authors.slice(0, 3), others });
})()`, 30000);
console.log('CARDS=' + (cardCheck ?? 'null'));
if (!cardCheck) {
  console.log('FAIL: 30 秒内朋友圈动态未加载（可能首次解密耗时更长）');
  process.exit(1);
}
const cc = JSON.parse(cardCheck);
if (cc.others.length > 0) {
  console.log(`FAIL: 存在非目标作者的动态: ${cc.others.join(', ')}`);
  process.exit(1);
}
console.log(`PASS: 共 ${cc.total} 条动态，作者全部为「${targetAuthor.name}」`);

// 6) 校验「返回全部」按钮存在
const backBtn = await cdp.eval(`(() => [...document.querySelectorAll('button')].some((b) => b.textContent.includes('返回全部')) ? 'true' : 'false')()`);
console.log('BACK_BTN=' + backBtn);
if (backBtn !== 'true') {
  console.log('FAIL: 未找到「返回全部」按钮');
  process.exit(1);
}
console.log('PASS: 「返回全部」按钮可用');
console.log('ALL_PASS');
ws.close();
process.exit(0);
