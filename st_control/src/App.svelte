<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { AgentInfo } from './lib/communication/types';
  import {
    serverStatus,
    agents,
    eventLog,
    initEventListeners,
    refreshServerStatus,
  } from './lib/communication';
  import { WeChatPanel, WeChatBootstrap } from '@wechat';
  import LlmPanel from './lib/llm/LlmPanel.svelte';
  import { startLlmSync } from './lib/llm/store.svelte';
  import DataDashboard from './lib/DataDashboard.svelte';
  import DbManager from './lib/DbManager.svelte';
  import KnowledgeBase from './lib/kb/KnowledgeBase.svelte';
  import AgentPanel from './lib/agents/AgentPanel.svelte';
import PlatformOverview from './lib/components/PlatformOverview.svelte';
  import AgentDetailModal from './lib/components/AgentDetailModal.svelte';
  import ApiHelpModal from './lib/components/ApiHelpModal.svelte';
  import PanelSection from './lib/components/PanelSection.svelte';
  import SettingsModal from './lib/components/SettingsModal.svelte';
  import OcrPanel from './lib/ocr/OcrPanel.svelte';
  import AutomationPanel from './lib/automation/AutomationPanel.svelte';
  import GlobalSearch from './lib/search/GlobalSearch.svelte';
  import HarnessTab from './lib/harness/HarnessTab.svelte';
  import ThemeFlickeringGrid from './lib/components/fancy/ThemeFlickeringGrid.svelte';
  import Sonner from './lib/components/ui/sonner/sonner.svelte';

  // ---------- 通知 ----------
  let notifications = $state<Array<{ id: number; title: string; message: string; type: 'success' | 'warn' | 'error' }>>([]);
  let msgCount = $state(0);
  // 微信数据管理启动页门控：每次进入先过启动页（初始化检查）再进入主界面
  let wechatReady = $state(false);
  $effect(() => {
    if (activeTab === 'wechat') wechatReady = false;
  });
  function notify(t: string, m: string, type: 'success' | 'warn' | 'error') {
    const id = Date.now() + Math.random();
    notifications = [...notifications, { id, title: t, message: m, type }];
    setTimeout(() => { notifications = notifications.filter(n => n.id !== id); }, 4000);
  }

  // ---------- 弹窗 ----------
  let modalAgent = $state<AgentInfo | null>(null);
  let helpOpen = $state(false);
  let settingsOpen = $state(false);
  let searchOpen = $state(false);

  // ─── API 文档：运行时设置与调试数据 ───
  function openAgentDetail(agent: AgentInfo) {
    modalAgent = agent;
  }

  // ---------- 生命周期 ----------
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  onMount(() => {
    initEventListeners();
    // 大模型配置实时同步：监听后端变更广播，驱动所有使用模型的界面自动刷新
    startLlmSync();
    const unsub = eventLog.subscribe(logs => {
      if (!logs.length) return;
      const l = logs[0];
      if (l.event === 'agent_connected') notify('Agent 接入', l.detail, 'success');
      if (l.event === 'agent_disconnected') notify('Agent 断开', l.detail, 'warn');
    });
    // AI 角色面板点击「使用」后自动跳转到全局调用面板，实现跨模块一键调度
    const onRoleSelected = () => {
      activeTab = 'llm';
    };
    window.addEventListener('role-selected', onRoleSelected as EventListener);
    pollTimer = setInterval(refreshServerStatus, 3000);
    document.addEventListener('keydown', handleSidebarKeydown);
    return () => { window.removeEventListener('role-selected', onRoleSelected as EventListener); document.removeEventListener('keydown', handleSidebarKeydown); unsub(); if (pollTimer) clearInterval(pollTimer); };
  });

  onDestroy(() => { if (pollTimer) clearInterval(pollTimer); });

  // ---------- 面板切换 ----------
  let activeTab = $state<'monitor' | 'harness' | 'agents' | 'automation' | 'db_manager' | 'wechat' | 'llm' | 'kb' | 'ocr'>('monitor');
  // 首页双视图：落地页（默认）↔ 系统监控（数据看板并入首页）
  let homeView = $state<'overview' | 'sys'>('overview');
  // 概览卡/搜索跳转统一入口：'ai_chat'（原独立板块）已并入 Harness 会话；
  // 'ai_copy' / 'ai_roles' 已并入大模型面板；'bot' 已并入自动化面板；'data_dashboard' 已并入首页系统监控
  function navigateToTab(tab: string) {
    if (tab === 'ai_chat') {
      activeTab = 'harness';
    } else if (tab === 'ai_copy' || tab === 'ai_roles') {
      activeTab = 'llm';
    } else if (tab === 'bot') {
      activeTab = 'automation';
    } else if (tab === 'data_dashboard') {
      homeView = 'sys';
      activeTab = 'monitor';
    } else {
      activeTab = tab as typeof activeTab;
    }
  }
  let settingsTab = $state<'general' | 'server' | 'log' | 'personalize' | 'database'>('general');
  // 请求微信数据面板打开「设置」页（启动页「去配置」等入口）
  let wechatConfigTick = $state(0);

  /** 折叠 rail 模式：启动即自动折叠，悬浮展开、移开自动折叠 */
  let sidebarCollapsed = $state(true);
  let sidebarDrawerOpen = $state(false);
  /** 悬浮展开（折叠 rail 模式：鼠标移入临时展开、移出自动折叠） */
  let sidebarHover = $state(false);
  /** 实际展开：手动展开 或（手动折叠且悬浮中）；移动端抽屉打开时强制全宽，
   *  避免折叠宽度把抽屉压成窄条 */
  const sidebarExpanded = $derived(
    sidebarDrawerOpen ? true : sidebarCollapsed ? sidebarHover : true,
  );

  function toggleSidebar() {
    if (window.innerWidth <= 768) {
      sidebarDrawerOpen = !sidebarDrawerOpen;
    } else if (sidebarCollapsed) {
      // 悬浮展开中点击 = 固定展开（取消自动折叠）
      sidebarCollapsed = false;
      sidebarHover = false;
    } else {
      sidebarCollapsed = true;
    }
  }

  function closeSidebarDrawer() {
    sidebarDrawerOpen = false;
  }

  // ---------- 窗口控制 ----------
  const appWindow = getCurrentWindow();
  let isMaximized = $state(false);
  appWindow.isMaximized().then(m => isMaximized = m);

  function minimizeWindow() { appWindow.minimize(); }
  function toggleMaximize() {
    appWindow.toggleMaximize();
    isMaximized = !isMaximized;
  }
  function closeWindow() { appWindow.close(); }

  // ---------- 窗口拖拽 ----------
  function onTitlebarMouseDown(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (target.closest('.titlebar-actions')) return;
    appWindow.startDragging();
  }

  function handleSidebarKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'b') {
      e.preventDefault();
      toggleSidebar();
    }
    // Ctrl+K：知识库面板内聚焦知识库搜索，其余场景打开全局搜索弹窗
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      if (activeTab === 'kb') return;
      if (helpOpen || settingsOpen || searchOpen) return;
      e.preventDefault();
      searchOpen = true;
    }
    if (e.key === 'Escape' && sidebarDrawerOpen) {
      closeSidebarDrawer();
    }
  }


  // ---------- 视图 ----------
  let statusText = $derived.by(() => {
    const m: Record<string, string> = { running: '运行中', starting: '启动中', stopping: '停止中', error: '异常', stopped: '已停止' };
    return m[$serverStatus.status] || $serverStatus.status;
  });
  let statusCls = $derived.by(() => {
    const m: Record<string, string> = { running:'tag-success', starting:'tag-warn', stopping:'tag-warn', error:'tag-danger', stopped:'tag-default' };
    return m[$serverStatus.status] || 'tag-default';
  });
  let eventList = $derived($eventLog);
  let srvStatus = $derived($serverStatus);
  let agentList = $derived($agents);
