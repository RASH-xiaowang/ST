<script lang="ts">
  /**
   * 用户管理组件（仅全局管理员可用）
   * 功能：查看用户列表、创建用户、重置密码、设置管理员、删除用户
   */
  import { kbApi } from './services/ipc';
  import { kbConfirm } from './KbConfirm.svelte';
  import type { UserItem } from './kbTypes';
  import KbIcon from './KbIcon.svelte';
  import KbModal from './KbModal.svelte';
  import { Button } from '../components/ui/button';
  import { Badge } from '../components/ui/badge';
  import { Empty, EmptyTitle } from '../components/ui/empty';
  import { Skeleton } from '../components/ui/skeleton';

  interface Props {
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
    hideHeader?: boolean;
  }
  let { notify, hideHeader = false }: Props = $props();

  let users = $state<UserItem[]>([]);
  let loading = $state(false);
  let err = $state('');

  // 创建用户弹窗
  let createOpen = $state(false);
  let newUsername = $state('');
  let newDisplayName = $state('');
  let newPassword = $state('');
  let createBusy = $state(false);
  let createErr = $state('');

  // 重置密码弹窗
  let resetOpen = $state(false);
  let resetTarget = $state<UserItem | null>(null);
  let resetPassword = $state('');
  let resetBusy = $state(false);
  let resetErr = $state('');

  async function loadUsers() {
    loading = true; err = '';
    try {
      users = await kbApi.listUsers();
    } catch (e: unknown) {
      err = '加载用户失败：' + e;
    } finally {
      loading = false;
    }
  }

  function openCreate() {
    newUsername = ''; newDisplayName = ''; newPassword = '';
    createErr = ''; createBusy = false; createOpen = true;
  }

  async function doCreate() {
    if (!newUsername.trim()) { createErr = '请输入用户名'; return; }
    if (!newPassword.trim()) { createErr = '请输入密码'; return; }
    createBusy = true; createErr = '';
    try {
      await kbApi.invoke<number>('kb_create_user', {
        username: newUsername.trim(),
        displayName: newDisplayName.trim() || null,
        password: newPassword,
      });
      createOpen = false;
      await loadUsers();
      notify('用户已创建：' + newUsername.trim());
    } catch (e: unknown) {
      createErr = '创建失败：' + e;
    } finally {
      createBusy = false;
    }
  }

  function openReset(user: UserItem) {
    resetTarget = user;
    resetPassword = ''; resetErr = ''; resetBusy = false; resetOpen = true;
  }

  async function doReset() {
    if (!resetTarget || !resetPassword.trim()) { resetErr = '请输入新密码'; return; }
    resetBusy = true; resetErr = '';
    try {
      await kbApi.invoke<void>('kb_reset_password', {
        userId: resetTarget.id,
        newPassword: resetPassword,
      });
      resetOpen = false;
      notify('密码已重置：' + (resetTarget.displayName || resetTarget.username));
    } catch (e: unknown) {
      resetErr = '重置失败：' + e;
    } finally {
      resetBusy = false;
    }
  }

  async function toggleAdmin(user: UserItem) {
    const newStatus = !user.isAdmin;
    const action = newStatus ? '设为管理员' : '取消管理员';
    if (!await kbConfirm({
      title: action,
      message: `确定将「${user.displayName || user.username}」${action}？`,
    })) return;
    try {
      await kbApi.invoke<void>('kb_set_admin', { userId: user.id, isAdmin: newStatus });
      await loadUsers();
      notify(action + '成功');
    } catch (e: unknown) {
      notify(action + '失败：' + e, 'error');
    }
  }

  async function deleteUser(user: UserItem) {
    if (!await kbConfirm({
      title: '删除用户',
      message: `确定删除用户「${user.displayName || user.username}」？该操作不可恢复。`,
      danger: true,
      confirmText: '删除',
    })) return;
    try {
      await kbApi.invoke<void>('kb_delete_user', { userId: user.id });
      await loadUsers();
      notify('用户已删除');
    } catch (e: unknown) {
      notify('删除失败：' + e, 'error');
    }
  }

  // 初始加载
  loadUsers();
</script>

