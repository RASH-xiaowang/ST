<script lang="ts">
    import ModelSelect from '../../llm/components/ModelSelect.svelte';
    import WechatHoverButton from './WechatHoverButton.svelte';
  import { NativeSelect, NativeSelectOption } from '../../components/ui/native-select';

  let {
    form = $bindable(),
    groups = $bindable([]),
    members = $bindable([]),
    formats = $bindable([]),
    selectGroup = () => {},
    toggleTarget = () => {},
    providerChange = () => {},
  } = $props();
</script>

<div class="ds-form">
  <div class="ds-field">
    <label class="ds-label" for="ds-group">群聊</label>
    <NativeSelect id="ds-group" wrapperClass="w-full" bind:value={form.group_username}
      onchange={() => selectGroup(form.group_username)}>
      <NativeSelectOption value="">选择群聊…</NativeSelectOption>
      {#each groups as g (g.username)}
        <NativeSelectOption value={g.username}>{g.name || g.username}</NativeSelectOption>
      {/each}
    </NativeSelect>
  </div>

  <div class="ds-field">
    <span class="ds-label">关注成员</span>
    <label class="ds-check">
      <input type="checkbox" bind:checked={form.target_all} disabled={!form.group_username}
        onchange={() => { if (form.target_all) form.target_users = []; }} />
      全部成员
    </label>
    {#if !form.target_all && form.group_username}
      <div class="ds-member-chips">
        {#if members.length === 0}
          <span class="ds-member-empty">未读取到群成员，可重新选择群聊重试</span>
        {:else}
          {#each members as m (m.username)}
            <WechatHoverButton
              text={m.name}
              onclick={() => toggleTarget(m.username)}
              class={(form.target_users ?? []).includes(m.username) ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'}
            />
          {/each}
        {/if}
      </div>
    {/if}
    {#if form.target_all}
      <p class="ds-hint">将总结群内全部成员的聊天记录</p>
    {:else if (form.target_users ?? []).length}
      <p class="ds-hint">已关注 {(form.target_users ?? []).length} 位成员</p>
    {/if}
  </div>

  <div class="ds-field">
    <span class="ds-label">模型提供方 / 分析模型</span>
    <ModelSelect
      layout="grid"
      bind:providerId={form.provider_id}
      bind:model={form.model}
      onProviderChange={() => providerChange()}
    />
  </div>

  <div class="ds-field">
    <span class="ds-label">分析格式</span>
    <div class="ds-format-grid">
      {#each formats as f (f.key)}
        <label class="ds-format" class:ds-format-on={form.format === f.key}>
          <input type="radio" name="ds-format" value={f.key} bind:group={form.format} />
          <span class="ds-format-name">{f.label}</span>
          <span class="ds-format-desc">{f.desc}</span>
        </label>
      {/each}
    </div>
  </div>

  {#if form.format === 'custom'}
    <div class="ds-field">
      <label class="ds-label" for="ds-prompt">自定义提示词模板</label>
      <textarea
        id="ds-prompt"
        class="ds-textarea"
        rows={4}
        bind:value={form.custom_prompt}
        placeholder={"支持占位符：{date} {group} {targets}；例如：请用表格形式总结 {group} 在 {date} 的聊天，成员：{targets}"}
      ></textarea>
    </div>
  {/if}

  <div class="ds-grid">
    <div class="ds-field">
      <label class="ds-label" for="ds-time">每日定时时间</label>
      <input id="ds-time" type="time" class="ds-select ds-time" bind:value={form.schedule_time} />
      <p class="ds-hint">到点自动总结前一天的聊天记录</p>
    </div>
    <div class="ds-field">
    <span class="ds-label">定时开关</span>
      <label class="ds-check">
        <input type="checkbox" bind:checked={form.enabled} />
        {form.enabled ? '已启用' : '已暂停'}
      </label>
    </div>
  </div>
</div>

<style>
  .ds-form { display: flex; flex-direction: column; gap: 13px; }
  .ds-field { display: flex; flex-direction: column; gap: 6px; }
  .ds-label { font-size: 11.5px; font-weight: 700; color: var(--wc-text2); }
  .ds-time { width: 140px; }
  .ds-textarea { padding: 9px 11px; border-radius: 8px; border: 1px solid var(--wc-border); background: var(--wc-bg2); color: var(--wc-text); font-size: 12.5px; line-height: 1.6; resize: vertical; outline: none; font-family: inherit; }
  .ds-textarea:focus { border-color: var(--wc-theme,#576b95); }
  .ds-hint { font-size: 11.5px; color: var(--wc-muted); margin: 0; line-height: 1.6; }
  .ds-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .ds-check { display: inline-flex; align-items: center; gap: 7px; font-size: 12.5px; color: var(--wc-text); cursor: pointer; }
  .ds-check input { accent-color: var(--wc-theme,#576b95); width: 15px; height: 15px; cursor: pointer; }
  .ds-member-chips { display: flex; flex-wrap: wrap; gap: 6px; max-height: 132px; overflow-y: auto; padding: 2px; }
  .ds-member-empty { font-size: 11.5px; color: var(--wc-muted); }
  .ds-format-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 8px; }
  .ds-format { display: flex; flex-direction: column; gap: 3px; padding: 10px 12px; border: 1px solid var(--wc-border); border-radius: 10px; background: var(--wc-bg2); cursor: pointer; transition: all .15s ease; }
  .ds-format:hover { border-color: var(--wc-text2); }
  .ds-format-on { border-color: var(--wc-theme,#576b95); background: color-mix(in srgb, var(--wc-theme,#576b95) 8%, transparent); }
  .ds-format input { display: none; }
  .ds-format-name { font-size: 12.5px; font-weight: 700; color: var(--wc-text); }
  .ds-format-desc { font-size: 11.5px; color: var(--wc-muted); line-height: 1.5; }
</style>
