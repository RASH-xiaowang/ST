<script lang="ts">
  import { errText } from '../../format';
  import { onMount, onDestroy } from 'svelte';
  import { formatBytes, formatTs } from '../../format';
  import { createMsg } from '../../services/msg.svelte';
  import { createWechatBackup, deleteWechatBackup, listWechatBackups, restoreWechatBackup } from '../services/ipc';
  import type { WechatBackupCreateResult, WechatBackupRestoreResult } from '../types';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import WechatHoverButton from './WechatHoverButton.svelte';

  interface BackupItem {
    name: string;
    path: string;
    size: number;
    modified: number;
  }

  let backupDir = $state('');
  let passphrase = $state('');
  let creating = $state(false);
  let restoring = $state(false);
  let progress = $state<{ show: boolean; label: string; percent: number } | null>(null);
  let backupResult = $state<WechatBackupCreateResult | null>(null);
  let restoreResult = $state<WechatBackupRestoreResult | null>(null);
  let items = $state<BackupItem[]>([]);
  const msg = createMsg(5000);
  let unlisten: UnlistenFn | null = null;

  function fmtSize(n: number): string {
    return formatBytes(n, { gbPrecision: 2 });
  }

  function fmtDate(ts: number): string {
    return formatTs(ts, { showYear: true });
  }

  async function pickDir() {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const sel = await open({ directory: true, multiple: false, title: '选择备份保存目录' });
    if (typeof sel === 'string' && sel) {
      backupDir = sel;
      loadList();
    }
  }

  async function createBackup() {
    if (!backupDir.trim()) {
      msg.show('请先选择备份保存目录', false);
      return;
    }
    if (passphrase.trim().length < 4) {
      msg.show('请设置至少 4 位的备份口令', false);
      return;
    }
    creating = true;
    progress = { show: true, label: '准备打包解密数据库…', percent: 0 };
    backupResult = null;
    try {
      const r = await createWechatBackup({
        passphrase,
        outputDir: backupDir,
      });
      backupResult = r;
      msg.show(`✅ 备份完成：${r.filename}（${fmtSize(r.size)}）`);
      loadList();
    } catch (e: unknown) {
      msg.show(`备份失败：${errText(e)}`, false);
    } finally {
      creating = false;
      progress = null;
    }
  }

  async function pickRestoreFile() {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const sel = await open({
      multiple: false,
      title: '选择加密备份文件 (.stbak)',
      filters: [{ name: '加密备份', extensions: ['stbak'] }],
    });
    if (typeof sel === 'string' && sel) {
      restorePath = sel;
    }
  }

  let restorePath = $state('');

  async function restoreBackup() {
    if (!restorePath) {
      msg.show('请先选择备份文件', false);
      return;
    }
    if (passphrase.trim().length < 4) {
      msg.show('请输入备份口令', false);
      return;
    }
    if (!confirm(`确定从该备份恢复吗？将把解密数据库复制到本地解密区（不会删除现有数据）。\n${restorePath}`)) return;
    restoring = true;
    restoreResult = null;
    try {
      const r = await restoreWechatBackup({ path: restorePath, passphrase });
      restoreResult = r;
      msg.show(`✅ 恢复完成：导入 ${r?.imported ?? 0} 个文件`);
    } catch (e: unknown) {
      msg.show(`恢复失败：${errText(e)}`, false);
    } finally {
      restoring = false;
    }
  }

  async function loadList() {
    if (!backupDir.trim()) return;
    try {
      const r = await listWechatBackups(backupDir);
      items = Array.isArray(r?.items) ? r.items : [];
    } catch (e) {
      console.warn('[backup] 列表加载失败:', e);
    }
  }

  async function deleteBackup(it: BackupItem) {
    if (!confirm(`确定删除备份文件？\n${it.name}`)) return;
    try {
      await deleteWechatBackup(it.path);
      msg.show('已删除备份');
      loadList();
    } catch (e: unknown) {
      msg.show(`删除失败：${errText(e)}`, false);
    }
  }

  onMount(async () => {
    try {
      unlisten = await listen<{ op?: string; message?: string; percent?: number }>('wechat-op-progress', (event) => {
        const p = event.payload;
        if (p?.op === 'archive') {
          progress = { show: true, label: p.message ?? '打包中…', percent: Number(p.percent ?? 0) };
        }
      });
    } catch (e) {
      console.warn('[backup] 进度监听失败:', e);
    }
  });

  onDestroy(() => {
    unlisten?.();
  });
</script>

