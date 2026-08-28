# 知识库系统全面审计报告

**审计日期**：2025-07  
**审计范围**：st_control 知识库全栈（前端 + 后端 + 数据库 + 测试）  
**审计方法**：逐文件静态代码审查（只读），不修改任何代码  

---

## 一、文档接入

| 项目 | 现状 | 问题描述 | 严重度 |
|------|------|----------|--------|
| 支持格式 | ✅完整 | 支持 txt/md/csv/json/log + 代码文件（py/js/ts/rs/go/java/c/cpp/rb/sh/sql 等 20+ 种）+ docx/xlsx/pdf + 旧版 Office (doc/ppt/xls) + ODF + RTF + EPUB + 图片 (png/jpg/gif/webp/bmp)，覆盖全面 | - |
| 上传方式 | ✅完整 | 支持文件上传（data 数组或 base64）、网页抓取（URL → Markdown）、手动创建文档。批量网页抓取也已实现 | - |
| 去重 | ✅完整 | 文件按内容哈希去重（`file_objects` 表 `UNIQUE(hash)`），同知识库内相同内容文档也按 hash 检测重复跳过 | - |
| 大小限制 | ✅完整 | 单文件上传上限 200MB（`MAX_UPLOAD_SIZE`），全局存储配额 2GB（`KB_STORAGE_QUOTA`），网页抓取响应体限 10MB | - |
| 格式嗅探 | ✅完整 | `sniff_format_magic()` 对二进制格式（PDF/Office/图片）做魔数校验，声明类型与内容不符时尽早拒绝 | - |
| SSRF 防护 | ✅完整 | 网页抓取时拒绝内网/保留地址（含 IPv6 link-local、云厂商 metadata 100.100.x.x），DNS rebinding 防护 | - |
| 缺少 web 端 URL 自动标题提取质量优化 | ⚠️部分 | `extract_web_text()` 为简易 HTML 解析器，对 SPA / React 渲染页面无法提取正文（仅去除标签）；未集成 readability 算法 | P2 |

---

## 二、解析与分块

| 项目 | 现状 | 问题描述 | 严重度 |
|------|------|----------|--------|
| 解析引擎 | ✅完整 | txt/md/csv 原生 UTF-8 解析；docx/xlsx 独立解析器 + anydoc（PDF/Office/ODF/RTF/EPUB，MIT）回退链；PDF 有 OCR 回退 | - |
| 损坏文件防御 | ✅完整 | `sniff_format_magic()` 魔数校验；zip 容器在解压前按中央目录预检（`guard_zip_bomb()`），拦截压缩炸弹 | - |
| Zip 炸弹防御 | ✅完整 | 三层防护：单条目 ≤64MB、条目数 ≤4096、总解压 ≤256MB；`read_zip_entry_text()` 用 `take()` 限制读取量 | - |
| 超长文本 | ⚠️部分 | 文本文件用 `String::from_utf8_lossy()` 全量加载到内存，无截断保护；大 txt 文件（如日志 >100MB）可能 OOM | P2 |
| 分块策略 | ✅完整 | 三种策略：recursive（递归字符分片，按段落/句子边界 + 重叠窗口）、title（标题感知）、parent_child（父子分块：父块粗粒度回答，子块细粒度检索） | - |
| 分块配置 | ✅完整 | 全局可配（`kb_chunk_settings` 表：strategy/size/overlap），上传时可覆盖；overlap 上限为 chunk_size/2 | - |
| 中文切分 | ✅完整 | `cjk_spaced()` 在连续汉字间插入空格，使 FTS5 unicode61 按单字建 token；查询端统一 `fts_safe_query()` 做同样处理 | - |
| FTS 中文整句查询 | ✅完整 | 短中文词（≤4字）短语匹配，长句按单字 OR 展开（避免 0 命中）；中文标点过滤 | - |

---

## 三、向量化与索引

