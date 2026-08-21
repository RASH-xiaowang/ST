"use client";

/**
 * 首屏 3D 动态背景：
 * 粒子场 + 线框网格地平 + 波浪面 + 光晕；
 * 滚动驱动镜头运动（回滚可逆）、鼠标视差、主题色联动、自适应降级。
 */
import { useMemo, useRef } from "react";
import * as THREE from "three";
import { useSceneCanvas } from "./useSceneCanvas";
import { StaticHero } from "./fallbacks";
import type { Quality } from "./engine";

const PARTICLE_BASE = 1400;

function themeColors(): { a: THREE.Color; b: THREE.Color; c: THREE.Color } {
  if (typeof window === "undefined") {
    return {
      a: new THREE.Color("#22d3ee"),
      b: new THREE.Color("#8b5cf6"),
      c: new THREE.Color("#ec4899"),
    };
  }
  const css = getComputedStyle(document.documentElement);
  const read = (name: string, fallback: string) =>
    css.getPropertyValue(name).trim() || fallback;
  return {
    a: new THREE.Color(read("--accent", "#22d3ee")),
    b: new THREE.Color(read("--accent-2", "#8b5cf6")),
    c: new THREE.Color(read("--accent-3", "#ec4899")),
  };
}

export function BackgroundScene({ className = "" }: { className?: string }) {
  const groupRef = useRef<THREE.Group | null>(null);

  const handlers = useMemo(
    () => {
      let particles: THREE.Points;
      let wave: THREE.Mesh;
      let grid: THREE.GridHelper;
      let glowA: THREE.Sprite;
      let glowB: THREE.Sprite;
      let uniforms: { uTime: { value: number }; uColorA: { value: THREE.Color }; uColorB: { value: THREE.Color } };
      let baseWaveOpacity = 0.14;

      const makeGlowTexture = () => {
        const c = document.createElement("canvas");
        c.width = c.height = 128;
        const g = c.getContext("2d")!;
        const grad = g.createRadialGradient(64, 64, 0, 64, 64, 64);
        grad.addColorStop(0, "rgba(255,255,255,1)");
        grad.addColorStop(0.3, "rgba(255,255,255,0.5)");
        grad.addColorStop(1, "rgba(255,255,255,0)");
        g.fillStyle = grad;
        g.fillRect(0, 0, 128, 128);
        return new THREE.CanvasTexture(c);
      };

      const init = ({ scene, camera }: {
        scene: THREE.Scene;
        camera: THREE.PerspectiveCamera;
      }) => {
        const colors = themeColors();
        camera.position.set(0, 1.6, 13);
        camera.lookAt(0, -0.4, 0);

        const group = new THREE.Group();
        groupRef.current = group;
        scene.add(group);

        // ── 粒子场（自定义着色器：柔和圆点 + 时间漂移） ──
        const count = Math.round(PARTICLE_BASE);
        const positions = new Float32Array(count * 3);
        for (let i = 0; i < count; i++) {
          positions[i * 3] = (Math.random() - 0.5) * 26;
          positions[i * 3 + 1] = (Math.random() - 0.5) * 14;
          positions[i * 3 + 2] = (Math.random() - 0.5) * 12;
        }
        const geo = new THREE.BufferGeometry();
        geo.setAttribute("position", new THREE.BufferAttribute(positions, 3));
        uniforms = {
          uTime: { value: 0 },
          uColorA: { value: colors.a },
          uColorB: { value: colors.b },
        };
        const mat = new THREE.ShaderMaterial({
          transparent: true,
          depthWrite: false,
          blending: THREE.AdditiveBlending,
          uniforms: uniforms as unknown as Record<string, THREE.IUniform>,
          vertexShader: /* glsl */ `
            uniform float uTime;
            attribute vec3 position;
            varying float vMix;
            void main() {
              vec3 p = position;
              p.y += sin(uTime * 0.5 + position.x * 0.4) * 0.5;
              p.x += cos(uTime * 0.35 + position.z * 0.5) * 0.4;
              vec4 mv = modelViewMatrix * vec4(p, 1.0);
              gl_Position = projectionMatrix * mv;
              gl_PointSize = clamp(90.0 / -mv.z, 1.0, 9.0);
              vMix = smoothstep(-7.0, 7.0, position.y) * 0.6 + 0.4;
            }
          `,
          fragmentShader: /* glsl */ `
            uniform vec3 uColorA;
            uniform vec3 uColorB;
            varying float vMix;
            void main() {
              vec2 uv = gl_PointCoord - 0.5;
              float d = length(uv);
              float alpha = smoothstep(0.5, 0.05, d) * 0.85;
              vec3 color = mix(uColorA, uColorB, vMix);
              gl_FragColor = vec4(color, alpha);
            }
          `,
        });
        particles = new THREE.Points(geo, mat);
        group.add(particles);

        // ── 线框网格地平 ──
        grid = new THREE.GridHelper(46, 34, colors.a, colors.b);
        grid.position.y = -3.1;
        const gm = grid.material as THREE.Material & { opacity: number; transparent: boolean };
        gm.opacity = 0.16;
        gm.transparent = true;
        group.add(grid);

        // ── 波浪面 ──
        const wgeo = new THREE.PlaneGeometry(34, 20, 120, 70);
        wgeo.rotateX(-Math.PI / 2.25);
        const wmat = new THREE.MeshBasicMaterial({
          color: colors.b,
          wireframe: true,
          transparent: true,
          opacity: 0.14,
        });
        wave = new THREE.Mesh(wgeo, wmat);
        wave.position.y = -2.3;
        wave.position.z = -2;
        group.add(wave);

        // ── 光晕精灵 ──
        const glowTex = makeGlowTexture();
        const glow = (color: THREE.Color, scale: number) => {
          const m = new THREE.SpriteMaterial({
            map: glowTex,
            color,
            transparent: true,
            opacity: 0.5,
            blending: THREE.AdditiveBlending,
            depthWrite: false,
          });
          const s = new THREE.Sprite(m);
          s.scale.setScalar(scale);
          group.add(s);
          return s;
        };
        glowA = glow(colors.a, 16);
        glowA.position.set(-6, 2.4, -5);
        glowB = glow(colors.c, 13);
        glowB.position.set(6.5, -1.5, -4);

        // 主题切换联动
        const onTheme = () => {
          const c = themeColors();
          uniforms.uColorA.value.copy(c.a);
          uniforms.uColorB.value.copy(c.b);
          const light = document.documentElement.dataset.theme === "light";
          (grid.material as THREE.LineBasicMaterial).color.copy(c.a);
          (grid.material as THREE.Material & { opacity: number }).opacity = light ? 0.09 : 0.16;
          (wave.material as THREE.MeshBasicMaterial).color.copy(c.b);
          baseWaveOpacity = light ? 0.07 : 0.14;
        };
        const mo = new MutationObserver(onTheme);
        mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
        return () => mo.disconnect();
      };

      const draw = ({ camera, t, dt, tier }: {
        camera: THREE.PerspectiveCamera;
        t: number;
        dt: number;
        tier: Quality;
      }) => {
        const group = groupRef.current;
        if (!group) return;
        if (uniforms) uniforms.uTime.value = t;

        // 粒子系数（自适应档位）
        const scale = tier === "high" ? 1 : tier === "medium" ? 0.55 : 0.25;

        // 滚动驱动镜头：0..1 进度 → 镜头缓升 + 前推（回滚可逆）
        const scrollP =
          typeof window !== "undefined"
            ? Math.min(1, Math.max(0, window.scrollY / Math.max(400, window.innerHeight)))
            : 0;
        const targetY = 1.6 - scrollP * 3.4;
        const targetZ = 13 - scrollP * 3;
        camera.position.y += (targetY - camera.position.y) * Math.min(1, dt * 4);
        camera.position.z += (targetZ - camera.position.z) * Math.min(1, dt * 4);

        // 鼠标视差
        const px = window.__hnsPointerX ?? 0;
        const py = window.__hnsPointerY ?? 0;
        camera.position.x += (px * 1.1 - camera.position.x) * Math.min(1, dt * 3);
        camera.lookAt(0, -0.4 - py * 0.5 + scrollP * -1.2, 0);

        group.rotation.y += dt * 0.02 * (0.4 + scale);
        // 波浪面缓慢起伏
        if (wave) {
          const wm = wave.material as THREE.MeshBasicMaterial;
          wm.opacity = baseWaveOpacity + Math.sin(t * 0.8) * 0.03;
        }
      };

      return { init, draw };
    },
    [],
  );

  const { ref, tier, failed } = useSceneCanvas(handlers);

  if (failed || tier === "off") {
    return <StaticHero className={className} />;
  }

  return (
    <div
      ref={ref}
      className={className}
      aria-hidden="true"
      data-testid="hero-canvas"
    />
  );
}

// 全局指针归一化（-1..1），由 Hero 注入
declare global {
  interface Window {
    __hnsPointerX: number;
    __hnsPointerY: number;
  }
}
