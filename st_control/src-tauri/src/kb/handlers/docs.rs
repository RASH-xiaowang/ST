// ============================================================
// 知识库管理 — 文档域
// 自 handlers.rs 拆分：目录树、文档上传/下载/CRUD、移动/重命名/标签。
// 按小步增量填充；当前：目录树 + 上传/抓取/流水线 + 重命名/标签 + 列表/详情/下载/删除 + FAQ。
// ============================================================

use super::cleanup_orphan_file_objects;
use super::{
    log_metric_event, refresh_wiki_for_doc, resolve_embedding_pair, MetricEvent, KB_STORAGE_QUOTA,
};
use crate::kb::db::KbDatabase;
use crate::kb::embed;
use crate::kb::parse::{self, Chunk, ChunkConfig};
use rusqlite::params_from_iter;
use serde::{Deserialize, Serialize};
use tauri::State;
#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct DirNode {
    pub id: i64,
    pub kb_id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub depth: i32,
    pub children: Vec<DirNode>,
}

/// 返回指定知识库的完整目录树（递归，根节点 depth=0）
#[tauri::command]
pub async fn kb_list_dirs(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
) -> Result<Vec<DirNode>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_access_kb(&db, kb_id, uid) {
        return Err("无权限：你无权访问该知识库".to_string());
    }
    let conn = db.conn_lock();
    let mut stmt = conn
        .prepare(
            "SELECT id, kb_id, parent_id, name FROM kb_directories WHERE kb_id = ?1 ORDER BY name",
        )
        .map_err(|e| e.to_string())?;
    let all: Vec<DirNode> = stmt
        .query_map(rusqlite::params![kb_id], |row| {
            Ok(DirNode {
                id: row.get(0)?,
                kb_id: row.get(1)?,
                parent_id: row.get(2)?,
                name: row.get(3)?,
                depth: 0,
                children: Vec::new(),
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(build_tree(all))
}

/// 将扁平目录列表构建为以 parent_id 为父子关系的递归树
fn build_tree(mut all: Vec<DirNode>) -> Vec<DirNode> {
    use std::collections::HashMap;
    let ids: Vec<i64> = all.iter().map(|d| d.id).collect();
    let mut by_id: HashMap<i64, DirNode> = HashMap::new();
    for d in all.drain(..) {
        by_id.insert(d.id, d);
    }
    let mut roots: Vec<DirNode> = Vec::new();
    // 复制 id 列表以避免借用冲突
    for id in ids {
        let node = match by_id.remove(&id) {
            Some(n) => n,
            None => continue,
        };
        match node.parent_id {
            Some(pid) => {
                if let Some(parent) = by_id.get_mut(&pid) {
                    parent.children.push(node);
                } else {
                    // 父节点缺失，归为根
                    roots.push(node);
                }
            }
            None => roots.push(node),
        }
    }
    // 递归回填每个节点的 depth
    fn set_depth(nodes: &mut [DirNode], depth: i32) {
        for n in nodes.iter_mut() {
            n.depth = depth;
            set_depth(&mut n.children, depth + 1);
        }
    }
    set_depth(&mut roots, 0);
    roots
}

#[tauri::command]
pub async fn kb_create_dir(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
    parent_id: Option<i64>,
    name: String,
) -> Result<i64, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    // 编辑权限：owner/admin/editor 可建目录
    let role = crate::kb::retrieval::kb_role(&db, kb_id, uid);
    if !matches!(
        role.as_deref(),
        Some("owner") | Some("admin") | Some("editor")
    ) {
        return Err("无权限：仅知识库 owner/admin/editor 可创建目录".to_string());
    }
    let conn = db.conn_lock();
    conn.execute(
        "INSERT INTO kb_directories (kb_id, parent_id, name) VALUES (?1,?2,?3)",
        rusqlite::params![kb_id, parent_id, name],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

/// 递归收集目录树下的目录 id 与文档 id
fn collect_dir_docs(conn: &rusqlite::Connection, dir_id: i64) -> (Vec<i64>, Vec<i64>) {
    let mut dirs = vec![dir_id];
    let mut docs: Vec<i64> = Vec::new();
    let mut stack = vec![dir_id];
    while let Some(did) = stack.pop() {
        if let Ok(mut stmt) = conn.prepare("SELECT id FROM kb_directories WHERE parent_id = ?1") {
            if let Ok(rows) = stmt.query_map(rusqlite::params![did], |r| r.get::<_, i64>(0)) {
                for r in rows.filter_map(|r| r.ok()) {
                    dirs.push(r);
                    stack.push(r);
                }
            }
        }
        if let Ok(mut stmt) = conn.prepare("SELECT id FROM documents WHERE dir_id = ?1") {
            if let Ok(rows) = stmt.query_map(rusqlite::params![did], |r| r.get::<_, i64>(0)) {
                docs.extend(rows.filter_map(|r| r.ok()));
            }
        }
    }
    (dirs, docs)
}

#[tauri::command]
pub async fn kb_rename_dir(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    dir_id: i64,
    name: String,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("目录名不能为空".to_string());
    }
    let kb_id: i64 = {
        let c = db.conn_lock();
        c.query_row(
            "SELECT kb_id FROM kb_directories WHERE id = ?1",
            rusqlite::params![dir_id],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|_| "目录不存在".to_string())?
    };
    let role = crate::kb::retrieval::kb_role(&db, kb_id, uid);
    if !matches!(
        role.as_deref(),
        Some("owner") | Some("admin") | Some("editor")
    ) {
        return Err("无权限：仅知识库 owner/admin/editor 可重命名目录".to_string());
    }
    let conn = db.conn_lock();
    conn.execute(
        "UPDATE kb_directories SET name = ?1 WHERE id = ?2",
        rusqlite::params![name, dir_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 删除目录（级联删除其全部子目录与文档，含 FTS 索引清理）
#[tauri::command]
pub async fn kb_delete_dir(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    dir_id: i64,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let kb_id: i64 = {
        let c = db.conn_lock();
        c.query_row(
            "SELECT kb_id FROM kb_directories WHERE id = ?1",
            rusqlite::params![dir_id],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|_| "目录不存在".to_string())?
    };
    let role = crate::kb::retrieval::kb_role(&db, kb_id, uid);
    if !matches!(
        role.as_deref(),
        Some("owner") | Some("admin") | Some("editor")
    ) {
        return Err("无权限：仅知识库 owner/admin/editor 可删除目录".to_string());
    }
    let conn = db.conn_lock();
    let (dirs, docs) = collect_dir_docs(&conn, dir_id);
    // 收集本目录树文档引用的 file_object_id（删除文档后清理孤儿 BLOB）
    let fo_ids: Vec<i64> = {
        let mut out: Vec<i64> = Vec::new();
        for doc_id in &docs {
            if let Ok(mut s) = conn
                .prepare("SELECT DISTINCT file_object_id FROM document_versions WHERE doc_id = ?1")
            {
                if let Ok(rows) = s.query_map(rusqlite::params![doc_id], |r| r.get::<_, i64>(0)) {
                    out.extend(rows.filter_map(|r| r.ok()));
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    };
    // 先清理 FTS 索引，再删除文档（chunks/versions 靠外键级联）
    for doc_id in &docs {
        conn.execute(
            "DELETE FROM chunks_fts WHERE rowid IN (SELECT id FROM document_chunks WHERE doc_id = ?1)",
            rusqlite::params![doc_id],
        ).map_err(|e| e.to_string())?;
        // 清理该文档关联 Wiki 页面的 FTS 索引（wiki_pages 由外键级联删除，普通 FTS 表不会自动清理）
        conn.execute(
            "DELETE FROM wiki_pages_fts WHERE rowid IN (SELECT id FROM wiki_pages WHERE doc_id = ?1)",
            rusqlite::params![doc_id],
        ).map_err(|e| e.to_string())?;
    }
    for doc_id in &docs {
        conn.execute(
            "DELETE FROM documents WHERE id = ?1",
            rusqlite::params![doc_id],
        )
        .map_err(|e| e.to_string())?;
    }
    cleanup_orphan_file_objects(&conn, &fo_ids)?;
    let placeholders = dirs.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM kb_directories WHERE id IN ({})", placeholders);
    let binds: Vec<&dyn rusqlite::types::ToSql> = dirs
        .iter()
        .map(|d| d as &dyn rusqlite::types::ToSql)
        .collect();
    conn.execute(&sql, binds.as_slice())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 将文档移动到另一目录（target_dir_id 为 None 表示移到知识库根目录）
#[tauri::command]
pub async fn kb_move_doc(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    doc_id: i64,
    target_dir_id: Option<i64>,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let (kb_id, cur_dir): (i64, Option<i64>) = {
        let c = db.conn_lock();
        c.query_row(
            "SELECT kb_id, dir_id FROM documents WHERE id = ?1",
            rusqlite::params![doc_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "文档不存在".to_string())?
    };
    let role = crate::kb::retrieval::kb_role(&db, kb_id, uid);
    if !matches!(
        role.as_deref(),
        Some("owner") | Some("admin") | Some("editor")
    ) {
        return Err("无权限：仅知识库 owner/admin/editor 可移动文档".to_string());
    }
    if let Some(tid) = target_dir_id {
        let tkb: i64 = {
            let c = db.conn_lock();
            c.query_row(
                "SELECT kb_id FROM kb_directories WHERE id = ?1",
                rusqlite::params![tid],
                |r| r.get::<_, i64>(0),
            )
            .map_err(|_| "目标目录不存在".to_string())?
        };
        if tkb != kb_id {
            return Err("目标目录不属于该知识库".to_string());
        }
    }
    if cur_dir == target_dir_id {
        return Ok(());
    }
    let conn = db.conn_lock();
    conn.execute(
        "UPDATE documents SET dir_id = ?1 WHERE id = ?2",
        rusqlite::params![target_dir_id, doc_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadDocInput {
    pub kb_id: i64,
    pub dir_id: Option<i64>,
    pub title: String,
    pub file_type: String,
    pub data: Vec<u8>, // 原始文件二进制
    pub embedding_provider: Option<String>,
    pub embedding_model: Option<String>,
    /// 分块策略：recursive（默认）/ title（标题感知）/ parent_child（父子分块）
    pub chunk_strategy: Option<String>,
    /// 分块大小（字符数，可选）
    pub chunk_size: Option<usize>,
    /// 分块重叠（字符数，可选）
    pub chunk_overlap: Option<usize>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
#[serde(rename_all = "camelCase")]
pub struct UploadResult {
    pub doc_id: i64,
    pub job_id: i64,
    pub chunk_count: usize,
    pub embedded: usize,
    pub failed_embed: usize,
    /// 命中同知识库内相同内容文件时返回已存在文档（重复跳过，不再建新文档）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_doc_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_title: Option<String>,
}

/// 单文件上传大小上限（与内存解析策略匹配，避免大文件阻塞主线程）
const MAX_UPLOAD_SIZE: usize = 200 * 1024 * 1024;

/// 当前全局存储占用（file_objects 总字节数，含全部版本历史）
fn global_storage_used(db: &KbDatabase) -> i64 {
    let conn = db.conn_lock();
    conn.query_row("SELECT COALESCE(SUM(size), 0) FROM file_objects", [], |r| {
        r.get::<_, i64>(0)
    })
    .unwrap_or(0)
}

/// 上传文档：同步落库（文件/文档/版本/任务）后，由后台任务异步执行 解析 → 分片 → 向量化。
/// 返回后前端可通过 kb_list_jobs / 文档状态轮询进度。
#[tauri::command]
pub async fn kb_upload_document(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    mut input: UploadDocInput,
) -> Result<UploadResult, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    // 权限：可访问知识库即可上传（成员或开放）
    if !crate::kb::retrieval::can_access_kb(&db, input.kb_id, uid) {
        return Err("无权限：你不是该知识库成员".to_string());
    }
    // 上传前置校验：空文件 / 大小上限 / 存储配额
    if input.data.is_empty() {
        return Err("文件内容为空，无法上传".to_string());
    }
    if input.data.len() > MAX_UPLOAD_SIZE {
        return Err(format!(
            "文件大小超过上限（{} MB），当前文件 {} MB",
            MAX_UPLOAD_SIZE / 1024 / 1024,
            input.data.len() / 1024 / 1024
        ));
    }
    let used = global_storage_used(&db);
    if used + input.data.len() as i64 > KB_STORAGE_QUOTA {
        return Err(format!(
            "存储空间不足：已用 {} / 配额 {}，请先清理或提升配额",
            used, KB_STORAGE_QUOTA
        ));
    }
    // 1. 校验文件类型（尽早失败，避免创建无效文档记录；解析为 CPU 密集，移出 tokio worker）
    {
        let ft = input.file_type.clone();
        let (data_back, validate) = tauri::async_runtime::spawn_blocking(move || {
            let r = parse::parse_document(&ft, &input.data);
            (input.data, r)
        })
        .await
        .map_err(|e| format!("解析任务失败: {}", e))?;
        input.data = data_back;
        validate?;
    }

    // 2. 同知识库内相同内容（哈希一致）视为重复上传：跳过建库，返回已存在文档
    let hash = format!("{:x}", md5_short(&input.data));
    {
        let conn = db.conn_lock();
        if let Ok((exist_id, exist_title)) = conn.query_row(
            "SELECT id, title FROM documents WHERE kb_id = ?1 AND hash = ?2 LIMIT 1",
            rusqlite::params![input.kb_id, hash],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        ) {
            log::info!(
                "重复上传被跳过: kb={} doc={} title={}",
                input.kb_id,
                exist_id,
                exist_title
            );
            return Ok(UploadResult {
                doc_id: exist_id,
                job_id: 0,
                chunk_count: 0,
                embedded: 0,
                failed_embed: 0,
                duplicate_doc_id: Some(exist_id),
                duplicate_title: Some(exist_title),
            });
        }
    }

    // 3. 存储原始文件（去重）→ 建文档/版本/任务（同步完成，保证幂等可见）
    let (doc_id, version_id, job_id) = {
        let conn = db.conn_lock();
        conn.execute(
            "INSERT INTO file_objects (hash, ext, size, blob_data) VALUES (?1,?2,?3,?4)
             ON CONFLICT(hash) DO NOTHING",
            rusqlite::params![hash, input.file_type, input.data.len() as i64, input.data],
        )
        .map_err(|e| e.to_string())?;
        let file_obj_id = conn
            .query_row(
                "SELECT id FROM file_objects WHERE hash = ?1",
                rusqlite::params![hash],
                |r| r.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())?;

        conn.execute(
            "INSERT INTO documents (kb_id, dir_id, title, original_name, file_type, file_size, source, hash, status, process_status, created_by)
             VALUES (?1,?2,?3,?4,?5,?6,'upload',?7,'processing','pending',?8)",
            rusqlite::params![input.kb_id, input.dir_id, input.title, input.title, input.file_type, input.data.len() as i64, hash, uid],
        ).map_err(|e| e.to_string())?;
        let doc_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO document_versions (doc_id, version_no, file_object_id, created_by) VALUES (?1,1,?2,?3)",
            rusqlite::params![doc_id, file_obj_id, uid],
        ).map_err(|e| e.to_string())?;
        let version_id = conn.last_insert_rowid();

        conn.execute(
            "UPDATE documents SET current_version_id = ?1 WHERE id = ?2",
            rusqlite::params![version_id, doc_id],
        )
        .map_err(|e| e.to_string())?;

        conn.execute(
            "INSERT INTO processing_jobs (doc_id, version_id, stage) VALUES (?1,?2,'pending')",
            rusqlite::params![doc_id, version_id],
        )
        .map_err(|e| e.to_string())?;
        let job_id = conn.last_insert_rowid();
        (doc_id, version_id, job_id)
    };

    // 3. 后台异步流水线：解析 → 分片 → 向量化 → 更新状态
    let db_task = (*db).clone();
    let kb_id = input.kb_id;
    let file_type = input.file_type.clone();
    let data = input.data; // 移动所有权到任务，避免大文件二次拷贝
    let (embedding_provider, embedding_model) = resolve_embedding_pair(
        &db,
        input.embedding_provider.clone(),
        input.embedding_model.clone(),
    );
    let chunk_strategy = input.chunk_strategy.clone();
    let chunk_size = input.chunk_size;
    let chunk_overlap = input.chunk_overlap;
    tauri::async_runtime::spawn(async move {
        process_document_async(
            db_task,
            DocProcessJob {
                kb_id,
                doc_id,
                version_id,
                job_id,
                file_type,
                data,
            },
            ChunkingOptions {
                embedding_provider,
                embedding_model,
                chunk_strategy,
                chunk_size,
                chunk_overlap,
            },
        )
        .await;
    });

    Ok(UploadResult {
        doc_id,
        job_id,
        chunk_count: 0,
        embedded: 0,
        failed_embed: 0,
        duplicate_doc_id: None,
        duplicate_title: None,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewVersionInput {
    pub doc_id: i64,
    pub file_type: String,
    pub data: Vec<u8>,
    pub note: Option<String>,
    pub embedding_provider: Option<String>,
    pub embedding_model: Option<String>,
    pub chunk_strategy: Option<String>,
    pub chunk_size: Option<usize>,
    pub chunk_overlap: Option<usize>,
}

/// 上传文档新版本：为已有文档追加版本（版本号自动 +1）并重新走
/// 解析 → 分片 → 向量化流水线；旧版本保留，可随时回滚。
#[tauri::command]
pub async fn kb_upload_new_version(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    mut input: NewVersionInput,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let (kb_id, title): (i64, String) = {
        let conn = db.conn_lock();
        conn.query_row(
            "SELECT kb_id, title FROM documents WHERE id = ?1",
            rusqlite::params![input.doc_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("文档不存在: {}", e))?
    };
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可上传新版本".to_string());
    }
    // 上传前置校验：空文件 / 大小上限 / 存储配额（与首次上传一致）
    if input.data.is_empty() {
        return Err("文件内容为空，无法上传".to_string());
    }
    if input.data.len() > MAX_UPLOAD_SIZE {
        return Err(format!(
            "文件大小超过上限（{} MB），当前文件 {} MB",
            MAX_UPLOAD_SIZE / 1024 / 1024,
            input.data.len() / 1024 / 1024
        ));
    }
    let used = global_storage_used(&db);
    if used + input.data.len() as i64 > KB_STORAGE_QUOTA {
        return Err(format!(
            "存储空间不足：已用 {} / 配额 {}，请先清理或提升配额",
            used, KB_STORAGE_QUOTA
        ));
    }
    // 校验文件类型（与首次上传一致，尽早失败；解析为 CPU 密集，移出 tokio worker）
    {
        let ft = input.file_type.clone();
        let (data_back, validate) = tauri::async_runtime::spawn_blocking(move || {
            let r = parse::parse_document(&ft, &input.data);
            (input.data, r)
        })
        .await
        .map_err(|e| format!("解析任务失败: {}", e))?;
        input.data = data_back;
        validate?;
    }

    // 落库：去重文件对象 + 新版本 + 处理任务（同步完成，保证可追踪）
    let (version_id, job_id) = {
        let conn = db.conn_lock();
        let hash = format!("{:x}", md5_short(&input.data));
        // 文件按哈希去重：已存在则保持原记录（引用计数由 document_versions 的
        // NOT EXISTS 孤儿清理判定，不再维护 ref_count 字段）
        conn.execute(
            "INSERT INTO file_objects (hash, ext, size, blob_data) VALUES (?1,?2,?3,?4)
             ON CONFLICT(hash) DO NOTHING",
            rusqlite::params![hash, input.file_type, input.data.len() as i64, input.data],
        )
        .map_err(|e| e.to_string())?;
        let file_obj_id: i64 = conn
            .query_row(
                "SELECT id FROM file_objects WHERE hash = ?1",
                rusqlite::params![hash],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let next_version: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version_no), 0) + 1 FROM document_versions WHERE doc_id = ?1",
                rusqlite::params![input.doc_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let note = input
            .note
            .clone()
            .unwrap_or_else(|| format!("上传新版本 v{}", next_version));
        conn.execute(
            "INSERT INTO document_versions (doc_id, version_no, file_object_id, note, created_by)
             VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params![input.doc_id, next_version, file_obj_id, note, uid],
        )
        .map_err(|e| e.to_string())?;
        let version_id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE documents SET current_version_id = ?1, status='processing', process_status='pending',
                    file_type = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![version_id, input.file_type, input.doc_id],
        ).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO processing_jobs (doc_id, version_id, stage) VALUES (?1,?2,'pending')",
            rusqlite::params![input.doc_id, version_id],
        )
        .map_err(|e| e.to_string())?;
        (version_id, conn.last_insert_rowid())
    };

    // 后台异步流水线（复用上传的处理函数）
    let db_task = (*db).clone();
    let doc_id = input.doc_id;
    let file_type = input.file_type.clone();
    let data = input.data;
    let (embedding_provider, embedding_model) = resolve_embedding_pair(
        &db,
        input.embedding_provider.clone(),
        input.embedding_model.clone(),
    );
    let chunk_strategy = input.chunk_strategy.clone();
    let chunk_size = input.chunk_size;
    let chunk_overlap = input.chunk_overlap;
    tauri::async_runtime::spawn(async move {
        process_document_async(
            db_task,
            DocProcessJob {
                kb_id,
                doc_id,
                version_id,
                job_id,
                file_type,
                data,
            },
            ChunkingOptions {
                embedding_provider,
                embedding_model,
                chunk_strategy,
                chunk_size,
                chunk_overlap,
            },
        )
        .await;
    });

    Ok(
        serde_json::json!({ "docId": doc_id, "versionId": version_id, "jobId": job_id, "title": title }),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchUrlInput {
    pub url: String,
    pub kb_id: i64,
    pub dir_id: Option<i64>,
    pub embedding_provider: Option<String>,
    pub embedding_model: Option<String>,
}

/// 网页抓取：下载 URL → 提取标题与正文 → 作为 Markdown 文档入库并走解析流水线
#[tauri::command]
pub async fn kb_fetch_url(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    input: FetchUrlInput,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_access_kb(&db, input.kb_id, uid) {
        return Err("无权限：你不是该知识库成员".to_string());
    }
    let url = input.url.trim().to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL 必须以 http:// 或 https:// 开头".to_string());
    }
    // SSRF 防护：拒绝抓取内网 / 保留地址
    let parsed = reqwest::Url::parse(&url).map_err(|_| "URL 格式无效".to_string())?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL 缺少主机名".to_string())?
        .to_string();
    if host_is_private(&host).await {
        return Err("出于安全考虑，禁止抓取内网或保留地址".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| format!("抓取网页失败: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("网页返回错误 {}", status));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| format!("读取网页内容失败: {}", e))?;
    let (raw_title, text) = extract_web_text(&html);
    if text.trim().is_empty() {
        return Err("网页未提取到正文内容".to_string());
    }
    let title = if raw_title.trim().is_empty() {
        url.clone()
    } else {
        raw_title.trim().to_string()
    };
    let md = format!("# {}\n\n> 来源：{}\n\n{}", title, url, text);

    // 落库：与文件上传共用同一套文档/版本/任务结构
    let (embedding_provider, embedding_model) =
        resolve_embedding_pair(&db, input.embedding_provider, input.embedding_model);
    let (doc_id, version_id, job_id) = {
        let conn = db.conn_lock();
        let hash = format!("{:x}", md5_short(md.as_bytes()));
        conn.execute(
            "INSERT INTO file_objects (hash, ext, size, blob_data) VALUES (?1,'md',?2,?3)
             ON CONFLICT(hash) DO NOTHING",
            rusqlite::params![hash, md.len() as i64, md.as_bytes()],
        )
        .map_err(|e| e.to_string())?;
        let file_obj_id: i64 = conn
            .query_row(
                "SELECT id FROM file_objects WHERE hash = ?1",
                rusqlite::params![hash],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO documents (kb_id, dir_id, title, original_name, file_type, file_size, source, hash, status, process_status, created_by)
             VALUES (?1,?2,?3,?3,'md',?4,'fetch',?5,'processing','pending',?6)",
            rusqlite::params![input.kb_id, input.dir_id, title, md.len() as i64, hash, uid],
        )
        .map_err(|e| e.to_string())?;
        let doc_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO document_versions (doc_id, version_no, file_object_id, created_by) VALUES (?1,1,?2,?3)",
            rusqlite::params![doc_id, file_obj_id, uid],
        )
        .map_err(|e| e.to_string())?;
        let version_id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE documents SET current_version_id = ?1 WHERE id = ?2",
            rusqlite::params![version_id, doc_id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO processing_jobs (doc_id, version_id, stage) VALUES (?1,?2,'pending')",
            rusqlite::params![doc_id, version_id],
        )
        .map_err(|e| e.to_string())?;
        (doc_id, version_id, conn.last_insert_rowid())
    };

    let db_task = (*db).clone();
    let kb_id = input.kb_id;
    let chunk_strategy = None;
    let chunk_size = None;
    let chunk_overlap = None;
    tauri::async_runtime::spawn(async move {
        process_document_async(
            db_task,
            DocProcessJob {
                kb_id,
                doc_id,
                version_id,
                job_id,
                file_type: "md".to_string(),
                data: md.into_bytes(),
            },
            ChunkingOptions {
                embedding_provider,
                embedding_model,
                chunk_strategy,
                chunk_size,
                chunk_overlap,
            },
        )
        .await;
    });
    Ok(serde_json::json!({ "docId": doc_id, "jobId": job_id, "title": title }))
}

/// 判断 IP 是否为内网 / 保留地址（SSRF 防护）
fn ip_is_private(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v) => {
            v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_broadcast()
                || v.is_unspecified()
                // 云厂商 metadata 服务（阿里云 100.100.100.200）
                || (v.octets()[0] == 100 && v.octets()[1] == 100)
        }
        std::net::IpAddr::V6(v) => v.is_loopback() || v.is_unspecified() || v.is_unique_local(),
    }
}

/// 解析主机并判断是否解析到内网地址（域名任一解析结果内网即拒绝，防 DNS rebinding）
async fn host_is_private(host: &str) -> bool {
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return ip_is_private(ip);
    }
    if let Ok(mut addrs) = tokio::net::lookup_host((host, 443)).await {
        return addrs.any(|a| ip_is_private(a.ip()));
    }
    false
}

/// 简易网页正文提取：去除 script/style/注释/标签，按 UTF-8 解码文本，
/// 保留标题与段落文本（修复中文乱码与脚本内容穿透问题）。
fn extract_web_text(html: &str) -> (String, String) {
    let mut title = String::new();
    if let Some(start) = html.find("<title") {
        if let Some(gt) = html[start..].find('>') {
            let content_start = start + gt + 1;
            if let Some(end) = html[content_start..].find("</title>") {
                title = html[content_start..content_start + end].trim().to_string();
            }
        }
    }
    let bytes = html.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0usize;
    let mut in_skip: Option<String> = None; // script / style / comment
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if bytes[i..].starts_with(b"<!--") {
                in_skip = Some("comment".to_string());
                i += 4;
                continue;
            }
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_alphanumeric() {
                j += 1;
            }
            let tag = html[i + 1..j].to_ascii_lowercase();
            if tag == "script" || tag == "style" {
                in_skip = Some(tag);
            }
            // 跳过整个开始/结束标签
            while i < bytes.len() && bytes[i] != b'>' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            if in_skip.is_none() {
                out.push(b' ');
            }
            continue;
        }
        if let Some(sk) = &in_skip {
            let lower = html[i..].to_ascii_lowercase();
            if sk == "comment" && bytes[i..].starts_with(b"-->") {
                in_skip = None;
                i += 3;
                continue;
            }
            if (sk == "script" && lower.starts_with("</script"))
                || (sk == "style" && lower.starts_with("</style"))
            {
                in_skip = None;
            }
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    let text = String::from_utf8_lossy(&out);
    // 归一化为段落
    let mut paragraphs: Vec<String> = Vec::new();
    let mut buf = String::new();
    for chunk in text.split(['\n', '\r']) {
        let line = chunk.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            if !buf.trim().is_empty() {
                paragraphs.push(buf.trim().to_string());
                buf.clear();
            }
            continue;
        }
        buf.push_str(&line);
        buf.push('\n');
        if buf.len() > 300 {
            paragraphs.push(buf.trim().to_string());
            buf.clear();
        }
    }
    if !buf.trim().is_empty() {
        paragraphs.push(buf.trim().to_string());
    }
    let mut joined = paragraphs.join("\n\n");
    // 限制正文长度，避免超大页面入库
    if joined.len() > 200_000 {
        let mut cut = 200_000;
        while cut > 0 && !joined.is_char_boundary(cut) {
            cut -= 1;
        }
        joined.truncate(cut);
    }
    (title, joined)
}

/// 后台文档处理流水线：解析 → 分片入库 → 向量化 → 更新文档/任务状态。
/// 失败时文档与任务均标记 failed，并写入 processing_logs。
/// 文档处理参数（嵌入模型 + 分块配置）
pub struct ChunkingOptions {
    pub embedding_provider: Option<String>,
    pub embedding_model: Option<String>,
    pub chunk_strategy: Option<String>,
    pub chunk_size: Option<usize>,
    pub chunk_overlap: Option<usize>,
}

/// 文档处理任务（定位信息 + 待处理文件）
pub struct DocProcessJob {
    pub kb_id: i64,
    pub doc_id: i64,
    pub version_id: i64,
    pub job_id: i64,
    pub file_type: String,
    pub data: Vec<u8>,
}

async fn process_document_async(db: KbDatabase, job: DocProcessJob, opts: ChunkingOptions) {
    let DocProcessJob {
        kb_id,
        doc_id,
        version_id,
        job_id,
        file_type,
        data,
    } = job;
    let embedding_provider = opts.embedding_provider;
    let embedding_model = opts.embedding_model;
    let chunk_strategy = opts.chunk_strategy;
    let chunk_size = opts.chunk_size;
    let chunk_overlap = opts.chunk_overlap;
    let mark_failed = |err: &str| {
        let conn = db.conn_lock();
        let _ = conn.execute(
            "UPDATE documents SET status='failed', process_status='failed' WHERE id = ?1",
            rusqlite::params![doc_id],
        );
        let _ = conn.execute(
            "UPDATE processing_jobs SET stage='failed', progress=1.0, error=?1 WHERE id = ?2",
            rusqlite::params![err, job_id],
        );
        let _ = conn.execute(
            "INSERT INTO processing_logs (job_id, level, message) VALUES (?1,'error',?2)",
            rusqlite::params![job_id, err],
        );
        log::error!("文档处理失败: doc={} err={}", doc_id, err);
    };

    // 解析 + 分片入库（CPU 密集：文档解析/分块/FTS 写入，移出 tokio worker）
    let parse_outcome = {
        let db_block = db.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<(Vec<Chunk>, Vec<i64>), String> {
            let parsed = parse::parse_document(&file_type, &data).map_err(|e| e.to_string())?;
            // 分块配置（overlap 上限依赖最终 chunk_size，先算 size 再算 overlap）
            let base_cfg = ChunkConfig::default();
            let chunk_size = chunk_size.map(|sz| sz.max(100)).unwrap_or(base_cfg.chunk_size);
            let cfg = ChunkConfig {
                strategy: chunk_strategy
                    .as_deref()
                    .unwrap_or("recursive")
                    .parse()
                    .unwrap_or(parse::ChunkStrategy::Recursive),
                chunk_size,
                overlap: chunk_overlap
                    .map(|ov| ov.min(chunk_size / 2))
                    .unwrap_or(base_cfg.overlap),
                ..base_cfg
            };
            // 分片入库（含 FTS 同步）
            let chunks = parse::chunk_text(&parsed.text, &cfg);
            // 写新分片前先清理该文档旧版本的分片（普通 FTS 表需手动清理），
            // 保证同一时间库里只保留当前版本的分片，避免新旧版本内容混在一起被检索到
            {
                let conn = db_block.conn_lock();
                let _ = conn.execute(
                    "DELETE FROM chunks_fts WHERE rowid IN (SELECT id FROM document_chunks WHERE doc_id = ?1)",
                    rusqlite::params![doc_id],
                );
                let _ = conn.execute(
                    "DELETE FROM document_chunks WHERE doc_id = ?1",
                    rusqlite::params![doc_id],
                );
            }
            let chunk_ids = parse::save_chunks(&db_block, kb_id, doc_id, version_id, &chunks)
                .map_err(|e| e.to_string())?;
            Ok((chunks, chunk_ids))
        })
        .await
        .map_err(|e| format!("文档解析任务失败: {}", e))
    };
    let (chunks, chunk_ids) = match parse_outcome {
        Ok(Ok(v)) => v,
        Ok(Err(e)) | Err(e) => {
            mark_failed(&e);
            return;
        }
    };
    // 向量化
    {
        let conn = db.conn_lock();
        let _ = conn.execute(
            "UPDATE documents SET process_status='embedding' WHERE id = ?1",
            rusqlite::params![doc_id],
        );
        let _ = conn.execute(
            "UPDATE processing_jobs SET stage='embedding', progress=0.5 WHERE id = ?1",
            rusqlite::params![job_id],
        );
    }
    let id_chunk_pairs: Vec<(i64, Chunk)> = chunk_ids
        .iter()
        .zip(chunks.iter())
        .map(|(id, c)| (*id, c.clone()))
        .collect();
    // 未配置嵌入模型（或传入的模型被标记为非嵌入类型被回退）：跳过向量化，
    // 文档仍解析/分片成功，标记 ready + no_embedding，可正常打开、预览、全文检索。
    let no_embedding = embedding_provider
        .as_deref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
        || embedding_model
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true);
    let (ok, fail_n, embed_dim): (usize, usize, Option<usize>) = if no_embedding {
        (0, 0, None)
    } else {
        match embed::embed_chunks(
            &db,
            kb_id,
            &id_chunk_pairs,
            embedding_provider.as_deref(),
            embedding_model.as_deref(),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                // 向量化前置校验失败（如知识库已用其他嵌入模型）不再整篇标失败：
                // 内容已解析分片完成，标记 embed_error 供用户处理后重试。
                let conn = db.conn_lock();
                let _ = conn.execute(
                    "UPDATE documents SET status='ready', process_status='embed_error' WHERE id = ?1",
                    rusqlite::params![doc_id],
                );
                let _ = conn.execute(
                    "UPDATE processing_jobs SET stage='embed_error', progress=1.0, error=?1 WHERE id = ?2",
                    rusqlite::params![e, job_id],
                );
                let _ = conn.execute(
                    "INSERT INTO processing_logs (job_id, level, message) VALUES (?1,'warn',?2)",
                    rusqlite::params![job_id, e],
                );
                log::warn!("文档向量化前置校验失败: doc={} err={}", doc_id, e);
                (0, chunks.len(), None)
            }
        }
    };
    // 完成：解析/分片成功即视为 ready（可操作），向量化状态由 process_status 细分。
    // 若任务已被手动停止（kb_stop_processing 标记为 failed），不再改写任务状态，仅复位文档。
    {
        let conn = db.conn_lock();
        let still_active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM processing_jobs WHERE id = ?1 AND stage IN ('pending','parsing','chunking','embedding')",
                rusqlite::params![job_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if still_active == 0 {
            let _ = conn.execute(
                "UPDATE documents SET status='ready', process_status='ready' WHERE id = ?1",
                rusqlite::params![doc_id],
            );
            drop(conn);
            return;
        }
        let (process_status, stage, err_msg) = if no_embedding {
            (
                "no_embedding",
                "done",
                "未配置嵌入模型，文档已解析但未向量化（可正常打开/预览/全文检索，语义检索需先配置 Embeddings 模型后重新处理）",
            )
        } else if !chunks.is_empty() && ok == 0 {
            (
                "embed_error",
                "embed_error",
                "全部向量化失败（请检查大模型嵌入配置：提供方/模型是否可用），文档已解析但未向量化",
            )
        } else {
            ("ready", "done", "")
        };
        let _ = conn.execute(
            "UPDATE documents SET status='ready', process_status=?1 WHERE id = ?2",
            rusqlite::params![process_status, doc_id],
        );
        if err_msg.is_empty() {
            let _ = conn.execute(
                "UPDATE processing_jobs SET stage=?1, progress=1.0 WHERE id = ?2",
                rusqlite::params![stage, job_id],
            );
            let _ = conn.execute(
                "INSERT INTO processing_logs (job_id, level, message) VALUES (?1,'info',?2)",
                rusqlite::params![
                    job_id,
                    format!(
                        "处理完成：分片 {}，嵌入成功 {}，失败 {}",
                        chunks.len(),
                        ok,
                        fail_n
                    )
                ],
            );
        } else {
            let _ = conn.execute(
                "UPDATE processing_jobs SET stage=?1, progress=1.0, error=?2 WHERE id = ?3",
                rusqlite::params![stage, err_msg, job_id],
            );
            let _ = conn.execute(
                "INSERT INTO processing_logs (job_id, level, message) VALUES (?1,'warn',?2)",
                rusqlite::params![job_id, err_msg],
            );
        }
    }
    // 记录嵌入模型/维度（record_embedding_meta 内部会加锁，必须在锁外调用）
    if let Some(dim) = embed_dim {
        let _ =
            embed::record_embedding_meta(&db, kb_id, embedding_model.as_deref().unwrap_or(""), dim);
    }
    // 源文档内容变化 → 自动刷新关联 Wiki 页面的摘要/实体
    if ok > 0 {
        refresh_wiki_for_doc(&db, doc_id);
    }
    log::info!(
        "文档处理完成: doc={} chunks={} ok={} fail={}",
        doc_id,
        chunks.len(),
        ok,
        fail_n
    );
}

fn md5_short(data: &[u8]) -> u128 {
    // 简易 128 位哈希（非加密，仅去重用）：FNV-1a 组合
    let mut h: u128 = 0x6c62272e07bb014262b821756295c58d;
    for &b in data {
        h ^= b as u128;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// 重命名文档（仅改展示标题，original_name 保留原始文件名）
#[tauri::command]
pub async fn kb_rename_document(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    doc_id: i64,
    title: String,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("文档名称不能为空".to_string());
    }
    if title.chars().count() > 200 {
        return Err("文档名称过长（最多 200 字）".to_string());
    }
    let kb_id: i64 = {
        let c = db.conn_lock();
        c.query_row(
            "SELECT kb_id FROM documents WHERE id = ?1",
            rusqlite::params![doc_id],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| format!("文档不存在: {}", e))?
    };
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可重命名文档".to_string());
    }
    let conn = db.conn_lock();
    conn.execute(
        "UPDATE documents SET title = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![title, doc_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 设置文档标签（整体替换）
#[tauri::command]
pub async fn kb_set_doc_tags(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    doc_id: i64,
    tags: Vec<String>,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let kb_id: i64 = {
        let c = db.conn_lock();
        c.query_row(
            "SELECT kb_id FROM documents WHERE id = ?1",
            rusqlite::params![doc_id],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| format!("文档不存在: {}", e))?
    };
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可设置标签".to_string());
    }
    let cleaned: Vec<String> = tags
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && t.len() <= 30)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let conn = db.conn_lock();
    conn.execute(
        "DELETE FROM kb_doc_tags WHERE doc_id = ?1",
        rusqlite::params![doc_id],
    )
    .map_err(|e| e.to_string())?;
    for t in &cleaned {
        conn.execute(
            "INSERT OR IGNORE INTO kb_doc_tags (doc_id, tag) VALUES (?1,?2)",
            rusqlite::params![doc_id, t],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 列出知识库内的全部标签及使用数量
#[tauri::command]
pub async fn kb_list_tags(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
) -> Result<Vec<serde_json::Value>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_access_kb(&db, kb_id, uid) {
        return Err("无权限：你无权访问该知识库".to_string());
    }
    let conn = db.conn_lock();
    let mut stmt = conn
        .prepare(
            "SELECT t.tag, COUNT(*) AS cnt FROM kb_doc_tags t
             JOIN documents d ON d.id = t.doc_id
             WHERE d.kb_id = ?1 GROUP BY t.tag ORDER BY cnt DESC, t.tag ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![kb_id], |r| {
            Ok(serde_json::json!({ "tag": r.get::<_, String>(0)?, "count": r.get::<_, i64>(1)? }))
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[tauri::command]
// IPC 契约要求扁平参数（前端固定传参顺序），参数对象收敛不适用于 command 入口
#[allow(clippy::too_many_arguments)]
pub async fn kb_list_documents(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
    page: Option<i64>,
    page_size: Option<i64>,
    keyword: Option<String>,
    status: Option<String>,
    tag: Option<String>,
    dir_id: Option<i64>,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_access_kb(&db, kb_id, uid) {
        return Err("无权限：你无权访问该知识库".to_string());
    }
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(50).clamp(1, 500);
    let keyword = keyword.unwrap_or_default().trim().to_string();
    let status = status.filter(|s| !s.is_empty());

    let conn = db.conn_lock();
    // 动态过滤条件：全匿名 ? 占位符 + params_from_iter，避免编号错位
    let mut conds: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(kb_id)];
    if let Some(did) = dir_id {
        conds.push("dir_id = ?".to_string());
        params.push(Box::new(did));
    }
    if !keyword.is_empty() {
        conds.push(
            "(title LIKE ? OR original_name LIKE ? OR EXISTS (SELECT 1 FROM document_chunks c WHERE c.doc_id = documents.id AND c.content LIKE ?))"
                .to_string(),
        );
        let like = format!("%{}%", keyword);
        params.push(Box::new(like.clone()));
        params.push(Box::new(like.clone()));
        params.push(Box::new(like));
    }
    if let Some(st) = status {
        conds.push("status = ?".to_string());
        params.push(Box::new(st));
    }
    if let Some(t) = tag {
        let t = t.trim().to_string();
        if !t.is_empty() {
            conds.push(
                "EXISTS (SELECT 1 FROM kb_doc_tags WHERE doc_id = documents.id AND tag = ?)"
                    .to_string(),
            );
            params.push(Box::new(t));
        }
    }
    let where_clause = if conds.is_empty() {
        String::new()
    } else {
        format!(" AND {}", conds.join(" AND "))
    };
    let base = format!("FROM documents WHERE kb_id = ?1{}", where_clause);

    let total: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) {}", base),
            params_from_iter(params.iter()),
            |r| r.get(0),
        )
        .unwrap_or(0);

    let limit = page_size;
    let offset = (page - 1) * page_size;
    let mut binds: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    binds.push(&limit);
    binds.push(&offset);
    let mut stmt = conn
        .prepare(
            &format!(
                "SELECT id, title, file_type, status, process_status, created_at, updated_at, file_size, source,
                        COALESCE((SELECT GROUP_CONCAT(tag, ',') FROM kb_doc_tags WHERE doc_id = documents.id), '') {}
                 ORDER BY updated_at DESC, id DESC LIMIT ? OFFSET ?",
                base
            ),
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map(binds.as_slice(), |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "title": row.get::<_, String>(1)?,
            "fileType": row.get::<_, Option<String>>(2)?,
            "status": row.get::<_, String>(3)?,
            "processStatus": row.get::<_, Option<String>>(4)?,
            "createdAt": row.get::<_, String>(5)?,
            "updatedAt": row.get::<_, String>(6)?,
            "fileSize": row.get::<_, Option<i64>>(7)?,
            "source": row.get::<_, Option<String>>(8)?,
            "tags": row.get::<_, String>(9)?.split(',').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect::<Vec<_>>(),
        }))
    })
    .map_err(|e| e.to_string())?;
    let items: Vec<serde_json::Value> = rows.filter_map(|r| r.ok()).collect();
    Ok(serde_json::json!({
        "items": items,
        "total": total,
        "page": page,
        "pageSize": page_size,
    }))
}

// ─── 文档查看 / 删除 ───

/// 返回文档元信息、分片列表与最新版本的文本（由 file_object 重新解析得出）
#[tauri::command]
pub async fn kb_get_document(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    doc_id: i64,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    // 先取元数据并释放锁，再做权限校验（can_access_doc 内部会再次加锁，
    // 若此时仍持有 conn 会因 Mutex 不可重入而死锁）
    let meta = {
        let conn = db.conn_lock();
        conn.query_row(
            "SELECT id, kb_id, title, original_name, file_type, status, process_status, created_at, updated_at
             FROM documents WHERE id = ?1",
            rusqlite::params![doc_id],
            |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, i64>(0)?,
                    "kbId": row.get::<_, i64>(1)?,
                    "title": row.get::<_, String>(2)?,
                    "originalName": row.get::<_, Option<String>>(3)?,
                    "fileType": row.get::<_, Option<String>>(4)?,
                    "status": row.get::<_, String>(5)?,
                    "processStatus": row.get::<_, Option<String>>(6)?,
                    "createdAt": row.get::<_, String>(7)?,
                    "updatedAt": row.get::<_, String>(8)?,
                }))
            },
        ).map_err(|e| format!("文档不存在: {}", e))?
    };

    // 权限：文档级访问校验（知识库可访问 + 文档 ACL 未拒绝）
    let doc_kb_id: i64 = meta.get("kbId").and_then(|v| v.as_i64()).unwrap_or(0);
    if !crate::kb::retrieval::can_access_doc(&db, doc_kb_id, doc_id, uid) {
        return Err("无权限：你无权访问该文档".to_string());
    }

    // 最新版本正文：从 document_versions → file_objects.blob_data 重新解析
    let conn = db.conn_lock();
    let file_type: Option<String> = conn
        .query_row(
            "SELECT file_type FROM documents WHERE id = ?1",
            rusqlite::params![doc_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();
    let content: Option<String> = {
        let fo: Option<Vec<u8>> = conn
            .query_row(
                "SELECT fo.blob_data FROM document_versions dv
                 JOIN file_objects fo ON fo.id = dv.file_object_id
                 WHERE dv.doc_id = ?1 ORDER BY dv.version_no DESC LIMIT 1",
                rusqlite::params![doc_id],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .ok();
        fo.and_then(|data| {
            let ft = file_type.clone().unwrap_or_else(|| "txt".to_string());
            parse::parse_document(&ft, &data).ok().map(|p| p.text)
        })
    };

    // 分片
    let chunks: Vec<serde_json::Value> = {
        let mut stmt = conn.prepare(
            "SELECT id, seq, content, token_count FROM document_chunks WHERE doc_id = ?1 ORDER BY seq ASC"
        ).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![doc_id], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, i64>(0)?,
                    "seq": row.get::<_, i64>(1)?,
                    "content": row.get::<_, String>(2)?,
                    "tokens": row.get::<_, i64>(3)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows.flatten() {
            out.push(r);
        }
        out
    };

    drop(conn); // 释放锁后再写埋点，避免同线程重复加锁死锁
    log_metric_event(
        &db,
        &MetricEvent {
            uid,
            event_type: "doc_view",
            kb_id: Some(doc_kb_id),
            doc_id: Some(doc_id),
            page_id: None,
            session_id: None,
            detail: None,
        },
    );
    Ok(serde_json::json!({
        "meta": meta,
        "content": content,
        "chunks": chunks,
    }))
}

/// 下载文档原始文件（返回 base64 内容、文件名与大小）
#[tauri::command]
pub async fn kb_download_document(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    doc_id: i64,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let (kb_id, title, file_type): (i64, String, Option<String>) = {
        let conn = db.conn_lock();
        conn.query_row(
            "SELECT kb_id, title, file_type FROM documents WHERE id = ?1",
            rusqlite::params![doc_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| format!("文档不存在: {}", e))?
    };
    if !crate::kb::retrieval::can_access_doc(&db, kb_id, doc_id, uid) {
        return Err("无权限：你无权访问该文档".to_string());
    }
    let blob: Option<Vec<u8>> = {
        let conn = db.conn_lock();
        conn.query_row(
            "SELECT fo.blob_data FROM document_versions dv
             JOIN file_objects fo ON fo.id = dv.file_object_id
             WHERE dv.doc_id = ?1 ORDER BY dv.version_no DESC LIMIT 1",
            rusqlite::params![doc_id],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .ok()
    };
    let data = blob.unwrap_or_default();
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
    let ext = file_type.unwrap_or_else(|| "txt".to_string());
    // 标题已带扩展名时不重复追加（如 abc.pdf 不应变成 abc.pdf.pdf）
    let base = title.trim();
    let file_name = if base
        .to_lowercase()
        .ends_with(&format!(".{}", ext.to_lowercase()))
    {
        base.to_string()
    } else {
        format!("{}.{}", base, ext)
    };
    log_metric_event(
        &db,
        &MetricEvent {
            uid,
            event_type: "doc_download",
            kb_id: Some(kb_id),
            doc_id: Some(doc_id),
            page_id: None,
            session_id: None,
            detail: None,
        },
    );
    Ok(serde_json::json!({
        "fileName": file_name,
        "fileType": ext,
        "size": data.len(),
        "dataBase64": b64,
    }))
}

/// 批量下载：将选中文档（最新版本原文件）打包为 zip 后返回 base64
#[tauri::command]
pub async fn kb_batch_download(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    doc_ids: Vec<i64>,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if doc_ids.is_empty() {
        return Err("未选择任何文档".to_string());
    }
    if doc_ids.len() > 500 {
        return Err("单次批量下载最多 500 个文档".to_string());
    }
    // 逐个读取（权限 + 最新版本原文件），跳过无法访问的文档
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for doc_id in doc_ids {
        let (kb_id, title, file_type): (i64, String, Option<String>) = {
            let conn = db.conn_lock();
            let r = conn.query_row(
                "SELECT kb_id, title, file_type FROM documents WHERE id = ?1",
                rusqlite::params![doc_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            );
            match r {
                Ok(v) => v,
                Err(_) => continue,
            }
        };
        if !crate::kb::retrieval::can_access_doc(&db, kb_id, doc_id, uid) {
            continue;
        }
        let blob: Option<Vec<u8>> = {
            let conn = db.conn_lock();
            conn.query_row(
                "SELECT fo.blob_data FROM document_versions dv
                 JOIN file_objects fo ON fo.id = dv.file_object_id
                 WHERE dv.doc_id = ?1 ORDER BY dv.version_no DESC LIMIT 1",
                rusqlite::params![doc_id],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .ok()
        };
        let data = blob.unwrap_or_default();
        let ext = file_type.unwrap_or_else(|| "txt".to_string());
        let base = title.trim();
        let mut name = if base
            .to_lowercase()
            .ends_with(&format!(".{}", ext.to_lowercase()))
        {
            base.to_string()
        } else {
            format!("{}.{}", base, ext)
        };
        // 防重名：同名文件追加序号
        if files.iter().any(|(n, _)| n == &name) {
            let stem = name
                .rsplit_once('.')
                .map(|(s, e)| (s.to_string(), format!(".{}", e)))
                .unwrap_or((name.clone(), String::new()));
            let mut i = 2usize;
            loop {
                let cand = format!("{}({}){}", stem.0, i, stem.1);
                if !files.iter().any(|(n, _)| n == &cand) {
                    name = cand;
                    break;
                }
                i += 1;
            }
        }
        files.push((name, data));
    }
    if files.is_empty() {
        return Err("没有可下载的文档（可能已被删除或无权限）".to_string());
    }
    // zip 打包（zip crate 已作为 docx 解析依赖引入；压缩为 CPU 密集，移出 tokio worker）
    let file_count = files.len();
    let bytes = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, String> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zw = zip::ZipWriter::new(cursor);
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        for (name, data) in &files {
            zw.start_file(name, opts)
                .map_err(|e| format!("写入 zip 失败: {}", e))?;
            zw.write_all(data)
                .map_err(|e| format!("写入 zip 失败: {}", e))?;
        }
        let cursor = zw.finish().map_err(|e| format!("打包失败: {}", e))?;
        Ok(cursor.into_inner())
    })
    .await
    .map_err(|e| format!("打包任务失败: {}", e))??;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(serde_json::json!({
        "count": file_count,
        "size": bytes.len(),
        "fileName": format!("知识库文档批量下载_{}.zip", chrono::Local::now().format("%Y%m%d_%H%M%S")),
        "dataBase64": b64,
    }))
}

/// 删除文档（连同其版本、分片、向量与 FTS 索引一并删除）
#[tauri::command]
pub async fn kb_delete_document(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    doc_id: i64,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    // 权限：需可管理文档所属知识库
    let kb_id: i64 = {
        let c = db.conn_lock();
        c.query_row(
            "SELECT kb_id FROM documents WHERE id = ?1",
            rusqlite::params![doc_id],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| format!("文档不存在: {}", e))?
    };
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可删除文档".to_string());
    }
    let conn = db.conn_lock();
    // 清理该文档关联 Wiki 页面的 FTS 索引（wiki_pages 由外键级联删除，但普通 FTS 表不会自动清理）
    conn.execute(
        "DELETE FROM wiki_pages_fts WHERE rowid IN (SELECT id FROM wiki_pages WHERE doc_id = ?1)",
        rusqlite::params![doc_id],
    )
    .map_err(|e| e.to_string())?;
    // 收集该文档引用的 file_object_id（在删除版本前完成，供孤儿清理）
    let fo_ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT DISTINCT file_object_id FROM document_versions WHERE doc_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![doc_id], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    // 先删除 FTS 索引行（与 document_chunks 行一一对应）
    conn.execute(
        "DELETE FROM chunks_fts WHERE rowid IN (SELECT id FROM document_chunks WHERE doc_id = ?1)",
        rusqlite::params![doc_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM document_chunks WHERE doc_id = ?1",
        rusqlite::params![doc_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM document_versions WHERE doc_id = ?1",
        rusqlite::params![doc_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM documents WHERE id = ?1",
        rusqlite::params![doc_id],
    )
    .map_err(|e| e.to_string())?;
    // 删除不再被任何版本引用的孤儿 file_objects（修复：旧逻辑在删版本后才查询，
    // 永远查不到引用，导致去重 BLOB 永久残留）
    cleanup_orphan_file_objects(&conn, &fo_ids)?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaqEntryInput {
    pub question: String,
    pub answer: String,
    pub category: Option<String>,
}

/// 导入 FAQ 问答对（按「知识库 + 问题」幂等 upsert）
#[tauri::command]
pub async fn kb_faq_import(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
    entries: Vec<FaqEntryInput>,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可导入 FAQ".to_string());
    }
    let conn = db.conn_lock();
    let mut imported = 0usize;
    for e in &entries {
        let q = e.question.trim();
        let a = e.answer.trim();
        if q.is_empty() || a.is_empty() {
            continue;
        }
        conn.execute(
            "INSERT INTO faq_entries (kb_id, question, answer, category, updated_at)
             VALUES (?1,?2,?3,?4,datetime('now'))
             ON CONFLICT(kb_id, question) DO UPDATE SET answer=excluded.answer, category=excluded.category, updated_at=datetime('now')",
            rusqlite::params![kb_id, q, a, e.category],
        )
        .map_err(|e| e.to_string())?;
        imported += 1;
    }
    Ok(serde_json::json!({ "imported": imported }))
}

/// 列出知识库内的 FAQ 问答对
#[tauri::command]
pub async fn kb_faq_list(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
) -> Result<Vec<serde_json::Value>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_access_kb(&db, kb_id, uid) {
        return Err("无权限：你无权访问该知识库".to_string());
    }
    let conn = db.conn_lock();
    let mut stmt = conn
        .prepare(
            "SELECT id, question, answer, category, updated_at FROM faq_entries WHERE kb_id = ?1 ORDER BY id DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![kb_id], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "question": r.get::<_, String>(1)?,
                "answer": r.get::<_, String>(2)?,
                "category": r.get::<_, Option<String>>(3)?,
                "updatedAt": r.get::<_, String>(4)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 删除 FAQ 问答对
#[tauri::command]
pub async fn kb_faq_delete(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
    entry_id: i64,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可删除 FAQ".to_string());
    }
    let conn = db.conn_lock();
    conn.execute(
        "DELETE FROM faq_entries WHERE id = ?1 AND kb_id = ?2",
        rusqlite::params![entry_id, kb_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// FAQ 匹配：问题相同或互相包含（≥4 字）时命中，取问题最长者
pub(crate) fn faq_match(
    conn: &rusqlite::Connection,
    kb_id: i64,
    query: &str,
) -> Option<(String, String)> {
    let q = query.trim();
    if q.len() < 2 {
        return None;
    }
    let ql = q.to_lowercase();
    let mut stmt = conn
        .prepare(
            "SELECT question, answer FROM faq_entries WHERE kb_id = ?1 ORDER BY LENGTH(question) DESC",
        )
        .ok()?;
    let rows = stmt
        .query_map(rusqlite::params![kb_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .ok()?;
    for r in rows.flatten() {
        let qq = r.0.trim();
        let aa = r.1.trim();
        if qq.is_empty() || aa.is_empty() {
            continue;
        }
        let qql = qq.to_lowercase();
        if ql == qql
            || (qql.len() >= 4 && ql.contains(&qql))
            || (ql.len() >= 4 && qql.contains(&ql))
        {
            return Some((qq.to_string(), aa.to_string()));
        }
    }
    None
}
