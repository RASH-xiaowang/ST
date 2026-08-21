// 系统指标 — Tauri IPC 封装层
import { invoke } from '@tauri-apps/api/core';

export function getRealtimeMetrics<T = unknown>(): Promise<T> {
  return invoke<T>('get_realtime_metrics');
}
