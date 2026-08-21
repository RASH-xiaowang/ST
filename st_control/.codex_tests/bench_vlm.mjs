// VLM 模型速度基准：同一张图，对比多个模型的耗时
import fs from 'node:fs';

const KEY = 'sk-mxdftdttxxzldbzxmkphmlifcnsnkpuzesnahvsoxhgnqxvm';
const img = fs.readFileSync('E:/ST/.codex_shots/wechat_ui/t07-moments.png');
const b64 = img.toString('base64');

const CANDIDATES = [
  'Qwen/Qwen3-VL-8B-Instruct',
  'Qwen/Qwen3-VL-30B-A3B-Instruct',
  'Qwen/Qwen3-VL-32B-Instruct',
  'zai-org/GLM-4.5V',
];

const question = '一句话说明这张截图是哪个功能界面。';

for (const model of CANDIDATES) {
  const t0 = Date.now();
  try {
    const resp = await fetch('https://api.siliconflow.cn/v1/chat/completions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + KEY },
      body: JSON.stringify({
        model,
        max_tokens: 300,
        messages: [{
          role: 'user',
          content: [
            { type: 'image_url', image_url: { url: 'data:image/png;base64,' + b64 } },
            { type: 'text', text: question },
          ],
        }],
      }),
    });
    const txt = await resp.text();
    const ms = Date.now() - t0;
    if (resp.status === 200) {
      const j = JSON.parse(txt);
      const ans = (j?.choices?.[0]?.message?.content ?? '').slice(0, 60);
      console.log(`${model}: ${ms}ms -> ${ans}`);
    } else {
      console.log(`${model}: ${ms}ms -> HTTP ${resp.status} ${txt.slice(0, 100)}`);
    }
  } catch (e) {
    console.log(`${model}: ${Date.now() - t0}ms -> EXC ${String((e && e.message) || e)}`);
  }
}
