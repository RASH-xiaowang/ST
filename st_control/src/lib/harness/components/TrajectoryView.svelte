<script lang="ts">
  // ============================================================
  // TrajectoryView — 会话「轨迹」标签页（DSH ui-trajectory 迁移）
  // 台账数据由后端从会话日志投影（harness_trajectory），渲染与回放同源：
  // 按轮次分组（用户消息为轮边界），行类型 = 用户/助手/工具/系统；
  // 工具栏 = 搜索 / 全部折叠/展开 / 耗时切换。
  // ============================================================
  import type { TrajectoryEntry } from "../types";
  import SearchIcon from "@lucide/svelte/icons/search";
  import UserIcon from "@lucide/svelte/icons/user";
  import BotIcon from "@lucide/svelte/icons/bot";
  import Settings2Icon from "@lucide/svelte/icons/settings-2";
  import CheckIcon from "@lucide/svelte/icons/check";
  import XIcon from "@lucide/svelte/icons/x";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import CopyIcon from "@lucide/svelte/icons/copy";

  let { entries = [], turnCount = 0, toolCallCount = 0, onOpenPath, onInspect }: {
    entries?: TrajectoryEntry[];
    turnCount?: number;
    toolCallCount?: number;
    /** 打开文件（产物/路径点击） */
    onOpenPath?: (path: string) => void;
    /** Inspect 跳转：把轨迹行送到右侧详情面板（DSH Inspect 语义） */
    onInspect?: (e: TrajectoryEntry) => void;
  } = $props();

  let search = $state("");
  let showDuration = $state(true);
  let turnsCollapsed = $state(false);
  let collapsedTurns = $state<Set<number>>(new Set());
  let expandedEntries = $state<Set<string>>(new Set());
  let copiedText = $state("");

  /** 带唯一渲染身份的条目：后端 seq 是事件序号，同一事件的多条工具
   * 调用共享同一 seq（AssistantToolCalls 一次声明多个 call），不能作为
   * keyed each 的 key（重复 key 会让 Svelte 渲染崩溃）。投影时注入
   * 严格递增的 uid 作为渲染身份与展开状态身份。 */
  type TrajEntry = TrajectoryEntry & { uid: string };

  /** 轮次分组：user 条目开启新轮（turn 从 0 起），其余条目归入当前轮 */
  const grouped = $derived.by(() => {
    const groups: { turn: number; entries: TrajEntry[] }[] = [];
    let turn = -1;
    let uid = 0;
    for (const e of entries) {
      if (e.kind === "user") turn += 1;
      let g = groups.at(-1);
      if (g === undefined || turn !== g.turn) {
        g = { turn, entries: [] };
        groups.push(g);
      }
      g.entries.push({ ...e, uid: String(uid++) });
    }
    return groups;
  });

  /** 搜索过滤（名称/内容/参数/结果/摘要） */
  const filtered = $derived.by(() => {
    const q = search.trim().toLowerCase();
    if (!q) return grouped;
    return grouped
      .map((g) => ({
        turn: g.turn,
        entries: g.entries.filter((e) => {
          const hay = [
            e.kind === "user" ? e.content : "",
            e.kind === "assistant" ? e.content : "",
            e.kind === "tool" ? `${e.name} ${e.args} ${e.result}` : "",
            e.kind === "system" ? `${e.summary} ${e.detail}` : "",
          ].join("\n").toLowerCase();
          return hay.includes(q);
        }),
      }))
      .filter((g) => g.entries.length > 0);
  });

  // ─── 表格视图（DSH TrajectoryTable 迁移） ───
  // 行模型 = 轮次折叠摘要行（turn）+ 条目行（entry），拍平 filtered；
  // 固定行高 → 窗口化虚拟滚动（只渲染可视区 ±8 行）。
  type TrajRowModel =
    | { kind: "turn"; turn: number; steps: number; toolCalls: number }
    | { kind: "entry"; entry: TrajEntry; turn: number };
  const tableRows = $derived.by(() => {
    const rows: TrajRowModel[] = [];
    for (const g of filtered) {
      const tools = g.entries.filter((e) => e.kind === "tool").length;
      rows.push({ kind: "turn", turn: g.turn, steps: g.entries.length, toolCalls: tools });
      for (const e of g.entries) rows.push({ kind: "entry", entry: e, turn: g.turn });
    }
    return rows;
  });
  const TABLE_ROW_H = 40;
  let trajMode = $state<"timeline" | "table">("timeline");
  let trajScrollTop = $state(0);
  let trajViewH = $state(600);
  const tableStart = $derived(Math.max(0, Math.floor(trajScrollTop / TABLE_ROW_H) - 8));
  const tableEnd = $derived(
    Math.min(tableRows.length, Math.ceil((trajScrollTop + trajViewH) / TABLE_ROW_H) + 8),
  );
  const tableVisible = $derived(tableRows.slice(tableStart, tableEnd));
  function trajRowKey(r: TrajRowModel): string {
    return r.kind === "turn" ? `turn-${r.turn}` : `e-${r.entry.uid}`;
  }
  function trajRowTop(i: number): string {
    return `${(tableStart + i) * TABLE_ROW_H}px`;
  }
  /** 表格行类型标签（DSH 轨迹类型：SYSTEM/USER/CONTEXT/COMPACTED/ASSISTANT/TOOL） */
  function trajTypeLabel(e: TrajEntry): string {
    switch (e.kind) {
      case "user": return "USER";
      case "assistant": return "ASSISTANT";
      case "tool": return "TOOL";
      case "system": return e.event === "context" ? "CONTEXT" : e.event === "compaction" ? "COMPACTED" : "SYSTEM";
      default: return "SYSTEM";
    }
  }

  function fmtDuration(ms: number): string {
    if (ms >= 1000) return `${(ms / 1000).toFixed(1)}s`;
    return `${Math.round(ms)}ms`;
  }

  function fmtTime(t: string): string {
    // DB created_at：%Y-%m-%dT%H:%M:%S → 显示 HH:MM:SS
    const m = t.match(/(\d{2}):(\d{2}):(\d{2})/);
    return m ? `${m[1]}:${m[2]}:${m[3]}` : t;
  }

  function truncate(s: string, n: number): string {
    if (s.length <= n) return s;
    return s.slice(0, n) + "…";
  }

  function prettyText(s?: string): string {
    if (!s) return "";
    try {
      return JSON.stringify(JSON.parse(s), null, 2);
    } catch {
      return s;
    }
  }

  async function copyText(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      /* 剪贴板不可用时静默忽略 */
    }
    copiedText = text.slice(0, 20);
    window.setTimeout(() => {
      if (copiedText === text.slice(0, 20)) copiedText = "";
    }, 1500);
  }

  function toggleTurn(turn: number) {
    const next = new Set(collapsedTurns);
    if (next.has(turn)) next.delete(turn);
    else next.add(turn);
    collapsedTurns = next;
  }

  function toggleAllTurns() {
    turnsCollapsed = !turnsCollapsed;
    collapsedTurns = turnsCollapsed ? new Set(grouped.map((g) => g.turn)) : new Set();
  }

  function toggleEntry(uid: string) {
    const next = new Set(expandedEntries);
    if (next.has(uid)) next.delete(uid);
    else next.add(uid);
    expandedEntries = next;
  }
