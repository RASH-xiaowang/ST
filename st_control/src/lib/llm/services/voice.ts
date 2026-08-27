// 语音对话工具：把 MediaRecorder 产出的音频转为本地 Whisper 需要的
// 16kHz 单声道 16-bit PCM WAV；转换失败时返回 null（前端可回退云端原格式）。

/** 兼容 WebKit 前缀的 AudioContext 构造器（Chrome / Safari / WebView2） */
export function resolveAudioContext(): typeof AudioContext {
  return window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
}

/** 将 PCM 浮点采样编码为 16-bit 单声道 WAV 字节 */
export function encodeWav(samples: Float32Array, sampleRate: number): Uint8Array {
  const buffer = new ArrayBuffer(44 + samples.length * 2);
  const view = new DataView(buffer);
  const writeStr = (offset: number, s: string) => {
    for (let i = 0; i < s.length; i++) view.setUint8(offset + i, s.charCodeAt(i));
  };
  writeStr(0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  writeStr(8, "WAVE");
  writeStr(12, "fmt ");
  view.setUint32(16, 16, true); // fmt 块大小
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // 单声道
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true); // 字节率
  view.setUint16(32, 2, true); // 块对齐
  view.setUint16(34, 16, true); // 位深
  writeStr(36, "data");
  view.setUint32(40, samples.length * 2, true);
  let off = 44;
  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(off, s < 0 ? s * 0x8000 : s * 0x7fff, true);
    off += 2;
  }
  return new Uint8Array(buffer);
}

/** 时域采样（AnalyserNode.getByteTimeDomainData 输出，0..255）的均方根电平（0..1） */
export function rmsLevel(buf: Uint8Array): number {
  let sum = 0;
  for (let i = 0; i < buf.length; i++) {
    const v = (buf[i] - 128) / 128;
    sum += v * v;
  }
  return Math.sqrt(sum / buf.length);
}

/** VAD 状态：是否检测到说话、当前静音起始时间（0 = 尚未开始计时） */
export interface VadState {
  voiced: boolean;
  silenceStart: number;
}

/**
 * VAD 单步决策：根据当前电平推进状态机。
 * - 电平超过阈值 → 标记 voiced 并清零静音计时
 * - 已 voiced 且静音超过 silenceMs → stop=true（自动停止录音）
 * - 其余情况原样返回
 */
export function vadStep(
  rms: number,
  state: VadState,
  now: number,
  opts: { threshold?: number; silenceMs?: number } = {},
): { state: VadState; stop: boolean } {
  const threshold = opts.threshold ?? 0.012;
  const silenceMs = opts.silenceMs ?? 1600;
  if (rms > threshold) {
    return { state: { voiced: true, silenceStart: 0 }, stop: false };
  }
  if (state.voiced) {
    if (state.silenceStart === 0) {
      return { state: { voiced: true, silenceStart: now }, stop: false };
    }
    if (now - state.silenceStart > silenceMs) {
      return { state, stop: true };
    }
  }
  return { state, stop: false };
}

/** TTS 格式 → MIME（未知/空格式按 mp3 处理） */
export function audioMime(fmt: string): string {
  switch ((fmt || 'mp3').toLowerCase()) {
    case 'wav':
      return 'audio/wav';
    case 'ogg':
      return 'audio/ogg';
    case 'flac':
      return 'audio/flac';
    case 'aac':
      return 'audio/aac';
    case 'opus':
      return 'audio/opus';
    case 'mp3':
    default:
      return 'audio/mpeg';
  }
}

/** 一次语音合成候选（提供方 id + 模型） */
export interface SpeechAttempt {
  provider_id?: string | null;
  model?: string | null;
}

/** 语音合成候选提供方（buildSpeechAttempts 输入，只需 id/enabled/models/model_meta） */
export interface SpeechProviderLike {
  id: string;
  enabled?: boolean;
  models?: string[];
  model_meta?: Record<string, { model_type?: string | null }>;
}

