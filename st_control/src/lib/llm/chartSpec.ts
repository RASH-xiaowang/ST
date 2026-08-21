/* ============================================================
 * LLM — 图表规范归一化（纯函数）
 * 自 ChartView.svelte 下沉：兼容三种数据描述，输出统一结构。
 * ============================================================ */
import type { ChartSpec } from './types';

export type Series = { name?: string; data: number[] };
export type PieItem = { label: string; value: number };

export interface NormalizedChart {
  kind: 'bar' | 'line' | 'pie';
  title?: string;
  labels: string[];
  series: Series[];
  pie: PieItem[];
}

/**
 * 归一化图表 spec（纯 SVG 渲染前）：
 * 1) { type, labels, series } 常规轴类
 * 2) { type:'pie', data } 饼图
 * 3) 退化：无 series 时把饼图数据当单系列柱状
 */
export function normalizeChart(s: ChartSpec): NormalizedChart {
  if (!s || typeof s !== 'object') {
    return { kind: 'pie', labels: [], series: [], pie: [] };
  }
  const title = typeof s.title === 'string' ? s.title : undefined;
  const kind = (s.type === 'line' ? 'line' : s.type === 'bar' ? 'bar' : 'pie') as
    | 'bar'
    | 'line'
    | 'pie';

  let pie: PieItem[] = [];
  if (Array.isArray(s.data)) {
    pie = s.data
      .map((d) => ({
        label: String(d?.label ?? d?.name ?? d?.x ?? ''),
        value: Number(d?.value ?? d?.y ?? d ?? 0),
      }))
      .filter((d: PieItem) => isFinite(d.value));
  }

  let labels: string[] = Array.isArray(s.labels) ? s.labels.map(String) : [];
  let series: Series[] = [];
  if (Array.isArray(s.series)) {
    series = s.series.map((ser) => ({
      name: typeof ser?.name === 'string' ? ser.name : undefined,
      data: Array.isArray(ser?.data) ? ser.data.map((v) => Number(v) || 0) : [],
    }));
  }

  if (kind !== 'pie' && series.length === 0 && pie.length > 0) {
    labels = pie.map((p) => p.label ?? '');
    series = [{ name: title, data: pie.map((p) => p.value) }];
  }
  return { kind, title, labels, series, pie };
}