| 项目 | 现状 | 问题描述 | 严重度 |
|------|------|----------|--------|
| 嵌入模型配置 | ✅完整 | `kb_model_settings` 表按角色（embedding/inference/parsing/rerank/multimodal）存配置；前端设置页可选提供方 + 模型 | - |
| 维度一致性 | ✅完整 | `ensure_embedding_compatible()` 校验当前模型与知识库已记录模型是否一致，不一致则拒绝；`record_embedding_meta()` 维度变化时告警而非覆盖 | - |
| 非嵌入模型拦截 | ✅完整 | `is_definitely_not_embedding_in()` 检查 model_meta 标记，被标记为「对话」的模型不会被误用为嵌入模型 | - |
| 向量存储 | ✅完整 | BLOB 存储（f32 小端序列化），`serialize_embedding()`/`deserialize_embedding()` 实现；SQLite BLOB 方案适合 SME 单机场景 | - |
| 索引增量更新 | ✅完整 | `vector_index.rs` 内存缓存（generation 计数器方案），文档变更时 `invalidate()` 递增 generation，下次检索自动重新加载；支持增量刷新 | - |
| 向量缓存容量 | ✅完整 | 内存估算：10K chunks × 768 dim ≈ 30MB，50K ≈ 150MB；桌面场景合理 | - |
| 删除/重建 | ✅完整 | 文档删除时清理 FTS 索引 + chunks；重处理时先删旧分片再建新分片；FTS 重建函数 `rebuild_fts_indexes()` 存在 | - |
| 大库保护 | ✅完整 | `vector_search_capped()` 超过阈值（默认 500，可配 `vector_scan_cap`）时自动走 FTS 候选池预筛 + 向量精排 | - |
| 检索 LRU 缓存 | ✅完整 | 双层缓存：retrieval.rs 的 200 条/60s TTL + vector_index.rs 的 100 条/30s TTL；缓存 key 含嵌入模型标识 | - |

---

## 四、检索

| 项目 | 现状 | 问题描述 | 严重度 |
|------|------|----------|--------|
| BM25 检索 | ✅完整 | FTS5 + bm25() 排序；CJK 预处理（单字插入空格）；支持中文标点过滤 | - |
| 向量检索 | ✅完整 | Rust 侧余弦相似度计算；大库自动 FTS 预筛；内存缓存索引加速 | - |
| 混合检索 (RRF) | ✅完整 | `rrf_fuse()` 实现 Reciprocal Rank Fusion（k=60），BM25 + 向量两路结果融合 | - |
| Rerank 重排序 | ✅完整 | 支持配置 rerank 模型（`kb_model_settings` role='rerank'），对检索结果按相关性重排；失败时保持原顺序 | - |
| 中文检索 | ✅完整 | `fts_safe_query()` 处理：短中文词短语匹配、长句 OR 展开、标点过滤、混合中英文支持 | - |
| 分页 | ⚠️部分 | 检索结果返回全部 top_k 条，无游标分页；大结果集前端需一次性渲染 | P2 |
| 权限过滤 | ✅完整 | `visible_kb_ids()` 综合成员关系 + ACL allow/deny；`can_access_doc()` 文档级 ACL；deny 优先 | - |
| 缓存 | ✅完整 | LRU 缓存（200 条/60s TTL），key 含查询+模式+topK+嵌入模型 | - |
| 检索日志 | ✅完整 | `search_logs` 表记录每次检索（kb_id/user_id/query/mode/hit_count/created_at） | - |
| 混合检索降级 | ✅完整 | 向量检索不可用时自动降级为纯 BM25，保证检索按钮始终可用 | - |

---

## 五、RAG 问答

| 项目 | 现状 | 问题描述 | 严重度 |
|------|------|----------|--------|
| 上下文组装 | ✅完整 | `rag_context()` 检索 → 组装上下文 + 文档标题 + 章节路径；父子分块增强：命中子块时附加父块内容（≤3倍长度） | - |
| 引用定位/高亮 | ✅完整 | `highlight()` 函数按查询关键词切分文本为高亮片段；前端可直接调用或后端 `kb_highlight` 计算 | - |
| 流式输出 | ✅完整 | `kb_rag_stream` 通过 Tauri Channel 逐段推送 delta/done/error 帧；`kb_rag_cancel` 精准取消（序列号方案） | - |
| 会话 | ✅完整 | `qa_sessions`/`qa_messages` 表持久化问答历史；`load_conversation_history()` 加载最近 5 轮对话（≤4000 字符） | - |
| 系统提示词 | ✅完整 | 可自定义（`kb_chunk_settings` key='rag_system_prompt'），默认提示词含"严格基于知识上下文"约束 | - |
| 测试模型 | ✅完整 | `kb_test_model` 支持对话/嵌入/重排序三类模型连通性测试，含详细错误分类（模型不存在/密钥无效/频率超限） | - |
| FAQ 优先 | ✅完整 | 检索前先匹配 FAQ 问答对，命中直接给出标准答案，不走 RAG 流程；支持导入/删除/列表 | - |
| 多轮记忆 | ✅完整 | `persist_qa_exchange()` 每次问答自动持久化；自动用问题前 24 字作为会话标题 | - |

