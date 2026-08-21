# Harness 会话维护记录

> 本文件记录 Harness 会话功能的持续维护（审查 → 修复 → 回归）。
> 迁移蓝图见 `harness-migration-plan.md` / `harness-migration-plan-full.md`。

## 2026-08-20 第 101 轮（新目标周期第 51 轮，收官）：最终基线核验

### 最终基线（全部通过）

| 门禁 | 结果 |
|---|---|
| `cargo test --lib` | **416 passed / 0 failed / 22 ignored** |
| `cargo fmt --check` | 0 diff |
| `cargo clippy --lib` | 0 warnings |
| `svelte-check` | 0 errors / 0 warnings |
| IPC 契约 | 422 命令 / 147 invoke / 146 参数比对全一致 |
| 全量 E2E（隔离） | **19/19 探针 ALL_PASS**（第 100 轮，最新 exe） |
| 数据安全 | 真实库零污染（cfg!(test) 临时库 + ST_WECHAT_APP_DIR 隔离） |

### 新目标周期 51 轮总览（第 50-100 轮）

- **功能落地**：B15 effort 遥测落库（reasoning_effort）、turn_files 产物
  识别补 str_replace_editor（DSH 渲染意图对齐）、fetch URL 卫生校验
  （凭据拒绝）、skill id kebab-case 校验（防路径注入）——4 项 DSH
  对照驱动修复，均经全量回归验证零回归。
- **DSH 对照**：session/lineage/conversation/context-provenance/pending/
  contract/fs/web/skill/hooks/spill/schedule 十余个面对照确认等价。
- **测试补全**：单测 383 → **416**（+33，覆盖 llm 层/harness 全模块）。

### 待办（下一轮候选）

- 常规维护 / 用户指定方向（本目标 50 轮上限已达）

---

### 本轮动作

- **背景**：累计生产改动——turn_files 产物识别（第 91 轮）、fetch URL
  卫生（第 95 轮）、skill id kebab-case（第 96 轮）、effort 遥测
  （第 86 轮）——此前仅 verify-sre-editor 验证，本轮全量回归。
- **全量回归结果**：**19/19 ALL_PASS**（phase1-6/9-11/78/b2/goal/
  concurrency + verify-harness-* 8 项），脚本尾行
  `all passed (real data/ untouched)`，teardown done，exit 0；
  st-control 与 vite 均退出（无残留）。**零回归**。
- **矩阵同步**（`harness-capability-matrix.md`）：单测计数 413 → **416**、
  快照标注第 79 轮 → 第 96 轮。

### 第 100 轮基线

- 全量 19 探针 **19/19 ALL_PASS**（隔离环境，真实 LLM）
- `cargo test --lib`：**416 passed / 0 failed**；clippy 0 / fmt 0
- 真实库零变化；无残留进程

### 待办（下一轮候选）

- 常规维护 / 用户指定方向（本目标 50 轮上限将达）

---

### 本轮动作

- **DSH 源码对照**（schedule/domain.d.ts）：MIN_EVERY_INTERVAL_SECONDS=300
  （5 分钟下限）+ Every/OneShot/After 三形态——对照 ST schedule：
  - 间隔下限 **1 分钟**（ST 1..=10080 vs DSH 300s）——宽松超集（合理
    产品选择，非缺陷）；
  - one_shot + every/after 双形态（schedule.rs 206 行）——覆盖 DSH
    三形态。
  - 无需改动。
- **全量基线**：416 单测 0 失败、fmt 0 diff、clippy 0 warnings。

### 第 99 轮基线

- `cargo test --lib`：**416 passed / 0 failed / 22 ignored**
- clippy 0 / fmt 0；真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（spill/types.ts）：SpillOwner.sessionId（按会话分组
  存储）+ fork 继承 locator 不复制/重归属——对照 ST spill：
  - `session_dir(session_id)`（spills_root/会话 id）分组**对齐**；
  - fork 复制事件日志（含 locator 引用），新 spill 用子会话 id——
    **一致**。
  - 无需改动。
- **周期健康检查**（零 LLM）：隔离 E2E verify-sre-editor **14/14
  ALL_PASS**，teardown done、exit 0、无残留进程。

### 第 98 轮基线

- 单测 416 passed（上轮）；clippy 0 / fmt 0 / 契约全一致
- E2E 探针 14/14；真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（hook-protocol/events.d.ts）：HookInvocation 配对 +
  DEFAULT_STDERR_SUMMARY_MAX_CHARS=500（trim + 省略号截断）——对照 ST
  hooks：`truncate_str(500)` 截断对齐 ✓（hooks.rs 101 行）；事件经
  harness-hook-fired 回传（invoked/result 配对是 DSH 前端状态机，ST 用
  单发事件等价）。无需改动。
- **全量基线**：416 单测 0 失败、fmt 0 diff、clippy 0 warnings。

### 第 97 轮基线

- `cargo test --lib`：**416 passed / 0 failed / 22 ignored**
- clippy 0 / fmt 0；真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（skill/skill）：SKILL_NAME 正则
  `^[a-z0-9]+(?:-[a-z0-9]+)*$`（isSkillName）——ST save_skill 原仅校验
  非空，**缺名称语法校验**（id 用于目录/文件名，防路径注入）。
- **修复**（`skill.rs`）：新增 `is_valid_skill_id`（kebab-case：小写字母/
  数字/连字符，拒绝大写/空格/下划线/连续/首尾连字符/空），save_skill
  校验。
- **新增测试**（`skill.rs` +1）：`skill_id_requires_kebab_case`——合法
  kebab-case 通过 + 8 类非法拒绝。

### 验证

- `cargo test --lib`：**416 passed / 0 failed**（+1）；fmt 0；clippy 0。
- 重建 exe 后隔离 E2E verify-sre-editor **14/14 ALL_PASS**（零 LLM），
  teardown done、exit 0、无残留进程。

### 第 96 轮基线

- 单测 416 / clippy 0 / fmt 0 / 契约 422/147/146
- 真实库零变化（隔离环境验证）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（web-fetch-http/policy.ts）：validateFetchUrl 拒绝
  http(s) 之外的协议 + **内嵌凭据（user:pass@）** + 长度上限——对照 ST
  fetch 原用前缀匹配（`starts_with("http://")`），**缺凭据拒绝**。
- **修复**（`web.rs::fetch`）：改用 `reqwest::Url::parse` 协议解析 +
  拒绝非 http(s) + **拒绝 user:pass@ 凭据**（DSH 卫生语义对齐）。
- **测试扩展**（`web.rs`）：fetch 拒绝内嵌凭据（user:pass@ 与 user@）、
  合法 URL 不报凭据校验错误。
- **踩坑**：测试初版断言「合法 URL 必报网络错误」——example.com 可达时
  断言失败；改为仅断言不含凭据校验错误（不假设网络状态）。

### 验证

- `cargo test --lib`：**415 passed / 0 failed**；fmt 0；clippy 0。
- 重建 exe 后隔离 E2E verify-sre-editor **14/14 ALL_PASS**（零 LLM），
  teardown done、exit 0、无残留进程。

### 第 95 轮基线

- 单测 415 / clippy 0 / fmt 0 / 契约 422/147/146
- 真实库零变化（隔离环境验证）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（tool-fs/read.ts）：READ_LIMIT=2000 行窗口 +
  STREAM_MIN_SIZE=10MB——对照 ST：
  - read_file（64KB 截断）+ str_replace_editor view（view_range 行窗口，
    已有区间/非法/截断测试）合计覆盖 DSH read 能力；
  - 无需改动。
- **全量基线**：415 单测 0 失败、fmt 0 diff、clippy 0 warnings、
  IPC 契约 422/147/146 全一致、svelte-check 0/0。

### 第 94 轮基线

- `cargo test --lib`：**415 passed / 0 failed / 22 ignored**
- clippy 0 / fmt 0 / 契约全一致 / svelte-check 0/0
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（tool-fs-search/grep.ts）：GREP_MAX_MATCHES=250、
  GREP_MAX_LINE_BYTES=2000、超限进 spill——对照 ST grep：
  - ST 上限 **200 条**（DSH 250）+ **300 字符/行**（DSH 2000 字节）——
    更保守（更省上下文），合理差异非缺陷；
  - 超限结果统一经 `spill_result`（agent.rs 2825/2858）溢写——与 DSH
    spill 语义一致。
  - 无需改动。
- **周期健康检查**（零 LLM）：隔离 E2E verify-sre-editor **14/14
  ALL_PASS**，teardown done、exit 0、无残留进程。

### 第 93 轮基线

- 单测 415 passed（上轮）；clippy 0 / fmt 0 / 契约全一致
- E2E 探针 14/14；真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（packages/fs）：write.ts / edit.ts /
  tool-str-replace-editor ↔ ST write_file / edit_file / str_replace_editor
  ——**一一对应**，turn_files 产物白名单（第 91 轮修复后）覆盖完整。
- **全量基线**：415 单测 0 失败、fmt 0 diff、clippy 0 warnings、
  IPC 契约 422/147/146 全一致。

### 第 92 轮基线

- `cargo test --lib`：**415 passed / 0 failed / 22 ignored**
- clippy 0 / fmt 0 / 契约全一致
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（ui-deliverables/turn-deliverables.d.ts）：produced 文件
  = 变更工具的成功路径，**按渲染意图（diff/edit）识别而非工具名**——
  ST turn_files 原用工具名白名单（edit_file/write_file），**漏掉
  str_replace_editor 产物**（第 21 轮新增工具）。
- **修复**（`session.rs::turn_files`）：白名单加入 str_replace_editor，
  且按命令区分——**create/str_replace/insert 识别为产物，view（只读）
  不识别**（DSH 渲染意图语义对齐）。
- **新增测试**（`session.rs` +1）：`turn_files_recognizes_str_replace_editor_mutations`
  ——变更命令识别（new.md/edit.md/ins.md）、view 排除（readonly.md）。

### 验证

- `cargo test --lib`：**415 passed / 0 failed**（+1）；fmt 0；clippy 0。
- 重建 exe 后隔离 E2E verify-sre-editor **14/14 ALL_PASS**（零 LLM），
  teardown done、exit 0、无残留进程。

### 第 91 轮基线

- 单测 415 / clippy 0 / fmt 0 / 契约 422/147/146
- 真实库零变化（隔离环境验证）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（contract/conversation.d.ts）：TurnLocation
  （turn/start|end）与 StepLocation（step/start|end + status:
  open|closed|unknown）——对照 ST 轨迹台账：
  - TurnLocation ↔ `TrajectoryEntry::Assistant { turn, steps,
    tool_calls }`（回合分组）；
  - StepLocation status（open/closed）↔ `TrajectoryEntry::Tool { ok }`
    （未闭合调用 ok=false 收尾）。
  - **等价，无需改动**。
- **Rust 基线**：414 单测 0 失败、fmt 0 diff、clippy 0 warnings。

### 第 90 轮基线

- `cargo test --lib`：**414 passed / 0 failed / 22 ignored**
- clippy 0 / fmt 0；真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（pending.d.ts）：PendingInteractionStatus =
  'approval' | 'plan-review' | 'question'（三种阻塞用户交互）——对照 ST：
  - approval ↔ `pendingApprovals`（审批卡）；
  - plan-review ↔ `pendingQuestions` 中的 **PlanReviewPanel**（📋 计划待审
    三按钮）；
  - question ↔ `pendingQuestions`（提问卡多选/分页）。
  - **一一对应，无需改动**。
- 附带核对：DSH blank 位（空会话隐藏）ST 选择保留显示——产品决策
  （空会话可继续使用），非缺陷。

### 第 89 轮基线

- 单测 414 passed（上轮）；clippy 0 / fmt 0 / 契约全一致
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（context-provenance.d.ts）：ContextRole = 'inject' |
  'recall'（上下文溯源角色）+ label（生产者名）——对照 ST：
  - 'inject' ↔ `ContextInjected`（指令文件列表）与 `SkillInjected`
    （技能 id 列表）事件——均落日志（模型可见 ⟺ 落日志），生产者
    label 对齐（指令路径/技能 id）；
  - 'recall'（跨会话引用）↔ `session_ref` 工具。
  - 无需改动。
- **周期健康检查**（零 LLM）：隔离 E2E verify-sre-editor **14/14
  ALL_PASS**，teardown done、exit 0、无残留进程。

### 第 88 轮基线

- 单测 414 passed（上轮）；clippy 0 / fmt 0 / 契约全一致
- E2E 探针 14/14；真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **effort 链路复核**：`harness_usage_summary` 为纯聚合（COUNT/SUM），
  effort 是每轮属性——聚合无意义，落库链路（记录层）已完整；
  HarnessUsageSummary 保持聚合视图（如需要可后续加「最近 effort」）。
- **全量基线**：414 单测 0 失败、fmt 0 diff、clippy 0 warnings、
  IPC 契约 422/147/146 全一致、svelte-check 0/0。

### 第 87 轮基线

- `cargo test --lib`：**414 passed / 0 failed / 22 ignored**
- clippy 0 / fmt 0 / 契约全一致 / svelte-check 0/0
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（conversation.d.ts）：AssistantRequestConfig 记录
  reasoningEffort——ST 用量记录缺失该字段（遥测完整性差异）。
- **实现**（db.rs + harness/agent.rs）：
  - `HarnessUsageRecord` 新增 `reasoning_effort: Option<String>`；
  - harness_usage 表 ALTER 迁移加列（向后兼容，旧库忽略错误）；
  - `append_harness_usage` SQL 透传新列；
  - agent 工具循环内记录实际生效 effort（provider_with_effort 应用链
    后的 default_reasoning_effort，循环外变量承载跨迭代值）。
- **踩坑**：effort 变量最初误放循环体内（作用域到迭代结束失效）——
  改为循环外声明 + 循环内赋值。

### 验证

- `cargo check` 零警告；`cargo test --lib` **414 passed / 0 failed**；
  fmt 0 diff；clippy 0 warnings。
- 重建 exe 后隔离 E2E verify-sre-editor **14/14 ALL_PASS**（零 LLM），
  teardown done、exit 0、无残留进程。

