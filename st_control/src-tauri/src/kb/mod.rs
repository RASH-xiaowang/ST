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

/// 判断字符是否为 CJK 统一表意文字（与 cjk_spaced 的判定保持一致）
fn is_cjk_char(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}')
}

/// 判断字符是否为中文/全角标点（索引端不会为其建 token，查询端需过滤）
fn is_cjk_punct(c: char) -> bool {
    matches!(
        c,
        '\u{3000}'..='\u{303F}' // CJK 符号与标点：、。，《》【】…
        | '\u{FF00}'..='\u{FFEF}' // 全角标点：，！？；：等
        | '\u{2018}'..='\u{201D}' // 中文引号：‘ ’ “ ”
        | '\u{2014}' | '\u{2026}' // 破折号 — 与省略号 …
        | '·'
    )
}

/// 将单个查询词转换为 FTS5 安全子句。
///
/// - 纯 ASCII/数字词：整词加引号做短语精确匹配；
/// - 中文：
///   - 不含标点的短短语（≤ 4 字）加引号做短语匹配（保持原精准行为）；
///   - 较长句子或含标点按「单字 OR」展开：避免把无空格的中文整句当作必须逐字
///     连续的短语（原实现导致中文整句查询 0 命中），OR 召回后由 bm25 排序与
///     向量精排保证相关性；
/// - 过滤中文/全角标点，避免生成索引中不存在的 token。
fn fts_token_clause(raw: &str) -> String {
    // 清洗 FTS5 保留字符
    let cleaned: String = raw
        .chars()
        .filter(|c| !matches!(c, '"' | '(' | ')' | '*' | '^' | ':' | '-'))
        .collect();
    if cleaned.is_empty() {
        return String::new();
    }
    // 统一去除中文/全角标点：纯标点词（如 ……）应返回空，避免生成无效 MATCH 子句
    let no_punct: String = cleaned.chars().filter(|c| !is_cjk_punct(*c)).collect();
    if no_punct.is_empty() {
        return String::new();
    }
    if !no_punct.chars().any(is_cjk_char) {
        return format!("\"{}\"", cjk_spaced(&no_punct));
    }
    let spaced = cjk_spaced(&no_punct);
    let terms: Vec<&str> = spaced.split_whitespace().collect();
    if terms.is_empty() {
        return String::new();
    }
    if terms.len() <= 4 {
        format!("\"{}\"", terms.join(" "))
    } else {
        terms.join(" OR ")
    }
}

/// 将用户查询转为 FTS5 安全查询（KB 分片全文检索与 Wiki 页面全文检索共用）。
///
/// 各空白分隔词转为独立子句后以空格连接（FTS5 默认 AND）。
pub(crate) fn fts_safe_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(fts_token_clause)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
