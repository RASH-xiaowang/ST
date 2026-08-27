// ============================================================
// 知识库后端集成测试
// 覆盖：数据库初始化 → 种子用户 → 上传解析 → 分片入库 → FTS/BM25 检索
//      → 权限判定 → Wiki 页面/链接/图谱/全文搜索 → BM25 特殊字符防护
//      → 旧版外部内容 FTS 表的迁移重建
// 使用临时数据库（KbDatabase::open_at），不触碰生产数据。
// 运行：cargo test --offline --test kb_backend
// ============================================================
use kb::auth::ensure_seed_users;
use kb::db::KbDatabase;
use kb::parse::{self, ChunkConfig};
use kb::retrieval;
use kb::wiki;
use rusqlite::params;
use st_control_lib::kb;

use std::sync::atomic::{AtomicU64, Ordering};
static DB_SEQ: AtomicU64 = AtomicU64::new(0);

/// 在系统临时目录创建独立的测试数据库（每个调用唯一路径，避免并行测试争用同一文件）
fn temp_db() -> KbDatabase {
    let seq = DB_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("st_control_kb_it_{}_{}", std::process::id(), seq));
    std::fs::create_dir_all(&dir).unwrap();
    let db = KbDatabase::open_at(dir.join("test_kb.db")).unwrap();
    // 所有测试统一具备种子用户（kb_members/knowledge_bases 外键依赖 users）
    ensure_seed_users(&db);
    db
}

