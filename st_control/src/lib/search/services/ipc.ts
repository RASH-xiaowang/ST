// 全局搜索 — Tauri IPC 封装层
import { invoke } from '@tauri-apps/api/core';
import type { SearchEvent } from '../types';
import type { SearchIndexStatus } from '../../wechat/types';

export function getWechatSearchIndexStatus(): Promise<SearchIndexStatus> {
  return invoke('get_wechat_search_index_status');
}

export function queryEvents(limit: number, offset: number): Promise<SearchEvent[]> {
  return invoke('query_events', { limit, offset });
}
