// ============================================================
// 大模型配置 — 全局共享响应式存储
// ============================================================
import { errText } from '../format';
// 所有使用模型的界面统一从这里读取 / 订阅：
//  - 后端任意配置变更（新增/删除提供方、添加/删除模型、设置默认等）都会广播
//    llm-config-changed 事件；
//  - 本模块监听该事件并重新拉取配置，同时通知所有订阅者（如知识库、智能体等
//    通过 kb_list_models 取模型列表的界面）实时刷新，无需人工点击「刷新」。
// 注意：此文件必须是 .svelte.ts 扩展名（使用了 $state rune）。

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { llmApi } from './services/ipc';
import type { LlmConfig } from './types';

/** 与后端 llm::handlers::LLM_CONFIG_CHANGED_EVENT 保持一致 */
export const LLM_CONFIG_CHANGED_EVENT = 'llm-config-changed';

export const llmStore = $state<{
  config: LlmConfig;
  loading: boolean;
  error: string;
  /** 每次成功刷新自增；组件可用 $effect 观察它来响应配置变化 */
  revision: number;
}>({
  config: { providers: [], default_provider_id: null },
  loading: true,
  error: '',
  revision: 0,
});

type ChangeListener = () => void;

const listeners = new Set<ChangeListener>();
let started = false;
let unlisten: UnlistenFn | null = null;

function notifyChanged() {
  for (const cb of listeners) {
    try {
      cb();
    } catch (e) {
      console.warn('[llm-store] 订阅者回调异常:', e);
    }
  }
}

/**
 * 拉取最新的大模型配置并广播给所有订阅者。
 * @param opts.silent 后台事件触发的刷新为 true：不切换 loading 状态，避免界面闪烁
 */
export async function refreshLlmConfig(opts: { silent?: boolean } = {}) {
  if (!opts.silent) {
    llmStore.loading = true;
  }
  llmStore.error = '';
  try {
    llmStore.config = await llmApi.getConfig();
    llmStore.revision += 1;
    notifyChanged();
  } catch (e: unknown) {
    llmStore.error = `配置加载失败：${errText(e)}`;
    console.error('[llm-store] refreshLlmConfig', e);
  } finally {
    if (!opts.silent) {
      llmStore.loading = false;
    }
  }
}

/**
 * 订阅大模型配置变更（刷新成功后回调）。返回取消订阅函数。
 * 适用于通过 kb_list_models 等派生接口取模型列表的界面（知识库、智能体）。
 */
export function onLlmConfigChanged(cb: ChangeListener): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

/** 幂等启动：全局只注册一次 Tauri 事件监听（建议在 App 挂载时调用一次） */
export function startLlmSync() {
  if (started) return;
  started = true;
  listen<unknown>(LLM_CONFIG_CHANGED_EVENT, () => {
    // 后台静默刷新：不闪 loading，直接把最新配置推送到所有界面
    refreshLlmConfig({ silent: true });
  })
    .then((fn) => {
      unlisten = fn;
    })
    .catch((e) => {
      console.warn('[llm-store] 监听 llm-config-changed 失败:', e);
    });
}

/** 仅供测试 / 需要主动断开监听时使用 */
export function stopLlmSync() {
  unlisten?.();
  unlisten = null;
  started = false;
}
