<script lang="ts">
  // ============================================================
  // 消息列表容器（蓝图 T-蓝图-7 第二步）：虚拟滚动状态 + 滚动/加载机制下沉
  // - 数据源（messages/加载状态）与业务回调（onLoadMore）由父组件注入
  // - 内部持有：估算高度/前缀和/滚动位置/可视窗口/吸底守护/顶部哨兵懒加载
  // - 对外方法（bind:this）：scrollToBottom / restorePosition / scrollToIdx /
  //   setStickToBottom / isStickToBottom / updateEstimate / loadMore /
  //   setMessages / appendMessages / prependMessages / clearMessages
  // ============================================================
  import { onMount, onDestroy, tick } from 'svelte';
  import type { WeChatMessage } from '../types';
  import {
    computePrefixSums,
    computeVisRange,
    estimateMsgHeight,
    shouldShowDivider,
    trimMessageWindow,
  } from '../utils/virtualList';
  import { formatDividerTime } from '../utils/format';
  import MessageRow, { type MessageRowCtx, type MessageRowActions } from './MessageRow.svelte';
  import WechatHoverButton from './WechatHoverButton.svelte';

  const MSG_MAX_KEEP = 1500; // 内存中保留的消息上限（原 300）

  let {
    messages,
    loading,
    error,
    hasMore,
    curSession,
    officialHistory,
    rowCtx,
    rowActions,
    onLoadMore,
    onOpenUrl,
    onVisibleChange,
  }: {
    messages: WeChatMessage[];
    loading: boolean;
    error: string;
    hasMore: boolean;
    curSession: string | null;
    officialHistory: Record<string, string>;
    rowCtx: MessageRowCtx;
    rowActions: MessageRowActions;
    /** 加载更多回调：返回 false 表示会话已切换等无需恢复滚动的位置 */
    onLoadMore: () => Promise<boolean>;
    onOpenUrl: (url?: string | null) => void;
    onVisibleChange?: (msgs: WeChatMessage[]) => void;
  } = $props();

  // ── 虚拟滚动状态（自 WeChatPanel 下沉）──
  let msgEstH = $state<number[]>([]);   // 与 messages 一一对应的估算高度（px）
  let msgPrefix = $state<number[]>([]); // 前缀和：msgPrefix[i] = 前 i 条总高
  let msgTotalEst = $state(0);
  let msgScrollTop = $state(0);
  let msgViewH = $state(600);
  /** 用户是否停留在消息列表底部附近（距底 <120px 视为吸底状态）。
   *  需为响应式：虚拟滚动窗口（visRange）依赖它决定是否覆盖到底部 */
  let stickToBottom = $state(true);
  /** 正在向上加载历史消息并手动恢复滚动位置，期间禁止自动吸底 */
  let restoringScroll = false;
  /** 消息内容容器（仅在有消息时渲染），供 ResizeObserver 观察。
   *  必须用 $state 声明，否则 bind:this 赋值不具响应性，$effect 不会重新触发 */
  let msgsInnerEl = $state<HTMLDivElement | null>(null);
  /** 消息顶部哨兵：进入视口即触发加载更多（分辨率无关，替代纯 scrollTop 阈值判断） */
  let msgTopSentinelEl = $state<HTMLDivElement | null>(null);
  /** 消息区容器尺寸观察器：容器变高/变矮（头部出现、窗口调整）时重新吸底 */
  let containerRO: ResizeObserver | null = null;
  let msgsEl = $state<HTMLElement | null>(null);
  let calibrateTimer: ReturnType<typeof setTimeout> | null = null;
  let scrollRaf = 0;
  let scrollDebounceId: ReturnType<typeof setTimeout> | null = null;
  /** 顶部哨兵抑制窗口：进入会话/刷新后的一小段时间内，即使哨兵可见也
   *  不触发加载更多。否则进场瞬间列表 scrollTop=0、哨兵必然处于视口内，
   *  loadMore 会立即拉取历史并把「吸底」滚动恢复打断——表现为最新消息
   *  停在视口下方，需要手动滚轮才能看到（进场贴底失败 bug）。 */
  let suppressSentinel = $state(false);
  let suppressTimer: ReturnType<typeof setTimeout> | null = null;

  function rebuildMsgMetrics() {
    const { prefix, total } = computePrefixSums(msgEstH);
    msgPrefix = prefix;
    msgTotalEst = total;
  }
  $effect(() => { void msgEstH; rebuildMsgMetrics(); });

  let visRange = $derived.by(() => computeVisRange(messages.length, msgTotalEst, msgViewH, msgPrefix, msgScrollTop, stickToBottom));
  let visibleMsgs = $derived.by(() => messages.slice(visRange.start, visRange.end));

  // 可视窗口变化时通知父组件（图片懒加载预检等）
  $effect(() => {
    visibleMsgs;
    onVisibleChange?.(visibleMsgs);
  });

  /** 滚动停止后按实际渲染高度校准（160ms 防抖，不打断滚动） */
  function scheduleCalibrate() {
    if (calibrateTimer) clearTimeout(calibrateTimer);
    calibrateTimer = setTimeout(() => {
      calibrateTimer = null;
      calibrateRenderedHeights();
    }, 160);
  }
  function calibrateRenderedHeights() {
    const el = msgsInnerEl;
    if (!el) return;
    const nodes = el.querySelectorAll<HTMLElement>('[data-idx]');
    if (!nodes.length) return;
    const est = msgEstH.slice();
    let changed = false;
    for (const node of nodes) {
      const gi = Number(node.dataset.idx);
      if (!Number.isFinite(gi) || gi < 0 || gi >= est.length) continue;
      let h = node.offsetHeight;
      if (h <= 0) continue;
      // 相邻时间分隔条计入该条高度
      const prev = node.previousElementSibling;
      if (prev?.classList.contains('wc-time-divider')) h += (prev as HTMLElement).offsetHeight;
      if (Math.abs(h - est[gi]) > 2) { est[gi] = h; changed = true; }
    }
    if (changed) msgEstH = est; // 触发 $effect 重建前缀和
  }

  /** 与 messages 对齐的估算高度维护（trim 内部同步；返回裁剪后的消息数组） */
  function trimToLimit(next: WeChatMessage[]): WeChatMessage[] {
    if (next.length <= MSG_MAX_KEEP) return next;
    const trimmed = trimMessageWindow(next, msgEstH, MSG_MAX_KEEP);
    msgEstH = trimmed.estH;
    msgScrollTop = Math.max(0, msgScrollTop - trimmed.removedH);
    return trimmed.messages;
  }
  /** 以下方法由父组件在更新 messages 后同步调用，保证消息数组与估算高度对齐 */
  export function setMessages(next: WeChatMessage[]): WeChatMessage[] {
    msgEstH = next.map(estimateMsgHeight);
    msgScrollTop = 0;
    // 进场/刷新后抑制哨兵一小段时间：等贴底滚动与图片加载稳定后再放行
    suppressSentinel = true;
    if (suppressTimer) clearTimeout(suppressTimer);
    suppressTimer = setTimeout(() => {
      suppressSentinel = false;
      suppressTimer = null;
    }, 900);
    return next;
  }
  export function appendMessages(next: WeChatMessage[], extra: WeChatMessage[]): WeChatMessage[] {
    msgEstH = [...msgEstH, ...extra.map(estimateMsgHeight)];
    return trimToLimit(next);
  }
  export function prependMessages(next: WeChatMessage[], extra: WeChatMessage[]): WeChatMessage[] {
    // 历史消息插入头部后，原可视窗口整体下移：滚动位置同步前移，
    // 保证"加载更多"后仍停留在用户正在看的位置
    const addedH = extra.reduce((a, m) => a + estimateMsgHeight(m), 0);
    msgEstH = [...extra.map(estimateMsgHeight), ...msgEstH];
    msgScrollTop += addedH;
    return trimToLimit(next);
  }
  export function clearMessages(): void {
    msgEstH = [];
    msgScrollTop = 0;
  }
  /** 单条高度失效（实时推送更新 / 编辑后）：按最新内容重算并触发校准 */
  export function updateEstimate(idx: number, height: number): void {
    if (msgEstH[idx] === undefined) return;
    msgEstH[idx] = height;
    scheduleCalibrate();
  }

  /**
   * 滚动到消息列表底部。
   * 用双 rAF 等待 Svelte DOM 更新与浏览器布局完成后再设置 scrollTop，
   * 否则图片/表情尚未撑开高度时 scrollHeight 偏小，最新消息会被裁掉半截。
   */
  export function scrollToBottom(): void {
    const el = msgsEl;
    if (!el) return;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        el.scrollTop = el.scrollHeight;
      });
    });
  }

  export function setStickToBottom(v: boolean): void {
    stickToBottom = v;
  }
  export function isStickToBottom(): boolean {
    return stickToBottom;
  }
  export function getScrollTop(): number {
    return msgScrollTop;
  }
  /** 刷新后恢复阅读位置：原本贴底则贴底，否则按原滚动位置定位 */
  export async function restorePosition(stick: boolean, scrollTop: number): Promise<void> {
    stickToBottom = stick;
    msgScrollTop = scrollTop;
    await tick();
    if (stick) {
      scrollToBottom();
    } else if (msgsEl) {
      msgsEl.scrollTop = Math.min(scrollTop, msgsEl.scrollHeight - msgsEl.clientHeight);
    }
  }
  /** 定位消息行：优先 data-idx（虚拟窗口），退回第 idx 个渲染行（全量场景） */
  export function scrollToIdx(idx: number): boolean {
    const inner = msgsInnerEl;
    if (!inner) return false;
    const el = inner.querySelector<HTMLElement>(`[data-idx="${idx}"]`)
      ?? inner.querySelectorAll<HTMLElement>('.wc-msg')[idx];
    if (!el) return false;
    el.scrollIntoView({ block: 'center' });
    return true;
  }
  /** 加载更多：捕获加载前高度，历史消息插入头部后恢复滚动位置 */
  export async function loadMore(): Promise<void> {
    const el = msgsEl;
    if (!el) return;
    const prevHeight = el.scrollHeight;
    restoringScroll = true;
    const ok = await onLoadMore();
    if (!ok) {
      // 会话已切换等场景：不复位滚动位置，直接放行吸底守护
      restoringScroll = false;
      return;
    }
    await tick();
    if (msgsEl) msgsEl.scrollTop = msgsEl.scrollHeight - prevHeight;
    // 恢复后按最终位置重算吸底状态：加载更多会把视口拉到「最新页顶部」
    // 或停留在原地，此时若仍按旧吸底状态处理，后续内容长高会与用户
    // 正在看的位置打架（进场贴底失败 bug 的另一半根因）
    if (msgsEl) {
      stickToBottom = msgsEl.scrollHeight - msgsEl.scrollTop - msgsEl.clientHeight < 120;
    }
    // 滚动位置恢复完成后再放行吸底守护
    setTimeout(() => { restoringScroll = false; }, 100);
  }

  /** 滚动边界检测（带 300ms 防抖） */
  function onScrollMsgs(e: Event) {
    const el = e.target as HTMLElement;
    // 跟踪用户是否停留在底部附近：图片/表情异步加载撑高内容时，
    // 仅在用户本来就在底部时才自动跟随吸底，不打扰翻看历史的人
    stickToBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 120;
    // 虚拟滚动：滚动位置用 rAF 节流更新窗口，避免每帧多次触发派生重算
    if (!scrollRaf) {
      scrollRaf = requestAnimationFrame(() => {
        scrollRaf = 0;
        msgScrollTop = el.scrollTop;
      });
    }
    if (scrollDebounceId) clearTimeout(scrollDebounceId);
    scrollDebounceId = setTimeout(() => {
      if (el.scrollTop < 60) {
        void loadMore();
      }
    }, 300);
    scheduleCalibrate();
  }

  /** 吸底守护（内容侧）：消息追加、图片/表情/背景等媒体异步加载撑高内容时，
   *  只要处于吸底状态就重新吸底。ResizeObserver 覆盖所有高度变化来源，
   *  不依赖 img load 事件（对 CSS 背景图、懒加载图同样生效）。 */
  $effect(() => {
    const inner = msgsInnerEl;
    if (!inner) return;
    const ro = new ResizeObserver(() => {
      if (stickToBottom && !restoringScroll) scrollToBottom();
    });
    ro.observe(inner);
    return () => ro.disconnect();
  });

  /** 吸底守护（容器侧）：进入界面后头部信息更新、顶栏出现/消失、窗口布局稳定
   *  导致容器高度变化时，已设置的 scrollTop 会偏离底部，需要重新吸底 */
  function setupBottomGuard() {
    if (!msgsEl || containerRO) return;
    containerRO = new ResizeObserver(() => {
      if (stickToBottom && !restoringScroll) scrollToBottom();
    });
    containerRO.observe(msgsEl);
  }
  function teardownBottomGuard() {
    containerRO?.disconnect();
    containerRO = null;
  }

  /** 顶部哨兵懒加载：消息列表最上方哨兵进入视口即加载更多历史消息。
   *  用 IntersectionObserver 替代纯 scrollTop 阈值判断，与容器高度/分辨率无关：
   *  任何屏幕下滚到接近顶部都会触发，且不依赖窗口尺寸变化。 */
  $effect(() => {
    const sentinel = msgTopSentinelEl;
    if (!sentinel || !curSession || !hasMore) return;
    // 进场稳定期内哨兵即使可见也不加载：等贴底滚动完成后再武装
    if (suppressSentinel) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries.some((en) => en.isIntersecting)) return;
        if (loading || !hasMore) return;
        void loadMore();
      },
      { root: msgsEl ?? null, rootMargin: "80px 0px 0px 0px" },
    );
    io.observe(sentinel);
    return () => io.disconnect();
  });

  /** 是否需要在两条消息之间插入时间分隔条（PC：间隔 >5 分钟） */
  function needDivider(idx: number): boolean {
    const cur = messages[idx];
    if (!cur) return false;
    if (idx === 0) {
      if (!cur.divider) cur.divider = formatDividerTime(cur.ts);
      return true;
    }
    const prev = messages[idx - 1];
    const need = shouldShowDivider(prev, cur);
    if (need && !cur.divider) cur.divider = formatDividerTime(cur.ts);
    return need;
  }

  onMount(() => {
    // 启动消息区吸底守护（媒体加载撑高内容时自动保持最新消息可见）
    setupBottomGuard();
  });
  onDestroy(() => {
    teardownBottomGuard();
    if (scrollDebounceId) clearTimeout(scrollDebounceId);
    if (calibrateTimer) clearTimeout(calibrateTimer);
    if (suppressTimer) clearTimeout(suppressTimer);
  });
