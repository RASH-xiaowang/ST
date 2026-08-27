<script lang="ts">
  /**
   * 个人微信通信面板 — 主容器
   * 迁移自 viewapi 项目，包含 9 大功能模块
   * 登录 / 个人信息 / 联系人 / 消息 / 朋友圈 / 标签 / 收藏夹 / 推送解析 / 调用日志
   */
  import type { ModuleId } from '../../wechat-api/types';
  import WechatLogin from '../../wechat-api/components/WechatLogin.svelte';
  import WechatProfile from '../../wechat-api/components/WechatProfile.svelte';
  import WechatContacts from '../../wechat-api/components/WechatContacts.svelte';
  import WechatMessages from '../../wechat-api/components/WechatMessages.svelte';
  import WechatSns from '../../wechat-api/components/WechatSns.svelte';
  import WechatLabels from '../../wechat-api/components/WechatLabels.svelte';
  import WechatFavorites from '../../wechat-api/components/WechatFavorites.svelte';
  import WechatWebhook from '../../wechat-api/components/WechatWebhook.svelte';
  import WechatApiLogs from '../../wechat-api/components/WechatApiLogs.svelte';
  import WechatGroup from '../../wechat-api/components/WechatGroup.svelte';
  import WechatFinder from '../../wechat-api/components/WechatFinder.svelte';
  import { consoleState, purgeAll } from '../../wechat-api/stores/console.svelte';

  let activeModule = $state<ModuleId>('login');

  const modules: Array<{ id: ModuleId; label: string; icon: string }> = [
    { id: 'login', label: '登录', icon: 'M8 1a4 4 0 0 1 4 4v2h2a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1h2V5a4 4 0 0 1 4-4zm0 2a2 2 0 0 0-2 2v2h4V5a2 2 0 0 0-2-2z' },
    { id: 'profile', label: '个人信息', icon: 'M8 2a3 3 0 0 1 3 3v1a3 3 0 0 1-6 0V5a3 3 0 0 1 3-3zm-5 9a5 5 0 0 1 10 0H3z' },
    { id: 'contacts', label: '联系人与群', icon: 'M6 8a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm0 1c-2.67 0-8 1.34-8 4v1h16v-1c0-2.66-5.33-4-8-4zm8-1a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm0 1c-.93 0-1.78.13-2.56.35C13.29 10.13 15 11.07 15 12v1h4v-1c0-1.33-2.67-2.33-5-3z' },
    { id: 'messages', label: '消息', icon: 'M2 3h12a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H5l-3 3V4a1 1 0 0 1 1-1z' },
    { id: 'sns', label: '朋友圈', icon: 'M8 1a7 7 0 1 1 0 14A7 7 0 0 1 8 1zm0 2a5 5 0 1 0 0 10A5 5 0 0 0 8 3zm0 2a3 3 0 1 1 0 6 3 3 0 0 1 0-6z' },
    { id: 'labels', label: '标签', icon: 'M2 3h5l7 7-5 5-7-7V3zm2 2v2.59l5 5L13.41 8l-5-5H4z' },
    { id: 'favorites', label: '收藏夹', icon: 'M8 1l2.47 5L16 6.87l-4 3.87.94 5.5L8 13.77l-4.94 2.47.94-5.5-4-3.87L6.53 6z' },
    { id: 'group', label: '群管理', icon: 'M8 1a4 4 0 0 1 4 4v1a4 4 0 0 1-8 0V5a4 4 0 0 1 4-4zm-6 10a6 6 0 0 1 12 0H2z' },
    { id: 'finder', label: '视频号', icon: 'M8 1l3 5h-2v4h2l-3 5V1zM5 6h2V4l-3 5 3 5v-2H5l-3-5z' },
    { id: 'webhook', label: '推送解析', icon: 'M4 4h8v2H4V4zm0 4h8v2H4V8zm0 4h5v2H4v-2z' },
    { id: 'api-logs', label: '调用日志', icon: 'M3 3h10v2H3V3zm0 4h10v2H3V7zm0 4h7v2H3v-2z' },
  ];

  function handleClearCache() {
    if (confirm('确定清理全部缓存数据？')) {
      purgeAll();
      location.reload();
    }
  }
</script>

