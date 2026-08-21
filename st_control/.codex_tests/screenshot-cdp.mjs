// ============================================================
// 截图冒烟：通过 WebView2 CDP 对运行中的应用截图
// 运行：node st_control/.codex_tests/screenshot-cdp.mjs [out.png] [waitMs] [navTitle]
// 前置：应用以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 启动
// ============================================================
const OUT = process.argv[2] || 'E:/ST/_shots/fancyui-shell.png';
const WAIT_MS = Number(process.argv[3] || 5000);
const NAV_TITLE = process.argv[4] || '';
const CDP_BASE = 'http://127.0.0.1:9222';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function findTarget() {
  for (let i = 0; i < 60; i++) {
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
  throw new Error('60 秒内未发现 CDP 页面目标');
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
await cdp.send('Page.enable');

// 可选：点击侧边栏导航切换到指定面板
if (NAV_TITLE) {
  const clicked = await cdp.eval(
    `(() => { const b = document.querySelector('.nav-item[title="${NAV_TITLE}"]'); if (!b) return false; b.click(); return true; })()`
  );
  console.log('切换导航:', NAV_TITLE, clicked ? '✓' : '（未找到）');
}

// 等动画/数据就绪
await sleep(WAIT_MS);

const info = await cdp.eval(`(() => ({
  title: document.title,
  url: location.href,
  navActive: document.querySelector('.nav-item.active .nav-text')?.textContent?.trim() || '',
  statCount: document.querySelectorAll('.monitor-stat').length,
  sparkles: !!document.querySelector('.sparkles-text'),
  glowBorder: document.querySelectorAll('.animate-glow').length,
  beam: !!document.querySelector('.border-beam'),
  canvas: document.querySelectorAll('.app-ambient canvas').length,
}))()`);
console.log('页面信息:', JSON.stringify(info, null, 2));

const shot = await cdp.send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
const fs = await import('node:fs');
const path = await import('node:path');
fs.mkdirSync(path.dirname(OUT), { recursive: true });
fs.writeFileSync(OUT, Buffer.from(shot.data, 'base64'));
console.log('截图已保存:', OUT);
ws.close();
process.exit(0);
