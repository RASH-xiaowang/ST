<script lang="ts">
  import { errText } from '../format';
  import { onMount, onDestroy, type Component } from "svelte";
  import { getWechatSearchIndexStatus, queryEvents } from "./services/ipc";
  import { buildWechatSearchIndex, searchWechatMessages, getContacts } from "../wechat/services/ipc";
  import { excerpt, highlight } from "./searchText";
  import type { ContactHit, SearchEvent, WechatSearchHit } from "./types";
  import { kbApi } from "../kb/services/ipc";
  import { Button } from "../components/ui/button";
  import { RippleButton } from "fancy-ui-svelte";
  import { Input } from "../components/ui/input";
  import { Badge } from "../components/ui/badge";
  import { Skeleton } from "../components/ui/skeleton";
  import { NativeSelect, NativeSelectOption } from "../components/ui/native-select";
  import { Tabs, TabsList, TabsTrigger, TabsContent } from "../components/ui/tabs";
  import { kbUser, refreshKbUser } from "../kb/auth.svelte";
  import type { KbSummary, RetrievedChunk } from "../kb/kbTypes";
  import SearchIcon from "@lucide/svelte/icons/search";
  import MessagesSquareIcon from "@lucide/svelte/icons/messages-square";
  import DatabaseIcon from "@lucide/svelte/icons/database";
  import UsersIcon from "@lucide/svelte/icons/users";
  import ActivityIcon from "@lucide/svelte/icons/activity";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import ZapIcon from "@lucide/svelte/icons/zap";
  import XIcon from "@lucide/svelte/icons/x";

  let { onNavigate, onClose }: { onNavigate?: (tab: string) => void; onClose?: () => void } = $props();

  // ─── 状态 ───
  type Scope = "all" | "wechat" | "kb" | "contacts" | "events";
  const SCOPES: Array<{ id: Scope; label: string; icon: Component }> = [
    { id: "all", label: "全部", icon: SearchIcon },
    { id: "wechat", label: "微信消息", icon: MessagesSquareIcon },
    { id: "kb", label: "知识库", icon: DatabaseIcon },
    { id: "contacts", label: "通讯录", icon: UsersIcon },
    { id: "events", label: "平台事件", icon: ActivityIcon },
  ];

  let scope = $state<Scope>("all");
  let query = $state("");
  let searchInput = $state<HTMLInputElement | null>(null);
  let searching = $state(false);
  let error = $state("");
  let searched = $state(false);
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  // 微信消息
  let wxHits = $state<WechatSearchHit[]>([]);
  let wxLoading = $state(false);
  let wxError = $state("");
  let wxIndex = $state<{ exists: boolean; rows: number; built_at: string | null } | null>(null);
  let wxIndexBuilding = $state(false);
  let wxSearched = $state(false);

  // 知识库
  let kbList = $state<KbSummary[]>([]);
  let kbId = $state<number>(0); // 0 = 全部可见知识库
  let kbHits = $state<RetrievedChunk[]>([]);
  let kbLoading = $state(false);
  let kbError = $state("");
  let kbSearched = $state(false);

  // 通讯录
  let contacts = $state<ContactHit[]>([]);
  let contactHits = $state<ContactHit[]>([]);
  let contactsLoading = $state(false);
  let contactsError = $state("");

  // 平台事件
  let events = $state<SearchEvent[]>([]);
  let eventHits = $state<SearchEvent[]>([]);
  let eventsLoading = $state(false);
  let eventsError = $state("");

  const q = $derived(query.trim());
  const totalHits = $derived(wxHits.length + kbHits.length + contactHits.length + eventHits.length);

  // ─── 微信搜索 ───
  async function loadWxIndex() {
    try {
    wxIndex = await getWechatSearchIndexStatus();
    } catch {
      wxIndex = null;
    }
  }

  async function buildWxIndex() {
    wxIndexBuilding = true;
    wxError = "";
    try {
    const r = await buildWechatSearchIndex(true);
      wxIndex = { exists: true, rows: r?.rows ?? 0, built_at: r?.built_at ?? null };
      wxError = "";
    } catch (e: unknown) {
      wxError = `索引构建失败：${errText(e)}`;
    } finally {
      wxIndexBuilding = false;
    }
  }

  async function searchWechat(kw: string) {
    if (!kw) {
      wxHits = [];
      wxSearched = false;
      return;
    }
    wxLoading = true;
    wxError = "";
    wxSearched = true;
    try {
    const r = await searchWechatMessages({ query: kw, limit: 120 });
      wxHits = r?.hits ?? [];
      if (r?.indexed === false && wxHits.length === 0) {
        wxError = "搜索索引尚未构建，已使用全表扫描（数据量大时较慢）。可点击「构建索引」加速。";
      }
    } catch (e: unknown) {
      wxError = errText(e);
      wxHits = [];
    } finally {
      wxLoading = false;
    }
  }

  // ─── 知识库搜索 ───
  async function ensureKbSession() {
    await refreshKbUser();
    if (!kbUser.user) {
      try {
    await kbApi.login();
        await refreshKbUser();
      } catch {
        /* 单机默认 admin 免密，失败则留给面板内提示 */
      }
    }
    return !!kbUser.user;
  }

  async function loadKbList() {
    try {
    kbList = await kbApi.list(kbUser.user?.id ?? 1);
    } catch (e: unknown) {
      kbError = `知识库加载失败：${errText(e)}`;
    }
  }

  async function searchKb(kw: string) {
    if (!kw) {
      kbHits = [];
      kbSearched = false;
      return;
    }
    if (!kbUser.user) {
      kbError = "知识库未登录，请先打开「知识库管理」面板，或重试。";
      kbHits = [];
      return;
    }
    kbLoading = true;
    kbError = "";
    kbSearched = true;
    try {
    const res = await kbApi.search({
        input: {
          userId: kbUser.user.id,
          kbId: kbId === 0 ? null : kbId,
          query: kw,
          topK: 12,
          mode: "hybrid",
          providerId: null,
          model: null,
        },
      });
      kbHits = res ?? [];
    } catch (e: unknown) {
      kbError = errText(e);
      kbHits = [];
    } finally {
      kbLoading = false;
    }
  }

  // ─── 通讯录 ───
  async function loadContacts() {
    if (contacts.length > 0 || contactsLoading) return;
    contactsLoading = true;
    contactsError = "";
    try {
    const r = await getContacts();
      contacts = r?.contacts ?? [];
    } catch (e: unknown) {
      contactsError = errText(e);
    } finally {
      contactsLoading = false;
    }
  }

  function filterContacts(kw: string) {
    if (!kw) {
      contactHits = [];
      return;
    }
    const low = kw.toLowerCase();
    contactHits = contacts.filter((c) =>
      [c.display_name, c.nick_name, c.remark, c.alias, c.username, c.description]
        .filter(Boolean)
        .some((v) => String(v).toLowerCase().includes(low)),
    ).slice(0, 100);
  }

  // ─── 平台事件 ───
  async function loadEvents() {
    if (events.length > 0 || eventsLoading) return;
    eventsLoading = true;
    eventsError = "";
    try {
    events = await queryEvents(500, 0);
    } catch (e: unknown) {
      eventsError = errText(e);
    } finally {
      eventsLoading = false;
    }
  }

  function filterEvents(kw: string) {
    if (!kw) {
      eventHits = [];
      return;
    }
    const low = kw.toLowerCase();
    eventHits = events.filter((ev) =>
      [ev.event_type, ev.source, ev.title, ev.detail, ev.level]
        .filter(Boolean)
        .some((v) => String(v).toLowerCase().includes(low)),
    ).slice(0, 100);
  }

  // ─── 执行搜索 ───
  async function doSearch() {
    const kw = q;
    error = "";
    if (!kw) {
      wxHits = []; kbHits = []; contactHits = []; eventHits = [];
      searched = false;
      return;
    }
    searched = true;
    searching = true;
    await Promise.all([
      searchWechat(kw),
      searchKb(kw),
      loadContacts().then(() => filterContacts(kw)),
      loadEvents().then(() => filterEvents(kw)),
    ]);
    searching = false;
  }

  function onInput() {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      doSearch();
    }, 350);
  }

  function clearSearch() {
    query = "";
    wxHits = []; kbHits = []; contactHits = []; eventHits = [];
    searched = false;
    error = "";
    searchInput?.focus();
  }

  onMount(async () => {
    loadWxIndex();
    if (await ensureKbSession()) {
      await loadKbList();
    }
    // 弹窗打开后自动聚焦搜索框
    requestAnimationFrame(() => {
      searchInput?.focus();
      searchInput?.select();
    });
  });

  onDestroy(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
  });

  const wxCount = $derived(wxHits.length);
  const kbCount = $derived(kbHits.length);
  const contactCount = $derived(contactHits.length);
  const eventCount = $derived(eventHits.length);

  const scopeLoading = $derived(
    scope === "all"
      ? searching
      : scope === "wechat"
        ? wxLoading
        : scope === "kb"
          ? kbLoading
          : scope === "contacts"
            ? contactsLoading
            : eventsLoading,
  );