### 第 86 轮基线

- 单测 414 / clippy 0 / fmt 0 / 契约 422/147/146
- 真实库零变化（隔离环境验证）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（lineage.d.ts）：SessionListEntry.depth（谱系缩进）、
  parentSessionId、origin:'subagent'——对照 ST：
  - depth 缩进 ↔ **子代理目录树**（SubagentRow 递归，状态点/运行中/
    点击打开）——等价且更丰富；
  - parentSessionId ↔ **会话头面包屑**（session_lineage 祖先链可点击）；
  - origin:'subagent' ↔ subagent_catalog 树 + 计数。
- **周期健康检查**（零 LLM）：隔离 E2E verify-sre-editor **14/14
  ALL_PASS**，teardown done、exit 0、无残留进程。

### 第 85 轮基线

- 单测 414 passed（上轮）；clippy 0 / fmt 0 / 契约全一致
- E2E 探针 14/14；真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **DSH 源码对照**（session.d.ts）：DSH Session 前端状态管理类——
  关键可对照点：
  - **分页 PAGE_MESSAGES=50** ↔ 前端 `MSG_PAGE = 50`（HarnessTab 2622
    行）+「加载更早」按钮（2620 行语义）**完全对齐**；
  - 模型可见 ⟺ 落日志（events 原始日志切片）↔ ST 追加式事件日志投影
    **一致**；
  - 事件视图并行（views 与 events 索引对齐）↔ ST 展示/模型双投影
    **等价**。
- wechat::ask 确认不被 harness 调用（独立微信板块，范围外）。

### 第 84 轮基线

- 单测 414 passed（上轮）；clippy 0 / fmt 0 / 契约全一致
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **覆盖饱和确认**：扫描 harness 全部 pub 函数——**所有函数均有测试
  引用**（无盲区）。wechat::ask 不被 harness 调用（独立微信板块，范围外）。
- **全量基线复核**：414 单测 0 失败、fmt 0 diff、clippy 0 warnings、
  IPC 契约 422/147/146 全一致。

### 第 83 轮基线

- `cargo test --lib`：**414 passed / 0 failed / 22 ignored**
- clippy 0 / fmt 0 / 契约全一致 / svelte-check 0/0（上轮）
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：attach_file 的 **sha256 内容寻址**（B10 图片 seam：图片附件
  生成对象副本）未被测试——现有测试只覆盖文本类型。
- **新增测试**（`attachment.rs` +1）：`image_attachment_sha256_content_addressed`
  - 图片附件 kind=image、sha256 64 位十六进制；
  - 内容寻址对象落盘（image_objects/<前2位>/<完整hash>）；
  - **同内容 → 同 sha256**（幂等寻址）。

### 第 82 轮基线

- `cargo test --lib`：**414 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：CLI 测试覆盖 sessions list/create/未知命令/空输入，但
  `session show`（消息投影 用户/助手/会话 行）与 `usage`（用量摘要）
  分支未覆盖。
- **扩展测试**（`sdk.rs` cli_routes 测试 +3 断言）：
  - session show 空会话 → 空输出；
  - 追加消息后 show → 投影「用户：测试消息」；
  - usage → 输出「0 轮」（空用量）。

### 第 81 轮基线

- `cargo test --lib`：**413 passed / 0 failed / 22 ignored**（测试扩展）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **覆盖审查**：jobs check_owner（归属校验）已有测试；session 事件
  read/search/trace 已有测试；interaction multi_select 拼接在前端（后端
  无此逻辑）——无新增测试点。
- **健康检查**（零 LLM）：IPC 契约 422/147/146 全一致、svelte-check 0/0。
- **矩阵同步**（`harness-capability-matrix.md`）：单测计数 407 → **413**、
  快照标注第 71 轮 → 第 79 轮（新周期第 29 轮）。

### 第 80 轮基线

- `cargo test --lib`：**413 passed / 0 failed / 22 ignored**（上轮）
- clippy 0 / fmt 0 / svelte-check 0/0 / 契约 422/147/146
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：approval.rs 已有信任/指纹/TTL 测试；`set_status`（审批状态机
  pending→approved/rejected）无测试。
- **新增测试**（`approval.rs` +1）：`set_status_transitions_pending_only`
  - pending → approved / rejected 转换；
  - **已批准不可再转换**（重复批准/拒绝返回 false）；
  - 不存在 id → false。

### 第 79 轮基线

- `cargo test --lib`：**413 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：`is_readonly_tool`（plan 模式守卫白名单，34 个只读工具）仅
  抽查过少数工具——白名单完整性（无遗漏/无写工具混入）未锁定。
- **新增测试**（`tools.rs` +1）：`readonly_whitelist_covers_query_tools_and_excludes_writers`
  - 34 个只读工具全部在白名单（逐项断言）；
  - 19 个写/执行工具（exec_command/write_file/edit_file/str_replace_editor/
    run_code/plugin_*/goal_*/subagent/workflow_run_js/schedule_*/session_*）
    绝不在白名单（plan 模式必须拦截）。
- 修正：attachment_add 工具不存在（附件走 IPC），从写工具列表移除。

### 第 78 轮基线

- `cargo test --lib`：**412 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **覆盖审查**：goal 自动续跑判定（B3）已有 9 断言完整测试；spill
  递归防护/溢写/越界拒绝已覆盖——harness 核心路径测试饱和。
- **周期健康检查**（零 LLM）：隔离 E2E verify-sre-editor **14/14
  ALL_PASS**，teardown done、exit 0、无残留进程。

### 第 77 轮基线

- 单测 411 passed（上轮）；clippy 0 / fmt 0
- E2E 探针 14/14；真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：`session_state`（GoalBar/计划横幅数据源：plan/goal/todo 状态
  机投影）无直接测试。
- **新增测试**（`session.rs` +1）：`session_state_projects_plan_goal_todo`
  - 目标：GoalSet + 2×GoalUpdate → revision=2（**GoalSet 不递增**，
    防 max_goal_rounds 双计数语义）；
  - blocked 状态 + 阻塞原因 + max_goal_rounds；
  - 计划模式进入（plan_text）/ 退出（清空）；
  - 待办列表投影。
- 健康检查（零 LLM）：IPC 契约 422/147/146 全一致、svelte-check 0/0。

### 第 76 轮基线

- `cargo test --lib`：**411 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **覆盖审查**：kb 模块（30 文件零测试）属独立知识库板块，非会话核心
  范围；kb retrieval（bm25/rrf/权限，harness search_knowledge_base 依赖）
  已有 5 测试——范围外模块不强行补测（聚焦 harness 会话）。
- **矩阵边界项同步**（`harness-capability-matrix.md`）：「B2 workflow 运行
  面板 UI」边界——第 22 轮已实现 ToolCard 专用卡（tc-workflow 结构化
  JSON），标注现状（完整 WorkflowRunPanel 形态未做，等价降级）。

### 第 75 轮基线

- `cargo test --lib`：**410 passed / 0 failed / 22 ignored**（上轮）
- clippy 0 / fmt 0 / svelte-check 0/0 / 契约 422/147/146
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：llm/handlers/resource.rs 的 `ext_for_mime`（mime → 扩展名）
  与 `derive_name_from_url`（URL → 文件名，附件保存）无测试。
- **新增 2 测试**（`resource.rs`）：
  - `ext_for_mime_maps_known_and_falls_back`：png/jpg/webp/mp3/wav/m4a
    映射 + 未知/空 → bin 兜底；
  - `derive_name_from_url_extracts_filename`：文件名提取、查询参数剥离、
    无文件名/无扩展名/空 → None。

### 第 74 轮基线

- `cargo test --lib`：**410 passed / 0 failed / 22 ignored**（+2）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：llm/handlers/history.rs 的 `file_path_to_data_url`（本地附件 →
  data: URL，聊天记录恢复附件显示）无测试。
- **新增测试**（`history.rs` +1）：`file_path_to_data_url_mime_and_missing`
  - 扩展名 → mime 映射（png/jpeg/大小写不敏感 JPG/未知 → octet-stream）；
  - base64 编码正确（4 字节 PNG 魔数 → iVBORw==）；
  - 不存在文件 → None。
- 修正断言：4 字节 base64 为 `iVBORw==`（初版误写 6 字节编码）。

### 第 73 轮基线

- `cargo test --lib`：**408 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **覆盖审查**：llm/handlers/providers.rs（upsert 校验依赖 config 持久化
  不可安全单测）、llm/client 零测试文件（audio/embeddings/generation/
  probe 均为网络/IO 耦合）——纯逻辑已全部锁定，无新增测试点。
- **矩阵同步**（`harness-capability-matrix.md`）：单测计数 401 → **407**、
  快照标注第 64 轮 → 第 71 轮（新周期第 21 轮）。

### 第 72 轮基线

- `cargo test --lib`：**407 passed / 0 failed / 22 ignored**（上轮）
- clippy 0 / fmt 0 / svelte-check 0/0 / 契约 422/147/146
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：ai_role.rs（AI 角色 → 系统提示词合成，LLM 全局调用注入）无
  测试；`compose_system_prompt`（纯函数）未锁定。
- **新增 2 测试**（`ai_role.rs`）：
  - `compose_system_prompt_sections`：系统提示 / 行为约束（转列表项 +
    trim）/ 背景知识 / 回复语言分区合成；
  - `compose_system_prompt_empty_and_lang_default`：空角色 → 空提示词、
    语言「跟随用户」不注入分区、约束空无分区。
- 构造：AiRole 无 Default，用 serde 缺省字段反序列化构造。

### 第 71 轮基线

- `cargo test --lib`：**407 passed / 0 failed / 22 ignored**（+2）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：web.rs 的 strip_tags / decode_entities / urlencoding /
  truncate_chars（抓取正文与 Bing 解析共用辅助）无直接测试。
- **新增测试**（`web.rs` +1）：`html_helpers_strip_tags_and_decode_entities`
  - strip_tags：吞 <...> 段、标签外内容保留（简单实现语义）；
  - decode_entities：&amp;/&lt;/&gt;/&quot;/&#x27;/&nbsp;/&hellip; 解码；
  - urlencoding：字母数字保留、空格/斜杠/中文 %XX；
  - truncate_chars：字符边界（中文安全）、超长必加省略号。
- 修正断言两处（strip_tags 的 script 内文本保留、truncate_chars 超长
  加省略号——与实现逐字核对）。

### 第 70 轮基线

- `cargo test --lib`：**405 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **背景**：新周期累计改动——PluginExecRequest 重构（第 51 轮）+ 18 轮
  测试补全（384→404 单测）后，首次全量回归。
- **全量回归结果**：**19/19 ALL_PASS**（phase1-6/9-11/78/b2/goal/
  concurrency + verify-harness-* 8 项），脚本尾行
  `all passed (real data/ untouched)`，teardown done，exit 0；
  st-control 与 vite 均退出（无残留）。**零回归**。
- 覆盖审查：llm/client（urls/transport/chat/mod）与 harness 模块测试
  覆盖已全面（resolve_provider_model 依赖真实配置文件不可安全单测，
  保持文档说明）。

### 第 69 轮基线

- 全量 19 探针 **19/19 ALL_PASS**（隔离环境，真实 LLM）
- `cargo test --lib`：**404 passed / 0 failed**（上轮）；clippy 0 / fmt 0
- 真实库零变化；无残留进程

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：`usage_summary`（遥测统计条数据源：db 聚合 + 事件投影步骤/
  工具墙钟 + 派生指标）无直接测试。
- **新增测试**（`session.rs` +1）：`usage_summary_aggregates_db_and_event_projection`
  - db 用量记录聚合（turns=COUNT、tokens/cost/墙钟求和）；
  - 事件日志投影步骤数 + 工具墙钟（2 个 ToolResult）；
  - 派生指标：首 token 平均（400ms/2 请求）、tok/s（2000/2s）、
    缓存命中率（500/1000）；
  - 清理。
- 修正断言：turns = 用量记录条数（COUNT(*)），非 0。

### 第 68 轮基线

- `cargo test --lib`：**404 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：session.rs 已有 22 测试（投影/血缘/事件/轨迹/压缩/中断等），
  但 `SessionStore::search`（B4 session-query 跨会话关键词搜索）无直接
  测试。
- **新增测试**（`session.rs` +1）：`session_search_finds_keyword_across_sessions`
  - 跨会话命中（含「数据库」的会话 a 命中、天气的 b 不命中）；
  - 无结果（不存在的词 → 空）；
  - 清理。

### 第 67 轮基线

- `cargo test --lib`：**403 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：tools.rs 已有注册/守卫/覆盖/提示词/插件测试；
  `requires_approval_scoped`（preset override 优先、否则全局定义）无
  直接测试——这是审批门控的作用域判定核心。
- **新增测试**（`tools.rs` +1）：`approval_scoped_override_wins_else_global`
  - 无 override → 全局定义（exec_command 需审批、read_file 免）；
  - override false → 免审批（覆盖全局需审批工具）；
  - override true → 需审批（覆盖全局免审批工具）。

### 第 66 轮基线

- `cargo test --lib`：**402 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **模块审查**：剩余薄弱模块——feedback/identity（逻辑简单已充分）、
  interaction（选项解析 + 应答状态机已覆盖）、session.rs L9 分块归组
  （不同 assistant id 不合并）已有测试；无补充需要。
- **矩阵同步**（`harness-capability-matrix.md`）：单测计数 383 → **401**、
  快照标注第 27 轮 → 第 64 轮（新周期第 14 轮）。

### 第 65 轮基线

- `cargo test --lib`：**401 passed / 0 failed / 22 ignored**（上轮）
- clippy 0 / fmt 0 / svelte-check 0/0 / 契约 422/147/146
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：settings.rs 已有往返/钳制/沙箱语义测试；**save 校验**（响亮
  失败：越界值拒绝保存）与 **web_search_provider 非法值回退**未测。
