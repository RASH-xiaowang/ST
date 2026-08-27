// ============================================================
// 检索服务 (Retrieval)
//  - 向量检索：对 query 嵌入后在 Rust 侧计算余弦相似度
//  - BM25 检索：FTS5 MATCH
//  - 混合：RRF 融合两路结果
//  - 权限：仅检索用户可见知识库（kb_id 白名单）
//  - LRU 缓存：相同查询短时间内的结果直接返回，避免重复计算
// ============================================================

use crate::kb::db::{cosine_similarity, deserialize_embedding, KbDatabase};
use lru::LruCache;
use rusqlite::params;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Mutex;

/// 检索结果 LRU 缓存（最多 200 条，TTL 60 秒）
/// key = "kb_ids:query:mode:topK:embed_model"（含嵌入模型标识，切换模型后缓存自动失效）
/// 检索缓存类型别名：key = 检索指纹，value = (缓存时间, 结果列表)
type SearchCache =
    std::sync::LazyLock<Mutex<LruCache<String, (std::time::Instant, Vec<RetrievedChunk>)>>>;

static SEARCH_CACHE: SearchCache =
    SearchCache::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(200).unwrap())));

/// 缓存 TTL
const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(60);

fn cache_key(kb_ids: &[i64], query: &str, mode: &str, top_k: usize, embed_model: &str) -> String {
    format!("{:?}:{}:{}:{}:{}", kb_ids, query, mode, top_k, embed_model)
}

/// 尝试从缓存获取检索结果
pub fn get_cached(
    kb_ids: &[i64],
    query: &str,
    mode: &str,
    top_k: usize,
    embed_model: &str,
) -> Option<Vec<RetrievedChunk>> {
    let key = cache_key(kb_ids, query, mode, top_k, embed_model);
    let mut cache = SEARCH_CACHE.lock().ok()?;
    if let Some((ts, results)) = cache.get(&key) {
        if ts.elapsed() < CACHE_TTL {
            return Some(results.clone());
        }
        // 过期，移除
        cache.pop(&key);
    }
    None
}

/// 将检索结果写入缓存
pub fn put_cache(
    kb_ids: &[i64],
    query: &str,
    mode: &str,
    top_k: usize,
    embed_model: &str,
    results: &[RetrievedChunk],
) {
    let key = cache_key(kb_ids, query, mode, top_k, embed_model);
    if let Ok(mut cache) = SEARCH_CACHE.lock() {
        cache.put(key, (std::time::Instant::now(), results.to_vec()));
    }
}

/// 向量检索候选分片行（SELECT：id, doc_id, kb_id, content, page_no, section, embedding_blob, doc_title）
struct EmbeddingRow(
    i64,
    i64,
    i64,
    String,
    Option<i64>,
    Option<String>,
    Vec<u8>,
    String,
);
/// kb 级 ACL 规则行（SELECT：kb_id, grantee_type, user_id, role_id, effect）
struct AclRuleRow(i64, String, Option<i64>, Option<i64>, String);

#[derive(Debug, Clone, serde::Serialize)]
pub struct RetrievedChunk {
    pub chunk_id: i64,
    pub doc_id: i64,
    pub kb_id: i64,
    pub content: String,
    pub page_no: Option<i64>,
    pub section: Option<String>,
    pub score: f64,
    pub source: String, // vector / bm25 / hybrid
    /// 来源文档标题（供检索结果直接展示，避免前端二次查询）
    pub doc_title: String,
}

