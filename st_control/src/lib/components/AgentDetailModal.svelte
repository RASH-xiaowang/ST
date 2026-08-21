<!-- Agent 详情弹窗：展示接入信息 + 下发任务（自包含组件，从 App.svelte 抽出） -->
<script lang="ts">
  import type { AgentInfo } from '../communication/types';
  import { agentApi } from '../agents/services/ipc';
  import { Input } from './ui/input';
  import { Textarea } from './ui/textarea';
  import Modal from './Modal.svelte';
  import { RippleButton } from 'fancy-ui-svelte';

  let {
    agent,
    onClose,
    notify,
  }: {
    agent: AgentInfo | null;
    onClose: () => void;
    notify: (title: string, msg: string, type: 'success' | 'warn' | 'error') => void;
  } = $props();

  let cmdMethod = $state('task.execute');
  let cmdPayload = $state('{ "key": "value" }');
  let cmdSending = $state(false);
  let cmdResult = $state('');

  // 每次打开时重置任务表单
  $effect(() => {
    if (agent) {
      cmdMethod = 'task.execute';
      cmdPayload = '{ "key": "value" }';
      cmdResult = '';
    }
  });

  async function handleSendTask() {
    if (!agent) return;
    cmdSending = true;
    cmdResult = '';
    try {
      const payload = JSON.parse(cmdPayload);
      const msgId = await agentApi.sendCommand(agent.id, cmdMethod, payload);
      cmdResult = `✓ 已下发 (${msgId.slice(0, 8)}...)`;
      notify('任务已下发', `方法: ${cmdMethod} → ${agent.name}`, 'success');
    } catch (err) {
      cmdResult = `✕ ${err}`;
      notify('下发失败', String(err), 'error');
    } finally {
      cmdSending = false;
    }
  }
</script>

{#if agent}
  <Modal open={agent !== null} onClose={onClose}>
      <div class="modal-hd">
        <h2 class="modal-title">{agent.name}</h2>
        <span class="modal-id mono muted">ID: {agent.id}</span>
        <button class="modal-close" onclick={onClose} aria-label="关闭" title="关闭">
          <svg viewBox="0 0 16 16" width="14" height="14" fill="none" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
        </button>
      </div>

      <div class="modal-body">
        <dl class="dl modal-dl">
          <div><dt>主机</dt><dd class="mono">{agent.remoteAddr}</dd></div>
          <div><dt>接入时间</dt><dd class="mono">{new Date(agent.connectedAt).toLocaleString()}</dd></div>
          <div><dt>最后心跳</dt><dd class="mono">{new Date(agent.lastHeartbeat).toLocaleString()}</dd></div>
          <div><dt>状态</dt><dd><span class="tag tag-success">在线</span></dd></div>
        </dl>

        <div class="modal-divider"></div>

        <h3 class="modal-subtitle">下发任务</h3>

        <div class="form-group">
          <label for="cmd-method">命令方法</label>
          <Input id="cmd-method" bind:value={cmdMethod} placeholder="e.g. task.execute" />
        </div>

        <div class="form-group">
          <label for="cmd-payload">参数 (JSON)</label>
          <Textarea id="cmd-payload" bind:value={cmdPayload} rows={4} placeholder={'{"key": "value"}'} />
        </div>

        <RippleButton onclick={handleSendTask} disabled={cmdSending} rippleColor="#a5f3fc"
          class="h-9 rounded-md border-0 bg-[var(--primary)] px-4 text-sm font-medium text-[var(--primary-foreground)] hover:opacity-90">
          {cmdSending ? '发送中...' : '下发任务'}
        </RippleButton>

        {#if cmdResult}
          <div class="cmd-result">{cmdResult}</div>
        {/if}
      </div>
  </Modal>
{/if}

<style>
  .modal-hd {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklab, var(--popover) 88%, black 12%);
  }
  .modal-title { font-size: 16px; font-weight: 700; color: var(--foreground); flex: 1; }
  .modal-id { font-size: 11.5px; }
  .modal-close {
    width: 28px;
    height: 28px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 14px;
    cursor: pointer;
  }
  .modal-close:hover { background: var(--muted); color: var(--foreground); }
  .modal-body { padding: 18px; overflow-y: auto; }
  .modal-divider { height: 1px; background: var(--border); margin: 16px 0; }
  .modal-subtitle { font-size: 13px; font-weight: 600; color: var(--foreground); margin: 0 0 10px; }
  .form-group { display: flex; flex-direction: column; gap: 6px; margin-bottom: 12px; }
  .form-group label { font-size: 12px; font-weight: 500; color: var(--muted-foreground); }
  .cmd-result {
    margin-top: 12px;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    background: color-mix(in oklab, var(--primary) 10%, transparent);
    border: 1px solid color-mix(in oklab, var(--primary) 30%, transparent);
    color: var(--foreground);
    font-size: 12px;
    font-family: var(--font-mono);
  }
  .dl { display: flex; flex-direction: column; }
  .dl > div { display: flex; justify-content: space-between; gap: 16px; padding: 7px 0; border-bottom: 1px dashed var(--border); }
  .dl > div:last-child { border-bottom: none; }
  .dl dt { color: var(--muted-foreground); font-size: 12px; }
  .dl dd { font-size: 13px; color: var(--foreground); text-align: right; word-break: break-all; }
  .tag {
    display: inline-flex;
    align-items: center;
    height: 22px;
    padding: 0 9px;
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 600;
    background: var(--muted);
    color: var(--muted-foreground);
  }
  .tag-success { background: color-mix(in oklab, #22c55e 16%, transparent); color: #4ade80; }
  .mono { font-family: var(--font-mono); }
  .muted { color: var(--muted-foreground); }
</style>
