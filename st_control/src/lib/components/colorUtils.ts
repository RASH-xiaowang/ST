/* ============================================================
 * 通用颜色工具
 * 自 PreferencesPanel.svelte 下沉：hex → rgba/亮度/可读文字色。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */

/** hex (#RRGGBB) → rgba() 字符串 */
export function hexToRgba(hex: string, alpha: number): string {
  const c = hex.replace('#', '');
  const r = parseInt(c.substring(0, 2), 16);
  const g = parseInt(c.substring(2, 4), 16);
  const b = parseInt(c.substring(4, 6), 16);
  return `rgba(${r},${g},${b},${alpha})`;
}

/** hex → 感知亮度（0~1，Rec.709 系数） */
export function hexLum(hex: string): number {
  const c = hex.replace('#', '');
  const r = parseInt(c.substring(0, 2), 16);
  const g = parseInt(c.substring(2, 4), 16);
  const b = parseInt(c.substring(4, 6), 16);
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
}

/** 色卡上的主文字颜色（按亮度取深/浅） */
export function swatchTextColor(hex: string): string {
  return hexLum(hex) < 0.45 ? 'rgba(235,238,244,0.95)' : 'rgba(24,28,34,0.88)';
}

/** 色卡上的次要文字颜色 */
export function swatchSubColor(hex: string): string {
  return hexLum(hex) < 0.45 ? 'rgba(235,238,244,0.72)' : 'rgba(24,28,34,0.62)';
}

/** 任意 CSS 颜色 → #rrggbb（canvas 解析；透明/非法返回 null） */
export function cssColorToHex(color: string): string | null {
  try {
    const canvas = document.createElement('canvas');
    canvas.width = 1;
    canvas.height = 1;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    ctx.fillStyle = color;
    ctx.fillRect(0, 0, 1, 1);
    const d = ctx.getImageData(0, 0, 1, 1).data;
    // 透明或非法颜色（fillStyle 解析失败）时放弃
    if (d[3] === 0) return null;
    return '#' + [d[0], d[1], d[2]].map((v) => v.toString(16).padStart(2, '0')).join('');
  } catch {
    return null;
  }
}