</script>

<div class="wc-msgs" bind:this={msgsEl} onscroll={onScrollMsgs} bind:clientHeight={msgViewH}>
  {#if loading && messages.length === 0}
    <div class="wc-empty" style="height:100%"><span class="wc-loading-inline"></span>正在加载消息…</div>
  {:else if error}
    <div class="wc-empty wc-error-hint" style="height:100%">
      <p>⚠️ 消息加载失败</p>
      <p class="wc-error-text">{error}</p>
      <p class="wc-error-tip">请检查解密数据库是否存在，或点击 ▶ 启动监控自动解密</p>
    </div>
  {:else if messages.length === 0}
    {#if (curSession || '').startsWith('gh_') && officialHistory[curSession ?? '']}
      <div class="wc-empty wc-official-empty" style="height:100%">
        <p class="wc-official-empty-title">该公众号暂无本地消息记录</p>
        <p class="wc-error-tip">本地没有同步该公众号的文章，可打开微信中的历史消息页查看</p>
        <WechatHoverButton text="查看历史消息" onclick={() => onOpenUrl(officialHistory[curSession ?? ''])} class="!px-3 !py-1 !text-xs" />
      </div>
    {:else}
      <div class="wc-empty" style="height:100%">暂无消息记录</div>
    {/if}
  {:else}
    <div class="wc-msgs-inner" bind:this={msgsInnerEl}>
      {#if hasMore}
        <div class="wc-msg-top-bar" class:wc-msg-top-loading={loading} bind:this={msgTopSentinelEl}>
          {#if loading}
            <span class="wc-msgs-loading-tip"><span class="wc-loading-inline"></span>加载更多消息</span>
          {:else}
            <span class="wc-msg-top-hint">向上滑动加载更多历史消息</span>
          {/if}
        </div>
      {/if}
      {#if visRange.topPad > 0}
        <div class="wc-virtual-pad" style="height:{visRange.topPad}px" aria-hidden="true"></div>
      {/if}
      {#each visibleMsgs as m, vi (m.local_id+'_'+m.ts)}
        {@const gi = vi + visRange.start}
        <MessageRow
          {m}
          {gi}
          divider={needDivider(gi) ? m.divider ?? null : null}
          ctx={rowCtx}
          actions={rowActions}
        />
      {/each}
      {#if visRange.bottomPad > 0}
        <div class="wc-virtual-pad" style="height:{visRange.bottomPad}px" aria-hidden="true"></div>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* ── 消息列表容器 scoped CSS：自 WeChatPanel.svelte 迁移，保持样式等价 ── */
  .wc-msgs { flex:1; overflow-y:auto; padding:12px 16px; scrollbar-width:none; -ms-overflow-style:none; overscroll-behavior:contain; }
  .wc-msgs::-webkit-scrollbar { width:0; height:0; display:none; }
  .wc-virtual-pad { pointer-events:none; }
  .wc-msg-top-bar { padding:6px 0;text-align:center;font-size:11.5px;color:var(--wc-muted);transition:opacity .2s; }
  .wc-msg-top-loading { opacity:1; }
  .wc-msg-top-hint { opacity:0.6; }
  .wc-empty { display:flex;align-items:center;justify-content:center;color:var(--wc-muted);font-size:13px;padding:40px;text-align:center; }
  .wc-official-empty { flex-direction:column; gap:8px; }
  .wc-official-empty-title { font-size:13px; color:var(--wc-text2); margin:0; }
  .wc-error-hint { flex-direction:column; gap:6px; padding:24px 16px; }
  .wc-error-text { font-size:11.5px; color:#fa5151; word-break:break-all; }
  .wc-error-tip { font-size:11.5px; color:var(--wc-muted); line-height:1.5; }
  .wc-loading-inline { display:inline-block;width:14px;height:14px;margin-right:6px;border:2px solid var(--wc-border);border-top-color:var(--wc-text);border-radius:50%;animation:wc-spin .7s linear infinite;vertical-align:middle; }
</style>
