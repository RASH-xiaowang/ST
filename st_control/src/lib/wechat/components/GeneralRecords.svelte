<script lang="ts">
  import { onMount } from 'svelte';
  import WechatHoverButton from './WechatHoverButton.svelte';
  import {
    exportWechatRecordsCsv,
    listWechatFinder,
    listWechatFriendVerifications,
    listWechatMiniPrograms,
    listWechatRedEnvelopes,
    listWechatRevokes,
    listWechatTransfers,
  } from '../services/ipc';
  import { hbStatus, kindIcon, liveStatus, shortUser, transferSubType } from '../utils/records';
  import { formatTs } from '../../format';
  import { downloadBlob } from '../../download';
  import type { RecordListItem } from '../types';

  interface Props {
    /** 点击记录跳转到对应会话（可定位消息） */
    onopen?: (username: string, localId?: number) => void;
  }
  let { onopen }: Props = $props();

  type Kind = 'revokes' | 'transfers' | 'redpackets' | 'finder' | 'miniprograms' | 'friendverifications';
  let kind = $state<Kind>('revokes');
  let items = $state<RecordListItem[]>([]);
  let total = $state(0);
  /** 各类型已加载过的总数缓存（tab 计数在切换后依然可见） */
  let totals = $state<Partial<Record<Kind, number>>>({});
  let loading = $state(false);
  let keyword = $state('');
  let error = $state('');
  let page = $state(0);
  const PAGE_SIZE = 50;

  const KIND_META: Record<Kind, { label: string; desc: string; icon: string }> = {
    revokes: { label: '撤回消息', desc: '本地撤回缓存记录，点击会话可跳转定位', icon: 'rewind' },
    transfers: { label: '转账记录', desc: '微信转账明细，点击会话可跳转', icon: 'card' },
    redpackets: { label: '红包记录', desc: '微信红包明细，点击会话可跳转', icon: 'gift' },
    finder: { label: '视频号', desc: '视频号直播 / 用户页记录', icon: 'film' },
    miniprograms: { label: '小程序', desc: '已使用的小程序联系人', icon: 'app' },
    friendverifications: { label: '好友验证', desc: '新朋友 / 好友验证消息', icon: 'users' },
  };

  async function load(reset = true) {
    if (reset) page = 0;
    loading = true;
    error = '';
    try {
      const cmdMap = {
        revokes: listWechatRevokes,
        transfers: listWechatTransfers,
        redpackets: listWechatRedEnvelopes,
        finder: listWechatFinder,
        miniprograms: listWechatMiniPrograms,
        friendverifications: listWechatFriendVerifications,
      };
      const r = await cmdMap[kind]({
        limit: PAGE_SIZE,
        offset: reset ? 0 : page * PAGE_SIZE,
        q: keyword || null,
      });
      const list = Array.isArray(r?.items) ? r.items : [];
      items = reset ? list : [...items, ...list];
      total = r?.total ?? items.length;
      totals = { ...totals, [kind]: total };
    } catch (e: unknown) {
      error = `${e}`;
    } finally {
      loading = false;
    }
  }

  async function exportCsv() {
    try {
      const r = await exportWechatRecordsCsv({ kind });
      const csv = r?.csv ?? '';
      if (!csv) {
        error = '无数据可导出';
        return;
      }
      const blob = new Blob(["\uFEFF" + csv], { type: 'text/csv;charset=utf-8' });
      downloadBlob(blob, `wechat_${kind}_${new Date().toISOString().slice(0, 10)}.csv`);
    } catch (e: unknown) {
      error = `导出失败: ${e}`;
    }
  }

  function switchKind(k: Kind) {
    kind = k;
    keyword = '';
    lastKeyword = '';
    load(true);
  }

  function fmtTime(ts: number | string | null | undefined): string {
    return formatTs(Number(ts) || 0, { showYear: true, invalidPlaceholder: String(ts) });
  }

  // 搜索防抖：输入停顿 400ms 自动搜索（无需点按钮/回车）
  let lastKeyword = '';
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const v = keyword.trim();
    if (v === lastKeyword) return;
    lastKeyword = v;
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => load(true), 400);
    return () => {
      if (searchTimer) clearTimeout(searchTimer);
    };
  });

  onMount(() => load(true));
</script>

