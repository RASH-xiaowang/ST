/* ============================================================
 * 大模型对话 — TTS 音频播放器状态机
 * 自 GlobalChatTab.svelte 下沉：Audio 播放、状态文案、打断解析、
 * 打断监听启停钩子。注意：本文件使用 $state rune。
 * ============================================================ */
import { audioMime } from './voice';

/** 播报状态（$state 可变对象：属性级变更即可驱动重渲染） */
export const ttsPlayer = $state({
  speaking: false,
  speakingIndex: null as number | null,
  audioPlayer: null as HTMLAudioElement | null,
});

/** 组件回调：状态文案 / 错误提示 / 打断监听启停 */
export interface TtsPlayerHooks {
  onStatus: (text: string) => void;
  onMicError: (text: string) => void;
  onBargeStart: () => void;
  onBargeStop: () => void;
}

let hooks: TtsPlayerHooks | null = null;
let playResolve: (() => void) | null = null;

/** 注册组件回调（组件 onMount 时调用一次） */
export function setTtsPlayerHooks(h: TtsPlayerHooks): void {
  hooks = h;
}

/** 把 SpeechResult 组装成 data URL（格式 → MIME） */
export function ttsDataUrl(res: { format: string; audio_data: string }): string {
  return `data:${audioMime(res.format)};base64,${res.audio_data}`;
}

/** 播放一段合成音频；直到播完或被 stopTtsPlayer 打断才 resolve */
export function playTtsAudio(
  src: string,
  msgIndex: number | null,
  opts: { viaNative?: boolean; voiceMode?: boolean; voiceLoop?: boolean } = {},
): Promise<void> {
  return new Promise((resolve) => {
    const player = new Audio(src);
    ttsPlayer.audioPlayer = player;
    ttsPlayer.speaking = true;
    ttsPlayer.speakingIndex = msgIndex;
    const viaNative = !!opts.viaNative;
    const loop = !!opts.voiceMode && !!opts.voiceLoop;
    hooks?.onStatus(
      viaNative
        ? loop
          ? 'AI 正在说话…（系统语音，可直接开口打断）'
          : 'AI 正在说话…（系统语音）'
        : loop
          ? 'AI 正在说话…（可直接开口打断）'
          : 'AI 正在说话…',
    );
    let settled = false;
    const finish = () => {
      if (settled) return;
      settled = true;
      if (playResolve === finish) playResolve = null;
      if (ttsPlayer.audioPlayer === player) ttsPlayer.audioPlayer = null;
      ttsPlayer.speaking = false;
      hooks?.onBargeStop();
      resolve();
    };
    playResolve = finish;
    player.onended = finish;
    player.onerror = () => {
      hooks?.onMicError('语音播放失败，可点击消息旁的喇叭重听');
      finish();
    };
    player
      .play()
      .then(() => hooks?.onBargeStart())
      .catch(() => {
        hooks?.onMicError('语音播放被拦截，可点击消息旁的喇叭重听');
        finish();
      });
  });
}

/** 停止播报：暂停并清空音频、复位状态、resolve 等待中的播放 */
export function stopTtsPlayer(): void {
  const player = ttsPlayer.audioPlayer;
  if (player) {
    try {
      player.pause();
      player.src = '';
    } catch {
      /* 忽略 */
    }
    ttsPlayer.audioPlayer = null;
  }
  ttsPlayer.speaking = false;
  ttsPlayer.speakingIndex = null;
  if (playResolve) {
    const r = playResolve;
    playResolve = null;
    r();
  }
}