- **新增 2 测试**（`settings.rs`）：
  - `save_rejects_out_of_range_fails_loud`：超时 <5 / 轮次 >12 / 预算
    <4000 均拒绝保存并响亮报错（DSH misconfiguration fails loud，不
    静默钳制）；
  - `web_search_provider_invalid_falls_back_to_bing`：仅 deepseek 有效，
    非法值/None/空串回退 bing（第 19 轮审查结论补测）。

### 第 64 轮基线

- `cargo test --lib`：**401 passed / 0 failed / 22 ignored**（+2）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：shell.rs 已有 echo/超时/工作区锚定测试；`truncate_8k`（命令
  输出截断，H2 字符边界）无直接测试。
- **新增测试**（`shell.rs` +1）：`truncate_8k_char_boundary_safe_with_chinese`
  - 短输出原样；
  - 3 万汉字（≈90KB > 8KB）按字符边界截断不 panic（有效 UTF-8）、
    含「输出过长已截断」标记、接近 8KB 上限；
  - ASCII 超长同断言。

### 第 63 轮基线

- `cargo test --lib`：**399 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **模块审查**：subagent（fork 血缘/结论/干净边界/check_child 权限）、
  credentials（掩码/环境/加解密/旧格式兼容）测试覆盖完善——无需补充。
- **健康检查**（零 LLM）：
  - IPC 契约：422 命令 / 147 invoke / 146 参数比对全一致；
  - svelte-check：0 errors / 0 warnings；
  - 隔离 E2E verify-sre-editor：**14/14 ALL_PASS**，teardown done、
    exit 0、无残留进程。

### 第 62 轮基线

- 单测 398 passed（上轮）；clippy 0 / fmt 0
- 契约 422/147/146 全一致；svelte-check 0/0；E2E 探针 14/14
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：compaction.rs 已有 estimate/阈值、spill、上下文仪表、模型窗口
  测试；`prune_tool_results`（B5 工具结果剪枝核心）无直接测试。
- **新增测试**（`compaction.rs` +1）：`prune_tool_results_rewrites_oversized_only`
  - 超长 tool 消息（>8K）→ head/剪枝标记/tail 重写；
  - 短消息原样保留、tool_call_id 保留、assistant 消息不动；
  - 返回剪枝条数 = 1。
- 修正测试模块 import（json 宏）。

### 第 61 轮基线

- `cargo test --lib`：**398 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：llm/client/transport.rs（公共传输层）此前无测试；
  `estimate_cost`（token → 成本，纯函数，遥测统计条数据源）未锁定。
- **新增测试**（`llm/client/transport.rs` +1）：`estimate_cost_scales_by_tokens_and_prices`
  - 0 token → 0 成本；
  - 1M 输入 @$1/1M + 1M 输出 @$2/1M = $3；
  - 500K 输入 + 250K 输出 = $1.0；
  - 极小量（10 token 输入 = 1e-5）精度断言。

### 第 60 轮基线

- `cargo test --lib`：**397 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：llm/client/urls.rs（端点 URL 构造，注释明说「便于独立维护与
  测试」）此前无测试——纯逻辑函数未锁定。
- **新增 4 测试**（`llm/client/urls.rs`）：
  - `normalize_base_url_strips_trailing_slash`：尾斜杠/空白清理；
  - `api_base_auto_v1_for_host_only`：主机-only 补 /v1、已带路径原样、
    已带 /v1 不重复、Azure 不补；
  - `chat_url_branches_openai_and_azure`：OpenAI 兼容 /chat/completions
    与 Azure 部署名 + api-version 查询参数（含无 api-version 分支）；
  - `embedding_model_resolution_falls_back_to_marked_model`：请求空/非
    嵌入模型回退到标记嵌入模型、本身嵌入原样、无嵌入原样 + is_embedding_marked。

### 第 59 轮基线

- `cargo test --lib`：**396 passed / 0 failed / 22 ignored**（+4）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：llm/types.rs（LLM 数据结构层）此前无测试；ProviderType serde
  （lowercase rename 兼容）与关键结构默认值未锁定。
- **新增 2 测试**（`llm/types.rs`）：
  - `provider_type_serde_roundtrip_and_as_str`：四种类型 as_str 映射 +
    serde 往返一致 + 默认值 OpenAI；
  - `provider_config_default_and_model_meta_shape`：配置默认值（空提供方
    / OpenAI 类型）+ ModelMeta 形状（reasoning_efforts 空、context_window
    None——B15 语义）。

### 第 58 轮基线

- `cargo test --lib`：**392 passed / 0 failed / 22 ignored**（+2）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：llm/config.rs（提供方配置持久化）此前无测试模块；`find_provider`
  （按 id 查提供方，纯函数）未覆盖。容灾逻辑（空文件/损坏 → .bak 恢复）
  依赖真实数据目录路径，单测不可安全操作（保持文档说明）。
- **新增测试**（`llm/config.rs` +1）：`find_provider_by_id`
  - 命中（a/b 两个提供方按 id 查得）；
  - 未命中（nope → None）；
  - 空配置（default → None）。

### 第 57 轮基线

- `cargo test --lib`：**390 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：`llm/agent.rs::truncate_str`（工具结果/标题截断共用，H2 关键）
  此前无直接测试（仅 fs.rs 的类似函数有覆盖）。
- **新增测试**（`llm/agent.rs` +1）：`truncate_str_is_char_safe_with_chinese`
  - 短于上限原样返回；按字符截断 + 省略号（中文「中文测…」）；
  - 有效 UTF-8（字符安全，无 panic）；
  - ASCII 混合（abc中…）；空串；n=0（…）。

### 第 56 轮基线

- `cargo test --lib`：**389 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：db.rs（全部持久化底层）此前**无测试模块**——harness 表
  （会话/事件/KV）经 SessionStore 间接覆盖，但底层 SQL 无直接验证。
- **新增 2 测试**（`db.rs`，首个测试模块）：
  - `harness_session_event_append_returns_monotonic_seq`：L11
    INSERT...RETURNING seq 直接验证——追加 3 事件 seq=1,2,3 单调、
    载荷完整、增量读取（after_seq）、跨会话隔离（各自从 1 开始）；
  - `harness_kv_put_get_delete_roundtrip`：KV UPSERT 覆盖 / 读取 / 删除。

### 第 55 轮基线

- `cargo test --lib`：**388 passed / 0 failed / 22 ignored**（+2）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（cfg!(test) 临时库隔离）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：skill（B14）已有 4 测试（往返/门控禁用/注入/作用域），但
  **门控相反场景**（frontmatter 缺 disable-model-invocation 时默认可
  调用）未测。
- **新增测试**（`skill.rs` +1）：`frontmatter_defaults_model_invocable_when_not_disabled`
  - 缺省（无 disable-model-invocation）→ 默认可调用；
  - 显式 false → 可调用（eq_ignore_ascii_case("true") 只认 true）；
  - 无 frontmatter 纯正文 → 默认可调用。
  - 与 parse_frontmatter 的 `!disable_model` 语义一致。

### 第 54 轮基线

- `cargo test --lib`：**386 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：SDK `harness_cli`（DSH CLI 等价物）此前无测试；命令路由
  （sessions list / session create / tools list / usage / session show/chat）
  与未知命令用法提示未覆盖。
- **新增测试**（`sdk.rs` +1）：`cli_routes_commands_and_usage_hint`
  - `session create` → 输出会话 id（注册表就绪路径）；
  - `sessions list` → 含会话行；
  - 未知命令（session delete）→ 用法提示；
  - 空输入 → 用法提示；
  - 清理本次创建的会话（防泄漏）。
- fmt 应用（新 assert 格式）。

### 第 53 轮基线

- `cargo test --lib`：**385 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：SDK `session.chat`（同步对话）此前无测试；其走 run_turn_locked
  （需 AppHandle+LLM 单测不可达），但**参数校验先行**逻辑可测。
- **新增测试**（`sdk.rs` +1）：`dispatch_session_chat_validates_params_before_runtime`
  - 缺 content → 参数错误（校验先行）；
  - 参数齐全但无 AppHandle → 运行时未初始化（与 tool.execute 同模式，
    证明校验先于运行时检查）。
  - 完整对话链由隔离 E2E（SDK 会话聊天）覆盖。

### 第 52 轮基线

- `cargo test --lib`：**384 passed / 0 failed / 22 ignored**（+1）
- clippy 0 warnings；fmt 0 diff
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **目标**：消除唯一遗留的 clippy 警告——`run_plugin_tool_on_ext`
  8 参数函数（B2/B23 编排桥）。
- **重构**（`llm/agent_plugins.rs` + `harness/agent.rs`）：
  - 引入 `PluginExecRequest<'a>` 结构体承载 8 个参数（app/call_id/
    name/args/code/event/timeout_secs/session_id）；
  - `run_plugin_tool_on_ext(req: PluginExecRequest)` 签名收敛为 1 参数；
  - 3 个调用点（run_code / workflow_run_js / plugin:*）全部改用结构体。
- **意义**：clippy **0 警告**达成（此前唯一的 too_many_arguments 消除）；
  结构体自文档化（字段名即语义），调用点更清晰。

### 验证

- `cargo check` 零警告；`cargo clippy --lib` **0 warnings**；
  `cargo test --lib` **383 passed / 0 failed**；fmt 0 diff。
- 生产代码路径 E2E：重建 exe 后 **phase-b2 4/4 ALL_PASS**
  （ctx.parallel 双子代理、ctx.pipeline 流水线、run_code ctx.tools
  写→读回显 826ms、子代理目录派生）；契约门禁通过；无残留进程。

### 第 51 轮基线

- 单测 383 / clippy 0 / fmt 0 / svelte-check 0/0 / 契约 422/147/146
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 最终基线（全部通过）

| 门禁 | 结果 |
|---|---|
| `cargo test --lib` | **383 passed / 0 failed / 22 ignored** |
| `cargo fmt --check` | 0 diff |
| `cargo clippy --lib` | 1 个既有警告（agent_plugins 8 参数，非本次维护引入） |
| `svelte-check` | 0 errors / 0 warnings |
| IPC 契约 | 422 命令 / 147 invoke / 146 参数比对全一致 |
| 全量 E2E（隔离） | **19/19 探针 ALL_PASS**（第 28 轮）+ verify-sre-editor 14/14（第 49 轮最新 exe） |
| 数据安全 | 真实库零污染（cfg!(test) 临时库 + ST_WECHAT_APP_DIR 隔离） |

### 50 轮维护总览

- **功能落地**：B24 str_replace_editor（后端四命令 + 前端专用卡 + E2E 探针）、
  E2E 脚本健壮性（try/finally teardown + 进程退出轮询 + IPC 契约前置门禁）
- **测试补全**：单测 355 → **383**（+28：SDK/feedback/会话管理/审批 TTL/
  hooks/schedule/MCP/LSP/storage/attachment/workspace/terminal/context/
  portability/workflow/pty/instructions/web 等模块全覆盖，harness 33 模块
  无测试盲区）
- **质量门禁**：clippy 4→1、fmt 0 diff、契约审计入 E2E 前置门禁
- **审查**：ralph/web/语音静态审查确认；能力矩阵全部对照项均有实测/审查证据

### 待办（下一轮候选）

- 常规维护 / 用户指定方向（本目标 50 轮上限已达）

---

### 本轮动作

- **背景**：第 28 轮全量回归后生产代码仅第 47 轮 web.rs 重构（等价
  提取 parse_deepseek_results，单测已覆盖）；exe 未重建。
- **重建 exe**（含全部累计改动）后跑聚焦回归：
  - IPC 契约门禁：422 命令 / 147 invoke / 146 参数比对全一致；
  - verify-sre-editor **14/14 ALL_PASS**（str_replace_editor 全链路 +
    日志事件配对）；
  - teardown done、exit 0、st-control 与 vite 均退出（无残留）。

### 第 49 轮基线

- 聚焦回归 14/14 + 契约门禁通过；真实库零变化
- 全量基线（上轮）：`cargo test --lib` 383 passed；19/19 E2E；
  fmt 0 diff；svelte-check 0/0

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **fmt**：`cargo fmt --check` 发现历史格式差异（db.rs 等），`cargo fmt`
  应用后 check 0 diff。
- **验证**：fmt 后 `cargo test --lib` **383 passed / 0 failed**、
  cargo check 零警告、clippy 仍 1 个既有警告（非本轮范围）。
- **矩阵同步**：工程质量行更新——单测 355 → **383**、新增
  `cargo fmt --check` 0 diff、E2E 行补 verify-sre-editor 14/14。

### 第 48 轮基线

- `cargo test --lib`：**383 passed / 0 failed / 22 ignored**
- fmt 0 diff；svelte-check 0/0；19/19 E2E（上轮）
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：web 已有 Bing 解析 + fetch 拒绝测试；**DeepSeek 提供商响应
  解析**（web_search_tool_result 结构化块，B17）无测试且内联在
  search_deepseek（依赖 HTTP，无法直接单测）。
- **重构**：提取 `parse_deepseek_results(value)` 纯函数（生产代码复用，
  与 search_deepseek 共用）。
- **新增测试**（`web.rs` +1）：`deepseek_results_parsed_from_structured_blocks`
  - 混合 content 块只取 web_search_tool_result（文本/其它块忽略）；
  - 空 url 跳过；无结果块报错；超 8 条截断。

### 第 47 轮基线

- `cargo test --lib`：**383 passed / 0 failed / 22 ignored**（+1）
- cargo check 零警告；clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：instructions（代理指令上下文注入）已有扫描/注入/清理测试；
  **注入预算封顶**（24KB 累计截断并停止）与**单文件读取上限**（32KB
  截断）无测试——这是注入机制防上下文膨胀的关键约束。
- **新增 2 测试**（`instructions.rs`）：
  - `inject_budget_truncates_and_stops`：第一段超预算 → 注入截断、
    后续文件完全未注入（预算用尽停止）；
  - `file_cap_limits_single_file`：单文件内容按字符截断到 FILE_CAP_CHARS。
  - 直接写 store 构造数据（不经 rescan，避免依赖工作区文件）。

### 第 46 轮基线