/**
 * 构建语音合成候选顺序：当前选中的提供方优先，再按启用提供方的「语音」模型
 * 逐个追加（不去重，保持与原 trySpeech 一致；无效候选由调用方跳过）。
 */
export function buildSpeechAttempts(
  current: { provider_id?: string | null; model?: string | null } | null,
  providers: SpeechProviderLike[],
): SpeechAttempt[] {
  const attempts: SpeechAttempt[] = [
    { provider_id: current?.provider_id ?? null, model: current?.model ?? null },
  ];
  for (const p of providers) {
    if (!p.enabled) continue;
    // 优先：手动标记为「语音」类型的模型
    const marked = p.models?.find((m) => p.model_meta?.[m]?.model_type === '语音');
    if (marked) {
      attempts.push({ provider_id: p.id, model: marked });
      continue;
    }
    // 兜底：模型名含 tts / speech / voice / audio 关键词 → 自动识别为语音模型
    const hit = p.models?.find((m) => {
      const l = m.toLowerCase();
      return l.includes('tts') || l.includes('speech') || l.includes('voice') || l.includes('audio') || l.includes('mimo');
    });
    if (hit) attempts.push({ provider_id: p.id, model: hit });
  }
  return attempts;
}

/** 流式播报句子队列 + 预取槽（纯数据结构，可独立单测） */
export class SpeechQueue<T> {
  private items: string[] = [];
  private prefetched: { text: string; chunk: T } | null = null;

  get length(): number {
    return this.items.length;
  }

  push(...texts: string[]): void {
    this.items.push(...texts);
  }

  peek(): string | undefined {
    return this.items[0];
  }

  next(): string | undefined {
    return this.items.shift();
  }

  setPrefetched(text: string, chunk: T): void {
    this.prefetched = { text, chunk };
  }

  takePrefetched(): { text: string; chunk: T } | null {
    const p = this.prefetched;
    this.prefetched = null;
    return p;
  }

  reset(): void {
    this.items = [];
    this.prefetched = null;
  }
}

/** 解码任意录音 Blob → 16kHz 单声道 WAV 字节；解码失败返回 null */
export async function blobToWav16kMono(blob: Blob): Promise<Uint8Array | null> {
  try {
    const arrayBuf = await blob.arrayBuffer();
    const Ctx = resolveAudioContext();
    const ctx = new Ctx();
    const audioBuffer = await ctx.decodeAudioData(arrayBuf.slice(0));
    const srcRate = audioBuffer.sampleRate;
    const targetRate = 16000;
    const outLen = Math.max(1, Math.ceil((audioBuffer.length * targetRate) / srcRate));
    const offline = new OfflineAudioContext(1, outLen, targetRate);
    const src = offline.createBufferSource();
    src.buffer = audioBuffer;
    src.connect(offline.destination);
    src.start(0);
    const rendered = await offline.startRendering();
    await ctx.close().catch(() => {});
    return encodeWav(rendered.getChannelData(0), targetRate);
  } catch {
    return null;
  }
}