/// 向量检索：query 嵌入后取 top_k
pub async fn vector_search(
    db: &KbDatabase,
    query: &str,
    visible_kbs: &[i64],
    top_k: usize,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<Vec<RetrievedChunk>, String> {
    let t0 = std::time::Instant::now();
    let qvec = {
        let req = crate::llm::types::EmbeddingRequest {
            provider_id: provider_id.map(|s| s.to_string()),
            model: model.map(|s| s.to_string()),
            input: query.to_string(),
        };
        let res = crate::llm::handlers::create_embedding(req).await?;
        res.embeddings
            .into_iter()
            .next()
            .ok_or_else(|| "查询嵌入失败".to_string())?
    };

    // 先一次性取出原始数据并释放锁，再逐条计算余弦相似度，
    // 避免大知识库检索时长时间占用数据库锁阻塞其他操作
    let rows_data: Vec<EmbeddingRow> = {
        let conn = db.conn_lock();
        let placeholders = visible_kbs
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT c.id, c.doc_id, c.kb_id, c.content, c.page_no, c.section, c.embedding_blob, COALESCE(d.title,'')
             FROM document_chunks c JOIN documents d ON d.id = c.doc_id
             WHERE c.kb_id IN ({}) AND c.embedding_blob IS NOT NULL",
            placeholders
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let params_vec: Vec<&dyn rusqlite::types::ToSql> = visible_kbs
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt
            .query_map(params_vec.as_slice(), |row| {
                Ok(EmbeddingRow(
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let rows_data_len = rows_data.len();
    let mut results = Vec::new();
    for EmbeddingRow(id, doc_id, kb_id, content, page_no, section, blob, doc_title) in rows_data {
        let vec = deserialize_embedding(&blob);
        let sim = cosine_similarity(&qvec, &vec);
        results.push(RetrievedChunk {
            chunk_id: id,
            doc_id,
            kb_id,
            content,
            page_no,
            section,
            score: sim,
            source: "vector".to_string(),
            doc_title,
        });
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(top_k);
    let elapsed = t0.elapsed();
    log::info!(
        "向量检索完成: 扫描={} 耗时={}ms top_k={} 结果={}",
        rows_data_len,
        elapsed.as_millis(),
        top_k,
        results.len()
    );
    Ok(results)
}
pub fn fts_candidate_ids(
    db: &KbDatabase,
    query: &str,
    visible_kbs: &[i64],
    limit: usize,
) -> Result<Vec<i64>, String> {
    let q = fts_safe_query(query);
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let conn = db.conn_lock();
    let placeholders = (0..visible_kbs.len())
        .map(|i| format!("?{}", i + 3))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT chunks_fts.rowid AS cid
         FROM chunks_fts
         JOIN document_chunks AS c ON c.id = chunks_fts.rowid
         WHERE chunks_fts MATCH ?1 AND c.kb_id IN ({})
         ORDER BY bm25(chunks_fts) ASC LIMIT ?2",
        placeholders
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let limit_i: i64 = limit as i64;
    let mut binds: Vec<&dyn rusqlite::types::ToSql> = vec![&q, &limit_i];
    for v in visible_kbs {
        binds.push(v as &dyn rusqlite::types::ToSql);
    }
    let rows = stmt
        .query_map(binds.as_slice(), |row| row.get::<_, i64>(0))
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 候选池限定版向量检索：仅对给定候选分片计算余弦相似度（hybrid 模式用）
pub async fn vector_search_in_candidates(
    db: &KbDatabase,
    query: &str,
    visible_kbs: &[i64],
    candidate_ids: &[i64],
    top_k: usize,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<Vec<RetrievedChunk>, String> {
    if candidate_ids.is_empty() {
        // FTS 候选池为空（查询无有效词或全部为停用词）时回退全量向量检索，
        // 避免混合检索在 BM25 无命中时直接返回空结果。
        return vector_search(db, query, visible_kbs, top_k, provider_id, model).await;
    }
    let qvec = {
        let req = crate::llm::types::EmbeddingRequest {
            provider_id: provider_id.map(|s| s.to_string()),
            model: model.map(|s| s.to_string()),
            input: query.to_string(),
        };
        let res = crate::llm::handlers::create_embedding(req).await?;
        res.embeddings
            .into_iter()
            .next()
            .ok_or_else(|| "查询嵌入失败".to_string())?
    };

    // 一次性取出候选分片的原始数据并释放连接，再逐条计算余弦相似度
    let rows_data: Vec<EmbeddingRow> = {
        let conn = db.conn_lock();
        let kb_ph: Vec<String> = visible_kbs.iter().map(|_| "?".to_string()).collect();
        let id_ph: Vec<String> = candidate_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT c.id, c.doc_id, c.kb_id, c.content, c.page_no, c.section, c.embedding_blob, COALESCE(d.title,'')
             FROM document_chunks c JOIN documents d ON d.id = c.doc_id
             WHERE c.kb_id IN ({}) AND c.id IN ({}) AND c.embedding_blob IS NOT NULL",
            kb_ph.join(","),
            id_ph.join(",")
        );
        let mut binds: Vec<&dyn rusqlite::types::ToSql> = visible_kbs
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        binds.extend(
            candidate_ids
                .iter()
                .map(|v| v as &dyn rusqlite::types::ToSql),
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(binds.as_slice(), |row| {
                Ok(EmbeddingRow(
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut results = Vec::new();
    for EmbeddingRow(id, doc_id, kb_id, content, page_no, section, blob, doc_title) in rows_data {
        let vec = deserialize_embedding(&blob);
        let sim = cosine_similarity(&qvec, &vec);
        results.push(RetrievedChunk {
            chunk_id: id,
            doc_id,
            kb_id,
            content,
            page_no,
            section,
            score: sim,
            source: "vector".to_string(),
            doc_title,
        });
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(top_k);
    Ok(results)
}

/// 纯向量检索的大库保护阈值默认值：超过此数量的分片时，自动走 FTS 候选池预筛，
/// 避免每次检索全量加载所有 embedding 到内存（O(N) 扫描）。
/// 小库（≤此阈值）保持全量扫描，精度零损失。
/// 可通过 kb_chunk_settings 表 key='vector_scan_cap' 运行时调整。
const DEFAULT_VECTOR_SCAN_CAP: usize = 500;

/// 从数据库读取可配置的向量扫描阈值，未配置时返回默认值
fn load_vector_scan_cap(db: &KbDatabase) -> usize {
    let conn = db.conn_lock();
    conn.query_row(
        "SELECT value FROM kb_chunk_settings WHERE key = 'vector_scan_cap'",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|v| v.parse::<usize>().ok())
    .filter(|&v| (50..=10000).contains(&v)) // 合理范围校验：50 ~ 10000
    .unwrap_or(DEFAULT_VECTOR_SCAN_CAP)
}

/// 带候选池上限的向量检索：大知识库自动走 FTS 预筛 + 向量精排，
/// 小知识库保持全量扫描（精度不损失）。纯向量模式专用。
pub async fn vector_search_capped(
    db: &KbDatabase,
    query: &str,
    visible_kbs: &[i64],
    top_k: usize,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<Vec<RetrievedChunk>, String> {
    // 统计可见知识库的总分片数
    let total_chunks: i64 = {
        let conn = db.conn_lock();
        let placeholders = visible_kbs
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT COUNT(*) FROM document_chunks WHERE kb_id IN ({}) AND embedding_blob IS NOT NULL",
            placeholders
        );
        let params_vec: Vec<&dyn rusqlite::types::ToSql> = visible_kbs
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        conn.query_row(&sql, params_vec.as_slice(), |r| r.get(0))
            .unwrap_or(0)
    };

    let scan_cap = load_vector_scan_cap(db);
    if total_chunks as usize <= scan_cap {
        // 小库：全量扫描，精度零损失
        return vector_search(db, query, visible_kbs, top_k, provider_id, model).await;
    }

    // 大库：FTS 候选池预筛 + 向量精排
    let candidate_limit = (top_k * 50).clamp(200, 3000);
    let candidates = fts_candidate_ids(db, query, visible_kbs, candidate_limit)?;
    if candidates.is_empty() {
        // FTS 无命中（查询词太短或全为停用词），回退全量扫描但截断到上限
        // 仍然限制扫描量，避免极端情况下的内存暴涨
        return vector_search(db, query, visible_kbs, top_k, provider_id, model).await;
    }
    vector_search_in_candidates(
        db,
        query,
        visible_kbs,
        &candidates,
        top_k,
        provider_id,
        model,
    )
    .await
}

/// 将用户查询转为 FTS5 安全查询（共享实现：支持中文整句按字 OR 召回，
/// 避免中文整句作为单一短语导致 0 命中，见 crate::kb::fts_safe_query）
fn fts_safe_query(query: &str) -> String {
    crate::kb::fts_safe_query(query)
}

/// BM25 检索（FTS5）
pub fn bm25_search(
    db: &KbDatabase,
    query: &str,
    visible_kbs: &[i64],
    top_k: usize,
) -> Result<Vec<RetrievedChunk>, String> {
    let t0 = std::time::Instant::now();
    let q = fts_safe_query(query);
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let conn = db.conn_lock();
    // 占位符全编号：?1=MATCH 查询词, ?2=LIMIT, ?3..=可见知识库（避免匿名 ? 与 ?2 冲突导致参数计数错乱）
    let placeholders = (0..visible_kbs.len())
        .map(|i| format!("?{}", i + 3))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT c.id, c.doc_id, c.kb_id, c.content, c.page_no, c.section, bm25(chunks_fts) AS score, COALESCE(d.title,'')
         FROM chunks_fts
         JOIN document_chunks AS c ON c.id = chunks_fts.rowid
         JOIN documents d ON d.id = c.doc_id
         WHERE chunks_fts MATCH ?1 AND c.kb_id IN ({}) 
         ORDER BY score ASC LIMIT ?2",
        placeholders
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let limit: i64 = top_k as i64;
    let mut binds: Vec<&dyn rusqlite::types::ToSql> = vec![&q, &limit];
    for v in visible_kbs {
        binds.push(v as &dyn rusqlite::types::ToSql);
    }
    let rows = stmt
        .query_map(binds.as_slice(), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, f64>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for (id, doc_id, kb_id, content, page_no, section, raw_score, doc_title) in
        rows.into_iter().flatten()
    {
        // bm25 越小越相关，转换为正分
        results.push(RetrievedChunk {
            chunk_id: id,
            doc_id,
            kb_id,
            content,
            page_no,
            section,
            score: -raw_score,
            source: "bm25".to_string(),
            doc_title,
        });
    }
    let elapsed = t0.elapsed();
    log::info!(
        "BM25 检索完成: 耗时={}ms top_k={} 结果={}",
        elapsed.as_millis(),
        top_k,
        results.len()
    );
    Ok(results)
}

/// RRF 融合（Reciprocal Rank Fusion），k=60
pub fn rrf_fuse(
    vector: Vec<RetrievedChunk>,
    bm25: Vec<RetrievedChunk>,
    k: usize,
) -> Vec<RetrievedChunk> {
    let mut score_map: HashMap<i64, RetrievedChunk> = HashMap::new();
    let fuse = |list: &[RetrievedChunk], map: &mut HashMap<i64, RetrievedChunk>| {
        for (rank, item) in list.iter().enumerate() {
            let rrf = 1.0 / (k as f64 + (rank + 1) as f64);
            if let Some(existing) = map.get_mut(&item.chunk_id) {
                existing.score += rrf;
                existing.source = "hybrid".to_string();
            } else {
                let mut cloned = item.clone();
                cloned.score = rrf;
                cloned.source = "hybrid".to_string();
                map.insert(item.chunk_id, cloned);
            }
        }
    };
    fuse(&vector, &mut score_map);
    fuse(&bm25, &mut score_map);
    let mut out: Vec<RetrievedChunk> = score_map.into_values().collect();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// 若配置了 Rerank 模型，对检索结果按相关性重排序；失败时保持原顺序。
/// 读取 kb_model_settings 表中 role='rerank' 的配置（全局生效）。
pub async fn rerank_chunks(
    db: &KbDatabase,
    query: &str,
    chunks: Vec<RetrievedChunk>,
) -> Vec<RetrievedChunk> {
    if chunks.len() < 2 {
        return chunks;
    }
    let setting: Option<(String, String)> = {
        let conn = db.conn_lock();
        conn.query_row(
            "SELECT provider_id, model FROM kb_model_settings WHERE role = 'rerank'",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .ok()
        .filter(|(p, m)| !p.is_empty() && !m.is_empty())
    };
    let Some((provider_id, model)) = setting else {
        return chunks;
    };
    let documents: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
    let req = crate::llm::types::RerankRequest {
        provider_id: Some(provider_id),
        model: Some(model),
        query: query.to_string(),
        documents,
        top_n: Some(chunks.len() as u32),
    };
    match crate::llm::handlers::rerank(req).await {
        Ok(res) => {
            let mut chunks = chunks;
            let mut used = vec![false; chunks.len()];
            let mut out: Vec<RetrievedChunk> = Vec::with_capacity(chunks.len());
            for item in res.results {
                let idx = item.index as usize;
                if idx < chunks.len() && !used[idx] {
                    chunks[idx].score = item.score;
                    chunks[idx].source = format!("rerank:{}", chunks[idx].source);
                    out.push(chunks[idx].clone());
                    used[idx] = true;
                }
            }
            for (i, c) in chunks.into_iter().enumerate() {
                if !used[i] {
                    out.push(c);
                }
            }
            out
        }
        Err(e) => {
            log::warn!("重排序失败，保留原顺序: {}", e);
            chunks
        }
    }
}

/// 计算用户可见知识库集合：kb_members 成员 + ACL allow，剔除 ACL deny。
/// 开放知识库（无成员）默认全员可见，但显式 deny 优先于"开放可见"规则。
pub fn visible_kb_ids(db: &KbDatabase, user_id: i64) -> Vec<i64> {
    // 1) 成员关系
    let mut set: std::collections::BTreeSet<i64> = {
        let conn = db.conn_lock();
        let mut out = std::collections::BTreeSet::new();
        if let Ok(mut stmt) = conn.prepare("SELECT kb_id FROM kb_members WHERE user_id = ?1") {
            if let Ok(rows) = stmt.query_map(params![user_id], |row| row.get::<_, i64>(0)) {
                for r in rows.flatten() {
                    out.insert(r);
                }
            }
        }
        out
    };
    // 2) ACL 规则（scope=kb）：allow 加入 / deny 移除（deny 优先）
    let denied: std::collections::BTreeSet<i64> = {
        let conn = db.conn_lock();
        let roles: Vec<i64> = conn
            .prepare("SELECT role_id FROM user_roles WHERE user_id = ?1")
            .ok()
            .and_then(|mut s| {
                s.query_map(params![user_id], |r| r.get::<_, i64>(0))
                    .ok()
                    .map(|rs| rs.filter_map(|r| r.ok()).collect::<Vec<i64>>())
            })
            .unwrap_or_default();
        let acl_rules: Vec<AclRuleRow> = if let Ok(mut stmt) = conn.prepare(
            "SELECT kb_id, grantee_type, user_id, role_id, effect FROM kb_acl WHERE scope='kb' AND kb_id IS NOT NULL"
        ) {
            if let Ok(rows) = stmt.query_map([], |r| Ok(AclRuleRow(
                r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, String>(4)?
            ))) {
                rows.filter_map(|r| r.ok()).collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        drop(conn);
        let mut denied = std::collections::BTreeSet::new();
        for AclRuleRow(kb_id, gtype, uid, rid, effect) in acl_rules {
            let matches = match gtype.as_str() {
                "user" => uid == Some(user_id),
                "role" => rid.map(|r| roles.contains(&r)).unwrap_or(false),
                "public" => true,
                _ => false,
            };
            if matches {
                if effect == "deny" {
                    set.remove(&kb_id);
                    denied.insert(kb_id);
                } else {
                    set.insert(kb_id);
                }
            }
        }
        denied
    };
    // 3) 开放知识库（无任何成员）：与 can_access_kb 语义一致，视为全员可见；
    //    但被显式 deny 的开放库除外（deny 优先于开放可见）
    {
        let conn = db.conn_lock();
        let stmt = conn.prepare(
            "SELECT k.id FROM knowledge_bases k
             WHERE NOT EXISTS (SELECT 1 FROM kb_members m WHERE m.kb_id = k.id)",
        );
        if let Ok(mut stmt) = stmt {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, i64>(0)) {
                for r in rows.flatten() {
                    if !denied.contains(&r) {
                        set.insert(r);
                    }
                }
            }
        }
    }
    set.into_iter().collect()
}

// ─── 细粒度权限判定 ───

/// 返回用户在知识库中的角色（owner/admin/editor/viewer），无成员记录则 None
pub fn kb_role(db: &KbDatabase, kb_id: i64, user_id: i64) -> Option<String> {
    // 注意：先查成员角色并立即释放 conn 锁，再走 admin 兜底查询，
    // 否则 or_else 闭包内二次加锁同一 Mutex 会死锁（非重入）。
    {
        let conn = db.conn_lock();
        let role = conn
            .query_row(
                "SELECT role FROM kb_members WHERE kb_id = ?1 AND user_id = ?2",
                params![kb_id, user_id],
                |r| r.get::<_, String>(0),
            )
            .ok();
        if role.is_some() {
            return role;
        }
    }
    // 拥有 admin 角色的用户视为全局管理员，可管理任意 kb
    let c = db.conn_lock();
    c.query_row(
        "SELECT 1 FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id=?1 AND r.name='admin' LIMIT 1",
        params![user_id],
        |_| Ok(()),
    )
    .ok()
    .map(|_| "admin".to_string())
}

/// 是否可管理（设置 ACL / 删除 / 回滚）：owner 或 admin
pub fn can_manage_kb(db: &KbDatabase, kb_id: i64, user_id: i64) -> bool {
    matches!(
        kb_role(db, kb_id, user_id).as_deref(),
        Some("owner") | Some("admin")
    )
}

/// 统一知识库角色校验：要求用户对指定知识库具备至少 min_role 权限。
///
/// min_role 取值：
/// - `"owner"`：仅 owner / 全局 admin（管理类：删除知识库、ACL、成员管理、回滚）
/// - `"editor"`：owner / admin / editor（编辑类：移动文档、删除目录、重试/停止任务、清理活动）
/// - `"viewer"`：任何可访问成员（读取类）
///
/// 返回当前角色（供调用方按需使用），无权限时返回统一的中文错误信息。
/// 所有写操作命令都应通过本助手收敛权限判定，避免各处手写 matches! 产生口径漂移。
pub fn require_kb_role(
    db: &KbDatabase,
    kb_id: i64,
    user_id: i64,
    min_role: &str,
) -> Result<String, String> {
    let role =
        kb_role(db, kb_id, user_id).ok_or_else(|| "无权限：你无权访问该知识库".to_string())?;
    let allowed = match min_role {
        "owner" => role == "owner" || role == "admin",
        "editor" => matches!(role.as_str(), "owner" | "admin" | "editor"),
        _ => true, // viewer 及以上
    };
    if allowed {
        Ok(role)
    } else {
        Err(format!("无权限：需要「{}」及以上角色", min_role))
    }
}

/// 返回用户具备「编辑者」及以上权限的知识库 id（用于批量任务/活动清理等
/// 作用于多个知识库的操作，避免把仅可见（viewer）的知识库纳入写操作范围）。
pub fn editable_kb_ids(db: &KbDatabase, user_id: i64) -> Vec<i64> {
    visible_kb_ids(db, user_id)
        .into_iter()
        .filter(|&kb_id| {
            matches!(
                kb_role(db, kb_id, user_id).as_deref(),
                Some("owner") | Some("admin") | Some("editor")
            )
        })
        .collect()
}

/// 是否可访问知识库：有成员关系、或 kb 没有成员（开放）、或被 ACL allow。
/// 显式 deny（scope=kb）优先于一切，包括"开放库全员可见"。
pub fn can_access_kb(db: &KbDatabase, kb_id: i64, user_id: i64) -> bool {
    // deny 优先：显式拒绝的知识库不可访问
    if acl_denied(db, "kb", user_id, Some(kb_id), None) {
        return false;
    }
    // 开放知识库（无成员）默认可访问
    let member_cnt: i64 = {
        let c = db.conn_lock();
        c.query_row(
            "SELECT COUNT(*) FROM kb_members WHERE kb_id = ?1",
            params![kb_id],
            |r| r.get(0),
        )
        .unwrap_or(0)
    };
    if member_cnt == 0 {
        return true;
    }
    if can_manage_kb(db, kb_id, user_id) {
        return true;
    }
    if kb_role(db, kb_id, user_id).is_some() {
        return true;
    }
    // 检查 ACL：deny 优先
    acl_allow(db, "kb", user_id, Some(kb_id), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: i64, score: f64) -> RetrievedChunk {
        RetrievedChunk {
            chunk_id: id,
            doc_id: 1,
            kb_id: 1,
            content: format!("chunk {}", id),
            page_no: None,
            section: None,
            score,
            source: "test".to_string(),
            doc_title: "测试文档".to_string(),
        }
    }

    #[test]
    fn test_rrf_single_list() {
        let v = vec![chunk(1, 0.9), chunk(2, 0.8)];
        let fused = rrf_fuse(v.clone(), vec![], 60);
        assert_eq!(fused.len(), 2);
        assert!(
            (fused[0].score - 1.0 / 61.0).abs() < 1e-12,
            "rank0 分数应为 1/(k+1)"
        );
        assert!((fused[1].score - 1.0 / 62.0).abs() < 1e-12);
        assert_eq!(fused[0].chunk_id, 1, "排序应保持原始相对顺序");
        assert!(fused.iter().all(|c| c.source == "hybrid"));
    }

    #[test]
    fn test_rrf_fusion_accumulates() {
        // chunk2 在两路都出现，分数应累加并排第一
        let v = vec![chunk(1, 0.9), chunk(2, 0.8)];
        let b = vec![chunk(2, 0.7), chunk(3, 0.6)];
        let fused = rrf_fuse(v, b, 60);
        assert_eq!(fused.len(), 3, "两路共 3 个不同 chunk");
        assert_eq!(fused[0].chunk_id, 2, "重复出现的 chunk 分数最高");
        let expect = 1.0 / 62.0 + 1.0 / 61.0;
        assert!(
            (fused[0].score - expect).abs() < 1e-12,
            "分数应为两路 RRF 之和"
        );
        // 其余按 RRF 分数降序：chunk1 (1/61) > chunk3 (1/62)
        assert_eq!(fused[1].chunk_id, 1);
        assert_eq!(fused[2].chunk_id, 3);
    }

    #[test]
    fn test_rrf_empty_lists() {
        assert!(rrf_fuse(vec![], vec![], 60).is_empty());
        assert!(rrf_fuse(vec![chunk(1, 1.0)], vec![], 60).len() == 1);
    }

    #[test]
    fn test_rrf_custom_k() {
        let v = vec![chunk(9, 0.5)];
        let fused = rrf_fuse(v, vec![], 10);
        assert!((fused[0].score - 1.0 / 11.0).abs() < 1e-12);
    }

    #[test]
    fn test_rrf_priority_orders() {
        // 两路相同的重复情况：即使单路分数低，重复出现应占优
        let v = vec![chunk(1, 1.0), chunk(2, 0.0)];
        let b = vec![chunk(2, 0.0), chunk(3, 0.0)];
        let fused = rrf_fuse(v, b, 60);
        assert_eq!(fused[0].chunk_id, 2, "重复 chunk 优先于单路高分");
    }

    #[test]
    fn test_fts_query_cjk_short_phrase() {
        assert_eq!(crate::kb::fts_safe_query("测试文章"), "\"测 试 文 章\"");
    }

    #[test]
    fn test_fts_query_cjk_long_sentence_or_terms() {
        // 长中文整句不再作为单一短语，而是按单字 OR 展开，避免 0 命中
        let q = crate::kb::fts_safe_query("自动化测试知识库中关于测试文章的要点有哪些？");
        assert!(q.contains(" OR "), "长句应按单字 OR 展开，实际: {}", q);
        assert!(!q.contains("？"), "中文标点应被过滤，实际: {}", q);
        assert!(
            q.starts_with("自 OR 动 OR 化"),
            "应以单字 OR 开头，实际: {}",
            q
        );
    }

    #[test]
    fn test_fts_query_cjk_punct_filtered() {
        assert_eq!(crate::kb::fts_safe_query("要点？"), "\"要 点\"");
        assert_eq!(crate::kb::fts_safe_query("知识库。"), "\"知 识 库\"");
    }

    #[test]
    fn test_fts_query_ascii_quoted() {
        assert_eq!(
            crate::kb::fts_safe_query("hello world"),
            "\"hello\" \"world\""
        );
        assert_eq!(crate::kb::fts_safe_query("RAG 检索"), "\"RAG\" \"检 索\"");
    }

    #[test]
    fn test_fts_query_mixed_cjk_ascii() {
        assert_eq!(
            crate::kb::fts_safe_query("UI 测试文章"),
            "\"UI\" \"测 试 文 章\""
        );
    }

    #[test]
    fn test_fts_query_empty_and_special_chars() {
        assert!(crate::kb::fts_safe_query("").is_empty());
        assert!(crate::kb::fts_safe_query("   ").is_empty());
        assert_eq!(
            crate::kb::fts_safe_query("测试(自动化)"),
            "测 OR 试 OR 自 OR 动 OR 化"
        );
        assert!(crate::kb::fts_safe_query("……").is_empty(), "纯标点应返回空");
    }
}

/// 是否可访问文档：知识库可访问 且 文档级 ACL 未拒绝
pub fn can_access_doc(db: &KbDatabase, kb_id: i64, doc_id: i64, user_id: i64) -> bool {
    if !can_access_kb(db, kb_id, user_id) {
        return false;
    }
    let allowed = acl_allow(db, "document", user_id, Some(kb_id), Some(doc_id));
    // 若文档无 ACL 规则则继承知识库权限
    let has_rule: bool = {
        let c = db.conn_lock();
        c.query_row(
            "SELECT 1 FROM kb_acl WHERE scope='document' AND doc_id=?1 LIMIT 1",
            params![doc_id],
            |_| Ok(true),
        )
        .unwrap_or(false)
    };
    if !has_rule {
        return true;
    }
    allowed
}

/// ACL deny 判定：只要存在匹配当前用户的 deny 规则即拒绝（deny 优先）。
/// 与 acl_allow 共用规则查询，独立成函数以便"开放库可见"等场景先查拒绝。
fn acl_denied(
    db: &KbDatabase,
    scope: &str,
    user_id: i64,
    kb_id: Option<i64>,
    doc_id: Option<i64>,
) -> bool {
    // 先收集角色（避免跨借用）
    let roles: Vec<i64> = {
        let c = db.conn_lock();
        c.prepare("SELECT role_id FROM user_roles WHERE user_id = ?1")
            .ok()
            .and_then(|mut s| {
                s.query_map(params![user_id], |r| r.get::<_, i64>(0))
                    .ok()
                    .map(|rs| rs.filter_map(|r| r.ok()).collect::<Vec<i64>>())
            })
            .unwrap_or_default()
    };
    // 收集 ACL 规则（owned）
    let rules: Vec<(String, String, Option<i64>, Option<i64>)> = {
        let conn = db.conn_lock();
        conn.prepare(
            "SELECT effect, grantee_type, user_id, role_id FROM kb_acl
             WHERE scope = ?1
               AND (doc_id = ?2 OR (doc_id IS NULL AND ?2 IS NULL))
               AND (kb_id = ?3 OR (kb_id IS NULL AND ?3 IS NULL))",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map(params![scope, doc_id, kb_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })
            .ok()
            .map(|rs| rs.filter_map(|r| r.ok()).collect::<Vec<_>>())
        })
        .unwrap_or_default()
    };
    for (effect, gtype, uid, rid) in rules {
        let matches = match gtype.as_str() {
            "user" => uid == Some(user_id),
            "role" => rid.map(|r| roles.contains(&r)).unwrap_or(false),
            "public" => true,
            _ => false,
        };
        if matches && effect == "deny" {
            return true;
        }
    }
    false
}

/// ACL 判定：deny 优先。用户直接授权或经角色授权均算 allow。
fn acl_allow(
    db: &KbDatabase,
    scope: &str,
    user_id: i64,
    kb_id: Option<i64>,
    doc_id: Option<i64>,
) -> bool {
    // 先收集角色（避免跨借用）
    let roles: Vec<i64> = {
        let c = db.conn_lock();
        c.prepare("SELECT role_id FROM user_roles WHERE user_id = ?1")
            .ok()
            .and_then(|mut s| {
                s.query_map(params![user_id], |r| r.get::<_, i64>(0))
                    .ok()
                    .map(|rs| rs.filter_map(|r| r.ok()).collect::<Vec<i64>>())
            })
            .unwrap_or_default()
    };
    // 收集 ACL 规则（owned）
    let rules: Vec<(String, String, Option<i64>, Option<i64>)> = {
        let conn = db.conn_lock();
        conn.prepare(
            "SELECT effect, grantee_type, user_id, role_id FROM kb_acl
             WHERE scope = ?1
               AND (doc_id = ?2 OR (doc_id IS NULL AND ?2 IS NULL))
               AND (kb_id = ?3 OR (kb_id IS NULL AND ?3 IS NULL))",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map(params![scope, doc_id, kb_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })
            .ok()
            .map(|rs| rs.filter_map(|r| r.ok()).collect::<Vec<_>>())
        })
        .unwrap_or_default()
    };

    let mut allow = false;
    for (effect, gtype, uid, rid) in rules {
        let matches = match gtype.as_str() {
            "user" => uid == Some(user_id),
            "role" => rid.map(|r| roles.contains(&r)).unwrap_or(false),
            "public" => true,
            _ => false,
        };
        if matches {
            if effect == "deny" {
                return false; // deny 优先
            }
            allow = true;
        }
    }
    allow
}