/// 建一个含 owner 成员的知识库（模拟 kb_create 的数据库行为）
fn insert_kb(db: &KbDatabase, name: &str, owner_id: i64) -> i64 {
    let c = db.conn_lock();
    c.execute(
        "INSERT INTO knowledge_bases (name, owner_id) VALUES (?1,?2)",
        params![name, owner_id],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    c.execute(
        "INSERT OR IGNORE INTO kb_members (kb_id, user_id, role) VALUES (?1,?2,'owner')",
        params![id, owner_id],
    )
    .unwrap();
    id
}

#[test]
fn test_db_init_and_seed_users() {
    let db = temp_db();
    // schema 就绪：关键表存在
    for t in [
        "users",
        "knowledge_bases",
        "kb_directories",
        "documents",
        "document_chunks",
        "chunks_fts",
        "kb_acl",
        "wiki_pages",
        "wiki_pages_fts",
        "qa_sessions",
    ] {
        let n: i64 = {
            let c = db.conn_lock();
            c.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table','virtual table') AND name = ?1",
                params![t], |r| r.get(0),
            ).unwrap()
        };
        assert_eq!(n, 1, "缺少表: {}", t);
    }
    // FTS 表必须是普通表（非外部内容表，避免先删后插导致数据库损坏）
    for t in ["chunks_fts", "wiki_pages_fts"] {
        let ddl: String = {
            let c = db.conn_lock();
            c.query_row(
                "SELECT sql FROM sqlite_master WHERE name = ?1",
                params![t],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert!(!ddl.contains("content="), "{} 不应是外部内容表: {}", t, ddl);
    }
    // 种子用户与角色
    ensure_seed_users(&db);
    let (users, roles): (i64, i64) = {
        let c = db.conn_lock();
        (
            c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
                .unwrap(),
            c.query_row("SELECT COUNT(*) FROM roles", [], |r| r.get(0))
                .unwrap(),
        )
    };
    assert_eq!(users, 2, "应 seed admin + 测试员");
    assert_eq!(roles, 3, "应 seed admin/editor/viewer");
}

#[test]
fn test_upload_chunk_bm25_full_pipeline() {
    let db = temp_db();
    let uid = 1i64;
    let kb_id = insert_kb(&db, "测试知识库", uid);

    // 模拟 kb_upload_document 的落库前半段：file_object + document + version
    let (doc_id, version_id) = {
        let c = db.conn_lock();
        c.execute(
            "INSERT INTO file_objects (hash, ext, size, blob_data) VALUES ('h1','md',3,x'68656c6c6f')",
            [],
        ).unwrap();
        let fo_id = c.last_insert_rowid();
        c.execute(
            "INSERT INTO documents (kb_id, title, file_type, status, process_status, created_by) VALUES (?1,'测试文档','md','processing','parsing',?2)",
            params![kb_id, uid],
        ).unwrap();
        let doc_id = c.last_insert_rowid();
        c.execute(
            "INSERT INTO document_versions (doc_id, version_no, file_object_id, created_by) VALUES (?1,1,?2,?3)",
            params![doc_id, fo_id, uid],
        ).unwrap();
        (doc_id, c.last_insert_rowid())
    };

    // 解析 + 分片 + 入库（含 FTS 同步）
    let text = "# 第一章\n企业知识库后端支持向量检索与全文检索，分片策略包括递归、标题感知与父子分块。\n# 第二章\n混合检索使用 RRF 融合向量与 BM25 两路结果。";
    let parsed = parse::parse_document("md", text.as_bytes()).unwrap();
    let cfg = ChunkConfig {
        chunk_size: 800,
        overlap: 120,
        min_chunk: 100,
        strategy: parse::ChunkStrategy::Recursive,
    };
    let chunks = parse::chunk_text(&parsed.text, &cfg);
    assert!(!chunks.is_empty(), "应有分片产出");
    let ids = parse::save_chunks(&db, kb_id, doc_id, version_id, &chunks).unwrap();
    assert_eq!(ids.len(), chunks.len());

    // BM25 检索命中（中文子串：unicode61 + 汉字间隔预处理）
    let hits = retrieval::bm25_search(&db, "向量检索", &[kb_id], 5).unwrap();
    assert!(!hits.is_empty(), "BM25 应命中分片");
    assert!(hits[0].content.contains("向量检索"));
    assert_eq!(hits[0].source, "bm25");

    // 标题感知分片保留 section（回填 title 路径）
    let parsed2 = parse::parse_document(
        "md",
        "# 第一章\n第一段内容足够长用于分片处理。\n## 第一节\n小节内容也足够长用于生成分片。"
            .as_bytes(),
    )
    .unwrap();
    let cfg2 = ChunkConfig {
        chunk_size: 200,
        overlap: 20,
        min_chunk: 20,
        strategy: parse::ChunkStrategy::Title,
    };
    let chunks2 = parse::chunk_text(&parsed2.text, &cfg2);
    assert!(chunks2
        .iter()
        .any(|c| c.section.as_deref() == Some("第一章 / 第一节")));

    // 特殊字符查询不抛错（回归：FTS 转义）
    let r = retrieval::bm25_search(&db, "测试 - (foo) \"bar\"", &[kb_id], 5);
    assert!(r.is_ok(), "含特殊字符的查询不应报错: {:?}", r.err());
}

#[test]
fn test_permissions_members_and_acl() {
    let db = temp_db();
    let owner = 1i64;
    let stranger = 99i64;
    // 外键需要该用户存在
    {
        let c = db.conn_lock();
        c.execute(
            "INSERT INTO users (id, username) VALUES (?1,'stranger')",
            params![stranger],
        )
        .unwrap();
    }
    let kb_id = insert_kb(&db, "私有知识库", owner);

    // 有成员的库：陌生人不可访问，owner 可访问，owner 可管理
    assert!(retrieval::can_access_kb(&db, kb_id, owner));
    assert!(retrieval::can_manage_kb(&db, kb_id, owner));
    assert!(!retrieval::can_access_kb(&db, kb_id, stranger));
    assert!(retrieval::visible_kb_ids(&db, owner).contains(&kb_id));
    assert!(!retrieval::visible_kb_ids(&db, stranger).contains(&kb_id));

    // 开放库（无成员）：任何人可访问
    let open_kb = {
        let c = db.conn_lock();
        c.execute("INSERT INTO knowledge_bases (name) VALUES ('开放库')", [])
            .unwrap();
        c.last_insert_rowid()
    };
    assert!(retrieval::can_access_kb(&db, open_kb, stranger));

    // 文档级 ACL：先 allow 后 deny（deny 优先）
    let (doc_id,) = {
        let c = db.conn_lock();
        c.execute(
            "INSERT INTO documents (kb_id, title, file_type) VALUES (?1,'文档A','txt')",
            params![kb_id],
        )
        .unwrap();
        let d = c.last_insert_rowid();
        c.execute(
            "INSERT INTO kb_acl (scope, doc_id, kb_id, grantee_type, user_id, effect) VALUES ('document',?1,?2,'user',?3,'allow')",
            params![d, kb_id, stranger],
        ).unwrap();
        c.execute(
            "INSERT INTO kb_acl (scope, doc_id, kb_id, grantee_type, user_id, effect) VALUES ('document',?1,?2,'user',?3,'deny')",
            params![d, kb_id, stranger],
        ).unwrap();
        (d,)
    };
    assert!(
        !retrieval::can_access_doc(&db, kb_id, doc_id, stranger),
        "deny 应优先于 allow"
    );
}

#[test]
fn test_wiki_pages_links_graph_and_search() {
    let db = temp_db();
    let uid = 1i64;
    let kb_id = insert_kb(&db, "Wiki 测试库", uid);

    // 先建目标页，再建引用页，最后更新引用页重建链接
    let p2 = wiki::create_page(
        &db,
        &wiki::WikiPageInput {
            kb_id,
            doc_id: None,
            title: "分片策略".into(),
            summary: None,
            content_md: Some("支持递归、标题感知与父子分块三种策略。".into()),
        },
        uid,
    )
    .unwrap();
    let p1 = wiki::create_page(
        &db,
        &wiki::WikiPageInput {
            kb_id,
            doc_id: None,
            title: "架构设计".into(),
            summary: Some("知识库总体架构".into()),
            content_md: Some("核心流程见 [[分片策略]] 页面。".into()),
        },
        uid,
    )
    .unwrap();
    wiki::update_page(
        &db,
        p1,
        &wiki::WikiPageInput {
            kb_id,
            doc_id: None,
            title: "架构设计".into(),
            summary: Some("知识库总体架构".into()),
            content_md: Some("核心流程见 [[分片策略]] 页面。".into()),
        },
    )
    .unwrap();

    let pages = wiki::list_pages(&db, kb_id).unwrap();
    assert_eq!(pages.len(), 2);

    let detail = wiki::get_page(&db, p1).unwrap();
    assert_eq!(detail.out_links.len(), 1, "出链应解析到分片策略页");
    assert_eq!(detail.out_links[0].page_id, p2);
    // 自动反链：A→B 时会补建 B→A（rebuild_kb_links 双向连接），故 p1 也有 1 条入链（来自 p2）
    assert_eq!(detail.in_links.len(), 1, "自动反链：p2 指向 p1");
    assert_eq!(detail.in_links[0].page_id, p2);

    let detail2 = wiki::get_page(&db, p2).unwrap();
    assert_eq!(detail2.in_links.len(), 1, "入链应包含架构设计页");
    assert_eq!(detail2.in_links[0].page_id, p1);

    let g = wiki::graph(&db, kb_id).unwrap();
    assert_eq!(g.nodes.len(), 2);
    // 正向 p1→p2 + 自动反链 p2→p1
    assert_eq!(g.edges.len(), 2);
    assert!(g.edges.iter().any(|e| e.from == p1 && e.to == p2));
    assert!(g.edges.iter().any(|e| e.from == p2 && e.to == p1));

    // Wiki 全文检索（2 字中文子串也可命中）
    let found = wiki::search_pages(&db, kb_id, "分片", 10).unwrap();
    assert!(!found.is_empty(), "Wiki 全文检索应命中");
}

/// 旧版外部内容 FTS 表 → 普通 FTS5 的迁移回归测试
#[test]
fn test_fts_migration_from_legacy_external_content() {
    let seq = DB_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("st_control_kb_mig_{}_{}", std::process::id(), seq));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("legacy.db");
    let kb_id;
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn.execute_batch(include_str!("../src/kb/schema.sql"))
            .unwrap();
        // 模拟旧版：把 FTS 表替换为外部内容表
        conn.execute_batch("DROP TABLE wiki_pages_fts;").unwrap();
        conn.execute_batch("DROP TABLE chunks_fts;").unwrap();
        conn.execute_batch("CREATE VIRTUAL TABLE wiki_pages_fts USING fts5(title, summary, content_md, content='wiki_pages', content_rowid='id', tokenize='unicode61');").unwrap();
        conn.execute_batch("CREATE VIRTUAL TABLE chunks_fts USING fts5(content, content='document_chunks', content_rowid='id', tokenize='unicode61');").unwrap();
        // 写入旧数据（先插 FTS 再删的旧路径在正常库中会损坏，这里只模拟存量数据）
        conn.execute("INSERT INTO users (username) VALUES ('admin')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO knowledge_bases (name, owner_id) VALUES ('旧库', 1)",
            [],
        )
        .unwrap();
        kb_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (kb_id, title, slug, summary, content_md, status) VALUES (?1,'架构','架构','摘要','支持 [[分片]] 与混合检索。','published')",
            params![kb_id],
        ).unwrap();
        let pid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages_fts (rowid, title, summary, content_md) VALUES (?1,'架构','摘要','支持 [[分片]] 与混合检索。')",
            params![pid],
        ).unwrap();
        conn.execute(
            "INSERT INTO documents (kb_id, title, file_type, status) VALUES (?1,'文档','txt','ready')",
            params![kb_id],
        ).unwrap();
        let doc = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO document_versions (doc_id, version_no) VALUES (?1,1)",
            params![doc],
        )
        .unwrap();
        let ver = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO document_chunks (kb_id, doc_id, version_id, seq, content) VALUES (?1,?2,?3,0,'知识库支持向量检索与全文检索。')",
            params![kb_id, doc, ver],
        ).unwrap();
        let cid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO chunks_fts (rowid, content) VALUES (?1,'知识库支持向量检索与全文检索。')",
            params![cid],
        )
        .unwrap();
    }

    // 用 KbDatabase 重新打开 → 触发迁移（检测 content= → 重建 + 回填）
    let db = KbDatabase::open_at(path).unwrap();
    let ddl: String = {
        let c = db.conn_lock();
        c.query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'wiki_pages_fts'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert!(
        !ddl.contains("content="),
        "迁移后不应再是外部内容表: {}",
        ddl
    );

    // Wiki 检索可用（中文子串）
    let found = wiki::search_pages(&db, kb_id, "检索", 10).unwrap();
    assert!(!found.is_empty(), "迁移后 Wiki 全文检索应命中");

    // 分片 BM25 检索可用（中文子串）
    let hits = retrieval::bm25_search(&db, "向量检索", &[kb_id], 5).unwrap();
    assert!(!hits.is_empty(), "迁移后分片 BM25 应命中");
}

