// ============================================================
// 知识库管理 — Wiki 页面命令
// 自 handlers.rs 拆分：列表/目录/搜索/详情/图谱（查询类）
// 与页面 CRUD / LLM 自动提炼（创建/更新/删除/生成/批量提取）。
// ============================================================

use crate::kb::db::KbDatabase;
use tauri::State;

use super::resolve_inference_pair;
use super::{log_metric_event, MetricEvent};

/// 列出知识库全部页面（按标题排序）
#[tauri::command]
pub async fn kb_wiki_list_pages(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
) -> Result<Vec<crate::kb::wiki::WikiPageItem>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_access_kb(&db, kb_id, uid) {
        return Err("无权限：你无权访问该知识库".to_string());
    }
    crate::kb::wiki::list_pages(&db, kb_id)
}

/// 列出知识库内的 Wiki 目录（含页面数，供前端按目录筛选）
/// 只返回包含 Wiki 页面的目录，避免文档目录删除影响 Wiki 目录树
#[tauri::command]
pub async fn kb_wiki_dirs(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
) -> Result<Vec<serde_json::Value>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_access_kb(&db, kb_id, uid) {
        return Err("无权限：你无权访问该知识库".to_string());
    }
    let conn = db.conn_lock();
    // 只返回包含 Wiki 页面的目录（独立于文档目录树）
    let mut stmt = conn
        .prepare(
            "SELECT d.id, d.parent_id, d.name,
                    (SELECT COUNT(*) FROM wiki_pages p WHERE p.dir_id = d.id) AS cnt
             FROM kb_directories d
             WHERE d.kb_id = ?1
               AND EXISTS (SELECT 1 FROM wiki_pages p WHERE p.dir_id = d.id)
             ORDER BY d.name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![kb_id], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "parentId": r.get::<_, Option<i64>>(1)?,
                "name": r.get::<_, String>(2)?,
                "count": r.get::<_, i64>(3)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    let raw: Vec<(i64, Option<i64>, String, i64)> = rows
        .filter_map(|r| r.ok())
        .map(|v| {
            (
                v.get("id").and_then(|x| x.as_i64()).unwrap_or(0),
                v.get("parentId").and_then(|x| x.as_i64()),
                v.get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                v.get("count").and_then(|x| x.as_i64()).unwrap_or(0),
            )
        })
        .collect();
    Ok(dir_subtree_counts(&raw)
        .into_iter()
        .map(|(id, parent_id, name, count)| serde_json::json!({ "id": id, "parentId": parent_id, "name": name, "count": count }))
        .collect())
}

/// 计算每个目录的页面总数（含全部子孙目录的页面）。
/// 实体页归档在「实体/<类型>」子目录，父目录计数若不包含子孙会显示 0，易被误认为异常。
pub fn dir_subtree_counts(
    rows: &[(i64, Option<i64>, String, i64)],
) -> Vec<(i64, Option<i64>, String, i64)> {
    use std::collections::HashMap;
    let mut direct: HashMap<i64, i64> = HashMap::new();
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    for (id, parent, _, cnt) in rows {
        direct.insert(*id, *cnt);
        if let Some(p) = parent {
            children.entry(*p).or_default().push(*id);
        }
    }
    fn subtree_total(
        id: i64,
        children: &HashMap<i64, Vec<i64>>,
        direct: &HashMap<i64, i64>,
        memo: &mut HashMap<i64, i64>,
    ) -> i64 {
        if let Some(v) = memo.get(&id) {
            return *v;
        }
        let mut s = direct.get(&id).copied().unwrap_or(0);
        if let Some(cs) = children.get(&id) {
            for c in cs {
                s += subtree_total(*c, children, direct, memo);
            }
        }
        memo.insert(id, s);
        s
    }
    let mut memo = HashMap::new();
    rows.iter()
        .map(|(id, parent, name, _)| {
            let total = subtree_total(*id, &children, &direct, &mut memo);
            (*id, *parent, name.clone(), total)
        })
        .collect()
}

/// 用 BM25 全文检索 Wiki 页面，按相关度返回
#[tauri::command]
pub async fn kb_wiki_search(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<crate::kb::wiki::WikiPageItem>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_access_kb(&db, kb_id, uid) {
        return Err("无权限：你无权访问该知识库".to_string());
    }
    crate::kb::wiki::search_pages(&db, kb_id, &query, limit.unwrap_or(20).clamp(1, 200))
}

