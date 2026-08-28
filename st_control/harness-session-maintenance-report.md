# Harness 会话功能维护报告

**日期**: 2026-08-17  
**范围**: `src/lib/harness/` 前端 + `src-tauri/src/harness/` 后端会话相关模块

---

## 一、发现的问题

| # | 严重度 | 类型 | 位置 | 描述 |
|---|--------|------|------|------|
| 1 | **高** | BUG | `instructions.rs:22-24` | `INJECT_BUDGET_CHARS`（24KB）< `FILE_CAP_CHARS`（32KB），单文件截断后仍超预算，`file_cap_limits_single_file` 测试恒失败 |
| 2 | **中** | BUG | `HarnessTab.svelte:1873` | `runCli()` 内 `const input` 遮蔽组件级 `$state` 变量 `input`，后续维护者引用歧义 |
| 3 | **中** | BUG | `HarnessTab.svelte:2770-2785` | `initTab()` 恢复设置时未读取 `voice_name` / `voice_speed`，首次加载后 TTS 音色/语速丢失（需打开治理中心才恢复） |
| 4 | **中** | BUG | `session.rs:1239` | `derive_display_messages` 中 `WorkflowRun` 的 `stage` 字段以 0-based 展示（"阶段 0/3"），`trajectory()` 已修正为 1-based（"阶段 1/3"），两处不一致 |
| 5 | **低** | 类型 | `types.ts:380` | `FeedbackRecord.comment` 字段 JSDoc 与 `message_seq` 注释合并为一行，文档生成器误归属 |

---

## 二、已修复内容

### 修复 1：指令注入预算与文件容量不匹配
- **文件**: `src-tauri/src/harness/instructions.rs`
- **变更**: `INJECT_BUDGET_CHARS` 从 24KB 增至 36KB（≥ `FILE_CAP_CHARS` + 包装开销），确保单文件截断后能完整注入
- **附带**: 更新 `inject_budget_truncates_and_stops` 测试用两个半预算文件验证多文件截断逻辑

### 修复 2：`runCli()` 变量遮蔽
- **文件**: `src/lib/harness/HarnessTab.svelte`
- **变更**: 局部变量 `input` 重命名为 `cliCmd`，消除与组件 `$state` 变量的遮蔽

### 修复 3：`initTab()` 语音设置恢复
- **文件**: `src/lib/harness/HarnessTab.svelte`
- **变更**: `initTab()` 中 `currentSettings` 构造增加 `voice_name` / `voice_speed` 字段，并同步恢复 `voiceName` / `voiceSpeed` 局部状态

### 修复 4：WorkflowRun 阶段序号展示一致性
- **文件**: `src-tauri/src/harness/session.rs`
- **变更**: `derive_display_messages` 中 `WorkflowRun` 的 format 字符串从 `stage` 改为 `stage + 1`，与 `trajectory()` 保持一致（用户可读 1-based 序号）

### 修复 5：FeedbackRecord JSDoc 格式
- **文件**: `src/lib/harness/types.ts`
- **变更**: `comment` 属性与 `message_seq` 的 JSDoc 注释之间增加换行，确保文档正确归属

---

## 三、验证命令与结果

| 命令 | 结果 |
|------|------|
| `npx svelte-check --output human` | ✅ 0 errors, 0 warnings |
| `cargo fmt --check` | ✅ 无格式问题 |
| `cargo clippy --lib --no-default-features` | ✅ 0 warnings |
| `cargo test --lib --no-default-features` | ✅ 457 passed, 0 failed, 21 ignored |
| `node .codex_tests/smoke-ipc-contract.mjs` | ✅ 442 命令, 174 调用, 全部一致 |
| `node .codex_tests/smoke-harness-session.mjs`（新增） | ✅ 50 项检查全部通过 |

---

## 四、新增测试

### `.codex_tests/smoke-harness-session.mjs`
覆盖 Harness 会话功能的类型契约与投影逻辑：
- `DisplayMessage` 三态 role（user/assistant/meta）及 seq 字段存在性
- `FeedbackRecord` 字段完整性（comment/message_seq 分行 JSDoc）
- `HarnessStreamEvent.done` 含 seq/model/cost
- `HarnessEvent` 后端枚举全部 19 个变体存在性
- 前端 `HarnessEvent` 旧日志判别 9 个关键 type 存在性
- `WorkflowRun` stage 在 display_messages 和 trajectory 中均使用 1-based
- `ipc.ts` 含 14 个核心会话 IPC 函数

---

## 五、未处理项与后续建议

| # | 严重度 | 描述 | 建议 |
|---|--------|------|------|
| 1 | 低 | `ToolCard.svelte` 的 `isRead` 仅匹配 `read_file`，`read_image` 回退到通用卡片 | 可增加 `read_image` 为图片预览卡（与已有 `PREVIEW_IMAGE_EXTS` 复用） |
| 2 | 低 | 侧栏搜索结果 (`searchHits`) 只能点击跳转，无独立关闭按钮 | 可在搜索结果区域增加「✕ 清除」按钮 |
| 3 | 低 | `instructions.rs::inject()` 截断使用 `chars().take(budget)`（字符计），但预算用 `len()`（字节计），CJK 内容可能导致截断后仍超字节预算 | 建议统一为字节截断：`&section[..budget.min(section.len())]`，注意 UTF-8 边界 |
| 4 | 信息 | `openDrawer()` 与 `initTab()` 各自独立调用 `harnessApi.getSettings()`，两处设置恢复逻辑可合并为共享函数 | 低优先级重构 |
| 5 | 信息 | `HarnessTab.svelte` 超过 6000 行，治理中心抽屉各标签页渲染逻辑集中在一个文件 | 长期可考虑拆分为 `DrawerTabs/` 子组件 |

---

## 六、变更文件清单

| 文件 | 变更类型 |
|------|----------|
| `src-tauri/src/harness/instructions.rs` | 修复预算常量 + 更新测试 |
| `src-tauri/src/harness/session.rs` | 修复 WorkflowRun stage 展示 |
| `src/lib/harness/types.ts` | 修复 JSDoc 格式 |
| `src/lib/harness/HarnessTab.svelte` | 修复变量遮蔽 + 语音设置恢复 |
| `.codex_tests/smoke-harness-session.mjs` | 新增会话契约烟测 |