/** 把 Markdown 文本清理成适合朗读的纯文本 */
export function plainTextForSpeech(text: string): string {
  return text
    // 推理模型的思考块：不朗读
        // 表情符号：不朗读
    .replace(/[\u{1F600}-\u{1F64F}\u{1F300}-\u{1F5FF}\u{1F680}-\u{1F6FF}\u{1F900}-\u{1F9FF}\u{2600}-\u{26FF}\u{2700}-\u{27BF}\u{FE00}-\u{FE0F}\u{1F000}-\u{1F02F}\u{1F0A0}-\u{1F0FF}\u{1F100}-\u{1F64F}\u{1F910}-\u{1F96B}\u{1F980}-\u{1F9E0}]/gu, "")
    .replace(/<\s*think\s*>[\s\S]*?<\s*\/\s*think\s*>/gi, " ")
    .replace(/【思考】[\s\S]*?【\/思考】/g, " ")
    // 代码块：简短提示
    .replace(/```[^\n]*\n[\s\S]*?```/g, "，代码如下，")
    .replace(/```[\s\S]*?```/g, "，代码块，")
    // 图片：跳过
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    // 链接：只保留文字
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    // 行内代码：去掉反引号
    .replace(/`([^`]+)`/g, "$1")
    // 标记符号：保留情感标点
    .replace(/[#*_~|]/g, " ")
    // 引用和列表：转为停顿
    .replace(/^>\s*/gm, "，")
    .replace(/^[-*+]\s+/gm, "，")
    .replace(/^\d+\.\s+/gm, "，")
    // 多余空行：转为句号停顿
    .replace(/\n\s*\n/g, "。")
    // 单个换行：转为逗号停顿
    .replace(/\n/g, "，")
    // 多余空格
    .replace(/\s+/g, " ")
    .trim();
}

/** 按中文/英文句读切句：返回完整句与末尾未完成部分 */
export function extractSentences(text: string): { complete: string[]; remainder: string } {
  const trimmed = text.trim();
  if (!trimmed) return { complete: [], remainder: "" };
  const parts = trimmed
    .split(/(?<=[。！？!?；;\n])/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  if (parts.length === 0) return { complete: [], remainder: "" };
  const endsWithBoundary = /[。！？!?；;\n]$/.test(trimmed);
  const complete = endsWithBoundary ? parts : parts.slice(0, -1);
  const remainder = endsWithBoundary ? "" : parts[parts.length - 1];
  return {
    complete: complete.filter((s) => s.length >= 2),
    remainder,
  };
}

/**
 * 流式文本喂入器：LLM 增量输出时按句切分，返回新形成的完整句；
 * 未完成的半句自动衔接下一轮输入，保证跨 chunk 不丢字、不重复。
 */
export class StreamSpeechFeeder {
  private pending = "";

  /** 喂入一段新增文本（增量 delta），返回新形成的完整句 */
  feed(delta: string): string[] {
    if (!delta) return [];
    this.pending += plainTextForSpeech(delta);
    const { complete, remainder } = extractSentences(this.pending);
    this.pending = remainder;
    return complete;
  }

  /** 收尾：返回最后一句未入队的完整句（可能为空数组） */
  finish(): string[] {
    const out = this.pending.trim() ? [this.pending.trim()] : [];
    this.pending = "";
    return out;
  }

  reset() {
    this.pending = "";
  }
}

/** 朗读分段：按句子切分，附带句后停顿与逐句语速（模拟真人节奏） */
export interface SpeechSegment {
  text: string;
  pauseMs: number;
  speed: number;
}

/**
 * 把文本切成朗读分段：
 * - 按句号/问号/感叹号/分号切分，保留标点（TTS 会读出停顿）
 * - 问句：语速稍慢 + 句后长停顿（留给听者反应）
 * - 感叹句：语速稍快 + 句后较长停顿（情绪回响）
 * - 句号：中等停顿；长句额外放缓
 */
export function splitForSpeech(text: string, baseSpeed: number): SpeechSegment[] {
  const cleaned = plainTextForSpeech(text);
  if (!cleaned) return [];
  const sentences = cleaned
    .split(/(?<=[。！？!?；;])/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  return sentences.map((s) => {
    const last = s.slice(-1);
    let pauseMs = 220;   // 默认短停顿（逗号级）
    let speed = baseSpeed;
    if (/[。；;]/.test(last)) {
      pauseMs = 350;     // 句号/分号：中等停顿
      speed = baseSpeed * 1.0;
    } else if (/[！!]/.test(last)) {
      pauseMs = 480;     // 感叹号：较长停顿 + 稍快
      speed = baseSpeed * 1.15;
    } else if (/[？?]/.test(last)) {
      pauseMs = 550;     // 问号：最长停顿 + 稍慢
      speed = baseSpeed * 0.92;
    }
    // 长句：额外放缓，避免赶读
    if (s.length > 50) speed *= 0.97;
    // 短句（<10字）：略快，保持节奏
    if (s.length < 10) speed *= 1.06;
    return { text: s, pauseMs, speed };
  });
}

/** 暂停辅助（毫秒） */
export function delay(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
