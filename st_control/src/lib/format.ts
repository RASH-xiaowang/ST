/* ============================================================
 * 通用格式化工具
 * 统一各组件局部重复实现的 fmtBytes，行为经参数化保持逐组件一致：
 * - nullPlaceholder：null/undefined 时的输出（默认 '0 B'）
 * - gbPrecision：GB 及以上单位的小数位（KbDashboard 用 2，其余用 1）
 * - 单位序列可配置（DataDashboard 含 PB）
 * ============================================================ */

export interface FormatBytesOptions {
  /** null/undefined 时的输出；传 '-' 表示空值占位 */
  nullPlaceholder?: string;
  /** GB 及以上单位的小数位数（默认 1；KbDashboard 原实现为 2） */
  gbPrecision?: number;
  /** 单位序列（默认 B..TB；DataDashboard 原实现含 PB） */
  units?: string[];
}

/** 从任意错误值提取可展示文本（IPC 拒绝值可能是 string / Error / 其它） */
export function errText(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === 'string') return e;
  if (typeof e === 'object' && e !== null) {
    const m = (e as { message?: unknown }).message;
    if (typeof m === 'string') return m;
    try { return JSON.stringify(e); } catch { /* ignore */ }
  }
  return String(e ?? '');
}

const DEFAULT_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

export interface FormatDateTimeOptions {
  /** 是否显示年份（KbDocs 显示，KbChat/AutomationPanel 不显示） */
  showYear?: boolean;
  /** 使用 toLocaleString('zh-CN') 风格（AutomationPanel） */
  useLocale?: boolean;
  /** 非法时间戳的占位输出（默认原样返回输入） */
  invalidPlaceholder?: string;
  /** 仅输出日期（YYYY-MM-DD / MM-DD），不含时间（日期刻度场景） */
  dateOnly?: boolean;
  /** 输入为无时区的 UTC 字符串（如 SQLite datetime('now')），按 UTC 解析并转为本地展示 */
  utc?: boolean;
}

/** 字节格式化（0 → '0 B'；null/undefined 按 nullPlaceholder 处理） */
export function formatBytes(n: number | null | undefined, options: FormatBytesOptions = {}): string {
  const { nullPlaceholder = '0 B', gbPrecision = 1, units = DEFAULT_UNITS } = options;
  if (n == null) return nullPlaceholder;
  if (!n) return '0 B';
  let i = Math.floor(Math.log(n) / Math.log(1024));
  // gbPrecision=2 时以 GB（索引 3）封顶（保持 KbDashboard 原语义：TB+ 显示为 GB）；
  // 其余情况允许使用 units 中更大的单位（如 DataDashboard 的 PB）
  const maxIdx = gbPrecision === 2 ? Math.min(3, units.length - 1) : units.length - 1;
  i = Math.min(i, maxIdx);
  const precision = i === 0 ? 0 : i === 1 ? 1 : gbPrecision;
  return (n / Math.pow(1024, i)).toFixed(precision) + ' ' + units[i];
}

/** 时间戳（秒/毫秒/微秒自适应）→ Date；非法返回 null */
export function tsToDate(ts: number): Date | null {
  if (!ts) return null;
  let ms: number;
  if (ts > 1e15) ms = ts / 1000;     // 微秒 → 毫秒
  else if (ts > 1e12) ms = ts;       // 毫秒
  else ms = ts * 1000;               // 秒
  const d = new Date(ms);
  return isNaN(d.getTime()) ? null : d;
}

/** 时间戳 → 展示字符串（格式由选项决定；非法返回占位/原文） */
export function formatTs(ts: number, options: FormatDateTimeOptions = {}): string {
  const { invalidPlaceholder } = options;
  if (!ts) return invalidPlaceholder ?? '';
  const d = tsToDate(ts);
  if (!d) return invalidPlaceholder ?? String(ts);
  return formatDate(d, options);
}

/** Date → 展示字符串（YYYY-MM-DD HH:mm / MM-DD HH:mm / zh-CN locale） */
export function formatDate(d: Date, options: FormatDateTimeOptions = {}): string {
  const { showYear = true, useLocale = false, dateOnly = false } = options;
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  const datePart = showYear ? `${d.getFullYear()}-${mm}-${dd}` : `${mm}-${dd}`;
  if (dateOnly) return datePart;
  if (useLocale) {
    return d.toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
  }
  const hh = String(d.getHours()).padStart(2, '0');
  const mi = String(d.getMinutes()).padStart(2, '0');
  return `${datePart} ${hh}:${mi}`;
}

/** ISO 字符串 → 展示字符串（' ' 分隔的日期时间兼容解析；非法返回原文） */
export function formatIsoTime(iso: string, options: FormatDateTimeOptions = {}): string {
  if (!iso) return '';
  const { utc = false } = options;
  const normalized = iso.includes('T') ? iso : iso.replace(' ', 'T');
  // 数据库时间大多为 SQLite datetime('now')（UTC，无时区后缀）；utc=true 时补 Z，
  // 由 Date 转为本地时区展示，避免显示成“比本地慢 8 小时”的原始 UTC。
  const parsed = utc && !/(Z|[+-]\d{2}:?\d{2})$/i.test(normalized) ? normalized + 'Z' : normalized;
  const d = new Date(parsed);
  return isNaN(d.getTime()) ? iso : formatDate(d, options);
}

/** ISO 字符串 → YYYY-MM-DD（仅日期；' ' 分隔兼容；非法返回原文） */
export function formatDateOnly(iso: string, utc = false): string {
  if (!iso) return '';
  const normalized = iso.replace(' ', 'T');
  const parsed = utc && !/(Z|[+-]\d{2}:?\d{2})$/i.test(normalized) ? normalized + 'Z' : normalized;
  const d = new Date(parsed);
  if (isNaN(d.getTime())) return iso;
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}
