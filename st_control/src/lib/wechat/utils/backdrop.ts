/* ============================================================
 * 微信模块 — Gargantua 黑洞背景 iframe 参数构建（纯函数）
 * 自 GargantuaBackdrop.svelte 下沉：URLSearchParams 参数组装。
 * 语义逐字保持：steps/cam 走 truthy 判断，bright/star/sky 走非 null。
 * ============================================================ */

/** 构建 Gargantua 背景 iframe URL（含默认 bg/q 参数） */
export function gargantuaFrameUrl(opts: {
  steps?: number;
  cam?: string;
  /** 组件侧默认 true（motion = true 时不带 nocine） */
  motion?: boolean;
  bright?: number | null;
  star?: number | null;
  sky?: number | null;
}): string {
  const p = new URLSearchParams({ bg: '1', q: 'standard' });
  if (opts.steps) p.set('steps', String(opts.steps));
  if (opts.cam) p.set('cam', opts.cam);
  if (opts.motion === false) p.set('nocine', '1');
  if (opts.bright != null) p.set('bright', String(opts.bright));
  if (opts.star != null) p.set('star', String(opts.star));
  if (opts.sky != null) p.set('sky', String(opts.sky));
  return `/gargantua/index.html?${p.toString()}`;
}
