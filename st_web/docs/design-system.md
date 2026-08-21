# 设计系统文档

## 1. 设计理念

「前沿科技 · 未来感 · 高端产品气质」。三层空间结构：**前景内容 → 中景 UI（毛玻璃卡片）→ 背景 3D/粒子场景**。所有视觉元素统一由语义令牌驱动，明暗双模式共用同一套组件。

## 2. 色彩体系

语义变量定义于 `src/app/globals.css`（`--accent` 三色渐变：青 `#22d3ee` → 紫 `#8b5cf6` → 品红 `#ec4899`；金色 `#f5c33b` 用于热点/警示强调）。

| 令牌 | 用途 |
|---|---|
| `--bg` / `--bg-2` | 页面背景（暗 `#04060d` / 亮 `#f5f7fc`） |
| `--surface` / `--surface-2` / `--surface-3` | 毛玻璃卡片与悬浮面 |
| `--text` / `--muted` / `--faint` | 三级文字层级 |
| `--border` / `--border-2` | 描边层级 |
| `--accent/-2/-3` | 主强调三色（渐变端点） |
| `--ok/--warn/--err` | 语义状态色 |
| `--glow/-2` | 霓虹光晕 |

Tailwind 侧通过 `@theme inline` 映射为 `bg-bg`、`text-text`、`border-border`、`text-accent` 等工具类；`text-gradient` / `text-metal` / `glass` / `glow-ring` / `grid-overlay` / `noise` / `glow-hr` / `outline-text` 为站点级工具类。

## 3. 排版

- 显示字体：**Space Grotesk Variable**（可变字体，`font-display`）——标题、数字、品牌。
- 等宽字体：**JetBrains Mono Variable**（`font-mono`）——代码、指标、标签。
- 中文字形回退 PingFang SC / Microsoft YaHei。
- 层级：`text-7xl→4xl`（首屏主张）/ `text-4xl→3xl`（区块标题）/ `text-xl→lg`（卡片标题）/ `text-sm`（正文）/ `text-[10px] mono`（眉题、标签）。行高正文 1.7–1.9。

## 4. 间距与栅格

- 内容容器 `max-w-7xl`，区块纵向 `py-24 lg:py-32`；区块内元素间距 8 的倍数（8/12/16/20/24/32）。
- 断点：`sm 640 / md 768 / lg 1024 / xl 1280 / 2xl 1536`；超宽屏内容居中，3D 画布全幅。
- 圆角体系：卡片 12–16px、按钮 8–12px、胶囊 999px；阴影统一“发光投影”（`--glow` 参与）。

## 5. 组件规范（关键组件）

| 组件 | 规则 |
|---|---|
| Nav | 透明 → 滚动后毛玻璃（`--nav-bg` + backdrop-blur）；移动端抽屉；焦点态 2px accent 描边 |
| SectionHeading | 序号（mono）+ 眉题 + 主标题 + 副标题，居中/左对齐两态 |
| 卡片 | `glass` + `card-hover`（上浮 4px + 发光描边） |
| 按钮 | 主 CTA 三色渐变 + 发光阴影；次按钮描边 hover 变 accent |
| 表格 | `glass` 容器 + 细分割线 + 行 hover 底色 |
| 弹窗（案例/搜索） | 遮罩 `bg-black/60 + blur`，Esc 可关，焦点管理 |
| Accordion | 单开 + 网格行高过渡动画 |

## 6. 可访问性

- 语义化：`header/nav/main/section/article/footer`，单一 `h1`。
- 键盘：全部交互元素可 Tab，`:focus-visible` 统一描边；搜索框方向键/回车导航。
- ARIA：3D 画布以 `role="img"` + `aria-label` 描述；弹窗 `role="dialog" aria-modal`；手风琴 `aria-expanded/aria-controls`。
- 对比度：正文 `--text` 对 `--bg` ≥ 7:1，`--muted` ≥ 4.5:1（WCAG AA）。
- 减动效：`prefers-reduced-motion` 下关闭全部循环动画，3D 渲染单帧静态画面，滚动进场直接可见。
