<!-- 个性化设置面板：字体 / 背景 / 文本颜色 / 透明度（自包含组件，从 App.svelte 抽出） -->
<script lang="ts">
  import { onMount } from 'svelte';
  import CheckIcon from "@lucide/svelte/icons/check";
  import { hexLum, hexToRgba, swatchSubColor, swatchTextColor } from './colorUtils';
  import { lsGet, lsSet } from '../storage';

  const STORAGE_KEY = 'st_control_prefs';

  let prefFontSize = $state(14);
  let prefFontStyle = $state('default');
  let prefBgTheme = $state('bench');
  let prefFgTheme = $state('ink');
  let prefBgOpacity = $state(100);
  let prefFgOpacity = $state(100);

  const FONT_STYLES: Record<string, { label: string; desc: string; family: string }> = {
    default:  { label: '系统默认',  desc: '系统默认字体',  family: '-apple-system, "PingFang SC", "Microsoft YaHei", "Helvetica Neue", sans-serif' },
    noto:     { label: '思源黑体',  desc: 'Noto Sans SC — 现代无衬线', family: '"Noto Sans SC", -apple-system, sans-serif' },
    serif:    { label: '思源宋体',  desc: 'Noto Serif SC — 优雅衬线', family: '"Noto Serif SC", "Source Han Serif SC", Georgia, serif' },
    wenkai:   { label: '霞鹜文楷',  desc: 'LXGW WenKai — 温润手写',  family: '"LXGW WenKai", "KaiTi", serif' },
    zcool:    { label: '站酷快乐',  desc: 'ZCOOL KuaiLe — 活泼圆体', family: '"ZCOOL KuaiLe", cursive, sans-serif' },
    harmony:  { label: '鸿蒙 Sans', desc: 'HarmonyOS Sans — 系统原生', family: '"HarmonyOS Sans SC", "Noto Sans SC", -apple-system, sans-serif' },
    inter:    { label: 'Inter',     desc: 'Inter — 西文优先',  family: '"Inter", -apple-system, sans-serif' },
    jetbrains:{ label: '等宽字体',  desc: 'JetBrains Mono — 代码风格',  family: '"JetBrains Mono", "Fira Code", Consolas, monospace' },
  };

  const BG_THEMES: Record<string, { label: string; bgColor: string; cardBg: string }> = {
    bench: { label: '仪表台',    bgColor: '#ecebe7', cardBg: '#f7f6f2' },
    'bench-dark': { label: '仪表台深色', bgColor: '#16181d', cardBg: '#1f2229' },
    herb:  { label: '标本纸',    bgColor: '#f2f7f4', cardBg: '#ffffff' },
    oled:  { label: 'OLED 墨金', bgColor: '#020617', cardBg: '#0d1525' },
    mojin: { label: '墨金',      bgColor: '#0a0f1e', cardBg: '#141c2e' },
    deep:  { label: '深海蓝',    bgColor: '#061228', cardBg: '#0f1f3a' },
    darkp: { label: '暗紫',      bgColor: '#120f24', cardBg: '#1f1a35' },
    darkc: { label: '深青',      bgColor: '#061a1c', cardBg: '#0f2628' },
    graph: { label: '石墨灰',    bgColor: '#141518', cardBg: '#1f2124' },
    warmb: { label: '暖棕',      bgColor: '#1a130c', cardBg: '#2a1f14' },
    burg:  { label: '勃艮第',    bgColor: '#1a0e14', cardBg: '#2a1820' },
    fore:  { label: '森林绿',    bgColor: '#071410', cardBg: '#0f2018' },
    mid:   { label: '午夜',      bgColor: '#030710', cardBg: '#0c1422' },
  };

  const FG_THEMES: Record<string, { label: string; fontColor: string }> = {
    ink:   { label: '墨青', fontColor: '#26282e' },
    warmw: { label: '暖白', fontColor: '#e8dcc8' },
    moon:  { label: '月白', fontColor: '#dce0e8' },
    su:    { label: '素白', fontColor: '#e0e0dc' },
    apri:  { label: '杏白', fontColor: '#ecd8d4' },
    thin:  { label: '薄青', fontColor: '#d4e0dc' },
    lotus: { label: '藕紫', fontColor: '#e0d8e4' },
    frost: { label: '霜灰', fontColor: '#d8dce0' },
    rock:  { label: '岩灰', fontColor: '#ccc8c0' },
  };

  function loadPrefs() {
    const raw = lsGet(STORAGE_KEY);
    if (raw) {
      try {
        const p = JSON.parse(raw);
        // 旧默认主题迁移到新默认（仪表台/墨青）：墨金/暖白（v1 旧默认）与标本纸/墨青（上一版默认）
        // 均视为旧默认组合；其他主题（深色等）保持用户选择。与 index.html 首帧脚本保持一致。
        if ((p.bgTheme === 'mojin' && p.fgTheme === 'warmw') || (p.bgTheme === 'herb' && p.fgTheme === 'ink')) {
          p.bgTheme = 'bench';
          p.fgTheme = 'ink';
        }
        prefFontSize = p.fontSize ?? 14;
        prefFontStyle = p.fontStyle ?? 'default';
        prefBgTheme = p.bgTheme ?? 'bench';
        prefFgTheme = p.fgTheme ?? 'ink';
        prefBgOpacity = p.bgOpacity ?? 100;
        prefFgOpacity = p.fgOpacity ?? 100;
      } catch { /* ignore corrupt data */ }
    }
  }

  function saveAndApplyPrefs() {
    const p = { fontSize: prefFontSize, fontStyle: prefFontStyle, bgTheme: prefBgTheme, fgTheme: prefFgTheme, bgOpacity: prefBgOpacity, fgOpacity: prefFgOpacity };
    lsSet(STORAGE_KEY, JSON.stringify(p));
    applyPrefs();
  }

  function applyPrefs() {
    const root = document.documentElement;
    const bg = BG_THEMES[prefBgTheme] || BG_THEMES.bench;
    const fg = FG_THEMES[prefFgTheme] || FG_THEMES.ink;
    const bgAlpha = Math.max(0, Math.min(100, prefBgOpacity)) / 100;
    const fgAlpha = Math.max(0, Math.min(100, prefFgOpacity)) / 100;
    root.style.setProperty('--app-font-size', prefFontSize + 'px');
    root.style.setProperty('--app-font-color', hexToRgba(fg.fontColor, fgAlpha));
    root.style.setProperty('--app-bg-color', hexToRgba(bg.bgColor, bgAlpha));
    root.style.setProperty('--app-color-card-bg', bg.cardBg);
    const font = FONT_STYLES[prefFontStyle] || FONT_STYLES.default;
    root.style.setProperty('--app-font-family', font.family);
  }

  onMount(loadPrefs);

  // 明暗配对自适应：深色背景配浅色文字、浅色背景配深色文字，避免“深字压深底/浅字浮浅底”
  $effect(() => {
    const bg = BG_THEMES[prefBgTheme];
    const fg = FG_THEMES[prefFgTheme];
    if (!bg || !fg) return;
    const bgDark = hexLum(bg.bgColor) < 0.45;
    const fgDark = hexLum(fg.fontColor) < 0.5;
    if (bgDark === fgDark) {
      prefFgTheme = bgDark ? 'moon' : 'ink';
    }
  });

  // 任意值变化时自动保存并应用
  $effect(() => { prefFontSize; prefFontStyle; prefBgTheme; prefFgTheme; prefBgOpacity; prefFgOpacity; saveAndApplyPrefs(); });
