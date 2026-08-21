/**
 * 生成 OG 社交分享图（1200×630 PNG）与桌面图标占位。
 * 使用 sharp 将内联 SVG 渲染为 PNG；在 build 前执行：npm run assets
 */
import { mkdirSync } from "node:fs";
import path from "node:path";
import sharp from "sharp";

const OUT = path.resolve("public");

const svg = (t, sub) => `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#04060d"/>
      <stop offset="0.6" stop-color="#0a1226"/>
      <stop offset="1" stop-color="#140a2e"/>
    </linearGradient>
    <linearGradient id="acc" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="#22d3ee"/>
      <stop offset="0.5" stop-color="#8b5cf6"/>
      <stop offset="1" stop-color="#ec4899"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.5" cy="0.35" r="0.6">
      <stop offset="0" stop-color="#22d3ee" stop-opacity="0.35"/>
      <stop offset="1" stop-color="#22d3ee" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="1200" height="630" fill="url(#bg)"/>
  <rect width="1200" height="630" fill="url(#glow)"/>
  <g stroke="#22d3ee" stroke-width="1" opacity="0.14">
    ${Array.from({ length: 11 }, (_, i) => `<line x1="${i * 120}" y1="0" x2="${i * 120}" y2="630"/>`).join("")}
    ${Array.from({ length: 6 }, (_, i) => `<line x1="0" y1="${i * 105}" x2="1200" y2="${i * 105}"/>`).join("")}
  </g>
  <g transform="translate(600 250)">
    <ellipse cx="0" cy="90" rx="210" ry="40" fill="none" stroke="url(#acc)" stroke-width="2" opacity="0.7" transform="rotate(-24)"/>
    <path d="M0 -78 L67 -39 L67 39 L0 78 L-67 39 L-67 -39 Z" fill="rgba(34,211,238,0.12)" stroke="url(#acc)" stroke-width="2.5"/>
    <circle r="26" fill="url(#acc)"/>
    <circle r="52" fill="none" stroke="url(#acc)" stroke-width="1.2" opacity="0.5"/>
  </g>
  <text x="600" y="470" text-anchor="middle" font-family="Segoe UI, Arial, sans-serif" font-size="72" font-weight="800" fill="#e8effd" letter-spacing="10">${t}</text>
  <text x="600" y="525" text-anchor="middle" font-family="Segoe UI, Arial, sans-serif" font-size="28" fill="#8b97ad">${sub}</text>
</svg>`;

const zh = svg("ST CONTROL", "把智能装进每一台机器 · 一体化 AI 智能控制台");
const en = svg("ST CONTROL", "Intelligence, deployed everywhere · AI Control Console");

mkdirSync(OUT, { recursive: true });
await sharp(Buffer.from(zh)).png({ compressionLevel: 9 }).toFile(path.join(OUT, "og.png"));
await sharp(Buffer.from(en)).png({ compressionLevel: 9 }).toFile(path.join(OUT, "og-en.png"));
console.log("✓ og.png / og-en.png generated");
