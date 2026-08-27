<script lang="ts">
  /**
   * ACL 权限管理组件
   * 功能：查看/添加/删除对象级权限规则
   */
  import { kbApi } from './services/ipc';
  import { kbConfirm } from './KbConfirm.svelte';
  import KbIcon from './KbIcon.svelte';
  import { Button } from '../components/ui/button';
  import { Empty, EmptyTitle } from '../components/ui/empty';
  import { Skeleton } from '../components/ui/skeleton';

  interface Props {
    kbId: number;
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
  }
  let { kbId, notify }: Props = $props();

  let acls = $state<Record<string, unknown>[]>([]);
  let loading = $state(false);
  let err = $state('');

  // 添加规则
  let addOpen = $state(false);
  let newScope = $state('kb');
  let newGranteeType = $state('user');
  let newUserId = $state('');
  let newEffect = $state('allow');
  let addBusy = $state(false);
  let addErr = $state('');

  async function loadAcls() {
    loading = true; err = '';
    try {
      // 防御：IPC 异常时后端可能返回 null，避免 acls 为 null 导致渲染崩溃（acls.length）
      acls = (await kbApi.getAcl(kbId)) ?? [];
    } catch (e: unknown) {
      err = '加载 ACL 失败：' + e;
    } finally {
      loading = false;
    }
  }

  function openAdd() {
    newScope = 'kb'; newGranteeType = 'user'; newUserId = '';
    newEffect = 'allow'; addErr = ''; addBusy = false; addOpen = true;
  }

  async function doAdd() {
    addBusy = true; addErr = '';
    try {
      await kbApi.setAcl({
        scope: newScope,
        kbId,
        granteeType: newGranteeType,
        userId: newUserId ? Number(newUserId) : undefined,
        effect: newEffect,
      });
      addOpen = false;
      await loadAcls();
      notify('ACL 规则已添加');
    } catch (e: unknown) {
      addErr = '添加失败：' + e;
    } finally {
      addBusy = false;
    }
  }

  async function deleteAcl(scope: string, granteeType: string, userId: number | null) {
    if (!await kbConfirm({ title: '删除 ACL 规则', message: '确定删除该权限规则？', danger: true, confirmText: '删除' })) return;
    try {
      await kbApi.deleteAcl({
        scope,
        kbId,
        granteeType,
        userId: userId ?? undefined,
      });
      await loadAcls();
      notify('ACL 规则已删除');
    } catch (e: unknown) {
      notify('删除失败：' + e, 'error');
    }
  }

  function scopeLabel(s: string): string {
    return s === 'kb' ? '知识库' : s === 'document' ? '文档' : s === 'folder' ? '文件夹' : s;
  }

  function effectLabel(e: string): string {
    return e === 'allow' ? '允许' : e === 'deny' ? '拒绝' : e;
  }

  $effect(() => { kbId; loadAcls(); });
</script>

