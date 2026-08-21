/* ============================================================
 * 通用 API URL 构造
 * 自 ApiHelpModal.svelte 下沉：本地 HTTP API 调试地址拼接。
 * ============================================================ */

/** 本地 API 调试地址：<base><path>，可选附带 access_token */
export function apiDebugUrl(path: string, port: number, token?: string | null): string {
  const sep = path.includes('?') ? '&' : '?';
  return `http://127.0.0.1:${port}${path}${token ? `${sep}access_token=${encodeURIComponent(token)}` : ''}`;
}