---

## 六、Wiki

| 项目 | 现状 | 问题描述 | 严重度 |
|------|------|----------|--------|
| 页面 CRUD | ✅完整 | 创建/更新/删除/列表/搜索/详情；创建后自动后台提取摘要与实体 | - |
| slug 唯一性 | ✅完整 | `UNIQUE(kb_id, slug)` 约束；创建时 `ensure_unique_slug()` 自动追加数字后缀 | - |
| 链接图/知识图谱 | ✅完整 | `wiki_links` 表存储页面间链接（related/reference/child_of/generated）；`kb_wiki_graph` 返回节点+边供前端可视化 | - |
| LLM 提炼 | ✅完整 | `extract_page_meta()` 从正文提取摘要与实体（LLM 调用）；`generate_with_jobs()` 从文档批量生成 Wiki 页面 | - |
| 版本控制 | ✅完整 | `wiki_page_versions` 表记录历史版本；`list_versions()`/`restore_version()` 实现版本回滚 | - |
| FTS | ✅完整 | `wiki_pages_fts` 虚拟表索引 title/summary/content_md；写入时统一 `cjk_spaced()` 预处理 | - |
| Wiki 目录 | ✅完整 | `kb_wiki_dirs` 返回含页面数的目录树；`dir_subtree_counts()` 递归计算子孙目录页面总数 | - |
| 批量提炼 | ✅完整 | `kb_wiki_extract_all` 支持 force 模式重置全量；批量取消标记 `generate_cancel_{kb_id}` | - |
| 源文档联动 | ✅完整 | 文档重新处理/版本回滚后自动 `refresh_wiki_for_doc()` 刷新关联 Wiki 页面摘要/实体 | - |

---

## 七、权限与安全

| 项目 | 现状 | 问题描述 | 严重度 |
|------|------|----------|--------|
| RBAC | ✅完整 | 四级角色：owner/admin/editor/viewer；`require_kb_role()` 统一校验；全局 admin 角色可管理任意 kb | - |
| ACL | ✅完整 | 对象级 ACL（document/folder/kb scope）；支持 user/role/public 三种受让类型；deny 优先；`kb_set_acl`/`kb_acl_delete` 分离 | - |
| 登录鉴权 | ✅完整 | bcrypt 密码哈希（cost=10）；空哈希拒绝放行；会话持久化到 `kb_session.json`（7 天过期）；大小写不敏感匹配 | - |
| 频率限制 | ✅完整 | `LoginRateLimiter`：5 次失败后锁定 5 分钟 | - |
| 审计日志 | ✅完整 | `kb_audit_log` 表记录 login/logout/create_kb/delete_kb/backup 等关键操作；`kb_list_audit_logs` 查询 | - |
| 敏感数据 | ✅完整 | 密码仅存 bcrypt 哈希；会话文件不含明文密码；审计日志不记录密码 | - |
| 开放知识库 | ✅完整 | 无成员的知识库默认全员可见，但显式 deny 优先于"开放可见"规则 | - |
| owner 保护 | ✅完整 | 不允许移除/降级 owner；仅 owner 可分配 owner 角色 | - |
| SSRF 防护 | ✅完整 | 网页抓取拒绝内网 IP、保留地址、DNS rebinding | - |

---

## 八、运维

