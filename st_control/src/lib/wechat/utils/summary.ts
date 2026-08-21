/* ============================================================
 * 微信数据管理模块 — 汇总展示纯函数
 * 自 DailySummary.svelte 下沉：日期时间/时长/数量格式化。
 * ============================================================ */
import type { DailySummaryRecord } from '../types';
import { formatTs } from '../../format';

/** 时间戳（秒/毫秒自适应）→ YYYY-MM-DD HH:mm；空/非法 → '—'。
 * 收敛到共享 formatTs（原实现只按毫秒解析，秒级输入会错显 1970；现自适应）。 */
export function fmtTime(ts?: number): string {
  return formatTs(ts ?? 0, { invalidPlaceholder: '—' });
}

/** 每日总结记录统计：总数 / 成功 / 失败 / 成功平均字符数 */
export function summarizeRecords(
  records: DailySummaryRecord[],
): { total: number; ok: number; fail: number; avgChars: number } {
  const ok = records.filter((r) => r.status === 'done');
  return {
    total: records.length,
    ok: ok.length,
    fail: records.length - ok.length,
    avgChars: ok.length
      ? Math.round(
          ok.reduce((a, r) => a + (Number(r.char_count) || 0), 0) / ok.length,
        )
      : 0,
  };
}

/** Date → YYYY-MM-DD */
export function fmtDate(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/** 时长（毫秒）：≤0 空，<1s 显示 ms，否则 s 1 位小数 */
export function fmtDuration(ms?: number): string {
  const n = Number(ms || 0);
  if (n <= 0) return '';
  if (n < 1000) return `${n} ms`;
  return `${(n / 1000).toFixed(1)} s`;
}

/** 数量：≤0 空，≥1 万 → "x.x万"（去尾 0），否则原样 */
export function fmtTokens(n?: number): string {
  const v = Number(n || 0);
  if (v <= 0) return '';
  if (v >= 10000) return `${(v / 10000).toFixed(1).replace(/\.0$/, '')}万`;
  return String(v);
}