<div class="wc-bak">
  <div class="wc-bak-hd">
    <div>
      <div class="wc-bak-title">备份管家</div>
      <div class="wc-bak-sub">加密备份全部解密数据库（AES-256 + 口令保护），可随时恢复</div>
    </div>
    {#if msg}
      <span class="wc-bak-msg" class:wc-bak-msg-err={!msg.state.ok}>{msg.state.text}</span>
    {/if}
  </div>

  <div class="wc-bak-main">
    <div class="wc-bak-card">
      <div class="wc-bak-card-title">创建加密备份</div>
      <div class="wc-bak-row">
        <input type="text" placeholder="备份保存目录" readonly bind:value={backupDir} />
          <WechatHoverButton text="选择目录" onclick={pickDir} class="!px-3 !py-1 !text-xs" />
      </div>
      <div class="wc-bak-row">
        <input type="password" placeholder="设置备份口令（至少 4 位，请务必牢记）" bind:value={passphrase} />
      </div>
      {#if progress?.show}
        <div class="wc-bak-progress">
          <div class="wc-bak-progress-bar"><div class="wc-bak-progress-fill" style="width:{progress.percent}%"></div></div>
          <span class="wc-bak-progress-label">{progress.label}（{progress.percent}%）</span>
        </div>
      {/if}
        <WechatHoverButton text={creating ? '备份中…' : '开始备份'} onclick={createBackup} disabled={creating || !backupDir} class="w-full" />
      {#if backupResult}
        <div class="wc-bak-result">
          <div>文件名：{backupResult.filename}</div>
          <div>大小：{fmtSize(backupResult.size)} · 文件数：{backupResult.file_count}</div>
          <div class="wc-bak-result-path">{backupResult.path}</div>
        </div>
      {/if}
    </div>

    <div class="wc-bak-card">
      <div class="wc-bak-card-title">恢复备份</div>
      <div class="wc-bak-row">
        <input type="text" placeholder="选择 .stbak 备份文件" readonly bind:value={restorePath} />
          <WechatHoverButton text="选择文件" onclick={pickRestoreFile} class="!px-3 !py-1 !text-xs" />
      </div>
      <div class="wc-bak-row">
        <input type="password" placeholder="输入备份口令" bind:value={passphrase} />
      </div>
        <WechatHoverButton text={restoring ? '恢复中…' : '恢复备份'} onclick={restoreBackup} disabled={restoring || !restorePath} />
      {#if restoreResult}
        <div class="wc-bak-result">
          <div>已导入 {restoreResult.imported ?? 0} 个文件</div>
          <div class="wc-bak-result-path">{restoreResult.target ?? ''}</div>
        </div>
      {/if}
      <div class="wc-bak-tip">恢复会把解密数据库写入本地解密区，不会删除或覆盖微信源库；之后可在「数据库管理」中核对数据。</div>
    </div>

    <div class="wc-bak-card wc-bak-list-card">
      <div class="wc-bak-card-title">已有备份{backupDir ? `（${backupDir}）` : ''}</div>
      {#if !backupDir}
        <div class="wc-bak-empty">选择备份目录后显示已有备份</div>
      {:else if items.length === 0}
        <div class="wc-bak-empty">该目录暂无 .stbak 备份</div>
      {:else}
        <div class="wc-bak-list">
          {#each items as it (it.path)}
            <div class="wc-bak-item">
              <div class="wc-bak-item-info">
                <span class="wc-bak-item-name">{it.name}</span>
                <span class="wc-bak-item-meta">{fmtSize(it.size)} · {fmtDate(it.modified)}</span>
              </div>
              <button class="wc-bak-item-del" onclick={() => deleteBackup(it)} title="删除备份">×</button>
            </div>
          {/each}
        </div>
      {/if}
      {#if backupDir}
          <WechatHoverButton text="刷新列表" onclick={loadList} class="!px-3 !py-1 !text-xs" />
      {/if}
    </div>
  </div>
</div>

<style>
  .wc-bak {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    padding: 16px 20px;
    gap: 12px;
    box-sizing: border-box;
    overflow-y: auto;
  }
  .wc-bak-hd {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-shrink: 0;
  }
  .wc-bak-title {
    font-size: 16px;
    font-weight: 700;
    color: var(--wc-text);
  }
  .wc-bak-sub {
    font-size: 11.5px;
    color: var(--wc-muted);
  }
  .wc-bak-msg {
    font-size: 12px;
    color: #27ae60;
  }
  .wc-bak-msg-err {
    color: #c0392b;
  }
  .wc-bak-main {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .wc-bak-card {
    border: 1px solid var(--wc-border-light);
    border-radius: 12px;
    background: var(--wc-card);
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 760px;
  }
  .wc-bak-card-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--wc-text);
  }
  .wc-bak-row {
    display: flex;
    gap: 8px;
  }
  .wc-bak-row input {
    flex: 1;
    min-width: 0;
    padding: 7px 10px;
    border: 1px solid var(--wc-border);
    border-radius: 6px;
    background: var(--wc-bg2);
    color: var(--wc-text);
    font-size: 12px;
    outline: none;
  }
  .wc-bak-progress {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .wc-bak-progress-bar {
    height: 8px;
    border-radius: 999px;
    background: var(--wc-bg2);
    overflow: hidden;
  }
  .wc-bak-progress-fill {
    height: 100%;
    background: var(--wc-theme, #576b95);
    /* impeccable-disable-next-line layout-transition -- 进度条宽度动画 */
    transition: width 0.2s ease;
  }
  .wc-bak-progress-label {
    font-size: 11.5px;
    color: var(--wc-muted);
  }
  .wc-bak-result {
    font-size: 12px;
    color: var(--wc-text2);
    border: 1px dashed var(--wc-border);
    border-radius: 8px;
    padding: 8px 10px;
    background: var(--wc-bg2);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .wc-bak-result-path {
    font-size: 11.5px;
    color: var(--wc-muted);
    word-break: break-all;
  }
  .wc-bak-tip {
    font-size: 11.5px;
    color: var(--wc-muted);
    line-height: 1.6;
  }
  .wc-bak-empty {
    color: var(--wc-muted);
    font-size: 12px;
    text-align: center;
    padding: 12px 0;
  }
  .wc-bak-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 260px;
    overflow-y: auto;
  }
  .wc-bak-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: 8px;
    background: var(--wc-bg2);
  }
  .wc-bak-item-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .wc-bak-item-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--wc-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .wc-bak-item-meta {
    font-size: 11.5px;
    color: var(--wc-muted);
  }
  .wc-bak-item-del {
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: 14px;
  }
</style>