</script>

<div class="settings-card">
  <div class="settings-card-title">字体样式</div>
  <div class="settings-card-desc">选择全局字体，更改后整个应用的文字样式将同步更新</div>
  <div class="pref-row font-select-row">
    <div class="font-grid">
      {#each Object.entries(FONT_STYLES) as [key, font]}
        <button class="font-card" class:active={prefFontStyle === key} onclick={() => (prefFontStyle = key)}>
          {#if prefFontStyle === key}
            <span class="pref-check"><CheckIcon class="size-3.5" /></span>
          {/if}
          <span class="font-card-label">{font.label}</span>
          <span class="font-card-desc">{font.desc}</span>
        </button>
      {/each}
    </div>
  </div>
</div>

<div class="settings-card">
  <div class="settings-card-title">背景颜色</div>
  <div class="settings-card-desc">选择预设主题背景或使用取色器自定义</div>
  <div class="theme-grid">
    {#each Object.entries(BG_THEMES) as [key, t]}
      <button class="theme-card" class:active={prefBgTheme === key} onclick={() => (prefBgTheme = key)}>
        {#if prefBgTheme === key}
          <span class="pref-check"><CheckIcon class="size-3.5" /></span>
        {/if}
        <div class="theme-swatch" style="background:{t.bgColor}">
          <span class="theme-label" style={`color:${swatchTextColor(t.bgColor)}`}>{t.label}</span>
          <span class="theme-hex" style={`color:${swatchSubColor(t.bgColor)}`}>#{t.bgColor.replace('#','').toUpperCase()}</span>
        </div>
      </button>
    {/each}
  </div>
  <div class="opacity-row">
    <span class="opacity-label">透明度</span>
    <input type="range" class="pref-range opacity-range" min="0" max="100" step="5" bind:value={prefBgOpacity} style={`background:linear-gradient(to right, var(--app-accent) 0%, var(--app-accent) ${prefBgOpacity}%, var(--app-color-input-border) ${prefBgOpacity}%)`} />
    <span class="opacity-value">{prefBgOpacity}%</span>
  </div>
</div>

<div class="settings-card">
  <div class="settings-card-title">文本颜色</div>
  <div class="settings-card-desc">选择预设文本颜色或使用取色器自定义</div>
  <div class="theme-grid theme-grid-wide">
    {#each Object.entries(FG_THEMES) as [key, t]}
      <button class="theme-card theme-card-wide" class:active={prefFgTheme === key} onclick={() => (prefFgTheme = key)}>
        {#if prefFgTheme === key}
          <span class="pref-check"><CheckIcon class="size-3.5" /></span>
        {/if}
        <div class="theme-swatch" style="background:{t.fontColor}">
          <span class="theme-label" style={`color:${swatchTextColor(t.fontColor)}`}>+</span>
          <span class="theme-label" style={`color:${swatchTextColor(t.fontColor)}`}>{t.label}</span>
          <span class="theme-hex" style={`color:${swatchSubColor(t.fontColor)}`}>#{t.fontColor.replace('#','').toUpperCase()}</span>
        </div>
      </button>
    {/each}
  </div>
  <div class="opacity-row">
    <span class="opacity-label">透明度</span>
    <input type="range" class="pref-range opacity-range" min="0" max="100" step="5" bind:value={prefFgOpacity} style={`background:linear-gradient(to right, var(--app-accent) 0%, var(--app-accent) ${prefFgOpacity}%, var(--app-color-input-border) ${prefFgOpacity}%)`} />
    <span class="opacity-value">{prefFgOpacity}%</span>
  </div>
</div>

<style>
  .settings-card {
    position: relative;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 16px 18px;
    transition: border-color 0.15s;
  }
  .settings-card:hover {
    border-color: color-mix(in oklab, var(--primary) 32%, var(--border));
  }
  .settings-card-title { font-size: 13px; font-weight: 600; letter-spacing: 0.04em; color: var(--foreground); margin-bottom: 10px; }
  .settings-card-desc { font-size: 12px; color: var(--muted-foreground); margin-bottom: 10px; }

  .font-grid, .theme-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 10px;
  }
  .font-card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--card) 70%, black 30%);
    color: var(--foreground);
    cursor: pointer;
    text-align: left;
  }
  .font-card:hover { border-color: var(--ring); }
  .font-card.active { border-color: var(--primary); box-shadow: 0 0 0 1px var(--primary); }
  .pref-check {
    position: absolute;
    top: 6px;
    right: 6px;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--primary);
    color: var(--primary-foreground);
  }
  .font-card-label { font-size: 13px; font-weight: 600; }
  .font-card-desc { font-size: 11.5px; color: var(--muted-foreground); }
  .theme-card {
    position: relative;
    padding: 4px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: transparent;
    cursor: pointer;
  }
  .theme-card:hover { border-color: var(--ring); }
  .theme-card.active { border-color: var(--primary); box-shadow: 0 0 0 1px var(--primary); }
  .theme-swatch {
    height: 64px;
    border-radius: calc(var(--radius-md) - 2px);
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    padding: 8px 10px;
    text-align: left;
  }
  .theme-label { font-size: 12px; font-weight: 600; }
  .theme-label:last-of-type { font-size: 11.5px; font-weight: 500; }
  .theme-hex { font-size: 11px; font-family: var(--font-mono); }
  .theme-card-wide .theme-swatch { height: 44px; flex-direction: row; align-items: center; gap: 10px; }
  .theme-card-wide .theme-hex { margin-left: auto; }
  .opacity-row { display: flex; align-items: center; gap: 12px; margin-top: 14px; }
  .opacity-label { font-size: 12px; color: var(--muted-foreground); width: 60px; }
  .opacity-value { font-size: 12px; color: var(--foreground); width: 44px; text-align: right; font-variant-numeric: tabular-nums; }
  .pref-range {
    -webkit-appearance: none;
    appearance: none;
    flex: 1;
    height: 6px;
    border-radius: 3px;
    outline: none;
  }
  .pref-range::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--foreground);
    border: 2px solid var(--background);
    box-shadow: 0 0 0 1px var(--border);
    cursor: pointer;
  }
</style>

