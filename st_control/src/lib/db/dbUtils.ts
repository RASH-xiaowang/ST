/* ============================================================
 * 数据库管理 — 纯工具函数
 * 自 DbManager.svelte 下沉：CSV 转义/导出、BLOB 处理、格式化。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */
import { formatBytes } from '../format';
import type { DbFileEntry } from './types';

/** CSV 字段转义（RFC 4180：含逗号/引号/换行时加引号并双写引号） */
export function csvEscape(v: unknown): string {
  const s = String(v ?? '');
  return /[",\r\n]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s;
}

/** UTF-8 字符串 → base64（供后端写文件，中文/emoji 安全） */
export function utf8ToBase64(s: string): string {
  const bytes = new TextEncoder().encode(s);
  let bin = '';
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin);
}

/** 是否为 BLOB 预览文本（hex…[N bytes]） */
export function isBlobPreview(s: string): boolean {
  return /…\[\d+ bytes\]$/.test(s);
}

/** BLOB → data URL（图片预览 / 下载） */
export function blobDataUrl(data: { mime?: string; base64: string }): string {
  return `data:${data.mime || 'application/octet-stream'};base64,${data.base64}`;
}

/** MIME → 文件扩展名 */
export function blobExt(mime: string): string {
  const m: Record<string, string> = {
    'image/png': 'png', 'image/jpeg': 'jpg', 'image/gif': 'gif',
    'image/webp': 'webp', 'image/bmp': 'bmp', 'application/pdf': 'pdf',
    'video/mp4': 'mp4',
  };
  return m[mime] || 'bin';
}

/** 字节格式化（0 → "0 B"，1KB+ 保留 1 位小数） */
export function fmtBytes(n: number): string {
  return formatBytes(n);
}

/** 用 canvas measureText 测量文字宽度（性能高，不修改 DOM；canvas 复用单例） */
let measureCanvas: HTMLCanvasElement | null = null;
export function measureTextWidth(text: string, font: string): number {
  const canvas = (measureCanvas ??= document.createElement('canvas'));
  const ctx = canvas.getContext('2d');
  if (!ctx) return 0;
  ctx.font = font;
  return ctx.measureText(text || '').width;
}

/** 时间戳列名白名单（DbManager 单元格/详情中的时间显示） */
export const TS_COLS = ['sort_seq', 'create_time', 'last_timestamp', 'timestamp', 'msg_time', 'time', 'ts'];

/**
 * 时间戳单元格 → 'YYYY-MM-DD HH:mm:ss'；非时间列/空/非法/越界返回 null。
 * 语义（自 DbManager 下沉，逐字保持）：值须为时间列名；秒或毫秒时间戳
 * （>1e12 视为毫秒）；有效窗口 1e8..4e9 秒。
 */
export function fmtTsValue(v: unknown, col: string): string | null {
  if (v === null || v === undefined || v === '') return null;
  if (!TS_COLS.includes(col)) return null;
  const n = Number(String(v).trim());
  if (!Number.isFinite(n) || n <= 0) return null;
  let sec = n;
  if (n > 1e12) sec = n / 1000; // 毫秒时间戳
  if (sec < 1e8 || sec > 4e9) return null;
  const d = new Date(sec * 1000);
  if (Number.isNaN(d.getTime())) return null;
  const p = (x: number) => String(x).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** 表列表分组：收藏 → 全部（自 DbManager dbTableSections 下沉） */
export function groupDbTables(
  tables: string[],
  pinned: string[],
  search: string,
): Array<{ label: string; tables: string[] }> {
  const q = search.trim().toLowerCase();
  const list = tables.filter((t) => !q || t.toLowerCase().includes(q));
  const pinnedHit = pinned.filter((t) => list.includes(t));
  const other = list.filter((t) => !pinned.includes(t));
  const sections: Array<{ label: string; tables: string[] }> = [];
  if (pinnedHit.length) sections.push({ label: '★ 收藏', tables: pinnedHit });
  sections.push({ label: q ? `匹配「${q}」` : '全部表', tables: other });
  return sections;
}

/** 外部数据库按「扫描根目录」分组（未命中扫描根的按所在目录分组；自 DbManager 下沉） */
export function groupDbFilesByRoot(
  files: DbFileEntry[],
  roots: string[],
): Array<{ dir: string; dirName: string; files: DbFileEntry[] }> {
  const map = new Map<string, { dir: string; dirName: string; files: DbFileEntry[] }>();
  for (const f of files) {
    const fp = f.path.replace(/\\/g, '/').toLowerCase();
    let dir = '';
    for (const r of roots) {
      const nr = r.replace(/\\/g, '/').toLowerCase().replace(/\/+$/, '');
      if (fp === nr || fp.startsWith(nr + '/')) {
        if (nr.length > dir.length) dir = r;
      }
    }
    if (!dir) {
      const idx = Math.max(f.path.lastIndexOf('\\'), f.path.lastIndexOf('/'));
      dir = idx > 0 ? f.path.slice(0, idx) : '';
    }
    const dirName = dir.split(/[\\/]/).pop() || dir;
    if (!map.has(dir)) map.set(dir, { dir, dirName, files: [] });
    map.get(dir)!.files.push(f);
  }
  return Array.from(map.values());
}
