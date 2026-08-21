<script lang="ts">
  // Gargantua 黑洞背景（本地 Three.js 相对论光线追踪）
  // 移植自 Kimi 分享的 GARGANTUA 页面：Schwarzschild 黑洞 + 吸积盘 + 引力透镜 + 电影镜头。
  // 纯背景模式（?bg=1）：隐藏 HUD/开场/交互/音频，自动循环电影镜头，低档画质保证流畅。
  // 资源位于 public/gargantua/，完全本地运行，无外部请求。
  // 加载期：iframe 就绪前显示深空兜底渐变，就绪后淡入，避免白闪。
  // 性能：IntersectionObserver 检测到容器不可见时 postMessage 暂停 WebGL 渲染。
  import { gargantuaFrameUrl } from '../utils/backdrop';
  let ready = $state(false);
  let {
    steps,
    /** 锁定构图（poster/edge/polar/close），如 poster = 黑洞居中 + 吸积盘 38° */
    cam,
    /** 关闭电影镜头循环，保持静止构图 */
    motion = true,
    /** 吸积盘亮度倍率（默认 1） */
    bright,
    /** 星空亮度倍率（默认 1） */
    star,
    /** 天光底色（0-0.15，默认 0.04） */
    sky,
  }: {
    steps?: number;
    cam?: string;
    motion?: boolean;
    bright?: number;
    star?: number;
    sky?: number;
  } = $props();

  const frameSrc = $derived(gargantuaFrameUrl({ steps, cam, motion, bright, star, sky }));

  let rootEl: HTMLDivElement | undefined;
  let frameEl: HTMLIFrameElement | undefined;
  $effect(() => {
    const el = rootEl;
    const frame = frameEl;
    if (!el || !frame) return;
    const io = new IntersectionObserver(
      ([entry]) => {
        const visible = entry?.isIntersecting ?? true;
        frame.contentWindow?.postMessage(
          { type: visible ? 'gargantua-resume' : 'gargantua-pause' },
          '*'
        );
      },
      { threshold: 0.05 }
    );
    io.observe(el);
    return () => io.disconnect();
  });
</script>

<div class="ga-backdrop" aria-hidden="true" bind:this={rootEl}>
  <div class="ga-fallback" class:ga-fade={ready}></div>
  <iframe
    class="ga-frame"
    class:ga-loaded={ready}
    bind:this={frameEl}
    src={frameSrc}
    title="Gargantua 黑洞背景"
    tabindex="-1"
    loading="eager"
    onload={() => (ready = true)}
  ></iframe>
</div>

<style>
  .ga-backdrop {
    position: absolute;
    inset: 0;
    overflow: hidden;
    z-index: 0;
    background: #05060a;
  }
  .ga-frame {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    border: 0;
    display: block;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.55s ease;
  }
  .ga-frame.ga-loaded { opacity: 1; }
  /* 加载期兜底渐变：iframe 就绪后淡出，避免白闪 */
  .ga-fallback {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(120% 90% at 50% 38%, rgba(127, 220, 255, 0.14), rgba(5, 6, 10, 0.55) 55%, #05060a 82%);
    transition: opacity 0.55s ease;
  }
  .ga-fallback.ga-fade { opacity: 0; }
  @media (prefers-reduced-motion: reduce) {
    .ga-frame, .ga-fallback { transition: none; }
  }
</style>