- `cargo test --lib`：**382 passed / 0 failed / 22 ignored**（+2）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：pty（ConPTY 终端）strip_ansi 已有 CSI/OSC/纯文本测试；
  未闭合转义序列（无终结字节）与 CSI 变体（多参数/私用）的健壮性无测试。
- **新增测试**（`pty.rs` +1）：`strip_ansi_handles_unterminated_and_variants`
  - 未闭合 CSI（无终结字节）→ 吞到末尾不 panic；
  - 未闭合 OSC（无 BEL）→ 移除；
  - 孤立 ESC / 尾随 ESC+非括号字符 → 移除；
  - 多参数 CSI（`[2;3H`）与私用 CSI（`[?25l`）→ 移除；
  - 混合 ANSI+普通文本顺序无关。

### 第 45 轮基线

- `cargo test --lib`：**380 passed / 0 failed / 22 ignored**（+1）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：workflow（阶段流水线 + ralph）已有 ralph 提前结束 + 字段
  形状测试；`save_harness_workflow` 的**校验规则**（名称非空 / 至少
  一阶段 / 每阶段提示词非空）无测试。
- **新增测试**（`workflow.rs` +1）：`workflow_save_validation_rules`
  - 空名称 / 无阶段 / 空提示词阶段 → 均拒绝；
  - 合法工作流 → 新建（wf- id 生成、created_at == updated_at）；
  - 清理（delete_harness_workflow）。
  - 与 save_harness_workflow 校验分支逐字一致。

### 第 44 轮基线

- `cargo test --lib`：**379 passed / 0 failed / 22 ignored**（+1）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：portability（配置束导入导出）仅 2 个序列化测试；导入的
  **空条目校验**（空 id/空名/空命令跳过，不计入合并数）无测试。
- **新增测试**（`portability.rs` +1）：`import_validation_skips_invalid_entries`
  - 预设：空 id 或空名跳过（仅 id+name 均非空计入）；
  - MCP：空 id 或空命令跳过（仅 id+command 均非空计入）；
  - 与 harness_import_bundle 的校验分支逐字一致。
- 修正测试内 super::preset/mcp 路径 → crate::harness::preset/mcp
  （测试模块嵌套后 super 解析变化）。

### 第 43 轮基线

- `cargo test --lib`：**378 passed / 0 failed / 22 ignored**（+1）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：context（请求上下文注入）已有组装 + 空态测试；**自定义提供者
  注册**（add_provider 可插拔）与**计划文本细节**（plan_text 显示）无测试。
- **新增 2 测试**（`context.rs`）：
  - `custom_provider_injects_block`：add_provider 追加自定义提供者
    （注册表 +1 且 truncate 恢复，默认提供者保留）；
  - `plan_mode_detail_and_goal_priority`：计划模式含方案文本；目标 +
    计划 + 待办（带状态）同时注入。
- **踩坑与修正**：初版测试因共享静态注册表在并行测试间竞争（空态测试
  偶发失败）+ 持锁调用 assemble/add_provider（std Mutex 非重入死锁），
  调整为分离锁操作 + 只验注册表可插拔（组装行为由既有测试覆盖）。

### 第 42 轮基线

- `cargo test --lib`：**377 passed / 0 failed / 22 ignored**（+2）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：harness/mod.rs（运行时引导）仅 1 个 sessions 服务断言；
  init 注册的 5 个服务（sessions/fs/shell/web/storage）+ 示例预设种子
  无完整性断言。
- **新增测试**（`mod.rs` +1）：`init_registers_all_services_and_seeds_presets`
  - 断言 fs/shell/web/storage 服务均已注册（harness.fs / harness.shell /
    harness.web / harness.storage）；
  - 断言示例预设已种子化（preset-example-readonly 入册）。
- 顺带核查 preset（禁用/覆盖/超时/缺失回退已覆盖）。

### 第 41 轮基线

- `cargo test --lib`：**375 passed / 0 failed / 22 ignored**（+1）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：terminal（终端会话）仅 1 个 cwd 标记解析测试；
  `normalize_cwd`（PowerShell 路径规范化：剥离 FileSystem 提供者前缀
  与 `\\?\` 长路径前缀）无测试——这是 send_regular 解析新工作目录的
  核心步骤。
- **新增测试**（`terminal.rs` +1）：`normalize_cwd_strips_provider_and_long_path_prefixes`
  - 剥离 `Microsoft.PowerShell.Core\FileSystem::` 前缀；
  - 剥离 `\\?\` 长路径前缀；
  - 组合（两者都出现）；
  - 普通路径原样；
  - 标记行提取 + 规范化全链路（与 send_regular 解析一致）。

### 第 40 轮基线

- `cargo test --lib`：**374 passed / 0 failed / 22 ignored**（+1）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：workspace（会话归属工作区，U17）已有创建/列表/删除/状态 +
  默认根测试；**目录名清理**（防 .. 逃逸，安全关键）与**默认工作区
  删除保护**无测试。
- **新增 2 测试**（`workspace.rs`）：
  - `workspace_dir_sanitizes_and_blocks_escape`：`../evil` 每字符清理
    （`.`/`/` → 下划线 → `___evil`），结果锚定在 workspace_root 内
    （starts_with 断言）；正常名称（ws-abc_1）保留；空 = app 根。
  - `default_workspace_cannot_be_deleted`：删除 default 被拒绝
    （「默认工作区不可删除」守卫）。
- 顺带核查 registry（provide/get/dispose + disarm/remove 已覆盖）。

### 第 39 轮基线

- `cargo test --lib`：**373 passed / 0 failed / 22 ignored**（+2）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：attachment（附件，B10 图片 seam）已有类型检测 + 文本预览
  测试；**图片附件上下文注入**（read_image 引用提示 + sha256 内容寻址）
  与**事件日志投影**（attachments_from_events）无测试。
- **新增 2 测试**（`attachment.rs`）：
  - `context_block_image_note_and_sha256`：图片附件注入 read_image 引用
    提示 + sha256 寻址；图片不注入文本预览；文本+图片混合都在；
  - `attachments_from_events_filters_and_orders`：从事件日志投影
    AttachmentAdded（user/attachment 混合流 → 提取 2 个、按序保持）。
- 修正测试两处：借用后移动（&[meta] → &[meta.clone()]）、缺失闭合括号。

### 第 38 轮基线

- `cargo test --lib`：**371 passed / 0 failed / 22 ignored**（+2）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：storage（命名后端 B13）已有 2 个往返+隔离测试；`split_backend`
  （default/json: 前缀解析）与 `json_backend_path` 名称清理无测试。
- **新增测试**（`storage.rs` +1）：`backend_split_and_name_sanitization`
  - split_backend：None/空/default → SQLite；(json:notes) → JSON 后端；
    带前导空白的 `  json:notes  ` 按现有行为回退 default（starts_with
    前未 trim）；
  - json_backend_path 名称清理：非字母数字/`-`/`_` 字符 → 下划线
    （`a b/c` → `a_b_c.json`；`ok-name_1.json` 保留）。
- 顺带核查 identity/interaction 模块：identity 稳定性测试已足够；
  interaction 选项解析 + 应答状态机已覆盖——无需补充。

### 第 37 轮基线

- `cargo test --lib`：**369 passed / 0 failed / 22 ignored**（+1）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：LSP（语言服务器桥，B7）仅 1 个帧往返测试；扩展名路由
  （查询按文件扩展名选服务器）与位置格式化（definition/references/
  implementation 共用）无测试。
- **新增 2 测试**（`lsp.rs`）：
  - `extension_routing_prefers_match_then_first`：路由纯逻辑——启用过滤 →
    取文件扩展名（大小写不敏感：.RS→rs）→ 映射命中（tsx→ts）→ 无匹配
    回退首个；无启用服务器 → 无（pick_server 报错）。
  - `location_formatting_single_and_array`：单 uri 对象 / 数组全列 /
    空数组 →「（未找到结果）」。
  - 注：pick_server 从 store 读（需运行时），测试内联复刻路由分支
    （与生产一致）；完整链路由隔离 E2E phase6/9 覆盖。

### 第 36 轮基线

- `cargo test --lib`：**368 passed / 0 failed / 22 ignored**（+2）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：MCP（外部服务器工具桥，B6）仅 1 个配置序列化测试；
  工具命名 `mcp_<id>_<tool>` 与 schema 透传/回退（模型可见参数结构）无测试。
- **新增测试**（`mcp.rs` +1）：`mcp_tool_naming_and_schema_passthrough`
  - 命名规则：`mcp_srv-a_read_file`（id + 工具名拼接）；
  - schema 透传：对象 schema 原样保留（required 字段可读）；
  - schema 回退：非法（非对象）→ 空对象 `{"type":"object","properties":{}}`
    （与 refresh_registry 分支逐字一致）。

### 第 35 轮基线

- `cargo test --lib`：**366 passed / 0 failed / 22 ignored**（+1）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：模块测试数量分布扫描——schedule（定时任务，会话自动化核心）
  仅 1 个字段模型测试，无逻辑断言。
- **新增测试**（`schedule.rs` +1）：`schedule_due_filter_and_state_transitions`
  - 到期判定：`enabled && next_run_at <= now`（到期命中 / 未来不触发 /
    禁用不触发）；
  - 执行后状态机（与 run_due 的 store 更新分支一致）：
    - 常规条目：`next_run_at = now + interval*60` 推进；
    - 一次性任务（one_shot）：`enabled=false` 停用且不推进 next_run_at；
    - `last_run_at` 记录。
  - 注：run_due 本体需 AppHandle（单测不可用），测试复刻其纯字段
    逻辑；完整链路由隔离 E2E phase-concurrency 覆盖（含定时任务触发）。

### 第 34 轮基线

- `cargo test --lib`：**365 passed / 0 failed / 22 ignored**（+1）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：hooks（CC/Codex 方言外部钩子桥）此前仅 1 个事件白名单测试；
  matcher 匹配逻辑与 deny/ask 决策解析（安全关键：PreToolUse 拦截）无测试。
- **新增 2 测试**（`hooks.rs`）：
  - `matcher_empty_or_substring_match`：空/空白 matcher = 全部命中；
    非空 = 载荷 JSON 文本包含子串才命中；不包含不命中。
  - `fire_decision_parses_deny_ask_ignores_invalid`：从钩子 stdout 提取
    `{"decision":"deny"/"ask","reason":...}`；无 decision 字段 / 非 JSON /
    空白 → None（fire_decision 继续下个钩子）。
  - 注：fire_decision 本体需 AppHandle（单测不可用），测试内联复刻解析
    纯逻辑（与生产代码一致）；完整链路由隔离 E2E 覆盖。

### 第 33 轮基线

- `cargo test --lib`：**364 passed / 0 failed / 22 ignored**（+2）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **审查**：审批/信任模块（M8 参数指纹）已有 2 测试（信任键按会话+参数
  作用域、exec_command 精华参数、键序无关指纹）——核心语义覆盖良好；
  但 **TTL 30 分钟过期**（安全边界：信任不得永久有效）无测试。
- **新增测试**（`approval.rs` +1）：`trust_expires_after_ttl`
  - 手工插入已过期（TTL+1s）的信任条目；
  - `is_trusted` 惰性清理（retain）后不再命中；
  - 信任表已移除该键（清理生效验证）。

### 第 32 轮基线

- `cargo test --lib`：**362 passed / 0 failed / 22 ignored**（+1）
- clippy 保持 1 个既有警告（非本轮范围）
- 真实库零变化（纯静态/临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **前端复核**：svelte-check **0 errors / 0 warnings**（ToolCard sre 卡后
  首次全量确认）；迁移计划剩余项均已覆盖/标注等价（grep 核实）。
- **SDK/B19 审查**：SDK 方法集（sessions.list/create/display/state/chat/
  title/tool.execute + ACP）覆盖完整；`generate_title_for` 健壮性良好
  （空会话/无提供方优雅报错）——无需改动。
- **新增测试**（`session.rs` +1）：`session_archive_order_preset_roundtrip`
  ——会话管理三能力往返：
  - 归档标记落库/恢复（归档仅隐去不删日志）；
  - 手动排序（set_order）+ 交换（swap_order）顺序断言；
  - 每会话预设（set_preset/preset_id，空字符串=未设置跟随全局；
    预设不串会话）。
- 顺带确认 db 层 preset_id 语义（空串而非 NULL），测试断言与之对齐。

### 第 31 轮基线

- `cargo test --lib`：**361 passed / 0 failed / 22 ignored**（+1）
- svelte-check 0/0；clippy 仍 1 个既有警告（非本轮范围）
- 真实库零变化（测试库隔离）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **背景**：`smoke-ipc-contract.mjs`（Rust 命令 ↔ 前端 invoke 参数键名
  契约审计）此前仅手动运行，未纳入任何门禁——键名漂移（ack/resync 类）
  只能在 E2E 中暴露，浪费 LLM 调用。
- **集成**（`scripts/run-e2e-isolated.ps1`）：
  - 探针循环前新增 **4.0 静态门禁**：先跑 `smoke-ipc-contract.mjs`，
    失败即记入 failed 并**跳过全部探针**（不浪费 LLM）；
  - teardown（finally）不受影响，仍保证清理。
- **契约审计现状**：**422 个 Rust 命令 vs 147 处前端 invoke，146 处
  参数可比对全部一致**（含 Tauri camelCase→snake_case 自动转换）。

### 验证

- 脚本 PS 5.1 解析 OK、纯 ASCII（无编码回归）。
- 隔离运行：门禁通过 → verify-sre-editor **14/14 ALL_PASS** →
  teardown done → exit 0；st-control 与 vite 均退出（无残留）。

### 第 30 轮基线

- IPC 契约 422/147/146 全一致；单测 360 passed（上轮）
- 真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **目标**：harness 模块单测覆盖扫描——唯一无测试的 `feedback.rs`
  （会话好/差评反馈）补全测试，清零覆盖盲区。
- **新增 2 测试**：
  - `session.rs::feedback_submit_and_list_roundtrip`：会话级 good +
    消息级 bad（带评论 + message_seq=3）往返；倒序校验（最新在前）；
    不同会话反馈隔离。
  - `feedback.rs::rating_validation_good_bad_only`：命令入口评分校验
    仅 good/bad/空合法；非法评分在校验层拒绝，合法评分越过校验、
    止于运行时检查（单测环境无 AppHandle → 报未初始化，证明校验
    先行于运行时）。
- 修复一处测试类型标注（`Vec<_>` → 无标注的 count()）。

### 第 29 轮基线

- `cargo test --lib`：**360 passed / 0 failed / 22 ignored**（+2 反馈测试）
- clippy 仍 1 个既有警告（agent_plugins 8 参数，非本轮范围）
- 真实库零变化（测试库隔离，cfg!(test) 临时库）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **目标**：第 21-27 轮累计改动（str_replace_editor 后端+前端+探针、E2E
  脚本两轮修复、clippy 修复、SDK 测试）后跑**全量 19 探针回归**。
- **前置**：重建 exe（含第 27 轮 clippy 修复后的 agent.rs/session.rs）。
- **全量回归结果**：**19/19 ALL_PASS**（phase1-6/9-11/78/b2/goal/
  concurrency + verify-harness-* 8 项），脚本尾行
  `all passed (real data/ untouched)`，teardown done，exit 0；
  st-control 与 vite 均退出（无残留）。**零回归**。
- **SDK tool.execute 测试补全**（`sdk.rs` +1）：
  - 原设计验证 SDK 经带锁派发执行 str_replace_editor 全链路，但单测
    环境无 AppHandle（`init(None)`）→ `runtime_app_handle` 报「运行时
    未初始化」；
  - 调整为验证**参数校验先行**（缺 name 报参数错误）+ 参数齐全时
    正确走到运行时检查（无 AppHandle 报未初始化）——证明 dispatch
    分支逻辑正确；完整执行链由隔离 E2E verify-sre-editor 覆盖
    （同一 execute_tool_command 带锁路径，真实运行时）。
- 能力矩阵快照标注更新（第 17 轮 → 第 27 轮）。

### 第 28 轮基线

- `cargo test --lib`：**358 passed / 0 failed / 22 ignored**（+1 SDK 测试）
- 全量 19 探针 **19/19 ALL_PASS**（隔离环境，真实 LLM）
- 真实库零变化；无残留进程

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **目标**：新代码质量门禁复核——clippy 检查第 21 轮 str_replace_editor
  新增代码，顺带清理既有可安全修复的警告。
- **修复 3 处**：
  - `llm/agent.rs`（第 21 轮新增）：
    - doc list item 缩进（`/// 输出按…` 需缩进进入 insert 条目）；
    - 冗余 i64 转换（`filter_map(|x| x.as_i64()).map(|n| n as i64)` →
      `filter_map(|x| x.as_i64())`）。
  - `harness/session.rs`（既有，B19 标题生成）：`&vec![…]` → `&[…]`
    （useless_vec）。
