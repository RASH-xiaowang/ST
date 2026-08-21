// 社交关系图谱 — 高清分享海报渲染
//
// 目标：导出「可直接发朋友圈」的高质量海报图。
// - 画幅：1:1（朋友圈方图）/ 3:4（竖版长图）
// - 排版：顶部标题 → 数据统计卡 → 深色图谱卡片 → 图例 → 生成时间
// - 图谱只展示节点（含头像）与连线，不显示网名/标签

export type PosterRatio = "1:1" | "3:4";

export interface PosterLayout {
  width: number;
  height: number;
  /** 图谱图层在画布上的位置与尺寸（逻辑像素） */
  graphX: number;
  graphY: number;
  graphW: number;
  graphH: number;
}

export function getPosterLayout(ratio: PosterRatio): PosterLayout {
  if (ratio === "3:4") {
    return { width: 1080, height: 1440, graphX: 68, graphY: 388, graphW: 944, graphH: 760 };
  }
  return { width: 1080, height: 1080, graphX: 68, graphY: 388, graphW: 944, graphH: 568 };
}

export interface PosterStatItem {
  label: string;
  value: string;
}

export interface PosterInput {
  /** 已渲染好的图谱图层（深色底，未污染画布） */
  graphLayer: HTMLCanvasElement;
  ratio: PosterRatio;
  tag: string;
  title: string;
  subtitle: string;
  stats: PosterStatItem[];
  legend: string;
  footer: string;
  scale?: number;
}

/** 海报文案配置（标签/统计/图例；自 RelationshipGraph.doExport 下沉，T-293） */
export function makePosterInput(opts: {
  ratio: PosterRatio;
  isPeople: boolean;
  dateStr: string;
  timeStr: string;
  contactBookFriends: number;
  personCount: number;
  groupCount: number;
  edgesCount: number;
  communityCount: number;
  totalGroups: number;
}): Omit<PosterInput, "graphLayer" | "scale"> {
  const { ratio, isPeople, dateStr, timeStr } = opts;
  const stats: PosterStatItem[] = isPeople
    ? [
        { label: "好友", value: fmtCount(opts.contactBookFriends) },
        { label: "展示节点", value: fmtCount(opts.personCount) },
        { label: "连线", value: fmtCount(opts.edgesCount) },
        { label: "圈子", value: fmtCount(opts.communityCount) },
      ]
    : [
        { label: "群聊", value: fmtCount(opts.totalGroups || opts.groupCount) },
        { label: "展示节点", value: fmtCount(opts.groupCount) },
        { label: "连线", value: fmtCount(opts.edgesCount) },
        { label: "圈子", value: fmtCount(opts.communityCount) },
      ];
  return {
    ratio,
    tag: "WECHAT SOCIAL GRAPH",
    title: "我的微信社交图谱",
    subtitle: `${isPeople ? "群友圈子" : "群聊网络"} · 数据来自本地微信记录 · ${dateStr}`,
    stats,
    legend: isPeople
      ? "● 颜色 = 圈子　— 连线 = 共同群数　◍ 大小 = 消息量"
      : "● 颜色 = 圈子　— 连线 = 共同成员数　◍ 大小 = 消息量",
    footer: `由 ST 控制台生成 · ${timeStr}`,
  };
}

const FONT = `-apple-system, "PingFang SC", "Microsoft YaHei", "Segoe UI", sans-serif`;

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
}

/** 大数格式化（万/亿） */
export function fmtCount(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0";
  if (n >= 1e8) return `${(n / 1e8).toFixed(1).replace(/\.0$/, "")}亿`;
  if (n >= 1e4) return `${(n / 1e4).toFixed(1).replace(/\.0$/, "")}万`;
  return String(Math.round(n));
}

