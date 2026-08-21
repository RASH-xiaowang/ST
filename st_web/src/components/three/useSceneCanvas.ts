"use client";

/**
 * 3D 画布生命周期 Hook：
 * - 创建 WebGLRenderer/场景/相机，ResizeObserver 同步尺寸
 * - IntersectionObserver：离屏暂停 rAF，回屏恢复
 * - prefers-reduced-motion：渲染单帧静态画面，不启动动画循环
 * - FPS 采样与自动降档（AdaptiveQuality）
 * - WebGL 不可用 / 上下文丢失：报告失败，由调用方渲染 2D 降级
 */
import { useCallback, useEffect, useRef, useState } from "react";
import * as THREE from "three";
import {
  AdaptiveQuality,
  applyProfileToRenderer,
  detectWebGL,
  pickInitialQuality,
  prefersReducedMotion,
  type Quality,
} from "./engine";

export interface SceneHandlers {
  init?: (ctx: {
    renderer: THREE.WebGLRenderer;
    scene: THREE.Scene;
    camera: THREE.PerspectiveCamera;
  }) => void | (() => void);
  /** 每帧回调；t 为累计秒数，dt 为帧间隔 */
  draw?: (ctx: {
    renderer: THREE.WebGLRenderer;
    scene: THREE.Scene;
    camera: THREE.PerspectiveCamera;
    t: number;
    dt: number;
    width: number;
    height: number;
    tier: Quality;
  }) => void;
  /** 质量档变化（粒子系数等场景参数重新计算） */
  onTier?: (tier: Quality) => void;
}

export type CanvasHandle = {
  ref: (el: HTMLDivElement | null) => void;
  tier: Quality;
  failed: boolean;
};

export function useSceneCanvas(handlers: SceneHandlers): CanvasHandle {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [tier, setTier] = useState<Quality>(() => pickInitialQuality());
  const [failed, setFailed] = useState(false);
  const hRef = useRef(handlers);
  hRef.current = handlers;

  const setRef = useCallback((el: HTMLDivElement | null) => {
    containerRef.current = el;
  }, []);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    if (pickInitialQuality() === "off" || !detectWebGL()) {
      setFailed(true);
      setTier("off");
      return;
    }

    const controller = new AdaptiveQuality(pickInitialQuality());

    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({
        antialias: controller.profile()?.antialias ?? false,
        alpha: true,
        powerPreference: "high-performance",
      });
    } catch {
      setFailed(true);
      setTier("off");
      return;
    }
    applyProfileToRenderer(renderer, controller.tier);
    const canvas = renderer.domElement;
    canvas.style.display = "block";
    canvas.style.width = "100%";
    canvas.style.height = "100%";
    container.appendChild(canvas);

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(50, 1, 0.1, 120);

    let cleanup: (() => void) | undefined;
    try {
      cleanup = hRef.current.init?.({ renderer, scene, camera }) ?? undefined;
    } catch {
      setFailed(true);
      setTier("off");
      renderer.dispose();
      canvas.remove();
      return;
    }

    const size = { w: 1, h: 1 };
    const resize = () => {
      const rect = container.getBoundingClientRect();
      size.w = Math.max(1, rect.width);
      size.h = Math.max(1, rect.height);
      renderer.setSize(size.w, size.h, false);
      camera.aspect = size.w / size.h;
      camera.updateProjectionMatrix();
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(container);

    let running = false;
    let inView = true;
    const reduced = prefersReducedMotion();

    const io = new IntersectionObserver(
      (entries) => {
        inView = entries[0]?.isIntersecting ?? true;
        if (inView && !running && !reduced) startLoop();
      },
      { threshold: 0 },
    );
    io.observe(container);

    let raf = 0;
    let last = performance.now();
    let elapsed = 0;
    let frameCount = 0;
    let fpsAccum = 0;
    let fpsWindowStart = performance.now();

    const frame = (now: number) => {
      raf = requestAnimationFrame(frame);
      const dt = Math.min(0.1, (now - last) / 1000);
      last = now;
      elapsed += dt;
      frameCount += 1;
      fpsAccum += dt;
      // 每 ~0.9s 评估一次帧率并自适应升降档
      if (now - fpsWindowStart >= 900) {
        const avgFps = fpsAccum > 0 ? frameCount / fpsAccum : 0;
        frameCount = 0;
        fpsAccum = 0;
        fpsWindowStart = now;
        const next = controller.sample(avgFps);
        if (next !== controller.tier) {
          controller.tier = next;
          setTier(next);
          hRef.current.onTier?.(next);
          applyProfileToRenderer(renderer, next);
        }
      }
      hRef.current.draw?.({
        renderer,
        scene,
        camera,
        t: elapsed,
        dt,
        width: size.w,
        height: size.h,
        tier: controller.tier,
      });
      renderer.render(scene, camera);
    };

    const startLoop = () => {
      if (running || reduced || !inView) return;
      running = true;
      last = performance.now();
      raf = requestAnimationFrame(frame);
    };

    if (reduced) {
      // 减动效：渲染单帧静态画面
      hRef.current.draw?.({
        renderer,
        scene,
        camera,
        t: 0,
        dt: 0,
        width: size.w,
        height: size.h,
        tier: controller.tier,
      });
      renderer.render(scene, camera);
    } else {
      startLoop();
    }

    const stop = () => {
      cancelAnimationFrame(raf);
      running = false;
      io.disconnect();
      ro.disconnect();
      cleanup?.();
      renderer.dispose();
      canvas.remove();
    };

    // WebGL 上下文丢失：上报失败（降级 2D）
    const onLost = (e: Event) => {
      e.preventDefault();
      stop();
      setFailed(true);
      setTier("off");
    };
    canvas.addEventListener("webglcontextlost", onLost);

    return () => {
      canvas.removeEventListener("webglcontextlost", onLost);
      stop();
    };
  }, []);

  return { ref: setRef, tier, failed };
}