</script>

<!--
THESIS: 这台控制台是一块个人仪表台——每块面板都是插在机架上的仪表，扫一眼读数、按一下操作；拒绝“深色控制台+霓虹辉光”的分类默认，也拒绝把内容区当装饰画布。
OWN-WORLD: 机台灰台面 + 骨白仪表面板 + 炭黑刻线墨 + 青蓝仅作“活体”指示灯与主操作；等宽读数、刻字式微标签、发丝级规则线，层级靠刻线与字重。
STORY: 使用者坐在这张工作台前，像操作一排精密仪表：状态靠指示灯，数据靠读数，操作靠那颗明确的主键，安静、可扫、可信。
FIRST VIEWPORT: 顶部机台灰标题栏带 LED 品牌块；首页为“工作台”：服务器状态带 + 四块仪表卡（在线/消息/端口/节点）+ 机架式快捷入口 + Agent/事件记录。
FORM: 仪表台 Bench Console，自建方向第4位；seed b9f46f82。
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, and DESIGN.md
-->
<div class="layout">
  <!-- ===== FancyUI 环境背景：微光网格（低透明度，衬托控制台氛围） ===== -->
  <div class="app-ambient" aria-hidden="true">
    <ThemeFlickeringGrid
      squareSize={5}
      gridGap={10}
      maxOpacity={0.05}
      flickerChance={0.12}
    />
  </div>

  <!-- ===== Agent 详情弹窗 ===== -->
  <AgentDetailModal agent={modalAgent} onClose={() => (modalAgent = null)} {notify} />

  <!-- ===== API 帮助弹窗 ===== -->
  <ApiHelpModal open={helpOpen} onClose={() => (helpOpen = false)} />

  <!-- ===== 全局搜索弹窗 ===== -->
  {#if searchOpen}
    <div
      class="modal-overlay"
      onclick={() => searchOpen = false}
      onkeydown={(e) => e.key === 'Escape' && (searchOpen = false)}
      role="dialog"
      tabindex="-1"
    >
      <div
        class="modal search-modal"
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => e.key === 'Escape' && (searchOpen = false)}
        role="dialog"
        aria-modal="true"
        tabindex="-1"
      >
        <GlobalSearch
          onClose={() => (searchOpen = false)}
          onNavigate={(tab) => {
            searchOpen = false;
            navigateToTab(tab);
          }}
        />
      </div>
    </div>
  {/if}

  <!-- ===== 设置弹窗 ===== -->
  <!-- ===== 设置弹窗 ===== -->
  <SettingsModal
    open={settingsOpen}
    tab={settingsTab}
    onTabChange={(t) => (settingsTab = t)}
    onClose={() => (settingsOpen = false)}
    statusText={statusText}
    statusCls={statusCls}
    serverPort={srvStatus.port}
    events={eventList}
  />
  <!-- ===== 通知容器 ===== -->
  <Sonner position="top-right" richColors />
  {#if notifications.length}
    <div class="toast-container">
      {#each notifications as n}
        <div class="toast toast-{n.type}" role="button" onclick={() => notifications = notifications.filter(x => x.id !== n.id)} onkeydown={(e) => e.key === 'Enter' && (notifications = notifications.filter(x => x.id !== n.id))} tabindex="0">
          <span class="toast-icon">
            {#if n.type === 'success'}
              <svg viewBox="0 0 16 16" width="12" height="12" fill="none" aria-hidden="true"><path d="M3 8.5 6.5 12 13 4.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>
            {:else if n.type === 'warn'}
              <svg viewBox="0 0 16 16" width="12" height="12" fill="none" aria-hidden="true"><path d="M8 3.5v5.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><circle cx="8" cy="12" r="0.9" fill="currentColor"/></svg>
            {:else}
              <svg viewBox="0 0 16 16" width="12" height="12" fill="none" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>
            {/if}
          </span>
          <div class="toast-body"><div class="toast-title">{n.title}</div><div class="toast-desc">{n.message}</div></div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- ===== 自定义标题栏（仪表台：LED 品牌块 + 刻线分隔） ===== -->
  <div class="titlebar" role="toolbar" tabindex="-1" data-tauri-drag-region onmousedown={onTitlebarMouseDown}>
    <div class="titlebar-left" data-tauri-drag-region>
      <div class="titlebar-brand" data-tauri-drag-region aria-hidden="true">
        <span class="titlebar-brand-led"></span>
        <div class="titlebar-brand-icon">ST</div>
      </div>
      <span class="titlebar-title">ST 控制台</span>
      <span class="titlebar-divider" aria-hidden="true"></span>
      <span class="titlebar-meta">本地控制台 · v1.0</span>
    </div>
    <div class="titlebar-actions">
      <button class="titlebar-btn" onclick={minimizeWindow} title="最小化">
        <svg viewBox="0 0 12 12" width="12" height="12"><rect y="5" width="12" height="1.5" fill="currentColor"/></svg>
      </button>
      <button class="titlebar-btn" onclick={toggleMaximize} title={isMaximized ? '还原' : '最大化'}>
        {#if isMaximized}
          <svg viewBox="0 0 12 12" width="12" height="12"><rect x="1" y="3" width="9" height="9" rx="1" fill="none" stroke="currentColor" stroke-width="1.2"/><rect x="3" y="1" width="9" height="9" rx="1" fill="none" stroke="currentColor" stroke-width="1.2"/></svg>
        {:else}
          <svg viewBox="0 0 12 12" width="12" height="12"><rect x="0.5" y="0.5" width="11" height="11" rx="1" fill="none" stroke="currentColor" stroke-width="1.2"/></svg>
        {/if}
      </button>
      <button class="titlebar-btn titlebar-btn-close" onclick={closeWindow} title="关闭">
        <svg viewBox="0 0 12 12" width="12" height="12"><line x1="1" y1="1" x2="11" y2="11" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><line x1="11" y1="1" x2="1" y2="11" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
      </button>
    </div>
  </div>

  <!-- ===== 中部区域：侧边导航栏 + 内容区 ===== -->
  <div class="body">
    <!-- 左侧导航栏（折叠为 rail：悬浮自动展开、移开自动折叠） -->
    <aside
      class="sidebar"
      class:sidebar-collapsed={!sidebarExpanded}
      class:sidebar-drawer={sidebarDrawerOpen}
      onmouseenter={() => (sidebarHover = true)}
      onmouseleave={() => (sidebarHover = false)}
    >
      <!-- 侧边栏头部：品牌区 -->
      <div class="sidebar-header">
        <div class="sidebar-brand">
          <div class="brand-icon-wrap">
            <span class="brand-icon-led"></span>
            <div class="brand-icon">ST</div>
          </div>
          <div class="brand-text">
            <h1 class="navbar-title">
              ST 控制台
            </h1>
            <div class="brand-meta">
              <span class="brand-version">v1.0</span>
              <span class="brand-edition">专业版</span>
            </div>
          </div>
        </div>
      </div>
      <nav class="nav-list">
        <!-- 全局搜索（弹窗）：快捷入口置顶，不设分区标题 -->
        <button class="nav-item nav-item-search" onclick={() => searchOpen = true} title="全局搜索 (Ctrl+K)">
          <svg class="nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none"><circle cx="6.5" cy="6.5" r="4" stroke="currentColor" stroke-width="1.5"/><line x1="9.5" y1="9.5" x2="13" y2="13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
          <span class="nav-text">搜索</span>
          <span class="nav-badge">Ctrl K</span>
        </button>

        <div class="nav-list-divider"></div>

        <div class="nav-section-label">概览</div>
        <button class="nav-item" class:active={activeTab === 'monitor'} onclick={() => activeTab = 'monitor'} role="tab" aria-selected={activeTab === 'monitor'} title="首页">
          <svg class="nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none">
            <path d="M2 7.5L8 2.5L14 7.5V13.5A.5.5 0 0 1 13.5 14H9.5V10H6.5V14H2.5A.5.5 0 0 1 2 13.5V7.5Z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/>
          </svg>
          <span class="nav-text">首页</span>
        </button>

        <div class="nav-section-label">AI 工作台</div>

        <button class="nav-item" class:active={activeTab === 'harness'} onclick={() => activeTab = 'harness'} role="tab" aria-selected={activeTab === 'harness'} title="Harness">
          <svg class="nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none"><circle cx="8" cy="8" r="2.2" stroke="currentColor" stroke-width="1.3"/><circle cx="8" cy="8" r="5.5" stroke="currentColor" stroke-width="1.3" stroke-dasharray="2.6 1.8"/><line x1="8" y1="1" x2="8" y2="2.6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/><line x1="8" y1="13.4" x2="8" y2="15" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/><line x1="1" y1="8" x2="2.6" y2="8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/><line x1="13.4" y1="8" x2="15" y2="8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg>
          <span class="nav-text">Harness</span>
        </button>

        <button class="nav-item" class:active={activeTab === 'agents'} onclick={() => activeTab = 'agents'} role="tab" aria-selected={activeTab === 'agents'} title="智能体">
          <svg class="nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none"><circle cx="8" cy="5" r="3" stroke="currentColor" stroke-width="1.3"/><path d="M2 14c0-3.3 2.7-6 6-6s6 2.7 6 6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg>
          <span class="nav-text">智能体</span>
          {#if agentList.length > 0}<span class="nav-badge">{agentList.length}</span>{/if}
        </button>

        <button class="nav-item" class:active={activeTab === 'llm'} onclick={() => activeTab = 'llm'} role="tab" aria-selected={activeTab === 'llm'} title="大模型">
          <svg class="nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none"><circle cx="8" cy="8" r="5.5" stroke="currentColor" stroke-width="1.3"/><circle cx="8" cy="8" r="2.5" stroke="currentColor" stroke-width="1.3"/><line x1="8" y1="2" x2="8" y2="4.5" stroke="currentColor" stroke-width="1.3"/><line x1="8" y1="11.5" x2="8" y2="14" stroke="currentColor" stroke-width="1.3"/><line x1="14" y1="8" x2="11.5" y2="8" stroke="currentColor" stroke-width="1.3"/><line x1="4.5" y1="8" x2="2" y2="8" stroke="currentColor" stroke-width="1.3"/></svg>
          <span class="nav-text">大模型</span>
        </button>

        <div class="nav-section-label">自动化</div>
        <button class="nav-item" class:active={activeTab === 'automation'} onclick={() => activeTab = 'automation'} role="tab" aria-selected={activeTab === 'automation'} title="自动化">
          <svg class="nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none"><circle cx="8" cy="4" r="2.5" stroke="currentColor" stroke-width="1.3"/><path d="M3 14.5c0-2.8 2.2-5 5-5s5 2.2 5 5" stroke="currentColor" stroke-width="1.3"/><line x1="11" y1="2" x2="14" y2="5" stroke="currentColor" stroke-width="1.3"/><line x1="14" y1="2" x2="11" y2="5" stroke="currentColor" stroke-width="1.3"/></svg>
          <span class="nav-text">自动化</span>
        </button>

        <div class="nav-section-label">数据与识别</div>
        <button class="nav-item" class:active={activeTab === 'wechat'} onclick={() => activeTab = 'wechat'} role="tab" aria-selected={activeTab === 'wechat'} title="微信数据">
          <svg class="nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none"><path d="M5.5 3C3 3 1 4.5 1 6.5c0 1.2.7 2.3 1.8 3L2 12l2.7-1.3c.5.2 1 .3 1.6.3M8 3c2.5 0 4.5 1.5 4.5 3.5S10.5 10 8 10" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/><circle cx="3.5" cy="6.5" r=".5" fill="currentColor"/><circle cx="5.5" cy="6.5" r=".5" fill="currentColor"/><circle cx="7.5" cy="6.5" r=".5" fill="currentColor"/><path d="M8 10c2.5 0 4.5-1.5 4.5-3.5 0-.5-.1-1-.3-1.5.9.5 1.8 1.3 1.8 2.5 0 1.2-.7 2.3-1.8 3L13 12l-2.1-1" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/></svg>
          <span class="nav-text">微信数据</span>
          {#if msgCount > 0}<span class="nav-badge">{msgCount}</span>{/if}
        </button>
        <button class="nav-item" class:active={activeTab === 'kb'} onclick={() => activeTab = 'kb'} role="tab" aria-selected={activeTab === 'kb'} title="知识库">
          <svg class="nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none"><path d="M2 3.5c0-1.1 2-2 6-2s6 .9 6 2-2 2-6 2-6-.9-6-2Z" stroke="currentColor" stroke-width="1.3"/><path d="M2 3.5v9c0 1.1 2 2 6 2s6-.9 6-2v-9" stroke="currentColor" stroke-width="1.3"/><line x1="2" y1="8" x2="14" y2="8" stroke="currentColor" stroke-width="1.3"/><path d="M5 6l1.2 1.2L5 8.4M10 6l-1.2 1.2L10 8.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/></svg>
          <span class="nav-text">知识库</span>
        </button>
        <button class="nav-item" class:active={activeTab === 'db_manager'} onclick={() => activeTab = 'db_manager'} role="tab" aria-selected={activeTab === 'db_manager'} title="数据库">
          <svg class="nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none"><ellipse cx="8" cy="3.5" rx="6" ry="2" stroke="currentColor" stroke-width="1.3"/><path d="M2 3.5v9c0 1.1 2.7 2 6 2s6-.9 6-2v-9" stroke="currentColor" stroke-width="1.3"/><line x1="2" y1="8" x2="14" y2="8" stroke="currentColor" stroke-width="1.3"/></svg>
          <span class="nav-text">数据库</span>
        </button>
        <button class="nav-item" class:active={activeTab === 'ocr'} onclick={() => activeTab = 'ocr'} role="tab" aria-selected={activeTab === 'ocr'} title="图文识别">
          <svg class="nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none"><rect x="1.5" y="1.5" width="13" height="13" rx="1.5" stroke="currentColor" stroke-width="1.3"/><circle cx="5.5" cy="5.5" r="1.5" fill="currentColor"/><path d="M2 11l3-3 2.5 2.5L10 8l4 4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/></svg>
          <span class="nav-text">图文识别</span>
        </button>
      </nav>

      <!-- 底部设置/状态 -->
      <div class="sidebar-spacer"></div>
      <div class="sidebar-footer">
        <!-- 状态行 -->
        <div class="footer-status-row {statusCls}">
          <span class="footer-status-dot"></span>
          <span class="footer-status-text" title={statusText}>{statusText}</span>
        </div>
        <div class="footer-divider"></div>
        <!-- 操作列表 -->
        <button class="footer-action" onclick={() => helpOpen = true} title="API 文档">
          <svg class="nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none">
            <path d="M2 2h12a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H2a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1z" stroke="currentColor" stroke-width="1.3"/>
            <line x1="4.5" y1="5" x2="11.5" y2="5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
            <line x1="4.5" y1="8" x2="11.5" y2="8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
            <line x1="4.5" y1="11" x2="8.5" y2="11" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
          </svg>
          <span class="footer-action-text">API 文档</span>
        </button>
        <button class="footer-action" onclick={() => settingsOpen = true} title="设置">
          <svg class="nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none">
            <circle cx="8" cy="8" r="2.5" stroke="currentColor" stroke-width="1.3"/>
            <path d="M8 1v2M8 13v2M1 8h2M13 8h2M2.5 2.5l1.5 1.5M12 12l1.5 1.5M2.5 13.5l1.5-1.5M12 4l1.5-1.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
          </svg>
          <span class="footer-action-text">设置</span>
        </button>
      </div>

      <!-- 折叠按钮栏：单条 chevron 用 CSS 旋转 180° 过渡，避免 {#if} 硬切两个图形 -->
      <div class="sidebar-collapse-bar">
        <button class="collapse-btn" onclick={toggleSidebar} title={sidebarExpanded ? '折叠侧边栏 (Ctrl+B)' : '展开侧边栏 (Ctrl+B)'} aria-label={sidebarExpanded ? '折叠侧边栏' : '展开侧边栏'}>
          <svg class="collapse-icon" viewBox="0 0 16 16" width="14" height="14" fill="none" aria-hidden="true">
            <path d="M6.2 4L10 8l-3.8 4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        </button>
      </div>
    </aside>

    <!-- 移动端抽屉遮罩 -->
    {#if sidebarDrawerOpen}
      <div class="sidebar-drawer-overlay" onclick={closeSidebarDrawer} onkeydown={(e) => e.key === 'Escape' && closeSidebarDrawer()} role="presentation" tabindex="-1"></div>
    {/if}

    <!-- 右侧内容区 -->
    <main class="content">
      <!-- 内容区背景：主题色闪烁网格（所有组件背景改为半透明，让网格透出作为背景） -->
      <div class="content-grid" aria-hidden="true">
        <ThemeFlickeringGrid
          squareSize={5}
          gridGap={9}
          maxOpacity={0.07}
          flickerChance={0.14}
        />
      </div>
      <!-- 所有面板同时渲染，仅隐藏非活跃的（保证后台任务不中断） -->
      <PanelSection active={activeTab === 'monitor'}>
        <!-- 首页：落地页（默认） ↔ 系统监控（原「数据看板」并入） -->
        {#if homeView === 'sys'}
          <div class="flex h-full min-h-0 flex-col">
            <div class="flex items-center gap-3 px-1 pt-1">
              <button
                class="rounded-md border border-[var(--border)] bg-[var(--card)] px-3 py-1.5 text-xs font-medium text-[var(--foreground)] transition hover:bg-[var(--muted)]"
                onclick={() => (homeView = 'overview')}
              >
                ← {statusText === '运行中' ? '返回首页' : 'Back'}
              </button>
              <span class="text-sm font-semibold">实时系统监控 · {statusText}</span>
            </div>
            <div class="min-h-0 flex-1 pt-2">
              <DataDashboard active={activeTab === 'monitor' && homeView === 'sys'} />
            </div>
          </div>
        {:else}
          <!-- 平台首页（概览营销页：能力全景 + 真实运行状态） -->
          <PlatformOverview
            {statusText}
            {statusCls}
            onNavigate={navigateToTab}
          />
        {/if}
      </PanelSection>
      <PanelSection active={activeTab === 'harness'}>
        <HarnessTab />
      </PanelSection>
      <PanelSection active={activeTab === 'agents'}>
        <AgentPanel onOpenAgent={openAgentDetail} />
      </PanelSection>
      <PanelSection active={activeTab === 'automation'}>
        <AutomationPanel />
      </PanelSection>
      <PanelSection active={activeTab === 'db_manager'}>
        <DbManager active={activeTab === 'db_manager'} notify={notify} />
      </PanelSection>
      <PanelSection active={activeTab === 'wechat'}>
        <!-- 微信数据管理：先过启动页（初始化检查），完成后进入主界面 -->
        {#if wechatReady}
          <WeChatPanel bind:msgCount={msgCount} openConfigTick={wechatConfigTick} />
        {:else}
          <WeChatBootstrap
            ondone={() => (wechatReady = true)}
            onconfig={() => { activeTab = 'wechat'; wechatReady = true; wechatConfigTick += 1; }}
          />
        {/if}
      </PanelSection>
      <PanelSection active={activeTab === 'llm'}>
        <LlmPanel />
      </PanelSection>
      <PanelSection active={activeTab === 'kb'}>
        <KnowledgeBase />
      </PanelSection>
      <PanelSection active={activeTab === 'ocr'}>
        <OcrPanel />
      </PanelSection>

    </main>
  </div>
</div>

<!-- ============================================================ -->
<!--   CSS 设计系统                                          -->
<!-- ============================================================ -->
<style>
  /* ═══════════════════════════════════════════════════════════
     ST 控制台 — 标本纸视觉外壳（shadcn 令牌 + 浅色植物学记录册）
     主题令牌来自 src/app.css（--background/--card/--primary/...），
     并联动 --app-* 个性化变量。
     注：不再 @import 外部字体（Google Fonts 会被网络环境阻断并阻塞首屏渲染），
     字体栈优先使用系统字体，需要时可在个性化中选择已安装字体。
     ═══════════════════════════════════════════════════════════ */

  :global(:root) {
    --app-font-size: 14px;
    --app-font-color: rgba(38, 40, 46, 1);
    --app-bg-color: #ecebe7;
    /* 卡片面兜底：从背景与文字派生（深底自动深面板，浅底自动骨白）；首帧脚本会写入精确主题值 */
    --app-color-card-bg: color-mix(in srgb, var(--app-bg-color, #ecebe7) 88%, var(--app-font-color, #26282e));
    --app-font-family: -apple-system, "PingFang SC", "Microsoft YaHei", "Helvetica Neue", sans-serif;
    --app-color-text: var(--app-font-color);
    --app-color-secondary: color-mix(in srgb, var(--app-font-color) 64%, var(--app-bg-color));
    --app-color-muted: color-mix(in srgb, var(--app-font-color) 46%, transparent);
    --app-color-border: color-mix(in srgb, var(--app-bg-color) 60%, var(--app-font-color));
    --app-color-border-light: color-mix(in srgb, var(--app-bg-color) 80%, var(--app-font-color));
    --app-color-hover-bg: color-mix(in srgb, var(--app-bg-color) 85%, var(--app-font-color));
    --app-color-bg-subtle: color-mix(in srgb, var(--app-bg-color) 92%, var(--app-font-color));
    --app-color-surface-alt: color-mix(in srgb, var(--app-bg-color) 50%, var(--app-color-card-bg));
    --app-color-input-border: color-mix(in srgb, var(--app-bg-color) 55%, var(--app-font-color));
    --app-accent: #22d3ee;
    --app-accent-hover: #0d93a5;
    --app-color-accent: var(--app-accent);
    --app-color-accent-hover: var(--app-accent-hover);
    --app-accent-light: rgba(13, 147, 165, 0.12);
    --app-accent-badge: rgba(13, 147, 165, 0.16);
    /* 微信配置等引用的几何/阴影令牌（此前未定义，导致圆角与阴影失效） */
    --app-radius-sm: 8px;
    --app-radius-md: 10px;
    --app-radius-lg: 12px;
    --app-radius-xl: 16px;
    --app-radius-2xl: 18px;
    --app-shadow-sm: 0 1px 2px color-mix(in srgb, var(--app-font-color) 14%, transparent);
    --app-gold: #b98a45;
    --app-gold-soft: rgba(185, 138, 69, 0.14);
    --app-success: #16a34a;
    --app-success-bg: rgba(22, 163, 74, 0.1);
    --app-success-dark: #15803d;
    --app-warning: #d97706;
    --app-warning-bg: rgba(217, 119, 6, 0.1);
    --app-danger: #dc2626;
  }

  /* ---------- 布局骨架 ---------- */
  .layout {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100vh;
    width: 100vw;
    background: var(--background);
    color: var(--foreground);
    overflow: hidden;
  }
  /* FancyUI 环境背景：微光网格层 */
  .app-ambient {
    position: absolute;
    inset: 0;
    z-index: 0;
    pointer-events: none;
  }
  .body {
    position: relative;
    z-index: 1;
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .content {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    padding: 16px;
    /* 机台灰台面：左上/右上的青蓝与台面微光，网格如刻线纹理透出 */
    background:
      radial-gradient(900px 420px at 12% -10%, color-mix(in oklab, var(--brand) 7%, transparent), transparent 62%),
      radial-gradient(1000px 480px at 92% -12%, color-mix(in oklab, #7fc9c8 9%, transparent), transparent 56%),
      linear-gradient(180deg, color-mix(in oklab, var(--app-bg-color) 92%, white) 0%, var(--app-bg-color) 100%);
    /* 组件表面保持实心骨白；微光网格在面板/卡片间隙透出 */
  }
  /* 内容区主题色闪烁网格层 */
  .content-grid {
    position: absolute;
    inset: 0;
    z-index: 0;
    pointer-events: none;
  }

  /* ---------- 自定义标题栏（仪表台：机台灰面板 + LED 品牌块） ---------- */
  .titlebar {
    position: relative;
    z-index: 1;
    height: 38px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 8px 0 10px;
    background: var(--sidebar);
    border-bottom: 1px solid var(--border);
    box-shadow: inset 0 1px 0 color-mix(in oklab, var(--foreground) 4%, transparent);
    user-select: none;
  }
  .titlebar::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 1px;
    pointer-events: none;
    background: color-mix(in oklab, var(--brand) 45%, transparent);
    opacity: 0.5;
  }
  .titlebar-left {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .titlebar-brand {
    position: relative;
    flex: none;
    width: 24px;
    height: 24px;
  }
  .titlebar-brand-led {
    position: absolute;
    top: -1px;
    right: -1px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--brand);
    border: 1px solid color-mix(in oklab, var(--sidebar) 80%, white);
    box-shadow: 0 0 6px color-mix(in oklab, var(--brand) 70%, transparent);
  }
  .titlebar-brand-icon {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    font-size: 11.5px;
    font-weight: 800;
    letter-spacing: 0.02em;
    color: var(--primary-foreground);
    background: var(--primary);
    border: 1px solid color-mix(in oklab, var(--primary) 70%, white);
  }
  .titlebar-title {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    color: var(--foreground);
    white-space: nowrap;
  }
  .titlebar-divider {
    width: 1px;
    height: 14px;
    background: var(--border);
  }
  .titlebar-meta {
    font-size: 11.5px;
    letter-spacing: 0.08em;
    color: var(--muted-foreground);
    white-space: nowrap;
  }
  .titlebar-actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .titlebar-btn {
    width: 42px;
    height: 30px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-radius: 7px;
    color: var(--muted-foreground);
    cursor: pointer;
    transition: background-color 0.16s ease, color 0.16s ease, box-shadow 0.16s ease;
  }
  .titlebar-btn:hover {
    background: color-mix(in oklab, var(--brand) 12%, transparent);
    color: var(--brand-strong);
  }
  .titlebar-btn-close:hover {
    background: var(--destructive);
    color: white;
    box-shadow: 0 0 12px color-mix(in oklab, var(--destructive) 46%, transparent);
  }

  /* ---------- 侧边栏 ---------- */
  /* 展开/折叠过渡统一走 Material ease-in-out 曲线：两端缓冲、中间最快，方向反转也不生硬。
     文字/徽章等不再用 display:none 硬切，而是跟随宽度收敛 + 淡入淡出：
     折叠时立即淡出，展开时略延迟淡入（等面板打开一点再露字） */
  .sidebar {
    --sidebar-ease: cubic-bezier(0.4, 0, 0.2, 1);
    --sidebar-dur: 0.26s;
    width: 232px;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--sidebar);
    border-right: 1px solid var(--sidebar-border);
    transition: width var(--sidebar-dur) var(--sidebar-ease);
    overflow: hidden;
  }
  .sidebar-collapsed { width: 64px; }
  .sidebar-drawer {
    position: absolute;
    z-index: 60;
    top: 38px;
    bottom: 0;
    left: 0;
    box-shadow: 12px 0 32px rgba(35, 48, 44, 0.22);
  }
  .sidebar-drawer-overlay {
    position: fixed;
    inset: 0;
    z-index: 55;
    background: rgba(0, 0, 0, 0.5);
  }

  .sidebar-header {
    padding: 16px 14px 14px;
    flex: none;
    border-bottom: 1px solid var(--sidebar-border);
    background: color-mix(in srgb, var(--sidebar) 96%, var(--brand) 2%);
    transition: padding-inline var(--sidebar-dur) var(--sidebar-ease);
  }
  .sidebar-brand {
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .brand-icon-wrap {
    position: relative;
    flex: none;
    margin-right: 11px;
    transition: margin-right var(--sidebar-dur) var(--sidebar-ease);
  }
  .brand-icon-led {
    position: absolute;
    top: -2px;
    right: -2px;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--brand);
    border: 2px solid var(--sidebar);
    box-shadow: 0 0 7px color-mix(in oklab, var(--brand) 65%, transparent);
    z-index: 1;
  }
  .brand-icon {
    width: 38px;
    height: 38px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    font-size: 15px;
    font-weight: 800;
    letter-spacing: 0.04em;
    color: var(--primary-foreground);
    background: var(--primary);
    border: 1px solid color-mix(in oklab, var(--primary) 70%, white);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.18);
  }
  .brand-text {
    min-width: 0;
    max-width: 176px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    gap: 4px;
    transition:
      max-width var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.18s ease 0.06s,
      transform var(--sidebar-dur) var(--sidebar-ease) 0.06s,
      visibility 0s linear 0.06s;
  }
  .navbar-title {
    font-size: 15px;
    font-weight: 800;
    letter-spacing: 0.03em;
    white-space: nowrap;
    color: var(--foreground);
    line-height: 1.2;
  }
  .brand-meta {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    white-space: nowrap;
  }
  .brand-version {
    font-family: var(--font-mono);
    font-size: 11.5px;
    font-weight: 700;
    line-height: 1;
    padding: 3px 6px;
    border-radius: 5px;
    background: color-mix(in oklab, var(--primary) 16%, transparent);
    color: var(--primary);
    border: 1px solid color-mix(in oklab, var(--primary) 28%, transparent);
  }
  .brand-edition {
    font-size: 11.5px;
    color: var(--muted-foreground);
  }
  /* 折叠 rail：宽度收敛 + 淡出 + 轻微左移，跟随面板一起收拢 */
  .sidebar-collapsed .brand-text {
    max-width: 0;
    opacity: 0;
    transform: translateX(-8px);
    visibility: hidden;
    transition:
      max-width var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.16s ease,
      transform 0.22s var(--sidebar-ease),
      visibility 0s linear var(--sidebar-dur);
  }
  .sidebar-collapsed .sidebar-header { padding-inline: 0; }
  .sidebar-collapsed .brand-icon-wrap { margin-right: 0; }

  .nav-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 6px 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .nav-section-label {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.16em;
    text-align: center;
    color: var(--muted-foreground);
    padding: 14px 10px 5px;
    max-height: 60px;
    overflow: hidden;
    transition:
      max-height var(--sidebar-dur) var(--sidebar-ease),
      padding-block var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.18s ease 0.06s,
      visibility 0s linear 0.06s;
  }
  .sidebar-collapsed .nav-section-label {
    max-height: 0;
    padding-block: 0;
    opacity: 0;
    visibility: hidden;
    transition:
      max-height var(--sidebar-dur) var(--sidebar-ease),
      padding-block var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.16s ease,
      visibility 0s linear var(--sidebar-dur);
  }
  .nav-item {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--sidebar-foreground);
    font-size: 13px;
    text-align: center;
    cursor: pointer;
    position: relative;
    transition: background 0.12s, color 0.12s;
  }
  .nav-item:hover { background: var(--sidebar-accent); }
  .nav-item.active {
    background: color-mix(in oklab, var(--brand) 12%, var(--sidebar));
    color: var(--sidebar-accent-foreground);
    font-weight: 600;
  }
  .nav-item.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 18%;
    bottom: 18%;
    width: 3.5px;
    border-radius: 0 4px 4px 0;
    background: var(--primary);
  }
  .nav-icon {
    flex: none;
    margin-right: 10px;
    color: currentColor;
    opacity: 0.9;
    transition: margin-right var(--sidebar-dur) var(--sidebar-ease);
  }
  .nav-text {
    flex: 0 1 auto;
    min-width: 0;
    max-width: 150px;
    overflow: hidden;
    white-space: nowrap;
    text-align: center;
    transition:
      max-width var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.18s ease 0.06s,
      transform var(--sidebar-dur) var(--sidebar-ease) 0.06s,
      visibility 0s linear 0.06s;
  }
  .nav-badge {
    flex: none;
    min-width: 18px;
    max-width: 60px;
    height: 18px;
    margin-left: 10px;
    padding: 0 5px;
    border-radius: 9px;
    background: var(--primary);
    color: var(--primary-foreground);
    font-size: 11.5px;
    font-weight: 700;
    display: grid;
    place-items: center;
    overflow: hidden;
    transition:
      max-width var(--sidebar-dur) var(--sidebar-ease),
      padding-inline var(--sidebar-dur) var(--sidebar-ease),
      margin-left var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.18s ease 0.06s,
      transform var(--sidebar-dur) var(--sidebar-ease) 0.06s,
      visibility 0s linear 0.06s;
  }
  .nav-item-search {
    margin: 0 0 2px;
    padding: 7px 10px;
    border: 1px solid var(--sidebar-border);
    background: var(--card);
    transition: border-color 0.15s, background 0.15s;
  }
  .nav-item-search:hover {
    border-color: color-mix(in oklab, var(--primary) 42%, transparent);
    background: color-mix(in oklab, var(--card) 92%, var(--brand) 6%);
  }
  .nav-item-search .nav-badge {
    min-width: auto;
    height: auto;
    padding: 2px 6px;
    border-radius: 5px;
    background: transparent;
    color: var(--muted-foreground);
    border: 1px solid var(--border);
    font-size: 10.5px;
    font-weight: 600;
    font-family: var(--font-mono);
  }
  .nav-list-divider {
    height: 1px;
    background: var(--sidebar-border);
    margin: 8px 6px;
    opacity: 1;
    transition: opacity 0.18s ease;
  }
  .sidebar-collapsed .nav-icon { margin-right: 0; }
  .sidebar-collapsed .nav-text {
    max-width: 0;
    opacity: 0;
    transform: translateX(-8px);
    visibility: hidden;
    transition:
      max-width var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.16s ease,
      transform 0.22s var(--sidebar-ease),
      visibility 0s linear var(--sidebar-dur);
  }
  .sidebar-collapsed .nav-badge {
    max-width: 0;
    min-width: 0;
    padding-inline: 0;
    margin-left: 0;
    opacity: 0;
    transform: translateX(6px);
    visibility: hidden;
    transition:
      max-width var(--sidebar-dur) var(--sidebar-ease),
      padding-inline var(--sidebar-dur) var(--sidebar-ease),
      margin-left var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.16s ease,
      transform 0.22s var(--sidebar-ease),
      visibility 0s linear var(--sidebar-dur);
  }
  .sidebar-collapsed .nav-list-divider { opacity: 0; }

  /* ---------- 侧边栏底部 ---------- */
  .sidebar-spacer { flex: 0 0 auto; height: 0; }
  .sidebar-footer {
    flex: none;
    padding: 12px 10px;
    border-top: 1px solid var(--sidebar-border);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .footer-status-row {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 5px 10px 7px;
    font-size: 12px;
    color: var(--muted-foreground);
  }
  .footer-status-dot {
    position: relative;
    width: 8px;
    height: 8px;
    margin-right: 8px;
    border-radius: 50%;
    background: var(--app-success, #52c41a);
    box-shadow: 0 0 8px var(--app-success, #52c41a);
    transition: margin-right var(--sidebar-dur) var(--sidebar-ease);
  }
  .footer-status-row.tag-success .footer-status-dot::after {
    content: '';
    position: absolute;
    inset: -4px;
    border-radius: 50%;
    border: 1px solid var(--app-success, #52c41a);
    animation: status-pulse 2.4s ease-out infinite;
  }
  @keyframes status-pulse {
    0% { transform: scale(0.55); opacity: 0.85; }
    70% { transform: scale(1.45); opacity: 0; }
    100% { transform: scale(1.45); opacity: 0; }
  }
  .tag-warn .footer-status-dot, .footer-status-row.tag-warn .footer-status-dot { background: #f5b301; box-shadow: 0 0 8px #f5b301; }
  .tag-danger .footer-status-dot, .footer-status-row.tag-danger .footer-status-dot { background: #ff5f56; box-shadow: 0 0 8px #ff5f56; }
  .tag-default .footer-status-dot, .footer-status-row.tag-default .footer-status-dot { background: #8a93a5; box-shadow: none; }
  .footer-divider {
    height: 1px;
    background: var(--sidebar-border);
    margin: 6px 4px;
    transition:
      height var(--sidebar-dur) var(--sidebar-ease),
      margin-block var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.18s ease;
  }
  .footer-action {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 8px 10px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--sidebar-foreground);
    font-size: 13px;
    cursor: pointer;
    text-align: center;
  }
  .footer-action:hover { background: var(--sidebar-accent); }
  .footer-action-text {
    white-space: nowrap;
    max-width: 140px;
    overflow: hidden;
    transition:
      max-width var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.18s ease 0.06s,
      transform var(--sidebar-dur) var(--sidebar-ease) 0.06s,
      visibility 0s linear 0.06s;
  }
  .footer-status-text {
    white-space: nowrap;
    max-width: 120px;
    overflow: hidden;
    transition:
      max-width var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.18s ease 0.06s,
      visibility 0s linear 0.06s;
  }
  .sidebar-collapsed .footer-status-text {
    max-width: 0;
    opacity: 0;
    visibility: hidden;
    transition:
      max-width var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.16s ease,
      visibility 0s linear var(--sidebar-dur);
  }
  .sidebar-collapsed .footer-action-text {
    max-width: 0;
    opacity: 0;
    transform: translateX(-8px);
    visibility: hidden;
    transition:
      max-width var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.16s ease,
      transform 0.22s var(--sidebar-ease),
      visibility 0s linear var(--sidebar-dur);
  }
  .sidebar-collapsed .footer-status-dot { margin-right: 0; }
  .sidebar-collapsed .footer-divider {
    height: 0;
    margin-block: 0;
    opacity: 0;
    transition:
      height var(--sidebar-dur) var(--sidebar-ease),
      margin-block var(--sidebar-dur) var(--sidebar-ease),
      opacity 0.16s ease;
  }

  .sidebar-collapse-bar {
    flex: none;
    display: flex;
    justify-content: center;
    padding: 6px 8px 10px;
  }
  .collapse-btn {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
    transition: background-color 0.18s ease, color 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease;
  }
  .collapse-btn:hover {
    background: var(--muted);
    color: var(--foreground);
    border-color: color-mix(in oklab, var(--brand) 35%, var(--border));
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--brand) 12%, transparent);
  }
  .collapse-icon {
    transition: transform 0.32s var(--sidebar-ease);
  }
  .sidebar-collapsed .collapse-icon { transform: rotate(180deg); }

  /* 系统预设减少动效时，折叠过渡退化为瞬间切换 */
  @media (prefers-reduced-motion: reduce) {
    .sidebar,
    .sidebar-header,
    .brand-icon-wrap,
    .brand-text,
    .nav-section-label,
    .nav-icon,
    .nav-text,
    .nav-badge,
    .nav-list-divider,
    .footer-status-dot,
    .footer-status-text,
    .footer-divider,
    .footer-action-text,
    .collapse-btn,
    .collapse-icon {
      transition-duration: 0.01ms !important;
      transition-delay: 0s !important;
    }
  }

  /* ---------- 通用卡片 ---------- */
  /* :global：.card 是全站共享语义类（子组件也直接 class="card"），
     组件作用域样式无法命中子组件元素——此前 PlatformOverview 等
     子组件里的 card 无背景/边框，卡片只剩透明底（J-25 修复） */
  :global(.card) {
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 16px 18px;
    /* 标本纸：纸白表面 + 细规则线，层级只靠边框，不叠重影 */
    box-shadow: none;
  }
  .card-grow { flex: 1; min-height: 0; display: grid; place-items: center; }

  /* ---------- 平台首页（监控） ---------- */
  .monitor-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 12px;
    padding: 2px 2px 0;
  }
  .monitor-title { font-size: 16px; font-weight: 700; color: var(--foreground); }
  .monitor-sub { font-size: 12.5px; color: var(--muted-foreground); margin-top: 2px; }
  .monitor-status {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 12.5px;
    color: var(--muted-foreground);
    padding: 5px 11px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--card);
  }
  .monitor-status-sep {
    width: 1px;
    height: 12px;
    background: var(--border);
  }
  .monitor-status-meta {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--muted-foreground);
  }

  .monitor-stats {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 12px;
  }
  .meter-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
  }
  .meter-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .meter-label {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--muted-foreground);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meter-led {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: color-mix(in oklab, var(--foreground) 16%, transparent);
    border: 1px solid var(--border);
    transition: background 0.2s ease, box-shadow 0.2s ease;
  }
  .meter-led-on {
    background: var(--brand);
    border-color: color-mix(in oklab, var(--brand) 60%, white);
    box-shadow: 0 0 8px color-mix(in oklab, var(--brand) 55%, transparent);
  }
  .meter-value {
    font-family: var(--font-mono);
    font-size: 26px;
    font-weight: 700;
    line-height: 1.05;
    color: var(--foreground);
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
  }
  .monitor-card-shell {
    position: relative;
    overflow: hidden;
    border-radius: var(--radius-lg);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  /* 快捷操作：机架式模块入口 */
  .monitor-quick {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .quick-label {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.16em;
    color: var(--muted-foreground);
  }
  .quick-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .quick-btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 7px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--card);
    color: var(--foreground);
    font-size: 12.5px;
    cursor: pointer;
    transition: border-color 0.15s ease, background 0.15s ease, transform 0.05s ease;
  }
  .quick-btn:hover {
    border-color: color-mix(in oklab, var(--brand) 45%, var(--border));
    background: color-mix(in oklab, var(--brand) 7%, var(--card));
  }
  .quick-btn:active {
    transform: translateY(1px);
  }
  .quick-btn svg {
    color: var(--brand-strong);
  }

  .monitor-cols {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }
  .monitor-card {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 0;
    overflow: hidden;
  }
  .monitor-card-hd {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }
  .monitor-card-title { font-size: 13.5px; font-weight: 600; color: var(--foreground); }
  .monitor-card-count { font-size: 12px; color: var(--muted-foreground); margin-left: auto; font-variant-numeric: tabular-nums; }
  .monitor-card-bd {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 6px 10px;
  }
  .monitor-empty {
    display: grid;
    place-items: center;
    height: 100%;
    min-height: 120px;
    font-size: 12.5px;
    color: var(--muted-foreground);
  }
  .monitor-agent {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border-radius: 8px;
  }
  .monitor-agent:hover { background: var(--muted); }
  .monitor-agent-dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--app-success, #52c41a);
  }
  .monitor-agent-main { flex: 1; min-width: 0; }
  .monitor-agent-name {
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: var(--foreground);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .monitor-agent-meta {
    display: block;
    font-size: 11.5px;
    color: var(--muted-foreground);
    margin-top: 1px;
  }
  .monitor-agent-time { flex: none; font-size: 11.5px; color: var(--muted-foreground); }
  .monitor-event {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 7px 10px;
    border-radius: 8px;
    font-size: 12px;
  }
  .monitor-event:hover { background: var(--muted); }
  .monitor-event-time {
    flex: none;
    color: var(--muted-foreground);
    font-variant-numeric: tabular-nums;
  }
  .monitor-event-name { flex: none; font-weight: 500; color: var(--foreground); }
  .monitor-event-detail {
    flex: 1;
    min-width: 0;
    color: var(--muted-foreground);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ---------- 表单 ---------- */
  /* ---------- 描述列表 ---------- */

  /* ---------- 标签 / 徽标 ---------- */

  /* ---------- 弹窗 ---------- */
  .modal-overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: rgba(35, 48, 44, 0.38);
    backdrop-filter: blur(4px);
    display: grid;
    place-items: center;
    padding: 24px;
  }
  .modal {
    width: min(640px, 94vw);
    max-height: 88vh;
    display: flex;
    flex-direction: column;
    background: var(--popover);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    box-shadow: 0 18px 50px rgba(35, 48, 44, 0.22);
    overflow: hidden;
  }
  .search-modal {
    width: min(960px, 94vw);
    height: min(760px, 88vh);
    padding: 0;
  }

  /* ---------- 设置 ---------- */

  /* ---------- Agent 日志 ---------- */

  /* ---------- 通知 ---------- */
  .toast-container {
    position: fixed;
    top: 48px;
    right: 16px;
    z-index: 200;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 340px;
  }
  .toast {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 12px 14px;
    border-radius: var(--radius-lg);
    background: var(--popover);
    border: 1px solid var(--border);
    box-shadow: 0 12px 36px rgba(35, 48, 44, 0.18), 0 0 20px -12px color-mix(in oklab, var(--primary) 34%, transparent);
    cursor: pointer;
    animation: toast-in 0.18s ease-out;
  }
  @keyframes toast-in {
    from { opacity: 0; transform: translateX(16px); }
    to { opacity: 1; transform: translateX(0); }
  }
  .toast-icon {
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 12px;
    font-weight: 700;
  }
  .toast-success .toast-icon { background: color-mix(in oklab, #16a34a 14%, transparent); color: #15803d; }
  .toast-warn .toast-icon { background: color-mix(in oklab, #d97706 14%, transparent); color: #b45309; }
  .toast-error .toast-icon { background: color-mix(in oklab, #dc2626 14%, transparent); color: #b91c1c; }
  .toast-title { font-size: 13px; font-weight: 600; color: var(--foreground); }
  .toast-desc { font-size: 12px; color: var(--muted-foreground); margin-top: 2px; word-break: break-all; }

  /* ---------- 通用 ---------- */
  .mono { font-family: var(--font-mono); }
  .muted { color: var(--muted-foreground); }

  @media (max-width: 900px) {
    .sidebar { position: absolute; z-index: 60; top: 38px; bottom: 0; left: 0; box-shadow: 12px 0 32px rgba(35,48,44,0.22); }
    .sidebar:not(.sidebar-drawer) { transform: translateX(-100%); }
  }
</style>

