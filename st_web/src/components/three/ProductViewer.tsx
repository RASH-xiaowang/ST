"use client";

/**
 * 交互式 3D 产品模型（程序化构建的「代理核心」）：
 * 旋转/缩放/平移（OrbitControls）、爆炸视图滑块、剖面裁剪、
 * 材质/配色切换、热点标注（射线拾取 + 信息卡）、空闲自动旋转。
 * WebGL 不可用 → StaticProduct 2D 降级。
 */
import { useMemo, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { useSceneCanvas } from "./useSceneCanvas";
import { StaticProduct } from "./fallbacks";
import type { Quality } from "./engine";

export interface Hotspot {
  id: string;
  title: string;
  desc: string;
  position: [number, number, number];
}

export interface ProductViewerProps {
  hotspots: Hotspot[];
  labels: {
    rotate: string;
    explode: string;
    section: string;
    scheme: string;
    auto: string;
    reset: string;
  };
}

type Scheme = { core: string; ring: string; shell: string };

const SCHEMES: Scheme[] = [
  { core: "#22d3ee", ring: "#8b5cf6", shell: "#ec4899" },
  { core: "#f5c33b", ring: "#f97316", shell: "#ef4444" },
  { core: "#34d399", ring: "#22d3ee", shell: "#818cf8" },
];

export function ProductViewer({ hotspots, labels }: ProductViewerProps) {
  const [explode, setExplode] = useState(0);
  const [section, setSection] = useState(false);
  const [schemeIdx, setSchemeIdx] = useState(0);
  const [activeHotspot, setActiveHotspot] = useState<string | null>(null);

  const uiRef = useRef({ explode: 0, section: false, scheme: 0, active: null as string | null });
  uiRef.current = { explode, section, scheme: schemeIdx, active: activeHotspot };

  const handlers = useMemo(() => {
    let coreGroup: THREE.Group;
    const ringMeshes: THREE.Mesh[] = [];
    const ringBase: THREE.Vector3[] = [];
    const hotspotSpheres: { id: string; mesh: THREE.Mesh; base: THREE.Vector3 }[] = [];
    let controls: OrbitControls;
    let clipPlane: THREE.Plane | null = null;
    let coreMat: THREE.MeshStandardMaterial;
    let shellMat: THREE.MeshStandardMaterial;
    let ringMat: THREE.MeshStandardMaterial;
    let autoRotate = true;

    const applyScheme = (s: Scheme) => {
      coreMat.color.set(s.core);
      coreMat.emissive.set(new THREE.Color(s.core).multiplyScalar(0.35));
      ringMat.color.set(s.ring);
      shellMat.color.set(s.shell);
      shellMat.emissive.set(new THREE.Color(s.shell).multiplyScalar(0.25));
    };

    const init = ({ scene, camera, renderer }: {
      scene: THREE.Scene;
      camera: THREE.PerspectiveCamera;
      renderer: THREE.WebGLRenderer;
    }) => {
      camera.position.set(0, 1.6, 7.6);
      camera.lookAt(0, 0, 0);

      scene.add(new THREE.AmbientLight(0xffffff, 0.7));
      const key = new THREE.DirectionalLight(0xffffff, 2.4);
      key.position.set(6, 8, 5);
      scene.add(key);
      const rim = new THREE.DirectionalLight(0x8b5cf6, 1.6);
      rim.position.set(-5, -2, -6);
      scene.add(rim);

      coreGroup = new THREE.Group();
      scene.add(coreGroup);

      // 内核心（实体 + 线框壳）
      const ico = new THREE.IcosahedronGeometry(1.05, 1);
      coreMat = new THREE.MeshStandardMaterial({
        color: SCHEMES[0].core,
        roughness: 0.25,
        metalness: 0.55,
        emissive: new THREE.Color(SCHEMES[0].core).multiplyScalar(0.35),
      });
      const coreMesh = new THREE.Mesh(ico, coreMat);
      coreGroup.add(coreMesh);

      const shellGeo = new THREE.IcosahedronGeometry(1.32, 0);
      shellMat = new THREE.MeshStandardMaterial({
        color: SCHEMES[0].shell,
        wireframe: true,
        emissive: new THREE.Color(SCHEMES[0].shell).multiplyScalar(0.25),
        roughness: 0.4,
      });
      const shell = new THREE.Mesh(shellGeo, shellMat);
      coreGroup.add(shell);

      // 三个轨道环（可爆炸展开）
      ringMat = new THREE.MeshStandardMaterial({
        color: SCHEMES[0].ring,
        roughness: 0.3,
        metalness: 0.6,
      });
      const ringDefs: { r: number; tube: number; rot: [number, number, number] }[] = [
        { r: 1.95, tube: 0.045, rot: [0.5, 0, 0.2] },
        { r: 2.45, tube: 0.04, rot: [1.15, 0.35, 0] },
        { r: 2.9, tube: 0.035, rot: [0.7, -0.5, 0.8] },
      ];
      for (const def of ringDefs) {
        const geo = new THREE.TorusGeometry(def.r, def.tube, 12, 110);
        const mesh = new THREE.Mesh(geo, ringMat);
        mesh.rotation.set(...def.rot);
        coreGroup.add(mesh);
        ringMeshes.push(mesh);
        ringBase.push(new THREE.Vector3(...def.rot));
      }

      // 轨道粒子
      const pCount = 260;
      const pos = new Float32Array(pCount * 3);
      for (let i = 0; i < pCount; i++) {
        const th = Math.random() * Math.PI * 2;
        const r = 2.2 + Math.random() * 1.1;
        const y = (Math.random() - 0.5) * 2.4;
        pos[i * 3] = Math.cos(th) * r;
        pos[i * 3 + 1] = y;
        pos[i * 3 + 2] = Math.sin(th) * r;
      }
      const pGeo = new THREE.BufferGeometry();
      pGeo.setAttribute("position", new THREE.BufferAttribute(pos, 3));
      const pMat = new THREE.PointsMaterial({
        color: 0x8b5cf6,
        size: 0.035,
        transparent: true,
        opacity: 0.8,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      });
      const points = new THREE.Points(pGeo, pMat);
      points.name = "orbit-points";
      coreGroup.add(points);

      // 热点球
      for (const h of hotspots) {
        const mesh = new THREE.Mesh(
          new THREE.SphereGeometry(0.09, 18, 18),
          new THREE.MeshBasicMaterial({ color: 0xf5c33b }),
        );
        mesh.position.set(...h.position);
        mesh.userData.hotspotId = h.id;
        coreGroup.add(mesh);
        hotspotSpheres.push({ id: h.id, mesh, base: mesh.position.clone() });
      }

      // 地面光环
      const glowRing = new THREE.Mesh(
        new THREE.TorusGeometry(3.4, 0.02, 8, 120),
        new THREE.MeshBasicMaterial({
          color: 0x22d3ee,
          transparent: true,
          opacity: 0.5,
          blending: THREE.AdditiveBlending,
        }),
      );
      glowRing.rotation.x = Math.PI / 2;
      glowRing.position.y = -2.1;
      scene.add(glowRing);

      controls = new OrbitControls(camera, renderer.domElement);
      controls.enableDamping = true;
      controls.dampingFactor = 0.08;
      controls.minDistance = 4;
      controls.maxDistance = 12;
      controls.maxPolarAngle = Math.PI * 0.85;
      controls.autoRotate = true;
      controls.autoRotateSpeed = 0.8;
      controls.addEventListener("start", () => (autoRotate = false));
      controls.addEventListener("end", () => (autoRotate = true));
    };

    const draw = ({ t, dt, camera, tier }: {
      t: number;
      dt: number;
      camera: THREE.PerspectiveCamera;
      tier: Quality;
    }) => {
      const ui = uiRef.current;
      coreGroup.rotation.y += dt * (autoRotate ? 0.25 : 0.04);

      // 爆炸视图：环沿自身法向外扩
      const ex = ui.explode;
      ringMeshes.forEach((m, i) => {
        m.rotation.set(ringBase[i].x, ringBase[i].y, ringBase[i].z);
        const out = new THREE.Vector3(0, 0, 1).applyEuler(m.rotation).normalize();
        m.position.copy(out.multiplyScalar(ex * (0.9 + i * 0.55)));
      });
      // 剖面裁剪
      const needsClip = ui.section;
      const mats: THREE.Material[] = [coreMat, shellMat, ringMat];
      if (needsClip) {
        if (!clipPlane) clipPlane = new THREE.Plane(new THREE.Vector3(1, 0.4, 0), 0);
        mats.forEach((m) => {
          m.clippingPlanes = [clipPlane!];
          m.clipShadows = false;
        });
      } else {
        mats.forEach((m) => (m.clippingPlanes = []));
        clipPlane = null;
      }

      // 配色
      applyScheme(SCHEMES[ui.scheme % SCHEMES.length]);

      // 热点：选中高亮脉冲
      for (const h of hotspotSpheres) {
        const active = ui.active === h.id;
        const s = 1 + (active ? Math.sin(t * 5) * 0.35 + 0.35 : 0);
        h.mesh.scale.setScalar(s);
        (h.mesh.material as THREE.MeshBasicMaterial).color.set(active ? 0xffffff : 0xf5c33b);
      }

      controls.update();
      void tier;
      void camera;
    };

    return { init, draw };
  }, [hotspots]);

  const { ref, tier, failed } = useSceneCanvas(handlers);

  if (failed || tier === "off") {
    return <StaticProduct className="h-[380px]" />;
  }

  return (
    <div className="relative" data-testid="product-viewer">
      <div ref={ref} className="h-[380px] w-full sm:h-[460px]" aria-label={labels.rotate} role="img" />
      {/* 控制条 */}
      <div className="mt-4 flex flex-wrap items-center gap-3 rounded-xl border border-border bg-surface px-4 py-3">
        <label className="flex flex-1 min-w-[220px] items-center gap-3 text-xs text-muted">
          <span className="shrink-0">{labels.explode}</span>
          <input
            type="range"
            min={0}
            max={1}
            step={0.01}
            value={explode}
            onChange={(e) => setExplode(Number(e.target.value))}
            className="h-1.5 flex-1 accent-[var(--accent)]"
            aria-label={labels.explode}
          />
          <span className="w-8 shrink-0 text-right font-mono">{Math.round(explode * 100)}%</span>
        </label>
        <button
          onClick={() => setSection(!section)}
          aria-pressed={section}
          className={`rounded-lg border px-3 py-1.5 text-xs transition ${
            section ? "border-accent bg-accent/15 text-accent" : "border-border text-muted hover:text-text"
          }`}
        >
          {labels.section}
        </button>
        <button
          onClick={() => setSchemeIdx((i) => (i + 1) % SCHEMES.length)}
          className="rounded-lg border border-border px-3 py-1.5 text-xs text-muted transition hover:text-text"
        >
          {labels.scheme} {schemeIdx + 1}/{SCHEMES.length}
        </button>
        <button
          onClick={() => {
            setExplode(0);
            setSection(false);
            setActiveHotspot(null);
          }}
          className="rounded-lg border border-border px-3 py-1.5 text-xs text-muted transition hover:text-text"
        >
          {labels.reset}
        </button>
      </div>
      {/* 热点图例 */}
      <div className="mt-3 flex flex-wrap gap-2">
        {hotspots.map((h) => (
          <button
            key={h.id}
            onClick={() => setActiveHotspot(activeHotspot === h.id ? null : h.id)}
            aria-pressed={activeHotspot === h.id}
            className={`rounded-full border px-3 py-1.5 text-xs transition ${
              activeHotspot === h.id
                ? "border-gold bg-gold/10 text-gold"
                : "border-border text-muted hover:text-text"
            }`}
          >
            ● {h.title}
          </button>
        ))}
      </div>
      {activeHotspot && (
        <div className="glass mt-3 rounded-xl p-4" data-testid="hotspot-card">
          <p className="text-sm font-semibold text-text">
            {hotspots.find((h) => h.id === activeHotspot)?.title}
          </p>
          <p className="mt-1 text-[13px] leading-relaxed text-muted">
            {hotspots.find((h) => h.id === activeHotspot)?.desc}
          </p>
        </div>
      )}
    </div>
  );
}