| 项目 | 现状 | 问题描述 | 严重度 |
|------|------|----------|--------|
| 任务队列 | ✅完整 | `processing_jobs` 表异步流水线（pending → parsing → chunking → embedding → done/failed）；前端 `kb_list_jobs` 轮询 | - |
| 进度/状态 | ✅完整 | `process_status` 字段细分到每阶段；`processing_logs` 记录详细日志；前端事件推送 `kb:doc-processed` | - |
| 重试 | ✅完整 | `kb_retry_job` 单个重试 + `kb_retry_failed_jobs` 批量重试；重置任务状态后重新走处理流水线 | - |
| 停止 | ✅完整 | `kb_stop_processing` 标记进行中任务为 failed + 置位批量取消标记 + 文档复位为 ready | - |
| 备份/恢复 | ✅完整 | `VACUUM INTO` 在线备份（不阻塞读写）；`restore_from_backup()` 恢复（含恢复前自动备份当前库）；`cleanup_backups()` 保留最近 N 个 | - |
| 导出/导入 | ✅完整 | `kb_export` 导出为 JSON（含元数据+文档+分片+Wiki+FAQ+链接，文件 base64 编码）；`kb_import` 恢复（含 id 映射） | - |
| 清理 (housekeeping) | ✅完整 | `kb_housekeeping` 清理孤儿 file_objects + 过期处理任务；`kb_clear_activity` 清理 jobs/logs/history | - |
| 统计指标 | ✅完整 | `kb_metric_events` 表埋点（search/rag/faq_hit/citation_click/doc_view/wiki_view 等 12 种事件）；`kb_get_analytics` 聚合统计 | - |
| 存储配额 | ✅完整 | 2GB 配额，上传前检查已用空间 | - |

---

## 九、前端 UI 完整性

| 项目 | 现状 | 问题描述 | 严重度 |
|------|------|----------|--------|
| 组件覆盖 | ✅完整 | KnowledgeBase/KbDocs/KbDocDetail/KbChat/WikiPanel/KbSettings/KbActivity/KbAuditLog/KbFaq/KbAcl/KbMembers/KbUserManagement/KbDashboard/ResourcePreview/KbDocUploadPanel/DirTree/WikiGraphCanvas/KbLogin/KbHelp/KbSelect/KbModal/KbConfirm/KbErrorBoundary/KbTrendChart/KbIcon 共 25+ 组件 | - |
| IPC 接入 | ✅完整 | `services/ipc.ts` 封装 90+ 个 IPC 调用，覆盖后端全部命令；`kbApi.invoke()` 通用兜底 | - |
| 错误边界 | ✅完整 | `KbErrorBoundary.svelte` 组件级错误捕获；每个异步调用都有 try/catch + 错误提示 | - |
| 空状态 | ✅完整 | 知识库列表为空、文档列表为空、搜索无结果等场景均有空状态展示 | - |
| 加载态 | ✅完整 | 搜索/上传/处理等异步操作有 loading 状态；`$state` 响应式管理 | - |
| 全局搜索 | ✅完整 | `GlobalSearch.svelte` 统一搜索入口，支持微信消息/知识库/通讯录/平台事件四域搜索 | - |
| 流式问答 UI | ✅完整 | `KbChat` 组件支持流式渲染 + 停止生成按钮 + 引用展示 + 高亮 | - |
| i18n | ⚠️部分 | 中文硬编码较多（错误信息、按钮文本、提示语），未接入 i18n 框架；但项目本身面向中文用户，影响有限 | P2 |

---

## 十、代码质量

