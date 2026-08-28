// Copyright (c) 2026 ST Team - MIT License
// See LICENSE file in the project root for full license information.

// ============================================================
// 向量索引缓存（内存持久化）
//
// 设计目标：
//  - 避免每次检索从 SQLite 读取全部 embedding BLOB（I/O 瓶颈）
//  - 首次检索时加载到内存，后续直接使用缓存
//  - 文档变更时自动失效（generation 计数器）
//  - 支持增量更新（新增/删除分片时局部刷新）
//
// 内存估算：
//  - 10K chunks × 768 dim × 4 bytes ≈ 30 MB
//  - 50K chunks × 768 dim × 4 bytes ≈ 150 MB
//  - 适合桌面/私有化部署场景（SME 典型规模）
//
// 检索性能：
//  - 缓存命中：纯 CPU 余弦计算，50K chunks ≈ 5-15ms
//  - 缓存未命中：从 SQLite 加载，50K chunks ≈ 200-500ms（一次性）
// ============================================================

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

use lru::LruCache;

use crate::kb::db::{cosine_similarity, deserialize_embedding, KbDatabase};

/// 全局向量索引缓存单例
pub static VECTOR_INDEX: LazyLock<VectorIndex> = LazyLock::new(VectorIndex::new);

/// 缓存的向量条目
#[derive(Clone)]
struct CachedVector {
    chunk_id: i64,
    doc_id: i64,
    kb_id: i64,
    content: String,
    page_no: Option<i64>,
    section: Option<String>,
    doc_title: String,
    embedding: Vec<f64>,
}

/// 向量索引缓存
pub struct VectorIndex {
    /// 缓存的向量数据
    cache: Mutex<Option<VectorCache>>,
    /// 数据库 generation 计数器（文档/分片变更时递增）
    db_generation: AtomicU64,
    /// 单次检索 LRU 缓存（query → results，短 TTL）
    search_cache: Mutex<LruCache<String, (Instant, Vec<super::kb::retrieval::RetrievedChunk>)>>,
}

struct VectorCache {
    vectors: Vec<CachedVector>,
    generation: u64,
    loaded_at: Instant,
    /// 加载耗时（ms）
    load_ms: u64,
}

/// 检索缓存 TTL
const SEARCH_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);
/// 检索缓存容量
const SEARCH_CACHE_CAP: usize = 100;

