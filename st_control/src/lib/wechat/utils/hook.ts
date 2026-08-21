/* ============================================================
 * 微信数据管理模块 — Hook 状态展示纯函数
 * 自 HookManager.svelte 下沉：状态文案与样式类映射。
 * ============================================================ */
import type { ImgHookStatus } from '../services/ipc';

/** Hook 状态文案 */
export function hookStatusLabel(s: ImgHookStatus | null): string {
  if (!s) return '检测中…';
  if (!s.supported) return '不支持';
  if (!s.enabled) return '未启用';
  if (!s.dll_ok) return 'DLL 缺失';
  if (s.hooked) return '正在监控';
  return '等待连接';
}

/** Hook 状态样式类 */
export function hookStatusCls(s: ImgHookStatus | null): string {
  if (!s) return 'hm-status-pending';
  if (!s.enabled) return 'hm-status-off';
  if (!s.dll_ok) return 'hm-status-err';
  if (s.hooked) return 'hm-status-on';
  return 'hm-status-pending';
}
