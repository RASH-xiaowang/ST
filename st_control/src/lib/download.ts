/* ============================================================
 * 通用浏览器文件下载工具（跨 feature 共享）
 * 收敛 GeneralRecords / PrivacyScan 等处的
 * Blob + createObjectURL + a.click 重复实现。
 * ============================================================ */

/** 浏览器端下载 Blob 为文件（自动触发下载并释放对象 URL） */
export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
