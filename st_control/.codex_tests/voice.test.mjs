// 语音对话纯逻辑测试：句子切分 / 流式喂入 / WAV 编码
// 运行：node .codex_tests/voice.test.mjs
import assert from "node:assert/strict";
import {
  plainTextForSpeech,
  extractSentences,
  StreamSpeechFeeder,
  encodeWav,
  rmsLevel,
  vadStep,
  audioMime,
  buildSpeechAttempts,
  SpeechQueue,
} from "../src/lib/llm/services/voice.ts";

// ─── 时域均方根电平 ───
assert.equal(rmsLevel(new Uint8Array([128, 128, 128])), 0); // 静音
assert.ok(Math.abs(rmsLevel(new Uint8Array([128, 255, 0])) - Math.sqrt((Math.pow(127 / 128, 2) + 1) / 3)) < 1e-9);
assert.ok(rmsLevel(new Uint8Array([128, 160, 96, 128])) > 0); // 有能量
assert.ok(Number.isNaN(rmsLevel(new Uint8Array(0)))); // 空缓冲与原地实现一致

// ─── VAD 状态机：静音计时 / 说话标记 / 自动停止 ───
const idle = { voiced: false, silenceStart: 0 };
const first = vadStep(0.0, idle, 1000);
assert.deepEqual(first.state, idle); // 未 voiced 的静音不启动计时
const speak = vadStep(0.5, idle, 1000);
assert.deepEqual(speak, { state: { voiced: true, silenceStart: 0 }, stop: false }); // 超过阈值 → voiced
const t0 = vadStep(0.0, { voiced: true, silenceStart: 0 }, 2000);
assert.deepEqual(t0, { state: { voiced: true, silenceStart: 2000 }, stop: false }); // 静音开始计时
const t1 = vadStep(0.0, t0.state, 3000);
assert.deepEqual(t1, { state: t0.state, stop: false }); // 未满 1.6s 不停止
const t2 = vadStep(0.0, t0.state, 4000);
assert.equal(t2.stop, true); // 超过 1.6s → 自动停止
assert.equal(vadStep(0.02, t0.state, 4000).state.voiced, true); // 重新说话 → 重新 voiced

// ─── TTS 格式 → MIME 映射 ───
assert.equal(audioMime("wav"), "audio/wav");
assert.equal(audioMime("OGG"), "audio/ogg");
assert.equal(audioMime("flac"), "audio/flac");
assert.equal(audioMime("aac"), "audio/aac");
assert.equal(audioMime("opus"), "audio/opus");
assert.equal(audioMime("mp3"), "audio/mpeg");
assert.equal(audioMime(""), "audio/mpeg"); // 空格式按 mp3 处理
assert.equal(audioMime("m4a"), "audio/mpeg"); // 未知格式回退 mpeg

// ─── 语音合成候选顺序：当前选中优先，再按启用提供方的「语音」模型 ───
const providers = [
  { id: "p1", enabled: true, models: ["m1", "m2"], model_meta: { m2: { model_type: "语音" } } },
  { id: "p2", enabled: false, models: ["m3"], model_meta: { m3: { model_type: "语音" } } },
  { id: "p3", enabled: true, models: ["m4"], model_meta: { m4: { model_type: "对话" } } },
];
assert.deepEqual(
  buildSpeechAttempts({ provider_id: "cur", model: "curM" }, providers),
  [
    { provider_id: "cur", model: "curM" },
    { provider_id: "p1", model: "m2" },
  ],
);
assert.deepEqual(buildSpeechAttempts(null, providers), [
  { provider_id: null, model: null },
  { provider_id: "p1", model: "m2" },
]); // 未选中也保留首个空候选（与原逻辑一致，调用方跳过）
assert.deepEqual(buildSpeechAttempts({ provider_id: null, model: null }, []), [
  { provider_id: null, model: null },
]); // 无提供方时仅剩空候选

// ─── 播报句子队列 + 预取槽 ───
const q = new SpeechQueue();
assert.equal(q.length, 0);
q.push("你好。", "世界。");
assert.equal(q.length, 2);
assert.equal(q.peek(), "你好。");
assert.equal(q.next(), "你好。");
assert.equal(q.next(), "世界。");
assert.equal(q.next(), undefined); // 空队列取不到
q.setPrefetched("预取句", "chunk-a");
assert.deepEqual(q.takePrefetched(), { text: "预取句", chunk: "chunk-a" });
assert.equal(q.takePrefetched(), null); // 取后清空
q.push("A");
q.setPrefetched("A", "chunk-b");
q.reset();
assert.equal(q.length, 0);
assert.equal(q.takePrefetched(), null); // reset 同时清空预取

// ─── Markdown 清理 ───
assert.equal(plainTextForSpeech("**你好**，`世界`！"), "你好 ， 世界 ！");
assert.equal(plainTextForSpeech("```js\ncode\n``` 继续"), "代码块 继续");
assert.equal(plainTextForSpeech("[链接](https://x.com) 内容"), "链接 内容");
assert.equal(plainTextForSpeech("![图](a.png) 图片"), "图片");

// ─── 句子切分 ───
assert.deepEqual(extractSentences("你好。你好吗？"), {
  complete: ["你好。", "你好吗？"],
  remainder: "",
});
assert.deepEqual(extractSentences("你好。你"), {
  complete: ["你好。"],
  remainder: "你",
});
assert.deepEqual(extractSentences("没有标点"), {
  complete: [],
  remainder: "没有标点",
});

// ─── 流式喂入：跨 chunk 拼接、不重复、不丢字 ───
const f = new StreamSpeechFeeder();
assert.deepEqual(f.feed("你好。你"), ["你好。"]);
assert.deepEqual(f.feed("好吗？"), ["你好吗？"]);
assert.deepEqual(f.finish(), []);
assert.deepEqual(f.feed("再说"), []);
assert.deepEqual(f.finish(), ["再说"]);

// 空输入与 reset
assert.deepEqual(f.feed(""), []);
assert.deepEqual(f.finish(), []);
f.reset();
assert.deepEqual(f.feed("只说一句"), []);
assert.deepEqual(f.finish(), ["只说一句"]);

// Markdown 跨 chunk（如 `**` 被拆到两段）也能正常拼接
const g = new StreamSpeechFeeder();
assert.deepEqual(g.feed("**你好"), []);
assert.deepEqual(g.feed("。**"), ["你好。"]);

// ─── WAV 编码：标准 44 字节头 + PCM16 单声道 ───
const samples = new Float32Array([0, 0.5, -0.5, 1, -1]);
const wav = encodeWav(samples, 16000);
assert.equal(wav.length, 44 + samples.length * 2);
assert.equal(String.fromCharCode(...wav.slice(0, 4)), "RIFF");
assert.equal(String.fromCharCode(...wav.slice(8, 12)), "WAVE");
const view = new DataView(wav.buffer, wav.byteOffset, wav.byteLength);
assert.equal(view.getUint16(20, true), 1); // PCM
assert.equal(view.getUint16(22, true), 1); // 单声道
assert.equal(view.getUint32(24, true), 16000);
assert.equal(view.getUint16(34, true), 16); // 位深
assert.equal(String.fromCharCode(...wav.slice(36, 40)), "data");
assert.equal(view.getInt16(44, true), 0);
assert.equal(view.getInt16(46, true), Math.trunc(0.5 * 0x7fff));
assert.equal(view.getInt16(48, true), Math.trunc(-0.5 * 0x8000));

console.log("voice.test.mjs: 全部通过");
