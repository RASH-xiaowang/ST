// ============================================================
// 知识库管理 — 导出/导入
// 支持将整个知识库（元数据 + 文档 + 分片 + Wiki 页面）导出为 JSON，
// 原始文件以 base64 编码嵌入，打包为单个 JSON 文件下载。
// ============================================================

use crate::kb::db::KbDatabase;
use tauri::State;

/// 导出知识库为 JSON（包含全部元数据、文档、分片、Wiki 页面）
/// 返回 base64 编码的 JSON 内容，前端可直接下载
#[tauri::command]
pub async fn kb_export(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_access_kb(&db, kb_id, uid) {
        return Err("无权限：你无权访问该知识库".to_string());
    }

    let conn = db.conn_lock();

    // 1. 知识库元数据
    let kb_meta: serde_json::Value = conn
        .query_row(
            "SELECT id, name, description, owner_id, pinned, is_system, created_at, updated_at
             FROM knowledge_bases WHERE id = ?1",
            rusqlite::params![kb_id],
            |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "name": r.get::<_, String>(1)?,
                    "description": r.get::<_, Option<String>>(2)?,
                    "ownerId": r.get::<_, Option<i64>>(3)?,
                    "pinned": r.get::<_, i64>(4)? != 0,
                    "isSystem": r.get::<_, i64>(5)? != 0,
                    "createdAt": r.get::<_, String>(6)?,
                    "updatedAt": r.get::<_, String>(7)?,
                }))
            },
        )
        .map_err(|e| format!("知识库不存在: {}", e))?;

    // 2. 目录
    let dirs: Vec<serde_json::Value> = {
        let mut stmt = conn
            .prepare("SELECT id, parent_id, name, created_at FROM kb_directories WHERE kb_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![kb_id], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "parentId": r.get::<_, Option<i64>>(1)?,
                    "name": r.get::<_, String>(2)?,
                    "createdAt": r.get::<_, String>(3)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // 3. 文档 + 版本 + 文件对象
    let mut documents = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, dir_id, title, original_name, file_type, file_size, source, hash, status, process_status, created_at, updated_at
                 FROM documents WHERE kb_id = ?1",
            )
            .map_err(|e| e.to_string())?;
        // 与文件内其他查询保持一致：逐行映射为 JSON，避免巨型元组类型
        let doc_rows: Vec<serde_json::Value> = stmt
            .query_map(rusqlite::params![kb_id], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "dirId": r.get::<_, Option<i64>>(1)?,
                    "title": r.get::<_, String>(2)?,
                    "originalName": r.get::<_, Option<String>>(3)?,
                    "fileType": r.get::<_, Option<String>>(4)?,
                    "fileSize": r.get::<_, Option<i64>>(5)?,
                    "source": r.get::<_, String>(6)?,
                    "hash": r.get::<_, Option<String>>(7)?,
                    "status": r.get::<_, String>(8)?,
                    "processStatus": r.get::<_, Option<String>>(9)?,
                    "createdAt": r.get::<_, String>(10)?,
                    "updatedAt": r.get::<_, String>(11)?,
                }))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        for doc in doc_rows {
            let doc_id = doc.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let dir_id = doc.get("dirId").and_then(|v| v.as_i64());
            let title = doc
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let orig_name = doc
                .get("originalName")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let file_type = doc
                .get("fileType")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let file_size = doc.get("fileSize").and_then(|v| v.as_i64());
            let source = doc
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let hash = doc
                .get("hash")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let status = doc
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let process_status = doc
                .get("processStatus")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let created_at = doc
                .get("createdAt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let updated_at = doc
                .get("updatedAt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // 标签
            let tags: Vec<String> = {
                let mut s = conn
                    .prepare("SELECT tag FROM kb_doc_tags WHERE doc_id = ?1")
                    .map_err(|e| e.to_string())?;
                let rows = s
                    .query_map(rusqlite::params![doc_id], |r| r.get::<_, String>(0))
                    .map_err(|e| e.to_string())?;
                rows.filter_map(|r| r.ok()).collect()
            };

            // 版本 + 文件对象（base64）
            let versions: Vec<serde_json::Value> = {
                let mut vs = conn.prepare(
                    "SELECT dv.id, dv.version_no, dv.note, dv.created_at, fo.hash, fo.ext, fo.size, fo.blob_data
                     FROM document_versions dv LEFT JOIN file_objects fo ON fo.id = dv.file_object_id
                     WHERE dv.doc_id = ?1"
                ).map_err(|e| e.to_string())?;
                let v_rows = vs
                    .query_map(rusqlite::params![doc_id], |r| {
                        let blob: Option<Vec<u8>> = r.get(7)?;
                        let b64 = blob.map(|b| {
                            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &b)
                        });
                        Ok(serde_json::json!({
                            "id": r.get::<_, i64>(0)?,
                            "versionNo": r.get::<_, i64>(1)?,
                            "note": r.get::<_, Option<String>>(2)?,
                            "createdAt": r.get::<_, String>(3)?,
                            "fileHash": r.get::<_, Option<String>>(4)?,
                            "fileExt": r.get::<_, Option<String>>(5)?,
                            "fileSize": r.get::<_, Option<i64>>(6)?,
                            "fileDataBase64": b64,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                v_rows.filter_map(|r| r.ok()).collect()
            };

            // 分片（不含向量，导入后重新处理）
            let chunks: Vec<serde_json::Value> = {
                let mut cs = conn.prepare(
                    "SELECT id, seq, content, page_no, section, char_start, char_end, token_count, parent_id
                     FROM document_chunks WHERE doc_id = ?1 ORDER BY seq"
                ).map_err(|e| e.to_string())?;
                let c_rows = cs
                    .query_map(rusqlite::params![doc_id], |r| {
                        Ok(serde_json::json!({
                            "id": r.get::<_, i64>(0)?,
                            "seq": r.get::<_, i64>(1)?,
                            "content": r.get::<_, String>(2)?,
                            "pageNo": r.get::<_, Option<i64>>(3)?,
                            "section": r.get::<_, Option<String>>(4)?,
                            "charStart": r.get::<_, i64>(5)?,
                            "charEnd": r.get::<_, i64>(6)?,
                            "tokenCount": r.get::<_, i64>(7)?,
                            "parentId": r.get::<_, Option<i64>>(8)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                c_rows.filter_map(|r| r.ok()).collect()
            };

            documents.push(serde_json::json!({
                "id": doc_id,
                "dirId": dir_id,
                "title": title,
                "originalName": orig_name,
                "fileType": file_type,
                "fileSize": file_size,
                "source": source,
                "hash": hash,
                "status": status,
                "processStatus": process_status,
                "tags": tags,
                "createdAt": created_at,
                "updatedAt": updated_at,
                "versions": versions,
                "chunks": chunks,
            }));
        }
    }

    // 4. Wiki 页面
    let wiki_pages: Vec<serde_json::Value> = {
        let mut stmt = conn.prepare(
            "SELECT id, dir_id, doc_id, title, slug, summary, content_md, status, extract_status, created_at, updated_at
             FROM wiki_pages WHERE kb_id = ?1"
        ).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![kb_id], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "dirId": r.get::<_, Option<i64>>(1)?,
                    "docId": r.get::<_, Option<i64>>(2)?,
                    "title": r.get::<_, String>(3)?,
                    "slug": r.get::<_, String>(4)?,
                    "summary": r.get::<_, String>(5)?,
                    "contentMd": r.get::<_, String>(6)?,
                    "status": r.get::<_, String>(7)?,
                    "extractStatus": r.get::<_, String>(8)?,
                    "createdAt": r.get::<_, String>(9)?,
                    "updatedAt": r.get::<_, String>(10)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // 5. Wiki 链接
    let wiki_links: Vec<serde_json::Value> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, from_page_id, to_page_id, link_type, weight, created_at
             FROM wiki_links WHERE kb_id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![kb_id], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "fromPageId": r.get::<_, i64>(1)?,
                    "toPageId": r.get::<_, i64>(2)?,
                    "linkType": r.get::<_, String>(3)?,
                    "weight": r.get::<_, f64>(4)?,
                    "createdAt": r.get::<_, String>(5)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // 6. FAQ
    let faqs: Vec<serde_json::Value> = {
        let mut stmt = conn.prepare(
            "SELECT id, question, answer, category, created_at FROM faq_entries WHERE kb_id = ?1"
        ).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![kb_id], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "question": r.get::<_, String>(1)?,
                    "answer": r.get::<_, String>(2)?,
                    "category": r.get::<_, Option<String>>(3)?,
                    "createdAt": r.get::<_, String>(4)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    drop(conn);

    // 组装导出包
    let export_data = serde_json::json!({
        "version": 1,
        "exportedAt": chrono::Utc::now().to_rfc3339(),
        "knowledgeBase": kb_meta,
        "directories": dirs,
        "documents": documents,
        "wikiPages": wiki_pages,
        "wikiLinks": wiki_links,
        "faqs": faqs,
    });

    // 序列化为 JSON 字符串并 base64 编码
    let json_str = serde_json::to_string_pretty(&export_data).map_err(|e| e.to_string())?;
    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        json_str.as_bytes(),
    );

    Ok(serde_json::json!({
        "dataBase64": b64,
        "fileName": format!("kb_export_{}.json", kb_id),
        "sizeBytes": json_str.len(),
    }))
}

/// 导入知识库（从导出的 JSON 包恢复）
/// 返回新建的知识库 ID
#[tauri::command]
pub async fn kb_import(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    data_base64: String,
    new_name: Option<String>,
) -> Result<serde_json::Value, String> {
    use base64::Engine;
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;

    // 解码 base64 → JSON
    let json_bytes = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("base64 解码失败: {}", e))?;
    let data: serde_json::Value =
        serde_json::from_slice(&json_bytes).map_err(|e| format!("JSON 解析失败: {}", e))?;

    let version = data.get("version").and_then(|v| v.as_i64()).unwrap_or(0);
    if version < 1 {
        return Err("不支持的导出格式版本".to_string());
    }

    let kb_meta = data
        .get("knowledgeBase")
        .ok_or("导出包缺少 knowledgeBase 字段")?;
    let kb_name = new_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            kb_meta
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("导入的知识库")
                .to_string()
        });
    let kb_desc = kb_meta
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let conn = db.conn_lock();

    // 1. 创建知识库
    conn.execute(
        "INSERT INTO knowledge_bases (name, description, owner_id) VALUES (?1,?2,?3)",
        rusqlite::params![kb_name, kb_desc, uid],
    )
    .map_err(|e| e.to_string())?;
    let new_kb_id = conn.last_insert_rowid();

    // 2. 导入目录（保留旧→新 id 映射）
    let mut dir_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    if let Some(dirs) = data.get("directories").and_then(|v| v.as_array()) {
        // 需要两次遍历：先建根目录，再建子目录（parent_id 依赖已存在的目录）
        let mut remaining = dirs.clone();
        let mut prev_len = remaining.len() + 1;
        while !remaining.is_empty() && remaining.len() < prev_len {
            prev_len = remaining.len();
            let mut next = Vec::new();
            for d in remaining {
                let old_id = d.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
                let old_parent = d.get("parentId").and_then(|v| v.as_i64());
                let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("未命名");
                // 父目录必须已导入（或无父目录）
                let new_parent = old_parent.and_then(|p| dir_map.get(&p).copied());
                if old_parent.is_some() && new_parent.is_none() {
                    next.push(d.clone());
                    continue;
                }
                conn.execute(
                    "INSERT INTO kb_directories (kb_id, parent_id, name) VALUES (?1,?2,?3)",
                    rusqlite::params![new_kb_id, new_parent, name],
                )
                .map_err(|e| e.to_string())?;
                dir_map.insert(old_id, conn.last_insert_rowid());
            }
            remaining = next;
        }
    }

    // 3. 导入文档 + 版本 + 分片
    let mut doc_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    if let Some(docs) = data.get("documents").and_then(|v| v.as_array()) {
        for doc in docs {
            let old_doc_id = doc.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let old_dir_id = doc.get("dirId").and_then(|v| v.as_i64());
            let new_dir_id = old_dir_id.and_then(|d| dir_map.get(&d).copied());
            let title = doc
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("无标题");
            let file_type = doc
                .get("fileType")
                .and_then(|v| v.as_str())
                .unwrap_or("txt");
            let file_size = doc.get("fileSize").and_then(|v| v.as_i64());
            let source = doc
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("upload");
            let hash = doc.get("hash").and_then(|v| v.as_str());

            conn.execute(
                "INSERT INTO documents (kb_id, dir_id, title, file_type, file_size, source, hash, status, process_status, created_by)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,'ready','ready',?8)",
                rusqlite::params![new_kb_id, new_dir_id, title, file_type, file_size, source, hash, uid],
            ).map_err(|e| e.to_string())?;
            let new_doc_id = conn.last_insert_rowid();
            doc_map.insert(old_doc_id, new_doc_id);

            // 标签
            if let Some(tags) = doc.get("tags").and_then(|v| v.as_array()) {
                for tag in tags {
                    if let Some(t) = tag.as_str() {
                        let _ = conn.execute(
                            "INSERT OR IGNORE INTO kb_doc_tags (doc_id, tag) VALUES (?1,?2)",
                            rusqlite::params![new_doc_id, t],
                        );
                    }
                }
            }

            // 版本 + 文件对象
            let mut last_version_id: Option<i64> = None;
            if let Some(versions) = doc.get("versions").and_then(|v| v.as_array()) {
                for ver in versions {
                    let ver_no = ver.get("versionNo").and_then(|v| v.as_i64()).unwrap_or(1);
                    let note = ver.get("note").and_then(|v| v.as_str());
                    let file_b64 = ver.get("fileDataBase64").and_then(|v| v.as_str());
                    let file_hash = ver.get("fileHash").and_then(|v| v.as_str()).unwrap_or("");
                    let file_ext = ver.get("fileExt").and_then(|v| v.as_str()).unwrap_or("");

                    let fo_id = if let Some(b64) = file_b64 {
                        let blob = base64::engine::general_purpose::STANDARD
                            .decode(b64)
                            .unwrap_or_default();
                        let fsize = blob.len() as i64;
                        conn.execute(
                            "INSERT INTO file_objects (hash, ext, size, blob_data) VALUES (?1,?2,?3,?4)
                             ON CONFLICT(hash) DO NOTHING",
                            rusqlite::params![file_hash, file_ext, fsize, blob],
                        ).map_err(|e| e.to_string())?;
                        conn.query_row(
                            "SELECT id FROM file_objects WHERE hash = ?1",
                            rusqlite::params![file_hash],
                            |r| r.get::<_, i64>(0),
                        )
                        .ok()
                    } else {
                        None
                    };

                    conn.execute(
                        "INSERT INTO document_versions (doc_id, version_no, file_object_id, note, created_by)
                         VALUES (?1,?2,?3,?4,?5)",
                        rusqlite::params![new_doc_id, ver_no, fo_id, note, uid],
                    ).map_err(|e| e.to_string())?;
                    let vid = conn.last_insert_rowid();
                    last_version_id = Some(vid);
                }
            }
            // 更新 current_version_id（指向最新版本，非第一个版本）
            if let Some(vid) = last_version_id {
                let _ = conn.execute(
                    "UPDATE documents SET current_version_id = ?1 WHERE id = ?2",
                    rusqlite::params![vid, new_doc_id],
                );
            }

            // 分片（不含向量，需后续重新处理）
            if let Some(chunks) = doc.get("chunks").and_then(|v| v.as_array()) {
                for chunk in chunks {
                    let seq = chunk.get("seq").and_then(|v| v.as_i64()).unwrap_or(0);
                    let content = chunk.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    let page_no = chunk.get("pageNo").and_then(|v| v.as_i64());
                    let section = chunk.get("section").and_then(|v| v.as_str());
                    let char_start = chunk.get("charStart").and_then(|v| v.as_i64()).unwrap_or(0);
                    let char_end = chunk.get("charEnd").and_then(|v| v.as_i64()).unwrap_or(0);
                    let token_count = chunk
                        .get("tokenCount")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    conn.execute(
                        "INSERT INTO document_chunks (kb_id, doc_id, version_id, seq, content, page_no, section, char_start, char_end, token_count)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                        rusqlite::params![new_kb_id, new_doc_id, last_version_id, seq, content, page_no, section, char_start, char_end, token_count],
                    ).map_err(|e| e.to_string())?;
                    let chunk_id = conn.last_insert_rowid();
                    // FTS 索引
                    let _ = crate::kb::db::fts_insert_chunk(&conn, chunk_id, content);
                }
            }
        }
    }

    // 4. 导入 Wiki 页面
    let mut page_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    if let Some(pages) = data.get("wikiPages").and_then(|v| v.as_array()) {
        for page in pages {
            let old_id = page.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let old_doc_id = page.get("docId").and_then(|v| v.as_i64());
            let new_doc_id = old_doc_id.and_then(|d| doc_map.get(&d).copied());
            let title = page
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("无标题");
            let slug = page.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            let summary = page.get("summary").and_then(|v| v.as_str()).unwrap_or("");
            let content_md = page.get("contentMd").and_then(|v| v.as_str()).unwrap_or("");
            let status = page
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("draft");

            conn.execute(
                "INSERT INTO wiki_pages (kb_id, doc_id, title, slug, summary, content_md, status, created_by)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                rusqlite::params![new_kb_id, new_doc_id, title, slug, summary, content_md, status, uid],
            ).map_err(|e| e.to_string())?;
            let new_page_id = conn.last_insert_rowid();
            page_map.insert(old_id, new_page_id);
            let _ =
                crate::kb::db::fts_insert_wiki_page(&conn, new_page_id, title, summary, content_md);
        }
    }

    // 5. 导入 Wiki 链接
    if let Some(links) = data.get("wikiLinks").and_then(|v| v.as_array()) {
        for link in links {
            let from_old = link.get("fromPageId").and_then(|v| v.as_i64()).unwrap_or(0);
            let to_old = link.get("toPageId").and_then(|v| v.as_i64()).unwrap_or(0);
            let from_new = page_map.get(&from_old).copied();
            let to_new = page_map.get(&to_old).copied();
            if let (Some(f), Some(t)) = (from_new, to_new) {
                let link_type = link
                    .get("linkType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("related");
                let weight = link.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO wiki_links (kb_id, from_page_id, to_page_id, link_type, weight)
                     VALUES (?1,?2,?3,?4,?5)",
                    rusqlite::params![new_kb_id, f, t, link_type, weight],
                );
            }
        }
    }

    // 6. 导入 FAQ
    if let Some(faqs) = data.get("faqs").and_then(|v| v.as_array()) {
        for faq in faqs {
            let question = faq.get("question").and_then(|v| v.as_str()).unwrap_or("");
            let answer = faq.get("answer").and_then(|v| v.as_str()).unwrap_or("");
            let category = faq.get("category").and_then(|v| v.as_str());
            let _ = conn.execute(
                "INSERT OR IGNORE INTO faq_entries (kb_id, question, answer, category) VALUES (?1,?2,?3,?4)",
                rusqlite::params![new_kb_id, question, answer, category],
            );
        }
    }

    drop(conn);

    Ok(serde_json::json!({
        "kbId": new_kb_id,
        "name": kb_name,
        "documents": doc_map.len(),
        "wikiPages": page_map.len(),
    }))
}
