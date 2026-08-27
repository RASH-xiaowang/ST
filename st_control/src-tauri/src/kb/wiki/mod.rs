// ════════════════════════════════════════════════════════════
// Wiki 模式 + 知识图谱
// 代理自动将原始文档提炼为相互链接的 Markdown 知识库（wiki 页面），
// 并以「页面 + 链接」构成可交互的知识图谱，实现知识的自我维护与可视化探索。
// ════════════════════════════════════════════════════════════
mod types;
pub use types::*;

mod utils;

mod fts;
pub use fts::*;

mod mutate;
pub use mutate::{
    create_page, delete_page, list_versions, restore_version, update_page, WikiVersionItem,
};

mod generate;
pub use generate::{generate, generate_with_jobs, list_ready_docs};

mod extract;
pub use extract::extract_page_meta;

mod query;
pub use query::{get_page, graph, list_pages};

/// wiki 页面完整行（SELECT：id, kb_id, doc_id, doc_title, title, slug, summary,
/// content_md, status, created_by, created_at, updated_at）
struct WikiPageRow(
    i64,
    i64,
    Option<i64>,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
    Option<i64>,
    String,
    String,
);

#[cfg(test)]
mod tests;
