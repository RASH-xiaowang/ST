// ============================================================
// 全站 UI 巡检（子视图）：tab 内的子标签/入口跳转后的界面截图
// 输出到 data/ui-audit/（vite watch.ignored，避免触发热重载）
// 运行：node st_control/.codex_tests/ui-audit-sub.mjs
// ============================================================
import { writeFileSync, mkdirSync } from 'node:fs';

const CDP_BASE = 'http://127.0.0.1:9222';
const OUT_DIR = 'E:/ST/st_control/data/ui-audit';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// [导航tab, 子视图按钮文案(部分匹配), 输出文件名]
const STEPS = [
  ['大模型', '接入配置', '大模型-接入配置'],
  ['大模型', '模型管理', '大模型-模型管理'],
  ['大模型', '流量与成本', '大模型-流量与成本'],
  ['自动化', '规则管理', '自动化-规则管理'],
  ['自动化', '消息与任务', '自动化-消息与任务'],
  ['自动化', '回复机器人', '自动化-回复机器人'],
  ['自动化', '概览', '自动化-概览'],
  ['图文识别', '分类映射', '图文识别-分类映射'],
  ['图文识别', '服务配置', '图文识别-服务配置'],
  ['图文识别', '接入文档', '图文识别-接入文档'],
  ['图文识别', '资源列表', '图文识别-资源列表'],
  ['知识库', 'AI问答', '知识库-AI问答'],
  ['智能体', '已接入 Agent', '智能体-已接入Agent'],
  ['消息通道', '微信', '消息通道-微信tab'],
];

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
    const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (r && r.result && r.result.value !== undefined) return r.result.value;
    await sleep(700);
  }
  return undefined;
}

mkdirSync(OUT_DIR, { recursive: true });

for (const [tab, sub, name] of STEPS) {
  // 导航
  let ok = false;
  for (let a = 0; a < 4 && !ok; a++) {
    ok = (await evalp(
      `(() => { const b = document.querySelector('.nav-item[title="${tab}"]'); if (!b) return false; b.click(); return true; })()`,
    )) === true;
    if (!ok) await sleep(800);
  }
  await sleep(1400);
  // 子视图按钮（部分匹配，优先精确）
  const clicked = await evalp(`(() => {
    const btns = [...document.querySelectorAll('button')].filter(b => b.offsetParent !== null);
    const norm = (t) => (t || '').replace(/\\s+/g, ' ').trim();
    const exact = btns.find(b => norm(b.textContent) === '${sub}');
    const partial = btns.find(b => norm(b.textContent).includes('${sub}'));
    const b = exact || partial;
    if (!b) return false;
    b.click();
    return true;
  })()`);
  await sleep(1500);
  const shot = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(`${OUT_DIR}/${name}.png`, Buffer.from(shot.data, 'base64'));
  console.log(`✓ ${name}（子按钮命中: ${clicked}）`);
}
console.log('子视图巡检完成');
process.exit(0);