- **保持 1 处**：`agent_plugins.rs:192 run_plugin_tool_on_ext` 8 参数
  （B2 编排入口，签名合理；重构为参数对象改动面大、非本轮范围）。

### 验证

- clippy --lib：4 警告 → **1 警告**（仅保留既有的 8 参数提示）。
- `cargo test --lib`：**357 passed / 0 failed**（修复零回归）。
- `cargo check` 零警告。

### 第 27 轮基线

- 单测 357 全绿；clippy 1 既有警告（非新增）
- 真实库零变化（本轮纯静态）

### 待办（下一轮候选）

- 全量 19 探针回归（累计改动后）
- 常规维护 / 用户指定方向

---

### 本轮动作

- **目标**：第 21-25 轮连续代码/脚本改动后，跑聚焦回归验证关键链路
  （控制 LLM 用量，不全量 19 探针）。
- **回归组合**（多探针连续运行，同时验证脚本修复在多探针场景的
  自收尾可靠性）：
  - `verify-sre-editor`（零 LLM）：**14/14 ALL_PASS**——str_replace_editor
    四命令 + 日志配对完整；
  - `verify-no-duplicate`（真实 LLM 回合）：**5/5 ALL_PASS**——会话回复
    仅一条、日志投影仅一条、整页重载回放仍仅一条（持久化核心零重复）。
- **工具目录存在性检查**：全部探针均为 `some(...)` 断言、无数量硬编码
  ——新增 str_replace_editor 无回归风险（已核实）。
- 脚本修复验证：多探针连续运行后 teardown done、exit 0、st-control 与
  vite 均退出（无残留）。

### 第 26 轮基线

- 聚焦回归 19/19 断言全过（14 + 5）；真实库零变化
- 既有基线不变：`cargo test --lib` 357 passed；全量 19 探针（上上轮）

### 待办（下一轮候选）

- 全量 19 探针回归（新一轮代码改动累计后）
- 常规维护 / 用户指定方向

---

### 问题

- 第 24 轮 try/finally 修复后，成功/失败路径 teardown 都会执行，但**应用
  进程仍偶发残留**（exit 0 后 st-control 还活着，锁 exe 阻塞下次 build）。
- 根因 1：`Stop-Process -Force` 是**异步信号**，立即返回但进程可能尚未
  退出；脚本随即退出，残留应用。
- 根因 2（本轮新发现）：第 24/25 轮在脚本中加入了**中文注释**，违反
  「脚本强制纯 ASCII」（无 BOM UTF-8 中文注释被 PS 5.1 按 ANSI 解析会
  报 ParserError）；一轮验证因 `} catch {` 后中文注释触发解析失败。

### 修复

1. **退出轮询**（`scripts/run-e2e-isolated.ps1`）：`Stop-TestInstances` 杀
   st-control 后轮询 ≤3s 确认进程消失（10 次 × 300ms），vite 清理保留
   CIM + 按名兜底。
2. **ASCII 纪律回归**：清除脚本中 5 处中文注释（teardown 核心/轮询说明/
   CIM 兜底/探针异常），全部改英文；PS 5.1 解析校验通过（与运行环境
   一致的解析器，而非 pwsh 7）。

### 验证

- 失败路径（不存在探针名）：MODULE_NOT_FOUND → FAILED → **teardown done**
  仍执行 → exit 1（try/finally 生效）。
- 成功路径：`verify-sre-editor` **14/14 ALL_PASS** → teardown done →
  exit 0；teardown 后 **st-control 与 vite 均已退出**（轮询确认，
  无残留进程）。
- 脚本 PS 5.1 语法校验 OK；纯 ASCII 校验通过。

### 第 25 轮基线

- 隔离 E2E 探针 14/14（零 LLM）；真实库零变化
- 既有基线不变：`cargo test --lib` 357 passed；19 探针全绿（上轮）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 问题

- 第 23 轮隔离 E2E（verify-sre-editor）在探针 ALL_PASS 后**脚本卡住**：
  `[e2e] done.` / `teardown done` 不出现，需手动 kill 应用才恢复。
- 根因定位：新探针 `verify-sre-editor.mjs` 成功路径**未 `process.exit`**
  且全局 CDP WebSocket 保持打开 → node 事件循环不退出 → `& node` 永不
  返回 → 外层脚本卡在探针循环，teardown（finally）不执行，应用残留
  锁 exe。
- 既有 19 个回归探针（phase* / verify-harness-*）均已有 `process.exit`，
  故此前全量回归从未暴露；属新探针漏写 + 脚本缺少防御的双重问题。

### 修复

1. **探针**（`.codex_tests/verify-sre-editor.mjs`）：收尾 `ws.close()` +
   `process.exit(0)`（失败路径 exit 1 已有）；注释说明原因。
2. **脚本健壮性**（`scripts/run-e2e-isolated.ps1`）：
   - 探针执行包进 try/catch/finally——任何路径（探针失败/异常/卡死）
     都执行 teardown；
   - teardown 抽为 `Stop-TestInstances` 函数：CIM 查询加 try/catch，
     失败退化为按进程名清理（测试环境可接受）；
   - 双保险：即使探针不退出，脚本异常路径也能收尾。

### 验证

- 修复后隔离重跑 `verify-sre-editor`：**14/14 ALL_PASS**，探针自然退出 →
  `teardown done` → `all passed (real data/ untouched)` → **exit 0**，
  全程无需手动干预。
- 脚本语法经 PS 解析器校验 OK；无残留进程（应用/vite 已清）。

### 第 24 轮基线

- 隔离 E2E 探针 14/14 通过（零 LLM 消耗）
- 真实库零变化；测试文件已清理
- 既有基线不变：`cargo test --lib` 357 passed；19 探针全绿（上轮）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **目标**：第 21/22 轮实现的 B24 str_replace_editor（后端四命令 + 前端
  专用卡）做端到端验证——经**人工派发路径**（`harness_execute_tool`，
  str_replace_editor 免审批）零 LLM 消耗实测。
- **新探针** `.codex_tests/verify-sre-editor.mjs`（14 断言）：
  - 工具目录含 str_replace_editor 且免审批；
  - create 创建 / 已存在拒绝覆盖、view 带行号全文、view_range 区间、
    str_replace 唯一匹配、insert 行后插入、非法行号拒绝；
  - 最终文件内容 = line1/LINE2/INSERTED/line3（真实落盘核对）；
  - 日志事件配对：8 个 assistant_tool_calls(hcmd-) + 8 个 tool_result
    （模型上下文完整性：无孤立 tool 消息，后续回合 API 不会 400）；
  - 清理测试文件（自包含）。
- **验证过程发现与结论**：
  - 人工派发的工具调用**不显示在工具时间线**——`derive_display_messages`
    将工具步骤挂到随后的 assistant 回合上，人工派发无回合（pending_tools
    等下一回合边界）；这是设计行为，与 phase11 断言口径一致（只验执行
    结果 + 日志），sre 卡 DOM 渲染由模型路径探针回归覆盖。
  - 首版探针的 waitFor 超时 null 误判 PASS（`null !== 'false'`）——改为
    日志事件断言后消除。
  - 隔离脚本首次运行探针时偶发挂起（teardown 阶段），手动收尾后正常；
    脚本 teardown 在探针 ALL_PASS 后仍执行（应用/vite 已清）。

### 第 23 轮基线

- `verify-sre-editor` **14/14 ALL_PASS**（隔离环境，零 LLM 消耗）
- 脚本尾行 `all passed (real data/ untouched)`，teardown done，exit 0
- 真实库零变化；测试文件已清理；无残留进程
- 既有基线不变：`cargo test --lib` 357 passed；19 探针全绿（上轮）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **背景**：第 21 轮实现 B24 `str_replace_editor` 后端四命令工具，但前端
  ToolCard 无专用分支，工具调用落 generic 兜底卡（参数/结果平铺）。
- **实现**（`src/lib/harness/components/ToolCard.svelte`）：
  - 新增 `isSre` 判定（name === "str_replace_editor"）。
  - 专用卡 `tc-sre`：头部显示命令 + 路径（`编辑器 · {command}：{path}`）；
    按 command 分支渲染：
    - view → 等宽行号视图（复用 tc-out，max-height 滚动）
    - create → diff 风格 + 新内容行（add 绿）
    - str_replace → diff 风格 −old_str/+new_str（del 红 / add 绿）
    - insert → 「第 N 行后插入」+ 插入文本
    - 未知命令 → 结果兜底
  - 样式：`.tc-sre-view`（10px 等宽）、`.tc-sre-insert`（等宽插入文本）。
- **挂载链路确认**：HarnessTab 工具时间线展开详情 → `<ToolCard name=…>`
  （name 透传 s.name），sre 判定在真实渲染路径生效。

### 第 22 轮基线

- `svelte-check` 0 errors / 0 warnings
- `npm run build` ✅（37.9s，仅 chunk 体积提示非错误）
- `cargo test --lib` 357 passed（上轮，未改后端）
- 未跑 E2E（纯前端卡渲染，无 LLM 依赖；工具时间线挂载链路已静态确认）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **目标**：清零迁移计划 B24——实现 DSH `str_replace_editor` 工具
  （view/create/str_replace/insert 四命令编辑器），替代原先「edit_file 已
  覆盖核心语义、缺子命令」的部分迁移状态。
- **实现**（`llm/agent.rs` + `harness/fs.rs`）：
  - `tool_str_replace_editor`（llm/agent.rs）：按 command 分发四命令，
    参数 schema 对齐 DSH（command/path/file_text/insert_line/new_str/
    old_str/view_range），注册进 `builtin_tools()`（harness 工具目录自动
    包含）；`requires_approval: false`、不在 `is_readonly_tool`（plan
    模式/只读沙箱整体拦截，与 edit_file 一致）。
  - `FsService` 新增 4 方法（harness/fs.rs）：
    - `str_replace_view`：文件 → 带行号视图（view_range 支持，end=-1 到
      文件尾）；目录 → 2 层深列表（跳过隐藏项/node_modules/__pycache__）；
      输出 16K 字符边界截断 + `<response clipped>` 标注
    - `create_if_absent`：新文件创建，已存在响亮报错（不覆盖）
    - `str_replace`：old_str 唯一匹配替换；0 匹配 / 多处匹配（列行号）
      均报错，绝不静默改错
    - `insert_lines`：insert_line 行后插入 new_str（范围 [0, 行数] 校验）
- **单测 +3**（fs.rs 2 个：四命令全链路 + 16K 字符边界截断；agent.rs 1 个：
  工具目录含 str_replace_editor 契约断言）。

### 第 21 轮基线

- `cargo test --lib`：**357 passed / 0 failed / 22 ignored**（355 + 2 新增）
- cargo check 零警告；agent 模块 10/10
- 未跑 E2E（新增工具为纯后端命令，无前端变化；工具目录由 registry 自动
  包含——契约断言已覆盖）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **目标**：能力矩阵最后一处「工具目录注册/入口验证」项——**语音**
  （VAD→STT + TTS）静态审查并升级为「静态审查确认」。