<div class="kb-users">
  {#if !hideHeader}
  <div class="kb-users-hd">
    <h3 class="kb-users-title"><KbIcon name="users" size={16} />用户管理</h3>
    <Button size="sm" onclick={openCreate}><KbIcon name="plus" size={12} weight="bold" />创建用户</Button>
  </div>
  {:else}
  <div class="kb-users-hd">
    <div></div>
    <Button size="sm" onclick={openCreate}><KbIcon name="plus" size={12} weight="bold" />创建用户</Button>
  </div>
  {/if}

  {#if err}
    <div class="kb-msg err">{err}</div>
  {/if}

  {#if loading}
    <div class="flex flex-col gap-2 p-2">
      {#each Array(4) as _}
        <Skeleton class="h-[48px] rounded-lg" />
      {/each}
    </div>
  {:else if users.length === 0}
    <Empty class="min-h-[100px] p-4">
      <KbIcon name="users" size={20} color="var(--kb-text-3)" />
      <EmptyTitle class="text-sm">暂无用户</EmptyTitle>
    </Empty>
  {:else}
    <div class="kb-users-list">
      {#each users as u}
        <div class="kb-user-row">
          <span class="kb-user-avatar" class:admin={u.isAdmin}>{u.username.charAt(0).toUpperCase()}</span>
          <div class="kb-user-info">
            <span class="kb-user-name">{u.displayName || u.username}</span>
            <span class="kb-user-username">@{u.username}</span>
          </div>
          <div class="kb-user-badges">
            {#if u.isAdmin}
              <Badge variant="default" class="text-[10px]">管理员</Badge>
            {/if}
            <Badge variant="outline" class="text-[10px]">ID: {u.id}</Badge>
          </div>
          <div class="kb-user-actions">
            <Button variant="ghost" size="icon-sm" onclick={() => openReset(u)} title="重置密码">
              <KbIcon name="key" size={12} />
            </Button>
            <Button variant="ghost" size="icon-sm" onclick={() => toggleAdmin(u)} title={u.isAdmin ? '取消管理员' : '设为管理员'}>
              <KbIcon name={u.isAdmin ? 'shieldOff' : 'shield'} size={12} />
            </Button>
            <Button variant="ghost" size="icon-sm" onclick={() => deleteUser(u)} title="删除用户">
              <KbIcon name="trash" size={12} />
            </Button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- 创建用户弹窗 -->
{#if createOpen}
  <KbModal open={createOpen} onClose={() => { if (!createBusy) createOpen = false; }} ariaLabel="关闭创建用户弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="userPlus" size={16} color="var(--kb-accent-bright)" />创建用户</div>
      <div class="kb-modal-bd">
        <div style="display:flex;flex-direction:column;gap:12px">
          <label class="kb-label">用户名 *
            <input class="kb-input" placeholder="请输入用户名" bind:value={newUsername} maxlength="50" />
          </label>
          <label class="kb-label">显示名（可选）
            <input class="kb-input" placeholder="中文显示名" bind:value={newDisplayName} maxlength="50" />
          </label>
          <label class="kb-label">密码 *
            <input class="kb-input" type="password" placeholder="请输入密码" bind:value={newPassword} />
          </label>
          {#if createErr}<div class="kb-msg err">{createErr}</div>{/if}
        </div>
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn" onclick={() => createOpen = false} disabled={createBusy}>取消</button>
        <button class="kb-btn-md" onclick={doCreate} disabled={createBusy}>{createBusy ? '创建中…' : '创建'}</button>
      </div>
    </div>
  </KbModal>
{/if}

<!-- 重置密码弹窗 -->
{#if resetOpen && resetTarget}
  <KbModal open={resetOpen} onClose={() => { if (!resetBusy) resetOpen = false; }} ariaLabel="关闭重置密码弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="key" size={16} color="var(--kb-accent-bright)" />重置密码</div>
      <div class="kb-modal-bd">
        <p style="font-size:13px;margin:0 0 12px">为「{resetTarget.displayName || resetTarget.username}」设置新密码：</p>
        <label class="kb-label">新密码
          <input class="kb-input" type="password" placeholder="请输入新密码" bind:value={resetPassword} />
        </label>
        {#if resetErr}<div class="kb-msg err" style="margin-top:8px">{resetErr}</div>{/if}
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn" onclick={() => resetOpen = false} disabled={resetBusy}>取消</button>
        <button class="kb-btn-md" onclick={doReset} disabled={resetBusy}>{resetBusy ? '重置中…' : '确认重置'}</button>
      </div>
    </div>
  </KbModal>
{/if}

<style>
  .kb-users { display: flex; flex-direction: column; gap: 12px; }
  .kb-users-hd { display: flex; align-items: center; justify-content: space-between; }
  .kb-users-title { font-size: 14px; font-weight: 600; margin: 0; display: flex; align-items: center; gap: 6px; }
  .kb-users-list { display: flex; flex-direction: column; gap: 4px; }
  .kb-user-row {
    display: flex; align-items: center; gap: 10px;
    padding: 10px 12px; border: 1px solid var(--kb-border); border-radius: 8px;
    transition: background .12s;
  }
  .kb-user-row:hover { background: var(--kb-hover); }
  .kb-user-avatar {
    width: 32px; height: 32px; border-radius: 50%;
    background: var(--kb-hover-strong); color: var(--kb-accent-bright);
    display: grid; place-items: center; font-weight: 700; font-size: 14px; flex: none;
  }
  .kb-user-avatar.admin { background: color-mix(in srgb, var(--app-success) 14%, transparent); color: var(--app-success); }
  .kb-user-info { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .kb-user-name { font-size: 13px; font-weight: 500; color: var(--kb-text); }
  .kb-user-username { font-size: 11.5px; color: var(--kb-text-3); }
  .kb-user-badges { display: flex; gap: 4px; }
  .kb-user-actions { display: flex; gap: 4px; }
</style>
