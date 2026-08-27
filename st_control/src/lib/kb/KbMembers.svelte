<script lang="ts">
  /**
   * 知识库成员管理组件
   * 功能：查看成员列表、添加成员、修改角色、移除成员
   * 权限：仅知识库 owner/admin 可操作
   */
  import { kbApi } from './services/ipc';
  import { kbConfirm } from './KbConfirm.svelte';
  import type { MemberItem, UserItem } from './kbTypes';
  import KbIcon from './KbIcon.svelte';
  import KbModal from './KbModal.svelte';
  import { Button } from '../components/ui/button';
  import { Badge } from '../components/ui/badge';
  import { Empty, EmptyTitle } from '../components/ui/empty';
  import { Skeleton } from '../components/ui/skeleton';

  interface Props {
    kbId: number;
    isAdmin: boolean;
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
  }
  let { kbId, isAdmin, notify }: Props = $props();

  let members = $state<MemberItem[]>([]);
  let users = $state<UserItem[]>([]);
  let loading = $state(false);
  let err = $state('');

  // 添加成员弹窗
  let addOpen = $state(false);
  let addUserId = $state<number | null>(null);
  let addRole = $state('viewer');
  let addBusy = $state(false);
  let addErr = $state('');

  const ROLES = [
    { value: 'admin', label: '管理员', desc: '可管理成员和文档' },
    { value: 'editor', label: '编辑者', desc: '可编辑文档' },
    { value: 'viewer', label: '查看者', desc: '仅可查看' },
  ];

  async function loadMembers() {
    loading = true; err = '';
    try {
      members = await kbApi.listMembers(kbId);
    } catch (e: unknown) {
      err = '加载成员失败：' + e;
    } finally {
      loading = false;
    }
  }

  async function loadUsers() {
    try {
      users = await kbApi.listUsers();
    } catch {
      users = [];
    }
  }

  function openAdd() {
    addUserId = null;
    addRole = 'viewer';
    addErr = '';
    addBusy = false;
    addOpen = true;
    loadUsers();
  }

  async function doAdd() {
    if (addUserId === null) { addErr = '请选择用户'; return; }
    addBusy = true; addErr = '';
    try {
      await kbApi.addMember(kbId, addUserId, addRole);
      addOpen = false;
      await loadMembers();
      notify('成员已添加');
    } catch (e: unknown) {
      addErr = '添加失败：' + e;
    } finally {
      addBusy = false;
    }
  }

  async function changeRole(userId: number, newRole: string) {
    try {
      await kbApi.updateMemberRole(kbId, userId, newRole);
      await loadMembers();
      notify('角色已更新');
    } catch (e: unknown) {
      notify('修改角色失败：' + e, 'error');
    }
  }

  async function removeMember(userId: number, username: string) {
    if (!await kbConfirm({
      title: '移除成员',
      message: `确定将「${username}」移出该知识库？`,
      danger: true,
      confirmText: '移除',
    })) return;
    try {
      await kbApi.removeMember(kbId, userId);
      await loadMembers();
      notify('成员已移除');
    } catch (e: unknown) {
      notify('移除失败：' + e, 'error');
    }
  }

  function roleLabel(r: string): string {
    return ROLES.find(x => x.value === r)?.label ?? r;
  }

  // 初始加载
  $effect(() => { kbId; loadMembers(); });
</script>