</script>

<div class="hns-trajectory">
  <div class="hns-traj-toolbar">
    <span class="hns-traj-title">轨迹</span>
    <span class="hns-traj-count">{turnCount} 轮 · {toolCallCount} 次工具调用</span>
    <span class="hns-traj-search">
      <SearchIcon class="size-3" />
      <input bind:value={search} placeholder="搜索轨迹…" aria-label="搜索轨迹" />
    </span>
    <button
      class="hns-traj-btn"
      onclick={toggleAllTurns}
      title={turnsCollapsed ? "展开全部轮次" : "折叠全部轮次"}
    >
      {turnsCollapsed ? "⊞ 展开全部" : "⊟ 折叠全部"}
    </button>
    <button
      class="hns-traj-btn"
      class:on={trajMode === "timeline"}
      onclick={() => (trajMode = "timeline")}
      title="时间线视图"
    >
      时间线
    </button>
    <button
      class="hns-traj-btn"
      class:on={trajMode === "table"}
      onclick={() => (trajMode = "table")}
      title="表格视图（虚拟滚动）"
    >
      表格
    </button>
    <button
      class="hns-traj-btn"
      class:on={showDuration}
      onclick={() => (showDuration = !showDuration)}
      title="切换耗时显示"
    >
      <ClockIcon class="size-3.5" />耗时
    </button>
  </div>

  {#if trajMode === "table"}
    <div
      class="hns-traj-table"
      bind:clientHeight={trajViewH}
      onscroll={(e) => (trajScrollTop = (e.currentTarget as HTMLElement).scrollTop)}
    >
      <div class="hns-traj-table-head">
        <span class="hns-tcol-type">类型</span>
        <span class="hns-tcol-time">时间</span>
        <span class="hns-tcol-dur">时长</span>
        <span class="hns-tcol-sum">摘要</span>
      </div>
      <div
        class="hns-traj-table-spacer"
        style:height={`${Math.max(1, tableRows.length) * TABLE_ROW_H}px`}
      >
        {#each tableVisible as r, i (trajRowKey(r))}
          <div
            class="hns-traj-trow"
            class:turnrow={r.kind === "turn"}
            style:top={trajRowTop(i)}
          >
            {#if r.kind === "turn"}
              <span class="hns-tcol-type">
                <span class="hns-trow-turnname">轮 {r.turn + 1}</span>
              </span>
              <span class="hns-tcol-time"></span>
              <span class="hns-tcol-dur"></span>
              <span class="hns-tcol-sum hns-trow-meta">
                {r.steps} 条事件 · {r.toolCalls} 次工具调用
              </span>
            {:else}
              {@const e = r.entry}
              <span class="hns-tcol-type">
                {#if e.kind === "user"}
                  <UserIcon class="size-3" /><span class="hns-trow-type">USER</span>
                {:else if e.kind === "assistant"}
                  <BotIcon class="size-3" /><span class="hns-trow-type">ASSISTANT</span>
                {:else if e.kind === "tool"}
                  <Settings2Icon class="size-3" /><span class="hns-trow-type">TOOL</span>
                {:else}
                  <Settings2Icon class="size-3" /><span class="hns-trow-type">{trajTypeLabel(e)}</span>
                {/if}
              </span>
              <span class="hns-tcol-time" title={e.time}>{fmtTime(e.time)}</span>
              <span class="hns-tcol-dur">
                {#if e.kind === "tool" && showDuration}{fmtDuration(e.duration_ms)}{/if}
              </span>
              <span class="hns-tcol-sum">
                {#if e.kind === "user"}
                  {truncate(e.content, 140)}
                {:else if e.kind === "assistant"}
                  {truncate(e.content, 140)}
                  {#if e.steps > 0}<span class="hns-trow-meta"> · {e.steps} 步 · {e.tool_calls} 调用</span>{/if}
                {:else if e.kind === "tool"}
                  <span class="hns-trow-toolname">{e.name}</span>
                  {truncate(prettyText(e.args), 90)}
                  {#if !e.ok}<span class="hns-trow-err">✕</span>{/if}
                {:else}
                  <span class="hns-trow-sys">{e.summary}</span>
                {/if}
              </span>
              {#if onInspect}
                <button
                  class="hns-traj-inspect"
                  onclick={() => onInspect(e)}
                  title="在详情面板检查（Inspect）"
                >
                  检查
                </button>
              {/if}
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {:else}
  <div class="hns-traj-scroll">
    {#if filtered.length === 0}
      <div class="hns-traj-empty">{search ? "无匹配轨迹" : "暂无轨迹"}</div>
    {/if}
    {#each filtered as g (g.turn)}
      <div class="hns-traj-turn" class:collapsed={collapsedTurns.has(g.turn)}>
        <button class="hns-traj-turn-head" onclick={() => toggleTurn(g.turn)}>
          {#if collapsedTurns.has(g.turn)}
            <ChevronRightIcon class="size-3.5" />
          {:else}
            <ChevronDownIcon class="size-3.5" />
          {/if}
          <span class="hns-traj-turn-name">轮 {g.turn + 1}</span>
          <span class="hns-traj-turn-meta">
            {g.entries.filter((e) => e.kind === "tool").length} 次工具调用
          </span>
        </button>
        {#if !collapsedTurns.has(g.turn)}
          <div class="hns-traj-rows">
            {#each g.entries as e (e.uid)}
              {#if e.kind === "user"}
                <div class="hns-traj-row hns-traj-user">
                  <span class="hns-traj-ico"><UserIcon class="size-3.5" /></span>
                  <span class="hns-traj-time" title={e.time}>{fmtTime(e.time)}</span>
                  <span class="hns-traj-label">用户</span>
                  <span class="hns-traj-summary">{truncate(e.content, 200)}</span>
                  {#if e.content.length > 200}
                    <button class="hns-traj-more" onclick={() => toggleEntry(e.uid)}>
                      {expandedEntries.has(e.uid) ? "收起" : "展开"}
                    </button>
                  {/if}
                </div>
                {#if e.content.length > 200 && expandedEntries.has(e.uid)}
                  <div class="hns-traj-detail">
                    <div class="hns-traj-field">
                      <div class="hns-traj-field-head">
                        <span>完整内容</span>
                        <button class="hns-traj-copy" onclick={() => copyText(e.content)}>复制</button>
                      </div>
                      <pre class="hns-traj-pre">{e.content}</pre>
                    </div>
                  </div>
                {/if}
              {:else if e.kind === "assistant"}
                <div class="hns-traj-row hns-traj-assistant">
                  <span class="hns-traj-ico"><BotIcon class="size-3.5" /></span>
                  <span class="hns-traj-time" title={e.time}>{fmtTime(e.time)}</span>
                  <span class="hns-traj-label">助手</span>
                  <span class="hns-traj-summary">{truncate(e.content, 200)}</span>
                  {#if e.steps > 0 || e.tool_calls > 0}
                    <span class="hns-traj-meta">{e.steps} 步 · {e.tool_calls} 调用</span>
                  {/if}
                  {#if e.content.length > 200}
                    <button class="hns-traj-more" onclick={() => toggleEntry(e.uid)}>
                      {expandedEntries.has(e.uid) ? "收起" : "展开"}
                    </button>
                  {/if}
                </div>
                {#if e.content.length > 200 && expandedEntries.has(e.uid)}
                  <div class="hns-traj-detail">
                    <div class="hns-traj-field">
                      <div class="hns-traj-field-head">
                        <span>完整内容</span>
                        <button class="hns-traj-copy" onclick={() => copyText(e.content)}>复制</button>
                      </div>
                      <pre class="hns-traj-pre">{e.content}</pre>
                    </div>
                  </div>
                {/if}
              {:else if e.kind === "tool"}
                <div class="hns-traj-row hns-traj-tool" class:err={!e.ok}>
                  <span class="hns-traj-ico">
                    {#if e.ok}<CheckIcon class="size-3.5" />{:else}<XIcon class="size-3.5" />{/if}
                  </span>
                  <span class="hns-traj-time" title={e.time}>{fmtTime(e.time)}</span>
                  <span class="hns-traj-toolname">{e.name}</span>
                  <span class="hns-traj-summary">
                    {truncate(prettyText(e.args), 120)}
                    {#if showDuration}<span class="hns-traj-dur">· {fmtDuration(e.duration_ms)}</span>{/if}
                  </span>
                  <button class="hns-traj-more" onclick={() => toggleEntry(e.uid)}>
                    {expandedEntries.has(e.uid) ? "收起" : "详情"}
                  </button>
                  {#if onInspect}
                    <button class="hns-traj-more" onclick={() => onInspect(e)} title="在详情面板检查（Inspect）">
                      检查
                    </button>
                  {/if}
                </div>
                {#if expandedEntries.has(e.uid)}
                  <div class="hns-traj-detail">
                    <div class="hns-traj-field">
                      <div class="hns-traj-field-head">
                        <span>参数</span>
                        <button class="hns-traj-copy" onclick={() => copyText(prettyText(e.args))}>
                          {#if copiedText === e.args.slice(0, 20)}<CheckIcon class="size-3" />已复制{:else}<CopyIcon class="size-3" />复制{/if}
                        </button>
                      </div>
                      <pre class="hns-traj-pre">{prettyText(e.args)}</pre>
                    </div>
                    <div class="hns-traj-field">
                      <div class="hns-traj-field-head">
                        <span>结果{#if !e.ok}（失败）{/if}</span>
                        <button class="hns-traj-copy" onclick={() => copyText(e.result)}>复制</button>
                      </div>
                      <pre class="hns-traj-pre">{truncate(e.result, 4000)}</pre>
                    </div>
                  </div>
                {/if}
              {:else}
                <div class="hns-traj-row hns-traj-system">
                  <span class="hns-traj-ico"><Settings2Icon class="size-3.5" /></span>
                  <span class="hns-traj-time" title={e.time}>{fmtTime(e.time)}</span>
                  <span class="hns-traj-label">{e.event}</span>
                  <span class="hns-traj-summary">{truncate(e.summary, 160)}</span>
                  {#if e.detail}
                    <button class="hns-traj-more" onclick={() => toggleEntry(e.uid)}>
                      {expandedEntries.has(e.uid) ? "收起" : "详情"}
                    </button>
                  {/if}
                </div>
                {#if e.detail && expandedEntries.has(e.uid)}
                  <div class="hns-traj-detail">
                    <pre class="hns-traj-pre">{truncate(e.detail, 4000)}</pre>
                  </div>
                {/if}
              {/if}
              {#if e.kind === "assistant" && onOpenPath}
                <!-- 产物文件行扩展点：由 HarnessTab 在对话视图渲染；此处保留 onOpenPath 接入位 -->
              {/if}
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>
  {/if}
</div>

<style>
  .hns-trajectory {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .hns-traj-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--hns-border-light, rgba(128, 128, 128, .18));
    flex: none;
    flex-wrap: wrap;
  }
  .hns-traj-title {
    font-weight: 600;
    font-size: 13px;
    color: var(--hns-text, inherit);
  }
  .hns-traj-count {
    font-size: 11px;
    color: var(--hns-muted, #888);
    font-variant-numeric: tabular-nums;
  }
  .hns-traj-search {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 1px solid var(--hns-border-light, rgba(128, 128, 128, .25));
    border-radius: 8px;
    padding: 3px 8px;
    color: var(--hns-muted, #888);
    flex: 1;
    max-width: 240px;
  }
  .hns-traj-search input {
    border: 0;
    outline: 0;
    background: transparent;
    font-size: 12px;
    color: var(--hns-text, inherit);
    width: 100%;
  }
  .hns-traj-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11.5px;
    border: 1px solid var(--hns-border-light, rgba(128, 128, 128, .25));
    border-radius: 8px;
    padding: 3px 9px;
    background: transparent;
    color: var(--hns-muted, #888);
    cursor: pointer;
  }
  .hns-traj-btn:hover { color: var(--hns-text, inherit); border-color: var(--hns-accent, #4176e6); }
  .hns-traj-btn.on { color: var(--hns-accent, #4176e6); border-color: var(--hns-accent, #4176e6); }
  .hns-traj-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 10px 14px 20px;
  }
  .hns-traj-empty { color: var(--hns-muted, #888); font-size: 12px; text-align: center; padding: 40px 0; }
  .hns-traj-turn { margin-bottom: 10px; }
  .hns-traj-turn-head {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    background: transparent;
    border: 0;
    cursor: pointer;
    color: var(--hns-muted, #888);
    font-size: 12px;
    padding: 4px 2px;
  }
  .hns-traj-turn-head:hover { color: var(--hns-text, inherit); }
  .hns-traj-turn-name { font-weight: 600; color: var(--hns-text, inherit); }
  .hns-traj-turn-meta { font-size: 11px; }
  .hns-traj-rows { border-left: 2px solid var(--hns-border-light, rgba(128, 128, 128, .2)); margin-left: 7px; padding-left: 10px; }
  .hns-traj-row {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 3px 0;
    font-size: 12px;
    line-height: 1.5;
  }
  .hns-traj-ico {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 6px;
    background: rgba(128, 128, 128, .1);
    color: var(--hns-muted, #888);
  }
  .hns-traj-user .hns-traj-ico { color: var(--hns-accent, #4176e6); }
  .hns-traj-assistant .hns-traj-ico { color: #2ea043; }
  .hns-traj-tool .hns-traj-ico { color: #b08800; }
  .hns-traj-tool.err .hns-traj-ico { color: #d73a49; }
  .hns-traj-system .hns-traj-ico { color: #6e40c9; }
  .hns-traj-time {
    flex: none;
    font-size: 10.5px;
    color: var(--hns-muted, #888);
    font-variant-numeric: tabular-nums;
  }
  .hns-traj-label {
    flex: none;
    font-size: 10.5px;
    font-weight: 600;
    color: var(--hns-muted, #888);
    border: 1px solid var(--hns-border-light, rgba(128, 128, 128, .25));
    border-radius: 4px;
    padding: 0 5px;
  }
  .hns-traj-toolname {
    flex: none;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 11.5px;
    color: var(--hns-text, inherit);
    background: rgba(128, 128, 128, .08);
    border-radius: 4px;
    padding: 0 5px;
  }
  .hns-traj-summary {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--hns-muted, #888);
  }
  .hns-traj-dur { font-variant-numeric: tabular-nums; }
  .hns-traj-meta { flex: none; font-size: 10.5px; color: var(--hns-muted, #888); font-variant-numeric: tabular-nums; }
  .hns-traj-more {
    flex: none;
    font-size: 11px;
    color: var(--hns-accent, #4176e6);
    background: transparent;
    border: 0;
    cursor: pointer;
    padding: 0 2px;
  }
  .hns-traj-detail {
    margin: 2px 0 6px 27px;
    border: 1px solid var(--hns-border-light, rgba(128, 128, 128, .22));
    border-radius: 8px;
    padding: 8px 10px;
    background: rgba(128, 128, 128, .05);
  }
  .hns-traj-field { margin-bottom: 6px; }
  .hns-traj-field:last-child { margin-bottom: 0; }
  .hns-traj-field-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11px;
    color: var(--hns-muted, #888);
    margin-bottom: 3px;
  }
  .hns-traj-copy {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 10.5px;
    color: var(--hns-muted, #888);
    background: transparent;
    border: 0;
    cursor: pointer;
  }
  .hns-traj-copy:hover { color: var(--hns-text, inherit); }
  .hns-traj-inspect {
    flex: none;
    font-size: 10px;
    color: var(--hns-accent, #4176e6);
    background: transparent;
    border: 1px solid color-mix(in srgb, var(--hns-accent, #4176e6) 35%, transparent);
    border-radius: 5px;
    padding: 1px 7px;
    cursor: pointer;
    margin-right: 6px;
  }
  .hns-traj-inspect:hover { background: color-mix(in srgb, var(--hns-accent, #4176e6) 10%, transparent); }
  .hns-traj-pre {
    margin: 0;
    max-height: 260px;
    overflow: auto;
    font-size: 11px;
    font-family: ui-monospace, Consolas, monospace;
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--hns-text, inherit);
  }
  /* ─── 表格视图（DSH TrajectoryTable：固定行高 + 窗口化虚拟滚动） ─── */
  .hns-traj-table {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    position: relative;
    font-size: 11.5px;
  }
  .hns-traj-table-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 14px;
    border-bottom: 1px solid var(--hns-border-light, rgba(128, 128, 128, .18));
    position: sticky;
    top: 0;
    background: var(--hns-card, #fff);
    z-index: 2;
    color: var(--hns-muted, #888);
    font-size: 10.5px;
    font-weight: 600;
  }
  .hns-traj-table-spacer { position: relative; }
  .hns-traj-trow {
    position: absolute;
    left: 0; right: 0;
    height: 40px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 14px;
    border-bottom: 1px solid var(--hns-border-light, rgba(128, 128, 128, .1));
    overflow: hidden;
  }
  .hns-traj-trow.turnrow { background: rgba(128, 128, 128, .06); font-weight: 600; }
  .hns-tcol-type {
    flex: none;
    width: 120px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--hns-muted, #888);
  }
  .hns-tcol-type > :global(svg) { flex: none; }
  .hns-trow-type { font-size: 10px; font-weight: 700; letter-spacing: .4px; }
  .hns-trow-turnname { color: var(--hns-text, inherit); }
  .hns-tcol-time {
    flex: none;
    width: 76px;
    color: var(--hns-muted, #888);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
  }
  .hns-tcol-dur {
    flex: none;
    width: 64px;
    color: var(--hns-muted, #888);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
  }
  .hns-tcol-sum {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--hns-text, inherit);
  }
  .hns-trow-meta { color: var(--hns-muted, #888); font-size: 10.5px; }
  .hns-trow-toolname { color: #b08800; font-family: ui-monospace, Consolas, monospace; margin-right: 6px; }
  .hns-trow-err { color: #d73a49; margin-left: 6px; }
  .hns-trow-sys { color: #6e40c9; }
</style>
