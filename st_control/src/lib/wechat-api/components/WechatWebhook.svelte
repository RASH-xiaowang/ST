<script lang="ts">
  /** 推送解析模块 — 迁移自 viewapi webhook 分区 */
  import { saveWebhookUrl, loadWebhookUrl } from '../stores/console.svelte';

  let webhookUrl = $state(loadWebhookUrl());
  let sandboxJson = $state('');
  let terminalLines = $state<Array<{ time: string; text: string; type: string }>>([]);

  function addTerminal(text: string, type = 'info') {
    const time = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    terminalLines = [...terminalLines, { time, text, type }].slice(-500);
  }

  function saveUrl() {
    saveWebhookUrl(webhookUrl);
    addTerminal(`✅ Webhook 地址已保存: ${webhookUrl}`, 'success');
  }

  function simulatePush() {
    if (!sandboxJson.trim()) {
      addTerminal('❌ 请粘贴推送 JSON', 'error');
      return;
    }
    try {
      const data = JSON.parse(sandboxJson);
      handlePushMessage(data);
    } catch (e) {
      addTerminal(`❌ JSON 解析失败: ${(e as Error).message}`, 'error');
    }
  }

  function handlePushMessage(data: Record<string, unknown>) {
    const typeName = data.TypeName as string || 'Unknown';
    addTerminal(`📨 TypeName: ${typeName}`, 'push');

    if (typeName === 'AddMsg') {
      const msg = data.AddMsg as Record<string, unknown> || {};
      const fromUser = msg.FromUserName as string || '';
      const content = msg.Content as string || '';
      const msgType = msg.MsgType as number || 0;
      addTerminal(`  发件人: ${fromUser}`, 'push');
      addTerminal(`  消息类型: ${msgType}`, 'push');

      // 群聊消息解析
      if (fromUser.endsWith('@chatroom') && content.includes(':\n')) {
        const parts = content.split(':\n');
        addTerminal(`  群内发言人: ${parts[0]}`, 'push');
        addTerminal(`  消息内容: ${parts.slice(1).join(':\n')}`, 'push');
      } else {
        addTerminal(`  消息内容: ${content.slice(0, 500)}`, 'push');
      }
    }

    // 完整 JSON
    addTerminal(`  完整 JSON: ${JSON.stringify(data, null, 2).slice(0, 1000)}`, 'json');
  }

  // 暴露全局函数供外部调用
  if (typeof window !== 'undefined') {
    (window as unknown as Record<string, unknown>).handlePushMessage = (jsonStr: string) => {
      try {
        handlePushMessage(JSON.parse(jsonStr));
      } catch (e) {
        addTerminal(`❌ 解析失败: ${(e as Error).message}`, 'error');
      }
    };
  }

  function clearTerminal() { terminalLines = []; }
</script>

<div class="wa-mod">
  <div class="wa-mod-top">
    <div class="wa-card">
      <h3 class="wa-card-title">消息推送解析（Webhook）</h3>
      <p class="wa-hint">配置接收推送的服务器地址，并可通过 JSON 沙箱模拟接收推送消息。</p>
    </div>

    <div class="wa-card">
      <h3 class="wa-card-title">Webhook 地址配置</h3>
      <div class="wa-form-grid">
        <label class="wa-field">
          <span class="wa-label">接收推送的服务器地址</span>
          <div class="wa-input-row">
            <input type="url" bind:value={webhookUrl} placeholder="https://your-server.example/hook" />
            <button class="wa-btn wa-btn-primary" onclick={saveUrl}>保存配置</button>
          </div>
        </label>
      </div>
    </div>

    <div class="wa-card">
      <h3 class="wa-card-title">JSON 模拟推送测试沙箱</h3>
      <div class="wa-form-grid">
        <label class="wa-field">
          <span class="wa-label">回调 JSON</span>
          <textarea bind:value={sandboxJson} rows="6" placeholder={"{\"TypeName\":\"AddMsg\",...}" }></textarea>
        </label>
      </div>
      <div class="wa-actions">
        <button class="wa-btn wa-btn-primary" onclick={simulatePush}>模拟接收推送</button>
        <button class="wa-btn" onclick={clearTerminal}>清空终端</button>
      </div>
    </div>
  </div>

  <div class="wa-terminal-wrap">
    <div class="wa-terminal-head">
      <span class="wa-terminal-title">解析终端</span>
      <span class="wa-terminal-hint">黑底终端 · 绿字正文</span>
    </div>
    <div class="wa-terminal">
      {#each terminalLines as line}
        <div class="wa-terminal-line" class:error={line.type === 'error'} class:success={line.type === 'success'} class:push={line.type === 'push'} class:json={line.type === 'json'}>
          <span class="wa-terminal-time">[{line.time}]</span> {line.text}
        </div>
      {:else}
        <div class="wa-terminal-empty">等待推送消息...</div>
      {/each}
    </div>
  </div>
</div>

<style>
  .wa-mod { height: 100%; display: flex; flex-direction: column; gap: 12px; }
  .wa-mod-top { display: flex; flex-direction: column; gap: 12px; }
  .wa-card { background: var(--card); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 16px; }
  .wa-card-title { font-size: 14px; font-weight: 600; margin: 0 0 12px; }
  .wa-hint { font-size: 12px; color: var(--muted-foreground); margin: 0 0 12px; }
  .wa-form-grid { display: flex; flex-direction: column; gap: 10px; }
  .wa-field { display: flex; flex-direction: column; gap: 4px; }
  .wa-label { font-size: 12px; font-weight: 600; }
  .wa-field input, .wa-field textarea { padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; color: var(--foreground); }
  .wa-input-row { display: flex; gap: 8px; }
  .wa-input-row input { flex: 1; }
  .wa-actions { display: flex; gap: 8px; margin-top: 12px; }
  .wa-btn { padding: 6px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; cursor: pointer; color: var(--foreground); }
  .wa-btn:hover { background: var(--muted); }
  .wa-btn-primary { background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }
  .wa-terminal-wrap { flex: 1; min-height: 0; display: flex; flex-direction: column; border-radius: var(--radius-lg); overflow: hidden; border: 1px solid var(--border); }
  .wa-terminal-head { display: flex; align-items: center; justify-content: space-between; padding: 8px 14px; background: #2d2d2d; }
  .wa-terminal-title { font-size: 13px; font-weight: 600; color: #a9b7c6; }
  .wa-terminal-hint { font-size: 11.5px; color: #666; }
  .wa-terminal { flex: 1; min-height: 0; overflow-y: auto; background: #1e1e1e; padding: 10px; font-family: var(--font-mono); font-size: 12px; color: #a6e22e; }
  .wa-terminal-line { padding: 2px 0; white-space: pre-wrap; word-break: break-all; }
  .wa-terminal-line.error { color: #f56c6c; }
  .wa-terminal-line.success { color: #67c23a; }
  .wa-terminal-line.push { color: #e6a23c; }
  .wa-terminal-line.json { color: #909399; }
  .wa-terminal-time { color: #666; }
  .wa-terminal-empty { color: #666; }
</style>