<div class="kb-members">
  <div class="kb-members-hd">
    <h3 class="kb-members-title"><KbIcon name="users" size={16} />知识库成员</h3>
    {#if isAdmin}
      <Button size="sm" onclick={openAdd}><KbIcon name="plus" size={12} weight="bold" />添加成员</Button>
    {/if}
  </div>

  {#if err}
    <div class="kb-msg err">{err}</div>
  {/if}

  {#if loading}
    <div class="flex flex-col gap-2 p-2">
      {#each Array(3) as _}
        <Skeleton class="h-[48px] rounded-lg" />
      {/each}
    </div>
  {:else if members.length === 0}
    <Empty class="min-h-[100px] p-4">
      <KbIcon name="users" size={20} color="var(--kb-text-3)" />
      <EmptyTitle class="text-sm">暂无成员</EmptyTitle>
    </Empty>
  {:else}
    <div class="kb-members-list">
      {#each members as m}
        <div class="kb-member-row">
          <span class="kb-member-avatar">{m.username.charAt(0).toUpperCase()}</span>
          <div class="kb-member-info">
            <span class="kb-member-name">{m.displayName || m.username}</span>
            <span class="kb-member-user">@{m.username}</span>
          </div>
          <div class="kb-member-role">
            {#if m.role === 'owner'}
              <Badge variant="default">所有者</Badge>
            {:else if isAdmin}
              <select class="kb-select-sm" value={m.role} onchange={(e) => changeRole(m.userId, e.currentTarget.value)}>
                {#each ROLES as r}
                  <option value={r.value}>{r.label}</option>
                {/each}
              </select>
            {:else}
              <Badge variant="secondary">{roleLabel(m.role)}</Badge>
            {/if}
          </div>
          {#if isAdmin && m.role !== 'owner'}
            <Button variant="ghost" size="icon-sm" onclick={() => removeMember(m.userId, m.username)} title="移除成员">
              <KbIcon name="close" size={12} />
            </Button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- 添加成员弹窗 -->
{#if addOpen}
  <KbModal open={addOpen} onClose={() => { if (!addBusy) addOpen = false; }} ariaLabel="关闭添加成员弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="userPlus" size={16} color="var(--kb-accent-bright)" />添加成员</div>
      <div class="kb-modal-bd">
        <div style="display:flex;flex-direction:column;gap:12px">
          <label class="kb-label">选择用户
            <select class="kb-input" bind:value={addUserId}>
              <option value={null}>请选择…</option>
              {#each users.filter(u => !members.some(m => m.userId === u.id)) as u}
                <option value={u.id}>{u.displayName || u.username} (@{u.username}){u.isAdmin ? ' · 管理员' : ''}</option>
              {/each}
            </select>
          </label>
          <label class="kb-label">角色
            <select class="kb-input" bind:value={addRole}>
              {#each ROLES as r}
                <option value={r.value}>{r.label} — {r.desc}</option>
              {/each}
            </select>
          </label>
          {#if addErr}<div class="kb-msg err">{addErr}</div>{/if}
        </div>
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn" onclick={() => addOpen = false} disabled={addBusy}>取消</button>
        <button class="kb-btn-md" onclick={doAdd} disabled={addBusy}>{addBusy ? '添加中…' : '确认添加'}</button>
      </div>
    </div>
  </KbModal>
{/if}

<style>
  .kb-members { display: flex; flex-direction: column; gap: 12px; }
  .kb-members-hd { display: flex; align-items: center; justify-content: space-between; }
  .kb-members-title { font-size: 14px; font-weight: 600; margin: 0; display: flex; align-items: center; gap: 6px; }
  .kb-members-list { display: flex; flex-direction: column; gap: 4px; }
  .kb-member-row {
    display: flex; align-items: center; gap: 10px;
    padding: 10px 12px; border: 1px solid var(--kb-border); border-radius: 8px;
    transition: background .12s;
  }
  .kb-member-row:hover { background: var(--kb-hover); }
  .kb-member-avatar {
    width: 32px; height: 32px; border-radius: 50%;
    background: var(--kb-hover-strong); color: var(--kb-accent-bright);
    display: grid; place-items: center; font-weight: 700; font-size: 14px; flex: none;
  }
  .kb-member-info { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .kb-member-name { font-size: 13px; font-weight: 500; color: var(--kb-text); }
  .kb-member-user { font-size: 11.5px; color: var(--kb-text-3); }
  .kb-member-role { flex: none; }
  .kb-select-sm {
    padding: 3px 8px; border: 1px solid var(--kb-border); border-radius: 6px;
    background: var(--kb-card); font-size: 12px; color: var(--kb-text); cursor: pointer;
  }
</style>
