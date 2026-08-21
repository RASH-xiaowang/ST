<script lang="ts">
  import KbIcon from './KbIcon.svelte';

  export interface KbSelectItem {
    value: string | number;
    label: string;
    meta?: string;
  }

  interface Props {
    items: KbSelectItem[];
    value: string | number | null;
    onchange: (v: string | number) => void;
    placeholder?: string;
    disabled?: boolean;
    icon?: string;
    style?: string;
  }
  let { items, value, onchange, placeholder = '请选择…', disabled = false, icon = '', style = '' }: Props = $props();

  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);
  let pos = $state({ top: 0, left: 0, width: 0 });
  let hi = $state(0);

  const curLabel = $derived(items.find((i) => i.value === value)?.label ?? placeholder);

  function openMenu() {
    if (disabled || open) return;
    const r = triggerEl?.getBoundingClientRect();
    if (!r) return;
    pos = { top: r.bottom + 6, left: r.left, width: Math.max(r.width, 180) };
    hi = Math.max(0, items.findIndex((i) => i.value === value));
    open = true;
  }
  function close() {
    open = false;
  }
  function pick(v: string | number) {
    close();
    onchange(v);
  }

  // 打开期间：点击外部 / Esc 关闭；窗口滚动或缩放时收起（避免选项错位）
  $effect(() => {
    if (!open) return;
    const onDocPointer = (e: PointerEvent) => {
      if (!triggerEl?.contains(e.target as Node) && !menuEl?.contains(e.target as Node)) close();
    };
    const onDocKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
    };
    const onScroll = (e: Event) => {
      if (e.target instanceof Node && menuEl?.contains(e.target)) return;
      close();
    };
    document.addEventListener('pointerdown', onDocPointer, true);
    document.addEventListener('keydown', onDocKey, true);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('resize', onScroll);
    return () => {
      document.removeEventListener('pointerdown', onDocPointer, true);
      document.removeEventListener('keydown', onDocKey, true);
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('resize', onScroll);
    };
  });

  function onTriggerKey(e: KeyboardEvent) {
    if (disabled) return;
    if (!open && (e.key === 'ArrowDown' || e.key === 'Enter' || e.key === ' ')) {
      e.preventDefault();
      openMenu();
    } else if (open && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault();
      const n = items.length;
      if (n === 0) return;
      hi = e.key === 'ArrowDown' ? (hi + 1) % n : (hi - 1 + n) % n;
      scrollToHi();
    } else if (open && e.key === 'Enter') {
      e.preventDefault();
      const it = items[hi];
      if (it) pick(it.value);
    } else if (open && e.key === 'Tab') {
      close();
    }
  }
  function scrollToHi() {
    menuEl?.querySelector(`[data-idx="${hi}"]`)?.scrollIntoView({ block: 'nearest' });
  }
</script>

<div class="kb-dselect" style={style}>
  <button class="kb-select-trigger" class:open={open} bind:this={triggerEl} type="button" disabled={disabled}
    onclick={() => (open ? close() : openMenu())}
    onkeydown={onTriggerKey}
    aria-haspopup="listbox" aria-expanded={open}>
    {#if icon}<KbIcon name={icon} size={14} />{/if}
    <span class="kb-select-label" class:muted={!items.some((i) => i.value === value)}>{curLabel}</span>
    <KbIcon name="caretDown" size={12} class={`kb-caret${open ? ' kb-caret-open' : ''}`} />
  </button>

  {#if open}
    <div class="kb-select-menu" bind:this={menuEl} role="listbox" aria-label={placeholder}
      style="top:{pos.top}px;left:{pos.left}px;min-width:{pos.width}px">
      {#each items as it, i (it.value)}
        <button class="kb-select-option" class:active={it.value === value} class:hover={hi === i} data-idx={i} type="button"
          role="option" aria-selected={it.value === value}
          onmouseenter={() => (hi = i)}
          onclick={() => pick(it.value)}>
          <span class="kb-select-option-label">{it.label}</span>
          {#if it.meta}<span class="kb-select-option-meta">{it.meta}</span>{/if}
          {#if it.value === value}<KbIcon name="check" size={13} />{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .kb-dselect {
    position: relative;
    display: inline-flex;
  }
  .kb-select-trigger {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 34px;
    padding: 0 10px;
    background: linear-gradient(180deg, color-mix(in srgb, var(--kb-surface) 94%, #ffffff 4%), color-mix(in srgb, var(--kb-surface) 86%, #000000 5%));
    border: 1px solid var(--kb-border);
    border-radius: 8px;
    color: var(--kb-text);
    font-family: inherit;
    font-size: 13px;
    cursor: pointer;
    box-shadow: 0 1px 2px rgb(0 0 0 / .18);
    transition: border-color .15s, box-shadow .15s, background .15s;
  }
  .kb-select-trigger:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--kb-accent) 35%, var(--kb-border));
    background: color-mix(in srgb, var(--kb-surface) 92%, var(--kb-accent) 6%);
  }
  .kb-select-trigger:focus-visible, .kb-select-trigger.open {
    outline: none;
    border-color: color-mix(in srgb, var(--kb-accent) 55%, var(--kb-border));
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--kb-accent) 25%, transparent),
      0 0 0 3px color-mix(in srgb, var(--kb-accent) 16%, transparent),
      0 2px 10px -3px color-mix(in srgb, var(--kb-accent) 30%, transparent);
  }
  .kb-select-trigger:disabled { opacity: .55; cursor: not-allowed; }
  .kb-select-label {
    flex: 1;
    min-width: 0;
    color: var(--kb-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: left;
  }
  .kb-select-label.muted { color: var(--kb-text-3); }
  :global(.kb-caret) { transition: transform .18s ease; }
  :global(.kb-caret-open) { transform: rotate(180deg); }

  .kb-select-menu {
    position: fixed;
    z-index: 1200;
    background: color-mix(in srgb, var(--kb-surface-2) 92%, transparent);
    backdrop-filter: blur(14px);
    border: 1px solid color-mix(in srgb, var(--kb-accent) 22%, var(--kb-border-strong));
    border-radius: 12px;
    box-shadow: 0 16px 44px -12px rgb(0 0 0 / .55), 0 4px 14px -6px rgb(0 0 0 / .4);
    padding: 6px;
    max-height: 264px;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--kb-border-strong) transparent;
    transform-origin: top center;
    animation: kb-pop .14s ease-out;
  }
  @keyframes kb-pop {
    from { opacity: 0; transform: translateY(-4px) scale(.98); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }
  .kb-select-option {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--kb-text);
    font-family: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    transition: background .12s, color .12s, box-shadow .12s;
  }
  .kb-select-option:hover, .kb-select-option.hover {
    background: color-mix(in srgb, var(--kb-accent) 12%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--kb-accent) 22%, transparent);
  }
  .kb-select-option.active {
    color: var(--kb-accent-bright);
    font-weight: 600;
    background: color-mix(in srgb, var(--kb-accent) 8%, transparent);
  }
  .kb-select-option-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kb-select-option-meta { font-size: 11.5px; color: var(--kb-text-3); }
</style>
