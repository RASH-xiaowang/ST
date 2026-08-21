<script lang="ts">
  import { errText } from '../../format';
  import { llmApi } from "../services/ipc";
  import { llmStore } from "../store.svelte";
  import type { UsageSummaryItem } from "../types";
  import { fmtLimit, fmtRatio } from "../costFormat";
  import ActivityIcon from "@lucide/svelte/icons/activity";
  import CoinsIcon from "@lucide/svelte/icons/coins";
  import HashIcon from "@lucide/svelte/icons/hash";
  import { RippleButton } from "fancy-ui-svelte";
  import LiveNumber from "../../components/fancy/LiveNumber.svelte";

  let summary = $state<UsageSummaryItem[]>([]);
  let loading = $state(false);
  let error = $state("");

  async function load() {
    loading = true;
    error = "";
    try {
      summary = await llmApi.getUsageSummary();
    } catch (e: unknown) {
      error = `加载失败：${errText(e)}`;
      console.error("[UsageCostTab] load", e);
    } finally {
      loading = false;
    }
  }

  async function reset() {
    if (!confirm("确定清空全部流量与成本统计？")) return;
    try {
      await llmApi.resetUsage();
      await load();
    } catch (e: unknown) {
      error = `清空失败：${errText(e)}`;
    }
  }

  // 汇总总计
  let totalTokens = $derived(summary.reduce((s, i) => s + i.usage.total_tokens, 0));
  let totalCost = $derived(summary.reduce((s, i) => s + i.usage.cost, 0));
  let totalCalls = $derived(summary.reduce((s, i) => s + i.usage.call_count, 0));

  // 加载数据；大模型配置变更（新增/删除提供方等）后自动重新拉取
  $effect(() => {
    llmStore.revision;
    load();
  });
</script>

