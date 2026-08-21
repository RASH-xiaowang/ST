// ============================================================
// 通用颜色工具 — 运行期冒烟测试
// 锁定 colorUtils 下沉后的可观测输出：
//   hex→rgba / 亮度计算 / 深浅文字色选择
// 运行：node st_control/.codex_tests/smoke-color-utils.mjs
// ============================================================
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import esbuild from 'esbuild';

// mock canvas：cssColorToHex 依赖 DOM canvas 采样
globalThis.document = {
  createElement: () => ({
    width: 0,
    height: 0,
    getContext: () => ({
      fillStyle: '',
      fillRect: () => {},
      getImageData: () => ({ data: [255, 128, 0, 255] }),
    }),
  }),
};

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..');
const outDir = path.join(here, 'out');
mkdirSync(outDir, { recursive: true });

const src = readFileSync(path.join(root, 'src', 'lib', 'components', 'colorUtils.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'color-utils.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { hexToRgba, hexLum, swatchTextColor, swatchSubColor, cssColorToHex } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// hex → rgba
ok(hexToRgba('#ff8000', 0.5) === 'rgba(255,128,0,0.5)', 'hex 转 rgba（带 #）');
ok(hexToRgba('00ff00', 1) === 'rgba(0,255,0,1)', 'hex 转 rgba（无 #）');

// 亮度（Rec.709 加权）
ok(Math.abs(hexLum('#000000') - 0) < 1e-9, '黑色亮度 0');
ok(Math.abs(hexLum('#ffffff') - 1) < 1e-9, '白色亮度 1');
ok(hexLum('#ff8000') > hexLum('#0000ff'), '橙色比蓝色亮');

// 深浅文字色
ok(swatchTextColor('#111111') === 'rgba(235,238,244,0.95)', '深色底 → 浅色文字');
ok(swatchTextColor('#ffffff') === 'rgba(24,28,34,0.88)', '浅色底 → 深色文字');
ok(swatchSubColor('#111111') === 'rgba(235,238,244,0.72)', '深色底次要文字');
ok(swatchSubColor('#ffffff') === 'rgba(24,28,34,0.62)', '浅色底次要文字');

// CSS 颜色 → hex（canvas mock 返回 rgb(255,128,0)）
ok(cssColorToHex('rgb(255, 128, 0)') === '#ff8000', 'cssColorToHex 输出 #rrggbb');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
