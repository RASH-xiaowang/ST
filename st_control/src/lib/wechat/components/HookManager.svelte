<!--
  原图 Hook 管理：通过 img_helper.dll 模拟打开图片，强制微信下载高清原图。
  工作台布局（Operate）：状态条 → 工具栏 → 撑满剩余空间的会话列表 → 底部提示。
  - 白名单中的会话收到图片时自动触发原图下载，配合现有解码链路取原图；
  - 白名单为空 = 对所有会话生效（与 WeFlow 语义一致）；
  - 服务启用后修改名单立即重新注入微信进程。
-->
<script lang="ts">
  import { errText } from '../../format';
  import { onMount, onDestroy } from 'svelte';
  import { isGroup, isOfficial, kindOf, type SessionKind } from '../utils/session';
  import {
    getSessionList,
    imgHookSetWhitelist,
    imgHookStart,
    imgHookStatus,
    imgHookStop,
type ImgHookStatus,
  } from '../services/ipc';
import { hookStatusCls, hookStatusLabel } from '../utils/hook';
  import { filterByAnyKeyword } from '../utils/panel';
  import { createMsg } from '../../services/msg.svelte';
  import type { SessionEntry } from '../types';
  import LiveNumber from '../../components/fancy/LiveNumber.svelte';
  import WechatHoverButton from './WechatHoverButton.svelte';

  let sessions = $state<SessionEntry[]>([]);
  let sessionsLoading = $state(true);
  let kind = $state<SessionKind>('group');
  let keyword = $state('');
  let whitelist = $state<Set<string>>(new Set());
  let status = $state<ImgHookStatus | null>(null);
  let busy = $state(false);
  let error = $state('');
  const msg = createMsg(3500);
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  function avatarLetter(name: string): string {
    const s = (name || '?').trim();
    return s ? s.charAt(0).toUpperCase() : '?';
  }
  function colorFromName(name: string): string {
    const palette = ['#5b8def', '#f2a654', '#7bd6a3', '#9b7bff', '#e884b8', '#58c9d9', '#e5a85b', '#8f9fb5'];
    let h = 0;
    for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
    return palette[h % palette.length];
  }

  const filteredSessions = $derived(
    filterByAnyKeyword(
      kind === 'all' ? sessions : sessions.filter((s) => kindOf(s.username) === kind),
      keyword,
      (s) => s.name || '',
      (s) => s.username,
    ),
  );
  const filteredIds = $derived(filteredSessions.map((s) => s.username));
  const filteredSelectedCount = $derived(filteredIds.filter((id) => whitelist.has(id)).length);
  const allFilteredSelected = $derived(
    filteredIds.length > 0 && filteredSelectedCount === filteredIds.length,
  );
  const groupCount = $derived(sessions.filter((s) => isGroup(s.username)).length);

  async function loadSessions() {
    sessionsLoading = true;
    try {
      sessions = (await getSessionList()) ?? [];
    } catch (e: unknown) {
      error = '加载会话失败：' + (errText(e));
    } finally {
      sessionsLoading = false;
    }
  }

  async function refreshStatus() {
    try {
      status = await imgHookStatus();
      if (status?.whitelist) whitelist = new Set(status.whitelist);
    } catch (e: unknown) {
      error = '获取 Hook 状态失败：' + (errText(e));
    }
  }

  async function applyWhitelist(next: Set<string>) {
    whitelist = next;
    busy = true;
    try {
      status = await imgHookSetWhitelist([...next]);
      msg.show(`已更新 Hook 名单（${next.size} 个会话）`, true);
    } catch (e: unknown) {
      msg.show('更新名单失败：' + (errText(e)), false);
    } finally {
      busy = false;
    }
  }

  function toggleSession(id: string) {
    const next = new Set(whitelist);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    void applyWhitelist(next);
  }

  function selectFiltered() {
    const next = new Set(whitelist);
    filteredIds.forEach((id) => next.add(id));
    void applyWhitelist(next);
  }

  function selectGroups() {
    const next = new Set(whitelist);
    sessions.filter((s) => isGroup(s.username)).forEach((s) => next.add(s.username));
    void applyWhitelist(next);
  }

  function clearAll() {
    void applyWhitelist(new Set());
  }

  async function toggleService() {
    const target = !status?.enabled;
    busy = true;
    try {
      if (target) {
        status = await imgHookStart([...whitelist]);
        msg.show(status?.hooked ? 'Hook 已注入微信进程' : '服务已开启，等待微信启动', true);
      } else {
        status = await imgHookStop();
        msg.show('原图 Hook 已关闭', true);
      }
    } catch (e: unknown) {
      msg.show('操作失败：' + (errText(e)), false);
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    loadSessions();
    refreshStatus();
    pollTimer = setInterval(refreshStatus, 5000);
  });
  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
  });

  const statusLabel = $derived(hookStatusLabel(status));
  const statusCls = $derived(hookStatusCls(status));
