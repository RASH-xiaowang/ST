<script lang="ts">
  // ============================================================
  // 微信数据总览（仪表板）
  // 一屏看全微信数据资产：会话/群聊/好友/公众号/朋友圈/收藏/
  // 表情/撤回痕迹 + 存储空间概况。每个卡片可跳转对应功能视图。
  // ============================================================
  import { onMount } from 'svelte';
  import { getWechatDataOverview } from '../services/ipc';
  import type { WechatDataOverview } from '../types';
  import { errText, formatBytes } from '../../format';
  import WechatHoverButton from './WechatHoverButton.svelte';
  import MessagesSquareIcon from '@lucide/svelte/icons/messages-square';
  import UsersIcon from '@lucide/svelte/icons/users';
  import UserRoundIcon from '@lucide/svelte/icons/user-round';
  import MegaphoneIcon from '@lucide/svelte/icons/megaphone';
  import ImageIcon from '@lucide/svelte/icons/image';
  import StarIcon from '@lucide/svelte/icons/star';
  import LaughIcon from '@lucide/svelte/icons/laugh';
  import Undo2Icon from '@lucide/svelte/icons/undo-2';
  import HardDriveIcon from '@lucide/svelte/icons/hard-drive';
  import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';

  let {
    onNavigate,
    onOpenAuthor,
  }: {
    onNavigate: (tab: string) => void;
    /** 点击作者 chip 时跳转到该好友的朋友圈（未传入时 chip 不可点击） */
    onOpenAuthor?: (author: { username: string; name: string }) => void;
  } = $props();

  let data = $state<WechatDataOverview | null>(null);
  let loading = $state(false);
  let error = $state('');

  async function load() {
    loading = true;
    error = '';
    try {
      data = await getWechatDataOverview();
    } catch (e) {
      error = errText(e) || '加载失败';
    } finally {
      loading = false;
    }
  }

  onMount(load);

  const topCats = $derived(
    (data?.storage.categories ?? []).slice(0, 4),
  );
</script>