<div class="llm-usage">
  <div class="llm-toolbar">
    <div class="llm-subtitle">本月（{new Date().toISOString().slice(0, 7)}）流量与成本概览</div>
    <div class="llm-toolbar-actions">
      <RippleButton
        onclick={load}
        disabled={loading}
        rippleColor="#22d3ee"
        class="h-8 rounded-md border border-[var(--app-color-border)] bg-[var(--app-color-surface-alt)] px-3 text-[13px] font-medium text-[var(--app-color-text)] hover:bg-[var(--app-color-hover-bg)]"
      >刷新</RippleButton>
      <button class="llm-btn llm-btn-danger" onclick={reset}>清空统计</button>
    </div>
  </div>

  <div class="llm-overview">
    <div class="llm-stat-card">
      <div class="llm-stat">
        <span class="llm-stat-ico llm-stat-ico-blue"><ActivityIcon class="size-4.5" /></span>
        <div class="llm-stat-main"><div class="llm-stat-v"><LiveNumber value={totalCalls} duration={700} /></div><div class="llm-stat-k">调用次数</div></div>
      </div>
    </div>
    <div class="llm-stat-card">
      <div class="llm-stat">
        <span class="llm-stat-ico llm-stat-ico-cyan"><HashIcon class="size-4.5" /></span>
        <div class="llm-stat-main"><div class="llm-stat-v"><LiveNumber value={totalTokens} duration={700} /></div><div class="llm-stat-k">Token 总量</div></div>
      </div>
    </div>
    <div class="llm-stat-card">
      <div class="llm-stat">
        <span class="llm-stat-ico llm-stat-ico-amber"><CoinsIcon class="size-4.5" /></span>
        <div class="llm-stat-main"><div class="llm-stat-v">$<LiveNumber value={totalCost} duration={700} decimalPlaces={4} /></div><div class="llm-stat-k">成本合计</div></div>
      </div>
    </div>
  </div>

  {#if error}<div class="llm-error">{error}</div>{/if}

  {#if summary.length === 0}
    <div class="llm-empty">暂无用量数据，发起一次全局调用后将在此显示。</div>
  {:else}
    <div class="llm-usage-list">
      {#each summary as item (item.id)}
        <div class="llm-usage-card">
          <div class="llm-usage-head">
            <span class="llm-usage-name">{item.name}</span>
            {#if !item.enabled}<span class="llm-badge llm-badge-off">已禁用</span>{/if}
          </div>

          <div class="llm-meter">
            <div class="llm-meter-top"><span>Token</span><span>{item.usage.total_tokens.toLocaleString()} / {fmtLimit(item.monthly_token_limit)}</span></div>
            {#if item.monthly_token_limit == null}
              <div class="llm-meter-pct">不限额度</div>
            {:else}
              <div class="llm-bar"><div class="llm-bar-fill" class:llm-bar-warn={item.token_ratio > 80} style="width:{item.token_ratio}%"></div></div>
              <div class="llm-meter-pct">{fmtRatio(item.token_ratio)}</div>
            {/if}
          </div>

          <div class="llm-meter">
            <div class="llm-meter-top"><span>成本</span><span>${item.usage.cost.toFixed(4)} / {item.monthly_cost_limit == null ? "不限" : "$" + item.monthly_cost_limit}</span></div>
            {#if item.monthly_cost_limit == null}
              <div class="llm-meter-pct">不限额度</div>
            {:else}
              <div class="llm-bar"><div class="llm-bar-fill llm-bar-cost" class:llm-bar-warn={item.cost_ratio > 80} style="width:{item.cost_ratio}%"></div></div>
              <div class="llm-meter-pct">{fmtRatio(item.cost_ratio)}</div>
            {/if}
          </div>

          <div class="llm-usage-foot">
            调用 {item.usage.call_count} 次 · 输入 {item.usage.prompt_tokens.toLocaleString()} · 输出 {item.usage.completion_tokens.toLocaleString()}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .llm-usage { display: flex; flex-direction: column; gap: 12px; min-height: 100%; }
  .llm-toolbar { display: flex; align-items: center; justify-content: space-between; }
  .llm-subtitle { color: var(--app-color-muted); font-size: 13px; }
  .llm-toolbar-actions { display: flex; gap: 8px; }
  .llm-btn {
    display: inline-flex; align-items: center; gap: 5px; white-space: nowrap;
    background: var(--app-color-surface-alt); color: var(--app-color-text);
    border: 1px solid var(--app-color-border); border-radius: 7px; padding: 6px 12px; font-size: 13px; cursor: pointer;
  }
  .llm-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .llm-btn-danger { color: #f87171; border-color: #ef444433; }
  .llm-btn-danger:hover:not(:disabled) { background: #ef44441a; }
  .llm-overview { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; }
  .llm-stat {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 16px;
  }
  .llm-stat-card {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--card);
    box-shadow: none;
  }
  .llm-stat-ico {
    flex: none;
    width: 38px;
    height: 38px;
    border-radius: 9px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, .14);
  }
  .llm-stat-ico-blue { background: linear-gradient(135deg, #3b82f6, #1d4ed8); }
  .llm-stat-ico-cyan { background: linear-gradient(135deg, #22d3ee, #0891b2); }
  .llm-stat-ico-amber { background: linear-gradient(135deg, #f59e0b, #d97706); }
  .llm-stat-main { min-width: 0; }
  .llm-stat-v { font-size: 20px; font-weight: 700; color: var(--app-color-text); line-height: 1.15; font-variant-numeric: tabular-nums; }
  .llm-stat-k { font-size: 12px; color: var(--app-color-muted); margin-top: 2px; }
  .llm-empty { flex: 1; display: flex; align-items: center; justify-content: center; padding: 24px; min-height: 140px; text-align: center; color: var(--app-color-muted); border: 1px dashed var(--app-color-border); border-radius: 10px; }
  .llm-error { background: #ef44441a; color: #f87171; border: 1px solid #ef444433; padding: 8px 10px; border-radius: 7px; font-size: 13px; }
  .llm-usage-list { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 12px; }
  .llm-usage-card { background: var(--app-color-surface); border: 1px solid var(--app-color-border); border-radius: 10px; padding: 12px; display: flex; flex-direction: column; gap: 10px; transition: border-color 0.18s, box-shadow 0.18s; }
  .llm-usage-card:hover { border-color: color-mix(in srgb, #22d3ee 38%, var(--app-color-border)); box-shadow: 0 0 0 1px color-mix(in srgb, #22d3ee 14%, transparent), 0 8px 28px -14px rgba(34, 211, 238, 0.35); }
  .llm-usage-head { display: flex; align-items: center; gap: 6px; }
  .llm-usage-name { font-weight: 600; color: var(--app-color-text); }
  .llm-badge { font-size: 11.5px; padding: 1px 6px; border-radius: 5px; }
  .llm-badge-off { background: #ef44441a; color: #f87171; border: 1px solid #ef444433; }
  .llm-meter { display: flex; flex-direction: column; gap: 4px; }
  .llm-meter-top { display: flex; justify-content: space-between; font-size: 12px; color: var(--app-color-muted); }
  .llm-bar { height: 8px; background: var(--app-color-surface-alt); border-radius: 5px; overflow: hidden; }
  .llm-bar-fill { height: 100%; background: var(--app-color-accent); border-radius: 5px; transition: width 0.3s; }
  .llm-bar-fill.llm-bar-cost { background: #f59e0b; }
  .llm-bar-fill.llm-bar-warn { background: #ef4444; }
  .llm-meter-pct { font-size: 11.5px; color: var(--app-color-muted); text-align: right; }
  .llm-usage-foot { font-size: 11.5px; color: var(--app-color-muted); border-top: 1px solid var(--app-color-border); padding-top: 6px; }
</style>