</script>

<div class="hm-root">
  <!-- ═══ 状态条（命令区） ═══ -->
  <header class="hm-statusbar">
    <div class="hm-statusbar-left">
      <div class="hm-title">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
          <polyline points="14 2 14 8 20 8" />
          <path d="M8 13h3M12 17H8M16 13h1M17 17h1" />
        </svg>
        <span>原图 Hook</span>
      </div>
      <div class="hm-sub">模拟打开图片，强制微信下载高清原图</div>
    </div>
    <div class="hm-statusbar-right">
      <span class="hm-status {statusCls}">
        <i class="hm-status-dot"></i>
        {statusLabel}
      </span>
      {#if status?.pid}
        <span class="hm-stat mono">微信 PID {status.pid}</span>
      {/if}
      <span class="hm-stat">已开启 <b><LiveNumber value={whitelist.size} duration={400} /></b></span>
      <span class="hm-stat hm-stat-muted">DLL {status?.dll_ok ? '就绪' : '缺失'}</span>
      <label class="hm-switch" title="{status?.enabled ? '停用原图 Hook 服务' : '启用原图 Hook 服务'}">
        <input type="checkbox" checked={status?.enabled ?? false} disabled={busy || !status?.supported} onchange={toggleService} />
        <span class="hm-switch-slider"></span>
      </label>
    </div>
  </header>

  <!-- ═══ 工具栏 ═══ -->
  <div class="hm-toolbar">
    <div class="hm-search">
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
        <circle cx="11" cy="11" r="7" /><line x1="21" y1="21" x2="16.5" y2="16.5" />
      </svg>
      <input type="text" placeholder="搜索会话名称 / username…" bind:value={keyword} />
      {#if keyword}
      <button class="hm-search-clear" onclick={() => (keyword = '')} aria-label="清空搜索">×</button>
      {/if}
    </div>

    <div class="hm-kind" role="tablist" aria-label="会话类型筛选">
      {#each [
        { id: 'group', label: `群聊 ${groupCount}` },
        { id: 'private', label: '单聊' },
        { id: 'official', label: '公众号' },
        { id: 'all', label: '全部' },
      ] as k}
        <WechatHoverButton text={k.label} onclick={() => (kind = k.id as SessionKind)} class={kind === k.id ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
      {/each}
    </div>

    <div class="hm-actions">
      <span class="hm-selected">
        已选 <b>{whitelist.size}</b>
        {#if filteredSelectedCount > 0}<span class="hm-selected-in">（当前筛选 {filteredSelectedCount}）</span>{/if}
      </span>
      <WechatHoverButton text="全选当前" onclick={selectFiltered} disabled={filteredIds.length === 0 || allFilteredSelected} class="!px-3 !py-1 !text-xs" />
      <WechatHoverButton text="全选群聊" onclick={selectGroups} disabled={groupCount === 0} class="!px-3 !py-1 !text-xs" />
      <WechatHoverButton text="清空" onclick={clearAll} disabled={whitelist.size === 0} class="!px-3 !py-1 !text-xs" />
    </div>
  </div>

  {#if error || (msg.state.text && !msg.state.ok)}
    <div class="hm-banner hm-banner-err">{error || msg.state.text}</div>
  {:else if msg.state.text}
    <div class="hm-banner hm-banner-ok">{msg.state.text}</div>
  {/if}

  <!-- ═══ 会话列表（撑满剩余空间） ═══ -->
  <div class="hm-list-wrap">
    <div class="hm-list-hd">
      <span>会话（{filteredSessions.length}）</span>
      <span>原图 Hook</span>
    </div>
    <div class="hm-list">
      {#if sessionsLoading}
        <div class="hm-empty">
          <span class="hm-empty-spin"></span>
          <span>正在加载会话…</span>
        </div>
      {:else if filteredSessions.length === 0}
        <div class="hm-empty">
          <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true">
            <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" />
            <path d="M23 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" />
          </svg>
          <span>{keyword || kind !== 'all' ? '没有匹配的会话' : '暂无会话'}</span>
        </div>
      {:else}
        {#each filteredSessions as s (s.username)}
          <div class="hm-row" class:on={whitelist.has(s.username)}>
            <label class="hm-row-main">
              <input type="checkbox" checked={whitelist.has(s.username)} disabled={busy} onchange={() => toggleSession(s.username)} />
              <span class="hm-check"></span>
              <span class="hm-avatar" style="background:{colorFromName(s.name || s.username)}">{avatarLetter(s.name || s.username)}</span>
              <span class="hm-row-text">
                <span class="hm-name">{s.name || s.username}</span>
                <span class="hm-user">{s.username}</span>
              </span>
              {#if isGroup(s.username)}
                <span class="hm-tag">群聊</span>
              {:else if isOfficial(s.username)}
                <span class="hm-tag hm-tag-official">公众号</span>
              {/if}
            </label>
            <span class="hm-badge" class:on={whitelist.has(s.username)}>
              <i class="hm-badge-dot"></i>
              {whitelist.has(s.username) ? '已开启' : '未开启'}
            </span>
          </div>
        {/each}
      {/if}
    </div>
  </div>

  <!-- ═══ 底部提示 ═══ -->
  <footer class="hm-foot">
    {#if status?.enabled && whitelist.size === 0}
      <span class="hm-foot-note">名单为空，Hook 将默认对所有会话生效（与 WeFlow 语义一致）。</span>
    {/if}
    <span class="hm-foot-warn">
      <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
        <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" /><line x1="12" y1="8" x2="12" y2="13" />
      </svg>
      内存 Hook 修改微信行为，存在风控风险，请仅在必要会话开启。实现参考 WeFlow（CC BY-NC-SA 4.0）。
    </span>
  </footer>
</div>

<style>
  /* ── 工作台骨架：状态条 / 工具栏 / 列表撑满 / 底部提示 ── */
  .hm-root {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--wc-bg);
  }

  /* ── 状态条 ── */
  .hm-statusbar {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    padding: 10px 18px;
    border-bottom: 1px solid var(--wc-border);
    background: color-mix(in srgb, var(--wc-card) 55%, var(--wc-bg));
  }
  .hm-statusbar-left { min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .hm-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 15px;
    font-weight: 700;
    letter-spacing: 0.01em;
    color: var(--wc-text);
  }
  .hm-title svg { color: var(--wc-theme, #576b95); }
  .hm-sub { font-size: 12px; color: var(--wc-muted); }
  .hm-statusbar-right {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .hm-status {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 4px 12px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    border: 1px solid var(--wc-border);
    background: var(--wc-bg2);
    color: var(--wc-text2);
  }
  .hm-status-dot { width: 7px; height: 7px; border-radius: 50%; background: #8a93a5; }
  .hm-status-on { border-color: rgba(7, 193, 96, 0.4); background: rgba(7, 193, 96, 0.12); color: #7bd6a3; }
  .hm-status-on .hm-status-dot { background: #07c160; box-shadow: 0 0 8px #07c160; }
  .hm-status-pending { border-color: rgba(250, 173, 20, 0.4); background: rgba(250, 173, 20, 0.12); color: #f5b301; }
  .hm-status-pending .hm-status-dot { background: #f5b301; box-shadow: 0 0 8px #f5b301; animation: hm-pulse 1.6s ease-out infinite; }
  .hm-status-err { border-color: rgba(239, 68, 68, 0.4); background: rgba(239, 68, 68, 0.12); color: #f87171; }
  .hm-status-err .hm-status-dot { background: #ef4444; box-shadow: 0 0 8px #ef4444; }
  .hm-status-off { opacity: 0.75; }
  @keyframes hm-pulse {
    0% { transform: scale(0.6); opacity: 0.9; }
    70% { transform: scale(1.6); opacity: 0; }
    100% { transform: scale(1.6); opacity: 0; }
  }
  .hm-stat { font-size: 12px; color: var(--wc-text2); font-variant-numeric: tabular-nums; }
  .hm-stat b { color: var(--wc-text); font-weight: 700; }
  .hm-stat-muted { color: var(--wc-muted); }
  .mono { font-family: var(--font-mono); }

  /* ── 工具栏 ── */
  .hm-toolbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    padding: 10px 18px;
    border-bottom: 1px solid var(--wc-border);
  }
  .hm-search {
    flex: 1;
    min-width: 220px;
    max-width: 420px;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 10px;
    border: 1px solid var(--wc-border);
    border-radius: 8px;
    background: var(--wc-card);
    color: var(--wc-muted);
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .hm-search:focus-within {
    border-color: color-mix(in srgb, var(--wc-theme, #576b95) 60%, var(--wc-border));
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--wc-theme, #576b95) 16%, transparent);
  }
  .hm-search input {
    flex: 1;
    min-width: 0;
    border: none;
    background: none;
    outline: none;
    font-size: 12.5px;
    color: var(--wc-text);
  }
  .hm-search input::placeholder { color: var(--wc-muted); }
  .hm-search-clear {
    border: none;
    background: transparent;
    color: var(--wc-muted);
    cursor: pointer;
    font-size: 11.5px;
    padding: 2px 4px;
    border-radius: 4px;
  }
  .hm-search-clear:hover { color: var(--wc-text); background: var(--wc-item-hover); }
  .hm-kind {
    display: inline-flex;
    gap: 3px;
    background: var(--wc-bg2);
    border: 1px solid var(--wc-border);
    border-radius: 8px;
    padding: 3px;
  }
  .hm-kind-item {
    border: none;
    background: transparent;
    color: var(--wc-text2);
    font-size: 12px;
    padding: 4px 10px;
    border-radius: 6px;
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .hm-kind-item:hover { color: var(--wc-text); }
  .hm-kind-item.on { background: var(--wc-theme, #576b95); color: #fff; font-weight: 600; }
  .hm-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .hm-selected { font-size: 12px; color: var(--wc-muted); margin-right: 2px; white-space: nowrap; }
  .hm-selected b { color: var(--wc-theme, #576b95); font-weight: 700; }
  .hm-selected-in { opacity: 0.8; }

  /* ── 服务开关 ── */
  .hm-switch { position: relative; display: inline-flex; cursor: pointer; }
  .hm-switch input { position: absolute; opacity: 0; width: 0; height: 0; }
  .hm-switch-slider {
    width: 40px;
    height: 22px;
    border-radius: 999px;
    background: var(--wc-border-strong, #3a4557);
    position: relative;
    transition: background 0.18s;
  }
  .hm-switch-slider::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.35);
    transition: transform 0.18s;
  }
  .hm-switch input:checked + .hm-switch-slider { background: var(--wc-theme, #576b95); }
  .hm-switch input:checked + .hm-switch-slider::after { transform: translateX(18px); }
  .hm-switch input:disabled + .hm-switch-slider { opacity: 0.5; cursor: not-allowed; }

  /* ── 消息横幅 ── */
  .hm-banner {
    flex: none;
    margin: 10px 18px 0;
    padding: 7px 12px;
    border-radius: 8px;
    font-size: 12px;
    border: 1px solid var(--wc-border);
    background: var(--wc-bg2);
    color: var(--wc-text2);
  }
  .hm-banner-ok { border-color: rgba(7, 193, 96, 0.35); background: rgba(7, 193, 96, 0.08); color: #7bd6a3; }
  .hm-banner-err { border-color: rgba(239, 68, 68, 0.35); background: rgba(239, 68, 68, 0.08); color: #f87171; }

  /* ── 会话列表：占满剩余空间 ── */
  .hm-list-wrap {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 0 12px 12px;
  }
  .hm-list-hd {
    flex: none;
    display: flex;
    justify-content: space-between;
    padding: 10px 8px 8px;
    font-size: 11.5px;
    letter-spacing: 0.03em;
    color: var(--wc-muted);
    border-bottom: 1px solid var(--wc-border-light);
  }
  .hm-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 0 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hm-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--wc-muted);
    font-size: 12.5px;
    min-height: 160px;
  }
  .hm-empty svg { opacity: 0.55; }
  .hm-empty-spin {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid var(--wc-border);
    border-top-color: var(--wc-theme, #576b95);
    animation: hm-spin 0.8s linear infinite;
  }
  @keyframes hm-spin { to { transform: rotate(360deg); } }

  .hm-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 7px 10px;
    border-radius: 9px;
    border: 1px solid transparent;
    transition: background 0.12s, border-color 0.12s, box-shadow 0.12s;
  }
  .hm-row:hover { background: var(--wc-item-hover); }
  .hm-row.on {
    background: color-mix(in srgb, var(--app-wc-accent) 8%, var(--wc-bg2));
    border-color: color-mix(in srgb, var(--app-wc-accent) 30%, transparent);
    box-shadow: inset 2px 0 0 var(--app-wc-accent), 0 0 12px -5px color-mix(in srgb, var(--app-wc-accent) 45%, transparent);
  }
  .hm-row-main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
  }
  .hm-row-main input { position: absolute; opacity: 0; width: 0; height: 0; }
  .hm-check {
    width: 16px;
    height: 16px;
    flex: none;
    border-radius: 4px;
    border: 1.5px solid var(--wc-border-strong, #3a4557);
    background: var(--wc-card);
    position: relative;
    transition: all 0.12s;
  }
  .hm-row-main input:checked + .hm-check {
    background: var(--wc-theme, #576b95);
    border-color: var(--wc-theme, #576b95);
  }
  .hm-row-main input:checked + .hm-check::after {
    content: '';
    position: absolute;
    left: 4px;
    top: 1px;
    width: 4px;
    height: 8px;
    border: solid #fff;
    border-width: 0 2px 2px 0;
    transform: rotate(45deg);
  }
  .hm-row-main input:focus-visible + .hm-check {
    outline: 2px solid color-mix(in srgb, var(--wc-theme, #576b95) 50%, transparent);
    outline-offset: 2px;
  }
  .hm-avatar {
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: 8px;
    display: grid;
    place-items: center;
    color: #fff;
    font-size: 13px;
    font-weight: 700;
    user-select: none;
  }
  .hm-row-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .hm-name {
    font-size: 13px;
    color: var(--wc-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hm-user {
    font-size: 11.5px;
    color: var(--wc-muted);
    font-family: var(--font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hm-tag {
    flex: none;
    font-size: 11.5px;
    padding: 2px 7px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--app-wc-accent) 15%, transparent);
    color: var(--wc-text2);
    border: 1px solid color-mix(in srgb, var(--app-wc-accent) 25%, transparent);
  }
  .hm-tag-official { background: rgba(250, 173, 20, 0.12); color: #f5b301; border-color: rgba(250, 173, 20, 0.3); }
  .hm-badge {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    padding: 3px 10px;
    border-radius: 999px;
    border: 1px solid var(--wc-border);
    color: var(--wc-muted);
    background: var(--wc-bg2);
  }
  .hm-badge-dot { width: 6px; height: 6px; border-radius: 50%; background: #8a93a5; }
  .hm-badge.on {
    border-color: rgba(7, 193, 96, 0.4);
    background: rgba(7, 193, 96, 0.12);
    color: #7bd6a3;
  }
  .hm-badge.on .hm-badge-dot { background: #07c160; box-shadow: 0 0 7px #07c160; }

  /* ── 底部提示 ── */
  .hm-foot {
    flex: none;
    display: flex;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
    padding: 8px 18px;
    border-top: 1px solid var(--wc-border);
    background: color-mix(in srgb, var(--wc-card) 45%, var(--wc-bg));
  }
  .hm-foot-note { font-size: 11.5px; color: var(--wc-text2); }
  .hm-foot-warn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--wc-muted);
    line-height: 1.5;
  }
  .hm-foot-warn svg { flex: none; color: #f87171; }
</style>
