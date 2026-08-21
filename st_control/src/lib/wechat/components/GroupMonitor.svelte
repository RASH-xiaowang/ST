<script lang="ts">
  import { errText } from '../../format';
  import { onMount, onDestroy } from 'svelte';
  import { getSessionList } from '../services/ipc';
  import { escapeHtml } from '../utils';
  import { automationApi } from '../../automation/services/ipc';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import type { SessionEntry, WeChatMessagePayload } from '../types';
  import { filterByAnyKeyword, matchMonitors, type MonitorRule } from '../utils/panel';
  import { createMsg } from '../../services/msg.svelte';
  import WechatHoverButton from './WechatHoverButton.svelte';
  import { NativeSelect, NativeSelectOption } from '../../components/ui/native-select';

  interface MonitorItem extends MonitorRule {
    hits: number;
  }

  interface FeedMsg {
    id: string;
    time: string;
    sender: string;
    content: string;
    username: string;
    local_id?: number;
    hitIds: number[];
    raw: MonitorPayload;
  }

  /** wechat-message 事件载荷：单条消息或 batch 信封 */
  type MonitorPayload = WeChatMessagePayload & { batch?: boolean; messages?: WeChatMessagePayload[] };

  let { onJump = () => {} }: { onJump?: (c: { username: string; local_id?: number; name?: string }) => void } = $props();

  let groups = $state<SessionEntry[]>([]);
  let groupsLoading = $state(false);
  let scopeMode = $state<'all' | 'group'>('all');
  let selectedGroup = $state('');
  let searchText = $state('');
  let monitors = $state<MonitorItem[]>([]);
  let newKind = $state<MonitorItem['kind']>('keyword');
  let newValue = $state('');
  let feed = $state<FeedMsg[]>([]);
  let onlyHits = $state(false);
  let paused = $state(false);
  const msg = createMsg(4000);
  let unlisten: UnlistenFn | null = null;
  let seq = 0;
  let received = $state(0);

  const KIND_LABEL: Record<MonitorItem['kind'], string> = {
    keyword: '关键词',
    regex: '正则',
    sender: '成员',
    media: '媒体',
  };

  const QUICK_KEYWORDS = ['项目', '报销', '合同', '报价', '@我', '紧急', '已读'];

  async function loadGroups() {
    groupsLoading = true;
    try {
    const list = await getSessionList();
      groups = (list || [])
        .filter((s) => s?.is_group || (s?.username || '').endsWith('@chatroom'))
        .sort((a, b) => (b?.sort_ts ?? 0) - (a?.sort_ts ?? 0));
      if (groups.length > 0 && !selectedGroup) selectedGroup = groups[0].username;
    } catch (e: unknown) {
      msg.show(`加载群列表失败：${errText(e)}`, false);
    } finally {
      groupsLoading = false;
    }
  }

  const filteredGroups = $derived(
    filterByAnyKeyword(groups, searchText, (g) => g.name || '', (g) => g.username || ''),
  );

  function selectedGroupName(): string {
    return groups.find((g) => g.username === selectedGroup)?.name || selectedGroup || '未选择群聊';
  }

  function addMonitor() {
    const v = newValue.trim();
    if (!v) return;
    // 关键词以 @ 开头时转为成员监控
    let kind = newKind;
    let value = v;
    if (newKind === 'keyword' && v.startsWith('@')) {
      kind = 'sender';
      value = v.slice(1);
    }
    monitors.push({ id: ++seq, kind, value, enabled: true, hits: 0 });
    newValue = '';
  }

  function removeMonitor(id: number) {
    monitors = monitors.filter((m) => m.id !== id);
  }

  function toggleMonitor(id: number) {
    const m = monitors.find((x) => x.id === id);
    if (m) m.enabled = !m.enabled;
  }

  function highlight(text: string): string {
    const terms = monitors
      .filter((m) => m.enabled && m.kind === 'keyword' && m.value.trim())
      .map((m) => m.value.trim())
      .filter((t, i, a) => a.indexOf(t) === i)
      .sort((a, b) => b.length - a.length);
    if (terms.length === 0) return escapeHtml(text || '');
    const re = new RegExp(terms.map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|'), 'gi');
    return escapeHtml(text || '').replace(re, (m) => `<mark>${m}</mark>`);
  }

  function handlePayload(payload: MonitorPayload) {
    if (paused) return;
    received += 1;
    // 批量信封拆包
    if (payload && payload.batch === true && Array.isArray(payload.messages)) {
      for (const m of payload.messages) handlePayload(m);
      return;
    }
    const username = String(payload?.username ?? '');
    if (scopeMode === 'group' && username !== selectedGroup) return;
    const hitIds = matchMonitors(payload, monitors);
    for (const id of hitIds) {
      const mon = monitors.find((x) => x.id === id);
      if (mon) mon.hits += 1;
    }
    const ts = Math.floor((payload?.timestamp ?? 0) / 1_000_000);
    const d = ts > 0 ? new Date(ts * 1000) : new Date();
    const pad = (n: number) => String(n).padStart(2, '0');
    const entry: FeedMsg = {
      id: `${payload?.ack_id ?? Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
      time: `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`,
      sender: String(payload?.sender ?? payload?.sender_username ?? ''),
      content: String(payload?.content ?? ''),
      username,
      local_id: payload?.local_id ?? undefined,
      hitIds,
      raw: payload,
    };
    feed.unshift(entry);
    if (feed.length > 200) feed = feed.slice(0, 200);
  }

  const visibleFeed = $derived(onlyHits ? feed.filter((f) => f.hitIds.length > 0) : feed);
  const hitCount = $derived(feed.reduce((a, f) => a + (f.hitIds.length > 0 ? 1 : 0), 0));
  const activeMonitors = $derived(monitors.filter((m) => m.enabled));

  async function createRule() {
    const conds = monitors.filter((m) => m.enabled && m.value.trim());
    if (scopeMode === 'group') {
      if (!selectedGroup) {
        msg.show('请先选择要监控的群聊', false);
        return;
      }
    }
    if (conds.length === 0) {
      msg.show('请至少添加一个监控条件', false);
      return;
    }
    const conditions: Array<{ field: string; op: string; value: string }> = [];
    if (scopeMode === 'group') {
      conditions.push({ field: 'session', op: 'equals', value: selectedGroup });
    }
    for (const m of conds) {
      if (m.kind === 'keyword' || m.kind === 'regex') {
        conditions.push({
          field: 'content',
          op: m.kind === 'regex' ? 'regex' : 'contains',
          value: m.value,
        });
      } else if (m.kind === 'sender') {
        conditions.push({ field: 'sender', op: 'contains', value: m.value });
      } else if (m.kind === 'media') {
        conditions.push({ field: 'media_type', op: 'equals', value: m.value });
      }
    }
    const ruleName =
      scopeMode === 'group'
        ? `群监控：${selectedGroupName()}`
        : `群监控：${conds.map((c) => c.value).join(' / ')}`;
    try {
      const id = await automationApi.saveRule({
          id: null,
          name: ruleName,
          enabled: true,
          priority: 0,
          conditions,
          analyzeFields: [],
          promptOverride: '',
          providerId: '',
          model: '',
          dispatchMode: 'fixed',
          targetType: 'agent',
          targetId: '',
      });
      msg.show(`✅ 已创建自动化规则 #${id}，可在「自动化」页配置派发目标`);
    } catch (e: unknown) {
      msg.show(`创建规则失败：${errText(e)}`, false);
    }
  }

  function clearFeed() {
    feed = [];
    for (const m of monitors) m.hits = 0;
    received = 0;
  }

  onMount(async () => {
    loadGroups();
    try {
      unlisten = await listen<string | object>('wechat-message', (event) => {
        let payload: unknown = event.payload;
        if (typeof payload === 'string') {
          try {
            payload = JSON.parse(payload);
          } catch {
            return;
          }
        }
        handlePayload(payload as MonitorPayload);
      });
    } catch (e) {
      console.warn('[wechat:monitor] 事件监听失败:', e);
    }
  });

  onDestroy(() => {
    unlisten?.();
  });
</script>

<div class="wc-mon">
  <div class="wc-mon-hd">
    <div>
      <div class="wc-mon-title">群监控台</div>
      <div class="wc-mon-sub">实时监控群消息，命中关键词即时高亮，可一键转成自动化规则</div>
    </div>
    <div class="wc-mon-ctl">
      <NativeSelect size="sm" bind:value={scopeMode} onchange={() => (feed = [])}>
        <NativeSelectOption value="all">全部群聊</NativeSelectOption>
        <NativeSelectOption value="group">指定群聊</NativeSelectOption>
      </NativeSelect>
      {#if scopeMode === 'group'}
        <input
          type="text"
          placeholder="搜索群名…"
          bind:value={searchText}
          class="wc-mon-group-search"
        />
        <NativeSelect size="sm" bind:value={selectedGroup} disabled={groupsLoading}>
          {#if groupsLoading && filteredGroups.length === 0}
            <NativeSelectOption value="">加载群列表…</NativeSelectOption>
          {/if}
          {#each filteredGroups as g (g.username)}
            <NativeSelectOption value={g.username}>{g.name || g.username}</NativeSelectOption>
          {/each}
        </NativeSelect>
      {/if}
        <WechatHoverButton onclick={() => (paused = !paused)} class="!px-3 !py-1 !text-xs">
          {#if paused}
            <svg viewBox="0 0 24 24" width="11" height="11" fill="currentColor" aria-hidden="true"><path d="M8 5v14l11-7z"/></svg>
            <span>继续</span>
          {:else}
            <svg viewBox="0 0 24 24" width="11" height="11" fill="currentColor" aria-hidden="true"><rect x="6" y="5" width="4" height="14" rx="1"/><rect x="14" y="5" width="4" height="14" rx="1"/></svg>
            <span>暂停</span>
          {/if}
        </WechatHoverButton>
        <WechatHoverButton text="清空" onclick={clearFeed} class="!px-3 !py-1 !text-xs" />
    </div>
  </div>

  <div class="wc-mon-stats">
    <span class="wc-mon-chip">收到 {received}</span>
    <span class="wc-mon-chip">命中 {hitCount}</span>
    <span class="wc-mon-chip">命中率 {received ? Math.round((hitCount / received) * 100) : 0}%</span>
    <span class="wc-mon-chip">活跃条件 {activeMonitors.length}</span>
    {#if msg}
      <span class="wc-mon-msg" class:wc-mon-msg-err={!msg.state.ok}>{msg.state.text}</span>
    {/if}
  </div>

  <div class="wc-mon-main">
    <div class="wc-mon-config">
      <div class="wc-mon-config-title">监控条件</div>
      <div class="wc-mon-add">
        <NativeSelect size="sm" wrapperClass="shrink-0" bind:value={newKind}>
          <NativeSelectOption value="keyword">关键词</NativeSelectOption>
          <NativeSelectOption value="regex">正则</NativeSelectOption>
          <NativeSelectOption value="sender">成员</NativeSelectOption>
          <NativeSelectOption value="media">媒体类型</NativeSelectOption>
        </NativeSelect>
        <input
          type="text"
          placeholder={newKind === 'sender' ? '成员昵称 / wxid' : newKind === 'media' ? 'image / voice / video / file' : '关键词（@开头=成员）'}
          bind:value={newValue}
          onkeydown={(e) => { if (e.key === 'Enter') addMonitor(); }}
        />
        <WechatHoverButton text="添加" onclick={addMonitor} />
      </div>
      <div class="wc-mon-quick">
        {#each QUICK_KEYWORDS as k}
          <WechatHoverButton
            text={`+${k}`}
            onclick={() => {
              newKind = 'keyword';
              newValue = k;
              addMonitor();
            }}
            class="!px-3 !py-1 !text-xs"
          />
        {/each}
      </div>
      <div class="wc-mon-list">
        {#if monitors.length === 0}
          <div class="wc-mon-empty">尚未添加监控条件，可添加关键词或直接查看实时消息</div>
        {:else}
          {#each monitors as m (m.id)}
            <div class="wc-mon-item">
              <span class="wc-mon-item-kind">{KIND_LABEL[m.kind]}</span>
              <span class="wc-mon-item-value">{m.value}</span>
              <span class="wc-mon-item-hits">{m.hits}</span>
              <button class="wc-mon-item-toggle" onclick={() => toggleMonitor(m.id)} title={m.enabled ? '停用' : '启用'}>
                {m.enabled ? '●' : '○'}
              </button>
              <button class="wc-mon-item-del" onclick={() => removeMonitor(m.id)} title="删除">✕</button>
            </div>
          {/each}
        {/if}
      </div>
        <WechatHoverButton text="创建自动化规则" onclick={createRule} />
      <div class="wc-mon-rule-tip">规则将写入「自动化」页，命中后可配置 AI 分析 / 派发任务</div>
    </div>

    <div class="wc-mon-feed">
      <div class="wc-mon-feed-hd">
        <span>实时消息流{scopeMode === 'group' ? `：${selectedGroupName()}` : ''}</span>
        <label class="wc-mon-feed-toggle">
          <input type="checkbox" bind:checked={onlyHits} />
          仅显示命中
        </label>
      </div>
      <div class="wc-mon-feed-body">
        {#if visibleFeed.length === 0}
          <div class="wc-mon-empty">暂无消息{scopeMode === 'group' ? '，该群的新消息会实时出现在这里' : '，群聊新消息会实时出现在这里'}</div>
        {:else}
          {#each visibleFeed as f (f.id)}
            <div class="wc-mon-msg-item" class:wc-mon-msg-hit={f.hitIds.length > 0}>
              <div class="wc-mon-msg-top">
                <span class="wc-mon-msg-time">{f.time}</span>
                <span class="wc-mon-msg-sender">{f.sender || '系统'}</span>
                {#each f.hitIds as hid}
                  {@const m = monitors.find((x) => x.id === hid)}
                  {#if m}
                    <span class="wc-mon-msg-tag">{m.value}</span>
                  {/if}
                {/each}
                {#if f.local_id}
                  <button class="wc-mon-msg-jump" onclick={() => onJump({ username: f.username, local_id: f.local_id, name: f.sender })}>跳转 ›</button>
                {/if}
              </div>
              <div class="wc-mon-msg-content">{@html highlight(f.content)}</div>
            </div>
          {/each}
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .wc-mon {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    padding: 16px 20px;
    gap: 10px;
    box-sizing: border-box;
  }
  .wc-mon-hd {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }
  .wc-mon-title {
    font-size: 16px;
    font-weight: 700;
    color: var(--wc-text);
  }
  .wc-mon-sub {
    font-size: 11.5px;
    color: var(--wc-muted);
  }
  .wc-mon-ctl {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .wc-mon-ctl input {
    padding: 5px 8px;
    border: 1px solid var(--wc-border);
    border-radius: 5px;
    background: var(--wc-card);
    color: var(--wc-text);
    font-size: 12px;
  }
  .wc-mon-group-search {
    width: 130px;
  }
  .wc-mon-stats {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
    flex-shrink: 0;
  }
  .wc-mon-chip {
    font-size: 11.5px;
    padding: 3px 10px;
    border-radius: 999px;
    background: var(--wc-bg2);
    border: 1px solid var(--wc-border-light);
    color: var(--wc-text2);
  }
  .wc-mon-msg {
    font-size: 12px;
    color: #27ae60;
  }
  .wc-mon-msg-err {
    color: #c0392b;
  }
  .wc-mon-main {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 12px;
  }
  .wc-mon-config {
    width: 280px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    border: 1px solid var(--wc-border-light);
    border-radius: 10px;
    background: var(--wc-card);
    padding: 12px 14px;
    overflow-y: auto;
  }
  .wc-mon-config-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--wc-text);
  }
  .wc-mon-add {
    display: flex;
    gap: 6px;
  }
  .wc-mon-add input {
    flex: 1;
    min-width: 0;
    padding: 5px 8px;
    border: 1px solid var(--wc-border);
    border-radius: 5px;
    background: var(--wc-bg2);
    color: var(--wc-text);
    font-size: 12px;
  }
  .wc-mon-quick {
    display: flex;
    gap: 5px;
    flex-wrap: wrap;
  }
  .wc-mon-list {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .wc-mon-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    border-radius: 6px;
    background: var(--wc-bg2);
    font-size: 12px;
  }
  .wc-mon-item-kind {
    font-size: 11.5px;
    padding: 1px 6px;
    border-radius: 999px;
    background: rgba(87, 107, 149, 0.12);
    color: var(--wc-theme, #576b95);
    flex-shrink: 0;
  }
  .wc-mon-item-value {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--wc-text);
  }
  .wc-mon-item-hits {
    font-size: 11.5px;
    font-weight: 600;
    color: #e67e22;
    flex-shrink: 0;
  }
  .wc-mon-item-toggle,
  .wc-mon-item-del {
    border: none;
    background: transparent;
    cursor: pointer;
    color: var(--wc-muted);
    font-size: 12px;
    flex-shrink: 0;
    padding: 0 2px;
  }
  .wc-mon-item-del:hover {
    color: #c0392b;
  }
  .wc-mon-rule-tip {
    font-size: 11.5px;
    color: var(--wc-muted);
    line-height: 1.5;
  }
  .wc-mon-empty {
    color: var(--wc-muted);
    font-size: 12px;
    padding: 10px 0;
    text-align: center;
  }
  .wc-mon-feed {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--wc-border-light);
    border-radius: 10px;
    background: var(--wc-card);
    overflow: hidden;
  }
  .wc-mon-feed-hd {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-bottom: 1px solid var(--wc-border-light);
    font-size: 12px;
    font-weight: 600;
    color: var(--wc-text);
    flex-shrink: 0;
  }
  .wc-mon-feed-toggle {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    font-weight: 400;
    color: var(--wc-muted);
    cursor: pointer;
  }
  .wc-mon-feed-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .wc-mon-msg-item {
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--wc-bg2);
    border: 1px solid transparent;
  }
  .wc-mon-msg-hit {
    border-color: rgba(230, 126, 34, 0.45);
    background: rgba(230, 126, 34, 0.06);
  }
  .wc-mon-msg-top {
    display: flex;
    align-items: center;
    gap: 7px;
    flex-wrap: wrap;
  }
  .wc-mon-msg-time {
    font-size: 11.5px;
    color: var(--wc-muted);
    flex-shrink: 0;
  }
  .wc-mon-msg-sender {
    font-size: 12px;
    font-weight: 600;
    color: var(--wc-text);
  }
  .wc-mon-msg-tag {
    font-size: 11.5px;
    padding: 1px 6px;
    border-radius: 999px;
    background: rgba(230, 126, 34, 0.16);
    color: #d35400;
  }
  .wc-mon-msg-jump {
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--wc-theme, #576b95);
    font-size: 11.5px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .wc-mon-msg-content {
    margin-top: 4px;
    font-size: 12px;
    line-height: 1.6;
    color: var(--wc-text2);
    word-break: break-all;
    white-space: pre-wrap;
  }
  .wc-mon-msg-content :global(mark) {
    background: rgba(255, 213, 79, 0.55);
    color: inherit;
    border-radius: 2px;
    padding: 0 1px;
  }
</style>