/** 渲染海报（返回高分辨率 canvas，未污染，可直接 toBlob） */
export function buildPoster(input: PosterInput): HTMLCanvasElement {
  const layout = getPosterLayout(input.ratio);
  // 尽可能大：默认 7.5 倍（约 8100px），上限 8192px（8K）
  const scale = Math.max(
    1,
    Math.min(input.scale ?? 7.5, 8192 / Math.max(layout.width, layout.height)),
  );
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(layout.width * scale));
  canvas.height = Math.max(1, Math.round(layout.height * scale));
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("无法创建海报画布");
  ctx.setTransform(scale, 0, 0, scale, 0, 0);

  const W = layout.width;
  const H = layout.height;
  const pad = 56;

  // ── 背景：浅色渐变 + 点阵装饰 + 右上光斑 ──
  const bg = ctx.createLinearGradient(0, 0, 0, H);
  bg.addColorStop(0, "#f5f7fd");
  bg.addColorStop(1, "#e9edf8");
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, W, H);
  ctx.fillStyle = "rgba(61,107,242,0.045)";
  for (let y = 26; y < H; y += 38) {
    for (let x = 26; x < W; x += 38) {
      ctx.beginPath();
      ctx.arc(x, y, 1.4, 0, Math.PI * 2);
      ctx.fill();
    }
  }
  const glow = ctx.createRadialGradient(W - 110, 80, 10, W - 110, 80, 280);
  glow.addColorStop(0, "rgba(7,193,96,0.10)");
  glow.addColorStop(1, "rgba(7,193,96,0)");
  ctx.fillStyle = glow;
  ctx.fillRect(W - 430, 0, 430, 430);

  // ── 顶部标签 / 标题 / 副标题 ──
  ctx.textAlign = "left";
  ctx.textBaseline = "alphabetic";
  ctx.fillStyle = "#6b7594";
  ctx.font = `600 20px ${FONT}`;
  ctx.fillText(input.tag, pad, 68);
  ctx.fillStyle = "#1b2233";
  ctx.font = `700 54px ${FONT}`;
  ctx.fillText(input.title, pad, 128);
  ctx.fillStyle = "#7a84a3";
  ctx.font = `400 24px ${FONT}`;
  ctx.fillText(input.subtitle, pad, 170);

  // ── 数据统计卡 ──
  const statsY = 218;
  const statsH = 130;
  const gap = 20;
  const statsCount = input.stats.length;
  const cardW = (W - pad * 2 - gap * (statsCount - 1)) / statsCount;
  input.stats.forEach((s, i) => {
    const x = pad + i * (cardW + gap);
    roundRect(ctx, x, statsY, cardW, statsH, 24);
    ctx.fillStyle = "rgba(255,255,255,0.94)";
    ctx.fill();
    ctx.strokeStyle = "rgba(27,34,51,0.07)";
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.textAlign = "center";
    ctx.fillStyle = "#8a94b8";
    ctx.font = `500 20px ${FONT}`;
    ctx.fillText(s.label, x + cardW / 2, statsY + 44);
    ctx.fillStyle = "#1b2233";
    ctx.font = `700 42px ${FONT}`;
    ctx.fillText(s.value, x + cardW / 2, statsY + 104);
  });

  // ── 图谱卡片（白卡 + 内嵌深色图谱）──
  const cardX = pad;
  const cardY = layout.graphY - 12;
  const cardWide = W - pad * 2;
  const cardHigh = layout.graphH + 24;
  roundRect(ctx, cardX, cardY, cardWide, cardHigh, 32);
  ctx.fillStyle = "#ffffff";
  ctx.fill();
  ctx.save();
  roundRect(ctx, layout.graphX, layout.graphY, layout.graphW, layout.graphH, 22);
  ctx.clip();
  ctx.drawImage(input.graphLayer, layout.graphX, layout.graphY, layout.graphW, layout.graphH);
  ctx.restore();
  roundRect(ctx, cardX, cardY, cardWide, cardHigh, 32);
  ctx.strokeStyle = "rgba(27,34,51,0.09)";
  ctx.lineWidth = 1.5;
  ctx.stroke();

  // ── 图例与生成信息 ──
  ctx.textAlign = "center";
  const legendY = input.ratio === "3:4" ? H - 102 : 1004;
  ctx.fillStyle = "#8a94b8";
  ctx.font = `400 21px ${FONT}`;
  ctx.fillText(input.legend, W / 2, legendY);
  ctx.fillStyle = "#a6aec9";
  ctx.font = `400 19px ${FONT}`;
  ctx.fillText(input.footer, W / 2, legendY + 44);

  return canvas;
}

export function posterToBlob(
  canvas: HTMLCanvasElement,
  format: "png" | "jpeg",
): Promise<Blob> {
  return new Promise((res, rej) => {
    canvas.toBlob(
      (b) => (b ? res(b) : rej(new Error("海报生成失败"))),
      format === "jpeg" ? "image/jpeg" : "image/png",
      format === "jpeg" ? 0.95 : undefined,
    );
  });
}

export function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((res, rej) => {
    const fr = new FileReader();
    fr.onload = () => res(String(fr.result));
    fr.onerror = () => rej(fr.error);
    fr.readAsDataURL(blob);
  });
}
