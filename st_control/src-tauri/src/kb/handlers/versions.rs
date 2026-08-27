// ============================================================
// 知识库管理 — 版本控制
// 自 handlers.rs 拆分：版本列表 / 行级差异对比 / 历史版本回滚。
// ============================================================

use crate::kb::db::KbDatabase;
use crate::kb::embed;
use crate::kb::parse::{self, Chunk, ChunkConfig};
use serde::Serialize;
use tauri::State;

use super::{refresh_wiki_for_doc, resolve_embedding_pair};

#[derive(Serialize)]
#[allow(non_snake_case)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub id: i64,
    pub version_no: i64,
    pub note: Option<String>,
    pub created_by: Option<i64>,
    pub created_at: String,
}

#[tauri::command]
pub async fn kb_list_versions(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    doc_id: i64,
) -> Result<Vec<VersionInfo>, String> {
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
    if !crate::kb::retrieval::can_access_doc(&db, kb_id, doc_id, uid) {
        return Err("无权限：你无权访问该文档".to_string());
    }
    let conn = db.conn_lock();
    let mut stmt = conn.prepare(
        "SELECT id, version_no, note, created_by, created_at FROM document_versions WHERE doc_id = ?1 ORDER BY version_no DESC"
    ).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![doc_id], |row| {
            Ok(VersionInfo {
                id: row.get(0)?,
                version_no: row.get(1)?,
                note: row.get(2)?,
                created_by: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 按行 LCS diff：返回 (新增行, 删除行)，b 相对 a 的变化
fn line_diff(a: &[String], b: &[String]) -> (Vec<String>, Vec<String>) {
    let n = a.len();
    let m = b.len();
    let mut removed: Vec<String> = Vec::new();
    let mut added: Vec<String> = Vec::new();
    // 大文档降级：剥离公共前后缀后直接列出中间差异
    if (n as u64).saturating_mul(m as u64) > 4_000_000 {
        let mut i = 0;
        while i < n && i < m && a[i] == b[i] {
            i += 1;
        }
        let mut j = 0;
        while j < n.saturating_sub(i) && j < m.saturating_sub(i) && a[n - 1 - j] == b[m - 1 - j] {
            j += 1;
        }
        removed.extend_from_slice(&a[i..n.saturating_sub(j)]);
        added.extend_from_slice(&b[i..m.saturating_sub(j)]);
        return (added, removed);
    }
    // LCS DP
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            removed.push(a[i].clone());
            i += 1;
        } else {
            added.push(b[j].clone());
            j += 1;
        }
    }
    while i < n {
        removed.push(a[i].clone());
        i += 1;
    }
    while j < m {
        added.push(b[j].clone());
        j += 1;
    }
    (added, removed)
}

#[derive(Serialize)]
#[allow(non_snake_case)]
#[serde(rename_all = "camelCase")]
pub struct VersionDiff {
    pub from_version_no: i64,
    pub to_version_no: i64,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// 对比文档两个版本的内容差异（按行高亮新增/删除）
#[tauri::command]
pub async fn kb_version_diff(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    doc_id: i64,
    from_version_id: i64,
    to_version_id: i64,
) -> Result<VersionDiff, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let kb_id: i64 = {
        let c = db.conn_lock();
        c.query_row(
            "SELECT kb_id FROM documents WHERE id = ?1",
            rusqlite::params![doc_id],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|_| "文档不存在".to_string())?
    };
    if !crate::kb::retrieval::can_access_doc(&db, kb_id, doc_id, uid) {
        return Err("无权限：你无权访问该文档".to_string());
    }
    // 版本对比涉及两轮完整文档解析 + 行级 diff（CPU 密集），移出 tokio worker
    let (from_no, to_no, added, removed) = {
        let db_block = (*db).clone();
        tauri::async_runtime::spawn_blocking(
            move || -> Result<(i64, i64, Vec<String>, Vec<String>), String> {
                let conn = db_block.conn_lock();
                let fetch = |conn: &rusqlite::Connection,
                             version_id: i64|
                 -> Result<(i64, Vec<String>), String> {
                    let version_no: i64 = conn.query_row(
                    "SELECT version_no FROM document_versions WHERE id = ?1 AND doc_id = ?2",
                    rusqlite::params![version_id, doc_id],
                    |r| r.get(0),
                ).map_err(|_| "版本不存在".to_string())?;
                    // 旧版本的分片在重新处理/回滚时会被清空，因此直接读该版本的原始文件重新解析，
                    // 保证任意两个版本都能正确对比（file_objects.ext 记录了该版本当时的文件类型）
                    let row: Option<(String, Vec<u8>)> = conn
                        .query_row(
                            "SELECT COALESCE(fo.ext, 'txt'), fo.blob_data FROM document_versions dv
                         JOIN file_objects fo ON fo.id = dv.file_object_id
                         WHERE dv.id = ?1 AND dv.doc_id = ?2",
                            rusqlite::params![version_id, doc_id],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .ok();
                    let (file_type, blob) = row.ok_or_else(|| "版本缺少原始文件".to_string())?;
                    let text = parse::parse_document(&file_type, &blob)
                        .map(|p| p.text)
                        .unwrap_or_default();
                    Ok((version_no, text.lines().map(|l| l.to_string()).collect()))
                };
                let (from_no, from_lines) = fetch(&conn, from_version_id)?;
                let (to_no, to_lines) = fetch(&conn, to_version_id)?;
                let (added, removed) = line_diff(&from_lines, &to_lines);
                Ok((from_no, to_no, added, removed))
            },
        )
        .await
        .map_err(|e| format!("版本对比任务失败: {}", e))??
    };
    Ok(VersionDiff {
        from_version_no: from_no,
        to_version_no: to_no,
        added,
        removed,
    })
}

/// 将指定历史版本回滚为新的最新版本：重新解析该版本文件 → 切片 → 嵌向量
#[tauri::command]
pub async fn kb_restore_version(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    version_id: i64,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    // 1) 读取目标版本对应的原始文件
    let (doc_id, file_type, blob, note): (i64, String, Vec<u8>, String) = {
        let conn = db.conn_lock();
        conn.query_row(
            "SELECT dv.doc_id, COALESCE(d.file_type,''), fo.blob_data, COALESCE(dv.note,'')
             FROM document_versions dv
             JOIN documents d ON d.id = dv.doc_id
             JOIN file_objects fo ON fo.id = dv.file_object_id
             WHERE dv.id = ?1",
            rusqlite::params![version_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(|e| format!("版本不存在或缺少原始文件: {}", e))?
    };

    // 2) 重新解析 + 切片（CPU 密集，移出 tokio worker）
    let chunks = {
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<Chunk>, String> {
            let parsed = parse::parse_document(&file_type, &blob).map_err(|e| e.to_string())?;
            let cfg = ChunkConfig::default();
            let chunks = parse::chunk_text(&parsed.text, &cfg);
            if chunks.is_empty() {
                return Err("回滚内容为空，无法生成分片".to_string());
            }
            Ok(chunks)
        })
        .await
        .map_err(|e| format!("回滚解析任务失败: {}", e))??
    };

    // 3) 计算下一版本号
    let next_version: i64 = {
        let conn = db.conn_lock();
        conn.query_row(
            "SELECT COALESCE(MAX(version_no), 0) + 1 FROM document_versions WHERE doc_id = ?1",
            rusqlite::params![doc_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())?
    };

    // 4) 权限校验 + 读取目标版本文件与版本号（不持有 conn 锁，避免 Mutex 死锁）
    let kb_id = db_kb_id(&db, doc_id);
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可回滚版本".to_string());
    }
    let (file_object_id, target_version_no): (i64, i64) = {
        let conn = db.conn_lock();
        conn.query_row(
            "SELECT file_object_id, version_no FROM document_versions WHERE id = ?1",
            rusqlite::params![version_id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .map_err(|e| format!("版本不存在: {}", e))?
    };

    // 5) 写入新版本（复用同一 file_object_id）
    let new_version_id = {
        let conn = db.conn_lock();
        conn.execute(
            "INSERT INTO document_versions (doc_id, version_no, file_object_id, note, created_by)
             VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params![
                doc_id,
                next_version,
                file_object_id,
                format!("回滚自 v{}（{}）", target_version_no, note.trim()),
                uid
            ],
        )
        .map_err(|e| e.to_string())?;
        conn.last_insert_rowid()
    };

    // 6) 创建处理任务并清理旧分片（save_chunks 内部会加锁，先释放外层锁）
    let job_id = {
        let conn = db.conn_lock();
        crate::kb::db::fts_delete_chunks_by_doc(&conn, doc_id)?;
        conn.execute(
            "DELETE FROM document_chunks WHERE doc_id = ?1",
            rusqlite::params![doc_id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE documents SET status='processing', process_status = 'embedding', updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![doc_id],
        ).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO processing_jobs (doc_id, version_id, stage) VALUES (?1,?2,'embedding')",
            rusqlite::params![doc_id, new_version_id],
        )
        .map_err(|e| e.to_string())?;
        conn.last_insert_rowid()
    };
    let chunk_ids = parse::save_chunks(&db, kb_id, doc_id, new_version_id, &chunks)?;

    // 7) 嵌入向量（跨 await）
    let mut id_chunk_pairs: Vec<(i64, Chunk)> = Vec::with_capacity(chunk_ids.len());
    for (idx, id) in chunk_ids.iter().enumerate() {
        id_chunk_pairs.push((*id, chunks[idx].clone()));
    }
    let (embedding_provider, embedding_model) = resolve_embedding_pair(&db, None, None);
    let (ok, _fail, embed_dim) = match embed::embed_chunks(
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
            let conn = db.conn_lock();
            let _ = conn.execute(
                "UPDATE documents SET status='failed', process_status='failed', updated_at=datetime('now') WHERE id = ?1",
                rusqlite::params![doc_id],
            );
            let _ = conn.execute(
                "UPDATE processing_jobs SET stage='failed', progress=1.0, error=?1 WHERE id = ?2",
                rusqlite::params![e, job_id],
            );
            return Err(format!("回滚向量化失败：{}", e));
        }
    };

    // 8) 完成
    {
        let conn = db.conn_lock();
        if ok == 0 {
            conn.execute(
                "UPDATE documents SET status='failed', process_status='failed', updated_at=datetime('now') WHERE id = ?1",
                rusqlite::params![doc_id],
            ).map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE processing_jobs SET stage='failed', progress=1.0, error='回滚向量化全部失败（请检查嵌入配置）' WHERE id = ?1",
                rusqlite::params![job_id],
            ).map_err(|e| e.to_string())?;
        } else {
            conn.execute(
                "UPDATE documents SET status='ready', process_status='ready', current_version_id=?1, updated_at=datetime('now') WHERE id=?2",
                rusqlite::params![new_version_id, doc_id],
            ).map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE processing_jobs SET stage='done', progress=1.0 WHERE id = ?1",
                rusqlite::params![job_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    // record_embedding_meta 内部会加锁，必须在锁外调用
    if let Some(dim) = embed_dim {
        match embed::record_embedding_meta(&db, kb_id, "", dim) {
            Ok(Some(w)) => log::warn!("版本恢复嵌入告警: {}", w),
            Err(e) => log::warn!("记录嵌入元数据失败: {}", e),
            _ => {}
        }
    }
    // 源文档内容变化 → 自动刷新关联 Wiki 页面的摘要/实体
    if ok > 0 {
        refresh_wiki_for_doc(&db, doc_id);
    }
    Ok(())
}

/// 查询文档所属知识库 id（用于 save_chunks 写入 kb_id）
fn db_kb_id(db: &KbDatabase, doc_id: i64) -> i64 {
    let conn = db.conn_lock();
    conn.query_row(
        "SELECT kb_id FROM documents WHERE id = ?1",
        rusqlite::params![doc_id],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
}
