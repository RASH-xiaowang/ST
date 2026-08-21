// 批量视觉审查 v3：32B 模型 + 挑剔式提问 + 4 并发
import fs from 'node:fs';
import path from 'node:path';

const KEY = 'sk-mxdftdttxxzldbzxmkphmlifcnsnkpuzesnahvsoxhgnqxvm';
const MODEL = 'Qwen/Qwen3-VL-32B-Instruct';
const SHOT_DIR = 'E:/ST/.codex_shots/wechat_ui';
const OUT = path.join(SHOT_DIR, 'vision_report2.json');

const files = fs.readdirSync(SHOT_DIR).filter((f) => f.endsWith('.png') && f.startsWith('t')).sort();
const question = '你是严格的 UI 审查员。仔细看这张应用界面截图，把每个"不够协调"的细节都列出来，宁可多报不要漏报。关注：①元素间距过大/过小/不一致 ②文字或内容被截断、贴边 ③按钮/卡片/行高不对齐 ④明显空白浪费 ⑤颜色对比弱或风格不统一 ⑥数据数字与标签排版别扭。每条格式："[区域] 问题描述"。如果界面确实很协调，只回答"协调"两个字。';

async function review(f) {
  const img = fs.readFileSync(path.join(SHOT_DIR, f));
  const b64 = img.toString('base64');
  const mime = f.toLowerCase().endsWith('.png') ? 'image/png' : 'image/jpeg';
  const t0 = Date.now();
  try {
    const resp = await fetch('https://api.siliconflow.cn/v1/chat/completions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + KEY },
      body: JSON.stringify({
        model: MODEL,
        max_tokens: 900,
        messages: [{
          role: 'user',
          content: [
            { type: 'image_url', image_url: { url: 'data:' + mime + ';base64,' + b64 } },
            { type: 'text', text: question },
          ],
        }],
      }),
    });
    const txt = await resp.text();
    if (resp.status === 200) {
      const j = JSON.parse(txt);
      return { f, content: j?.choices?.[0]?.message?.content ?? '(空)', ms: Date.now() - t0 };
    }
    return { f, content: 'API ERROR ' + resp.status + ' ' + txt.slice(0, 120), ms: Date.now() - t0 };
  } catch (e) {
    return { f, content: 'EXC ' + String((e && e.message) || e), ms: Date.now() - t0 };
  }
}

const results = [];
let i = 0;
async function worker() {
  while (i < files.length) {
    const f = files[i++];
    const r = await review(f);
    results.push(r);
    console.log('OK', r.f, r.ms + 'ms');
  }
}
await Promise.all([worker(), worker(), worker(), worker()]);

const report = {};
for (const r of results) report[r.f] = r.content;
fs.writeFileSync(OUT, JSON.stringify(report, null, 2), 'utf8');
console.log('report ->', OUT);
