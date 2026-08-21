<script lang="ts">
  import { errText } from '../../format';
  import type { AskCitation } from '../types';
  import { askWechat } from '../services/ipc';
  import { onMount, onDestroy, tick } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { copyText } from '../../clipboard';
  import WechatHoverButton from './WechatHoverButton.svelte';
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import SearchIcon from "@lucide/svelte/icons/search";
  import AlertTriangleIcon from "@lucide/svelte/icons/alert-triangle";
  import ArrowRightIcon from "@lucide/svelte/icons/arrow-right";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";

  interface StatsTable {
    title: string;
    columns: string[];
    rows: string[][];
    summary: string;
  }

  interface AskEntry {
    question: string;
    answer?: string;
    error?: string;
    citations: AskCitation[];
    stats?: StatsTable[];
    steps?: string[];
    /** 问答进行中的实时进度（完成后清空，展示正式 steps） */
    liveSteps: string[];
    rounds?: number;
    plan?: unknown;
    llmUsed: boolean;
    elapsedMs?: number;
    at: string;
  }

  let { onJump = () => {} }: { onJump?: (c: AskCitation) => void } = $props();

  let question = $state('');
  let asking = $state(false);
  let entries = $state<AskEntry[]>([]);
  let copiedKey = $state('');
  let bodyEl = $state<HTMLDivElement | null>(null);
  /** 当前正在提问的问题（进度事件只挂到它身上） */
  let askingQ = $state('');
  let progressUnlisten: (() => void) | null = null;

  const SAMPLES = [
    '我和张三最近聊了什么项目？',
    '我上个月和谁聊得最多？',
    '最近哪些群最活跃？',
    '我去年转了几笔账？',
    '李四最近发了什么朋友圈？',
  ];

  // 实时进度：后端 ask-wechat-progress 事件流式更新当前条目的步骤
  onMount(() => {
    void listen<{ phase: string; message: string }>(
      'ask-wechat-progress',
      (e) => {
        const p = e.payload;
        if (!p?.message) return;
        const last = entries[entries.length - 1];
        if (!last || last.question !== askingQ) return;
        last.liveSteps = [...last.liveSteps, p.message].slice(-8);
      }
    ).then((un) => {
      progressUnlisten = un;
    });
  });
  onDestroy(() => {
    progressUnlisten?.();
  });

  // 新条目出现时自动滚到底部
  $effect(() => {
    void entries.length;
    if (!bodyEl) return;
    tick().then(() => {
      bodyEl?.scrollTo({ top: bodyEl.scrollHeight, behavior: 'smooth' });
    });
  });

  async function ask() {
    const q = question.trim();
    if (!q || asking) return;
    const history = entries.slice(-6).map((e) => ({
      question: e.question,
      answer: e.answer ?? '',
    }));
    asking = true;
    askingQ = q;
    entries.push({
      question: q,
      citations: [],
      llmUsed: false,
      liveSteps: [],
      at: new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' }),
    });
    // 【Svelte 5 关键】$state 数组 push 后元素被代理包装，局部对象引用
    // 与数组元素不再是同一对象——必须通过索引重新取值再赋值，
    // 否则赋值落在幽灵对象上，界面不更新（曾导致 stats/steps 不渲染）
    const idx = entries.length - 1;
    question = '';
    try {
      const r = await askWechat(q, 24, history);
      const cur = entries[idx];
      if (!cur || cur.question !== askingQ) return; // 已被新的提问取代（防御）
      cur.answer = r?.answer ?? undefined;
      cur.error = r?.error ?? undefined;
      cur.citations = Array.isArray(r?.citations) ? r.citations : [];
      cur.stats = Array.isArray(r?.stats) ? r.stats : [];
      cur.steps = Array.isArray(r?.steps) ? r.steps : [];
      cur.rounds = r?.rounds ?? undefined;
      cur.plan = r?.plan;
      cur.llmUsed = !!r?.llm_used;
      cur.elapsedMs = r?.elapsed_ms;
    } catch (e: unknown) {
      const cur = entries[idx];
      if (cur && cur.question === askingQ) cur.error = errText(e);
    } finally {
      const cur = entries[idx];
      if (cur && cur.question === askingQ) {
        cur.liveSteps = [];
        asking = false;
        askingQ = '';
      }
    }
  }

  /** 样例点击直接提问（无需再点一次「提问」） */
  function askSample(s: string) {
    if (asking) return;
    question = s;
    void ask();
  }

  function reask(entry: AskEntry) {
    if (asking) return;
    question = entry.question;
    void ask();
  }

  async function copyAnswer(entry: AskEntry) {
    if (!entry.answer) return;
    const ok = await copyText(entry.answer);
    if (ok) {
      copiedKey = entry.at + entry.question;
      setTimeout(() => { if (copiedKey === entry.at + entry.question) copiedKey = ''; }, 1600);
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void ask();
    }
  }

  function clearAll() {
    entries = [];
  }

  function canJump(c: AskCitation): boolean {
    return c.kind === 'message' && !!c.username && !!c.local_id;
  }

  // ── 回答富文本：转义 → 粗体 → 【n】内联引用 chip → 换行 ──
  function escapeHtml(s: string): string {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  function renderAnswerHtml(text: string): string {
    let esc = escapeHtml(text);
    esc = esc.replace(/\*\*([^*\n]+)\*\*/g, '<b>$1</b>');
    esc = esc.replace(/【(\d+(?:[,，、\s]\d+)*)】/g, (m, nums: string) => {
      const ids = nums
        .split(/[,，、\s]+/)
        .map((n: string) => parseInt(n, 10))
        .filter((n: number) => !Number.isNaN(n));
      if (!ids.length) return m;
      return ids
        .map((n: number) => `<button class="wc-ask-inline-cite" data-cite="${n}" type="button" title="查看引用 ${n}">${n}</button>`)
        .join('');
    });
    return esc.replace(/\n/g, '<br>');
  }

  /** 点击内联引用 chip → 高亮并滚动到对应引用卡片 */
  function onAnswerClick(e: Event) {
    const target = e.target as HTMLElement | null;
    const chip = target?.closest<HTMLElement>('[data-cite]');
    if (!chip || !bodyEl) return;
    const n = Number(chip.dataset.cite);
    const card = bodyEl.querySelector<HTMLElement>(`[data-cite-card="${n}"]`);
    if (!card) return;
    card.scrollIntoView({ behavior: 'smooth', block: 'center' });
    card.classList.remove('wc-ask-cite-flash');
    // 强制重排以重启高亮动画
    void card.offsetWidth;
    card.classList.add('wc-ask-cite-flash');
    setTimeout(() => card.classList.remove('wc-ask-cite-flash'), 1800);
  }
</script>

<div class="wc-ask">
  {#snippet citeBody(c: AskCitation)}
    <div class="wc-ask-cite-top">
      <span class="wc-ask-cite-kind">{c.kind_label}</span>
      <span class="wc-ask-cite-name">{c.name || c.username}</span>
      <span class="wc-ask-cite-time">{c.time}</span>
    </div>
    <div class="wc-ask-cite-snippet">{c.snippet}</div>
  {/snippet}

  <div class="wc-ask-hd">
    <div class="wc-ask-hd-title"><SparklesIcon class="size-4" /> 问我的微信</div>
    <div class="wc-ask-hd-sub">基于本地全部聊天数据提问：AI 自动规划检索 → 多轮补检 → 统计聚合，回答带可跳转原文引用</div>
    {#if entries.length > 0}
        <WechatHoverButton text="清空" onclick={clearAll} title="清空对话" class="!px-3 !py-1 !text-xs" />
    {/if}
  </div>

  <div class="wc-ask-input-row">
    <input
      type="text"
      placeholder="例如：我和张三上周聊了什么项目？"
      bind:value={question}
      onkeydown={onKeydown}
      disabled={asking}
    />
      <WechatHoverButton text={asking ? '思考中…' : '提问'} onclick={() => void ask()} disabled={asking || !question.trim()} />
  </div>

  <div class="wc-ask-samples">
    {#each SAMPLES as s}
        <WechatHoverButton text={s} onclick={() => askSample(s)} disabled={asking} class="!px-3 !py-1 !text-xs" title="点击直接提问" />
    {/each}
  </div>

  <div class="wc-ask-body" bind:this={bodyEl}>
    {#if entries.length === 0}
      <div class="wc-ask-empty">
        <div class="wc-ask-empty-icon"><SearchIcon class="size-8" /></div>
        <div class="wc-ask-empty-title">问任何关于你微信数据的问题</div>
        <div class="wc-ask-empty-points">
          <span>· 聊天记录检索：谁 / 何时 / 聊了什么（关键词 + 会话 + 时间范围）</span>
          <span>· 统计问答：一共多少条、和谁聊得最多、月度趋势、转账/红包笔数</span>
          <span>· 记录查询：转账、红包、收藏、朋友圈、联系人</span>
          <span>· 自动补检：首轮没找到会换词、扩时间再搜，最多 3 轮</span>
          <span>· 每条回答都附带可跳转的原文引用，回答内【1】可直接定位</span>
        </div>
        <div class="wc-ask-empty-tip">统计与检索直接读本地解密数据，未配置模型也能给出证据；配置模型后获得 AI 规划与总结回答</div>
      </div>
    {:else}
      {#each entries as entry (entry.at + entry.question)}
        <div class="wc-ask-item">
          <div class="wc-ask-q">
            <span class="wc-ask-q-text">{entry.question}</span>
            <span class="wc-ask-q-meta">
              {entry.at}
              {entry.elapsedMs != null ? ` · ${(entry.elapsedMs / 1000).toFixed(1)}s` : ''}
              {entry.llmUsed ? ' · AI' : ''}
              {entry.rounds != null ? ` · ${entry.rounds}轮` : ''}
            </span>
          </div>

          {#if asking && entry.question === askingQ}
            <div class="wc-ask-thinking"><span class="wc-loading-inline"></span> 正在检索并组织回答…</div>
          {:else if entry.answer}
            <div
              class="wc-ask-a"
              role="button"
              tabindex="0"
              title="引用编号可点击定位"
              onclick={onAnswerClick}
              onkeydown={(e) => { if (e.key === 'Enter') onAnswerClick(e); }}
            >
              {@html renderAnswerHtml(entry.answer)}
            </div>
            <div class="wc-ask-a-actions">
              <button class="wc-ask-mini-btn" onclick={() => void copyAnswer(entry)} title="复制回答">
                <CopyIcon class="size-3" /> {copiedKey === entry.at + entry.question ? '已复制' : '复制'}
              </button>
              <button class="wc-ask-mini-btn" onclick={() => reask(entry)} title="用同一问题重新检索回答">
                <RotateCcwIcon class="size-3" /> 重新检索
              </button>
            </div>
          {:else if entry.error}
            <div class="wc-ask-error"><AlertTriangleIcon class="size-3.5" /> {entry.error}</div>
          {/if}

          {#if asking && entry.question === askingQ && entry.liveSteps.length > 0}
            <div class="wc-ask-steps wc-ask-steps-live">
              {#each entry.liveSteps as s (s)}
                <div class="wc-ask-step"><span class="wc-ask-step-dot">◦</span>{s}</div>
              {/each}
            </div>
          {:else if entry.steps && entry.steps.length > 0}
            <div class="wc-ask-steps">
              {#each entry.steps as s (s)}
                <div class="wc-ask-step"><span class="wc-ask-step-dot">◦</span>{s}</div>
              {/each}
            </div>
          {/if}

          {#if entry.stats && entry.stats.length > 0}
            {#each entry.stats as t (t.title)}
              <div class="wc-ask-stat">
                <div class="wc-ask-stat-title">{t.title}<span class="wc-ask-stat-summary">{t.summary}</span></div>
                <table class="wc-ask-stat-table">
                  <thead>
                    <tr>
                      {#each t.columns as c (t.title + c)}
                        <th>{c}</th>
                      {/each}
                    </tr>
                  </thead>
                  <tbody>
                    {#each t.rows as row (t.title + row.join('|'))}
                      <tr>
                        {#each row as cell (t.title + row.join('|') + cell)}
                          <td>{cell}</td>
                        {/each}
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            {/each}
          {/if}

          {#if entry.citations.length > 0}
            <div class="wc-ask-cite-hd">引用 {entry.citations.length} 条</div>
            <div class="wc-ask-cites">
              {#each entry.citations as c, ci (c.kind + c.username + c.local_id + c.ts + c.snippet.slice(0, 16))}
                <div data-cite-card={ci + 1} class="wc-ask-cite-wrap">
                  {#if canJump(c)}
                    <button class="wc-ask-cite wc-ask-cite-jump" onclick={() => onJump(c)}>
                      <span class="wc-ask-cite-num">{ci + 1}</span>
                      {@render citeBody(c)}
                      <div class="wc-ask-cite-go">点击跳转定位 <ArrowRightIcon class="size-3.5" /></div>
                    </button>
                  {:else}
                    <div class="wc-ask-cite">
                      <span class="wc-ask-cite-num">{ci + 1}</span>
                      {@render citeBody(c)}
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .wc-ask {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    padding: 16px 20px;
    gap: 10px;
    box-sizing: border-box;
  }
  .wc-ask-hd {
    display: flex;
    align-items: baseline;
    gap: 10px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }
  .wc-ask-hd-title {
    font-size: 16px;
    font-weight: 700;
    color: var(--wc-text);
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .wc-ask-hd-sub {
    font-size: 11.5px;
    color: var(--wc-muted);
  }
  .wc-ask-input-row {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
  }
  .wc-ask-input-row input {
    flex: 1;
    min-width: 0;
    padding: 8px 12px;
    border: 1px solid var(--wc-border);
    border-radius: 6px;
    background: var(--wc-card);
    font-size: 13px;
    color: var(--wc-text);
    outline: none;
  }
  .wc-ask-input-row input:focus {
    border-color: var(--wc-theme, #576b95);
  }
  .wc-ask-samples {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }
  .wc-ask-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 4px 2px 12px;
    scrollbar-width: thin;
  }
  .wc-ask-empty {
    margin: auto;
    text-align: center;
    color: var(--wc-muted);
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 460px;
  }
  .wc-ask-empty-icon {
    font-size: 42px;
    display: inline-flex;
    color: var(--wc-theme, var(--brand));
  }
  .wc-ask-empty-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--wc-text);
  }
  .wc-ask-empty-points {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    line-height: 1.6;
  }
  .wc-ask-empty-tip {
    font-size: 11.5px;
    color: var(--wc-muted);
    border: 1px dashed var(--wc-border);
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--wc-bg2);
  }
  .wc-ask-item {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--wc-border-light);
    border-radius: 10px;
    background: var(--wc-card);
  }
  .wc-ask-q {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }
  .wc-ask-q-text {
    font-size: 13px;
    font-weight: 600;
    color: var(--wc-text);
    word-break: break-all;
  }
  .wc-ask-q-meta {
    font-size: 11px;
    color: var(--wc-muted);
    flex-shrink: 0;
    white-space: nowrap;
  }
  .wc-ask-thinking {
    font-size: 12px;
    color: var(--wc-muted);
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .wc-ask-a {
    font-size: 13px;
    line-height: 1.85;
    color: var(--wc-text);
    white-space: normal;
    word-break: break-word;
  }
  .wc-ask-a-actions {
    display: flex;
    gap: 8px;
  }
  .wc-ask-mini-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 5px;
    border: 1px solid var(--wc-border-light);
    background: var(--wc-bg2);
    color: var(--wc-text2);
    cursor: pointer;
    font: inherit;
  }
  .wc-ask-mini-btn:hover {
    border-color: var(--wc-theme, #576b95);
    color: var(--wc-theme, #576b95);
  }
  /* {@html} 注入的内联引用 chip 无法被 scoped 选择器命中，必须 :global */
  .wc-ask-a :global(.wc-ask-inline-cite) {
    display: inline-block;
    vertical-align: baseline;
    min-width: 17px;
    margin: 0 2px;
    padding: 0 4px;
    font-size: 10.5px;
    line-height: 16px;
    border-radius: 4px;
    border: 1px solid rgba(87, 107, 149, 0.35);
    background: rgba(87, 107, 149, 0.1);
    color: var(--wc-theme, #576b95);
    cursor: pointer;
    font-weight: 600;
    font-family: inherit;
  }
  .wc-ask-a :global(.wc-ask-inline-cite:hover) {
    background: rgba(87, 107, 149, 0.22);
  }
  .wc-ask-error {
    font-size: 12px;
    color: #c0392b;
    background: rgba(192, 57, 43, 0.08);
    border: 1px solid rgba(192, 57, 43, 0.2);
    padding: 8px 10px;
    border-radius: 6px;
  }
  .wc-ask-steps {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 6px 10px;
    border-left: 2px solid var(--wc-border);
    background: var(--wc-bg2);
    border-radius: 0 6px 6px 0;
  }
  .wc-ask-steps-live {
    border-left-color: var(--wc-theme, #576b95);
  }
  .wc-ask-step {
    font-size: 11.5px;
    color: var(--wc-muted);
    line-height: 1.6;
  }
  .wc-ask-step-dot {
    margin-right: 6px;
    color: var(--wc-theme, #576b95);
  }
  .wc-ask-stat {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border: 1px solid var(--wc-border-light);
    border-radius: 8px;
    background: var(--wc-bg2);
  }
  .wc-ask-stat-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--wc-text);
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .wc-ask-stat-summary {
    font-size: 11.5px;
    font-weight: 400;
    color: var(--wc-muted);
  }
  .wc-ask-stat-table {
    border-collapse: collapse;
    font-size: 11.5px;
    width: 100%;
  }
  .wc-ask-stat-table th {
    text-align: left;
    color: var(--wc-muted);
    font-weight: 600;
    padding: 3px 8px 3px 0;
    border-bottom: 1px solid var(--wc-border);
  }
  .wc-ask-stat-table td {
    color: var(--wc-text);
    padding: 3px 8px 3px 0;
    border-bottom: 1px solid var(--wc-border-light);
    word-break: break-all;
  }
  .wc-ask-stat-table tbody tr:last-child td {
    border-bottom: none;
  }
  .wc-ask-cite-hd {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--wc-muted);
    margin-top: 2px;
  }
  .wc-ask-cites {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 8px;
  }
  .wc-ask-cite-wrap {
    position: relative;
  }
  .wc-ask-cite-wrap:global(.wc-ask-cite-flash) {
    outline: 2px solid var(--wc-theme, #576b95);
    outline-offset: 2px;
    border-radius: 8px;
    animation: -global-wc-ask-cite-pulse 1.8s ease;
  }
  @keyframes -global-wc-ask-cite-pulse {
    0% { background: rgba(87, 107, 149, 0.28); }
    100% { background: transparent; }
  }
  .wc-ask-cite {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 10px 8px 30px;
    border: 1px solid var(--wc-border-light);
    border-radius: 8px;
    background: var(--wc-bg2);
    cursor: default;
    position: relative;
    width: 100%;
    box-sizing: border-box;
  }
  .wc-ask-cite-num {
    position: absolute;
    left: 8px;
    top: 8px;
    min-width: 16px;
    text-align: center;
    font-size: 10.5px;
    font-weight: 700;
    line-height: 16px;
    border-radius: 4px;
    background: rgba(87, 107, 149, 0.14);
    color: var(--wc-theme, #576b95);
  }
  button.wc-ask-cite {
    font: inherit;
    color: inherit;
    text-align: left;
  }
  .wc-ask-cite-jump {
    cursor: pointer;
    transition: border-color 0.12s ease, background 0.12s ease;
  }
  .wc-ask-cite-jump:hover {
    border-color: var(--wc-theme, #576b95);
    background: var(--wc-item-hover);
  }
  .wc-ask-cite-top {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .wc-ask-cite-kind {
    font-size: 11.5px;
    padding: 1px 6px;
    border-radius: 999px;
    background: rgba(87, 107, 149, 0.12);
    color: var(--wc-theme, #576b95);
    flex-shrink: 0;
  }
  .wc-ask-cite-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--wc-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .wc-ask-cite-time {
    font-size: 11.5px;
    color: var(--wc-muted);
    margin-left: auto;
    flex-shrink: 0;
  }
  .wc-ask-cite-snippet {
    font-size: 11.5px;
    color: var(--wc-text2);
    line-height: 1.5;
    line-clamp: 3;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-all;
  }
  .wc-ask-cite-go {
    font-size: 11.5px;
    color: var(--wc-theme, #576b95);
  }
</style>