/// 获取页面详情（含解析后的双向链接与标签）
#[tauri::command]
pub async fn kb_wiki_get_page(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    page_id: i64,
) -> Result<crate::kb::wiki::WikiPageDetail, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let page = crate::kb::wiki::get_page(&db, page_id)?;
    if !crate::kb::retrieval::can_access_kb(&db, page.kb_id, uid) {
        return Err("无权限：你无权访问该知识库".to_string());
    }
    log_metric_event(
        &db,
        &MetricEvent {
            uid,
            event_type: "wiki_view",
            kb_id: Some(page.kb_id),
            doc_id: page.doc_id,
            page_id: Some(page_id),
            session_id: None,
            detail: None,
        },
    );
    Ok(page)
}

/// 返回知识库页面图（节点 + 边，供前端可视化）
#[tauri::command]
pub async fn kb_wiki_graph(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
) -> Result<crate::kb::wiki::WikiGraph, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_access_kb(&db, kb_id, uid) {
        return Err("无权限：你无权访问该知识库".to_string());
    }
    crate::kb::wiki::graph(&db, kb_id)
}

/// 新建页面（仅 owner/admin；页面存在时返回既有 id）
#[tauri::command]
pub async fn kb_wiki_create_page(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    input: crate::kb::wiki::WikiPageInput,
) -> Result<i64, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_manage_kb(&db, input.kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可编辑页面".to_string());
    }
    let page_id = crate::kb::wiki::create_page(&db, &input, uid)?;
    // 创建后自动后台提取摘要与实体（推理模型）
    spawn_wiki_extract(&db, uid, page_id);
    Ok(page_id)
}

/// 更新页面（仅 owner/admin）
#[tauri::command]
pub async fn kb_wiki_update_page(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    page_id: i64,
    input: crate::kb::wiki::WikiPageInput,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let kb_id = wiki_page_kb_id(&db, page_id)?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可编辑页面".to_string());
    }
    crate::kb::wiki::update_page(&db, page_id, &input)?;
    // 内容变更后自动重新提取摘要与实体
    spawn_wiki_extract(&db, uid, page_id);
    Ok(())
}

/// 后台提交单页的摘要/实体提取（复用推理模型设置）
fn spawn_wiki_extract(db: &KbDatabase, uid: i64, page_id: i64) {
    let (provider, model) = resolve_inference_pair(db, None, None);
    let db_task = db.clone();
    tauri::async_runtime::spawn(async move {
        let _ = crate::kb::wiki::extract_page_meta(
            &db_task,
            uid,
            page_id,
            provider.as_deref(),
            model.as_deref(),
        )
        .await;
    });
}

/// Wiki 摘要联动：源文档内容变化后，使关联 Wiki 页面的摘要/实体失效并自动重新提取
pub(crate) fn refresh_wiki_for_doc(db: &KbDatabase, doc_id: i64) {
    let pages: Vec<i64> = {
        let conn = db.conn_lock();
        let mut stmt = match conn
            .prepare("SELECT id FROM wiki_pages WHERE doc_id = ?1 AND extract_status = 'done'")
        {
            Ok(s) => s,
            Err(_) => return,
        };
        let rows = match stmt.query_map(rusqlite::params![doc_id], |r| r.get::<_, i64>(0)) {
            Ok(r) => r,
            Err(_) => return,
        };
        rows.filter_map(|r| r.ok()).collect()
    };
    if pages.is_empty() {
        return;
    }
    let (provider, model) = resolve_inference_pair(db, None, None);
    let db_task = db.clone();
    tauri::async_runtime::spawn(async move {
        for pid in pages {
            if let Err(e) = crate::kb::wiki::extract_page_meta(
                &db_task,
                0,
                pid,
                provider.as_deref(),
                model.as_deref(),
            )
            .await
            {
                log::warn!("源文档变更后 Wiki 摘要刷新失败 page={} err={}", pid, e);
            }
        }
    });
}

/// 手动提取单个页面的摘要与实体（后台执行）
#[tauri::command]
pub async fn kb_wiki_extract(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    page_id: i64,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let kb_id = wiki_page_kb_id(&db, page_id)?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可提取摘要与实体".to_string());
    }
    spawn_wiki_extract(&db, uid, page_id);
    Ok(serde_json::json!({ "submitted": 1 }))
}