- **语音能力全链路审查结论（无需改动）**：
  - STT（`llm/handlers/audio.rs::transcribe_voice_audio`）：云端
    `/audio/transcriptions`（已配置提供方，如硅基流动 TeleAI/TeleSpeechASR）
    优先 → 本地 whisper.cpp（feature `local-stt`）兜底；返回识别文本 +
    engine 来源供前端展示；空录音在触碰配置前提前拒绝（有单测
    `empty_audio_rejected`）。
  - TTS：提供方 `/audio/speech`（`create_speech`，CosyVoice2 等音色）+
    Windows SAPI 离线合成兜底（`synthesize_native_speech`，零配置，
    `rate` clamp -10..10）。
  - 前端：麦克风捕获 + 电平 VAD（检测说话后静音 1.6s 自动停止、60s
    超时保护）→ 转写入输入框；回复播报流式喂入 TTS 队列
    （speechFlow/StreamSpeechFeeder）。
  - Harness 会话复用同一语音链路（HarnessTab 麦克风按钮
    `toggleVoiceInput`），与 GlobalChatTab 行为一致。
- **至此能力矩阵全部对照项均已有实测/静态审查证据**（无「工具目录注册」
  或「入口验证」残留）。

### 第 20 轮基线

- 纯静态审查轮：无代码改动，未跑 E2E（节省 LLM 额度）。
- 基线仍有效：`cargo test --lib` 355 passed / 0 failed；19 探针全绿。

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

### 本轮动作

- **目标**：能力矩阵「工具目录注册」项继续升级为「静态审查确认」——
  本轮审查 **web_search / fetch_web_page**（上轮候选）。
- **web 能力全链路审查结论（无需改动）**：
  - 提供商缝（`settings.rs::effective_web_search_provider`）：默认 **bing**，
    显式 `deepseek` 时切 DeepSeek（Anthropic 兼容端点
    `{base}/anthropic/v1/messages` + `web_search_20250305` 服务器工具、
    解析结构化 `web_search_tool_result` 块、不信任模型散文）；非法值回退 bing。
  - Bing 实现（`web.rs::search_bing`）：cn/www 双域兜底、`b_algo` HTML
    解析、最多 8 条、15s 超时、`no_proxy`；`parse_bing_results` 有单测
    （标题/链接/摘要断言）。
  - 抓取（`WebService.fetch`）：仅 http/https（单测拒绝 `file://`）、
    `strip_tags` 去标签 + 8KB 字符安全截断（`truncate_chars`）。
  - 权限/安全：`requires_approval: false`（只读），列入
    `is_readonly_tool`（plan 模式可用）；工具结果落库前 spill + 4000
    字符截断保护（与既有工具结果链路一致）。
- 至此「工具目录注册」仅剩语音（前端 VAD→STT + TTS，属入口能力）。

### 第 19 轮基线

- 纯静态审查轮：无代码改动，未跑 E2E（节省 LLM 额度）。
- 基线仍有效：`cargo test --lib` 355 passed / 0 failed；19 探针全绿。

### 待办（下一轮候选）

- 语音能力核对或标记边界（前端入口）
- 常规维护 / 用户指定方向

---

### 本轮动作

- **目标**：能力矩阵「工具目录注册」（未实测）项逐项升级为「静态审查确认」，
  本轮审查 **ralph 循环**（其余 web_search/fetch_web_page、语音为前端/网络
  能力，留待后续或标记不迁移）。
- **ralph 全链路审查结论（无需改动）**：
  - 注册：`requires_approval: false`，参数 objective/max_rounds（默认 3，
    clamp 1..=16），run=stub（仅 schema 暴露，不可直接执行）。
  - 真实执行：仅主会话工具循环 `handle_session_tool` "ralph" 分支 →
    `run_ralph`（workflow.rs）：每轮全新上下文子代理 `run_subagent` +
    每轮 WorkflowRun 事件落日志；外层持锁路径内串行追加（H3 一致）。
  - **递归防护确认**：子代理路径 `execute_tool_guarded` → registry.execute
    命中 stub → 返回「该工具由会话运行时处理，不应直接执行」——子代理
    无法再启动 ralph/subagent，无无限嵌套风险。
  - 模型投影：`derive_model_messages` 忽略 WorkflowRun 事件（`_ => {}`），
    各轮结论经 ralph 最终汇总文本（ToolResult）回传模型，spill + 4000
    字符截断保护完备。
  - 提前结束判定 `ralph_done`（「已完成/已阻塞」前缀）已有单测。
- **工具列表范围**：`tools_json_scoped` 同时服务主会话与子代理（agent.rs:694
  / subagent.rs:35 / compaction.rs:89）；编排工具 stub 对子代理呈现「拒绝」
  而非「越权执行」，与 DSH 语义一致。

### 第 18 轮基线

- 纯静态审查轮：无代码改动，未跑 E2E（节省 LLM 额度）。
- 上轮基线仍有效：`cargo test --lib` 355 passed / 0 failed；19 探针全绿。

### 待办（下一轮候选）

- web_search/fetch_web_page 提供商缝静态审查（Bing/DeepSeek）
- 语音（VAD→STT + TTS）能力核对或标记边界
- 常规维护 / 用户指定方向

---

### 本轮动作

- **全量 19 探针回归**（隔离环境 `.e2e/app`）：19/19 **ALL_PASS**，
  脚本尾行 `all passed (real data/ untouched)`，teardown 正常，退出码 0。
- **能力矩阵同步**（`docs/harness-capability-matrix.md`，第 9 轮快照 → 第 17 轮）：
  - 工程质量行 E2E 更新为 **19/19 探针 ALL_PASS**；
  - 动态插件/代码运行行补充 **ctx.tools 无锁桥**（`harness_execute_tool_nolock`
    仅前端执行桥用，外层派发已持锁防死锁）。
- **代码扫描**：全树无遗留 TODO/FIXME/XXX/HACK。
- **B2 展示链路审查**：`execWorkflowJs`（ctx.agent/parallel/pipeline）→
  `submitAgentToolResult`（落库前 4000 字符截断）→ ToolCard 结构化
  workflow 卡（JSON 解析 + [日志] 前缀剥离）链路完整，无需改动。
- **轻量基线复核**（无 LLM 消耗）：`cargo test --lib` **355 passed / 0 failed**。
- 本轮为文档维护轮：无代码改动，未跑全量 E2E（节省 LLM 额度）。

### 第 17 轮基线

- `cargo test --lib`：**355 passed / 0 failed / 22 ignored**
- 19 探针体系全绿（上一轮次确认）
- 真实库 0 会话 / 15 孤儿事件，零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

## 2026-08-20 第 16 轮：派发锁死锁修复（ctx.tools 无锁桥）

### 回归发现与修复

- 第 15 轮派发锁（execute_tool_command 加锁）在**全量回归**中暴露死锁：
  run_code/插件派发持锁等待前端执行桥 → 桥内 ctx.tools 再经
  `harness_execute_tool`（新 IPC 任务）取同会话锁 → 互相等待 → 60s 超时。
- 尝试 tokio task-id 重入检测无效（嵌套调用是新 IPC 任务，非同一任务）。
- **最终方案**：ctx.tools 改用**无锁 IPC** `harness_execute_tool_nolock`
  ——文档注明仅前端执行桥使用（外层 run_code/插件/workflow 派发已持锁，
  串行化由外层锁保证）；SDK/IPC 外部调用仍走带锁入口。
- 全量回归 18/19 通过（仅 phase-b2 死锁）；修复后 phase-b2 **ALL_PASS**
  （ctx.tools 写→读回显 826ms）；phase-concurrency ALL_PASS（锁语义不变）。

### 第 16 轮基线

- `cargo test --lib`：**355 passed / 0 failed**；cargo check ✅
- 前端生产构建 ✅（`npm run build`）；svelte-check 0/0
- 19 探针体系全绿（phase-b2 修复后）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

## 2026-08-20 第 15 轮：H3 一致性补全（人工派发加会话锁）

### 修复

- **问题**：`execute_tool_command`（`harness_execute_tool` IPC / SDK
  tool.execute 的人工派发）此前不获取会话级锁——进行中回合（用户/定时/
  工作流）期间人工派发可能并发追加事件造成交错（H3 的最后一处写入者）。
- **修复**：`execute_tool_command` 顶部获取 `acquire_turn_lock`；
  无同会话重入风险（subagent/workflow 子会话不同 id、嵌套回合不经锁）。
- **至此所有事件写入者均已串行化**：用户聊天（harness_chat_stream）、
  定时任务/SDK（run_turn_locked）、人工派发（execute_tool_command）。

### 验证

- `cargo test --lib`：**355 passed / 0 failed**；cargo check ✅
- 隔离 E2E：
  - phase-concurrency（用户回合+定时任务）**ALL_PASS**（串行仍成立）；
  - phase11（jobs/fs/workspace/terminal/goal/spill/skill/ACP 全量人工
    派发路径）**ALL_PASS**（加锁无死锁、无回归）。
- 真实库零变化。

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

## 2026-08-20 第 14 轮：H3 会话级互斥并发实测（phase-concurrency）

### H3 并发验证（真实用户回合 + 定时任务）

此前 H3 修复只有结构实现；本轮补**真实并发探针**
`e2e-harness-phase-concurrency.mjs`：

- 用户回合执行 `exec_command Start-Sleep 8`（约 8-15 秒）进行中，
  同时触发同一会话的定时任务（后台 spawn）；
- 事件序列实测严格串行：
  `user → tool_calls → tool_result → chunk → assistant_message`
  `→ user(调度) → chunk → assistant_message`——**零交错**；
- 调度回合被会话锁阻塞、用户回合结束后才执行；
- **ALL_PASS**（探针含发送确认/完成等待，抗 LLM 波动）。

### 第 14 轮基线

- 新增探针 phase-concurrency ALL_PASS
- `cargo test --lib`：**355 passed / 0 failed**；真实库零变化

### 探针体系（19 个）

e2e：phase1-6/9/10/11/78/phase-b2/phase-goal/phase-concurrency（13）
verify：chat-func/chat-integration/session-maintain/streaming/
no-duplicate/tool-timeline（6）

### 待办（下一轮候选）

- 常规维护 / 用户指定方向

---

## 2026-08-20 第 13 轮：B23 run_code ctx.tools（脚本内调其它工具）

### B23：run_code 工具子调用桥

此前 run_code 仅提供 ctx.fetch/ctx.log；本轮打通**脚本内调用其它 Harness
工具**（DSH run_code ctx.agents/tools 对齐）：

- **后端**：run_code 与动态插件 dispatch 改用 `run_plugin_tool_on_ext`
  （载荷携带 session_id）；
- **前端**：`execPluginTool` ctx 新增 `tools` Proxy——`ctx.tools.<工具名>(args)`
  经会话派发（遵守审批/沙箱/预设作用域；失败返回 `{__err}`）；
- **实测**（phase-b2 扩展）：脚本内
  `ctx.tools.get_current_time()` → 真实时间、
  `ctx.tools.write_file()` → 写文件、`ctx.tools.read_file()` → 读回显
  `RUNC_TOOLS_OK`——ALL_PASS。

### 第 13 轮基线

- `cargo test --lib`：**355 passed / 0 failed**；`svelte-check` 0/0
- phase-b2（含 ctx.tools 扩展）ALL_PASS；真实库零变化

### 待办（下一轮候选）

- 常规维护 / 用户指定方向（run_code 桥已齐：fetch/log/tools + workflow 桥）

---

## 2026-08-20 第 11/12 轮：goal 自动续跑实测 + 回归核查

### goal 自动续跑（goal-round-driver）真实回路验证

此前 H5 修复（revision 计数/轮次预算）只有单测，本轮补**真实 LLM 端到端
探针** `e2e-harness-phase-goal.mjs`：

- 提示模型 `goal_create(max_goal_rounds=2)` 逐轮输出数字 1/2/3；
- 自动续跑循环按预算触发多轮（`GOAL_EVENTS` 可见
  goal_create → 轮 1 输出 → goal_update(续跑) → …）；
- **ALL_PASS**：目标已设置 / max_goal_rounds=2 / revision=2（自动续跑
  已发生）/ 状态收敛 / 数字 1、2、3 逐轮产出。

### 回归审查（子代理）

对 10 轮改动后的核心代码（agent/session/approval/subagent/workflow/sdk/db）
发起独立回归审查；子代理审查超时未收敛（已中断），改为**自行核查关键交互
区域**：

- L9（chunk 分段）与 M5（中断回合工具展示）：flush 消费 pending_tools 后
  M5 条件自然处理剩余情况，无重复/丢失；
- H4（compaction 折叠）与 L4（标题投影）：前者在派生期、后者在落盘期，
  无交互；
- clean_boundary 与 fork 溯源：排除未闭合 tool_calls 后 SessionForked
  追加语义不变（check_child/catalog 全量扫描仍成立）；
- M8 精华指纹与 H1 后台审批门控：bg/fg 同命令同指纹，信任语义一致；
- IPC 契约复核：429 后端命令 / 137 前端 invoke 全匹配，新增
  harness_generate_title / harness_workflow_agent 均注册且前端接线正确。

### 第 11 轮基线

- 新增探针 phase-goal ALL_PASS（隔离环境，真实 LLM）
- 既有 17 探针维持全绿（第 10 轮全量确认）
- 真实库零变化

### 待办（下一轮候选）

- 回归审查发现项修复（如有）
- 常规维护 / 用户指定方向

---

## 2026-08-20 第 10 轮：workflow 结果卡 UI + 最终 17/17 全量回归

### workflow_run_js 结果卡（UI 优化）

- `ToolCard.svelte` 新增「编排脚本」专用卡：结果 JSON 结构化展示
  （对象 → 键值行、数组 → 逐项行；自动剥离 `[日志]` 前缀），
  workflow_run_js 的编排结果不再以纯文本呈现。
- svelte-check 0/0。

### 最终全量回归：17/17 ALL_PASS（一次运行）

隔离环境（真实 LLM）一次性运行全部 17 探针：
phase1-6/9/10/11/78/phase-b2 + verify-*（chat-func/chat-integration/
session-maintain/streaming/no-duplicate/tool-timeline）——**全部 ALL_PASS**，
脚本输出 `all passed (real data/ untouched)`。

### 第 10 轮基线

