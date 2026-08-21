// ============================================================
// E2E：朋友圈 HTML 导出（含全部图片/视频资源）
//   1. 作者过滤 + format=html → 导出 HTML + `<名>_media/` 资源目录
//   2. 校验 HTML 结构（头部/动态条目/img/video 引用）
//   3. 校验媒体文件真实有效（JPEG/PNG 魔数、MP4 ftyp）
//   4. UI：格式选择器含 HTML 选项
// 运行：node st_control/.codex_tests/e2e-moments-html-export.mjs
// ============================================================

import fs from 'node:fs';
import path from 'node:path';

const CDP_BASE = 'http://127.0.0.1:9222';
const OUT_DIR = 'E:/ST/st_control/data/ui-audit';
const HTML_PATH = path.join(OUT_DIR, 'moments_html_test.html');
const MEDIA_DIR = path.join(OUT_DIR, 'moments_html_test_media');

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
  throw new Error('40 秒内未发现 CDP 页面目标');
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
    const r = await this.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error('evaluate 异常: ' + JSON.stringify(r.exceptionDetails));
    return r.result.value;
  }
  async waitFor(expression, timeoutMs = 20000, stepMs = 500) {
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

fs.mkdirSync(OUT_DIR, { recursive: true });
fs.rmSync(HTML_PATH, { force: true });
fs.rmSync(MEDIA_DIR, { recursive: true, force: true });

// ── 0) 取作者 ──
const authorRaw = await cdp.eval(`(async () => {
  const d = await window.__TAURI_INTERNALS__.invoke('get_wechat_data_overview');
  const a = (d.moments_authors ?? [])[0];
  return JSON.stringify({ username: a?.username ?? '', name: a?.name ?? '' });
})()`);
const author = JSON.parse(authorRaw);
check(!!author.username, `测试作者: ${author.name}`);

// ── 1) HTML 导出（作者过滤；原图下载耗时较长，交给后端 run_blocking）──
console.log('EXPORTING...  (可能耗时数分钟，正在下载解密全部原图/视频)');
const t0 = Date.now();
const resRaw = await cdp.eval(`(async () => {
  try {
    const r = await window.__TAURI_INTERNALS__.invoke('export_moments', {
      format: 'html',
      authorUsername: ${JSON.stringify(author.username)},
      path: ${JSON.stringify(HTML_PATH.replaceAll('\\', '/'))},
    });
    return JSON.stringify(r);
  } catch (e) { return 'ERROR: ' + String(e); }
})()`);
const elapsed = Math.round((Date.now() - t0) / 1000);
console.log(`EXPORT_DONE in ${elapsed}s: ${resRaw}`);
const r = JSON.parse(resRaw);
check(typeof r.count === 'number' && r.count > 0, `导出动态 count=${r.count}`);
check(typeof r.media === 'number' && r.media > 0, `成功落盘媒体数 media=${r.media}（失败 ${r.media_failed ?? 0}）`);

// ── 2) HTML 文件结构 ──
const html = fs.readFileSync(HTML_PATH, 'utf8');
check(html.startsWith('<!DOCTYPE html>'), 'HTML 文件头正确');
check(html.includes(`<span class="m-author">${author.name}</span>`), `HTML 含作者「${author.name}」`);
const articleCount = (html.match(/<article class="moment">/g) ?? []).length;
check(articleCount === r.count, `HTML 动态条目数(${articleCount}) = count(${r.count})`);
const mediaRel = path.basename(MEDIA_DIR);
const imgRefs = (html.match(new RegExp(`src="${mediaRel}/img_`, 'g')) ?? []).length;
const videoRefs = (html.match(new RegExp(`src="${mediaRel}/vid_`, 'g')) ?? []).length;
console.log(`HTML 引用: img=${imgRefs}, video=${videoRefs}`);
check(imgRefs + videoRefs === r.media, `HTML 媒体引用数(${imgRefs + videoRefs}) = media(${r.media})`);
check(html.includes(`资源目录：${path.basename(MEDIA_DIR)}/`), 'HTML 头部标注资源目录');

// ── 3) 媒体文件真实有效 ──
if (!fs.existsSync(MEDIA_DIR)) {
  check(false, '媒体目录存在');
  process.exit(1);
}
const files = fs.readdirSync(MEDIA_DIR);
check(files.length === r.media, `媒体目录文件数(${files.length}) = media(${r.media})`);
const imgFiles = files.filter((f) => /^img_/.test(f));
const vidFiles = files.filter((f) => /^vid_/.test(f));
const covFiles = files.filter((f) => /^cover_/.test(f));
console.log(`媒体构成: img=${imgFiles.length}, vid=${vidFiles.length}, cover=${covFiles.length}`);

// 抽检前 3 张图魔数
let imgOk = 0;
for (const f of imgFiles.slice(0, 3)) {
  const head = fs.readFileSync(path.join(MEDIA_DIR, f)).subarray(0, 4);
  const sig = head.toString('hex');
  const jpg = sig.startsWith('ffd8');
  const png = sig === '89504e47';
  if (jpg || png) imgOk++;
}
check(imgOk === Math.min(3, imgFiles.length), `抽检 ${imgOk}/${Math.min(3, imgFiles.length)} 张图片魔数有效（JPEG/PNG）`);
// 抽检第一个视频 ftyp
if (vidFiles.length > 0) {
  const head = fs.readFileSync(path.join(MEDIA_DIR, vidFiles[0])).subarray(4, 8).toString('ascii');
  check(head === 'ftyp', `视频 MP4 ftyp 有效 (${vidFiles[0]})`);
} else {
  console.log('（该作者无视频动态，跳过视频校验）');
}

// ── 4) UI：格式选择器含 HTML ──
const opts = await cdp.waitFor(`(() => {
  const sel = document.querySelector('.wc-moments-fmt');
  return sel ? JSON.stringify([...sel.options].map((o) => o.value)) : 'false';
})()`, 20000);
check(opts === JSON.stringify(['csv', 'json', 'txt', 'html']), `格式选择器选项 = ${opts}`);

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