<div class="kb-acl">
  <div class="kb-acl-hd">
    <h3 class="kb-acl-title"><KbIcon name="shield" size={16} />ACL 权限规则</h3>
    <Button size="sm" onclick={openAdd}><KbIcon name="plus" size={12} weight="bold" />添加规则</Button>
  </div>
  <p class="kb-acl-hint">对象级 ACL 规则，支持按用户或角色授权，deny 优先。</p>

  {#if err}<div class="kb-msg err">{err}</div>{/if}

  {#if loading}
    <div class="flex flex-col gap-2 p-2">
      {#each Array(3) as _}
        <Skeleton class="h-[40px] rounded-lg" />
      {/each}
    </div>
  {:else if acls.length === 0}
    <Empty class="min-h-[100px] p-4">
      <KbIcon name="shield" size={20} color="var(--kb-text-3)" />
      <EmptyTitle class="text-sm">暂无 ACL 规则</EmptyTitle>
    </Empty>
  {:else}
    <div class="kb-acl-list">
      {#each acls as acl}
        <div class="kb-acl-item">
          <span class="kb-acl-scope">{scopeLabel(String(acl.scope || ''))}</span>
          <span class="kb-acl-grantee">{String(acl.granteeType || '')}{acl.userId ? `#${acl.userId}` : ''}</span>
          <span class="kb-acl-effect" class:allow={acl.effect === 'allow'} class:deny={acl.effect === 'deny'}>
            {effectLabel(String(acl.effect || ''))}
          </span>
          <span class="kb-acl-time">{String(acl.createdAt || '')}</span>
          <button class="kb-btn-sm kb-dang" onclick={() => deleteAcl(String(acl.scope || ''), String(acl.granteeType || ''), acl.userId ? Number(acl.userId) : null)} title="删除">
            <KbIcon name="trash" size={12} />
          </button>
        </div>
      {/each}
    </div>
  {/if}
</div>

{#if addOpen}
  <div class="kb-modal-overlay" onclick={() => { if (!addBusy) addOpen = false; }} onkeydown={(e) => e.key === 'Escape' && (addOpen = false)} role="dialog" aria-modal="true" tabindex="-1">
    <div class="kb-modal-box" role="presentation" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="kb-modal-hd"><KbIcon name="shield" size={16} color="var(--kb-accent-bright)" />添加 ACL 规则</div>
      <div class="kb-modal-bd">
        <div style="display:flex;flex-direction:column;gap:12px">
          <label class="kb-label">作用域
            <select class="kb-select" bind:value={newScope}>
              <option value="kb">知识库</option>
              <option value="document">文档</option>
              <option value="folder">文件夹</option>
            </select>
          </label>
          <label class="kb-label">授权类型
            <select class="kb-select" bind:value={newGranteeType}>
              <option value="user">用户</option>
              <option value="role">角色</option>
              <option value="public">公开</option>
            </select>
          </label>
          {#if newGranteeType === 'user'}
            <label class="kb-label">用户 ID
              <input class="kb-input" type="number" bind:value={newUserId} placeholder="用户 ID" />
            </label>
          {/if}
          <label class="kb-label">效果
            <select class="kb-select" bind:value={newEffect}>
              <option value="allow">允许</option>
              <option value="deny">拒绝</option>
            </select>
          </label>
          {#if addErr}<div class="kb-msg err">{addErr}</div>{/if}
        </div>
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn" onclick={() => addOpen = false} disabled={addBusy}>取消</button>
        <button class="kb-btn-md" onclick={doAdd} disabled={addBusy}>{addBusy ? '添加中…' : '添加'}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .kb-acl { display: flex; flex-direction: column; gap: 12px; }
  .kb-acl-hd { display: flex; align-items: center; justify-content: space-between; }
  .kb-acl-title { font-size: 14px; font-weight: 600; margin: 0; display: flex; align-items: center; gap: 6px; }
  .kb-acl-hint { font-size: 12px; color: var(--kb-text-3); margin: 0; }
  .kb-acl-list { display: flex; flex-direction: column; gap: 4px; max-height: 300px; overflow-y: auto; }
  .kb-acl-item { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border: 1px solid var(--kb-border); border-radius: 6px; font-size: 12px; }
  .kb-acl-scope { font-weight: 600; }
  .kb-acl-grantee { color: var(--kb-text-2); }
  .kb-acl-effect { padding: 1px 6px; border-radius: 4px; font-weight: 600; }
  .kb-acl-effect.allow { background: color-mix(in srgb, var(--app-success) 14%, transparent); color: var(--app-success); }
  .kb-acl-effect.deny { background: color-mix(in srgb, var(--app-danger) 14%, transparent); color: var(--app-danger); }
  .kb-acl-time { color: var(--kb-text-3); margin-left: auto; }
  .kb-modal-overlay { position: fixed; inset: 0; z-index: 100; background: rgba(0,0,0,0.4); display: grid; place-items: center; }
  .kb-modal-box { background: var(--app-bg-color); border: 1px solid var(--kb-border); border-radius: 12px; width: min(400px, 90vw); }
  .kb-modal-hd { display: flex; align-items: center; gap: 8px; padding: 16px; border-bottom: 1px solid var(--kb-border-subtle); font-size: 14px; font-weight: 600; }
  .kb-modal-bd { padding: 16px; }
  .kb-modal-ft { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--kb-border-subtle); }
</style>
