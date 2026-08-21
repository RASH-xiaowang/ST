// ============================================================
// 全站 UI 巡检：逐个打开功能 tab → 截图 + 采集面板内按钮清单
// （按钮清单用于发现 tab 内的子视图/跳转入口）
// 运行：node st_control/.codex_tests/ui-audit-all.mjs
// ============================================================
import { writeFileSync, mkdirSync } from 'node:fs';

const CDP_BASE = 'http://127.0.0.1:9222';
// 输出放 data/（vite watch.ignored）：写截图会触发页面热重载，放被监视目录会毁掉执行上下文
const OUT_DIR = 'E:/ST/st_control/data/ui-audit';

const TABS = [
  '首页', 'Harness', 'AI 文案', '智能体', 'AI 角色', '大模型', '自动化',
  '消息通道', '微信数据', '知识库', '数据看板', '数据库', '图文识别',
];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function findTarget() {
  for (let i = 0; i < 60; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page');
      if (t) return t;
    } catch { /* retry */ }
    await sleep(1000);
  }
  throw new Error('CDP 页面目标未找到');
}

const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0;
const pending = new Map();
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) {
    const { resolve, reject } = pending.get(m.id);
    pending.delete(m.id);
    if (m.error) reject(new Error(JSON.stringify(m.error)));
    else resolve(m.result);
  }
};
const send = (method, params = {}) => new Promise((res, rej) => {
  const i = ++id;
  pending.set(i, { resolve: res, reject: rej });
  ws.send(JSON.stringify({ id: i, method, params }));
});
await send('Runtime.enable');
async function evalp(expression) {
  for (let a = 0; a < 4; a++) {
    const r = await send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (r && r.result && r.result.value !== undefined) return r.result.value;
    await sleep(700);
  }
  return undefined;
}

mkdirSync(OUT_DIR, { recursive: true });
const report = [];

// 等待页面就绪（导航项加载完成）
let ready = false;
for (let i = 0; i < 30; i++) {
  const n = await evalp(`document.querySelectorAll('.nav-item').length`);
  if (typeof n === 'number' && n >= 13) { ready = true; break; }
  await sleep(1000);
}
if (!ready) {
  console.log('页面未就绪，巡检中止');
  process.exit(1);
}

for (const tab of TABS) {
  // 点击导航
  const clicked = await evalp(
    `(() => { const b = document.querySelector('.nav-item[title="${tab}"]'); if (!b) return false; b.click(); return true; })()`,
  );
  if (!clicked) { console.log(`SKIP ${tab}（导航不存在）`); continue; }
  await sleep(1800);

  // 采集面板内可见按钮文案（子视图/跳转入口线索）
  const buttons = await evalp(`(() => {
    const panel = [...document.querySelectorAll('button')].filter(b => b.offsetParent !== null);
    const seen = new Set();
    return panel.map(b => (b.textContent || '').replace(/\\s+/g, ' ').trim())
      .filter(t => t.length > 0 && t.length <= 24 && !seen.has(t) && (seen.add(t), true))
      .slice(0, 60);
  })()`);

  // 截整页视口
  const shot = await send('Page.captureScreenshot', { format: 'png' });
  const file = `${OUT_DIR}/${tab}.png`;
  writeFileSync(file, Buffer.from(shot.data, 'base64'));
  report.push({ tab, file, buttons: buttons || [] });
  console.log(`✓ ${tab} 截图 + ${(buttons || []).length} 个按钮`);
}

writeFileSync(`${OUT_DIR}/audit.json`, JSON.stringify(report, null, 1), 'utf8');
console.log('\n巡检完成 →', OUT_DIR);
process.exit(0);
