<script lang="ts">
  import { errText } from '../../format';
  import { onMount } from 'svelte';
  import { getAnnualAvailableYears, getAnnualSummary } from '../services/ipc';
  import { bestIndex, buildPersonaTags, calmIndex, fmtInt, fmtNum, heatBg, hourShare, peakInfoOf, pct, weekendShareOf } from '../utils/annual';
  import type { AnnualSummaryData } from '../types';
  import LiveNumber from '../../components/fancy/LiveNumber.svelte';
    import WechatHoverButton from './WechatHoverButton.svelte';

  let years = $state<number[]>([]);
  let year = $state(0);
  let loading = $state(false);
  let error = $state('');
  let data = $state<AnnualSummaryData | null>(null);
  let shownTotal = $state(0);

  async function loadYears() {
    try {
    const r = await getAnnualAvailableYears();
      years = (r?.years ?? []).map((x) => Number(x)).filter((n: number) => Number.isFinite(n) && n > 2000);
      if (years.length === 0) {
        error = '还没有可统计的消息数据。请先在「设置 → 微信」里完成数据库解密，再回到这里生成报告。';
      } else {
        year = years[0];
        await loadSummary();
      }
    } catch (e: unknown) {
      error = errText(e);
    }
  }

  async function loadSummary() {
    if (!year) return;
    loading = true;
    error = '';
    try {
    data = await getAnnualSummary(year);
      shownTotal = 0;
      // 数字滚动（尊重系统减少动态效果设置）
      const target = Number(data?.total_messages || 0);
      if (typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) {
        shownTotal = target;
      } else {
        const t0 = performance.now();
        const dur = 700;
        const step = (now: number) => {
          const p = Math.min(1, (now - t0) / dur);
          const eased = 1 - Math.pow(1 - p, 3);
          shownTotal = Math.round(target * eased);
          if (p < 1) requestAnimationFrame(step);
        };
        requestAnimationFrame(step);
      }
    } catch (e: unknown) {
      error = errText(e);
      data = null;
    } finally {
      loading = false;
    }
  }

  // 热力图配色：微信绿，按强度提升透明度
  const monthLabels = ['1月','2月','3月','4月','5月','6月','7月','8月','9月','10月','11月','12月'];

  let maxKind = $derived(Math.max(0, ...(data?.kind_counts ?? []).map((k) => Number(k.count) || 0)));
  let maxMonth = $derived(Math.max(0, ...(data?.monthly_counts ?? []).map((n) => Number(n) || 0)));
  let heatMatrix = $derived<number[][]>(data?.heatmap?.matrix ?? []);
  let maxHeat = $derived(Math.max(1, ...heatMatrix.flat().map((n) => Number(n) || 0)));

  // 峰值时刻：最活跃的星期与小时
  let peakInfo = $derived(peakInfoOf(data?.heatmap, heatMatrix));

  // ── 年度画像：从现有数据派生的洞察 ──
  let total = $derived(Number(data?.total_messages || 0));
  let days = $derived(Number(data?.active_days || 0));
  let dayAvg = $derived(days > 0 ? total / days : 0);
  let textShare = $derived(pct(Number(data?.text_messages || 0), total));
  let heatTotal = $derived(heatMatrix.reduce((s, r) => s + r.reduce((a, b) => a + (Number(b) || 0), 0), 0));
  let nightShare = $derived.by(() => {
    if (!heatTotal) return 0;
    return hourShare(heatMatrix, [23, 0, 1, 2, 3, 4]);
  });
  let morningShare = $derived.by(() => {
    if (!heatTotal) return 0;
    return hourShare(heatMatrix, [5, 6, 7, 8, 9]);
  });
  let weekendShare = $derived(heatTotal ? weekendShareOf(heatMatrix) : 0);
  let groupShare = $derived(pct((data?.top_groups ?? []).reduce((a: number, g) => a + (Number(g.count) || 0), 0), total));
  let bestMonth = $derived.by(() => {
    const mc = data?.monthly_counts ?? [];
    const idx = bestIndex(mc);
    if (idx < 0) return { index: -1, count: 0, label: '' };
    const best = Number(mc[idx]) || 0;
    return { index: idx, count: best, label: monthLabels[idx] };
  });
  let calmMonth = $derived.by(() => {
    const mc = data?.monthly_counts ?? [];
    const idx = calmIndex(mc);
    if (idx < 0) return { index: -1, count: 0, label: '' };
    const calm = Number(mc[idx]) || 0;
    return { index: idx, count: calm, label: monthLabels[idx] };
  });
  let topContactShare = $derived(pct(Number(data?.top_contacts?.[0]?.count || 0), total));
  let topGroupShare = $derived(pct(Number(data?.top_groups?.[0]?.count || 0), total));
  let emojiTotal = $derived((data?.top_emojis ?? []).reduce((a: number, e) => a + (Number(e.count) || 0), 0));
  let topEmojiShare = $derived(pct(Number(data?.top_emojis?.[0]?.count || 0), emojiTotal));

  // 人物标签（纯文本，无装饰性 emoji）
  let personaTags = $derived(
    buildPersonaTags({ nightShare, morningShare, weekendShare, groupShare, dayAvg })
  );

  const hourTicks = [0, 3, 6, 9, 12, 15, 18, 21, 23];
  const heatCellIndex = (w: number, h: number) => w * 24 + h;

  onMount(() => { loadYears(); });