</script>

<div class="gs-root">
  <header class="gs-head">
    <div class="flex items-center gap-3">
      <span class="gs-head-ico"><SearchIcon class="size-4.5" /></span>
      <span class="text-base font-bold">全局搜索</span>
      <Badge variant="secondary" class="h-6 gap-1.5 px-2.5 text-xs">
        {#if searched} {totalHits} 条结果 {:else} 跨模块检索 {/if}
      </Badge>
    </div>
    <div class="flex items-center gap-2 text-xs text-muted-foreground">
      <kbd class="gs-kbd">Ctrl</kbd><span>+</span><kbd class="gs-kbd">K</kbd>
      <span class="text-muted-foreground/60">聚焦搜索</span>
      <span class="gs-close-sep"></span>
      <button class="gs-close" onclick={onClose} title="关闭 (Esc)"><XIcon class="size-4" /></button>
    </div>
  </header>

  <div class="gs-searchbar">
    <div class="gs-search-input-wrap">
      <SearchIcon class="gs-search-ico" />
      <Input
        bind:ref={searchInput}
        bind:value={query}
        oninput={onInput}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            doSearch();
          }
          if (e.key === "Escape") {
            if (query.trim()) {
              // 已输入内容时先清空，再按一次关闭
              clearSearch();
              e.stopPropagation();
            } else {
              onClose?.();
            }
          }
        }}
        class="gs-search-input"
        placeholder="搜索微信消息、知识库、通讯录与平台事件…"
        aria-label="全局搜索关键词"
      />
      {#if query}
        <button class="gs-clear-btn" onclick={clearSearch} title="清空 (Esc)"><XIcon class="size-3.5" /></button>
      {/if}
      {#if searching}
        <RefreshCwIcon class="gs-spin gs-search-ico" />
      {/if}
    </div>
    <RippleButton
      onclick={doSearch}
      disabled={searching || !q}
      rippleColor="#a5f3fc"
      class="h-8 rounded-md border-0 bg-[var(--primary)] px-3.5 text-xs font-medium text-[var(--primary-foreground)] hover:opacity-90"
    >
      <SearchIcon class="size-3.5" />
      搜索
    </RippleButton>
  </div>

  {#if error}<div class="gs-error">{error}</div>{/if}

  <Tabs bind:value={scope} class="gs-tabs">
    <TabsList>
      {#each SCOPES as s}
        <TabsTrigger value={s.id} class="gap-1.5">
          <s.icon class="size-3.5" />
          {s.label}
          {#if searched}
            {#if s.id === "all"}<span class="gs-count">{totalHits}</span>
            {:else if s.id === "wechat"}<span class="gs-count">{wxCount}</span>
            {:else if s.id === "kb"}<span class="gs-count">{kbCount}</span>
            {:else if s.id === "contacts"}<span class="gs-count">{contactCount}</span>
            {:else}<span class="gs-count">{eventCount}</span>
            {/if}
          {/if}
        </TabsTrigger>
      {/each}
    </TabsList>

    <TabsContent value="all" class="gs-content">
      {#if !searched}
        <div class="gs-empty">
          <SearchIcon class="gs-empty-ico" />
          <p>输入关键词，一次检索微信消息、知识库、通讯录与平台事件。</p>
          <div class="gs-suggest">
            <button class="gs-suggest-item" onclick={() => { query = "合同"; onInput(); }}>合同</button>
            <button class="gs-suggest-item" onclick={() => { query = "周报"; onInput(); }}>周报</button>
            <button class="gs-suggest-item" onclick={() => { query = "转账"; onInput(); }}>转账</button>
            <button class="gs-suggest-item" onclick={() => { query = "发票"; onInput(); }}>发票</button>
          </div>
        </div>
      {:else if scopeLoading}
        <div class="gs-skeletons">
          {#each [0, 1, 2, 3] as _}
            <Skeleton class="gs-skeleton" />
          {/each}
        </div>
      {:else if totalHits === 0}
        <div class="gs-no-result">未找到与「{query}」相关的结果</div>
      {:else}
        {#if wxCount > 0}
          {@render SectionTitle(MessagesSquareIcon, "微信消息", wxCount, () => (scope = "wechat"))}
          <div class="gs-results">
            {#each wxHits.slice(0, 6) as hit, i (i)}
              <button class="gs-hit" onclick={() => onNavigate?.("wechat")}>
                <span class="gs-avatar gs-avatar-wx">{hit.name?.slice(0, 1) || "微"}</span>
                <span class="gs-hit-body">
                  <span class="gs-hit-top">
                    <span class="gs-hit-title">{hit.name || hit.username}</span>
                    <span class="gs-hit-time">{hit.time}</span>
                  </span>
                  <span class="gs-hit-text" aria-label={hit.text}>{@html highlight(excerpt(hit.text ?? '', q, 150), q)}</span>
                </span>
              </button>
            {/each}
          </div>
        {/if}
        {#if kbCount > 0}
          {@render SectionTitle(DatabaseIcon, "知识库", kbCount, () => (scope = "kb"))}
          <div class="gs-results">
            {#each kbHits.slice(0, 6) as hit, i (i)}
              <button class="gs-hit" onclick={() => onNavigate?.("kb")}>
                <span class="gs-avatar gs-avatar-kb"><DatabaseIcon class="size-4" /></span>
                <span class="gs-hit-body">
                  <span class="gs-hit-top">
                    <span class="gs-hit-title">{hit.doc_title}</span>
                    <span class="gs-hit-meta">
                      {#if hit.section}<span class="gs-tag">{hit.section}</span>{/if}
                      {#if hit.page_no}<span class="gs-tag">第 {hit.page_no} 页</span>{/if}
                      <span class="gs-score">{(hit.score * 100).toFixed(1)}%</span>
                    </span>
                  </span>
                  <span class="gs-hit-text" aria-label={hit.content}>{@html highlight(excerpt(hit.content, q, 150), q)}</span>
                </span>
              </button>
            {/each}
          </div>
        {/if}
        {#if contactCount > 0}
          {@render SectionTitle(UsersIcon, "通讯录", contactCount, () => (scope = "contacts"))}
          <div class="gs-results">
            {#each contactHits.slice(0, 6) as c, i (i)}
              <button class="gs-hit" onclick={() => onNavigate?.("wechat")}>
                <span class="gs-avatar gs-avatar-c">{c.display_name?.slice(0, 1) || c.nick_name?.slice(0, 1) || "友"}</span>
                <span class="gs-hit-body">
                  <span class="gs-hit-top">
                    <span class="gs-hit-title">{c.display_name || c.nick_name || c.username}</span>
                    <span class="gs-hit-meta">
                      <span class="gs-tag">{c.local_type_label}</span>
                      {#if c.alias}<span class="gs-tag">{c.alias}</span>{/if}
                    </span>
                  </span>
                  {#if c.description}<span class="gs-hit-text">{c.description}</span>{/if}
                </span>
              </button>
            {/each}
          </div>
        {/if}
        {#if eventCount > 0}
          {@render SectionTitle(ActivityIcon, "平台事件", eventCount, () => (scope = "events"))}
          <div class="gs-results">
            {#each eventHits.slice(0, 6) as ev, i (i)}
              <button class="gs-hit" onclick={() => onNavigate?.("monitor")}>
                <span class="gs-avatar gs-avatar-ev"><ActivityIcon class="size-4" /></span>
                <span class="gs-hit-body">
                  <span class="gs-hit-top">
                    <span class="gs-hit-title">{ev.title || ev.event_type}</span>
                    <span class="gs-hit-meta">
                      <span class="gs-tag">{ev.event_type}</span>
                      <span class="gs-tag">{ev.source}</span>
                      <span class="gs-hit-time">{ev.timestamp}</span>
                    </span>
                  </span>
                  {#if ev.detail}<span class="gs-hit-text">{ev.detail}</span>{/if}
                </span>
              </button>
            {/each}
          </div>
        {/if}
      {/if}
    </TabsContent>

    <TabsContent value="wechat" class="gs-content">
      <div class="gs-scope-toolbar">
        <div class="gs-toolbar-info">
          <MessagesSquareIcon class="size-4 text-muted-foreground" />
          <span>全文检索已解密微信消息</span>
          {#if wxIndex}
            <Badge variant="outline" class="h-5 gap-1 px-2 text-xs">
              {wxIndex.exists && wxIndex.rows > 0
                ? `索引 ${wxIndex.rows.toLocaleString()} 条${wxIndex.built_at ? ` · ${wxIndex.built_at}` : ""}`
                : "索引未构建（搜索将回退全表扫描）"}
            </Badge>
          {/if}
        </div>
        <Button size="sm" variant="outline" onclick={buildWxIndex} disabled={wxIndexBuilding}>
          <ZapIcon class="size-3.5" />
          {wxIndexBuilding ? "构建中…" : wxIndex?.exists ? "重建索引" : "构建索引"}
        </Button>
      </div>
      {#if wxError}<div class="gs-warn">{wxError}</div>{/if}
      {#if wxLoading}
        <div class="gs-skeletons">{#each [0, 1, 2, 3] as _}<Skeleton class="gs-skeleton" />{/each}</div>
      {:else if !wxSearched}
        <div class="gs-empty"><MessagesSquareIcon class="gs-empty-ico" /><p>输入关键词搜索微信消息全文</p></div>
      {:else if wxHits.length === 0}
        <div class="gs-no-result">未找到相关微信消息</div>
      {:else}
        <div class="gs-results">
          {#each wxHits as hit, i (i)}
            <button class="gs-hit" onclick={() => onNavigate?.("wechat")}>
              <span class="gs-avatar gs-avatar-wx">{hit.name?.slice(0, 1) || "微"}</span>
              <span class="gs-hit-body">
                <span class="gs-hit-top">
                  <span class="gs-hit-title">{hit.name || hit.username}</span>
                  <span class="gs-hit-time">{hit.time}</span>
                </span>
                <span class="gs-hit-text" aria-label={hit.text}>{@html highlight(excerpt(hit.text ?? '', q, 200), q)}</span>
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </TabsContent>

    <TabsContent value="kb" class="gs-content">
      <div class="gs-scope-toolbar">
        <div class="gs-toolbar-info">
          <DatabaseIcon class="size-4 text-muted-foreground" />
          <span>混合检索（向量 + BM25）</span>
          <NativeSelect class="max-w-[220px]" bind:value={kbId} onchange={() => doSearch()}>
            <NativeSelectOption value={0}>全部可见知识库</NativeSelectOption>
            {#each kbList as kb (kb.id)}
              <NativeSelectOption value={kb.id}>{kb.name}{kb.isSystem ? "（系统）" : ""}</NativeSelectOption>
            {/each}
          </NativeSelect>
        </div>
        <Button size="sm" variant="outline" onclick={() => { loadKbList(); doSearch(); }}>
          <RefreshCwIcon class="size-3.5" />
          刷新
        </Button>
      </div>
      {#if kbError}<div class="gs-warn">{kbError}</div>{/if}
      {#if kbLoading}
        <div class="gs-skeletons">{#each [0, 1, 2, 3] as _}<Skeleton class="gs-skeleton" />{/each}</div>
      {:else if !kbSearched}
        <div class="gs-empty"><DatabaseIcon class="gs-empty-ico" /><p>输入关键词检索知识库文档内容</p></div>
      {:else if kbHits.length === 0}
        <div class="gs-no-result">未找到相关知识片段</div>
      {:else}
        <div class="gs-results">
          {#each kbHits as hit, i (i)}
            <button class="gs-hit" onclick={() => onNavigate?.("kb")}>
              <span class="gs-avatar gs-avatar-kb"><DatabaseIcon class="size-4" /></span>
              <span class="gs-hit-body">
                <span class="gs-hit-top">
                  <span class="gs-hit-title">{hit.doc_title}</span>
                  <span class="gs-hit-meta">
                    {#if hit.section}<span class="gs-tag">{hit.section}</span>{/if}
                    {#if hit.page_no}<span class="gs-tag">第 {hit.page_no} 页</span>{/if}
                    <span class="gs-score">{(hit.score * 100).toFixed(1)}%</span>
                  </span>
                </span>
                <span class="gs-hit-text" aria-label={hit.content}>{@html highlight(excerpt(hit.content, q, 220), q)}</span>
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </TabsContent>

    <TabsContent value="contacts" class="gs-content">
      <div class="gs-scope-toolbar">
        <div class="gs-toolbar-info">
          <UsersIcon class="size-4 text-muted-foreground" />
          <span>按显示名 / 昵称 / 备注 / 微信号 / 描述过滤</span>
          {#if contacts.length > 0}<Badge variant="outline" class="h-5 px-2 text-xs">{contacts.length} 位联系人</Badge>{/if}
        </div>
        <Button size="sm" variant="outline" onclick={() => { contacts = []; loadContacts(); filterContacts(q); }}>
          <RefreshCwIcon class="size-3.5" />
          刷新
        </Button>
      </div>
      {#if contactsError}<div class="gs-warn">{contactsError}</div>{/if}
      {#if contactsLoading}
        <div class="gs-skeletons">{#each [0, 1, 2, 3] as _}<Skeleton class="gs-skeleton" />{/each}</div>
      {:else if contactHits.length === 0}
        <div class="gs-no-result">{q ? `未找到与「${q}」匹配的联系人` : "输入关键词过滤通讯录"}</div>
      {:else}
        <div class="gs-results">
          {#each contactHits as c, i (i)}
            <button class="gs-hit" onclick={() => onNavigate?.("wechat")}>
              <span class="gs-avatar gs-avatar-c">{c.display_name?.slice(0, 1) || c.nick_name?.slice(0, 1) || "友"}</span>
              <span class="gs-hit-body">
                <span class="gs-hit-top">
                  <span class="gs-hit-title">{c.display_name || c.nick_name || c.username}</span>
                  <span class="gs-hit-meta">
                    <span class="gs-tag">{c.local_type_label}</span>
                    {#if c.alias}<span class="gs-tag">{c.alias}</span>{/if}
                    {#if c.username}<span class="gs-tag gs-mono">{c.username}</span>{/if}
                  </span>
                </span>
                {#if c.remark && c.remark !== c.display_name}<span class="gs-hit-text">备注：{c.remark}</span>{/if}
                {#if c.description}<span class="gs-hit-text">{c.description}</span>{/if}
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </TabsContent>

    <TabsContent value="events" class="gs-content">
      <div class="gs-scope-toolbar">
        <div class="gs-toolbar-info">
          <ActivityIcon class="size-4 text-muted-foreground" />
          <span>检索最近 500 条平台事件记录</span>
          {#if events.length > 0}<Badge variant="outline" class="h-5 px-2 text-xs">{events.length} 条</Badge>{/if}
        </div>
        <Button size="sm" variant="outline" onclick={() => { events = []; loadEvents(); filterEvents(q); }}>
          <RefreshCwIcon class="size-3.5" />
          刷新
        </Button>
      </div>
      {#if eventsError}<div class="gs-warn">{eventsError}</div>{/if}
      {#if eventsLoading}
        <div class="gs-skeletons">{#each [0, 1, 2, 3] as _}<Skeleton class="gs-skeleton" />{/each}</div>
      {:else if eventHits.length === 0}
        <div class="gs-no-result">{q ? `未找到与「${q}」匹配的事件` : "输入关键词过滤平台事件"}</div>
      {:else}
        <div class="gs-results">
          {#each eventHits as ev, i (i)}
            <button class="gs-hit" onclick={() => onNavigate?.("monitor")}>
              <span class="gs-avatar gs-avatar-ev"><ActivityIcon class="size-4" /></span>
              <span class="gs-hit-body">
                <span class="gs-hit-top">
                  <span class="gs-hit-title">{ev.title || ev.event_type}</span>
                  <span class="gs-hit-meta">
                    <span class="gs-tag">{ev.event_type}</span>
                    <span class="gs-tag">{ev.source}</span>
                    <span class="gs-hit-time">{ev.timestamp}</span>
                  </span>
                </span>
                {#if ev.detail}<span class="gs-hit-text">{ev.detail}</span>{/if}
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </TabsContent>
  </Tabs>
</div>

<!-- 分组标题 -->
{#snippet SectionTitle(icon: Component, title: string, count: number, onMore: () => void)}
  {@const Comp = icon}
  <div class="gs-section-title">
    <div class="flex items-center gap-2">
      <Comp class="size-4 text-primary" />
      <span class="text-sm font-semibold">{title}</span>
      <span class="text-xs text-muted-foreground">{count} 条</span>
    </div>
    <button class="gs-more" onclick={onMore}>查看全部 →</button>
  </div>
{/snippet}

<style>
  .gs-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }
  .gs-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .gs-head-ico {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 9px;
    background: linear-gradient(135deg, var(--primary), color-mix(in oklab, var(--primary) 60%, #0ea5e9));
    color: var(--primary-foreground);
    box-shadow: 0 3px 10px color-mix(in oklab, var(--primary) 30%, transparent), inset 0 1px 0 rgba(255, 255, 255, .14);
    flex: none;
  }
  .gs-kbd {
    padding: 1px 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: color-mix(in oklab, var(--card) 75%, white 6%);
    font-family: var(--font-mono);
    font-size: 11.5px;
  }
  .gs-close-sep {
    width: 1px;
    height: 16px;
    background: var(--border);
    margin: 0 4px;
  }
  .gs-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
    transition: background .15s, color .15s;
  }
  .gs-close:hover {
    background: color-mix(in oklab, var(--foreground) 10%, transparent);
    color: var(--foreground);
  }
  .gs-searchbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 20px 4px;
    flex: none;
  }
  .gs-search-input-wrap {
    position: relative;
    flex: 1;
  }
  :global(.gs-search-input) {
    height: 40px;
    padding-left: 38px;
    padding-right: 72px;
    font-size: 14px;
  }
  :global(.gs-search-ico) {
    position: absolute;
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    width: 16px;
    height: 16px;
    color: var(--muted-foreground);
    pointer-events: none;
  }
  :global(.gs-spin) {
    right: 12px;
    left: auto;
    animation: gs-rotate 0.9s linear infinite;
  }
  @keyframes gs-rotate {
    to { transform: translateY(-50%) rotate(360deg); }
  }
  .gs-clear-btn {
    position: absolute;
    right: 8px;
    top: 50%;
    transform: translateY(-50%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    color: var(--muted-foreground);
    background: transparent;
    cursor: pointer;
  }
  .gs-clear-btn:hover {
    background: color-mix(in oklab, var(--foreground) 10%, transparent);
    color: var(--foreground);
  }
  .gs-error {
    margin: 8px 20px 0;
    padding: 8px 12px;
    border-radius: 8px;
    border: 1px solid color-mix(in oklab, var(--destructive) 45%, transparent);
    background: color-mix(in oklab, var(--destructive) 10%, transparent);
    color: var(--destructive);
    font-size: 12.5px;
    flex: none;
  }
  :global(.gs-tabs) {
    padding: 10px 20px 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
  }
  :global(.gs-content) {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 0 20px;
  }
  .gs-count {
    display: inline-flex;
    align-items: center;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 9px;
    background: color-mix(in oklab, var(--primary) 18%, transparent);
    color: var(--primary);
    font-size: 11.5px;
    font-weight: 600;
  }
  .gs-scope-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 4px 2px 10px;
  }
  .gs-toolbar-info {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--muted-foreground);
    min-width: 0;
  }
  .gs-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 64px 20px;
    color: var(--muted-foreground);
    font-size: 13px;
    text-align: center;
  }
  :global(.gs-empty-ico) {
    width: 38px;
    height: 38px;
    color: color-mix(in oklab, var(--muted-foreground) 45%, transparent);
  }
  .gs-suggest {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }
  .gs-suggest-item {
    padding: 5px 12px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--card) 70%, black 12%);
    color: var(--foreground);
    font-size: 12.5px;
    cursor: pointer;
    transition: border-color .15s, color .15s;
  }
  .gs-suggest-item:hover {
    border-color: var(--primary);
    color: var(--primary);
  }
  .gs-no-result {
    padding: 56px 20px;
    text-align: center;
    color: var(--muted-foreground);
    font-size: 13px;
  }
  .gs-skeletons {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 4px 2px;
  }
  :global(.gs-skeleton) {
    height: 64px;
    border-radius: 10px;
  }
  .gs-section-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 4px 8px;
  }
  .gs-more {
    display: inline-flex; align-items: center; gap: 4px; white-space: nowrap;
    font-size: 12px;
    color: var(--primary);
    cursor: pointer;
    background: none;
    border: none;
    padding: 2px 4px;
  }
  .gs-more:hover {
    text-decoration: underline;
  }
  .gs-results {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .gs-hit {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    width: 100%;
    padding: 11px 14px;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--card) 60%, black 8%);
    text-align: left;
    cursor: pointer;
    transition: border-color .15s, background .15s;
  }
  .gs-hit:hover {
    border-color: color-mix(in oklab, var(--primary) 45%, transparent);
    background: color-mix(in oklab, var(--card) 72%, black 6%);
    box-shadow: 0 0 0 1px color-mix(in oklab, var(--primary) 12%, transparent), 0 10px 26px -20px color-mix(in oklab, var(--primary) 60%, transparent);
  }
  .gs-avatar {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border-radius: 9px;
    font-size: 14px;
    font-weight: 600;
    flex: none;
  }
  .gs-avatar-wx { background: color-mix(in oklab, #22c55e 18%, transparent); color: #4ade80; }
  .gs-avatar-kb { background: color-mix(in oklab, var(--primary) 16%, transparent); color: var(--primary); }
  .gs-avatar-c { background: color-mix(in oklab, #a78bfa 18%, transparent); color: #c4b5fd; }
  .gs-avatar-ev { background: color-mix(in oklab, #f59e0b 16%, transparent); color: #fbbf24; }
  .gs-hit-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }
  .gs-hit-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .gs-hit-title {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .gs-hit-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }
  .gs-hit-time {
    font-size: 11.5px;
    color: var(--muted-foreground);
    flex: none;
  }
  .gs-hit-text {
    font-size: 12.5px;
    line-height: 1.6;
    color: var(--muted-foreground);
    display: -webkit-box;
    line-clamp: 2;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-all;
  }
  .gs-tag {
    padding: 1px 7px;
    border-radius: 5px;
    background: color-mix(in oklab, var(--foreground) 8%, transparent);
    color: var(--muted-foreground);
    font-size: 11.5px;
    flex: none;
  }
  .gs-score {
    font-size: 11.5px;
    color: var(--primary);
    font-weight: 600;
    flex: none;
  }
  .gs-mono {
    font-family: var(--font-mono);
  }
  .gs-warn {
    margin: 0 2px 10px;
    padding: 8px 12px;
    border-radius: 8px;
    border: 1px solid color-mix(in oklab, #f59e0b 40%, transparent);
    background: color-mix(in oklab, #f59e0b 9%, transparent);
    color: #fbbf24;
    font-size: 12.5px;
  }
  :global(.gs-hit-text mark) {
    background: color-mix(in oklab, var(--primary) 28%, transparent);
    color: var(--primary-foreground);
    border-radius: 3px;
    padding: 0 1px;
  }
</style>