/// 批量提取知识库内尚未提取（或失败）页面的摘要与实体（后台执行）
/// force=true 时先重置所有页面的 extract_status，再全量提取
#[tauri::command]
pub async fn kb_wiki_extract_all(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
    force: Option<bool>,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可批量提取".to_string());
    }
    let force = force.unwrap_or(false);
    let pages: Vec<i64> = {
        let conn = db.conn_lock();
        // 强制模式：先重置所有页面的 extract_status 为空
        if force {
            conn.execute(
                "UPDATE wiki_pages SET extract_status = '' WHERE kb_id = ?1",
                rusqlite::params![kb_id],
            )
            .map_err(|e| e.to_string())?;
            log::info!("[wiki] 强制重置知识库 {} 全部页面 extract_status", kb_id);
        }
        let mut stmt = conn
            .prepare(
                "SELECT id FROM wiki_pages WHERE kb_id = ?1 AND COALESCE(extract_status,'') != 'done' ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![kb_id], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let submitted = pages.len();
    if submitted == 0 {
        return Ok(serde_json::json!({ "submitted": 0 }));
    }
    let (provider, model) = resolve_inference_pair(&db, None, None);
    let db_task = (*db).clone();
    tauri::async_runtime::spawn(async move {
        for pid in pages {
            if let Err(e) = crate::kb::wiki::extract_page_meta(
                &db_task,
                uid,
                pid,
                provider.as_deref(),
                model.as_deref(),
            )
            .await
            {
                log::warn!("页面 {} 摘要/实体提取失败: {}", pid, e);
            }
        }
    });
    Ok(serde_json::json!({ "submitted": submitted, "force": force }))
}

/// 删除页面（仅 owner/admin）
#[tauri::command]
pub async fn kb_wiki_delete_page(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    page_id: i64,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let kb_id = wiki_page_kb_id(&db, page_id)?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可删除页面".to_string());
    }
    crate::kb::wiki::delete_page(&db, page_id)
}

/// 用 LLM 从文档提炼 Wiki 页面（仅 owner/admin）。
/// 改为后台任务执行：立即返回提交的文档数，进度可在「活动 → 处理任务」查看。
#[tauri::command]
pub async fn kb_wiki_generate(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    input: crate::kb::wiki::WikiGenerateInput,
) -> Result<serde_json::Value, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_manage_kb(&db, input.kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可生成页面".to_string());
    }
    let docs = crate::kb::wiki::list_ready_docs(&db, input.kb_id, input.doc_id)?;
    let submitted = docs.len();
    if submitted == 0 {
        return Err(if input.doc_id.is_some() {
            "指定文档不存在或未就绪".to_string()
        } else {
            "知识库内没有已就绪（ready）的文档，请先上传并完成处理".to_string()
        });
    }
    let db_task = (*db).clone();
    let kb_id = input.kb_id;
    // 新批量提交：清除历史取消标记，避免上次「停止处理」残留标记让新批量直接终止
    {
        let conn = db.conn_lock();
        let _ = conn.execute(
            "DELETE FROM kb_chunk_settings WHERE key = ?1",
            rusqlite::params![format!("generate_cancel_{}", kb_id)],
        );
    }
    // 未显式指定模型时：kb_model_settings(inference) → 全局默认对话模型（Wiki 提炼）
    let (provider, model) =
        resolve_inference_pair(&db, input.provider_id.clone(), input.model.clone());
    tauri::async_runtime::spawn(async move {
        let _ = crate::kb::wiki::generate_with_jobs(
            db_task,
            uid,
            kb_id,
            docs,
            provider.as_deref(),
            model.as_deref(),
        )
        .await;
    });
    Ok(serde_json::json!({ "submitted": submitted }))
}

/// 查询页面所属知识库 id（权限校验用）
fn wiki_page_kb_id(db: &KbDatabase, page_id: i64) -> Result<i64, String> {
    let conn = db.conn_lock();
    conn.query_row(
        "SELECT kb_id FROM wiki_pages WHERE id = ?1",
        rusqlite::params![page_id],
        |r| r.get::<_, i64>(0),
    )
    .map_err(|_| "页面不存在".to_string())
}

/// 列出页面的版本历史
#[tauri::command]
pub async fn kb_wiki_list_versions(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    page_id: i64,
) -> Result<Vec<crate::kb::wiki::WikiVersionItem>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let kb_id = wiki_page_kb_id(&db, page_id)?;
    if !crate::kb::retrieval::can_access_kb(&db, kb_id, uid) {
        return Err("无权限".to_string());
    }
    crate::kb::wiki::list_versions(&db, page_id)
}

/// 回滚页面到指定版本
#[tauri::command]
pub async fn kb_wiki_restore_version(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    page_id: i64,
    version_id: i64,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let kb_id = wiki_page_kb_id(&db, page_id)?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可回滚版本".to_string());
    }
    crate::kb::wiki::restore_version(&db, page_id, version_id)
}
