/* ============================================================
 * 知识库 — 文件展示/解析纯函数
 * 自 KbDocs.svelte 下沉：文件图标、预览 MIME、标签解析、目录展平。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */
import type { DirNode, KbSummary } from './kbTypes';

/** 文档状态 → 中文标签 */
export const STATUS_LABEL: Record<string, string> = {
  ready: '解析完成', processing: '解析中', pending: '待解析', failed: '解析失败',
};

/** 文档来源 → 中文标签 */
export const SOURCE_LABEL: Record<string, string> = {
  upload: '文件上传', fetch: '网页抓取', manual: '手动编辑',
};

/** 检索模式 → 中文标签 */
export const MODE_LABEL: Record<string, string> = {
  hybrid: '混合', vector: '向量', bm25: '全文',
};

/** 文件类型 → 图标名（未知回退 file） */
export function fileIco(t: string | null): string {
  if (!t) return 'file';
  if (t === 'pdf') return 'filePdf';
  if (['doc', 'docx', 'docm', 'rtf', 'odt', 'epub'].includes(t)) return 'fileDoc';
  if (['xls', 'xlsx', 'xlsm', 'xlsb', 'ods'].includes(t)) return 'fileXlsx';
  if (['ppt', 'pptx', 'pptm', 'pps', 'ppsx', 'ppsm', 'pot', 'odp'].includes(t)) return 'fileDoc';
  if (t === 'md' || t === 'markdown') return 'fileMd';
  if (t === 'csv') return 'fileCsv';
  return 'file';
}

/** 文件扩展名 → MIME（未知回退 octet-stream） */
export function previewMime(ft: string | null): string {
  const map: Record<string, string> = {
    pdf: 'application/pdf', png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg',
    gif: 'image/gif', webp: 'image/webp', bmp: 'image/bmp',
    md: 'text/markdown', txt: 'text/plain',
  };
  return map[ft ?? ''] ?? 'application/octet-stream';
}

/** 标签字符串 → 去重数组（中英文逗号/分号分隔，≤30 字符） */
export function parseTags(s: string): string[] {
  return [...new Set(s.split(/[,，;；]/).map((t) => t.trim()).filter((t) => t && t.length <= 30))];
}

/** 目录树 → 扁平列表（含深度，供移动目标选择） */
export function flattenDirs(nodes: DirNode[], depth = 0, out: { id: number; name: string; depth: number }[] = []): { id: number; name: string; depth: number }[] {
  for (const n of nodes) {
    out.push({ id: n.id, name: n.name, depth });
    flattenDirs(n.children, depth + 1, out);
  }
  return out;
}


/** 知识库首字母（emoji 开头回退 K） */
export function kbMonogram(name: string): string {
  const c = name.trim().charAt(0);
  return /[\p{Extended_Pictographic}]/u.test(c) ? 'K' : c.toUpperCase();
}

/** 趋势指示符：负值 ▼，否则 ▲；'--'/空 无指示 */
export function trendArrow(v: string): string {
  if (v === '--' || v === '') return '';
  return v.startsWith('-') ? '▼ ' : '▲ ';
}

/** 趋势样式类：负值 down，否则 up；'--'/空 无样式 */
export function trendClass(v: string): string {
  if (v === '--' || v === '') return '';
  return v.startsWith('-') ? 'kb-trend-down' : 'kb-trend-up';
}

/** 按名称/描述关键词过滤知识库（空白关键词返回原数组引用） */
export function filterKbsByKeyword(kbs: KbSummary[], keyword: string): KbSummary[] {
  const q = keyword.trim().toLowerCase();
  if (!q) return kbs;
  return kbs.filter(
    (k) => k.name.toLowerCase().includes(q) || (k.description ?? '').toLowerCase().includes(q),
  );
}