<div class="ov-root">
  <div class="ov-hd">
    <div class="ov-hd-info">
      <span class="ov-hd-title">微信数据总览</span>
      <span class="ov-hd-sub">本机微信数据资产一屏纵览 · 只读统计</span>
    </div>
    <WechatHoverButton text={loading ? '统计中…' : '刷新'} onclick={load} disabled={loading} class="!px-3 !py-1 !text-xs" />
  </div>

  {#if error}
    <div class="ov-error">{error}</div>
  {:else if !data}
    <div class="ov-loading"><RefreshCwIcon size={16} /> 正在统计微信数据…</div>
  {:else}
    <!-- 核心数字 -->
    <div class="ov-grid">
      <button type="button" class="ov-card" onclick={() => onNavigate('chats')} title="查看聊天">
        <span class="ov-card-icon"><MessagesSquareIcon size={16} /></span>
        <span class="ov-card-value">{data.sessions.toLocaleString()}</span>
        <span class="ov-card-label">会话</span>
        <span class="ov-card-arrow"><ArrowRightIcon size={12} /></span>
      </button>
      <button type="button" class="ov-card" onclick={() => onNavigate('contacts')} title="查看通讯录">
        <span class="ov-card-icon"><UsersIcon size={16} /></span>
        <span class="ov-card-value">{data.groups.toLocaleString()}</span>
        <span class="ov-card-label">群聊</span>
        <span class="ov-card-arrow"><ArrowRightIcon size={12} /></span>
      </button>
      <button type="button" class="ov-card" onclick={() => onNavigate('contacts')} title="查看通讯录">
        <span class="ov-card-icon"><UserRoundIcon size={16} /></span>
        <span class="ov-card-value">{data.contacts.toLocaleString()}</span>
        <span class="ov-card-label">好友</span>
        <span class="ov-card-arrow"><ArrowRightIcon size={12} /></span>
      </button>
      <button type="button" class="ov-card" onclick={() => onNavigate('bizchats')} title="查看公众号">
        <span class="ov-card-icon"><MegaphoneIcon size={16} /></span>
        <span class="ov-card-value">{data.official.toLocaleString()}</span>
        <span class="ov-card-label">公众号</span>
        <span class="ov-card-arrow"><ArrowRightIcon size={12} /></span>
      </button>
      <button type="button" class="ov-card" onclick={() => onNavigate('moments')} title="查看朋友圈">
        <span class="ov-card-icon"><ImageIcon size={16} /></span>
        <span class="ov-card-value">{data.moments.toLocaleString()}</span>
        <span class="ov-card-label">朋友圈</span>
        <span class="ov-card-arrow"><ArrowRightIcon size={12} /></span>
      </button>
      <button type="button" class="ov-card" onclick={() => onNavigate('favorites')} title="查看收藏">
        <span class="ov-card-icon"><StarIcon size={16} /></span>
        <span class="ov-card-value">{data.favorites.toLocaleString()}</span>
        <span class="ov-card-label">收藏</span>
        <span class="ov-card-arrow"><ArrowRightIcon size={12} /></span>
      </button>
      <button type="button" class="ov-card" onclick={() => onNavigate('emoticons')} title="查看表情">
        <span class="ov-card-icon"><LaughIcon size={16} /></span>
        <span class="ov-card-value">{data.emoticons.toLocaleString()}</span>
        <span class="ov-card-label">自定义表情</span>
        <span class="ov-card-arrow"><ArrowRightIcon size={12} /></span>
      </button>
      <button type="button" class="ov-card" onclick={() => onNavigate('storage')} title="查看存储空间">
        <span class="ov-card-icon"><HardDriveIcon size={16} /></span>
        <span class="ov-card-value">{formatBytes(data.storage.total_size, { nullPlaceholder: '0 B', gbPrecision: 1 })}</span>
        <span class="ov-card-label">媒体占用 · {data.storage.total_count.toLocaleString()} 项</span>
        <span class="ov-card-arrow"><ArrowRightIcon size={12} /></span>
      </button>
    </div>

    <!-- 存储概况 + 撤回痕迹 -->
    <div class="ov-cols">
      <section class="ov-panel">
        <div class="ov-panel-hd">
          <HardDriveIcon size={14} />
          <span>存储构成 Top {topCats.length}</span>
          <button type="button" class="ov-panel-go" onclick={() => onNavigate('storage')}>详情 →</button>
        </div>
        {#if topCats.length === 0}
          <div class="ov-empty">暂无媒体资源记录</div>
        {:else}
          <div class="ov-cat-list">
            {#each topCats as c (c.label)}
              <div class="ov-cat">
                <div class="ov-cat-row">
                  <span class="ov-cat-label">{c.label}</span>
                  <span class="ov-cat-meta">{c.count.toLocaleString()} 项 · {formatBytes(c.size, { nullPlaceholder: '0 B' })}</span>
                </div>
                <div class="ov-bar"><div class="ov-bar-fill" style:width={(c.size / Math.max(1, topCats[0].size)) * 100 + '%'}></div></div>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <section class="ov-panel">
        <div class="ov-panel-hd">
          <Undo2Icon size={14} />
          <span>撤回消息痕迹</span>
          <button type="button" class="ov-panel-go" onclick={() => onNavigate('revoked')}>详情 →</button>
        </div>
        <div class="ov-revoke">
          <span class="ov-revoke-value">{data.revoked.toLocaleString()}</span>
          <span class="ov-revoke-label">条被撤回消息的元数据痕迹（发送者/时间/类型可查）</span>
          <p class="ov-revoke-note">微信 4.x 防撤回机制在本地保留的删除缓存，可用于回顾"谁撤回了什么"。</p>
        </div>
      </section>
    </div>

    {#if (data.moments_authors ?? []).length > 0}
      <section class="ov-panel">
        <div class="ov-panel-hd">
          <UsersIcon size={14} />
          <span>朋友圈活跃 Top 15</span>
          <button type="button" class="ov-panel-go" onclick={() => onNavigate('moments')}>详情 →</button>
        </div>
        {#if onOpenAuthor}
          <p class="ov-authors-hint">点击作者，跳转查看 TA 的朋友圈</p>
        {/if}
        <div class="ov-authors">
          {#each (data.moments_authors ?? []) as a, i (a.username)}
            <button
              type="button"
              class="ov-author"
              class:ov-author-link={!!onOpenAuthor}
              title="{a.name} 共发布 {a.posts} 条{onOpenAuthor ? '，点击查看 TA 的朋友圈' : ''}"
              onclick={() => onOpenAuthor?.({ username: a.username, name: a.name })}
            >
              <span class="ov-author-rank">{i + 1}</span>
              <span class="ov-author-name" title={a.name}>{a.name}</span>
              <span class="ov-author-posts">{a.posts} 条</span>
            </button>
          {/each}
        </div>
      </section>
    {/if}
  {/if}
</div>

<style>
  .ov-root {
    display: flex;
    flex-direction: column;
    gap: 14px;
    height: 100%;
    overflow-y: auto;
    padding: 16px 18px;
  }
  .ov-hd {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .ov-hd-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ov-hd-title {
    font-size: 15.5px;
    font-weight: 700;
    color: var(--wc-text, var(--foreground));
  }
  .ov-hd-sub {
    font-size: 12.5px;
    color: var(--wc-text2, var(--muted-foreground));
  }
  .ov-error {
    font-size: 12.5px;
    color: #ff5f56;
    padding: 24px;
    text-align: center;
  }
  .ov-loading {
    display: flex;
    align-items: center;
    gap: 8px;
    justify-content: center;
    padding: 48px;
    font-size: 13px;
    color: var(--wc-muted, var(--muted-foreground));
  }
  .ov-loading :global(svg) {
    animation: ov-spin 1.2s linear infinite;
  }
  @keyframes ov-spin {
    to { transform: rotate(360deg); }
  }

  .ov-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 12px;
  }
  .ov-card {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 14px 16px;
    border-radius: 12px;
    border: 1px solid var(--wc-border, var(--border));
    background: var(--wc-bg2, var(--card));
    text-align: left;
    cursor: pointer;
    transition: border-color 0.15s, transform 0.15s;
  }
  .ov-card:hover {
    border-color: color-mix(in srgb, var(--app-accent, #22d3ee) 55%, var(--wc-border, var(--border)));
    transform: translateY(-2px);
  }
  .ov-card:focus-visible {
    outline: 2px solid var(--app-accent, #22d3ee);
    outline-offset: 1px;
  }
  .ov-card-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    color: var(--app-accent, #22d3ee);
    background: color-mix(in srgb, var(--app-accent, #22d3ee) 12%, transparent);
  }
  .ov-card-value {
    font-size: 21px;
    font-weight: 800;
    color: var(--wc-text, var(--foreground));
    font-variant-numeric: tabular-nums;
    line-height: 1.1;
  }
  .ov-card-label {
    font-size: 11.5px;
    color: var(--wc-muted, var(--muted-foreground));
  }
  .ov-card-arrow {
    position: absolute;
    right: 12px;
    top: 14px;
    color: var(--wc-muted, var(--muted-foreground));
    opacity: 0;
    transition: opacity 0.15s;
  }
  .ov-card:hover .ov-card-arrow {
    opacity: 1;
  }

  .ov-cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    align-items: start;
  }
  .ov-panel {
    border: 1px solid var(--wc-border, var(--border));
    border-radius: 12px;
    background: var(--wc-bg2, var(--card));
    padding: 13px 15px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .ov-panel-hd {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 13px;
    font-weight: 700;
    color: var(--wc-text, var(--foreground));
  }
  .ov-panel-hd :global(svg) {
    color: var(--app-accent, #22d3ee);
    flex: none;
  }
  .ov-panel-go {
    margin-left: auto;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--app-accent, #22d3ee);
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
  }
  .ov-empty {
    font-size: 12px;
    color: var(--wc-muted, var(--muted-foreground));
    padding: 14px 4px;
  }
  .ov-cat-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .ov-cat-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  .ov-cat-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--wc-text, var(--foreground));
  }
  .ov-cat-meta {
    font-size: 10.5px;
    color: var(--wc-muted, var(--muted-foreground));
    font-variant-numeric: tabular-nums;
  }
  .ov-bar {
    height: 6px;
    border-radius: 999px;
    background: var(--wc-nav-hover, var(--muted));
    overflow: hidden;
    margin-top: 4px;
  }
  .ov-bar-fill {
    height: 100%;
    border-radius: 999px;
    background: var(--app-accent, #22d3ee);
    min-width: 2px;
  }

  .ov-revoke {
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-start;
  }
  .ov-revoke-value {
    font-size: 26px;
    font-weight: 800;
    color: var(--wc-text, var(--foreground));
    font-variant-numeric: tabular-nums;
  }
  .ov-revoke-label {
    font-size: 12px;
    line-height: 1.6;
    color: var(--wc-muted, var(--muted-foreground));
  }
  .ov-revoke-note {
    margin: 0;
    font-size: 11px;
    line-height: 1.7;
    color: var(--wc-muted, var(--muted-foreground));
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--wc-nav-hover, var(--muted));
  }

  /* 朋友圈活跃作者行 */
  .ov-authors {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .ov-authors-hint {
    margin: 0 0 8px;
    font-size: 11px;
    color: var(--wc-muted, var(--muted-foreground));
  }
  .ov-author {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-family: inherit;
    color: var(--wc-text, var(--foreground));
    padding: 4px 10px 4px 6px;
    border: 1px solid transparent;
    border-radius: 999px;
    background: var(--wc-nav-hover, var(--muted));
    cursor: default;
    transition: border-color 0.15s ease, background 0.15s ease, color 0.15s ease;
  }
  .ov-author-link {
    cursor: pointer;
  }
  .ov-author-link:hover {
    color: var(--app-accent, #22d3ee);
    border-color: color-mix(in srgb, var(--app-accent, #22d3ee) 45%, transparent);
    background: color-mix(in srgb, var(--app-accent, #22d3ee) 10%, transparent);
  }
  .ov-author-rank {
    width: 17px;
    height: 17px;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 700;
    color: var(--app-accent, #22d3ee);
    background: color-mix(in srgb, var(--app-accent, #22d3ee) 14%, transparent);
    flex: none;
  }
  .ov-author-name {
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ov-author-posts {
    font-size: 10.5px;
    color: var(--wc-muted, var(--muted-foreground));
    font-variant-numeric: tabular-nums;
  }
</style>
