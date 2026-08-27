<script lang="ts">
  /**
   * 企业微信通信面板
   * 功能：企业微信消息收发、通讯录管理、应用消息推送等
   */
</script>

<div class="ew-panel">
  <div class="ew-header">
    <div class="ew-header-left">
      <svg class="ew-icon" viewBox="0 0 16 16" width="18" height="18" fill="none">
        <rect x="2" y="3" width="12" height="10" rx="2" stroke="currentColor" stroke-width="1.3"/>
        <path d="M5 7h6M5 10h4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
        <path d="M6 3V1.5M10 3V1.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
      </svg>
      <h2 class="ew-title">企业微信通信</h2>
    </div>
  </div>

  <div class="ew-body">
    <div class="ew-placeholder">
      <svg viewBox="0 0 64 64" width="64" height="64" fill="none">
        <rect x="12" y="16" width="40" height="32" rx="4" stroke="currentColor" stroke-width="2" opacity="0.3"/>
        <path d="M22 28h20M22 36h12" stroke="currentColor" stroke-width="2" stroke-linecap="round" opacity="0.3"/>
        <circle cx="32" cy="10" r="4" stroke="currentColor" stroke-width="2" opacity="0.3"/>
      </svg>
      <p class="ew-placeholder-text">企业微信通信功能</p>
      <p class="ew-placeholder-desc">支持企业微信消息收发、通讯录管理、应用消息推送、群组管理等功能</p>
    </div>
  </div>
</div>

<style>
  .ew-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .ew-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--card);
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    flex-shrink: 0;
  }
  .ew-header-left {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .ew-icon {
    color: var(--primary);
    flex-shrink: 0;
  }
  .ew-title {
    font-size: 15px;
    font-weight: 700;
    color: var(--foreground);
    margin: 0;
  }
  .ew-body {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--card);
    border: 1px solid var(--border);
    border-top: none;
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
  }
  .ew-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    color: var(--muted-foreground);
  }
  .ew-placeholder-text {
    font-size: 16px;
    font-weight: 600;
    color: var(--foreground);
    margin: 0;
  }
  .ew-placeholder-desc {
    font-size: 13px;
    color: var(--muted-foreground);
    margin: 0;
    text-align: center;
    max-width: 360px;
    line-height: 1.6;
  }
</style>
