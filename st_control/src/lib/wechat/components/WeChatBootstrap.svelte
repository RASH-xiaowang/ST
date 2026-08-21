<script lang="ts">
  import { onMount } from 'svelte';
  import { delay } from '../../async';
  import {
    detectWechatAccounts,
    getWechatConfig,
    getWechatDbStatus,
    getWechatKeysInfo,
  } from '../services/ipc';
  import type { DetectedAccount, WechatConfigData, WechatConfigResult } from '../types';
  import { getMonitorStatus } from '../services/ipc';
    import WechatHoverButton from './WechatHoverButton.svelte';
  import GargantuaBackdrop from './GargantuaBackdrop.svelte';

  interface Props {
    /** 初始化完成 */
    ondone?: (summary: string) => void;
    /** 引导前往微信配置页 */
    onconfig?: () => void;
  }
  let { ondone, onconfig }: Props = $props();

  // 启动页背景：Gargantua 黑洞光线追踪（见 GargantuaBackdrop.svelte）

  let percent = $state(0);
  let statusZh = $state('正在初始化…');
  let statusEn = $state('INITIALIZING');
  let phase = $state<'loading' | 'done' | 'blocked'>('loading');
  let summary = $state('');
  let blockedItems = $state<string[]>([]);

  // WeFlow 风格：中文状态 → 英文大写状态
  const EN_MAP: Record<string, string> = {
    '配置': 'LOADING CONFIGURATION',
    '账号': 'DETECTING ACCOUNTS',
    '密钥': 'LOADING KEYS',
    '数据库': 'CONNECTING DATABASE',
    '监控': 'STARTING MONITOR',
    '完成': 'READY',
    '初始化': 'INITIALIZING SYSTEM',
  };

  function setProgress(p: number, zh: string) {
    percent = Math.max(0, Math.min(100, Math.round(p)));
    statusZh = zh;
    for (const [k, v] of Object.entries(EN_MAP)) {
      if (zh.includes(k)) {
        statusEn = v;
        return;
      }
    }
    statusEn = 'PROCESSING';
  }

  onMount(async () => {
    const blocked: string[] = [];
    try {
      // 1) 加载配置
      setProgress(8, '正在加载配置…');
      let cfg: WechatConfigResult | null = null;
      try {
    cfg = await getWechatConfig();
      } catch (e) {
        console.error('[WeChatBootstrap] get_wechat_config', e);
      }
      const resolved = (cfg?.resolved ?? cfg?.config ?? {}) as WechatConfigData;
      const dbDir = resolved.db_dir || '';
      await delay(180);
      setProgress(22, '正在检测微信账号…');

      // 2) 检测微信账号
      let accounts: DetectedAccount[] = [];
      try {
    const list = await detectWechatAccounts();
        accounts = Array.isArray(list) ? list : [];
      } catch (e) {
        console.error('[WeChatBootstrap] detect_wechat_accounts', e);
      }
      await delay(220);
      setProgress(45, `发现 ${accounts.length} 个微信账号`);

      // 3) 检查密钥
      setProgress(58, '正在加载密钥配置…');
      let keyCount = 0;
      try {
    const ki = await getWechatKeysInfo();
        keyCount = ki?.keyCount ?? 0;
      } catch (e) {
        console.error('[WeChatBootstrap] get_wechat_keys_info', e);
      }
      await delay(220);
      setProgress(72, keyCount > 0 ? `密钥就绪（${keyCount} 个）` : '密钥未配置');

      // 4) 数据库状态
      setProgress(80, '正在连接数据库…');
      let dbOk = 0;
      let dbTotal = 0;
      try {
    const list = await getWechatDbStatus();
        if (Array.isArray(list)) {
          dbTotal = list.length;
          dbOk = list.filter((s) => /✅|可用|ok|正常|成功|ready/i.test(s)).length;
        }
      } catch (e) {
        console.error('[WeChatBootstrap] get_wechat_db_status', e);
      }
      await delay(200);
      setProgress(90, dbTotal > 0 ? `数据库就绪（${dbOk}/${dbTotal}）` : '数据库状态未知');

      // 5) 监控状态
      setProgress(95, '正在启动监控…');
      let monitoring = false;
      try {
        const st = await getMonitorStatus();
        monitoring = !!st?.running;
      } catch (e) {
        console.error('[WeChatBootstrap] getMonitorStatus', e);
      }
      await delay(180);

      // 完成条件评估
      const parts: string[] = [];
      if (!dbDir) {
        blocked.push('未配置微信数据库目录');
      } else {
        parts.push(`数据库目录已配置`);
      }
      if (keyCount === 0) {
        blocked.push('解密密钥未配置（需一键自动获取或手动填写）');
      } else {
        parts.push(`密钥 ${keyCount} 个`);
      }
      if (accounts.length === 0) {
        parts.push('未检测到账号（可在配置页手动指定）');
      }
      if (monitoring) {
        parts.push('监控运行中');
      }
      summary = parts.join(' · ') || '初始化完成';

      if (blocked.length > 0) {
        blockedItems = blocked;
        setProgress(100, '初始化未完成');
        phase = 'blocked';
        return;
      }

      setProgress(100, '初始化完成，正在进入…');
      phase = 'done';
      await delay(650);
      ondone?.(summary);
    } catch (e) {
      console.error('[WeChatBootstrap] 初始化失败', e);
      blockedItems = ['初始化过程发生异常，请检查配置'];
      setProgress(100, '初始化失败');
      phase = 'blocked';
    }
  });
