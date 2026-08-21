/**
 * Three.js 自适应引擎
 * —— 质量分级（high/medium/low/off）、GPU/帧率/视口自适应、
 *    离屏暂停、prefers-reduced-motion 降级、WebGL 不可用降级为 2D。
 */
import type { WebGLRenderer } from "three";

export type Quality = "high" | "medium" | "low" | "off";

export interface QualityProfile {
  /** 渲染像素比上限 */
  pixelRatio: number;
  antialias: boolean;
  /** 粒子数量系数（场景按基础数量 × 系数） */
  particleScale: number;
  /** 阴影 */
  shadows: boolean;
  /** 帧率目标（低于则降级） */
  minFps: number;
}

export const PROFILES: Record<Exclude<Quality, "off">, QualityProfile> = {
  high: { pixelRatio: 2, antialias: true, particleScale: 1, shadows: true, minFps: 50 },
  medium: { pixelRatio: 1.5, antialias: false, particleScale: 0.55, shadows: false, minFps: 45 },
  low: { pixelRatio: 1, antialias: false, particleScale: 0.25, shadows: false, minFps: 30 },
};

export function detectWebGL(): boolean {
  if (typeof window === "undefined") return false;
  try {
    const c = document.createElement("canvas");
    return !!(c.getContext("webgl2") || c.getContext("webgl"));
  } catch {
    return false;
  }
}

export function prefersReducedMotion(): boolean {
  if (typeof window === "undefined") return false;
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** 初始质量：设备能力 + 视口 + 减动效偏好 */
export function pickInitialQuality(): Quality {
  if (!detectWebGL()) return "off";
  if (prefersReducedMotion()) return "low";
  const dpr = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
  const small = typeof window !== "undefined" && window.matchMedia("(max-width: 640px)").matches;
  const cores = (typeof navigator !== "undefined" && navigator.hardwareConcurrency) || 4;
  if (small) return dpr >= 2 && cores >= 6 ? "medium" : "low";
  if (cores >= 8 && dpr >= 1.5) return "high";
  if (cores >= 4) return "medium";
  return "low";
}

/** 帧率采样器：滚动窗口均值 */
export class FpsSampler {
  private samples: number[] = [];
  private last = 0;

  tick(now: number): void {
    if (this.last === 0) {
      this.last = now;
      return;
    }
    const dt = now - this.last;
    this.last = now;
    if (dt <= 0) return;
    this.samples.push(1000 / dt);
    if (this.samples.length > 90) this.samples.shift();
  }

  average(): number {
    if (this.samples.length < 30) return 0;
    const sum = this.samples.reduce((a, b) => a + b, 0);
    return sum / this.samples.length;
  }
}

/** 自适应降级控制器：帧率持续不达标降级，持续富余升级（不超初始档） */
export class AdaptiveQuality {
  tier: Quality;
  private readonly initial: Quality;
  private lowStreak = 0;
  private highStreak = 0;

  constructor(initial: Quality) {
    this.initial = initial;
    this.tier = initial;
  }

  profile(): QualityProfile | null {
    return this.tier === "off" ? null : PROFILES[this.tier];
  }

  sample(fps: number): Quality {
    if (this.tier === "off" || fps <= 0) return this.tier;
    const target = PROFILES[this.tier].minFps;
    if (fps < target) {
      this.lowStreak += 1;
      this.highStreak = 0;
      if (this.lowStreak >= 2 && this.tier !== "low") {
        this.tier = this.tier === "high" ? "medium" : "low";
        this.lowStreak = 0;
      }
    } else if (fps > target + 8) {
      this.highStreak += 1;
      this.lowStreak = 0;
      const order: Quality[] = ["low", "medium", "high"];
      const idx = order.indexOf(this.tier);
      const initIdx = order.indexOf(this.initial);
      if (this.highStreak >= 3 && idx < initIdx) {
        this.tier = order[idx + 1];
        this.highStreak = 0;
      }
    }
    return this.tier;
  }
}

/** 应用质量档到渲染器（像素比 / 抗锯齿不可热改，按需重建由调用方处理） */
export function applyProfileToRenderer(
  renderer: WebGLRenderer,
  tier: Quality,
): void {
  if (tier === "off") return;
  const p = PROFILES[tier];
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, p.pixelRatio));
}
