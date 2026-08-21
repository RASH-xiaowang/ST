<script lang="ts">
  // ============================================================
  // 撤回消息记录
  // 数据源：微信 4.x 防撤回机制的本地删除缓存
  // （_weflow_anti_revoke_deleted_cache）——被撤回消息的元数据
  // 与内容副本。只读展示，用于回顾"谁撤回了什么"。
  // ============================================================
  import { onMount } from 'svelte';
  import { getWechatRevokedMessages } from '../services/ipc';
  import type { RevokedMessage } from '../types';
  import { errText } from '../../format';
  import { avatarLetter, colorFromName } from '../utils/format';
  import WechatHoverButton from './WechatHoverButton.svelte';
  import Undo2Icon from '@lucide/svelte/icons/undo-2';
  import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';

  let items = $state<RevokedMessage[]>([]);
  let loading = $state(false);
  let error = $state('');
  let expanded = $state<Set<number>>(new Set());

  /** 撤回类型构成（文本/图片/语音… 条数，降序） */
  const typeCounts = $derived.by(() => {
    const m = new Map<string, number>();
    for (const it of items) {
      const k = it.type_label || '未知';
      m.set(k, (m.get(k) ?? 0) + 1);
    }
    return [...m.entries()].sort((a, b) => b[1] - a[1]);
  });
  /** 撤回最多的发送者 Top 5 */
  const senderCounts = $derived.by(() => {
    const m = new Map<string, number>();
    for (const it of items) {
      const k = it.sender || '未知';
      m.set(k, (m.get(k) ?? 0) + 1);
    }
    return [...m.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5);
  });

  function fmtTime(ts: number): string {
    if (!ts) return '--';
    const d = new Date(ts * 1000);
    if (isNaN(d.getTime())) return '--';
    const p = (x: number) => String(x).padStart(2, '0');
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  }

  function toggle(idx: number) {
    const next = new Set(expanded);
    if (next.has(idx)) next.delete(idx);
    else next.add(idx);
    expanded = next;
  }

  async function load() {
    loading = true;
    error = '';
    try {
      items = await getWechatRevokedMessages(200);
    } catch (e) {
      error = errText(e) || '加载失败';
    } finally {
      loading = false;
    }
  }

  onMount(load);
</script>

