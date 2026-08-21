# 动画设计规范

## 原则

1. **性能优先**：所有循环动画统一走 `requestAnimationFrame`（3D 由 `useSceneCanvas` 单循环管理；DOM 动画用 CSS transform/opacity，避免布局属性）。
2. **可降级**：`prefers-reduced-motion: reduce` 时关闭全部循环与滚动动画（globals.css 全局规则 + 引擎单帧渲染）。
3. **可逆**：滚动驱动动画用归一化进度插值，上滚即回放。

## 动画清单

| 动画 | 实现 | 参数 |
|---|---|---|
| 平滑滚动 | Lenis（`SmoothScrollProvider`） | duration 1.1s，ease `1.001 - 2^-10t`；reduced-motion 回退原生 |
| 滚动进场 | `Reveal`（IntersectionObserver + `.reveal→.in`） | 位移 26px，0.7s cubic-bezier(0.16,1,0.3,1)，支持级联 delay |
| 首屏 3D | 粒子漂移/网格/波浪 + 镜头滚动叙事 + 鼠标视差 | 见 `docs/3d-visual.md` |
| 指标跑马灯 | `--animate-marquee`（translateX -50% 循环） | 30s linear |
| 发光/呼吸 | `--animate-pulse-slow`、`--animate-float`、`--animate-spin-slow` | 3.5–14s |
| 按钮反馈 | hover 亮度/位移 1px，`transition` 0.1–0.3s | |
| 卡片悬浮 | `.card-hover`：上浮 4px + 发光描边 | 0.3s cubic-bezier(0.16,1,0.3,1) |
| 手风琴展开 | `grid-template-rows 0fr→1fr` + opacity | 0.3s ease-out |
| 治理/弹窗入场 | `@keyframes hns-drawer-in` 类：位移+透明度 | 0.18s ease-out |
| 输入框聚焦 | 边框色过渡 + 光晕（`--glow`） | 0.15s |
| 3D 产品交互 | OrbitControls damping 0.08；爆炸/剖面/配色即时响应 | |

## 时间与缓动约定

- 标准缓动 `cubic-bezier(0.16, 1, 0.3, 1)`（快进慢停）。
- 微交互 ≤ 0.3s；区块进场 ≤ 0.8s；滚动叙事与滚动位置连续绑定。
- 级联延迟 60–120ms/项，避免同时段动画密度过高。

## 减动效降级矩阵

| 能力 | reduced-motion 行为 |
|---|---|
| Lenis 平滑滚动 | 关闭，原生滚动 |
| Reveal 进场 | 直接可见（无位移过渡） |
| marquee/pulse/float/spin | 全部 `animation: none` |
| 3D 背景/产品/图表 | 渲染单帧静态画面（`useSceneCanvas` reduced 分支） |
| 爆炸/剖面/配色 | 保留（用户显式触发的即时响应，无循环动画） |
