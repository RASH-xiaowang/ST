// 知识库全局共享设置（Svelte 5 runes 模块）
// 供「设置」页与文档上传/重处理共用，避免各页面各自维护一份分块参数。
import { invoke } from '@tauri-apps/api/core';

export const kbChunkCfg = $state<{
  strategy: 'recursive' | 'title' | 'parent_child';
  size: number;
  overlap: number;
}>({
  strategy: 'recursive',
  size: 800,
  overlap: 128,
});

// 从后端加载已保存的分块设置（首次进入知识库时调用）
export async function loadKbChunkCfg() {
  try {
    const s = await invoke<{ strategy: string; size: number; overlap: number }>('kb_get_chunk_settings');
    if (s?.strategy === 'recursive' || s?.strategy === 'title' || s?.strategy === 'parent_child') {
      kbChunkCfg.strategy = s.strategy;
    }
    if (typeof s?.size === 'number' && s.size > 0) kbChunkCfg.size = s.size;
    if (typeof s?.overlap === 'number' && s.overlap >= 0) kbChunkCfg.overlap = s.overlap;
  } catch {
    /* 未初始化数据库时忽略 */
  }
}

let saveTimer: ReturnType<typeof setTimeout> | null = null;
// 防抖保存：设置页修改后自动持久化，重启后依然生效
export function saveKbChunkCfg() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    invoke('kb_set_chunk_settings', {
      strategy: kbChunkCfg.strategy,
      size: kbChunkCfg.size,
      overlap: kbChunkCfg.overlap,
    }).catch(() => {
      /* 保存失败时保持内存值，下次修改重试 */
    });
  }, 350);
}