</script>

<div class="as-root">
  <header class="as-hd">
    <div class="as-brand">
      <h2 class="as-title">年度总结</h2>
      <span class="as-sub">从解密数据中生成的微信年度报告 · 仅本地计算</span>
    </div>
    <div class="as-actions">
      {#if years.length > 1}
        <div class="as-years" role="tablist" aria-label="选择年份">
          {#each years as y (y)}
            <WechatHoverButton
              text={`${y}`}
              onclick={() => { year = y; loadSummary(); }}
              class={y === year ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'}
            />
          {/each}
        </div>
      {/if}
      <WechatHoverButton text={loading ? '统计中' : '刷新'} onclick={loadSummary} disabled={loading || !year} title="重新统计" class="!px-3 !py-1 !text-xs" />
    </div>
  </header>

  {#if error}
    <div class="as-state">
      <div class="as-state-icon">
        <svg viewBox="0 0 24 24" width="26" height="26" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
          <circle cx="12" cy="12" r="9"/><path d="M12 8v5"/><circle cx="12" cy="16.5" r=".5" fill="currentColor"/>
        </svg>
      </div>
      <p class="as-state-text">{error}</p>
      <WechatHoverButton text="重新检测" onclick={loadYears} class="!px-3 !py-1 !text-xs" />
    </div>
  {:else if loading && !data}
    <div class="as-skeleton" aria-busy="true" aria-label="正在统计年度数据">
      <div class="sk sk-hero"></div>
      <div class="sk sk-heat"></div>
      <div class="sk-grid"><div class="sk sk-row"></div><div class="sk sk-row"></div></div>
    </div>
  {:else if data}
    <div class="as-scroll">
      <!-- 年度总览：一句话故事 + 关键数字 -->
      <section class="as-hero">
        <div class="as-hero-main">
          <h3 class="as-hero-title">{data.year} 年，你说了 <strong>{fmtInt(shownTotal)}</strong> 条消息</h3>
          <p class="as-hero-sub">在 {fmtInt(data.active_days)} 天里累计写下 {fmtInt(data.total_chars)} 字，平均每句 {Number(data.avg_chars || 0).toFixed(1)} 字，日均 {fmtNum(Math.round(dayAvg))} 条</p>
          <div class="as-persona">
            {#each personaTags as tag (tag)}<span class="as-persona-tag">{tag}</span>{/each}
          </div>
        </div>
        <dl class="as-stats">
          <div class="as-stat">
            <dt>活跃天数</dt>
            <dd><LiveNumber value={data.active_days} duration={900} /></dd>
          </div>
          <div class="as-stat">
            <dt>累计文字</dt>
            <dd><LiveNumber value={data.total_chars} duration={900} /></dd>
          </div>
          <div class="as-stat">
            <dt>日均消息</dt>
            <dd><LiveNumber value={Math.round(dayAvg)} duration={900} /></dd>
          </div>
          <div class="as-stat">
            <dt>文字占比</dt>
            <dd>{textShare}%</dd>
          </div>
        </dl>
      </section>

      <!-- 周活跃热力图（签名视觉） -->
      <section class="as-panel">
        <div class="as-panel-hd">
          <h3 class="as-panel-title">周活跃热力图</h3>
          <p class="as-panel-note">按星期 × 小时统计 · 共 {fmtNum(data.heatmap?.total ?? 0)} 条</p>
        </div>
        {#if heatMatrix.length}
          <div class="as-insight-row">
            {#if peakInfo.value > 0}
              <p class="as-insight">你最常在 <strong>{peakInfo.weekday} {peakInfo.hour}:00</strong> 活跃</p>
            {/if}
            <div class="as-heat-chips">
              <span class="as-heat-chip">周末 {weekendShare}%</span>
              <span class="as-heat-chip">深夜 {nightShare}%</span>
              <span class="as-heat-chip">清晨 {morningShare}%</span>
            </div>
          </div>
          <div class="as-heat">
            <div class="as-heat-y" aria-hidden="true">
              {#each data.heatmap.weekdayLabels as wd (wd)}<span>{wd}</span>{/each}
            </div>
            <div class="as-heat-wrap">
              <div class="as-heat-grid">
                {#each heatMatrix as row, wi (wi)}
                  {#each row as cell, hi (hi)}
                    <div
                      class="as-heat-cell"
                      class:as-heat-peak={cell > 0 && cell === maxHeat}
                      style="background:{heatBg(cell, maxHeat)};--i:{heatCellIndex(wi, hi)}"
                      title="{data.heatmap.weekdayLabels[wi]} {String(hi).padStart(2,'0')}:00 · {cell} 条"
                      role="img"
                      aria-label="{data.heatmap.weekdayLabels[wi]} {String(hi).padStart(2,'0')} 时，{cell} 条消息"
                    ></div>
                  {/each}
                {/each}
              </div>
              <div class="as-heat-x" aria-hidden="true">
                {#each hourTicks as h (h)}<span>{h}</span>{/each}
              </div>
            </div>
          </div>
        {:else}
          <p class="as-empty">今年没有可统计的消息时段</p>
        {/if}
      </section>

      <div class="as-cols">
        <!-- 月度活跃 -->
        <section class="as-panel">
          <div class="as-panel-hd">
            <h3 class="as-panel-title">月度活跃</h3>
            <p class="as-panel-note">全年消息分布</p>
          </div>
          {#if bestMonth.index >= 0}
            <p class="as-insight as-insight-month">最热闹的是 <strong>{bestMonth.label}</strong>（{fmtNum(bestMonth.count)} 条）{calmMonth.index >= 0 ? `，最安静的是 ${calmMonth.label}` : ''}</p>
          {/if}
          <div class="as-monthly">
            {#each data.monthly_counts as c, i (i)}
              <div class="as-month-col" title="{monthLabels[i]}：{fmtNum(c)} 条">
                <div
                  class="as-month-bar"
                  class:as-month-bar-best={i === bestMonth.index}
                  style="height:{maxMonth > 0 ? Math.max(2, Math.round(c / maxMonth * 100)) : 2}%"
                ></div>
                <span class="as-month-label">{i + 1}</span>
              </div>
            {/each}
          </div>
        </section>

        <!-- 消息类型 -->
        <section class="as-panel">
          <div class="as-panel-hd">
            <h3 class="as-panel-title">消息类型</h3>
            <p class="as-panel-note">按本地消息类型统计</p>
          </div>
          {#if data.kind_counts?.length}
            <div class="as-kinds">
              {#each data.kind_counts as k (k.kind)}
                <div class="as-kind-row">
                  <span class="as-kind-label">{k.label}</span>
                  <div class="as-kind-track"><div class="as-kind-fill" style="--w:{maxKind > 0 ? (k.count / maxKind) : 0}"></div></div>
                  <span class="as-kind-val">{fmtNum(k.count)}<em>{pct(Number(k.count), total)}%</em></span>
                </div>
              {/each}
            </div>
          {:else}
            <p class="as-empty">暂无数据</p>
          {/if}
        </section>
      </div>

      <div class="as-cols">
        <!-- 高频短语 -->
        <section class="as-panel">
          <div class="as-panel-hd">
            <h3 class="as-panel-title">高频短语</h3>
            <p class="as-panel-note">出现最多的短句</p>
          </div>
          {#if data.top_phrases?.length}
            <div class="as-chips">
              {#each data.top_phrases as p (p.key)}
                <span class="as-chip" title="出现 {p.count} 次">{p.key}<em>{p.count}</em></span>
              {/each}
            </div>
          {:else}<p class="as-empty">暂无数据</p>{/if}
        </section>

        <!-- 表情宇宙 -->
        <section class="as-panel">
          <div class="as-panel-hd">
            <h3 class="as-panel-title">表情宇宙</h3>
            <p class="as-panel-note">{#if data.top_emojis?.[0]}最爱 {data.top_emojis[0].key}，占表情 {topEmojiShare}%{/if}</p>
          </div>
          {#if data.top_emojis?.length}
            <div class="as-emoji-grid">
              {#each data.top_emojis.slice(0, 8) as e, i (e.key)}
                <div class="as-emoji-cell" title="出现 {e.count} 次">
                  <span class="as-emoji-char" class:as-emoji-top={i === 0}>{e.key}</span>
                  <span class="as-emoji-count">{fmtNum(e.count)}</span>
                </div>
              {/each}
            </div>
          {:else}<p class="as-empty">暂无数据</p>{/if}
        </section>
      </div>

      <div class="as-cols">
        <!-- 好友榜 -->
        <section class="as-panel">
          <div class="as-panel-hd">
            <h3 class="as-panel-title">聊得最多的人</h3>
            <p class="as-panel-note">{#if data.top_contacts?.[0]}占全年消息 {topContactShare}%{/if}</p>
          </div>
          {#if data.top_contacts?.length}
            <ol class="as-rank">
              {#each data.top_contacts as c, i (c.key)}
                <li>
                  <span class="as-rank-idx">{String(i + 1).padStart(2, '0')}</span>
                  <span class="as-rank-name">{c.name}</span>
                  <span class="as-rank-bar"><span style="width:{Math.round(c.count / data.top_contacts[0].count * 100)}%"></span></span>
                  <span class="as-rank-count">{fmtNum(c.count)}<em>{pct(Number(c.count), total)}%</em></span>
                </li>
              {/each}
            </ol>
          {:else}<p class="as-empty">暂无数据</p>{/if}
        </section>

        <!-- 群聊榜 -->
        <section class="as-panel">
          <div class="as-panel-hd">
            <h3 class="as-panel-title">最活跃的群聊</h3>
            <p class="as-panel-note">{#if data.top_groups?.[0]}占全年消息 {topGroupShare}%{/if}</p>
          </div>
          {#if data.top_groups?.length}
            <ol class="as-rank">
              {#each data.top_groups as c, i (c.key)}
                <li>
                  <span class="as-rank-idx">{String(i + 1).padStart(2, '0')}</span>
                  <span class="as-rank-name">{c.name}</span>
                  <span class="as-rank-bar"><span style="width:{Math.round(c.count / data.top_groups[0].count * 100)}%"></span></span>
                  <span class="as-rank-count">{fmtNum(c.count)}<em>{pct(Number(c.count), total)}%</em></span>
                </li>
              {/each}
            </ol>
          {:else}<p class="as-empty">暂无群聊数据</p>{/if}
        </section>
      </div>

      <!-- 第一句与最后一句 -->
      {#if data.earliest || data.latest}
        <section class="as-panel">
          <div class="as-panel-hd">
            <h3 class="as-panel-title">{data.year} 的第一句与最后一句</h3>
            <p class="as-panel-note">按时间排序</p>
          </div>
          <div class="as-timeline">
            {#if data.earliest}
              <div class="as-tl-item">
                <span class="as-tl-dot as-tl-dot-first"></span>
                <div class="as-tl-body">
                  <div class="as-tl-meta">
                    <span class="as-tl-tag">第一句</span>
                    <span class="as-tl-name">{data.earliest.name}</span>
                    <time class="as-tl-time">{data.earliest.date} {data.earliest.time}</time>
                  </div>
                  <p class="as-tl-text">{data.earliest.text || '（非文本消息）'}</p>
                </div>
              </div>
            {/if}
            {#if data.latest}
              <div class="as-tl-item">
                <span class="as-tl-dot as-tl-dot-last"></span>
                <div class="as-tl-body">
                  <div class="as-tl-meta">
                    <span class="as-tl-tag as-tl-tag-last">最后一句</span>
                    <span class="as-tl-name">{data.latest.name}</span>
                    <time class="as-tl-time">{data.latest.date} {data.latest.time}</time>
                  </div>
                  <p class="as-tl-text">{data.latest.text || '（非文本消息）'}</p>
                </div>
              </div>
            {/if}
          </div>
        </section>
      {/if}
    </div>
  {:else}
    <div class="as-state"><div class="as-state-icon as-spinner"></div><p class="as-state-text">加载中…</p></div>
  {/if}
</div>

<style>
  /* ===== 年度总结 =====
     视觉语言：微信绿 (#07c160) 作为数据色，中性面板承载；
     版式采用“一句话故事 + 关键数字 + 签名热力图 + 双列数据面板”。
     动效仅保留热力图入场与主数字滚动两个标志性时刻。 */
  .as-root {
    flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0;
    background: var(--wc-bg); color: var(--wc-text);
  }

  /* 顶栏 */
  .as-hd {
    display: flex; align-items: center; justify-content: space-between; gap: 14px;
    padding: 12px 18px; border-bottom: 1px solid var(--wc-border); flex-shrink: 0;
  }
  .as-brand { display: flex; align-items: baseline; gap: 10px; min-width: 0; }
  .as-title { font-size: 16px; font-weight: 700; margin: 0; letter-spacing: -0.01em; }
  .as-sub { font-size: 11.5px; color: var(--wc-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .as-actions { display: flex; align-items: center; gap: 10px; flex-shrink: 0; }
  .as-years { display: flex; gap: 3px; padding: 3px; background: var(--wc-bg2); border-radius: 10px; }
  /* 滚动内容 */
  .as-scroll { flex: 1; overflow-y: auto; padding: 18px 22px 30px; display: flex; flex-direction: column; gap: 18px; scrollbar-width: thin; }

  /* 年度总览：一句话故事 + 关键数字条 */
  .as-hero {
    display: flex; align-items: center; justify-content: space-between; gap: 28px;
    padding: 26px 28px; border-radius: 16px; border: 1px solid var(--wc-border);
    background:
      radial-gradient(120% 140% at 0% 0%, color-mix(in srgb, var(--as-green,#07c160) 9%, transparent), transparent 55%),
      var(--wc-card);
  }
  .as-hero-main { min-width: 0; }
  .as-hero-title { font-size: 27px; font-weight: 800; letter-spacing: -0.02em; margin: 0; line-height: 1.28; }
  .as-hero-title strong { color: var(--as-green,#07c160); font-variant-numeric: tabular-nums; }
  .as-hero-sub { font-size: 12.5px; color: var(--wc-muted); margin: 9px 0 0; line-height: 1.6; }
  .as-persona { display: flex; flex-wrap: wrap; gap: 7px; margin-top: 14px; }
  .as-persona-tag {
    padding: 4px 11px; border-radius: 999px; font-size: 11.5px; font-weight: 600;
    color: var(--as-green-ink,#048a4a);
    background: color-mix(in srgb, var(--as-green,#07c160) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--as-green,#07c160) 26%, transparent);
  }
  .as-stats { display: flex; align-items: stretch; gap: 0; margin: 0; flex-shrink: 0; }
  .as-stat { display: flex; flex-direction: column; justify-content: center; gap: 4px; padding: 4px 22px; min-width: 84px; }
  .as-stat + .as-stat { border-left: 1px solid var(--wc-border-light); }
  .as-stat dt { font-size: 11.5px; color: var(--wc-muted); order: 2; }
  .as-stat dd { margin: 0; font-size: 21px; font-weight: 800; color: var(--wc-text); font-variant-numeric: tabular-nums; letter-spacing: -0.01em; order: 1; }

  /* 面板通用 */
  .as-panel { border: 1px solid var(--wc-border); border-radius: 16px; background: var(--wc-card); padding: 18px 20px; }
  .as-panel-hd { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; margin-bottom: 14px; }
  .as-panel-title { font-size: 14px; font-weight: 700; margin: 0; letter-spacing: -0.01em; }
  .as-panel-note { font-size: 11.5px; color: var(--wc-muted); }
  .as-cols { display: grid; grid-template-columns: 1fr 1fr; gap: 18px; }
  .as-empty { color: var(--wc-muted); font-size: 12px; margin: 0; padding: 10px 0; }

  /* 洞察行（峰值 + 时段占比） */
  .as-insight-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; margin: -4px 0 12px; }
  .as-insight { font-size: 12px; color: var(--wc-text2); margin: 0; }
  .as-insight strong { color: var(--as-green-ink,#048a4a); font-weight: 700; }
  .as-insight-month { margin: -4px 0 12px; }
  .as-heat-chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .as-heat-chip {
    padding: 3px 9px; border-radius: 999px; font-size: 11.5px; color: var(--wc-text2);
    background: var(--wc-bg2); border: 1px solid var(--wc-border-light); font-variant-numeric: tabular-nums;
  }

  /* 热力图 */
  .as-heat { display: flex; gap: 10px; }
  .as-heat-y { display: flex; flex-direction: column; gap: 3px; padding-top: 1px; }
  .as-heat-y span { font-size: 11.5px; color: var(--wc-muted); height: 14px; line-height: 14px; white-space: nowrap; }
  .as-heat-wrap { flex: 1; min-width: 0; }
  .as-heat-grid {
    display: grid; grid-template-columns: repeat(24, 1fr); grid-template-rows: repeat(7, 1fr);
    gap: 3px;
  }
  .as-heat-cell {
    aspect-ratio: 1; border-radius: 3px; min-width: 0;
    animation: as-heat-in .5s ease-out both;
    animation-delay: calc(var(--i) * 2.2ms);
  }
  @keyframes as-heat-in { from { opacity: 0; transform: scale(.5); } to { opacity: 1; transform: scale(1); } }
  .as-heat-peak { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--as-green,#07c160) 45%, transparent); }
  .as-heat-x { display: flex; justify-content: space-between; font-size: 11.5px; color: var(--wc-muted); padding-top: 6px; }

  /* 月度 */
  .as-monthly { display: flex; align-items: flex-end; gap: 6px; height: 150px; margin-top: 4px; }
  .as-month-col { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: flex-end; gap: 5px; height: 100%; min-width: 0; }
  .as-month-bar {
    width: 100%; max-width: 22px; border-radius: 5px 5px 2px 2px; min-height: 2px;
    background: color-mix(in srgb, var(--as-green,#07c160) 62%, transparent);
    transition: background .2s ease, transform .15s ease;
  }
  .as-month-bar-best { background: var(--as-green,#07c160); }
  .as-month-col:hover .as-month-bar { background: var(--as-green,#07c160); transform: scaleX(1.06); }
  .as-month-label { font-size: 10.5px; color: var(--wc-muted); }

  /* 类型分布 */
  .as-kinds { display: flex; flex-direction: column; gap: 10px; }
  .as-kind-row { display: flex; align-items: center; gap: 10px; font-size: 12px; }
  .as-kind-label { width: 42px; color: var(--wc-text2); flex-shrink: 0; }
  .as-kind-track { flex: 1; height: 12px; border-radius: 6px; background: var(--wc-bg2); overflow: hidden; }
  .as-kind-fill {
    height: 100%; width: 100%; border-radius: 6px; background: var(--as-green,#07c160); opacity: .85;
    transform-origin: left; transform: scaleX(var(--w, 0)); transition: transform .3s ease;
  }
  .as-kind-val { width: 74px; text-align: right; color: var(--wc-muted); font-variant-numeric: tabular-nums; font-size: 11.5px; }
  .as-kind-val em { font-style: normal; color: var(--wc-text2); margin-left: 6px; }

  /* 短语 / 表情 */
  .as-chips { display: flex; flex-wrap: wrap; gap: 8px; }
  .as-chip {
    display: inline-flex; align-items: center; gap: 7px; padding: 6px 12px; border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--as-green,#07c160) 24%, var(--wc-border));
    background: color-mix(in srgb, var(--as-green,#07c160) 7%, var(--wc-card));
    font-size: 12.5px; color: var(--wc-text);
  }
  .as-chip em { font-style: normal; font-size: 11.5px; color: var(--wc-muted); font-variant-numeric: tabular-nums; }
  .as-emoji-grid { display: grid; grid-template-columns: repeat(8, 1fr); gap: 8px; }
  .as-emoji-cell { display: flex; flex-direction: column; align-items: center; gap: 4px; padding: 10px 4px; border-radius: 12px; background: var(--wc-bg2); }
  .as-emoji-char { font-size: 24px; line-height: 1.3; filter: grayscale(.15); }
  .as-emoji-top { transform: scale(1.12); }
  .as-emoji-count { font-size: 11.5px; color: var(--wc-muted); font-variant-numeric: tabular-nums; }

  /* 排行榜 */
  .as-rank { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 11px; }
  .as-rank li { display: flex; align-items: center; gap: 10px; font-size: 12.5px; }
  .as-rank-idx { width: 24px; color: var(--wc-muted); font-size: 11.5px; font-variant-numeric: tabular-nums; letter-spacing: .04em; }
  .as-rank li:first-child .as-rank-idx { color: var(--as-green-ink,#048a4a); font-weight: 800; }
  .as-rank-name { flex: 0 0 auto; max-width: 38%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .as-rank-bar { flex: 1; height: 6px; border-radius: 3px; background: var(--wc-bg2); overflow: hidden; min-width: 30px; }
  .as-rank-bar span { display: block; height: 100%; border-radius: 3px; background: color-mix(in srgb, var(--as-green,#07c160) 70%, transparent); }
  .as-rank-count { color: var(--wc-muted); font-size: 11.5px; font-variant-numeric: tabular-nums; flex-shrink: 0; text-align: right; }
  .as-rank-count em { font-style: normal; color: var(--wc-text2); margin-left: 6px; }

  /* 时间线 */
  .as-timeline { display: flex; flex-direction: column; gap: 0; padding-left: 6px; }
  .as-tl-item { position: relative; display: flex; gap: 14px; padding: 4px 0 16px; }
  .as-tl-item:last-child { padding-bottom: 4px; }
  .as-tl-item:not(:last-child)::before {
    content: ''; position: absolute; left: 4px; top: 16px; bottom: 0; width: 1px; background: var(--wc-border);
  }
  .as-tl-dot { position: relative; z-index: 1; width: 9px; height: 9px; margin-top: 4px; border-radius: 50%; flex-shrink: 0; }
  .as-tl-dot-first { background: var(--as-green,#07c160); box-shadow: 0 0 0 3px color-mix(in srgb, var(--as-green,#07c160) 18%, transparent); }
  .as-tl-dot-last { background: var(--wc-text2); }
  .as-tl-body { flex: 1; min-width: 0; }
  .as-tl-meta { display: flex; align-items: baseline; gap: 8px; flex-wrap: wrap; }
  .as-tl-tag { font-size: 11.5px; font-weight: 700; color: var(--as-green-ink,#048a4a); background: color-mix(in srgb, var(--as-green,#07c160) 10%, transparent); border-radius: 4px; padding: 1px 7px; }
  .as-tl-tag-last { color: var(--wc-text2); background: var(--wc-bg2); }
  .as-tl-name { font-weight: 600; font-size: 12.5px; }
  .as-tl-time { font-size: 11.5px; color: var(--wc-muted); }
  .as-tl-text {
    margin: 7px 0 0; font-size: 12.5px; line-height: 1.7; color: var(--wc-text2);
    background: var(--wc-bg2); border-radius: 10px; padding: 9px 13px; word-break: break-all;
  }

  /* 空态 / 错误 */
  .as-state { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; color: var(--wc-muted); font-size: 13px; padding: 40px; text-align: center; }
  .as-state-icon { color: var(--wc-muted); display: flex; align-items: center; justify-content: center; }
  .as-state-text { margin: 0; max-width: 420px; line-height: 1.7; }
  .as-spinner { width: 24px; height: 24px; border-radius: 50%; border: 2px solid var(--wc-border); border-top-color: var(--as-green,#07c160); animation: as-spin .8s linear infinite; }
  @keyframes as-spin { to { transform: rotate(360deg); } }

  /* 骨架屏 */
  .as-skeleton { flex: 1; padding: 18px 22px; display: flex; flex-direction: column; gap: 18px; }
  .sk { border-radius: 16px; background: linear-gradient(100deg, var(--wc-bg2) 40%, color-mix(in srgb, var(--wc-bg2) 60%, var(--wc-card)) 50%, var(--wc-bg2) 60%); background-size: 200% 100%; animation: sk-shine 1.4s ease infinite; }
  .sk-hero { height: 140px; }
  .sk-heat { height: 220px; }
  .sk-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 18px; }
  .sk-row { height: 180px; }
  @keyframes sk-shine { to { background-position: -200% 0; } }

  /* 响应式 */
  @media (max-width: 1180px) {
    .as-hero { flex-direction: column; align-items: stretch; }
    .as-stats { justify-content: space-between; }
    .as-stat { padding: 0 18px; }
    .as-stat:first-child { padding-left: 0; }
    .as-emoji-grid { grid-template-columns: repeat(4, 1fr); }
  }
  @media (max-width: 820px) {
    .as-cols { grid-template-columns: 1fr; }
    .as-sub { display: none; }
    .as-stats { flex-wrap: wrap; gap: 14px 0; }
    .as-stat { padding: 0 14px; }
  }

  @media (prefers-reduced-motion: reduce) {
    .as-heat-cell { animation: none; }
    .as-spinner, .sk { animation-duration: 0s; }
  }
</style>