<div class="rv-root">
  <div class="rv-hd">
    <div class="rv-hd-info">
      <span class="rv-hd-title">撤回消息记录</span>
      <span class="rv-hd-sub">微信 4.x 防撤回机制在本机保留的删除缓存 · 只读展示</span>
    </div>
    <WechatHoverButton text={loading ? '读取中…' : '刷新'} onclick={load} disabled={loading} class="!px-3 !py-1 !text-xs" />
  </div>

  <div class="rv-banner">
    <ShieldCheckIcon size={14} />
    <span>数据仅来自本机微信数据库的解密副本，不联网、不上传。缓存内容与撤回时间由微信客户端写入。</span>
  </div>

  {#if error}
    <div class="rv-error">{error}</div>
  {:else if loading && items.length === 0}
    <div class="rv-empty"><span class="wc-loading-inline"></span> 正在读取撤回缓存…</div>
  {:else if items.length === 0}
    <div class="rv-empty">
      <Undo2Icon size={18} />
      <p>暂无撤回消息记录</p>
      <p class="rv-empty-note">微信客户端未保留防撤回缓存，或解密库尚未同步最新数据</p>
    </div>
  {:else}
    <div class="rv-count">共 {items.length} 条被撤回消息</div>
    {#if typeCounts.length > 0}
      <div class="rv-stats">
        <div class="rv-stats-group">
          <span class="rv-stats-hd">类型构成</span>
          {#each typeCounts as [label, n]}<span class="rv-stat">{label} {n}</span>{/each}
        </div>
        {#if senderCounts.length > 0}
          <div class="rv-stats-group">
            <span class="rv-stats-hd">撤回最多</span>
            {#each senderCounts as [name, n]}<span class="rv-stat rv-stat-sender" title="{name} 撤回 {n} 条">{name} {n}</span>{/each}
          </div>
        {/if}
      </div>
    {/if}
    <div class="rv-list">
      {#each items as m, i (m.create_time + '-' + i)}
        <div class="rv-item" class:rv-item-open={expanded.has(i)}>
          <button type="button" class="rv-item-hd" onclick={() => toggle(i)} title={expanded.has(i) ? '收起' : '展开内容'}>
            <span class="rv-avatar" style:background={colorFromName(m.sender)}>{avatarLetter(m.sender)}</span>
            <span class="rv-sender">{m.sender}</span>
            <span class="rv-badge">{m.type_label}</span>
            <span class="rv-time">{fmtTime(m.create_time)}</span>
            <span class="rv-chevron">{expanded.has(i) ? '▾' : '▸'}</span>
          </button>
          {#if expanded.has(i)}
            <div class="rv-item-bd">
              <div class="rv-content">{m.content}</div>
              <div class="rv-note">↑ 该内容在微信客户端被撤回，此处为本地缓存副本</div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .rv-root {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    overflow-y: auto;
    padding: 16px 18px;
  }
  .rv-hd {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .rv-hd-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .rv-hd-title {
    font-size: 15.5px;
    font-weight: 700;
    color: var(--wc-text, var(--foreground));
  }
  .rv-hd-sub {
    font-size: 11.5px;
    color: var(--wc-muted, var(--muted-foreground));
  }
  .rv-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11.5px;
    color: var(--wc-muted, var(--muted-foreground));
    padding: 9px 12px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--app-accent, #22d3ee) 7%, var(--wc-bg2, var(--card)));
    border: 1px solid var(--wc-border, var(--border));
  }
  .rv-banner :global(svg) {
    color: var(--app-accent, #22d3ee);
    flex: none;
  }
  .rv-error {
    font-size: 12.5px;
    color: #ff5f56;
    padding: 24px;
    text-align: center;
  }
  .rv-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 40px 20px;
    color: var(--wc-muted, var(--muted-foreground));
    font-size: 13px;
  }
  .rv-empty :global(svg) {
    opacity: 0.5;
  }
  .rv-empty-note {
    font-size: 11.5px;
    margin: 0;
  }
  .rv-count {
    font-size: 12px;
    color: var(--wc-muted, var(--muted-foreground));
  }
  .rv-stats {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    font-size: 11.5px;
    padding: 8px 12px;
    border-radius: 10px;
    background: var(--wc-bg2, var(--card));
    border: 1px solid var(--wc-border, var(--border));
  }
  /* 两组统计：类型构成 ｜ 撤回最多，竖线分隔语义清晰 */
  .rv-stats-group {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .rv-stats-group + .rv-stats-group {
    padding-left: 10px;
    border-left: 1px solid var(--wc-border, var(--border));
  }
  .rv-stats-hd {
    font-weight: 600;
    color: var(--wc-muted, var(--muted-foreground));
    margin-right: 8px;
  }
  .rv-stat {
    padding: 1px 8px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--app-accent, #22d3ee) 10%, transparent);
    color: var(--wc-text, var(--foreground));
    font-variant-numeric: tabular-nums;
  }
  .rv-stat-sender {
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rv-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .rv-item {
    border: 1px solid var(--wc-border, var(--border));
    border-radius: 10px;
    background: var(--wc-bg2, var(--card));
    overflow: hidden;
  }
  .rv-item-hd {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 10px 12px;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
  }
  .rv-item-hd:hover {
    background: var(--wc-nav-hover, var(--muted));
  }
  .rv-avatar {
    flex: none;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    font-weight: 700;
    color: #fff;
  }
  .rv-sender {
    font-size: 13px;
    font-weight: 600;
    color: var(--wc-text, var(--foreground));
  }
  .rv-badge {
    flex: none;
    font-size: 10.5px;
    font-weight: 600;
    color: var(--app-accent, #22d3ee);
    border: 1px solid color-mix(in srgb, var(--app-accent, #22d3ee) 45%, transparent);
    border-radius: 999px;
    padding: 1px 8px;
  }
  .rv-time {
    margin-left: auto;
    flex: none;
    font-size: 11px;
    color: var(--wc-muted, var(--muted-foreground));
    font-variant-numeric: tabular-nums;
  }
  .rv-chevron {
    flex: none;
    font-size: 11px;
    color: var(--wc-muted, var(--muted-foreground));
  }
  .rv-item-bd {
    padding: 2px 12px 12px 50px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .rv-content {
    font-size: 13px;
    line-height: 1.7;
    color: var(--wc-text, var(--foreground));
    white-space: pre-wrap;
    word-break: break-word;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--wc-nav-hover, var(--muted));
  }
  .rv-note {
    font-size: 10.5px;
    color: var(--wc-muted, var(--muted-foreground));
  }
</style>
