"use client";

/**
 * 3D 数据可视化：性能指标柱状图 + 数据流轨道。
 * 悬停高亮（射线拾取 + CSS 提示）、点击选中详情；
 * 自适应降级。
 */
import { useMemo, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { useSceneCanvas } from "./useSceneCanvas";
import { StaticChart } from "./fallbacks";
import type { Quality } from "./engine";

export interface VizBar {
  id: string;
  label: string;
  value: number;
  unit: string;
  color: string;
}

export function DataViz3D({ bars, labels }: { bars: VizBar[]; labels: { hint: string } }) {
  const [selected, setSelected] = useState<string | null>(null);

  const handlers = useMemo(() => {
    const barMeshes: { id: string; mesh: THREE.Mesh; base: number }[] = [];
    let controls: OrbitControls;
    let raycaster: THREE.Raycaster;
    const pointer = new THREE.Vector2(-9, -9);

    const pick = (camera: THREE.PerspectiveCamera): string | null => {
      if (!raycaster || barMeshes.length === 0) return null;
      raycaster.setFromCamera(pointer, camera);
      const hits = raycaster.intersectObjects(
        barMeshes.map((b) => b.mesh),
        false,
      );
      const first = hits[0];
      if (!first) return null;
      return (first.object.userData.barId as string) ?? null;
    };

    const init = ({ scene, camera, renderer }: {
      scene: THREE.Scene;
      camera: THREE.PerspectiveCamera;
      renderer: THREE.WebGLRenderer;
    }) => {
      camera.position.set(0, 3.4, 9.5);
      camera.lookAt(0, 1.6, 0);
      scene.add(new THREE.AmbientLight(0xffffff, 0.8));
      const light = new THREE.DirectionalLight(0xffffff, 2);
      light.position.set(5, 8, 4);
      scene.add(light);

      const n = bars.length;
      const gap = 1.6;
      const startX = -((n - 1) * gap) / 2;
      for (const b of bars) {
        const i = bars.indexOf(b);
        const h = 0.5 + (b.value / 100) * 3.2;
        const geo = new THREE.BoxGeometry(0.9, h, 0.9);
        const mat = new THREE.MeshStandardMaterial({
          color: new THREE.Color(b.color),
          roughness: 0.3,
          metalness: 0.4,
          emissive: new THREE.Color(b.color).multiplyScalar(0.18),
        });
        const mesh = new THREE.Mesh(geo, mat);
        mesh.position.set(startX + i * gap, h / 2, 0);
        mesh.userData.barId = b.id;
        scene.add(mesh);
        barMeshes.push({ id: b.id, mesh, base: h });
      }

      const base = new THREE.Mesh(
        new THREE.BoxGeometry(n * gap + 0.8, 0.12, 1.4),
        new THREE.MeshStandardMaterial({ color: 0x8b5cf6, roughness: 0.5, metalness: 0.6 }),
      );
      base.position.set(0, 0, 0);
      scene.add(base);

      const flow = new THREE.Mesh(
        new THREE.TorusGeometry(4.6, 0.015, 8, 140),
        new THREE.MeshBasicMaterial({
          color: 0x22d3ee,
          transparent: true,
          opacity: 0.5,
          blending: THREE.AdditiveBlending,
        }),
      );
      flow.rotation.x = Math.PI / 2.2;
      flow.position.y = 1.5;
      scene.add(flow);

      controls = new OrbitControls(camera, renderer.domElement);
      controls.enableDamping = true;
      controls.dampingFactor = 0.08;
      controls.minDistance = 5;
      controls.maxDistance = 16;
      controls.maxPolarAngle = Math.PI * 0.62;
      controls.minPolarAngle = Math.PI * 0.15;
      controls.enablePan = false;
      controls.autoRotate = true;
      controls.autoRotateSpeed = 0.5;

      raycaster = new THREE.Raycaster();
      const onMove = (e: PointerEvent) => {
        const rect = renderer.domElement.getBoundingClientRect();
        pointer.set(
          ((e.clientX - rect.left) / rect.width) * 2 - 1,
          -((e.clientY - rect.top) / rect.height) * 2 + 1,
        );
      };
      const onClick = () => {
        const hit = pick(camera);
        setSelected((cur) => (hit === null ? cur : cur === hit ? null : hit));
      };
      renderer.domElement.addEventListener("pointermove", onMove);
      renderer.domElement.addEventListener("click", onClick);
      return () => {
        renderer.domElement.removeEventListener("pointermove", onMove);
        renderer.domElement.removeEventListener("click", onClick);
      };
    };

    const draw = ({ t, dt, camera, tier }: {
      t: number;
      dt: number;
      camera: THREE.PerspectiveCamera;
      tier: Quality;
    }) => {
      const hover = pick(camera);
      controls.update();

      for (const { mesh, base } of barMeshes) {
        const id = mesh.userData.barId as string;
        const isHover = hover === id;
        const mat = mesh.material as THREE.MeshStandardMaterial;
        mat.emissiveIntensity = isHover ? 0.9 : 0.25 + Math.sin(t * 1.6 + base) * 0.08;
        // 悬停抬升
        const targetY = isHover ? base * 0.53 : base * 0.5;
        mesh.position.y += (targetY - mesh.position.y) * Math.min(1, dt * 8);
      }
      void tier;
    };

    return { init, draw };
  }, [bars]);

  const { ref, tier, failed } = useSceneCanvas(handlers);

  if (failed || tier === "off") {
    return <StaticChart className="h-[320px] w-full" />;
  }

  const sel = bars.find((b) => b.id === selected);

  return (
    <div data-testid="data-viz">
      <div ref={ref} className="h-[340px] w-full sm:h-[400px]" role="img" aria-label={labels.hint} />
      <p className="mt-2 text-center font-mono text-[11px] text-faint">{labels.hint}</p>
      {sel && (
        <div className="glass mx-auto mt-3 max-w-md rounded-xl p-4 text-center">
          <p className="text-sm font-semibold text-text">
            {sel.label} · <span className="text-gradient font-bold">{sel.value}{sel.unit}</span>
          </p>
        </div>
      )}
    </div>
  );
}
