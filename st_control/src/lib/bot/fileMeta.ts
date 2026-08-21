/* ============================================================
 * 消息通道 — 选中文件元信息（纯函数）
 * 自 BotPanel.svelte 下沉：路径 → 展示名 + 类型分类，可独立单测。
 * 注：分类集与 wechat 的 extTone 不同（webm/flv/amr/ogg 等），
 * 保持原组件语义，不与其他分类器合并（T-298 记录在案）。
 * ============================================================ */

export type FileKind = 'image' | 'video' | 'audio' | 'file';

export interface FileMeta {
  name: string;
  kind: FileKind;
}

/** 文件路径 → 展示名与类型分类（末段为文件名） */
export function fileMetaOf(path: string): FileMeta {
  const name = path.split(/[\\/]/).pop() ?? path;
  const lower = name.toLowerCase();
  let kind: FileKind = 'file';
  if (/\.(png|jpe?g|gif|webp|bmp|heic|heif)$/.test(lower)) kind = 'image';
  else if (/\.(mp4|mov|avi|mkv|webm|flv)$/.test(lower)) kind = 'video';
  else if (/\.(silk|amr|wav|mp3|m4a|ogg)$/.test(lower)) kind = 'audio';
  return { name, kind };
}