/// file_objects 孤儿清理：删除版本后，无引用的原始文件 BLOB 应被回收；
/// 仍被其他文档版本引用的（去重共享）必须保留
#[test]
fn test_orphan_file_objects_cleanup() {
    let db = temp_db();
    let kb = {
        let c = db.conn_lock();
        c.execute(
            "INSERT INTO knowledge_bases (name, owner_id) VALUES ('清理库',1)",
            [],
        )
        .unwrap();
        c.last_insert_rowid()
    };
    let (f1, f2) = {
        let c = db.conn_lock();
        c.execute(
            "INSERT INTO file_objects (hash, ext, size, blob_data) VALUES ('f1','txt',1,x'61')",
            [],
        )
        .unwrap();
        let f1 = c.last_insert_rowid();
        c.execute(
            "INSERT INTO file_objects (hash, ext, size, blob_data) VALUES ('f2','txt',1,x'62')",
            [],
        )
        .unwrap();
        let f2 = c.last_insert_rowid();
        for (t, fo) in [("A", f1), ("B", f1), ("C", f2)] {
            c.execute(
                "INSERT INTO documents (kb_id, title, file_type) VALUES (?1,?2,'txt')",
                params![kb, t],
            )
            .unwrap();
            let d = c.last_insert_rowid();
            c.execute(
                "INSERT INTO document_versions (doc_id, version_no, file_object_id) VALUES (?1,1,?2)",
                params![d, fo],
            ).unwrap();
        }
        (f1, f2)
    };
    // C 删版本 → F2 无引用 → 清理
    {
        let c = db.conn_lock();
        c.execute("DELETE FROM document_versions WHERE doc_id = (SELECT id FROM documents WHERE title='C')", []).unwrap();
        kb::handlers::cleanup_orphan_file_objects(&c, &[f2]).unwrap();
        let cnt: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM file_objects WHERE id = ?1",
                params![f2],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 0, "F2 无引用应被清理");
    }
    // A 删版本 → B 仍引用 F1 → 保留
    {
        let c = db.conn_lock();
        c.execute("DELETE FROM document_versions WHERE doc_id = (SELECT id FROM documents WHERE title='A')", []).unwrap();
        kb::handlers::cleanup_orphan_file_objects(&c, &[f1]).unwrap();
        let cnt: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM file_objects WHERE id = ?1",
                params![f1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 1, "B 仍引用 F1 应保留");
    }
    // B 删版本 → F1 无引用 → 清理
    {
        let c = db.conn_lock();
        c.execute("DELETE FROM document_versions WHERE doc_id = (SELECT id FROM documents WHERE title='B')", []).unwrap();
        kb::handlers::cleanup_orphan_file_objects(&c, &[f1]).unwrap();
        let cnt: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM file_objects WHERE id = ?1",
                params![f1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 0, "无引用后 F1 应清理");
    }
}