<div class="gr-panel">
  <header class="gr-hd">
    <div class="gr-title">
      <span class="gr-title-icon">{@html kindIcon(KIND_META[kind].icon)}</span>
      <div>
        <h3>{KIND_META[kind].label}</h3>
        <p>{KIND_META[kind].desc}</p>
      </div>
    </div>
    <div class="gr-search">
      <input
        class="gr-input"
        type="text"
        placeholder="搜索会话 / 用户 / ID…"
        bind:value={keyword}
        onkeydown={(e) => e.key === 'Enter' && load(true)}
      />
        <WechatHoverButton text="搜索" onclick={() => load(true)} disabled={loading} class="!px-3 !py-1 !text-xs" />
        <WechatHoverButton text="导出" onclick={exportCsv} title="导出当前记录为 CSV" class="!px-3 !py-1 !text-xs" />
    </div>
  </header>

  <nav class="gr-nav">
    {#each Object.entries(KIND_META) as [k, meta]}
      <WechatHoverButton
        onclick={() => switchKind(k as Kind)}
        title={meta.desc}
        class={kind === k ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'}
      >
        {meta.label}
        {#if (totals[k as Kind] ?? 0) > 0}<span class="gr-count">{totals[k as Kind]}</span>{/if}
      </WechatHoverButton>
    {/each}
  </nav>

  {#if error}
    <div class="gr-error">⚠️ {error}</div>
  {/if}

  <div class="gr-content">
    <div class="gr-card">
      {#if kind === 'revokes'}
        <table class="gr-table">
          <thead><tr><th>会话</th><th>消息 ID</th><th>批次 ID</th><th>时间</th></tr></thead>
          <tbody>
            {#each items as it}
              <tr>
                <td><button class="gr-link" onclick={() => onopen?.(it.session_name ?? '', it.msg_local_id != null ? Number(it.msg_local_id) : undefined)}>{shortUser(it.session_name)}</button></td>
                <td class="gr-mono">{it.msg_local_id ?? '—'}</td>
                <td class="gr-mono">{it.batch_id ?? '—'}</td>
                <td class="gr-time">{fmtTime(it.msg_create_time)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else if kind === 'transfers'}
        <table class="gr-table">
          <thead><tr><th>会话</th><th>类型</th><th>收款方</th><th>付款方</th><th>时间</th></tr></thead>
          <tbody>
            {#each items as it}
              <tr>
                <td><button class="gr-link" onclick={() => onopen?.(it.session_name ?? '')}>{shortUser(it.session_name)}</button></td>
                <td>{transferSubType(it.pay_sub_type)}</td>
                <td>{shortUser(it.pay_receiver)}</td>
                <td>{shortUser(it.pay_payer)}</td>
                <td class="gr-time">{fmtTime(it.begin_transfer_time)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else if kind === 'redpackets'}
        <table class="gr-table">
          <thead><tr><th>会话</th><th>发送者</th><th>状态</th><th>类型</th><th>消息 ID</th></tr></thead>
          <tbody>
            {#each items as it}
              <tr>
                <td><button class="gr-link" onclick={() => onopen?.(it.session_name ?? '')}>{shortUser(it.session_name)}</button></td>
                <td>{shortUser(it.sender_user_name)}</td>
                <td>{hbStatus(it.hb_status)}</td>
                <td>{it.hb_type === 0 ? '普通红包' : `类型 ${it.hb_type}`}</td>
                <td class="gr-mono">{it.message_server_id ?? '—'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else if kind === 'finder'}
        <table class="gr-table">
          <thead><tr><th>视频号</th><th>直播状态</th><th>回放</th><th>直播 ID</th></tr></thead>
          <tbody>
            {#each items as it}
              <tr>
                <td>{shortUser(it.finder_username)}</td>
                <td>{liveStatus(it.live_status)}</td>
                <td>{Number(it.replay_status) === 1 ? '有回放' : '—'}</td>
                <td class="gr-mono">{it.finder_live_id ?? '—'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else if kind === 'miniprograms'}
        <table class="gr-table">
          <thead><tr><th>名称</th><th>用户名</th><th>AppID</th><th>更新时间</th></tr></thead>
          <tbody>
            {#each items as it}
              <tr>
                <td>{it.nickname || '（未命名）'}</td>
                <td>{shortUser(it.user_name)}</td>
                <td class="gr-mono">{it.app_id || '—'}</td>
                <td class="gr-time">{fmtTime(it.last_update_time)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <table class="gr-table">
          <thead><tr><th>用户</th><th>备注</th><th>验证消息</th><th>类型</th><th>时间</th></tr></thead>
          <tbody>
            {#each items as it}
              <tr>
                <td>{shortUser(it.user_name_)}</td>
                <td>{it.remark_ || '—'}</td>
                <td class="gr-verify">{it.content_ || '—'}</td>
                <td>{Number(it.is_sender_) === 1 ? '我发出的' : '收到的'}</td>
                <td class="gr-time">{fmtTime(it.timestamp_)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}

      {#if loading && items.length === 0}
        <div class="gr-state"><span class="gr-spin"></span>加载中…</div>
      {:else if items.length === 0}
        <div class="gr-state">
          <span class="gr-empty-icon">{@html kindIcon(KIND_META[kind].icon, 40)}</span>
          <span>暂无{KIND_META[kind].label}记录</span>
        </div>
      {/if}
    </div>

    {#if items.length < total}
      <WechatHoverButton text={loading ? '加载中…' : `加载更多（${items.length}/${total}）`} disabled={loading} onclick={() => load(false)} class="!px-3 !py-1 !text-xs" />
    {/if}
  </div>
</div>

<style>
  .gr-panel { flex: 1; overflow-y: auto; padding: 16px 18px 20px; display: flex; flex-direction: column; gap: 12px; scrollbar-width: thin; }
  .gr-hd { display: flex; align-items: center; justify-content: space-between; gap: 14px; flex-wrap: wrap; }
  .gr-title { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .gr-title-icon { width: 38px; height: 38px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; color: var(--wc-theme); background: color-mix(in srgb, var(--wc-theme) 10%, var(--wc-bg2)); border: 1px solid color-mix(in srgb, var(--wc-theme) 26%, var(--wc-border-light)); border-radius: 10px; }
  .gr-title h3 { margin: 0; font-size: 15px; font-weight: 700; color: var(--wc-text); }
  .gr-title p { margin: 2px 0 0; font-size: 11.5px; color: var(--wc-muted); }
  .gr-search { display: flex; gap: 8px; flex-shrink: 0; }
  .gr-input {
    padding: 7px 11px; border-radius: 8px; border: 1px solid var(--wc-border);
    background: var(--wc-bg); color: var(--wc-text); font-size: 12px;
    width: 220px; outline: none; transition: border-color .12s ease;
  }
  .gr-input:focus { border-color: var(--wc-accent, #4a9eff); }
  .gr-input::placeholder { color: var(--wc-muted); }
  .gr-nav { display: flex; gap: 8px; flex-wrap: wrap; padding-bottom: 12px; border-bottom: 1px solid var(--wc-border-light); }
  .gr-count { font-size: 11.5px; font-weight: 700; background: rgba(255,255,255,.25); border-radius: 999px; padding: 1px 7px; line-height: 15px; }
  .gr-error { padding: 10px 14px; border-radius: 8px; background: color-mix(in srgb, #ef4444 12%, transparent); color: #f87171; border: 1px solid color-mix(in srgb, #ef4444 30%, transparent); font-size: 12px; line-height: 1.5; word-break: break-all; }
  .gr-content { flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; scrollbar-width: thin; }
  .gr-card { border: 1px solid var(--wc-border-light); border-radius: 10px; overflow: hidden; background: var(--wc-card); }
  .gr-table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .gr-table th {
    text-align: left; padding: 9px 12px; color: var(--wc-muted); font-weight: 600;
    border-bottom: 1px solid var(--wc-border-light); font-size: 11.5px; letter-spacing: .3px;
    position: sticky; top: 0; z-index: 1; background: var(--wc-bg2); white-space: nowrap;
  }
  .gr-table td { padding: 9px 12px; border-bottom: 1px solid var(--wc-border-light); color: var(--wc-text); vertical-align: middle; }
  .gr-table tbody tr:last-child td { border-bottom: none; }
  .gr-table tbody tr { transition: background .12s, box-shadow .12s; }
  .gr-table tbody tr:hover td { background: var(--wc-item-hover); }
  .gr-table tbody tr:hover { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--app-wc-accent) 18%, transparent), inset 0 0 16px color-mix(in srgb, var(--app-wc-accent) 5%, transparent); }
  .gr-mono { font-family: 'Cascadia Code', 'Fira Code', Consolas, monospace; font-size: 11.5px; font-variant-numeric: tabular-nums; color: var(--wc-text2); }
  .gr-time { color: var(--wc-muted); white-space: nowrap; }
  .gr-verify { max-width: 320px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gr-link { background: none; border: none; padding: 0; color: var(--wc-accent, #4a9eff); cursor: pointer; font-size: 12px; text-decoration: none; font-weight: 600; }
  .gr-link:hover { text-decoration: underline; }
  .gr-state { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; color: var(--wc-muted); font-size: 13px; padding: 42px 0; }
  .gr-empty-icon { display: inline-flex; opacity: .8; color: var(--wc-muted); }
  .gr-spin { width: 16px; height: 16px; border: 2px solid var(--wc-border); border-top-color: var(--wc-text2); border-radius: 50%; animation: gr-spin .7s linear infinite; }
  @keyframes gr-spin { to { transform: rotate(360deg); } }
</style>
