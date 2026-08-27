<script lang="ts">
  /**
   * 知识库登录弹窗组件
   * 功能：用户名/密码登录、退出登录
   */
  import { kbApi } from './services/ipc';
  import { kbUser } from './auth.svelte';
  import KbIcon from './KbIcon.svelte';
  import KbModal from './KbModal.svelte';
  import { Button } from '../components/ui/button';

  interface Props {
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
    onLoginSuccess?: () => void;
  }
  let { notify, onLoginSuccess }: Props = $props();

  let open = $state(false);
  let username = $state('');
  let password = $state('');
  let busy = $state(false);
  let err = $state('');

  function show() {
    username = '';
    password = '';
    err = '';
    busy = false;
    open = true;
  }

  async function doLogin() {
    if (!username.trim()) { err = '请输入用户名'; return; }
    busy = true; err = '';
    try {
      const user = await kbApi.login(username.trim(), password);
      kbUser.user = user;
      open = false;
      notify('登录成功：' + (user.displayName || user.username));
      onLoginSuccess?.();
    } catch (e: unknown) {
      err = '登录失败：' + e;
    } finally {
      busy = false;
    }
  }

  async function doLogout() {
    try {
      await kbApi.logout();
      kbUser.user = null;
      notify('已退出登录');
    } catch (e: unknown) {
      notify('退出失败：' + e, 'error');
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && open) doLogin();
  }
</script>

<!-- 触发按钮：登录/退出 -->
{#if kbUser.user}
  <Button variant="ghost" size="sm" onclick={doLogout} title="退出登录">
    <KbIcon name="logout" size={13} />退出
  </Button>
{:else}
  <Button variant="ghost" size="sm" onclick={show} title="登录知识库">
    <KbIcon name="login" size={13} />登录
  </Button>
{/if}

<!-- 登录弹窗 -->
{#if open}
  <KbModal {open} onClose={() => { if (!busy) open = false; }} ariaLabel="关闭登录弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="login" size={16} color="var(--kb-accent-bright)" />登录知识库</div>
      <div class="kb-modal-bd">
        <div style="display:flex;flex-direction:column;gap:12px">
          <label class="kb-label">用户名
            <input class="kb-input" placeholder="请输入用户名" bind:value={username}
              onkeydown={onKeydown} autocomplete="username" />
          </label>
          <label class="kb-label">密码
            <input class="kb-input" type="password" placeholder="请输入密码" bind:value={password}
              onkeydown={onKeydown} autocomplete="current-password" />
          </label>
          {#if err}<div class="kb-msg err">{err}</div>{/if}
        </div>
      </div>
      <div class="kb-modal-ft">
        <Button variant="outline" onclick={() => open = false} disabled={busy}>取消</Button>
        <Button onclick={doLogin} disabled={busy}>{busy ? '登录中…' : '登录'}</Button>
      </div>
    </div>
  </KbModal>
{/if}