/// 开放知识库（无成员）应对全员可见（visible_kb_ids 与 can_access_kb 语义一致）
#[test]
fn test_visible_kb_ids_includes_open_kb() {
    let db = temp_db();
    let owner = 1i64;
    let stranger = 99i64;
    {
        let c = db.conn_lock();
        c.execute(
            "INSERT INTO users (id, username) VALUES (?1,'stranger')",
            params![stranger],
        )
        .unwrap();
    }
    let private_kb = insert_kb(&db, "私有库", owner);
    let open_kb = {
        let c = db.conn_lock();
        c.execute("INSERT INTO knowledge_bases (name) VALUES ('开放库')", [])
            .unwrap();
        c.last_insert_rowid()
    };
    let vis = kb::retrieval::visible_kb_ids(&db, stranger);
    assert!(!vis.contains(&private_kb), "非成员不应看到私有库");
    assert!(vis.contains(&open_kb), "开放库应对全员可见");
    assert!(kb::retrieval::can_access_kb(&db, open_kb, stranger));
}

/// 回归：开放知识库（无成员）+ scope=kb 的显式 deny → deny 优先，用户不可见/不可访问
#[test]
fn test_open_kb_acl_deny_wins() {
    let db = temp_db();
    let owner = 1i64;
    let stranger = 99i64;
    {
        let c = db.conn_lock();
        c.execute(
            "INSERT INTO users (id, username) VALUES (?1,'stranger')",
            params![stranger],
        )
        .unwrap();
    }
    let open_kb = {
        let c = db.conn_lock();
        c.execute(
            "INSERT INTO knowledge_bases (name) VALUES ('开放但被拒绝的库')",
            [],
        )
        .unwrap();
        let id = c.last_insert_rowid();
        c.execute(
            "INSERT INTO kb_acl (scope, kb_id, grantee_type, user_id, effect) VALUES ('kb',?1,'user',?2,'deny')",
            params![id, stranger],
        ).unwrap();
        id
    };
    let vis = kb::retrieval::visible_kb_ids(&db, stranger);
    assert!(!vis.contains(&open_kb), "开放库也不得绕过显式 deny");
    assert!(
        !kb::retrieval::can_access_kb(&db, open_kb, stranger),
        "can_access_kb 必须拒绝 deny 的开放库"
    );
    // 其他用户（owner）不受影响
    assert!(kb::retrieval::can_access_kb(&db, open_kb, owner));
    assert!(kb::retrieval::visible_kb_ids(&db, owner).contains(&open_kb));
}

