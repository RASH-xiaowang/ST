// ════════════════════════════════════════════════════════════
// Wiki 查询（页面列表 / 详情 / 知识图谱）
// 自 wiki.rs 拆分：列表与摘要、页面详情、出/入链与实体图。
// ════════════════════════════════════════════════════════════

use rusqlite::{params, OptionalExtension};

use super::types::{
    WikiEntity, WikiGraph, WikiGraphEdge, WikiGraphNode, WikiLinkInfo, WikiPageDetail, WikiPageItem,
};
use super::utils::{extract_wiki_links, OptionNone};
use super::WikiPageRow;
use crate::kb::db::KbDatabase;

// ────────────────────────────────────────────────────────────
// 查询
// ────────────────────────────────────────────────────────────

/// 列出知识库的全部 wiki 页面（含出入链计数）
pub fn list_pages(db: &KbDatabase, kb_id: i64) -> Result<Vec<WikiPageItem>, String> {
    let conn = db.conn_lock();
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.kb_id, p.dir_id, p.doc_id, COALESCE(d.title,''), p.title, p.slug, p.summary, p.status,
                    p.created_at, p.updated_at,
                    (SELECT COUNT(*) FROM wiki_links wl WHERE wl.from_page_id = p.id) AS out_links,
                    (SELECT COUNT(*) FROM wiki_links wl WHERE wl.to_page_id = p.id) AS in_links,
                    (SELECT COUNT(*) FROM wiki_page_entities e WHERE e.page_id = p.id) AS entity_count
             FROM wiki_pages p
             LEFT JOIN documents d ON d.id = p.doc_id
             WHERE p.kb_id = ?1
             ORDER BY p.updated_at DESC, p.id DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![kb_id], |row| {
            Ok(WikiPageItem {
                id: row.get(0)?,
                kb_id: row.get(1)?,
                dir_id: row.get(2)?,
                doc_id: row.get(3)?,
                doc_title: row.get::<_, String>(4)?.trim().to_string().into_none(),
                title: row.get(5)?,
                slug: row.get(6)?,
                summary: row.get(7)?,
                status: row.get(8)?,
                out_links: row.get(11)?,
                in_links: row.get(12)?,
                entity_count: row.get(13)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 提取正文中包含 `[[标题` 链接的行作为上下文片段
pub(crate) fn link_snippet(content_md: &str, title: &str) -> Option<String> {
    let lower = content_md.to_lowercase();
    let pat = format!("[[{}", title).to_lowercase();
    // to_lowercase 可能改变字节长度，索引不一致时跳过片段，避免越界/非字符边界 panic
    if lower.len() != content_md.len() {
        return None;
    }
    let pos = lower.find(&pat)?;
    if !content_md.is_char_boundary(pos) {
        return None;
    }
    let line_start = content_md[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let rest = &content_md[pos..];
    let line_end = rest.find('\n').map(|i| pos + i).unwrap_or(content_md.len());
    let mut line = content_md[line_start..line_end].trim().to_string();
    if line.len() > 90 {
        let mut cut = 87;
        while cut > 0 && !line.is_char_boundary(cut) {
            cut -= 1;
        }
        line = format!("{}…", &line[..cut]);
    }
    Some(line)
}

/// 提取正文中纯文本出现标题的行作为上下文片段
pub(crate) fn plain_snippet(content_md: &str, title: &str) -> Option<String> {
    let lower = content_md.to_lowercase();
    let t = title.to_lowercase();
    if lower.len() != content_md.len() {
        return None;
    }
    let pos = lower.find(&t)?;
    if !content_md.is_char_boundary(pos) {
        return None;
    }
    let line_start = content_md[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let rest = &content_md[pos..];
    let line_end = rest.find('\n').map(|i| pos + i).unwrap_or(content_md.len());
    let mut line = content_md[line_start..line_end].trim().to_string();
    if line.len() > 90 {
        let mut cut = 87;
        while cut > 0 && !line.is_char_boundary(cut) {
            cut -= 1;
        }
        line = format!("{}…", &line[..cut]);
    }
    Some(line)
}

/// 获取单页详情（正文 + 出链 + 入链 + 失效链接 + 未链接提及）
pub fn get_page(db: &KbDatabase, page_id: i64) -> Result<WikiPageDetail, String> {
    let conn = db.conn_lock();
    let base: Option<WikiPageRow> = conn.query_row(
            "SELECT p.id, p.kb_id, p.doc_id, d.title, p.title, p.slug, p.summary, p.content_md, p.status,
                    p.created_by, p.created_at, p.updated_at
             FROM wiki_pages p
             LEFT JOIN documents d ON d.id = p.doc_id
             WHERE p.id = ?1",
            params![page_id],
            |row| {
                Ok(WikiPageRow(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let b = base.ok_or_else(|| "页面不存在".to_string())?;

    // 出链（带当前页正文中的上下文片段）
    let out = {
        let mut stmt = conn
            .prepare(
                "SELECT wl.to_page_id, p.title, p.slug, wl.link_type, wl.weight
                 FROM wiki_links wl JOIN wiki_pages p ON p.id = wl.to_page_id
                 WHERE wl.from_page_id = ?1 ORDER BY wl.weight DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![page_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok())
            .map(|(page_id, title, slug, link_type, weight)| WikiLinkInfo {
                page_id,
                title: title.clone(),
                slug,
                link_type,
                weight,
                snippet: link_snippet(&b.7, &title),
            })
            .collect::<Vec<WikiLinkInfo>>()
    };
    // 入链（反向链接，带来源页正文中的上下文片段）
    let inc = {
        let mut stmt = conn
            .prepare(
                "SELECT wl.from_page_id, p.title, p.slug, wl.link_type, wl.weight, p.content_md
                 FROM wiki_links wl JOIN wiki_pages p ON p.id = wl.from_page_id
                 WHERE wl.to_page_id = ?1 ORDER BY wl.weight DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![page_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok())
            .map(
                |(page_id, title, slug, link_type, weight, content)| WikiLinkInfo {
                    page_id,
                    title: title.clone(),
                    slug,
                    link_type,
                    weight,
                    snippet: link_snippet(&content, &b.4),
                },
            )
            .collect::<Vec<WikiLinkInfo>>()
    };
    // 失效链接：正文引用了但库里不存在的标题
    let unresolved: Vec<String> = {
        let existing: std::collections::HashSet<String> = {
            let mut stmt = conn
                .prepare("SELECT title, slug FROM wiki_pages WHERE kb_id = ?1")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![b.1], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok())
                .flat_map(|(t, s)| vec![t, s])
                .collect()
        };
        let mut seen = std::collections::HashSet::new();
        extract_wiki_links(&b.7)
            .into_iter()
            .map(|(t, _)| t.trim().to_string())
            .filter(|t| !t.is_empty() && !existing.contains(t))
            .filter(|t| seen.insert(t.clone()))
            .collect()
    };
    // 未链接提及：纯文本提到本页标题但未使用 [[链接]] 的页面
    let unlinked_mentions: Vec<WikiLinkInfo> = {
        let mut stmt = conn
            .prepare(
                "SELECT p.id, p.title, p.slug, p.content_md
                 FROM wiki_pages p
                 WHERE p.kb_id = ?1 AND p.id != ?2 AND p.content_md LIKE ?3",
            )
            .map_err(|e| e.to_string())?;
        let like = format!("%{}%", b.4);
        let rows = stmt
            .query_map(params![b.1, page_id, like], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out: Vec<WikiLinkInfo> = Vec::new();
        for r in rows.filter_map(|r| r.ok()) {
            let (pid, title, slug, content) = r;
            // 已存在指向本页的链接 → 视为已链接，跳过
            let linked: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM wiki_links WHERE from_page_id = ?1 AND to_page_id = ?2",
                    params![pid, page_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if linked > 0 {
                continue;
            }
            // 纯文本提到（且非 [[...]] 形式）
            let lower = content.to_lowercase();
            let t = b.4.to_lowercase();
            if let Some(pos) = lower.find(&t) {
                // to_lowercase 长度变化或命中位置不是字符边界时跳过，避免切片 panic
                if lower.len() != content.len()
                    || !content.is_char_boundary(pos)
                    || pos + b.4.len() > content.len()
                    || !content.is_char_boundary(pos + b.4.len())
                {
                    continue;
                }
                let before = content[..pos].chars().next_back().unwrap_or(' ');
                let after = content[pos + b.4.len()..].chars().next().unwrap_or(' ');
                // [[标题]] 形式由上面已链接分支处理；这里排除 `[[标题` 的残留
                if before != '[' && after != ']' {
                    out.push(WikiLinkInfo {
                        page_id: pid,
                        title: title.clone(),
                        slug,
                        link_type: "mention".to_string(),
                        weight: 1.0,
                        snippet: plain_snippet(&content, &b.4),
                    });
                }
            }
        }
        // 限制数量，避免列表过长
        out.truncate(20);
        out
    };
    // 实体与提取状态
    let extract_status: String = conn
        .query_row(
            "SELECT extract_status FROM wiki_pages WHERE id = ?1",
            params![page_id],
            |r| r.get(0),
        )
        .unwrap_or_default();
    let entities: Vec<WikiEntity> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, entity_type, description FROM wiki_page_entities WHERE page_id = ?1 ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![page_id], |row| {
                Ok(WikiEntity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    description: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    Ok(WikiPageDetail {
        id: b.0,
        kb_id: b.1,
        doc_id: b.2,
        doc_title: b.3,
        title: b.4,
        slug: b.5,
        summary: b.6,
        content_md: b.7,
        status: b.8,
        created_by: b.9,
        created_at: b.10,
        updated_at: b.11,
        out_links: out,
        in_links: inc,
        unresolved,
        unlinked_mentions,
        entities,
        extract_status,
    })
}

/// 知识图谱：节点 = 页面，边 = 页面间链接
pub fn graph(db: &KbDatabase, kb_id: i64) -> Result<WikiGraph, String> {
    let conn = db.conn_lock();
    let nodes: Vec<WikiGraphNode> = {
        let mut stmt = conn
            .prepare(
                "SELECT p.id, p.title, p.doc_id, COALESCE(d.title,''), p.status,
                        (SELECT COUNT(*) FROM wiki_links wl WHERE wl.to_page_id = p.id) AS in_deg,
                        (SELECT COUNT(*) FROM wiki_links wl WHERE wl.from_page_id = p.id) AS out_deg,
                        kd.name
                 FROM wiki_pages p
                 LEFT JOIN documents d ON d.id = p.doc_id
                 LEFT JOIN kb_directories kd ON kd.id = p.dir_id
                 WHERE p.kb_id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![kb_id], |row| {
                Ok(WikiGraphNode {
                    id: row.get::<_, i64>(0)?,
                    page_id: row.get::<_, i64>(0)?,
                    title: row.get(1)?,
                    doc_id: row.get(2)?,
                    doc_title: row.get::<_, String>(3)?.trim().to_string().into_none(),
                    status: row.get(4)?,
                    in_degree: row.get(5)?,
                    out_degree: row.get(6)?,
                    dir_name: row.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let edges: Vec<WikiGraphEdge> = {
        let mut stmt = conn
            .prepare(
                "SELECT from_page_id, to_page_id, link_type, weight
                 FROM wiki_links WHERE kb_id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![kb_id], |row| {
                Ok(WikiGraphEdge {
                    from: row.get(0)?,
                    to: row.get(1)?,
                    link_type: row.get(2)?,
                    weight: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    // ── 隐含关系：共享实体 —— 即使两篇笔记没有显式 [[链接]]，
    // 只要抽取到同一实体（人物/组织/概念…），也视为语义相近并连成一条边，
    // 帮助发现思维中的隐藏关联。权重 = 共享实体数。
    let mut edges = edges;
    let entity_map: std::collections::HashMap<i64, Vec<String>> = {
        let mut stmt = conn
            .prepare("SELECT page_id, lower(trim(name)) FROM wiki_page_entities WHERE kb_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![kb_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let mut m: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
        for r in rows.flatten() {
            let (pid, name) = r;
            if name.trim().is_empty() {
                continue;
            }
            m.entry(pid).or_default().push(name);
        }
        m
    };
    let page_ids: Vec<i64> = nodes.iter().map(|n| n.id).collect();
    // 已显式链接的节点对不再生成隐含边（避免重复）
    let linked: std::collections::HashSet<(i64, i64)> = edges
        .iter()
        .map(|e| (e.from.min(e.to), e.from.max(e.to)))
        .collect();
    let mut candidates: Vec<(i64, i64, f64)> = Vec::new();
    for i in 0..page_ids.len() {
        for j in (i + 1)..page_ids.len() {
            let a = page_ids[i];
            let b = page_ids[j];
            if linked.contains(&(a.min(b), a.max(b))) {
                continue;
            }
            let ea = entity_map.get(&a);
            let eb = entity_map.get(&b);
            let shared = match (ea, eb) {
                (Some(x), Some(y)) => {
                    let (small, large) = if x.len() <= y.len() { (x, y) } else { (y, x) };
                    small.iter().filter(|n| large.contains(n)).count()
                }
                _ => 0,
            };
            if shared > 0 {
                candidates.push((a, b, shared as f64));
            }
        }
    }
    // 按共享实体数降序取前 300 条，避免知识库过大时图谱被边淹没
    candidates.sort_by(|x, y| y.2.partial_cmp(&x.2).unwrap_or(std::cmp::Ordering::Equal));
    candidates.truncate(300);
    for (a, b, w) in candidates {
        edges.push(WikiGraphEdge {
            from: a,
            to: b,
            link_type: "entity".to_string(),
            weight: w,
        });
    }
    // ── 幽灵节点：被 [[链接]] 引用但尚未创建的笔记（Obsidian 语义）。
    // 前端「仅显示已创建的笔记」开关关闭时，这些未创建笔记也会出现在图谱中，
    // 并可直接从图上创建。
    let mut ghost_nodes: Vec<WikiGraphNode> = Vec::new();
    let mut ghost_edges: Vec<(i64, i64)> = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, title, slug, content_md FROM wiki_pages WHERE kb_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![kb_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let pages: Vec<(i64, String, String, String)> = rows.filter_map(|r| r.ok()).collect();
        let existing: std::collections::HashSet<String> = pages
            .iter()
            .flat_map(|(_, t, s, _)| vec![t.clone(), s.clone()])
            .collect();
        let mut mentions: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut mentioner: std::collections::HashMap<String, std::collections::HashSet<i64>> =
            std::collections::HashMap::new();
        for (pid, _, _, content) in &pages {
            for (t, _) in extract_wiki_links(content) {
                let t = t.trim().to_string();
                if t.is_empty() || existing.contains(&t) {
                    continue;
                }
                *mentions.entry(t.clone()).or_insert(0) += 1;
                mentioner.entry(t.clone()).or_default().insert(*pid);
            }
        }
        // 排序 + 截断，避免异常内容导致图谱膨胀
        let mut titles: Vec<String> = mentions.keys().cloned().collect();
        titles.sort();
        titles.truncate(200);
        for (i, t) in titles.iter().enumerate() {
            let gid = -(i as i64 + 1);
            let who = mentioner.get(t).cloned().unwrap_or_default();
            ghost_nodes.push(WikiGraphNode {
                id: gid,
                page_id: gid,
                title: t.clone(),
                doc_id: None,
                doc_title: None,
                dir_name: None,
                status: "missing".to_string(),
                in_degree: who.len() as i64,
                out_degree: 0,
            });
            for pid in who {
                ghost_edges.push((pid, gid));
            }
        }
    }
    for (from, to) in ghost_edges {
        edges.push(WikiGraphEdge {
            from,
            to,
            link_type: "reference".to_string(),
            weight: 1.0,
        });
    }
    let mut all_nodes = nodes;
    all_nodes.extend(ghost_nodes);
    Ok(WikiGraph {
        nodes: all_nodes,
        edges,
    })
}
