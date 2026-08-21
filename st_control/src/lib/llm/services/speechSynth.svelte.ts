/* ============================================================
 * 大模型对话 — TTS 单句合成（提供方 → 系统原生兜底）
 * 自 GlobalChatTab.svelte 下沉：提供方 TTS 失败缓存、原生语音回退、
 * 引擎/错误状态回写钩子。注意：本文件使用 $state rune。
 * ============================================================ */
import type { SpeechResult } from '../types';

/** 合成结果块（提供方音频，可选 viaNative 标记系统语音） */
export type SpeechChunk = { kind: 'provider'; res: SpeechResult; viaNative?: boolean };

/** 合成状态（$state 可变对象：本会话内提供方 TTS 失败后不再重试） */
export const speechSynth = $state({
  providerTtsFailed: false,
});

/** 组件回调：提供方合成 / 原生合成 / 引擎与错误状态回写 */
export interface SpeechSynthHooks {
  tryProvider: (text: string) => Promise<SpeechResult | null>;
  synthesizeNative: (text: string) => Promise<SpeechResult | null>;
  onEngine: (label: string) => void;
  onError: (text: string) => void;
}

let hooks: SpeechSynthHooks | null = null;

/** 注册组件回调（组件 onMount 时调用一次） */
export function setSpeechSynthHooks(h: SpeechSynthHooks): void {
  hooks = h;
}

/** 单句合成：优先提供方 TTS；不可用时回退系统原生语音（零配置） */
export async function synthOneSpeech(text: string): Promise<SpeechChunk | null> {
  let providerErr = '';
  if (!speechSynth.providerTtsFailed) {
    try {
      const res = await hooks?.tryProvider(text);
      if (res) {
        speechSynth.providerTtsFailed = false;
        hooks?.onEngine(res.provider_name || res.model || '提供方 TTS');
        return { kind: 'provider', res };
      }
    } catch (e) {
      const msg = (e as { message?: string } | null)?.message ?? String(e);
      providerErr = msg;
      // 提供方 TTS 不可用（如当前模型不支持 /audio/speech），本会话内不再重试
      speechSynth.providerTtsFailed = true;
    }
  }
  // 提供方 TTS 不可用 → Windows SAPI 离线合成（零配置，不依赖 WebView2 speechSynthesis）
  try {
    const res = await hooks?.synthesizeNative(text);
    if (res?.audio_data) {
      hooks?.onEngine('Windows 系统语音');
      return { kind: 'provider', res, viaNative: true };
    }
  } catch (e) {
    const msg = (e as { message?: string } | null)?.message ?? String(e);
    hooks?.onError(`系统语音合成失败：${msg}`);
  }
  hooks?.onError(`语音合成失败：${providerErr || '未找到可用的语音合成模型'}`);
  return null;
}
