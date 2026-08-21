<script lang="ts">
  import { errText } from '../../format';
  import { downloadBlob } from '../../download';
  import { scanPrivacyRisks } from '../services/ipc';
  import type { PrivacyScanResult, PrivacyTopItem } from '../types';
  import WechatHoverButton from './WechatHoverButton.svelte';

  interface Sample {
    username: string;
    name: string;
    local_id: number;
    ts: number;
    time: string;
    snippet: string;
  }

  interface Category {
    key: string;
    label: string;
    icon: string;
    count: number;
    samples: Sample[];
  }

  let { onJump = () => {} }: { onJump?: (c: { username: string; local_id?: number; name?: string }) => void } = $props();

  let categories = $state<Category[]>([]);
  let topContacts = $state<PrivacyTopItem[]>([]);
  let topGroups = $state<PrivacyTopItem[]>([]);
  let totalHits = $state(0);
  let scanned = $state<PrivacyScanResult['scanned'] | null>(null);
  let loading = $state(false);
  let error = $state('');
  let expanded = $state<Record<string, boolean>>({});
  let showLimit = $state<Record<string, number>>({});

  async function scan() {
    loading = true;
    error = '';
    try {
    const r = await scanPrivacyRisks();
      categories = Array.isArray(r?.categories) ? r.categories : [];
      topContacts = Array.isArray(r?.top_contacts) ? r.top_contacts : [];
      topGroups = Array.isArray(r?.top_groups) ? r.top_groups : [];
      totalHits = Number(r?.total_hits ?? 0);
      scanned = r?.scanned ?? null;
      expanded = {};
      showLimit = {};
    } catch (e: unknown) {
      error = errText(e);
    } finally {
      loading = false;
    }
  }

  function toggleCat(key: string) {
    expanded[key] = !expanded[key];
  }

  function visibleSamples(c: Category): Sample[] {
    const n = showLimit[c.key] ?? 50;
    return c.samples.slice(0, n);
  }

  function moreSamples(c: Category) {
    showLimit[c.key] = (showLimit[c.key] ?? 50) + 50;
  }

  function involvedSessions(): number {
    const set = new Set<string>();
    for (const c of categories) for (const s of c.samples) set.add(s.username);
    return set.size;
  }

  function exportCsv() {
    const rows: string[][] = [['类别', '会话', '时间', '内容']];
    for (const c of categories) {
      for (const s of c.samples) {
        rows.push([c.label, s.name || s.username, s.time, s.snippet.replace(/"/g, '""')]);
      }
    }
    const csv = rows.map((r) => r.map((v) => `"${v}"`).join(',')).join('\n');
      const blob = new Blob(['\uFEFF' + csv], { type: 'text/csv;charset=utf-8' });
      downloadBlob(blob, `微信隐私体检_${new Date().toISOString().slice(0, 10)}.csv`);
  }
</script>

<div class="wc-privacy">
  <div class="wc-privacy-hd">
    <div>
      <div class="wc-privacy-title">隐私体检</div>
      <div class="wc-privacy-sub">扫描本地聊天记录中的敏感信息，识别风险并支持导出报告（数据仅在本机处理）</div>
    </div>
    <div class="wc-privacy-ctl">
      <WechatHoverButton text={loading ? '扫描中…' : totalHits > 0 ? '重新扫描' : '开始扫描'} onclick={scan} disabled={loading} />
      <WechatHoverButton text="导出报告" onclick={exportCsv} disabled={totalHits === 0} title="导出全部命中为 CSV" class="!px-3 !py-1 !text-xs" />
    </div>
  </div>

  {#if error}
    <div class="wc-privacy-error">⚠ {error}</div>
  {/if}

  {#if totalHits > 0}
    <div class="wc-privacy-chips">
      <span class="wc-privacy-chip">命中 {totalHits} 条</span>
      <span class="wc-privacy-chip">涉及会话 {involvedSessions()} 个</span>
      {#if scanned}
        <span class="wc-privacy-chip">扫描 {scanned.rows} 行</span>
        <span class="wc-privacy-chip">耗时 {scanned.elapsed_ms}ms</span>
      {/if}
    </div>

    <div class="wc-privacy-body">
      <div class="wc-privacy-cats">
        {#each categories as c (c.key)}
          <div class="wc-privacy-cat" class:wc-privacy-cat-open={expanded[c.key]}>
            <button class="wc-privacy-cat-hd" onclick={() => toggleCat(c.key)}>
              <span class="wc-privacy-cat-icon">{c.icon}</span>
              <span class="wc-privacy-cat-label">{c.label}</span>
              <span class="wc-privacy-cat-count">{c.count}</span>
              <span class="wc-privacy-cat-arrow">{expanded[c.key] ? '▾' : '▸'}</span>
            </button>
            {#if expanded[c.key]}
              <div class="wc-privacy-samples">
                {#if c.samples.length === 0}
                  <div class="wc-privacy-samples-empty">无命中</div>
                {:else}
                  {#each visibleSamples(c) as s (c.key + s.username + s.local_id)}
                    <div class="wc-privacy-sample">
                      <div class="wc-privacy-sample-top">
                        <span class="wc-privacy-sample-name">{s.name || s.username}</span>
                        <span class="wc-privacy-sample-time">{s.time}</span>
                        {#if s.local_id}
                          <button class="wc-privacy-sample-jump" onclick={() => onJump({ username: s.username, local_id: s.local_id, name: s.name })}>跳转 ›</button>
                        {/if}
                      </div>
                      <div class="wc-privacy-sample-snippet">{s.snippet}</div>
                    </div>
                  {/each}
                  {#if c.samples.length > (showLimit[c.key] ?? 50)}
                    <WechatHoverButton text={`显示更多（共 ${c.samples.length} 条）`} onclick={() => moreSamples(c)} class="!px-3 !py-1 !text-xs" />
                  {/if}
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>

      <div class="wc-privacy-side">
        <div class="wc-privacy-side-card">
          <div class="wc-privacy-side-title">风险联系人 TOP10</div>
          {#if topContacts.length === 0}
            <div class="wc-privacy-side-empty">无</div>
          {:else}
            {#each topContacts as t (t.username)}
              <div class="wc-privacy-side-row">
                <span class="wc-privacy-side-name">{t.name || t.username}</span>
                <span class="wc-privacy-side-count">{t.count}</span>
              </div>
            {/each}
          {/if}
        </div>
        <div class="wc-privacy-side-card">
          <div class="wc-privacy-side-title">风险群聊 TOP10</div>
          {#if topGroups.length === 0}
            <div class="wc-privacy-side-empty">无</div>
          {:else}
            {#each topGroups as t (t.username)}
              <div class="wc-privacy-side-row">
                <span class="wc-privacy-side-name">{t.name || t.username}</span>
                <span class="wc-privacy-side-count">{t.count}</span>
              </div>
            {/each}
          {/if}
        </div>
        <div class="wc-privacy-tip">
          提示：扫描结果仅用于本地自查。银行卡、身份证、密码等敏感信息建议在微信中开启“聊天记录迁移/备份”后妥善保管。
        </div>
      </div>
    </div>
  {:else if !loading && !error}
    <div class="wc-privacy-empty">
        <div class="wc-privacy-empty-icon"><svg viewBox="0 0 24 24" width="42" height="42" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><path d="M9 12l2 2 4-4"/></svg></div>
      <div class="wc-privacy-empty-title">体检你的微信数据</div>
      <div class="wc-privacy-empty-sub">
        将扫描聊天记录中的手机号、身份证号、银行卡号、邮箱、密码口令与地址信息，并按会话聚合风险分布。
      </div>
    </div>
  {/if}
</div>

<style>
  .wc-privacy {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    padding: 16px 20px;
    gap: 10px;
    box-sizing: border-box;
  }
  .wc-privacy-hd {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-shrink: 0;
  }
  .wc-privacy-title {
    font-size: 16px;
    font-weight: 700;
    color: var(--wc-text);
  }
  .wc-privacy-sub {
    font-size: 11.5px;
    color: var(--wc-muted);
  }
  .wc-privacy-ctl {
    display: flex;
    gap: 8px;
  }
  .wc-privacy-ctl :global(.wc-ihb:first-of-type) { margin-left: auto; }
  .wc-privacy-error {
    font-size: 12px;
    color: #c0392b;
    background: rgba(192, 57, 43, 0.08);
    border: 1px solid rgba(192, 57, 43, 0.2);
    padding: 8px 10px;
    border-radius: 6px;
  }
  .wc-privacy-chips {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }
  .wc-privacy-chip {
    font-size: 11.5px;
    padding: 3px 10px;
    border-radius: 999px;
    background: var(--wc-bg2);
    border: 1px solid var(--wc-border-light);
    color: var(--wc-text2);
  }
  .wc-privacy-body {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 12px;
    overflow: hidden;
  }
  .wc-privacy-cats {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-right: 4px;
  }
  .wc-privacy-cat {
    border: 1px solid var(--wc-border-light);
    border-radius: 10px;
    background: var(--wc-card);
    overflow: hidden;
  }
  .wc-privacy-cat-hd {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border: none;
    background: transparent;
    cursor: pointer;
    color: var(--wc-text);
    font-size: 13px;
  }
  .wc-privacy-cat-hd:hover {
    background: var(--wc-item-hover);
  }
  .wc-privacy-cat-icon {
    font-size: 16px;
  }
  .wc-privacy-cat-label {
    font-weight: 600;
  }
  .wc-privacy-cat-count {
    margin-left: auto;
    font-size: 12px;
    font-weight: 700;
    color: #e67e22;
  }
  .wc-privacy-cat-arrow {
    color: var(--wc-muted);
    font-size: 11.5px;
  }
  .wc-privacy-samples {
    border-top: 1px solid var(--wc-border-light);
    padding: 6px 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .wc-privacy-samples-empty {
    color: var(--wc-muted);
    font-size: 12px;
    text-align: center;
    padding: 8px 0;
  }
  .wc-privacy-sample {
    padding: 7px 9px;
    border-radius: 8px;
    background: var(--wc-bg2);
  }
  .wc-privacy-sample-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .wc-privacy-sample-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--wc-text);
  }
  .wc-privacy-sample-time {
    font-size: 11.5px;
    color: var(--wc-muted);
  }
  .wc-privacy-sample-jump {
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--wc-theme, #576b95);
    font-size: 11.5px;
    cursor: pointer;
  }
  .wc-privacy-sample-snippet {
    margin-top: 3px;
    font-size: 11.5px;
    color: var(--wc-text2);
    word-break: break-all;
    line-height: 1.5;
  }
  .wc-privacy-side {
    width: 250px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
  }
  .wc-privacy-side-card {
    border: 1px solid var(--wc-border-light);
    border-radius: 10px;
    background: var(--wc-card);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .wc-privacy-side-title {
    font-size: 12px;
    font-weight: 700;
    color: var(--wc-text);
  }
  .wc-privacy-side-empty {
    color: var(--wc-muted);
    font-size: 11.5px;
    text-align: center;
    padding: 6px 0;
  }
  .wc-privacy-side-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
  }
  .wc-privacy-side-name {
    color: var(--wc-text2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .wc-privacy-side-count {
    color: #e67e22;
    font-weight: 600;
    flex-shrink: 0;
  }
  .wc-privacy-tip {
    font-size: 11.5px;
    color: var(--wc-muted);
    line-height: 1.6;
    border: 1px dashed var(--wc-border);
    border-radius: 8px;
    padding: 8px 10px;
    background: var(--wc-bg2);
  }
  .wc-privacy-empty {
    margin: auto;
    text-align: center;
    color: var(--wc-muted);
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 420px;
  }
  .wc-privacy-empty-icon {
    font-size: 44px;
  }
  .wc-privacy-empty-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--wc-text);
  }
  .wc-privacy-empty-sub {
    font-size: 12px;
    line-height: 1.7;
  }
</style>
