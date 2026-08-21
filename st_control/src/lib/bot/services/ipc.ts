// 消息通道 — Tauri IPC 封装
import { invoke } from '@tauri-apps/api/core';
import type { AccountContact, BotAccount, BotLog, BotStatusSummary, QqbotContact, QrView } from '../types';
import type { BotPlatform } from '../types';

export const botApi = {
  listAccounts: () => invoke<BotAccount[]>('bot_list_accounts'),
  startQr: (accountId?: number | null) => invoke<QrView>('bot_start_qr', { accountId: accountId ?? null }),
  pollQr: (sessionId: string) =>
    invoke<{ status: string; accountId?: number; expiresAt?: string }>('bot_poll_qr', { sessionId }),
  cancelQr: (sessionId: string) => invoke<void>('bot_cancel_qr', { sessionId }),
  renameAccount: (id: number, name: string) => invoke<void>('bot_rename_account', { id, name }),
  unbindAccount: (id: number) => invoke<void>('bot_unbind_account', { id }),
  statusSummary: () => invoke<BotStatusSummary>('bot_status_summary'),
  addChannel: (platform: BotPlatform, name: string, config: string, targetId: string) =>
    invoke<number>('bot_add_channel', { platform, name, config, targetId }),
  updateChannel: (id: number, name: string, config: string, targetId: string) =>
    invoke<void>('bot_update_channel', { id, name, config, targetId }),
  testChannel: (accountId: number) => invoke<void>('bot_test_channel', { accountId }),
  sendText: (accountId: number, to: string, text: string) =>
    invoke<string>('bot_send_text', { accountId, to, text }),
  sendMedia: (accountId: number, to: string, path: string) =>
    invoke<string>('bot_send_media', { accountId, to, path }),
  listContacts: (accountId: number) => invoke<AccountContact[]>('bot_list_contacts', { accountId }),
  // QQ 官方机器人：网关自动收集到的 openid 目标
  listQqbotContacts: (accountId: number) =>
    invoke<QqbotContact[]>('bot_list_qqbot_contacts', { accountId }),
  listLogs: (accountId: number, page = 1, pageSize = 50) =>
    invoke<{ items: BotLog[]; total: number }>('bot_list_logs', { accountId, page, pageSize }),
  clearLogs: (accountId: number) => invoke<void>('bot_clear_logs', { accountId }),
};
