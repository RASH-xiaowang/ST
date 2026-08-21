/* ============================================================
 * 通用异步工具
 * ============================================================ */

/** Promise 延时（毫秒） */
export function delay(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
