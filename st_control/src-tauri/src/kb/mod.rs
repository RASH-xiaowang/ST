// ============================================================
// 知识库管理模块 (Knowledge Base)
// 子模块：
//  - db.rs        : 知识库独立 SQLite 库（业务数据 + 向量以 BLOB 存储）
//  - parse.rs     : 文档解析与分片
//  - embed.rs     : 向量化（复用 llm::handlers::create_embedding）
//  - retrieval.rs : 混合检索（向量 + BM25） + RRF 融合 + 权限过滤
//  - rag.rs       : RAG 检索增强生成（上下文组装 + 高亮定位）
//  - wiki.rs      : Wiki 页面管理（页面 CRUD + 链接图 + LLM 自动提炼）
//  - handlers.rs  : Tauri IPC 命令入口
// ============================================================

pub mod auth;
pub mod db;
pub mod embed;
pub mod handlers;
#[cfg(target_os = "windows")]
pub mod ocr;
pub mod parse;
pub mod rag;
pub mod retrieval;
pub mod wiki;

/// 在连续汉字之间插入空格，使 FTS5 unicode61 将每个汉字视为独立 token，
/// 从而支持中文子串/短语检索（FTS 索引写入与查询词两侧必须做同样处理）。
pub(crate) fn cjk_spaced(s: &str) -> String {
    fn is_cjk(c: char) -> bool {
        matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}')
    }
    let mut out = String::with_capacity(s.len() + 8);
    let mut prev_cjk = false;
    for c in s.chars() {
        let cur_cjk = is_cjk(c);
        if prev_cjk && cur_cjk {
            out.push(' ');
        }
        out.push(c);
        prev_cjk = cur_cjk;
    }
    out
}