| 项目 | 现状 | 问题描述 | 严重度 |
|------|------|----------|--------|
| IPC 前后端一致性 | ⚠️部分 | 后端注册了 `kb_rag`（同步 RAG）但前端 kbApi 仅封装 `kb_rag_stream`（流式），同步接口只能通过 `kbApi.invoke()` 通用调用 | P2 |
| IPC 前后端一致性 | ⚠️部分 | 后端注册了 `kb_highlight` 但前端 kbApi 未封装（组件内直接调用）；后端注册了 `kb_create_user`/`kb_change_password`/`kb_delete_user`/`kb_reset_password`/`kb_set_admin` 但前端 kbApi 未显式封装 | P2 |
| IPC 后端重复注册 | ⚠️部分 | `kb_fetch_url` 在 lib.rs 中注册了两次（line 664 和 702） | P1 |
| 未使用的死代码 | ⚠️部分 | `db.rs` 中 `CURRENT_USER` 常量（line 496）已被 `UserSession` 替代但仍存在；`vector_index.rs` 中 `generation()` 方法标记 `#[allow(dead_code)]` | P2 |
| Rust `#[cfg(test)]` 重复 | ⚠️部分 | `parse/mod.rs` line 408-409 连续两个 `#[cfg(test)]`，虽不影响编译但是代码冗余 | P2 |
| FTS 表非外部内容表 | ✅完整 | 注释明确说明了旧版外部内容表的问题，已迁移为普通 FTS5 表；`rebuild_fts_indexes()` 重建函数完备 | - |
| 连接池 | ✅完整 | r2d2 连接池（max 8，timeout 5s）替代单 Mutex；`try_conn_lock()` 非阻塞获取用于埋点 | - |
| 事务安全 | ✅完整 | `save_chunks()`/`delete_kb_clean()` 等写操作使用事务包裹；`delete_kb_clean()` 逐表显式清理 + 序列复位 | - |
| 向量序列化精度 | ⚠️部分 | f64 → f32 序列化有精度损失（约 1e-6），对 768 维向量的余弦相似度影响可忽略，但应在文档中注明 | P2 |
| 测试覆盖 | ❌缺失 | `.codex_tests/` 目录不存在（AGENTS.md 中提到的 smoke-kb-*.mjs、smoke-dir-tree.mjs、smoke-wiki-markdown.mjs、smoke-search-text.mjs 等文件均缺失） | P0 |
| Rust 单元测试 | ✅完整 | `db.rs`（embedding roundtrip/cosine）、`retrieval.rs`（RRF/FTS 查询）、`rag.rs`（UTF-8 截断）、`settings.rs`（模型类型判定）、`access.rs`（删除无残留）均有测试 | - |
| 前端单元测试 | ✅完整 | `dirTreeUtils.test.ts`/`fileUtils.test.ts`/`graphUtils.test.ts`/`chatUtils.test.ts`/`markdown.test.ts`/`wikiGraphModel.test.ts`/`wikiPanelUtils.test.ts` + 多个组件测试 | - |

---

## 十一、数据库 Schema

| 项目 | 现状 | 问题描述 | 严重度 |
|------|------|----------|--------|
| 表结构 | ✅完整 | 20+ 张表覆盖：用户/角色/知识库/目录/文档/版本/分片/FTS/任务/日志/Wiki 页面/实体/链接/FAQ/ACL/成员/指标/设置/审计 | - |
| 索引 | ✅完整 | 关键查询路径均有索引：`idx_doc_kb`/`idx_chunk_doc`/`idx_chunk_kb`/`idx_acl_doc`/`idx_wiki_kb`/`idx_kbme_type_time` 等 | - |
| FTS5 | ✅完整 | `chunks_fts`（分片全文检索）+ `wiki_pages_fts`（Wiki 全文检索），均使用 `unicode61` tokenizer | - |
| 外键约束 | ✅完整 | 全面使用 `ON DELETE CASCADE`；关键引用如 `documents.kb_id`、`document_chunks.doc_id`、`wiki_pages.kb_id` 等 | - |
| 迁移 | ✅完整 | `init_tables()` 包含 7+ 项迁移：FTS 外部内容表迁移、parent_id 列补充、pinned/is_system 列补充、source 列补充、extract_status/dir_id 列补充、draft 自动发布、孤立实体页清理 | - |
| CHECK 约束 | ✅完整 | `kb_acl` 表有 `CHECK(scope IN(...))` 和 `CHECK(grantee_type IN(...))`；`qa_messages` 有 `CHECK(role IN('user','assistant'))` | - |
| UNIQUE 约束 | ✅完整 | `users.username`/`roles.name`/`knowledge_bases` 无重复名约束（通过业务层校验）/`kb_acl` 复合唯一/`wiki_pages(kb_id, slug)`/`faq_entries(kb_id, question)` | - |

---

## 十二、Top 10 最值得优先修复/补全清单

### 🔴 P0 — 测试覆盖缺失

