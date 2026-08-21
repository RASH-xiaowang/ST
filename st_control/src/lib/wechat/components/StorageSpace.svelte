<script lang="ts">
  // ============================================================
  // 微信存储空间分析
  // 对标微信官方「存储空间」：总览 / 分类分布 / 会话排行 /
  // 发送者排行 / 大文件清单。数据来自 message_resource.db
  // （仅统计，不做任何删除操作）。
  // ============================================================
  import { onMount } from 'svelte';
  import { getWechatStorageStats } from '../services/ipc';
  import type { WechatStorageStats } from '../types';
  import { errText } from '../../format';
  import { formatBytes } from '../../format';
  import WechatHoverButton from './WechatHoverButton.svelte';
  import ImageIcon from '@lucide/svelte/icons/image';
  import FileIcon from '@lucide/svelte/icons/file';
  import UsersIcon from '@lucide/svelte/icons/users';
  import MessagesSquareIcon from '@lucide/svelte/icons/messages-square';
  import HardDriveIcon from '@lucide/svelte/icons/hard-drive';

  let { onOpenChat }: { onOpenChat?: (username: string) => void } = $props();

  let stats = $state<WechatStorageStats | null>(null);
  let loading = $state(false);
  let error = $state('');

  const totalLabel = $derived(
    stats ? formatBytes(stats.total_size, { nullPlaceholder: '0 B', gbPrecision: 1 }) : '--',
  );
  const maxCatSize = $derived(
    Math.max(1, ...(stats?.categories ?? []).map((c) => c.size)),
  );
  const maxChatSize = $derived(
    Math.max(1, ...(stats?.chats ?? []).map((c) => c.size)),
  );

  function fmtTime(ts: number): string {
    if (!ts) return '--';
    const d = new Date(ts * 1000);
    return isNaN(d.getTime()) ? '--' : `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  }

  async function load() {
    loading = true;
    error = '';
    try {
      stats = await getWechatStorageStats();
    } catch (e) {
      error = errText(e) || '加载失败';
    } finally {
      loading = false;
    }
  }

  onMount(load);
</script>

<div class="ws-root">
  <div class="ws-hd">
    <div class="ws-hd-info">
      <span class="ws-hd-title">存储空间分析</span>
      <span class="ws-hd-sub">来自消息资源库的只读统计（{stats ? `${stats.total_count.toLocaleString()} 项媒体资源` : '加载中…'}）</span>
    </div>
    <WechatHoverButton text="刷新" onclick={load} disabled={loading} class="!px-3 !py-1 !text-xs" />
  </div>

  {#if error}
    <div class="ws-error">{error}</div>
  {:else if loading && !stats}
    <div class="ws-empty">正在统计媒体资源…</div>
  {:else if stats}
    <!-- 总览 -->
    <div class="ws-total">
      <div class="ws-total-icon"><HardDriveIcon size={17} /></div>
      <div class="ws-total-main">
        <span class="ws-total-label">媒体资源总占用</span>
        <span class="ws-total-value">{totalLabel}</span>
      </div>
      <span class="ws-total-hint">仅统计聊天收发产生的媒体与文件，不含本地数据库本身</span>
    </div>

    <div class="ws-grid">
      <!-- 分类分布 -->
      <section class="ws-panel">
        <div class="ws-panel-hd"><ImageIcon size={14} /><span>分类分布</span></div>
        <div class="ws-cat-list">
          {#each stats.categories as c (c.label)}
            <div class="ws-cat">
              <div class="ws-cat-row">
                <span class="ws-cat-label">{c.label}</span>
                <span class="ws-cat-meta">{c.count.toLocaleString()} 项 · {formatBytes(c.size, { nullPlaceholder: '0 B' })}（{((c.size / maxCatSize) * 100).toFixed(0)}%）</span>
              </div>
              <div class="ws-bar"><div class="ws-bar-fill" style:width={(c.size / maxCatSize) * 100 + '%'}></div></div>
            </div>
          {/each}
        </div>
      </section>

      <!-- 会话排行 -->
      <section class="ws-panel">
        <div class="ws-panel-hd"><MessagesSquareIcon size={14} /><span>会话占用排行</span></div>
        <div class="ws-rank-list">
          {#if stats.chats.length === 0}
            <div class="ws-empty">暂无数据</div>
          {:else}
            {#each stats.chats.slice(0, 12) as c, i (c.username)}
              <button type="button" class="ws-rank" onclick={() => onOpenChat?.(c.username)} title="{c.name || c.username} 点击打开会话">
                <span class="ws-rank-idx">{i + 1}</span>
                <div class="ws-rank-main">
                  <span class="ws-rank-name">{c.name || c.username}</span>
                  <div class="ws-bar"><div class="ws-bar-fill" style:width={(c.size / maxChatSize) * 100 + '%'}></div></div>
                </div>
                <span class="ws-rank-meta">{c.count.toLocaleString()} 项 · {formatBytes(c.size, { nullPlaceholder: '0 B' })}</span>
              </button>
            {/each}
          {/if}
        </div>
      </section>

      <!-- 发送者排行 -->
      <section class="ws-panel">
        <div class="ws-panel-hd"><UsersIcon size={14} /><span>发送者排行</span></div>
        <div class="ws-rank-list">
          {#if stats.senders.length === 0}
            <div class="ws-empty">暂无数据</div>
          {:else}
            {#each stats.senders.slice(0, 12) as s, i (s.username)}
              <div class="ws-rank ws-rank-static">
                <span class="ws-rank-idx">{i + 1}</span>
                <div class="ws-rank-main">
                  <span class="ws-rank-name" title={s.username}>{s.name || s.username}</span>
                  <div class="ws-bar"><div class="ws-bar-fill" style:width={(s.size / maxChatSize) * 100 + '%'}></div></div>
                </div>
                <span class="ws-rank-meta">{s.count.toLocaleString()} 项 · {formatBytes(s.size, { nullPlaceholder: '0 B' })}</span>
              </div>
            {/each}
          {/if}
        </div>
      </section>

      <!-- 大文件清单 -->
      <section class="ws-panel ws-panel-wide">
        <div class="ws-panel-hd"><FileIcon size={14} /><span>大文件清单（Top {stats.large_files.length}）</span></div>
        <div class="ws-file-list">
          {#if stats.large_files.length === 0}
            <div class="ws-empty">暂无数据</div>
          {:else}
            {#each stats.large_files as f, i (f.name + i)}
              <button type="button" class="ws-file" onclick={() => onOpenChat?.(f.username)} title="定位到所属会话">
                <span class="ws-file-idx">{i + 1}</span>
                <span class="ws-file-name">{f.name || '(未知文件名)'}</span>
                <span class="ws-file-chat">{f.username || '-'}</span>
                <span class="ws-file-meta">{fmtTime(f.create_time)}</span>
                <span class="ws-file-size">{formatBytes(f.size, { nullPlaceholder: '0 B' })}</span>
              </button>
            {/each}
          {/if}
        </div>
      </section>
    </div>
  {/if}
</div>

<style>
  .ws-root {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    overflow-y: auto;
    padding: 14px 16px;
  }
  .ws-hd {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .ws-hd-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ws-hd-title {
    font-size: 15.5px;
    font-weight: 700;
    color: var(--wc-text, var(--foreground));
  }
  .ws-hd-sub {
    font-size: 11.5px;
    color: var(--wc-muted, var(--muted-foreground));
  }
  .ws-error {
    font-size: 12.5px;
    color: #ff5f56;
    padding: 20px;
    text-align: center;
  }
  .ws-empty {
    font-size: 12.5px;
    color: var(--wc-muted, var(--muted-foreground));
    padding: 24px;
    text-align: center;
  }

  /* 总览卡 */
  .ws-total {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 18px;
    border-radius: 12px;
    border: 1px solid var(--wc-border, var(--border));
    background: color-mix(in srgb, var(--app-accent, #22d3ee) 6%, var(--wc-bg, var(--card)));
  }
  .ws-total-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    border-radius: 10px;
    color: var(--app-accent, #22d3ee);
    background: color-mix(in srgb, var(--app-accent, #22d3ee) 12%, transparent);
    flex: none;
  }
  .ws-total-main {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ws-total-label {
    font-size: 12px;
    color: var(--wc-muted, var(--muted-foreground));
  }
  .ws-total-value {
    font-size: 24px;
    font-weight: 800;
    color: var(--wc-text, var(--foreground));
    font-variant-numeric: tabular-nums;
  }
  .ws-total-hint {
    margin-left: auto;
    font-size: 11.5px;
    color: var(--wc-muted, var(--muted-foreground));
    max-width: 320px;
    text-align: right;
  }

  /* 面板网格 */
  .ws-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    align-items: start;
  }
  .ws-panel {
    border: 1px solid var(--wc-border, var(--border));
    border-radius: 12px;
    background: var(--wc-bg2, var(--card));
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .ws-panel-wide {
    grid-column: 1 / -1;
  }
  .ws-panel-hd {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 13px;
    font-weight: 700;
    color: var(--wc-text, var(--foreground));
  }
  .ws-panel-hd :global(svg) {
    color: var(--app-accent, #22d3ee);
    flex: none;
  }

  /* 分类 */
  .ws-cat-list {
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  .ws-cat-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  .ws-cat-label {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--wc-text, var(--foreground));
  }
  .ws-cat-meta {
    font-size: 11px;
    color: var(--wc-muted, var(--muted-foreground));
    font-variant-numeric: tabular-nums;
  }
  .ws-bar {
    height: 6px;
    border-radius: 999px;
    background: var(--wc-nav-hover, var(--muted));
    overflow: hidden;
    margin-top: 4px;
  }
  .ws-bar-fill {
    height: 100%;
    border-radius: 999px;
    background: var(--app-accent, #22d3ee);
    min-width: 2px;
  }

  /* 排行 */
  .ws-rank-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ws-rank {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: 8px;
    text-align: left;
    width: 100%;
    cursor: pointer;
    background: transparent;
    border: none;
  }
  .ws-rank:hover {
    background: var(--wc-nav-hover, var(--muted));
  }
  .ws-rank-static {
    cursor: default;
  }
  .ws-rank-idx {
    flex: none;
    width: 18px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 5px;
    font-size: 10.5px;
    font-weight: 700;
    color: var(--wc-muted, var(--muted-foreground));
    background: var(--wc-nav-hover, var(--muted));
  }
  .ws-rank-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ws-rank-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--wc-text, var(--foreground));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ws-rank-meta {
    flex: none;
    font-size: 10.5px;
    color: var(--wc-muted, var(--muted-foreground));
    font-variant-numeric: tabular-nums;
  }

  /* 大文件 */
  .ws-file-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 300px;
    overflow-y: auto;
  }
  .ws-file {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: 8px;
    text-align: left;
    width: 100%;
    cursor: pointer;
    background: transparent;
    border: none;
    font-size: 11.5px;
  }
  .ws-file:hover {
    background: var(--wc-nav-hover, var(--muted));
  }
  .ws-file-idx {
    flex: none;
    width: 18px;
    color: var(--wc-muted, var(--muted-foreground));
    font-variant-numeric: tabular-nums;
  }
  .ws-file-name {
    flex: 1;
    min-width: 0;
    font-weight: 600;
    color: var(--wc-text, var(--foreground));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ws-file-chat {
    flex: none;
    max-width: 200px;
    color: var(--wc-muted, var(--muted-foreground));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ws-file-meta {
    flex: none;
    color: var(--wc-muted, var(--muted-foreground));
    font-variant-numeric: tabular-nums;
  }
  .ws-file-size {
    flex: none;
    width: 76px;
    text-align: right;
    font-weight: 700;
    color: var(--wc-text, var(--foreground));
    font-variant-numeric: tabular-nums;
  }
</style>
