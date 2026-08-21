/* ============================================================
 * 大模型对话 — 流式语音播报编排
 * 自 GlobalChatTab.svelte 下沉：句子队列、预取、会话令牌、
 * 合成/播放流水线。注意：本文件使用 $state rune。
 * ============================================================ */
import { SpeechQueue, StreamSpeechFeeder } from './voice';
import type { SpeechChunk } from './speechSynth.svelte';

/** 播报状态（$state：active 供回调/UI 判断当前是否有流式播报） */
export const speechFlow = $state({
  active: false,
});

let session = 0;
const queue = new SpeechQueue<SpeechChunk>();
let busy = false;
const feeder = new StreamSpeechFeeder();

/** 当前会话令牌（跨异步判断播报会话是否已失效） */
export function speechSessionId(): number {
  return session;
}

export function isCurrentSpeechSession(sid: number): boolean {
  return sid === session;
}

/** 终止当前流式播报会话（打断 / 新一轮语音对话 / 退出语音模式时调用） */
export function resetSpeechFlow(): void {
  session++;
  queue.reset();
  feeder.reset();
  speechFlow.active = false;
}

/** 把新增 delta 喂入，切出完整句排队播报 */
export function feedStreamSpeech(delta: string): void {
  const complete = feeder.feed(delta);
  if (complete.length) queue.push(...complete);
}

/** 回复结束：把剩余未完成句也入队并启动播报 */
export function finishStreamSpeech(): void {
  const last = feeder.finish();
  if (last.length) queue.push(...last);
}

/** 流式播报工作线程：逐句合成播放，并预取下一条减少句间停顿 */
export async function drainSpeechFlow(opts: {
  synth: (text: string) => Promise<SpeechChunk | null>;
  play: (chunk: SpeechChunk) => Promise<void>;
  isActive: () => boolean;
  onStatus: (text: string) => void;
  onDone: () => void;
}): Promise<void> {
  if (busy) return;
  busy = true;
  const sid = session;
  try {
    while (queue.length > 0) {
      if (sid !== session || !opts.isActive()) break;
      opts.onStatus('正在合成语音…');
      let res = queue.takePrefetched()?.chunk ?? null;
      if (!res) {
        const text = queue.next();
        if (text === undefined) break;
        res = await opts.synth(text);
      }
      if (!res || sid !== session || !opts.isActive()) break;
      const nextText = queue.peek();
      if (nextText !== undefined) {
        opts.synth(nextText)
          .then((r) => {
            if (sid === session && queue.peek() === nextText && r && opts.isActive()) {
              queue.next();
              queue.setPrefetched(nextText, r);
            }
          })
          .catch(() => {});
      }
      await opts.play(res);
    }
  } finally {
    busy = false;
  }
  if (sid === session && opts.isActive() && speechFlow.active) {
    speechFlow.active = false;
    opts.onDone();
  }
}