**问题**：`.codex_tests/` 目录完全不存在，AGENTS.md 中提到的 50+ 个 smoke 测试文件（`smoke-kb-graph-layout.mjs`/`smoke-kb-graph-style.mjs`/`smoke-chat-context.mjs`/`smoke-kb-chat-utils.mjs`/`smoke-kb-file-utils.mjs`/`smoke-search-text.mjs`/`smoke-wiki-markdown.mjs`/`smoke-dir-tree.mjs`/`smoke-ipc-contract.mjs` 等）全部缺失。CI 回归门可能无法运行。

**建议**：
1. 确认 `.codex_tests/` 是否被 `.gitignore` 排除或在其他分支
2. 若确实缺失，优先恢复 `smoke-ipc-contract.mjs`（IPC 契约测试）和 `smoke-kb-chat-utils.mjs`（RAG 逻辑测试）
3. 考虑将 smoke 测试迁移到 `src/` 下的 Vitest 测试，纳入 CI 标准流程

**文件路径**：`C:\Users\28361\Desktop\ST\st_control\.codex_tests/`（整个目录缺失）

---

### 🟡 P1 — IPC 命令重复注册

**问题**：`kb_fetch_url` 在 `lib.rs` line 664 和 line 702 被注册了两次。Tauri 2 会 panic 或静默覆盖，可能导致不确定行为。

**建议**：删除 `lib.rs` line 702 的重复注册。

**文件路径**：`src-tauri/src/lib.rs` line 702

---

### 🟡 P1 — 前端 kbApi 未封装的后端命令

**问题**：以下 5 个后端命令已注册但前端 `kbApi` 对象未显式封装（只能通过 `kbApi.invoke()` 通用调用，类型安全性差）：
- `kb_rag`（同步 RAG 问答）
- `kb_highlight`（高亮计算）
- `kb_create_user`（创建用户）
- `kb_change_password`（修改密码）
- `kb_delete_user` / `kb_reset_password` / `kb_set_admin`（用户管理）

**建议**：在 `services/ipc.ts` 的 `kbApi` 对象中补充这些命令的封装。

**文件路径**：`src/lib/kb/services/ipc.ts`

---

### 🟡 P1 — 大文本文件 OOM 风险

**问题**：文本类文件（txt/md/csv/json/log 等）在 `parse_document()` 中用 `String::from_utf8_lossy(data)` 全量加载到内存。若用户上传超大日志文件（>100MB），可能导致内存暴涨。

**建议**：
1. 对文本类文件增加大小检查（如 >50MB 时截断或拒绝）
2. 或改用流式解析 + 分段读取

**文件路径**：`src-tauri/src/kb/parse/mod.rs` line 207-213

---

### 🟡 P1 — FTS 索引同步一致性风险

**问题**：`document_chunks` 表和 `chunks_fts` 虚拟表是独立的两张表，所有写入路径必须手动同步 FTS 索引。代码中已通过 `db.rs` 的 `fts_insert_chunk()`/`fts_update_chunk()`/`fts_delete_chunks_by_doc()` 集中化，但若新增写入路径遗漏调用，会导致 FTS 索引与实际内容不一致。

**建议**：
1. 在 `document_chunks` 上添加 SQLite 触发器自动同步 FTS（但 SQLite FTS5 触发器有已知限制）
2. 或增加定期一致性校验脚本（比较 chunks_fts.rowid 与 document_chunks.id）

**文件路径**：`src-tauri/src/kb/db.rs` line 392-493

---

### 🟡 P2 — 向量序列化精度损失

**问题**：`serialize_embedding()` 将 f64 向量转为 f32 存储（每个维度损失约 1e-6 精度）。对 768 维向量的余弦相似度影响可忽略，但应在技术文档中注明。

**建议**：在 `db.rs` 的 `serialize_embedding()` 函数注释中补充精度说明。

**文件路径**：`src-tauri/src/kb/db.rs` line 499-505

---

### 🟡 P2 — 检索结果无游标分页

**问题**：`kb_search` 返回全部 top_k 条结果（最多 100 条），无游标分页机制。大结果集前端需一次性渲染。

**建议**：对于知识库规模较大（>1000 文档）的场景，考虑增加 offset/limit 参数支持分页。

