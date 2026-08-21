const fs = require('fs');
const p = 'AutomationPanel.svelte';
const lines = fs.readFileSync(p, 'utf8').split(/\r?\n/);
const isOpen = (l) => l.trim() === '<div class="ap-view flex min-h-0 flex-1 flex-col gap-3">';
const isElse = (l) => /^\s*\{:else if view === /.test(l);
// 1) 找出所有 ap-view open 与紧邻其后的 </div>（错位 close）
const removeIdx = [];
const insertBefore = [];
for (let i = 0; i < lines.length; i++) {
  if (isOpen(lines[i])) {
    // 若下一行是 </div>，则这是“空 wrapper”的错位 close → 删除该 close 并记住要在对应的 else-if 前补 close
    if (i + 1 < lines.length && lines[i + 1].trim() === '</div>') {
      removeIdx.push(i + 1);
    }
  }
  if (isElse(lines[i])) {
    insertBefore.push(i);
  }
}
// 2) 从后往前删除错位 close，并在每个 else-if 前插入 </div>
for (const idx of removeIdx.reverse()) lines.splice(idx, 1);
// 重新计算 else-if 行号（删除后已变化）
const elseIdx = [];
for (let i = 0; i < lines.length; i++) if (isElse(lines[i])) elseIdx.push(i);
// 在每个 else-if 前插入 close（从后往前）
for (const idx of elseIdx.reverse()) lines.splice(idx, 0, '  </div>');
// 3) 校验：ap-view open 数 == </div> 数（仅统计 ap-view 容器层级）
const opens = lines.filter(isOpen).length;
// 校验每个视图块内是否都包了 wrapper：ap-view 出现次数应为 5，且总 <div> 平衡由 svelte-check 把关
fs.writeFileSync(p, lines.join('\n'), 'utf8');
console.log('done. ap-view opens:', opens, ' else-if count:', elseIdx.length);