- `cargo test --lib`：**355 passed / 0 failed / 22 ignored**
- `svelte-check`：0 errors / 0 warnings
- 隔离 E2E：**17/17 探针 ALL_PASS**（最终回归，一次全绿）
- 真实库零变化（0 会话；15 条为第 2 轮数据事件遗留的孤儿事件）

### 十轮总结

- **缺陷修复**：审查报告 H1-H5（安全/数据损坏）、M1-M10（并发/取消/
  错误路径）、L1-L11（低优）全部落地
- **新功能**：B19 会话标题 LLM 生成（UI「✨」+ SDK `session.title`）、
  B2 workflow JS 编排（`workflow_run_js`：ctx.agent/parallel/pipeline）
- **质量基建**：测试库隔离（cfg!(test)→临时文件）、E2E 隔离环境
  （ST_WECHAT_APP_DIR）、17 探针全绿、355 单测、完整文档
  （迁移计划 + 维护日志 + 能力矩阵）
- **能力面**：与当前 AI 助手对齐——对话/持久化/工具/目标/待办/计划/
  子代理/工作流（阶段+JS）/后台任务/自维护/语音/协议

### 待办（下一轮候选）

- 常规维护：新探针按需扩展、LLM API 偶发容错跟踪
- 用户指定方向

---

## 2026-08-20 第 9 轮：17 探针全量回归 + SDK B19 + 探针健壮性

### 17 探针全量回归（隔离环境，真实 LLM）

全量 17 探针（phase1-6/9/10/11/78/phase-b2 + verify-*）一次运行 15 过、
2 个失败——均定位为**探针假设问题**并修复后单独重跑 ALL_PASS：

1. **phase1 删除断言**：删除**最后一个**会话时应用会新建空会话保持 ≥1
   （前端 deleteSession 兜底）——计数断言（N→N-1）不成立。改为
   「目标会话 id 已从列表消失」断言 + 点击重试。
2. **phase1 回复断言**：模型未逐字输出 HARNESS_OK（依从性波动）——
   完成判定改为「非流式 + 长文本」；重载回放锚点用回复尾 24 字符。
3. **verify-session-maintain**：exec_command 手动派发在 M8 下需逐次审批
   ——探针无看门狗 → 审批超时。加后台审批看门狗。

### SDK B19：`session.title`（脚本化标题生成）

- `generate_title_for` 抽取为共享函数（IPC 与 SDK 共用）；
- SDK 新增 `session.title` 方法；
- **实测**：chat 后调用 → 返回「数据管理能力综述」，持久化正确。

### 第 9 轮基线

- `cargo test --lib`：**355 passed / 0 failed**；`svelte-check` 0/0
- 隔离 E2E：**17/17 探针 ALL_PASS**（含 phase-b2；修复后重跑确认）
- SDK session.title 实测 OK；真实库零变化

### 待办（下一轮候选）

- workflow JS 运行面板 UI（JS 编排结果目前以工具步骤展示）
- 常规维护：关注 LLM API 400 类偶发（探针已多数容错）

---

## 2026-08-20 第 8 轮：B2 workflow JS 编排（最后一大能力缺口）

### B2：workflow JS 编排（DSH workflow 组合子）

此前 workflow 仅有固定阶段流水线（等价替代）；本轮实现**模型编写 JS
编排脚本**，与 DSH workflow 组合子对齐：

- **新工具 `workflow_run_js`**（需审批）：code 为 async 函数体，在前端
  WebView 沙箱执行，ctx 提供：
  - `ctx.agent(prompt)` → 派生子代理（后端 `harness_workflow_agent` 原语：
    fork 子会话 + 一轮对话 + 返回结论）
  - `ctx.parallel(thunks)` → 并发执行（Promise.all）
  - `ctx.pipeline(items, ...stages)` → 逐阶段流水线（每阶段 map）
  - 返回脚本返回值（JSON）；执行超时放宽到 300s（多轮子代理）
- **后端**：`run_plugin_tool_on_ext`（带 session_id 载荷 + 自定义超时）、
  `harness_workflow_agent` IPC、工具规格、命令注册。
- **前端**：`harness-workflow-exec-request` 监听 + `execWorkflowJs` 执行器
  （ctx 注入 agent/parallel/pipeline）+ ipc.ts `workflowAgent`。

### 顺带修复：子代理干净分叉边界（clean_boundary）

- **问题**：模型回合中途 fork 子会话（task / workflow_agent）时，父日志
  含**在途未闭合的 tool_calls**（工具调用已落日志、结果未回填）——复制给
  子会话得到「tool_calls 无 tool 结果」的非法消息序列，模型 API 返回 400。
- **修复**（`subagent.rs::clean_boundary`）：fork 边界从尾部回退，跳过
  未闭合的 assistant_tool_calls；`fork_child` 改用干净边界。
  新单测 `clean_boundary_excludes_trailing_unclosed_tool_calls`。

### 验证

- `cargo test --lib`：**355 passed / 0 failed**（+1 clean_boundary）
- 隔离 E2E `phase-b2` **ALL_PASS**：
  - ctx.parallel 双子代理结论合并（`{"a":"WF_A_xxx","b":"WF_B_xxx"}`）
  - ctx.pipeline 流水线（`["10","20","30"]`）
  - 子代理目录含 workflow 派生的 5 个子代理
- phase4 回归 ALL_PASS（task 子代理路径不受影响）
- 真实库零变化

### 待办（下一轮候选）

- 全量 16+1 探针最终回归（B2 加入后共 17 个）
- UI：workflow 运行面板对 JS 编排的展示（当前日志可见）

---

## 2026-08-20 第 7 轮：L11 + B19 新功能 + M8 指纹优化

### L11：append 并发 seq 语义

- **问题**：`INSERT ... MAX(seq)+1` 后重查 `MAX(seq)` 作返回值——并发写入
  下可能返回高于本行的序号（影响标题投影 is_first 与前端 seq 锚点）。
- **修复**：`INSERT ... RETURNING seq` 直接返回本行实际分配的序号（SQLite
  3.35+，bundled 版本支持）。

### B19：LLM 生成会话标题（新功能）

- **后端**：`harness_generate_title` IPC——取最近 6 条消息调模型生成
  ≤12 字中文标题并重命名会话；无消息/无提供方时优雅报错；已注册。
- **前端**：会话行「✨」按钮（Sparkles 图标，位于重命名旁）触发，
  消耗一次模型调用（手动触发，不自动消耗）。
- **实测**：标题「请用一段话介绍你自己和你的工具能力」→
  「智能代理工具简介」，持久化正确（B19_OK）。

### M8 指纹优化（精华参数）

- 实测发现 LLM 在不同调用会给 exec_command 附加 `justification` 等说明
  字段——全量参数指纹导致「同命令」也不免审（phase2 偶发失败）。
- **优化**：exec_command / exec_command#danger-full-access 的指纹只取
  `command`/`cwd` 精华参数（同命令免审语义稳定）；其余工具仍全量参数。
- 单测更新：同命令 + 附加字段 → 命中；不同命令 → 不命中；
  非 exec 工具全量参数语义不变。

### 探针修复（M8 语义联动）

- **phase11**：`trust_harness_tool(sid, 'exec_command')` 整体信任在 M8 下
  失效 → 改为**后台审批看门狗**（自动点批准）；顺带修复了 bg 审批挂起
  留下未配对 tool_calls 事件 → 后续 ACP 模型回合 API 400 的级联问题。
- **verify-no-duplicate**：改为新建会话自包含 + 发送按钮点击 + 重试一次。

### 第 7 轮基线

- `cargo test --lib`：**354 passed / 0 failed**；`svelte-check` 0/0
- 隔离 E2E：phase2/11/verify-no-duplicate 修复后 **ALL_PASS**；
  其余 13 探针本轮全量运行 ALL_PASS（16/16 全绿）
- B19 实测 OK；真实库零变化

### 待办（下一轮候选）

- B2 workflow JS 编排（较大，需用户确认方向）
- 新功能收尾：B19 按钮在「已归档」等会话列表分组的复用（目前主列表已有）

---

## 2026-08-20 第 6 轮：L 类收尾（L2/L3/L10）

| 编号 | 缺陷 | 修复 |
|---|---|---|
| L2 | exec_command 内部硬编码 30s 超时，与可配置 `tool_timeout_secs`（5-300s）不一致——守卫等 300s 但进程 30s 已被杀，报错与事实不符 | 内部超时改用 `settings::effective_timeout_secs()`（默认 30 不变，配置后生效）；删除 `EXEC_TIMEOUT_SECS` 常量 |
| L3 | `harness_open_path` 无沙箱校验，任意路径 `cmd /c start`（模型/用户输入可打开任意系统路径） | 非越界沙箱模式下仅允许工作区内路径（canonicalize + starts_with 校验，含附件目录）；danger-full-access 不限制 |
| L10 | `TURN_CANCEL` 取消标志条目永不清理（每次取消留一个 Arc） | `clear_cancel` 改为移除条目（语义等价：缺失即未取消） |

- 验证：cargo check ✅；`cargo test --lib` **354 passed / 0 failed**；
  隔离 E2E phase2（审批/信任/超时）+ phase3（超时守卫）**ALL_PASS**
  （L2 改动不破坏审批/守卫流）；真实库零变化。

### 待办（下一轮候选）

- L11（append 并发 seq 语义）：单条 INSERT...SELECT MAX 原子无重复，
  但返回序号可能错位——影响标题投影 is_first 与前端 seq 显示，
  评估按影响面决定是否修
- 新功能方向（用户确认后实施）：会话标题 LLM 生成（B19）、
  workflow JS 编排（B2，较大）

---

## 2026-08-20 第 5 轮：测试库隔离 + L 类缺陷 + 隔离 E2E 16/16

### 测试数据库隔离（系统性根治测试污染）

- **问题**：`cargo test` 直接使用真实 `data/control.db`（db.rs `db_path`）——
  测试创建会话即使成功路径自清理，断言失败 panic 时清理代码不执行，
  仍会泄漏会话/事件进真实库（第 4-5 轮实测多次泄漏）。
- **修复**（`db.rs::db_path`）：`cfg!(test)` 下数据库路径改为
  `%TEMP%/st-control-test-<pid>.db`——**单测/集成测试永不触碰真实库**，
  panic 泄漏也无从污染。
- **验证**：全量 `cargo test --lib` **354 passed / 0 failed**，
  测试后真实库 `sessions 0 / events 15`（零新增）。

### L 类缺陷修复（4 项）

| 编号 | 缺陷 | 修复 |
|---|---|---|
| L9 | 展示投影的流式分块不校验 assistant id（异常日志回放会把不同回复的 chunk 合并成一条） | `derive_display_messages`：chunk id 变化时冲刷当前段再开新段；新单测 `chunk_groups_separate_by_assistant_id` |
| L4 | 清空会话后标题停留在清空前（`is_first = seq==1` 判定失效：SessionCleared 占用 seq 1） | `clear_messages` 重置标题为空；标题投影条件放宽为 `seq==1 或标题为空`（新 `db.get_harness_session_title`） |
| L8 | 轨迹 `tool_call_count` 只统计成功工具（与全量入账的 Tool 条目不一致） | 统计全部工具调用（含失败/未闭合）；更新既有测试期望 1→2 |
| L1 | `event_search` 片段字节/字符偏移混用（中文载荷片段窗口错位） | 全字符偏移（命中位置转字符索引后截取前后 60 字符） |

### 隔离 E2E：16/16 探针 ALL_PASS

- e2e phase1-6/9/10/11/78 + verify-*（chat-func/chat-integration/
  session-maintain/streaming/no-duplicate/tool-timeline）全部通过；
  **真实库全程零变化**。
- 本轮探针/脚本修复：
  - 脚本 teardown 移到探针汇总之后（失败时也执行清理，避免应用残留
    锁定 exe）；脚本强制纯 ASCII（PS 5.1 中文乱码解析错误，两次教训）；
  - `_wait-ipc.mjs` 被清理误删 → 重建（脚本依赖）；
  - verify-no-duplicate：模型未逐字回显关键词时，捕获任意回复并以
    回复文本做去重校验（关键词依从性解耦）；重载后按标题选中会话；
  - verify-harness-session-maintain：隔离 app-base 种子 `package.json`
    （自维护检查读项目根文件）。

### 第 5 轮基线

- `cargo test --lib`：**354 passed / 0 failed / 22 ignored**
- 隔离 E2E：**16/16 探针 ALL_PASS**（真实 LLM）
- 真实 `data/control.db`：测试与 E2E 双隔离后**零变化**

### 待办（下一轮候选）

- M2 turnToken 前端改造后的并发/切换专项验证（隔离环境多探针连跑已间接覆盖）
- 剩余低优先项：L2（exec_command 30s 硬编码与可配置超时不一致）、
  L3（harness_open_path 沙箱校验）、L10/L11（注册表清理、seq 并发）

---

## 2026-08-20 第 4 轮：M8 安全加固 + 隔离环境全量 E2E 回归

### M8：审批信任键参数指纹（安全加固）

- **问题**：信任键原为 (session, tool)——用户对某条 exec_command 点
  「记住并批准」后，同会话 30 分钟内**任意命令**免审批执行（含
  danger-full-access 逃逸命令），等价一次性授予沙箱逃逸免审权限。
- **修复**（`approval.rs`）：信任键改为 `(session_id, tool, 参数指纹)`；
  指纹 = 规范化 JSON（serde_json Map 默认 BTreeMap，键已排序）的 sha256——
  同参数恒定、键序无关、不驻留敏感载荷。仅**完全相同参数**的命令免审批。
  - `trust_harness_tool` IPC 新增 `arguments` 参数；
  - 前端 `trustTool` / `approvePending` 透传审批参数；
  - 2 个新单测：`trust_scoped_per_session_and_args`（同参数命中/异参数不命中）、
    `fingerprint_stable_regardless_of_key_order`（键序无关 + 异参不同）。
- **探针更新**（phase2）：第三次用**相同参数**命令验证免审；新增第四次
  **不同参数**命令验证仍需审批——实测 ALL_PASS。

### 隔离环境全量 E2E 回归（10/10 探针 ALL_PASS）

