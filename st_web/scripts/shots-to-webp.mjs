// 将全部截图转为 WebP（压缩）供官网使用
import sharp from "sharp";
import { readdirSync } from "node:fs";

const files = readdirSync("public/screenshots").filter((f) => f.endsWith(".png"));

for (const f of files) {
  const name = f.replace(/\.png$/, "");
  await sharp(`public/screenshots/${f}`)
    .resize({ width: 1600 })
    .webp({ quality: 76 })
    .toFile(`public/screenshots/${name}.webp`);
  const info = await sharp(`public/screenshots/${name}.webp`).metadata();
  console.log(`${name}.webp ${Math.round(info.size / 1024)}KB`);
}
