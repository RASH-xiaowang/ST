// ════════════════════════════════════════════════════════════
// Wiki 数据结构（camelCase 序列化供前端直接使用）
// ════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WikiPageItem {
    pub id: i64,
    pub kb_id: i64,
    pub dir_id: Option<i64>,
    pub doc_id: Option<i64>,
    pub doc_title: Option<String>,
    pub title: String,
    pub slug: String,
    pub summary: String,
    pub status: String,
    /// 出链数
    pub out_links: i64,
    /// 入链数
    pub in_links: i64,
    /// 实体数
    pub entity_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WikiLinkInfo {
    pub page_id: i64,
    pub title: String,
    pub slug: String,
    pub link_type: String,
    pub weight: f64,
    /// 链接出现的上下文片段（Obsidian 风格的反向链接预览）
    #[serde(default)]
    pub snippet: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WikiPageDetail {
    pub id: i64,
    pub kb_id: i64,
    pub doc_id: Option<i64>,
    pub doc_title: Option<String>,
    pub title: String,
    pub slug: String,
    pub summary: String,
    pub content_md: String,
    pub status: String,
    pub created_by: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    /// 出链（本文引用的页面）
    pub out_links: Vec<WikiLinkInfo>,
    /// 入链（引用本文的页面）
    pub in_links: Vec<WikiLinkInfo>,
    /// 正文中引用但尚不存在的页面标题（失效/待建链接）
    pub unresolved: Vec<String>,
    /// 纯文本提到本页标题但未使用 `[[链接]]` 的页面（未链接提及）
    pub unlinked_mentions: Vec<WikiLinkInfo>,
    /// LLM 从正文抽取的实体
    pub entities: Vec<WikiEntity>,
    /// 摘要与实体提取状态：'' / pending / done / failed
    pub extract_status: String,
}

/// 页面实体（LLM 抽取）
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WikiEntity {
    pub id: i64,
    pub name: String,
    pub entity_type: String,
    pub description: Option<String>,
}

/// 知识图谱节点
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WikiGraphNode {
    pub id: i64,
    pub page_id: i64,
    pub title: String,
    pub doc_id: Option<i64>,
    pub doc_title: Option<String>,
    /// 所属目录名（用于图谱节点类型分类）
    pub dir_name: Option<String>,
    pub in_degree: i64,
    pub out_degree: i64,
    pub status: String,
}

/// 知识图谱边
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WikiGraphEdge {
    pub from: i64,
    pub to: i64,
    pub link_type: String,
    pub weight: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WikiGraph {
    pub nodes: Vec<WikiGraphNode>,
    pub edges: Vec<WikiGraphEdge>,
}

/// 生成请求
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WikiGenerateInput {
    pub kb_id: i64,
    /// 为空时生成知识库内全部已就绪文档
    pub doc_id: Option<i64>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
}

/// 创建/更新页面请求
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WikiPageInput {
    pub kb_id: i64,
    pub doc_id: Option<i64>,
    pub title: String,
    pub summary: Option<String>,
    pub content_md: Option<String>,
}
