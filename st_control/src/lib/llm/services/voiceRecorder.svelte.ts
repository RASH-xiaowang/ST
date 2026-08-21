/* ============================================================
 * 大模型对话 — 语音录音状态机
 * 自 GlobalChatTab.svelte 下沉：MediaRecorder 捕获 + 电平 VAD +
 * 静音自动停止 / 60s 无语音超时，最终以 Blob 回调交给调用方。
 * 注意：本文件使用 $state rune，扩展名必须是 .svelte.ts。
 * ============================================================ */
import { resolveAudioContext, rmsLevel, vadStep } from './voice';

/** 录音状态（$state 可变对象：属性级变更即可驱动重渲染） */
export const voiceRecorder = $state({
  recording: false,
  micError: '',
});

let recorder: MediaRecorder | null = null;
let audioCtxRef: AudioContext | null = null;
let levelSource: MediaStreamAudioSourceNode | null = null;
let analyser: AnalyserNode | null = null;
let silenceTimer: number | null = null;
let voiceDetected = false;
let silenceStart = 0;
let recStartAt = 0;
let recChunks: Blob[] = [];
let onBlob: ((blob: Blob) => void) | null = null;
let onStatus: ((text: string) => void) | null = null;

/** 首选录音 MIME（不支持 opus 时回退 webm / 空串走浏览器默认） */
function recordMime(): string {
  if (MediaRecorder.isTypeSupported('audio/webm;codecs=opus')) return 'audio/webm;codecs=opus';
  if (MediaRecorder.isTypeSupported('audio/webm')) return 'audio/webm';
  return '';
}

/** 开始录音：捕获 → VAD → 静音/超时自动停止，Blob 交回调用方 */
export function startVoiceRecorder(
  stream: MediaStream,
  hooks: { onBlob: (blob: Blob) => void; onStatus: (text: string) => void },
): void {
  if (voiceRecorder.recording || !stream) return;
  onBlob = hooks.onBlob;
  onStatus = hooks.onStatus;
  recChunks = [];
  voiceDetected = false;
  silenceStart = 0;
  recStartAt = Date.now();
  const mime = recordMime();
  try {
    const rec = new MediaRecorder(stream, mime ? { mimeType: mime } : undefined);
    rec.ondataavailable = (e) => {
      if (e.data.size > 0) recChunks.push(e.data);
    };
    rec.onstop = () => {
      const blob = new Blob(recChunks, { type: mime || 'audio/webm' });
      onBlob?.(blob);
    };
    rec.start(250);
    recorder = rec;
    voiceRecorder.recording = true;
    voiceRecorder.micError = '';
    onStatus?.('正在聆听…');
    startLevelMonitor(stream);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    voiceRecorder.micError = `录音启动失败：${msg}`;
  }
}

/** 停止录音（auto=true 表示静音自动停止，用于状态文案） */
export function stopVoiceRecorder(auto = false): void {
  if (!voiceRecorder.recording) return;
  if (silenceTimer) {
    clearTimeout(silenceTimer);
    silenceTimer = null;
  }
  if (levelSource) {
    try {
      levelSource.disconnect();
    } catch {
      /* 忽略 */
    }
    levelSource = null;
  }
  analyser = null;
  onStatus?.(auto ? '检测到静音，识别中…' : '识别中…');
  const rec = recorder;
  recorder = null;
  voiceRecorder.recording = false;
  if (rec && rec.state !== 'inactive') rec.stop();
}

/** 组件卸载时释放录音资源（停止录音、关闭 AudioContext、清定时器） */
export function releaseVoiceRecorder(): void {
  if (voiceRecorder.recording) stopVoiceRecorder(false);
  audioCtxRef?.close().catch(() => {});
  audioCtxRef = null;
  if (silenceTimer) {
    clearTimeout(silenceTimer);
    silenceTimer = null;
  }
}

/** 录音期间用音量电平做简单 VAD：检测到说话后静音 1.6 秒自动停止 */
function startLevelMonitor(stream: MediaStream): void {
  try {
    if (!audioCtxRef) {
      const Ctx = resolveAudioContext();
      audioCtxRef = new Ctx();
    }
    const ctx = audioCtxRef;
    if (ctx.state === 'suspended') ctx.resume().catch(() => {});
    levelSource = ctx.createMediaStreamSource(stream);
    const an = ctx.createAnalyser();
    an.fftSize = 1024;
    levelSource.connect(an);
    analyser = an;
  } catch {
    analyser = null;
  }
  const tick = () => {
    if (!voiceRecorder.recording) return;
    // 长时间无人说话时自动结束，避免一直挂着录音
    if (!voiceDetected && Date.now() - recStartAt > 60_000) {
      stopVoiceRecorder(false);
      onStatus?.('长时间未检测到语音，已停止聆听');
      return;
    }
    const an = analyser;
    if (an) {
      const buf = new Uint8Array(an.frequencyBinCount);
      an.getByteTimeDomainData(buf);
      const rms = rmsLevel(buf);
      const step = vadStep(rms, { voiced: voiceDetected, silenceStart }, Date.now());
      voiceDetected = step.state.voiced;
      silenceStart = step.state.silenceStart;
      if (step.stop) {
        stopVoiceRecorder(true);
        return;
      }
    }
    silenceTimer = window.setTimeout(tick, 120);
  };
  tick();
}