/// 回归：kb_list_jobs 在 kb_id=None（全部任务视图）时参数绑定正确，
/// 不再出现 `LIMIT ?50` 未绑定导致的必然失败。
#[test]
fn test_list_jobs_all_view_binds_limit() {
    let db = temp_db();
    let uid = 1i64;
    let kb_id = insert_kb(&db, "任务库", uid);
    {
        let c = db.conn_lock();
        c.execute(
            "INSERT INTO file_objects (hash, ext, size, blob_data) VALUES ('j1','md',3,x'6a6f62')",
            [],
        )
        .unwrap();
        let fo = c.last_insert_rowid();
        c.execute(
            "INSERT INTO documents (kb_id, title, file_type, status, process_status) VALUES (?1,'任务文档','md','processing','embedding')",
            params![kb_id],
        ).unwrap();
        let doc = c.last_insert_rowid();
        c.execute(
            "INSERT INTO document_versions (doc_id, version_no, file_object_id) VALUES (?1,1,?2)",
            params![doc, fo],
        )
        .unwrap();
        c.execute(
            "INSERT INTO processing_jobs (doc_id, version_id, stage, progress) VALUES (?1,?2,'embedding',0.5)",
            params![doc, c.last_insert_rowid()],
        ).unwrap();
    }
    let all = kb::handlers::list_jobs(&db, uid, None, 50).unwrap();
    assert_eq!(all.len(), 1, "全部视图应能列出任务");
    assert_eq!(all[0].docTitle, "任务文档");
    let scoped = kb::handlers::list_jobs(&db, uid, Some(kb_id), 50).unwrap();
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].stage, "embedding");
}

/// 回归：split_into_sections 使用一致的字符偏移（不再出现 line_char_len 恒 0 的零长度段/混用字节与字符）
#[test]
fn test_split_sections_char_offsets() {
    let md = kb::parse::parse_document("md", "# 标题\n正文内容".as_bytes()).unwrap();
    assert_eq!(md.sections.len(), 2, "标题段 + 尾部段");
    // 首段为空边界段（标题前），第二段为标题后的正文，偏移以字符计
    assert_eq!(md.sections[0].char_start, 0);
    assert_eq!(md.sections[0].char_end, 0);
    assert_eq!(md.sections[1].title.as_deref(), Some("标题"));
    assert_eq!(
        md.sections[1].char_start, 5,
        "标题行「# 标题\\n」共 5 个字符"
    );
    assert_eq!(md.sections[1].char_end, 9, "正文「正文内容」4 个字符");
}

/// P1 迁移：打开含历史遗留死表的旧库后，死表被幂等删除；file_objects.ref_count 字段不再存在
#[test]
fn test_migration_drops_dead_tables() {
    let seq = DB_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("st_control_kb_dead_{}_{}", std::process::id(), seq));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("legacy_dead.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn.execute_batch(include_str!("../src/kb/schema.sql"))
            .unwrap();
        // 模拟旧库遗留死表（含假数据，验证删除幂等且不报错）
        conn.execute_batch(
            "CREATE TABLE organizations (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE org_members (org_id INTEGER, user_id INTEGER);
             CREATE TABLE permissions (id INTEGER PRIMARY KEY, code TEXT);
             CREATE TABLE role_permissions (role_id INTEGER, permission_id INTEGER);
             CREATE TABLE password_reset_tokens (token TEXT PRIMARY KEY, user_id INTEGER);
             INSERT INTO organizations (name) VALUES ('遗留组织');",
        )
        .unwrap();
    }
    let db = KbDatabase::open_at(path).unwrap();
    let c = db.conn_lock();
    for t in [
        "organizations",
        "org_members",
        "permissions",
        "role_permissions",
        "password_reset_tokens",
    ] {
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                params![t],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "死表 {} 应被迁移删除", t);
    }
    let has_ref: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('file_objects') WHERE name='ref_count'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(has_ref, 0, "file_objects.ref_count 应已移除");
}