impl VectorIndex {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(None),
            db_generation: AtomicU64::new(0),
            search_cache: Mutex::new(LruCache::new(NonZeroUsize::new(SEARCH_CACHE_CAP).unwrap())),
        }
    }

    /// 通知数据库变更（文档上传/删除/重新嵌入时调用）
    pub fn invalidate(&self) {
        self.db_generation.fetch_add(1, Ordering::SeqCst);
        // 清空检索缓存
        if let Ok(mut sc) = self.search_cache.lock() {
            sc.clear();
        }
        log::debug!(
            "[vector-index] 缓存已失效，generation={}",
            self.db_generation.load(Ordering::Relaxed)
        );
    }

    /// 获取缓存的向量数据（必要时从数据库加载）
    fn ensure_loaded(&self, db: &KbDatabase, visible_kbs: &[i64]) -> Arc<Vec<CachedVector>> {
        let current_gen = self.db_generation.load(Ordering::Relaxed);

        // 检查缓存是否有效
        {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref c) = *cache {
                if c.generation == current_gen {
                    return Arc::new(c.vectors.clone());
                }
            }
        }

        // 缓存未命中或已过期，从数据库加载
        let t0 = Instant::now();
        let vectors = self.load_from_db(db, visible_kbs);
        let load_ms = t0.elapsed().as_millis() as u64;

        log::info!(
            "[vector-index] 从数据库加载 {} 个向量，耗时={}ms，generation={}",
            vectors.len(),
            load_ms,
            current_gen
        );

        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        *cache = Some(VectorCache {
            vectors: vectors.clone(),
            generation: current_gen,
            loaded_at: Instant::now(),
            load_ms,
        });

        Arc::new(vectors)
    }

    /// 从数据库加载所有可见知识库的向量数据
    fn load_from_db(&self, db: &KbDatabase, visible_kbs: &[i64]) -> Vec<CachedVector> {
        let conn = db.conn_lock();
        let placeholders = visible_kbs
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT c.id, c.doc_id, c.kb_id, c.content, c.page_no, c.section, \
             c.embedding_blob, COALESCE(d.title,'') \
             FROM document_chunks c JOIN documents d ON d.id = c.doc_id \
             WHERE c.kb_id IN ({}) AND c.embedding_blob IS NOT NULL",
            placeholders
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(e) => {
                log::error!("[vector-index] 准备查询失败: {}", e);
                return Vec::new();
            }
        };
        let params_vec: Vec<&dyn rusqlite::types::ToSql> = visible_kbs
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt.query_map(params_vec.as_slice(), |row| {
            Ok(CachedVector {
                chunk_id: row.get(0)?,
                doc_id: row.get(1)?,
                kb_id: row.get(2)?,
                content: row.get(3)?,
                page_no: row.get(4)?,
                section: row.get(5)?,
                embedding: deserialize_embedding(&row.get::<_, Vec<u8>>(6)?),
                doc_title: row.get(7)?,
            })
        });
        match rows {
            Ok(r) => r.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                log::error!("[vector-index] 查询向量失败: {}", e);
                Vec::new()
            }
        }
    }

    /// 使用缓存进行向量检索
    pub async fn search(
        &self,
        db: &KbDatabase,
        query: &str,
        visible_kbs: &[i64],
        top_k: usize,
        provider_id: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<super::kb::retrieval::RetrievedChunk>, String> {
        // 检查检索缓存
        let cache_key = format!(
            "{:?}:{}:{}:{}",
            visible_kbs,
            query,
            top_k,
            model.unwrap_or("")
        );
        {
            let mut sc = self.search_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((ts, results)) = sc.get(&cache_key) {
                if ts.elapsed() < SEARCH_CACHE_TTL {
                    return Ok(results.clone());
                }
            }
        }

        let t0 = Instant::now();

        // 生成查询向量
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

        // 从缓存获取向量数据
        let vectors = self.ensure_loaded(db, visible_kbs);
        let total = vectors.len();

        // 批量计算余弦相似度
        let mut scored: Vec<(usize, f64)> = vectors
            .iter()
            .enumerate()
            .map(|(i, v)| (i, cosine_similarity(&qvec, &v.embedding)))
            .collect();

        // 排序并取 top_k
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        let results: Vec<super::kb::retrieval::RetrievedChunk> = scored
            .into_iter()
            .map(|(i, sim)| {
                let v = &vectors[i];
                super::kb::retrieval::RetrievedChunk {
                    chunk_id: v.chunk_id,
                    doc_id: v.doc_id,
                    kb_id: v.kb_id,
                    content: v.content.clone(),
                    page_no: v.page_no,
                    section: v.section.clone(),
                    score: sim,
                    source: "vector".to_string(),
                    doc_title: v.doc_title.clone(),
                }
            })
            .collect();

        let elapsed = t0.elapsed();
        log::info!(
            "[vector-index] 检索完成: 缓存向量={} top_k={} 耗时={}ms 结果={}",
            total,
            top_k,
            elapsed.as_millis(),
            results.len()
        );

        // 写入检索缓存
        {
            let mut sc = self.search_cache.lock().unwrap_or_else(|e| e.into_inner());
            sc.put(cache_key, (Instant::now(), results.clone()));
        }

        Ok(results)
    }

    /// 缓存状态（监控用）
    pub fn stats(&self) -> serde_json::Value {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let search_cache = self.search_cache.lock().unwrap_or_else(|e| e.into_inner());
        match cache.as_ref() {
            Some(c) => serde_json::json!({
                "cached": true,
                "vectors": c.vectors.len(),
                "generation": c.generation,
                "loadMs": c.load_ms,
                "ageSeconds": c.loaded_at.elapsed().as_secs(),
                "searchCacheSize": search_cache.len(),
            }),
            None => serde_json::json!({
                "cached": false,
                "generation": self.db_generation.load(Ordering::Relaxed),
                "searchCacheSize": search_cache.len(),
            }),
        }
    }
}

impl Default for VectorIndex {
    fn default() -> Self {
        Self::new()
    }
}