**文件路径**：`src-tauri/src/kb/handlers/search.rs` line 28-146

---

### 🟡 P2 — i18n 国际化

**问题**：前端组件中文硬编码较多（错误信息、按钮文本、提示语），未接入 i18n 框架。若未来需要英文界面，改动量大。

**建议**：当前阶段可接受（面向中文用户）；若需国际化，建议引入 `svelte-i18n` 并提取所有中文字符串到 `locales/zh-CN.json`。

**文件路径**：`src/lib/kb/` 全部 `.svelte` 文件

---

### 🟡 P2 — `CURRENT_USER` 常量残留

**问题**：`db.rs` line 496 的 `pub const CURRENT_USER: i64 = 1` 已被 `UserSession` 替代，但常量仍存在。若其他代码误用此常量会绕过登录态。

**建议**：标记 `#[deprecated]` 或删除，全局搜索确认无引用后移除。

**文件路径**：`src-tauri/src/kb/db.rs` line 496

---

### 🟡 P2 — 网页正文提取质量

**问题**：`extract_web_text()` 为简易 HTML 解析器（去除 script/style/标签），对 SPA/React 渲染页面无法提取正文。未集成 readability 算法。

**建议**：考虑引入 `readability` crate 或 `trafilatura` 提升正文提取质量。

**文件路径**：`src-tauri/src/kb/handlers/docs.rs` line 1113-1208

---

## 附录：IPC 命令完整性对照表