/// P2 埋点：事件写入后 analytics_for 的 6 项指标按口径聚合（今日值 + 序列）
#[test]
fn test_metric_events_and_analytics() {
    let db = temp_db();
    let uid = 1i64;
    let kb_id = insert_kb(&db, "埋点库", uid);
    {
        let c = db.conn_lock();
        let ins = |etype: &str, detail: Option<&str>| {
            c.execute(
                "INSERT INTO kb_metric_events (event_type, kb_id, user_id, detail) VALUES (?1,?2,?3,?4)",
                params![etype, kb_id, uid, detail],
            )
            .unwrap();
        };
        // 检索 2 次（1 次有结果）、RAG 1 次、FAQ 命中 1 次、转人工 1 次、推荐点击 1 次
        ins(
            "search",
            Some("{\"mode\":\"hybrid\",\"topK\":10,\"hitCount\":3}"),
        );
        ins(
            "search",
            Some("{\"mode\":\"bm25\",\"topK\":10,\"hitCount\":0}"),
        );
        ins(
            "rag",
            Some("{\"mode\":\"hybrid\",\"topK\":5,\"contextCount\":5}"),
        );
        ins("faq_hit", Some("常见问题一"));
        ins("handoff_click", None);
        ins("recommend_click", Some("热门问题"));
        // 一个已完成的任务（task 指标）
        c.execute(
            "INSERT INTO file_objects (hash, ext, size, blob_data) VALUES ('m1','md',3,x'6d31')",
            [],
        )
        .unwrap();
        let fo = c.last_insert_rowid();
        c.execute(
            "INSERT INTO documents (kb_id, title, file_type, status, process_status) VALUES (?1,'任务','md','ready','ready')",
            params![kb_id],
        )
        .unwrap();
        let doc = c.last_insert_rowid();
        c.execute(
            "INSERT INTO document_versions (doc_id, version_no, file_object_id) VALUES (?1,1,?2)",
            params![doc, fo],
        )
        .unwrap();
        c.execute(
            "INSERT INTO processing_jobs (doc_id, version_id, stage) VALUES (?1,?2,'done')",
            params![doc, c.last_insert_rowid()],
        )
        .unwrap();
    }
    let v = kb::handlers::analytics_for(&db, uid).unwrap();
    let metrics = v
        .get("metrics")
        .and_then(|m| m.as_array())
        .expect("应有 6 项指标");
    assert_eq!(metrics.len(), 6);
    let find = |key: &str| -> &serde_json::Value {
        metrics
            .iter()
            .find(|m| m.get("key").and_then(|k| k.as_str()) == Some(key))
            .unwrap_or_else(|| panic!("缺少指标 {}", key))
    };
    let today_of = |key: &str| {
        find(key)
            .get("today")
            .and_then(|x| x.as_i64())
            .unwrap_or(-1)
    };
    assert_eq!(today_of("faq"), 1, "FAQ 命中事件数");
    assert_eq!(today_of("llm"), 1, "RAG 事件数");
    assert_eq!(today_of("recommend"), 1, "推荐点击数");
    // task / handoff 指标已随首页仪表盘合并移除，此处确认不再暴露
    assert!(metrics
        .iter()
        .all(|m| m.get("key").and_then(|k| k.as_str()) != Some("task")));
    assert!(metrics
        .iter()
        .all(|m| m.get("key").and_then(|k| k.as_str()) != Some("handoff")));
    assert_eq!(today_of("recall"), 50, "召回率 = 1/2 有结果检索");
    // 序列应补零为 7 天
    assert_eq!(
        find("messages")
            .get("series")
            .and_then(|s| s.as_array())
            .map(|a| a.len()),
        Some(7)
    );
}