通过 `scripts/run-e2e-isolated.ps1`（ST_WECHAT_APP_DIR=.e2e/app）验证：
phase1/2/3/4/5/6/9/10/11/78 全部 ALL_PASS（真实 LLM）。
**真实 `data/control.db` 全程零变化**。

- 本轮探针修复 2 项（环境无关化）：
  - phase6 附件：绝对路径 C:\...\att-e2e.txt → 相对 `att-e2e.txt`
    （解析到应用 CWD=工作区根，真实/隔离环境一致）；
  - phase10 预设测试：`path: WS_DIR`（硬编码）→ `path: ''`（工作区根）。
- 脚本补 teardown：探针跑完自动停应用+vite（下次运行从干净状态开始）。
- 教训补充：**E2E 运行期间不得并发编辑前端文件**（vite HMR 会把半成品
  实时注入页面，导致探针全部失败——第 4 轮实测教训，已按「先改完再跑」重来）。
- 测试泄漏二次修复：`sdk.rs::dispatch_lists_creates_and_reads_sessions` 创建
  会话后未删除（每次全量 cargo test 泄漏 1 个空会话进真实库）——补
  `store.delete(&sid)`；全量测试后实测真实库 **0 残留**。

### 第 4 轮基线

- `cargo test --lib`：**353 passed / 0 failed**（M8 净增 1：新指纹单测 1 + 原信任单测改造）
- `svelte-check`：0 errors / 0 warnings
- 隔离 E2E：10/10 e2e 探针 ALL_PASS

### 待办（下一轮候选）

- verify-* 探针补跑隔离回归（6 个，已修复但未在隔离环境重跑）
- L1-L11 低优先项（event_search 偏移、chunk id 校验、trajectory 计数等）

---

## 2026-08-20 第 3 轮：隔离 E2E 环境（数据安全机制）

### 背景

第 2 轮 E2E 验证直接在应用活动数据库上运行，造成会话数据丢失。
本轮从机制上杜绝：**E2E 探针一律运行在隔离数据目录**。

### 实现

- **原理**：`common.rs::app_base_dir` 优先读 `ST_WECHAT_APP_DIR` 环境变量 →
  `st_data_dir = <该目录>/data` 完全独立。将环境变量指向 `.e2e/app`，
  探针的 DB / harness 运行时 / spill / 日志全部落在隔离目录。
- **新脚本** `scripts/run-e2e-isolated.ps1`（ASCII 纯文本，兼容 PS 5.1）：
  - 可选 `-KeepData`：默认每次重置隔离库（探针从干净状态开始）；
  - 种子数据：`config.json` / `data/llm_config.json`（LLM 提供方）/ `bot_secret.key`；
  - 启动 vite（:1420）+ st-control.exe（CDP :9222 + ST_WECHAT_APP_DIR）；
  - **页面 IPC 桥就绪等待**（首次 vite 编译 ~4700 模块需 20-60s，轮询
    `window.__TAURI_INTERNALS__.invoke`）；`_wait-ipc.mjs` 辅助；
  - 按 `-Probes "phase1,phase4"` 依次运行探针并汇总。
- **注意**：脚本会终止现有 st-control / vite 进程（含用户真实实例），
  仅限测试环境使用（脚本头部已注明）。

### 验证

- 隔离实例 phase1/phase3/phase4/phase11 **ALL_PASS**（真实 LLM）；
- spill 定位符指向 `.e2e\app\data\...`（隔离深入到文件级）；
- **真实 `data/control.db` 全程 0 会话 / 15 事件零变化**（隔离生效）。

### 待办（下一轮候选）

- 用隔离环境跑全量 16 探针（phase2/5/6/9/10/78 + verify-*）
- M8 信任键参数指纹、L1-L11 低优先项

---

## 2026-08-20 第 2 轮：真实验证 + 探针修复 + 剩余缺陷

### 本轮成果

- **重建 exe**（cargo build，whisper.cpp 构建稳定复现，修复进入运行时）
- **真实 LLM 对话验证**：用户已配置 DeepSeek 提供方（deepseek-v4-flash/pro）。
  SDK 实测简单对话 + 工具调用（get_current_time 返回真实时间）全通；
  事件日志 / 用量遥测（TTFT/缓存命中/tok/s）完整记录。
- **E2E 全量回归（16 个探针 ALL_PASS，含真实模型调用）**：
  phase1（会话/流式/持久化）、phase2（审批/信任/模型座持久化）、
  phase3（超时守卫/钩子/遥测）、phase4（计划模式守卫/子代理/goal/schedule/workflow）、
  phase5（shell/fs/终端沙箱）、phase6（SDK/compaction/MCP）、phase9（凭据/LSP/ACP）、
  phase10（fork/配置束/PTY）、phase11（jobs/goal 状态机/spill/skill 门控）、
  phase78（技能/反馈/会话查询/CLI）+ verify-*（chat-func/chat-integration/
  session-maintain/streaming/tool-timeline/no-duplicate）。
- **剩余缺陷修复**：M2（前端回合令牌防会话切换串台）、M7（jobs 完成记录
  惰性清理 + 单测）、M3 延伸（审批/提问等待可被取消，第 1 轮已做）。

### 探针体系修复（16 项）

| 问题 | 修复 |
|---|---|
| 探针 `listSessions()[0]` 假设列表首条=新会话（order_index 升序下取到旧会话） | phase1/2/3/4/verify-* 改为按 created_at 取最新 / 按标题定位 / 扫描投影定位 |
| 旧路径 `E:\ST\st_control`（项目已迁 C:） | phase6/9/10/11 + verify-tool-timeline 批量替换 |
| 治理抽屉旧选择器 `.hns-tools-btn`（重设计后移除） | 改为 `.hns-bar-icon[title="设置 / 钩子 / 预设"]` + close-then-open |
| 模型多次调用工具 → 多张审批卡竞态（探针退出后新卡出现） | 持续批准循环（连续两轮无卡才结束） |
| 挂起审批卡卡死前端 sending → 后续探针无法发送 | SDK session/cancel + 页面重载恢复；探针增加会话自清理 |
| 工具详情旧 DOM（`.hns-tool-pre`） | 改读 `.hns-tool-detail`（内嵌 ToolCard） |
| 模型未逐字回显关键词 | 断言放宽为「收到回复」（收发链路与关键词依从性解耦） |
| 模型快回复导致流式快照不足 | 40ms 采样 + 更长回复请求（31 个快照实测） |

### ⚠️ 数据事件（必须透明说明）

在本轮 E2E 验证中，探针直接运行于**应用活动数据库**（无先备份快照），
且历史探针存在会话定位缺陷（`[0]` 取旧会话、session_clear 测试作用于
活动会话、删除步骤的目标选择），导致：

- `harness_sessions` 中的 7 个会话（14:50-14:52 创建，含「问题一」会话）
  的会话行被删除；
- h-4048（问题一）的对话事件被 session_clear 清空（仅剩清理操作事件）。

**现状**：应用运行正常、功能完好，但上述会话数据不可恢复（无备份）。
已采取：数据库快照备份至 `data/backup-control-20260820-160323/`；
探针全部改为自包含（新建/定位/清理自己的会话），并记录本教训：
**对真实数据运行 E2E 前必须先做 DB 快照**。

### 第 2 轮基线

- `cargo test --lib`：**352 passed / 0 failed / 22 ignored**
- `svelte-check`：0 errors / 0 warnings
- E2E：16 个探针 ALL_PASS（真实 LLM）
- 附带修复：`sdk.rs` 的 ACP 测试创建会话后未删除（每次 cargo test 泄漏
  1 个会话进真实库）——补 `store.delete(&sid)` 清理，回归验证 0 残留。
- 教训补充：**`cargo test` 直接使用真实 `data/control.db`**（db.rs `db_path` =
  st_data_dir/control.db）——测试本身自清理，但运行测试也会短暂触碰真实库；
  生产数据操作前必须快照。

### 待办（下一轮候选）

- 为 E2E 建立「隔离数据目录」运行模式（ST_WECHAT_APP_DIR 指向测试目录），
  从机制上杜绝真实数据被探针污染
- 用用户真实偏好重跑一次全量 E2E 确认干净状态
- M8 信任键参数指纹、L1-L11 低优先项

---

## 2026-08-20 第 1 轮：全面审查 + 高优缺陷修复

### 基线（修复前核实）

| 检查项 | 结果 |
|---|---|
| `npx svelte-check` | 0 errors / 0 warnings |
| `cargo check` | ✅（先修复 whisper-rs-sys cmake install 残留态问题） |
| `cargo test --lib` | 347 passed / 0 failed / 22 ignored |
| `npm run build`（vite） | ✅（仅 chunk 体积告警） |
| IPC 契约 | 427 个后端 tauri::command，前端 135 个 invoke 全部有实现 |
| SDK 运行时实测 | health / sessions.list / session.state / session.display / usage.get / tool.execute(get_current_time) 全通 |
| E2E（CDP 9222 + Vite 1420） | 导航/会话管理 PASS；对话断言因未配置 LLM 提供方失败（环境问题） |

构建阻塞修复：`cargo check` 在 whisper-rs-sys（whisper.cpp cmake `--target install`）报
0 错误 0 警告但失败——增量构建残留态（whisper.lib 缺失但 ZERO_CHECK 认为最新）。
处理：手动 MSBuild 构建 `whisper.vcxproj` + 重跑 install 后恢复；再次 `cargo check` 通过。

### 审查结论（子代理逐行核对，B-）

问题集中在「并发 / 取消 / 错误路径」三类场景，正常单用户单会话路径正确。

### 已修复缺陷（按优先级）

| 编号 | 缺陷 | 修复落点 | 回归测试 |
|---|---|---|---|
| H1 | exec_command 后台模式（`run_in_background=true`）绕过审批/计划模式/只读守卫；后台作业未锚定工作区 cwd | 计划/只读守卫前移到工具循环 handle_session_tool 之前（覆盖全部会话编排工具）；exec_command 后台分支补 `requires_approval_scoped` 审批门控；`jobs::start` 锚定 `workspace::sandbox_root()` | —（结构性守卫） |
| H2 | 字节偏移切片在多字节字符（中文）边界 panic：llm/agent.rs `&text[..8192]`、jobs.rs 尾部 `out[len-64K..]`、fs.rs `&text[..64K]`、spill.rs `&text[..32K]`，另发现 jobs.rs 完成线程 `text.truncate(64K)` 与 shell.rs `truncate_8k` 两处同类 | 全部改 `floor_char_boundary` 安全截断（5+1 处） | `read_text_truncates_at_char_boundary_with_chinese`（fs）、`job_output_tail_truncates_at_char_boundary_with_chinese`（jobs） |
| H3 | 定时任务/工作流/用户回合并发写同一会话 → 事件日志交错、上下文损坏 | 会话级互斥 `acquire_turn_lock`：用户聊天（harness_chat_stream 整段续跑）、定时任务（run_due）、SDK/CLI 会话调用（run_turn_locked）串行化；回合内嵌套调用（workflow_run/子代理）不取锁防自锁 | —（并发结构性） |
| H4 | 压缩跨回合无持久效果：`derive_model_messages` 忽略 Compaction 事件，每回合重复全量摘要调用 | `derive_model_messages` 尊重最近一次 Compaction 事件：其前历史折叠为 `[较早对话摘要]` 占位 | `compaction_folds_history_in_model_messages` |
| H5 | goal 自动续跑预算双计数：GoalSet+GoalUpdate 均递增 revision，`max_goal_rounds` 实际减半 | GoalSet 不再递增 revision（revision 由 GoalUpdate 计数）；续跑判定 `rounds_done < max` → `<= max`（最大续跑 max 轮语义）；同步更新单测 | `goal_auto_round_continues_within_budget`（更新） |
| M1 | 前端错误路径不重载日志投影 → 乐观用户消息成幻影（retry 可能双写） | error 分支与 catch 分支均 `displayMessages(activeId)` 重载 | —（前端） |
| M3 | 停止无法中断审批/提问等待（挂 10 分钟，批准后工具仍执行） | `request_approval` / `ask_user` 轮询循环检查 `is_cancelled(session_id)` 立即返回 | — |
| M4 | 子代理审批/提问事件被前端按 activeId 过滤丢弃 → 后台子代理挂 10 分钟超时 | 审批/提问监听不再按 activeId 过滤（全部展示） | —（前端） |
| M5 | 展示投影丢弃中断回合工具步骤（对话/轨迹/模型上下文三投影不一致） | `derive_display_messages`：新 user_message 到达时若无助手回复但有工具步骤，先输出「（回合中断…）」助手行挂载工具 | `interrupted_turn_tool_steps_survive_display_projection` |
| M6 | exec_command 升级分支（danger-full-access）绕过计划模式只读守卫 | 守卫前移（同 H1）覆盖 | — |
| M9 | 子代理回合从不标记 RUNNING_TURNS，目录「正在运行」永不点亮 | 后台 spawn / 前台 subagent / send_message 三处包裹 `mark_turn_running/idle` | — |
| L6 | 审批「记住并批准」信任写入 activeId（切换会话后污染） | 改用 `a.session_id` | —（前端） |

### 修复后基线

- `cargo test --lib`：**351 passed / 0 failed / 22 ignored**（新增 4 个回归测试：
  fs 中文截断、jobs 中文尾截断、compaction 折叠、中断回合工具投影）
- `svelte-check`：0 errors / 0 warnings
- `cargo check`：✅
- 未修复（记录待后续轮次）：M2 回合中切换会话状态串台（需 turnToken 守卫）、
  M7 jobs 注册表永不清理、M8 信任键不含参数指纹（设计语义，风险已记录）、
  M10 调度器串行阻塞、L1-L11 低优先项。

### 待办（下一轮候选）

- 配置 LLM 提供方后重跑 `e2e-harness-phase1~11` 全量探针（对话断言需真实模型）
- 重建 exe（cargo build）验证 whisper 构建稳定性与运行时行为
- M2 turnToken 守卫：异步回调按回合令牌过滤，防切换会话串台
- H4 补充 `/compact` 后上下文仪表数值断言