| 后端命令 | 前端 kbApi 封装 | 状态 |
|----------|----------------|------|
| kb_create | create | ✅ |
| kb_list | list | ✅ |
| kb_delete | remove | ✅ |
| kb_update | update | ✅ |
| kb_set_pin | setPin | ✅ |
| kb_list_dirs | listDirs | ✅ |
| kb_create_dir | createDir | ✅ |
| kb_rename_dir | renameDir | ✅ |
| kb_delete_dir | deleteDir | ✅ |
| kb_upload_document | uploadDocument | ✅ |
| kb_multimodal_analyze | multimodalAnalyze | ✅ |
| kb_upload_new_version | uploadNewVersion | ✅ |
| kb_fetch_url | fetchUrl | ✅ (⚠️后端重复注册) |
| kb_batch_fetch_url | batchFetchUrl | ✅ |
| kb_update_chunk | updateChunk | ✅ |
| kb_move_doc | moveDoc | ✅ |
| kb_rename_document | renameDocument | ✅ |
| kb_set_doc_tags | setDocTags | ✅ |
| kb_list_tags | listTags | ✅ |
| kb_faq_import | faqImport | ✅ |
| kb_faq_list | faqList | ✅ |
| kb_faq_delete | faqDelete | ✅ |
| kb_search | search | ✅ |
| kb_rag | ❌未封装 | ⚠️ |
| kb_rag_stream | ragStream | ✅ |
| kb_rag_cancel | ragCancel | ✅ |
| kb_highlight | ❌未封装 | ⚠️ |
| kb_list_versions | listVersions | ✅ |
| kb_version_diff | versionDiff | ✅ |
| kb_set_acl | setAcl | ✅ |
| kb_acl_delete | deleteAcl | ✅ |
| kb_list_documents | listDocuments | ✅ |
| kb_get_document | getDocument | ✅ |
| kb_delete_document | deleteDocument | ✅ |
| kb_restore_version | restoreVersion | ✅ |
| kb_download_document | downloadDocument | ✅ |
| kb_batch_download | batchDownload | ✅ |
| kb_reprocess_document | reprocessDocument | ✅ |
| kb_get_acl | getAcl | ✅ |
| kb_list_models | listModels | ✅ |
| kb_get_default_model | getDefaultModel | ✅ |
| kb_get_default_chat_model | getDefaultChatModel | ✅ |
| kb_get_model_settings | getModelSettings | ✅ |
| kb_set_model_settings | setModelSettings | ✅ |
| kb_get_chunk_settings | ❌未封装 | ⚠️ (通过 invoke) |
| kb_set_chunk_settings | ❌未封装 | ⚠️ (通过 invoke) |
| kb_get_rag_system_prompt | getRagSystemPrompt | ✅ |
| kb_set_rag_system_prompt | setRagSystemPrompt | ✅ |
| kb_test_model | testModel | ✅ |
| kb_export | exportKb | ✅ |
| kb_import | importKb | ✅ |
| kb_get_stats | getStats | ✅ |
| kb_get_analytics | getAnalytics | ✅ |
| kb_track_event | ❌未封装 | ⚠️ |
| kb_recommend_questions | recommendQuestions | ✅ |
| kb_get_analytics_settings | getAnalyticsSettings | ✅ |
| kb_set_analytics_settings | setAnalyticsSettings | ✅ |
| kb_housekeeping | housekeeping | ✅ |
| kb_list_users | listUsers | ✅ |
| kb_create_user | ❌未封装 | ⚠️ |
| kb_change_password | ❌未封装 | ⚠️ |
| kb_delete_user | ❌未封装 | ⚠️ |
| kb_reset_password | ❌未封装 | ⚠️ |
| kb_set_admin | ❌未封装 | ⚠️ |
| kb_list_roles | ❌未封装 | ⚠️ |
| kb_list_members | listMembers | ✅ |
| kb_add_member | addMember | ✅ |
| kb_remove_member | removeMember | ✅ |
| kb_update_member_role | updateMemberRole | ✅ |
| kb_qa_create_session | createSession | ✅ |
| kb_qa_list_sessions | listSessions | ✅ |
| kb_qa_list_messages | listMessages | ✅ |
| kb_qa_delete_session | deleteSession | ✅ |
| kb_search_history | searchHistory | ✅ |
| kb_list_jobs | listJobs | ✅ |
| kb_get_job_logs | getJobLogs | ✅ |
| kb_clear_activity | clearActivity | ✅ |
| kb_stop_processing | stopProcessing | ✅ |
| kb_retry_job | retryJob | ✅ |
| kb_retry_failed_jobs | retryFailedJobs | ✅ |
| kb_wiki_list_pages | wikiListPages | ✅ |
| kb_wiki_dirs | wikiDirs | ✅ |
| kb_wiki_search | wikiSearch | ✅ |
| kb_wiki_get_page | wikiGetPage | ✅ |
| kb_wiki_graph | wikiGraph | ✅ |
| kb_wiki_create_page | wikiCreatePage | ✅ |
| kb_wiki_update_page | wikiUpdatePage | ✅ |
| kb_wiki_delete_page | wikiDeletePage | ✅ |
| kb_wiki_generate | wikiGenerate | ✅ |
| kb_wiki_extract | wikiExtract | ✅ |
| kb_wiki_extract_all | wikiExtractAll | ✅ |
| kb_wiki_list_versions | wikiListVersions | ✅ |
| kb_wiki_restore_version | wikiRestoreVersion | ✅ |
| kb_backup | backup | ✅ |
| kb_list_backups | listBackups | ✅ |
| kb_cleanup_backups | cleanupBackups | ✅ |
| kb_list_audit_logs | listAuditLogs | ✅ |
| kb_login | login | ✅ |
| kb_logout | logout | ✅ |
| kb_current_user | ❌未封装 | ⚠️ (通过 auth.svelte.ts) |

---

## 总结

**整体评价**：知识库系统架构设计成熟，功能完备度高（约 95%），代码质量良好。

**亮点**：
1. 解析链路健壮（魔数嗅探 + zip 炸弹防御 + anydoc 回退 + OCR）
2. 检索架构完整（BM25 + 向量 + 混合 RRF + Rerank + 大库保护 + 缓存）
3. 权限体系精细（RBAC 四级角色 + 对象级 ACL + deny 优先 + 开放知识库）
4. 运维能力完备（任务队列 + 重试/停止 + 备份/恢复 + 导出/导入 + 审计日志）
5. Wiki 知识图谱功能独特（LLM 自动提炼 + 页面链接 + 可视化图谱 + 版本控制）

**最需关注**：
1. 🔴 测试覆盖缺失（`.codex_tests/` 目录不存在）
2. 🟡 IPC 命令重复注册（`kb_fetch_url`）
3. 🟡 前端未封装的后端命令（10+ 个）
