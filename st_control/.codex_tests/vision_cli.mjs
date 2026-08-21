// ============================================================
// 视觉识别 CLI：供 DSH 动态插件 vision-1 调用
// 用法: node vision_cli.mjs <请求JSON路径>
// 请求 JSON: { imagePath, question, key, model?, maxTokens? }
// 输出: stdout 打印纯文本答案；失败打印 {error:...} 并退出码 1
// ============================================================
import fs from 'node:fs';

const reqPath = process.argv[2];
if (!reqPath) {
  console.log(JSON.stringify({ error: '缺少请求文件参数' }));
  process.exit(1);
}
let req;
try {
  req = JSON.parse(fs.readFileSync(reqPath, 'utf8'));
} catch (e) {
  console.log(JSON.stringify({ error: '请求文件解析失败: ' + String(e) }));
  process.exit(1);
}

const model = req.model || 'Qwen/Qwen3-VL-8B-Instruct';
const maxTokens = req.maxTokens || 1200;

try {
  const img = fs.readFileSync(req.imagePath);
  const b64 = img.toString('base64');
  const lower = String(req.imagePath).toLowerCase();
  const mime = lower.endsWith('.png') ? 'image/png'
    : lower.endsWith('.webp') ? 'image/webp'
    : lower.endsWith('.gif') ? 'image/gif'
    : 'image/jpeg';

  const resp = await fetch('https://api.siliconflow.cn/v1/chat/completions', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + req.key },
    body: JSON.stringify({
      model,
      max_tokens: maxTokens,
      messages: [{
        role: 'user',
        content: [
          { type: 'image_url', image_url: { url: 'data:' + mime + ';base64,' + b64 } },
          { type: 'text', text: req.question },
        ],
      }],
    }),
  });
  const txt = await resp.text();
  if (resp.status === 200) {
    const j = JSON.parse(txt);
    const content = j?.choices?.[0]?.message?.content;
    console.log(typeof content === 'string' ? content : JSON.stringify(content));
    process.exit(0);
  }
  console.log(JSON.stringify({ error: '视觉 API status=' + resp.status + ' ' + txt.slice(0, 200) }));
  process.exit(1);
} catch (e) {
  console.log(JSON.stringify({ error: '视觉识别失败: ' + String((e && e.message) || e) }));
  process.exit(1);
}