/// P2 推荐：FAQ 命中热点优先、检索词次之、兜底 FAQ 补足，合并去重
#[test]
fn test_recommend_questions_ordering() {
    let db = temp_db();
    let uid = 1i64;
    let kb_id = insert_kb(&db, "推荐库", uid);
    {
        let c = db.conn_lock();
        c.execute(
            "INSERT INTO faq_entries (kb_id, question, answer) VALUES (?1,'如何重置密码？','设置中操作'),(?1,'如何导入文档？','上传即可')",
            params![kb_id],
        )
        .unwrap();
        c.execute(
            "INSERT INTO kb_metric_events (event_type, kb_id, detail) VALUES
             ('faq_hit',?1,'如何重置密码？'),('faq_hit',?1,'如何重置密码？'),('faq_hit',?1,'如何导入文档？')",
            params![kb_id],
        )
        .unwrap();
        c.execute(
            "INSERT INTO search_logs (kb_id, user_id, query, mode, hit_count) VALUES (?1,?2,'向量检索','hybrid',3),(?1,?2,'向量检索','hybrid',2)",
            params![kb_id, uid],
        )
        .unwrap();
    }
    let recs = kb::handlers::recommend_questions(&db, uid, Some(kb_id), 10).unwrap();
    assert!(!recs.is_empty());
    let questions: Vec<String> = recs
        .iter()
        .map(|r| {
            r.get("question")
                .and_then(|q| q.as_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();
    assert_eq!(questions[0], "如何重置密码？", "命中 2 次的 FAQ 应排最前");
    assert!(questions.contains(&"如何导入文档？".to_string()));
    assert!(
        questions.contains(&"向量检索".to_string()),
        "高频检索词应被推荐: {:?}",
        questions
    );
    // 去重：同一问题只出现一次
    let mut uniq = questions.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(questions.len(), uniq.len(), "推荐问题不应重复");
}

/// 目录计数口径：父目录应包含全部子孙目录的页面（实体页归档在「实体/<类型>」子目录）
#[test]
fn test_dir_subtree_counts() {
    let rows = vec![
        (12, None, "实体".to_string(), 0),
        (13, Some(12), "组织".to_string(), 9),
        (14, Some(12), "资源".to_string(), 8),
        (15, Some(13), "小组".to_string(), 3),
    ];
    let out = kb::handlers::dir_subtree_counts(&rows);
    let get = |id: i64| -> i64 {
        out.iter()
            .find(|(i, _, _, _)| *i == id)
            .map(|(_, _, _, c)| *c)
            .unwrap_or(-1)
    };
    assert_eq!(get(15), 3, "叶子目录只含自身页面");
    assert_eq!(get(13), 12, "组织 = 直属 9 + 小组 3");
    assert_eq!(get(14), 8);
    assert_eq!(get(12), 20, "实体 = 全部子孙页面之和");
}

/// 复刻 kb_list_documents 的动态条件绑定（?1 显式 + 匿名 ? 混用），
/// 验证 tag 过滤在 rusqlite 下参数编号正确、能真正过滤
#[test]
fn test_list_documents_tag_filter_binding() {
    use rusqlite::params_from_iter;
    use rusqlite::types::ToSql;
    let db = temp_db();
    let uid = 1i64;
    let kb_id = insert_kb(&db, "标签库", uid);
    {
        let c = db.conn_lock();
        // 两个文档，一个带「测试」标签
        c.execute(
            "INSERT INTO documents (kb_id, title, file_type, status) VALUES (?1,'带标签文档','md','ready'),(?1,'无标签文档','md','ready')",
            params![kb_id],
        )
        .unwrap();
        let tagged: i64 = c
            .query_row(
                "SELECT id FROM documents WHERE title='带标签文档'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        c.execute(
            "INSERT INTO kb_doc_tags (doc_id, tag) VALUES (?1,'测试')",
            params![tagged],
        )
        .unwrap();
    }
    let conn = db.conn_lock();
    // —— 与 kb_list_documents 完全一致的构建方式 ——
    let mut conds: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn ToSql>> = vec![Box::new(kb_id)];
    conds.push(
        "EXISTS (SELECT 1 FROM kb_doc_tags WHERE doc_id = documents.id AND tag = ?)".to_string(),
    );
    params.push(Box::new("测试".to_string()));
    let base = format!(
        "FROM documents WHERE kb_id = ?1 AND {}",
        conds.join(" AND ")
    );

    // 总数查询（params_from_iter）
    let total: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) {}", base),
            params_from_iter(params.iter()),
            |r| r.get(0),
        )
        .expect("总数查询不应失败");
    assert_eq!(total, 1, "带「测试」标签的文档应只有 1 篇");

    // 数据查询（显式绑定 + LIMIT/OFFSET）
    let limit: i64 = 50;
    let offset: i64 = 0;
    let mut binds: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
    binds.push(&limit);
    binds.push(&offset);
    let rows: Vec<i64> = conn
        .prepare(&format!("SELECT id {} ORDER BY id LIMIT ? OFFSET ?", base))
        .unwrap()
        .query_map(binds.as_slice(), |r| r.get::<_, i64>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(rows.len(), 1, "数据查询应只返回带标签文档");
}

/// 回归：VersionInfo / VersionDiff 序列化必须为 camelCase，
/// 否则前端版本列表显示 vundefined、对比头 fromVersionNo 缺失
#[test]
fn test_version_structs_camel_case_serialization() {
    let v = kb::handlers::VersionInfo {
        id: 1,
        version_no: 2,
        note: Some("说明".into()),
        created_by: Some(3),
        created_at: "2026-08-03 12:00:00".into(),
    };
    let sv = serde_json::to_value(&v).unwrap();
    assert_eq!(
        sv.get("versionNo").and_then(|x| x.as_i64()),
        Some(2),
        "应为 versionNo（驼峰）"
    );
    assert!(sv.get("version_no").is_none(), "不应保留蛇形 version_no");
    assert_eq!(
        sv.get("createdAt").and_then(|x| x.as_str()),
        Some("2026-08-03 12:00:00")
    );

    let d = kb::handlers::VersionDiff {
        from_version_no: 1,
        to_version_no: 2,
        added: vec!["+行".into()],
        removed: vec![],
    };
    let sd = serde_json::to_value(&d).unwrap();
    assert_eq!(sd.get("fromVersionNo").and_then(|x| x.as_i64()), Some(1));
    assert_eq!(sd.get("toVersionNo").and_then(|x| x.as_i64()), Some(2));
    assert!(sd.get("from_version_no").is_none());
}

/// 连接池语义：try_conn_lock 为非阻塞获取（1ms 超时），
/// 空闲时立即返回可用连接；并发持有/获取多路连接不产生死锁
#[test]
fn test_try_conn_lock_pool_semantics() {
    let db = temp_db();
    let conn = db.try_conn_lock().expect("空闲池应立即返回连接（非阻塞）");
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
        .unwrap();
    assert!(n >= 0, "返回的连接应可用");
    drop(conn);

    // 并发持有/获取多路连接不应死锁（回归旧 Mutex 同线程重复加锁场景）
    let mut handles = Vec::new();
    for _ in 0..4 {
        let db = db.clone();
        handles.push(std::thread::spawn(move || {
            let _g = db.conn_lock();
            let _t = db.try_conn_lock();
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
}

/// 统计接口：文档/分片/Wiki 页/任务计数
#[test]
fn test_stats_for_counts() {
    let db = temp_db();
    let uid = 1i64;
    let kb_id = insert_kb(&db, "统计库", uid);
    let (doc_id, version_id) = {
        let c = db.conn_lock();
        c.execute(
            "INSERT INTO file_objects (hash, ext, size, blob_data) VALUES ('s1','md',3,x'73746174')",
            [],
        ).unwrap();
        let fo = c.last_insert_rowid();
        c.execute(
            "INSERT INTO documents (kb_id, title, file_type, status, process_status) VALUES (?1,'统计文档','md','ready','ready')",
            params![kb_id],
        ).unwrap();
        let d = c.last_insert_rowid();
        c.execute(
            "INSERT INTO document_versions (doc_id, version_no, file_object_id) VALUES (?1,1,?2)",
            params![d, fo],
        )
        .unwrap();
        (d, c.last_insert_rowid())
    };
    // 两个分片
    let cfg = ChunkConfig {
        chunk_size: 800,
        overlap: 120,
        min_chunk: 100,
        strategy: parse::ChunkStrategy::Recursive,
    };
    let chunks = parse::chunk_text(
        "第一段用于统计的内容，包含足够长度的文字以产生分片。\n第二段继续补充内容。",
        &cfg,
    );
    assert!(!chunks.is_empty());
    parse::save_chunks(&db, kb_id, doc_id, version_id, &chunks).unwrap();
    // 一个 wiki 页
    wiki::create_page(
        &db,
        &wiki::WikiPageInput {
            kb_id,
            doc_id: None,
            title: "统计页".into(),
            summary: None,
            content_md: Some("内容".into()),
        },
        uid,
    )
    .unwrap();

    let vis = kb::retrieval::visible_kb_ids(&db, uid);
    let s = kb::handlers::stats_for(&db, &vis).unwrap();
    assert_eq!(s.kb_count, 1);
    assert_eq!(s.doc_count, 1);
    assert_eq!(s.chunk_count, chunks.len() as i64);
    assert_eq!(s.wiki_page_count, 1);
    assert_eq!(s.doc_ready, 1);
    assert_eq!(s.doc_processing, 0);
}

/// P1 越权矩阵：统一 require_kb_role / editable_kb_ids 的角色门槛。
/// 覆盖 owner / editor / viewer / 非成员四类用户对 owner / editor / viewer 三个门槛的判定。
#[test]
fn test_role_authorization_matrix() {
    let db = temp_db();
    let owner = 1i64;
    let editor = 9001i64;
    let viewer = 9002i64;
    let outsider = 9003i64;
    {
        let c = db.conn_lock();
        for (id, name) in [
            (editor, "editor_u"),
            (viewer, "viewer_u"),
            (outsider, "outsider_u"),
        ] {
            c.execute(
                "INSERT INTO users (id, username) VALUES (?1,?2)",
                params![id, name],
            )
            .unwrap();
        }
    }
    let kb_id = insert_kb(&db, "权限库", owner);
    {
        let c = db.conn_lock();
        c.execute(
            "INSERT INTO kb_members (kb_id, user_id, role) VALUES (?1,?2,'editor')",
            params![kb_id, editor],
        )
        .unwrap();
        c.execute(
            "INSERT INTO kb_members (kb_id, user_id, role) VALUES (?1,?2,'viewer')",
            params![kb_id, viewer],
        )
        .unwrap();
    }

    // owner：owner / editor / viewer 门槛全部放行
    assert!(retrieval::require_kb_role(&db, kb_id, owner, "owner").is_ok());
    assert!(retrieval::require_kb_role(&db, kb_id, owner, "editor").is_ok());
    assert!(retrieval::require_kb_role(&db, kb_id, owner, "viewer").is_ok());
    // editor：editor / viewer 放行，owner 拒绝
    assert!(retrieval::require_kb_role(&db, kb_id, editor, "owner").is_err());
    assert!(retrieval::require_kb_role(&db, kb_id, editor, "editor").is_ok());
    assert!(retrieval::require_kb_role(&db, kb_id, editor, "viewer").is_ok());
    // viewer：viewer 放行，editor / owner 拒绝
    assert!(retrieval::require_kb_role(&db, kb_id, viewer, "owner").is_err());
    assert!(retrieval::require_kb_role(&db, kb_id, viewer, "editor").is_err());
    assert!(retrieval::require_kb_role(&db, kb_id, viewer, "viewer").is_ok());
    // 非成员：任何门槛都拒绝
    assert!(retrieval::require_kb_role(&db, kb_id, outsider, "viewer").is_err());

    // editable_kb_ids：owner 与 editor 可见，viewer / 非成员不可见
    assert!(retrieval::editable_kb_ids(&db, owner).contains(&kb_id));
    assert!(retrieval::editable_kb_ids(&db, editor).contains(&kb_id));
    assert!(!retrieval::editable_kb_ids(&db, viewer).contains(&kb_id));
    assert!(!retrieval::editable_kb_ids(&db, outsider).contains(&kb_id));
}
