// ============================================================
// 回归验证：导航跳转 + 布局（可独立运行 exe / dev 双模式复用）
// 通过 WebView2 CDP 驱动：
//   1. 断言页面加载自内置资源（tauri:// 协议，而非 devUrl）
//   2. 依次点击 13 个导航项，断言 active 正确且可见面板恰为 1
//   3. 断言侧边栏/主内容区布局无重叠
//   4. 调用真实后端 IPC，证明 standalone 模式下 Rust 后端可用
// 运行：node st_control/.codex_tests/verify-nav-cdp.mjs [port] [screenshot.png]
// 前置：exe 以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 启动
// ============================================================

import { writeFileSync } from 'node:fs';

const CDP_BASE = `http://127.0.0.1:${process.argv[2] ?? 9222}`;
const SHOT_PATH = process.argv[3] ?? 'C:\\Users\\28361\\Desktop\\ST\\st_control\\data\\ui-audit\\fix-nav-standalone.png';
// 第 4 个参数传 'dev' 时按 devUrl (Vite) 模式断言，否则按独立 exe 模式断言。
const MODE = process.argv[4] ?? 'standalone';

const NAV_TITLES = [
  '首页',
  'Harness',
  'AI 文案',
  '智能体',
  'AI 角色',
  '大模型',
  '自动化',
  '消息通道',
  '微信数据',
  '知识库',
  '数据看板',
  '数据库',
  '图文识别',
];

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function findTarget() {
  for (let i = 0; i < 90; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page' && !x.url.startsWith('about:'));
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

// 1. 页面来源：独立 exe 必须走内置资源（custom protocol）；dev 模式走 Vite devUrl
const origin = await cdp.eval(`({ href: location.href, title: document.title })`);
if (MODE === 'dev') {
  ok(origin.href.startsWith('http://localhost:1420'), `dev 模式加载自 Vite devUrl：${origin.href}`);
} else {
  ok(
    origin.href.startsWith('tauri://') || origin.href.startsWith('http://tauri.localhost'),
    `页面加载自内置资源：${origin.href}`,
  );
  ok(!origin.href.includes('localhost:1420'), '未回退到 devUrl (localhost:1420)');
}
ok(origin.title === 'ST 控制台', `窗口标题正确：${origin.title}`);

// 2. 导航项数量（等待应用外壳渲染完成）
let navReady = false;
for (let i = 0; i < 60; i++) {
  const n = await cdp.eval(`document.querySelectorAll('.nav-item:not(.nav-item-search)').length`);
  if (typeof n === 'number' && n >= 12) { navReady = true; break; }
  await sleep(500);
}
const navCount = await cdp.eval(
  `document.querySelectorAll('.nav-item:not(.nav-item-search)').length`,
);
ok(navReady && navCount === 13, `导航项共 13 个（实际 ${navCount}）`);

// 3. 逐个点击导航，断言跳转 + 布局
const CLICK = (title) =>
  `(() => {
    const b = document.querySelector('.nav-item[title="${title}"]');
    if (!b) return 'missing';
    b.click();
    return 'ok';
  })()`;

const SNAPSHOT = `(() => {
  const active = document.querySelector('.nav-item.active');
  const panels = document.querySelectorAll('.panel:not(.panel-hidden)');
  const hiddenPanels = document.querySelectorAll('.panel.panel-hidden');
  const allPanels = document.querySelectorAll('.panel');
  const sidebar = document.querySelector('.sidebar')?.getBoundingClientRect();
  const content = document.querySelector('main.content')?.getBoundingClientRect();
  const visibleText = [...panels].reduce((n, p) => n + (p.textContent || '').trim().length, 0);
  return {
    activeTitle: active?.getAttribute('title') ?? null,
    visiblePanels: panels.length,
    // 真实可见性：.panel-hidden 必须实际 display:none（Svelte 5 下样式必须由
    // PanelSection 自持，否则所有面板同屏堆叠）
    hiddenCount: allPanels.length - panels.length,
    hiddenRealHidden: [...hiddenPanels].every((p) => getComputedStyle(p).display === 'none'),
    visibleText,
    sidebar: sidebar ? { x: sidebar.x, w: sidebar.width, right: sidebar.right } : null,
    content: content ? { x: content.x, w: content.width, left: content.left, right: content.right } : null,
  };
})()`;

let last = null;
for (const title of NAV_TITLES) {
  const clickRes = await cdp.eval(CLICK(title));
  ok(clickRes === 'ok', `点击导航「${title}」`);
  await sleep(400);
  last = await cdp.eval(SNAPSHOT);
  ok(last.activeTitle === title, `激活导航切换为「${title}」（实际 ${last.activeTitle}）`);
  ok(last.visiblePanels === 1, `可见面板恰为 1 个（实际 ${last.visiblePanels}）`);
  ok(last.hiddenCount === 12, `其余面板隐藏（实际 ${last.hiddenCount} 个）`);
  ok(last.hiddenRealHidden, '隐藏面板真实 display:none（非仅缺类名）');
  ok(last.visibleText > 0, `面板已渲染内容（文本 ${last.visibleText} 字符）`);
}

// 4. 布局断言：侧边栏贴左、内容区在侧边栏右侧且无重叠
ok(last.sidebar && Math.abs(last.sidebar.x) < 1 && Math.abs(last.sidebar.w - 232) <= 2, '侧边栏位于 x=0 且宽度 232');
ok(last.content && last.content.left >= last.sidebar.right - 1, '主内容区位于侧边栏右侧，无重叠');
ok(last.content && last.content.w >= 1300, `主内容区宽度正常（${last.content.w}px）`);

// 5. 真实后端 IPC：standalone 模式下 Rust 命令仍可用
const cfg = await cdp.eval(`window.__TAURI_INTERNALS__.invoke('get_llm_config')`);
ok(cfg && typeof cfg === 'object' && Array.isArray(cfg.providers), '后端 IPC get_llm_config 调用成功');

// 6. 截图留档
const shot = await cdp.send('Page.captureScreenshot', { format: 'png' });
if (shot?.data) {
  writeFileSync(SHOT_PATH, Buffer.from(shot.data, 'base64'));
  console.log('📸 截图已保存：', SHOT_PATH);
}

console.log(`\n导航/布局回归验证全部通过：${passed} 项`);
ws.close();
process.exit(0);
