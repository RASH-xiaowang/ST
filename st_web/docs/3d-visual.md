# 3D 视觉设计与交互说明

## 1. 引擎架构（`src/components/three/`）

| 模块 | 职责 |
|---|---|
| `engine.ts` | 质量档位 `high/medium/low/off`、WebGL 探测、减动效偏好、FPS 采样器、自适应升降档控制器、渲染器参数应用 |
| `useSceneCanvas.ts` | 画布生命周期 Hook：renderer/scene/camera 创建、ResizeObserver、IntersectionObserver 离屏暂停/回屏恢复、rAF 统一循环、上下文丢失降级 |
| `BackgroundScene.tsx` | 首屏 3D 背景：粒子场（自定义着色器，柔和圆点 + 时间漂移 + 三色渐变）、线框网格地平、波浪面、光晕精灵；**滚动驱动镜头**（scrollY→camera.y/z 插值，回滚可逆）+ 鼠标视差；主题切换经 MutationObserver 联动配色 |
| `ProductViewer.tsx` | 交互式「代理核心」模型：OrbitControls（旋转/缩放/平移）、**爆炸视图滑块**（轨道环沿法向外扩）、**剖面裁剪**（clippingPlane）、**配色切换**（3 套材质方案）、**热点标注**（选中脉冲高亮 + 信息卡）、空闲自动旋转、交互时暂停 |
| `DataViz3D.tsx` | 3D 性能柱状图 + 数据流轨道：射线拾取悬停高亮/点击选中详情、自动旋转、自适应粒子 |
| `fallbacks.tsx` | 2D 静态降级视觉（SVG 六边形主视觉 / 产品图 / CSS 柱状图） |

## 2. 三个 3D 元素实现要点

**动态 3D 背景**（首屏/全局氛围）
- 粒子：`BufferGeometry` + `ShaderMaterial`（AdditiveBlending，顶点着色器按时间做正弦漂移，片元以径向渐变做柔和圆点），数量按档位缩放（1400×粒子系数）。
- 滚动叙事：`window.scrollY / innerHeight` 归一为 0..1，相机 y 1.6→-1.8、z 13→10 插值，回滚可逆；进入产品总览后背景自然退场（镜头随滚动下沉）。
- 主题联动：读取 CSS 变量 `--accent/-2/-3` 注入着色器 uniform，`data-theme` 变化时经 MutationObserver 热更新。

**交互式 3D 模型**（产品总览「把运行时的每一层拆开看」）
- 模型为**程序化构建**（Icosahedron 核心 + 线框壳 + 3 轨道环 + 粒子带 + 底座光晕），不依赖外部 GLB——零二进制资产、首屏更轻；模型定义即源码（本目录 TS 模块），可按需替换为 glTF/GLB（加载器路径与 DRACO/Meshopt 说明见下）。
- 爆炸视图：0..100% 滑块 → 环沿自身法线方向平移（0.9/1.45/2.0 倍径）。
- 剖面：`THREE.Plane` 附加到全部核心材质 `clippingPlanes`。
- 热点：球体节点 + `userData.hotspotId`；射线拾取在 pointermove/click 上驱动图例按钮与信息卡。

**3D 数据可视化**（架构与性能）
- 指标 → 等宽 BoxGeometry 柱（高度映射数值），悬停发光+抬升（位置插值）、点击选中详情卡；Torus 数据流轨道 + 自动旋转。

## 3. 性能自适应策略

1. **初始档位**：WebGL2 不可用 → `off`；`prefers-reduced-motion` → `low`（单帧静态）；移动视口 → `medium/low`；桌面按硬件并发数与 DPR 分档。
2. **运行时升降档**：每 ~0.9s 统计滚动帧率——连续 2 个窗口低于档位目标（50/45/30fps）自动降档（像素比 2→1.5→1、关闭抗锯齿、粒子 55%→25%）；连续 3 个窗口富余则回升（不超过初始档）。
3. **离屏暂停**：IntersectionObserver 驱动 rAF 启停；`document.hidden` 时由浏览器节流兜底。
4. **低端设备**：`off` 档渲染 2D 静态视觉（内容完整可读）。
5. **上下文丢失**：`webglcontextlost` 捕获 → 上报失败 → 切换 2D 降级。

## 4. 3D 资源文件说明（交付清单）

本项目**不依赖外部二进制 3D 资产**（模型全部程序化），故无 GLB/贴图文件；程序化模型的“源文件”即 `src/components/three/*.tsx` 中的几何构建代码。若后续需替换为外部模型：
1. 导出 glTF/GLB（推荐 Draco 压缩：`gltf-pipeline -d`，或 Meshopt：`gltfpack -cc`）；
2. 放入 `public/models/`，使用 `three/addons/loaders/GLTFLoader.js` + `DRACOLoader`（`three/addons/loaders/DRACOLoader.js`，decoder 放 `public/draco/`）异步加载；
3. 沿用 `useSceneCanvas` 生命周期与 `fallbacks` 降级路径。