</script>

<div class="wbs">
  <!-- 启动页黑洞：poster 构图（居中 + 吸积盘 38°）锁定，关闭电影镜头，
       提高吸积盘/星空亮度，略微提升画质，保证初始化瞬间画面稳定耐看 -->
  <GargantuaBackdrop steps={170} cam="poster" motion={false} bright={1.55} star={1.45} sky={0.055} />
  {#if phase === 'blocked'}
    <div class="wbs-strip wbs-strip-blocked">
      <div class="wbs-strip-main">
        <span class="wbs-dot wbs-dot-danger"></span>
        <span class="wbs-blocked-title">初始化未完成</span>
        <ul class="wbs-blocked-list">
          {#each blockedItems as item}
            <li>{item}</li>
          {/each}
        </ul>
        <span class="wbs-spacer"></span>
        <WechatHoverButton text="前往微信配置" onclick={() => onconfig?.()} />
      </div>
    </div>
  {:else}
    <div class="wbs-strip" class:wbs-done={phase === 'done'}>
      <div class="wbs-strip-main">
        <span class="wbs-mark" aria-hidden="true">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8">
            <circle cx="12" cy="12" r="2.4" />
            <path d="M12 1.5v3M12 19.5v3M1.5 12h3M19.5 12h3M4.6 4.6l2.1 2.1M17.3 17.3l2.1 2.1M4.6 19.4l2.1-2.1M17.3 6.7l2.1-2.1" />
          </svg>
        </span>
        <span class="wbs-title">微信数据管理</span>
        <span class="wbs-status-zh">{statusZh}</span>
        <span class="wbs-spacer"></span>
        <span class="wbs-status-en">{statusEn}</span>
        <span class="wbs-pct">{percent}<span class="wbs-pct-sym">%</span></span>
      </div>
      <div class="wbs-progress">
        <div class="wbs-track">
          <div class="wbs-fill" class:indeterminate={phase === 'loading' && percent >= 95} style="width:{percent}%"></div>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .wbs {
    position: relative;
    min-height: calc(100vh - 120px);
    padding: 24px;
    overflow: hidden;
    background: var(--app-bg-color);
  }
  /* 长条 HUD：横贯底部的毛玻璃进度条，黑洞主体完全可见 */
  .wbs-strip {
    position: absolute;
    z-index: 1;
    left: 28px;
    right: 28px;
    bottom: 26px;
    min-height: 58px;
    padding: 12px 20px 10px;
    border-radius: 14px;
    background: color-mix(in srgb, var(--app-color-card-bg) 26%, transparent);
    border: 1px solid color-mix(in srgb, var(--app-color-border) 40%, transparent);
    box-shadow: 0 8px 28px rgba(0,0,0,0.18), 0 1px 6px rgba(0,0,0,0.10);
    backdrop-filter: blur(9px) saturate(1.08);
    -webkit-backdrop-filter: blur(9px) saturate(1.08);
    color: var(--app-font-color, inherit);
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 10px;
    animation: wbs-rise 0.55s cubic-bezier(0.16,1,0.3,1) both;
  }
  .wbs-done { animation: wbs-pulse 0.7s ease both; }
  @keyframes wbs-rise {
    from { opacity: 0; transform: translateY(16px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  @keyframes wbs-pulse {
    0% { opacity: 1; }
    50% { opacity: 0.72; }
    100% { opacity: 1; }
  }

  .wbs-strip-main {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .wbs-mark {
    width: 26px; height: 26px;
    border-radius: 8px;
    flex-shrink: 0;
    display: grid;
    place-items: center;
    color: var(--app-wc-accent, #576b95);
    background: color-mix(in srgb, var(--app-wc-accent, #576b95) 16%, transparent);
    box-shadow: 0 0 14px color-mix(in srgb, var(--app-wc-accent, #576b95) 35%, transparent);
  }
  .wbs-title {
    font-size: 13.5px;
    font-weight: 650;
    margin: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 0;
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.55);
  }
  .wbs-status-zh {
    font-size: 12px;
    color: var(--app-color-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 0 1 auto;
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.55);
  }
  .wbs-spacer { flex: 1; min-width: 12px; }
  .wbs-status-en {
    font-size: 10px;
    letter-spacing: 0.2em;
    color: var(--app-color-muted);
    font-family: var(--font-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
    white-space: nowrap;
    flex-shrink: 0;
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.55);
  }
  .wbs-pct {
    font-size: 17px;
    font-weight: 400;
    color: var(--app-wc-accent, #576b95);
    font-variant-numeric: tabular-nums;
    font-family: var(--font-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
    white-space: nowrap;
    min-width: 48px;
    text-align: right;
    flex-shrink: 0;
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.55);
  }
  .wbs-pct-sym { font-size: 11px; color: var(--app-color-muted); }
  .wbs-progress { display: flex; flex-direction: column; gap: 9px; }
  .wbs-track {
    position: relative;
    width: 100%;
    height: 2px;
    background: color-mix(in srgb, var(--app-color-border) 70%, transparent);
    border-radius: 2px;
    overflow: hidden;
  }
  .wbs-fill {
    position: absolute;
    top: 0; left: 0;
    height: 100%;
    width: 0%;
    background: linear-gradient(90deg, color-mix(in srgb, var(--app-wc-accent, #576b95) 70%, transparent), var(--app-wc-accent, #576b95) 60%, color-mix(in srgb, var(--app-wc-accent, #576b95) 80%, #fff));
    border-radius: 2px;
    box-shadow: 0 0 10px color-mix(in srgb, var(--app-wc-accent, #576b95) 65%, transparent);
    transition: width 0.35s ease-out;
  }
  .wbs-fill.indeterminate {
    width: 100% !important;
    background: linear-gradient(90deg, transparent, var(--app-wc-accent) 45%, color-mix(in srgb, var(--app-wc-accent) 55%, white) 50%, var(--app-wc-accent) 55%, transparent);
    background-size: 200% 100%;
    animation: wbs-flow 1.6s linear infinite;
  }
  @keyframes wbs-flow {
    0% { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }
  .wbs-strip-blocked .wbs-strip-main { flex-wrap: wrap; row-gap: 8px; }
  .wbs-blocked-title { font-size: 13.5px; font-weight: 700; margin: 0; color: var(--app-danger, #dc2626); white-space: nowrap; }
  .wbs-blocked-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    min-width: 0;
  }
  .wbs-blocked-list li {
    font-size: 11.5px;
    color: var(--app-color-muted);
    padding: 4px 10px;
    border: 1px solid var(--app-color-border);
    border-radius: 999px;
    background: var(--app-color-surface-alt);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 320px;
    line-height: 1.5;
  }
  .wbs-dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .wbs-dot-danger { background: var(--app-danger, #dc2626); box-shadow: 0 0 8px var(--app-danger, #dc2626); }
</style>