<div class="pw-panel">
  <!-- 顶栏：Token 状态 + appId + 快捷操作 -->
  <div class="pw-topbar">
    <div class="pw-topbar-left">
      <svg class="pw-topbar-icon" viewBox="0 0 16 16" width="18" height="18" fill="none">
        <path d="M5.5 3C3 3 1 4.5 1 6.5c0 1.2.7 2.3 1.8 3L2 12l2.7-1.3c.5.2 1 .3 1.6.3M8 3c2.5 0 4.5 1.5 4.5 3.5S10.5 10 8 10" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>
        <circle cx="3.5" cy="6.5" r=".5" fill="currentColor"/>
        <circle cx="5.5" cy="6.5" r=".5" fill="currentColor"/>
        <circle cx="7.5" cy="6.5" r=".5" fill="currentColor"/>
        <path d="M8 10c2.5 0 4.5-1.5 4.5-3.5 0-.5-.1-1-.3-1.5.9.5 1.8 1.3 1.8 2.5 0 1.2-.7 2.3-1.8 3L13 12l-2.1-1" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
      <h2 class="pw-topbar-title">个人微信通信</h2>
    </div>
    <div class="pw-topbar-right">
      <span class="pw-token-tag" class:valid={consoleState.tokenStatus === 'valid_locked'} class:invalid={consoleState.tokenStatus === 'invalid'}>
        Token: {consoleState.tokenStatus === 'valid_locked' ? '已锁定' : consoleState.tokenStatus === 'invalid' ? '已失效' : '未校验'}
      </span>
      {#if consoleState.appId}
        <span class="pw-appid-tag">appId: {consoleState.appId}</span>
      {/if}
      {#if consoleState.loginNickName}
        <span class="pw-nick-tag">{consoleState.loginNickName}</span>
      {/if}
      <button class="pw-clear-btn" onclick={handleClearCache}>清理缓存</button>
    </div>
  </div>

  <div class="pw-body">
    <!-- 侧边导航 -->
    <aside class="pw-sidebar">
      <nav class="pw-nav">
        {#each modules as mod}
          <button
            class="pw-nav-item"
            class:active={activeModule === mod.id}
            onclick={() => activeModule = mod.id}
            title={mod.label}
          >
            <svg class="pw-nav-icon" viewBox="0 0 16 16" width="16" height="16" fill="none">
              <path d={mod.icon} fill="currentColor"/>
            </svg>
            <span class="pw-nav-label">{mod.label}</span>
          </button>
        {/each}
      </nav>
    </aside>

    <!-- 内容区 -->
    <main class="pw-content">
      {#if activeModule === 'login'}
        <WechatLogin />
      {:else if activeModule === 'profile'}
        <WechatProfile />
      {:else if activeModule === 'contacts'}
        <WechatContacts />
      {:else if activeModule === 'messages'}
        <WechatMessages />
      {:else if activeModule === 'sns'}
        <WechatSns />
      {:else if activeModule === 'labels'}
        <WechatLabels />
      {:else if activeModule === 'favorites'}
        <WechatFavorites />
      {:else if activeModule === 'group'}
        <WechatGroup />
      {:else if activeModule === 'finder'}
        <WechatFinder />
      {:else if activeModule === 'webhook'}
        <WechatWebhook />
      {:else if activeModule === 'api-logs'}
        <WechatApiLogs />
      {/if}
    </main>
  </div>
</div>

<style>
  .pw-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }

  /* 顶栏 */
  .pw-topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in srgb, var(--card) 96%, var(--primary) 4%);
    flex-shrink: 0;
    gap: 12px;
    flex-wrap: wrap;
  }
  .pw-topbar-left {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .pw-topbar-icon { color: var(--primary); flex-shrink: 0; }
  .pw-topbar-title { font-size: 15px; font-weight: 700; margin: 0; white-space: nowrap; }
  .pw-topbar-right {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .pw-token-tag, .pw-appid-tag, .pw-nick-tag {
    font-size: 11.5px; padding: 2px 8px; border-radius: 4px;
    font-family: var(--font-mono);
    background: var(--muted); color: var(--muted-foreground);
  }
  .pw-token-tag.valid { background: color-mix(in srgb, #16a34a 14%, transparent); color: #15803d; }
  .pw-token-tag.invalid { background: color-mix(in srgb, #dc2626 14%, transparent); color: #b91c1c; }
  .pw-clear-btn {
    padding: 4px 10px; border: 1px solid var(--border); border-radius: 6px;
    background: var(--card); font-size: 12px; cursor: pointer; color: var(--foreground);
  }
  .pw-clear-btn:hover { background: var(--muted); }

  /* 主体 */
  .pw-body {
    flex: 1;
    min-height: 0;
    display: flex;
    overflow: hidden;
  }

  /* 侧边导航 */
  .pw-sidebar {
    width: 160px;
    flex-shrink: 0;
    border-right: 1px solid var(--border);
    background: color-mix(in srgb, var(--card) 97%, var(--primary) 3%);
    overflow-y: auto;
    padding: 8px 6px;
  }
  .pw-nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .pw-nav-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--foreground);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
    transition: background 0.12s, color 0.12s;
    position: relative;
  }
  .pw-nav-item:hover { background: var(--muted); }
  .pw-nav-item.active {
    background: color-mix(in srgb, var(--primary) 12%, var(--card));
    color: var(--primary);
    font-weight: 600;
  }
  .pw-nav-item.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 20%;
    bottom: 20%;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--primary);
  }
  .pw-nav-icon { flex-shrink: 0; opacity: 0.8; }
  .pw-nav-item.active .pw-nav-icon { opacity: 1; }
  .pw-nav-label { white-space: nowrap; }

  /* 内容区 */
  .pw-content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    padding: 16px;
    background: var(--background);
  }

  @media (max-width: 768px) {
    .pw-sidebar { width: 48px; padding: 8px 4px; }
    .pw-nav-label { display: none; }
    .pw-nav-item { justify-content: center; padding: 8px 6px; }
    .pw-nav-item.active::before { display: none; }
    .pw-topbar { padding: 8px 12px; }
  }
</style>
