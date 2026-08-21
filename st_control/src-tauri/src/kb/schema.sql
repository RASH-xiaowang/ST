-- ============================================================
-- 知识库平台 数据库 Schema (SQLite)
-- 业务元数据 + 文档分片 + 向量(BLOB) + 权限 ACL + 版本控制
-- ============================================================

-- ─── 用户与角色（RBAC 基础） ───
CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT NOT NULL UNIQUE,
    display_name  TEXT,
    password_hash TEXT NOT NULL DEFAULT '',   -- bcrypt 哈希；空串表示未设置密码（仅用户名登录）
    is_admin      INTEGER NOT NULL DEFAULT 0,  -- 兼容旧数据：1=管理员
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS roles (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE,
    description TEXT
);

CREATE TABLE IF NOT EXISTS user_roles (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id     INTEGER NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);

-- ─── 知识库与目录（树形） ───
CREATE TABLE IF NOT EXISTS knowledge_bases (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    description TEXT,
    owner_id    INTEGER REFERENCES users(id),
    pinned      INTEGER NOT NULL DEFAULT 0,   -- 置顶（常用知识库固定顶部）
    is_system   INTEGER NOT NULL DEFAULT 0,   -- 系统知识库（不可删除/重命名）
    embedding_model TEXT,            -- 记录该库使用的嵌入模型（维度一致性校验）
    embedding_dim   INTEGER,         -- 向量维度
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_kb_owner ON knowledge_bases(owner_id);

CREATE TABLE IF NOT EXISTS kb_directories (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kb_id       INTEGER NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    parent_id   INTEGER REFERENCES kb_directories(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_dir_kb ON kb_directories(kb_id);
CREATE INDEX IF NOT EXISTS idx_dir_parent ON kb_directories(parent_id);

-- 知识库成员（角色级授权，最常用粒度）
CREATE TABLE IF NOT EXISTS kb_members (
    kb_id       INTEGER NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role        TEXT NOT NULL DEFAULT 'viewer',  -- owner/admin/editor/viewer
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (kb_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_kbmem_user ON kb_members(user_id);

-- 对象级 ACL（文档/文件夹级，支持按用户或角色批量授权，deny 优先）
CREATE TABLE IF NOT EXISTS kb_acl (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    scope       TEXT NOT NULL DEFAULT 'document',   -- document / folder / kb
    doc_id      INTEGER REFERENCES documents(id) ON DELETE CASCADE,        -- scope=document
    dir_id      INTEGER REFERENCES kb_directories(id) ON DELETE CASCADE,    -- scope=folder
    kb_id       INTEGER REFERENCES knowledge_bases(id) ON DELETE CASCADE,   -- scope=kb
    grantee_type TEXT NOT NULL DEFAULT 'user',       -- user / role / public
    user_id     INTEGER REFERENCES users(id) ON DELETE CASCADE,
    role_id     INTEGER REFERENCES roles(id),
    effect      TEXT NOT NULL DEFAULT 'allow',       -- allow / deny
    created_by  INTEGER REFERENCES users(id),
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (scope IN ('document','folder','kb')),
    CHECK (grantee_type IN ('user','role','public')),
    -- SQLite 的 UNIQUE 约束不支持表达式（COALESCE）。NULL 在 UNIQUE 中互不冲突，
    -- 正好满足「同一 scope 下不同 doc_id/dir_id/kb_id 组合唯一、NULL 允许共存」的需求。
    UNIQUE(scope, doc_id, dir_id, kb_id, grantee_type, user_id, role_id)
);
CREATE INDEX IF NOT EXISTS idx_acl_doc ON kb_acl(doc_id);
CREATE INDEX IF NOT EXISTS idx_acl_dir ON kb_acl(dir_id);
CREATE INDEX IF NOT EXISTS idx_acl_kb  ON kb_acl(kb_id);

-- ─── 文档与版本 ───
CREATE TABLE IF NOT EXISTS documents (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kb_id       INTEGER NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    dir_id      INTEGER REFERENCES kb_directories(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
      original_name TEXT,
      file_type   TEXT,                -- pdf/docx/md/txt/csv/xlsx
      file_size   INTEGER,
      source      TEXT NOT NULL DEFAULT 'upload',  -- upload/fetch/manual（来源）
      hash        TEXT,                -- 文件内容哈希（去重用）
    current_version_id INTEGER,      -- → document_versions.id
    status      TEXT NOT NULL DEFAULT 'processing',  -- processing/ready/failed（生命周期）
    process_status TEXT,             -- pending/parsing/chunking/embedding/ready/failed（当前版本进度）
    created_by  INTEGER REFERENCES users(id),
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_doc_kb ON documents(kb_id);
CREATE INDEX IF NOT EXISTS idx_doc_dir ON documents(dir_id);

-- 文档标签
CREATE TABLE IF NOT EXISTS kb_doc_tags (
    doc_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    tag    TEXT NOT NULL,
    PRIMARY KEY (doc_id, tag)
);
CREATE INDEX IF NOT EXISTS idx_doc_tags_tag ON kb_doc_tags(tag);

CREATE TABLE IF NOT EXISTS document_versions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    doc_id      INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    version_no  INTEGER NOT NULL,    -- 1,2,3...
    file_object_id INTEGER REFERENCES file_objects(id) ON DELETE CASCADE,
    note        TEXT,                -- 版本说明
    created_by  INTEGER REFERENCES users(id),
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(doc_id, version_no)
);
CREATE INDEX IF NOT EXISTS idx_ver_doc ON document_versions(doc_id);

-- 去重文件存储（原始文件二进制）
CREATE TABLE IF NOT EXISTS file_objects (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    hash        TEXT NOT NULL,
    ext         TEXT,
    size        INTEGER,
    blob_data   BLOB,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(hash)
);

-- ─── 文档分片（chunk）元数据（与向量共存于同一库） ───
CREATE TABLE IF NOT EXISTS document_chunks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kb_id       INTEGER NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    doc_id      INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    version_id  INTEGER REFERENCES document_versions(id) ON DELETE CASCADE,
    seq         INTEGER NOT NULL,    -- 分片在文档中的顺序
    content     TEXT NOT NULL,
    page_no     INTEGER,
    section     TEXT,
    char_start  INTEGER,
    char_end    INTEGER,
    token_count INTEGER,
    parent_id   INTEGER,             -- 父子分块：子块关联父块 id（父块为 NULL）
    embedding_blob BLOB,             -- 向量（f32 小端序列化）
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_chunk_doc ON document_chunks(doc_id);
CREATE INDEX IF NOT EXISTS idx_chunk_kb ON document_chunks(kb_id);
CREATE INDEX IF NOT EXISTS idx_chunk_ver ON document_chunks(version_id);

-- 全文检索（FTS5，普通表：索引自带内容副本，由写入路径手动同步；
-- 注意：外部内容表(content='...')先删后插会对未索引 rowid 报 "database disk image is malformed"，
-- 故统一使用普通 FTS5 表保证健壮性）
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    content,
    tokenize='unicode61'
);

-- ─── 处理任务（异步流水线状态机） ───
CREATE TABLE IF NOT EXISTS processing_jobs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    doc_id      INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    version_id  INTEGER REFERENCES document_versions(id) ON DELETE CASCADE,
    stage       TEXT NOT NULL DEFAULT 'pending',  -- pending/parsing/chunking/embedding/done/failed
    progress    REAL DEFAULT 0.0,
    error       TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_job_doc ON processing_jobs(doc_id);

-- ─── 检索历史 ───
CREATE TABLE IF NOT EXISTS search_logs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kb_id       INTEGER REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    user_id     INTEGER REFERENCES users(id) ON DELETE CASCADE,
    query       TEXT NOT NULL,
    mode        TEXT NOT NULL DEFAULT 'hybrid',  -- vector/bm25/hybrid
    hit_count   INTEGER DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─── 处理日志（任务流水线的详细记录） ───
CREATE TABLE IF NOT EXISTS processing_logs (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id    INTEGER NOT NULL REFERENCES processing_jobs(id) ON DELETE CASCADE,
    level     TEXT NOT NULL DEFAULT 'info',   -- info/warn/error
    message   TEXT,
    detail    TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_plog_job ON processing_logs(job_id);

-- 知识库模型设置（推理/解析/嵌入/重排序四类，全局生效）
  CREATE TABLE IF NOT EXISTS kb_model_settings (
      id          INTEGER PRIMARY KEY AUTOINCREMENT,
      role        TEXT NOT NULL,             -- inference / parsing / embedding / rerank
      provider_id TEXT NOT NULL DEFAULT '',
      model       TEXT NOT NULL DEFAULT '',
      updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
      UNIQUE (role)
  );

  -- 知识库分块设置（全局生效，上传/重处理/新版本共用）
  CREATE TABLE IF NOT EXISTS kb_chunk_settings (
      key         TEXT PRIMARY KEY,          -- strategy / size / overlap
      value       TEXT NOT NULL DEFAULT '',
      updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
  );

-- ─── 问答会话（RAG 持久化） ───
CREATE TABLE IF NOT EXISTS qa_sessions (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER REFERENCES users(id) ON DELETE CASCADE,
    kb_id      INTEGER REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    title      TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_qas_user ON qa_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_qas_kb   ON qa_sessions(kb_id);

CREATE TABLE IF NOT EXISTS qa_messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  INTEGER NOT NULL REFERENCES qa_sessions(id) ON DELETE CASCADE,
    role        TEXT NOT NULL,               -- user / assistant
    content     TEXT,
    citations   TEXT,                        -- JSON: 引用来源列表
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (role IN ('user','assistant'))
);
CREATE INDEX IF NOT EXISTS idx_qam_session ON qa_messages(session_id);

-- FAQ 问答对（检索时优先命中，直接给出标准答案）
CREATE TABLE IF NOT EXISTS faq_entries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kb_id       INTEGER NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    question    TEXT NOT NULL,
    answer      TEXT NOT NULL,
    category    TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (kb_id, question)
);
CREATE INDEX IF NOT EXISTS idx_faq_kb ON faq_entries(kb_id);

-- ─── 指标事件（埋点：检索/RAG/FAQ命中/引用点击/文档操作/转人工/推荐点击等） ───
CREATE TABLE IF NOT EXISTS kb_metric_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type  TEXT NOT NULL,           -- search / rag / faq_hit / citation_click / doc_view /
                                          -- doc_download / doc_edit_chunk / doc_reprocess /
                                          -- wiki_view / wiki_graph_click / handoff_click / recommend_click
    kb_id       INTEGER REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    doc_id      INTEGER REFERENCES documents(id) ON DELETE CASCADE,
    page_id     INTEGER REFERENCES wiki_pages(id) ON DELETE CASCADE,
    user_id     INTEGER REFERENCES users(id) ON DELETE CASCADE,
    session_id  INTEGER REFERENCES qa_sessions(id) ON DELETE CASCADE,
    detail      TEXT,                    -- 事件附加信息（JSON：hitCount/mode/topK/contextCount/question 等）
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_kbme_type_time ON kb_metric_events(event_type, created_at);
CREATE INDEX IF NOT EXISTS idx_kbme_kb_time ON kb_metric_events(kb_id, created_at);

-- 指标卡配置（显示名 + 是否展示；key 为 8 项内置指标标识）
CREATE TABLE IF NOT EXISTS kb_analytics_settings (
    key         TEXT PRIMARY KEY,        -- messages/sessions/faq/llm/recall/handoff/task/recommend
    label       TEXT NOT NULL DEFAULT '',
    visible     INTEGER NOT NULL DEFAULT 1,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─── Wiki 模式：提炼的知识库页面（相互链接的 Markdown 知识库） ───
CREATE TABLE IF NOT EXISTS wiki_pages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kb_id       INTEGER NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    dir_id      INTEGER REFERENCES kb_directories(id) ON DELETE SET NULL,  -- 页面所属目录（实体页按类型归档）
    doc_id      INTEGER REFERENCES documents(id) ON DELETE CASCADE,   -- 来源文档（可空：纯手工/聚合页）
    title       TEXT NOT NULL,
    slug        TEXT NOT NULL,               -- URL 友好的唯一标识
    summary     TEXT DEFAULT '',             -- 一句话摘要
    content_md  TEXT DEFAULT '',             -- Markdown 正文
    status      TEXT NOT NULL DEFAULT 'draft',  -- draft / published
    extract_status TEXT NOT NULL DEFAULT '',     -- '' / pending / done / failed（摘要与实体提取状态）
    created_by  INTEGER REFERENCES users(id),
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (kb_id, slug)
);
CREATE INDEX IF NOT EXISTS idx_wiki_kb ON wiki_pages(kb_id);
CREATE INDEX IF NOT EXISTS idx_wiki_doc ON wiki_pages(doc_id);

-- 页面实体（LLM 从正文抽取，用于知识网络与检索）
CREATE TABLE IF NOT EXISTS wiki_page_entities (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kb_id       INTEGER NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    page_id     INTEGER NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    entity_type TEXT NOT NULL DEFAULT '',
    description TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_wpe_page ON wiki_page_entities(page_id);
CREATE INDEX IF NOT EXISTS idx_wpe_kb ON wiki_page_entities(kb_id);

-- Wiki 页面间链接（知识图谱的边）
CREATE TABLE IF NOT EXISTS wiki_links (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    kb_id         INTEGER NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    from_page_id  INTEGER NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    to_page_id    INTEGER NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    link_type     TEXT NOT NULL DEFAULT 'related',  -- related / reference / child_of / generated
    weight        REAL DEFAULT 1.0,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (from_page_id, to_page_id, link_type)
);
CREATE INDEX IF NOT EXISTS idx_wiki_links_from ON wiki_links(from_page_id);
CREATE INDEX IF NOT EXISTS idx_wiki_links_to   ON wiki_links(to_page_id);
CREATE INDEX IF NOT EXISTS idx_wiki_links_kb   ON wiki_links(kb_id);

-- Wiki 页面全文检索（FTS5，普通表，由写入路径手动同步；见 chunks_fts 注释）
CREATE VIRTUAL TABLE IF NOT EXISTS wiki_pages_fts USING fts5(
    title,
    summary,
    content_md,
    tokenize='unicode61'
);

