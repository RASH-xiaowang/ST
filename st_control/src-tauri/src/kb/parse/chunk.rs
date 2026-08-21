// ============================================================
// 文档分片（Chunking）
// 自 parse.rs 拆分：recursive/title/parent_child 三策略 +
// 断点查找与 token 估算。
// ============================================================

use super::{ChunkConfig, ChunkStrategy};

/// 按配置的分块策略执行分片
pub fn chunk_text(text: &str, cfg: &ChunkConfig) -> Vec<Chunk> {
    match cfg.strategy {
        ChunkStrategy::Recursive => chunk_recursive(text, cfg),
        ChunkStrategy::Title => chunk_by_title(text, cfg),
        ChunkStrategy::ParentChild => chunk_parent_child(text, cfg),
    }
}

/// 递归字符分片（带重叠），尽量保留在段落/句子边界
fn chunk_recursive(text: &str, cfg: &ChunkConfig) -> Vec<Chunk> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut seq = 0usize;
    while start < chars.len() {
        let mut end = (start + cfg.chunk_size).min(chars.len());
        // 尽量在边界处断句
        if end < chars.len() {
            let boundary = find_break_point(&chars, start, end);
            if boundary > start + cfg.min_chunk {
                end = boundary;
            }
        }
        let slice: String = chars[start..end].iter().collect();
        if slice.trim().len() >= cfg.min_chunk / 2 {
            chunks.push(Chunk {
                seq,
                content: slice.trim().to_string(),
                char_start: start,
                char_end: end,
                token_count: estimate_tokens(&slice),
                section: None,
                page_no: None,
                parent_id: None,
            });
            seq += 1;
        }
        if end >= chars.len() {
            break;
        }
        // 重叠回退
        let step = end.saturating_sub(cfg.overlap);
        start = if step > start { step } else { end };
    }
    chunks
}

/// 标题感知分块：按 Markdown 标题（#~######）切分章节，
/// 分片内容携带章节标题路径前缀，section 字段记录所属章节
fn chunk_by_title(text: &str, cfg: &ChunkConfig) -> Vec<Chunk> {
    // 按行扫描，切分为 (title, 内容片段) 列表；标题行不计入内容
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut cur_title = String::new();
    let mut cur_body = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') && !trimmed.starts_with("## ") && !trimmed.starts_with("### ") {
            // 一级标题：直接开始新章节
            if !cur_title.is_empty() || !cur_body.trim().is_empty() {
                sections.push((cur_title.clone(), cur_body.clone()));
            }
            cur_title = trimmed.trim_start_matches('#').trim().to_string();
            cur_body.clear();
        } else if trimmed.starts_with('#') {
            // 二级及以下标题：追加到当前章节标题路径
            let sub = trimmed.trim_start_matches('#').trim().to_string();
            cur_title = if cur_title.is_empty() {
                sub.clone()
            } else {
                format!("{} / {}", cur_title, sub)
            };
        } else {
            cur_body.push_str(line);
            cur_body.push('\n');
        }
    }
    if !cur_title.is_empty() || !cur_body.trim().is_empty() {
        sections.push((cur_title.clone(), cur_body));
    }
    if sections.is_empty() {
        return chunk_recursive(text, cfg);
    }

    let mut chunks = Vec::new();
    let mut seq = 0usize;
    for (title, body) in &sections {
        if body.trim().is_empty() {
            continue;
        }
        let prefix = title.trim();
        let sec_chars: Vec<char> = body.chars().collect();
        let mut start = 0usize;
        while start < sec_chars.len() {
            let mut end = (start + cfg.chunk_size).min(sec_chars.len());
            if end < sec_chars.len() {
                let boundary = find_break_point(&sec_chars, start, end);
                if boundary > start + cfg.min_chunk {
                    end = boundary;
                }
            }
            let slice: String = sec_chars[start..end].iter().collect();
            if slice.trim().len() >= cfg.min_chunk / 2 {
                let content = if prefix.is_empty() {
                    slice.trim().to_string()
                } else {
                    format!("【{}】{}", prefix, slice.trim())
                };
                chunks.push(Chunk {
                    seq,
                    content,
                    char_start: start,
                    char_end: end,
                    token_count: estimate_tokens(&slice),
                    section: if prefix.is_empty() {
                        None
                    } else {
                        Some(prefix.to_string())
                    },
                    page_no: None,
                    parent_id: None,
                });
                seq += 1;
            }
            if end >= sec_chars.len() {
                break;
            }
            let step = end.saturating_sub(cfg.overlap);
            start = if step > start { step } else { end };
        }
    }
    if chunks.is_empty() {
        return chunk_recursive(text, cfg);
    }
    chunks
}

/// 父子分块：先按大粒度生成父块，再在父块内切细粒度子块。
/// 父块用于回答上下文（context），子块用于检索命中（index），子块通过 parent_id 关联父块。
fn chunk_parent_child(text: &str, cfg: &ChunkConfig) -> Vec<Chunk> {
    let parent_cfg = ChunkConfig {
        chunk_size: cfg.chunk_size.saturating_mul(3).max(1500),
        overlap: cfg.overlap,
        min_chunk: cfg.min_chunk,
        strategy: ChunkStrategy::Recursive,
    };
    let parents = chunk_recursive(text, &parent_cfg);
    if parents.is_empty() {
        return Vec::new();
    }
    let child_cfg = ChunkConfig {
        chunk_size: (cfg.chunk_size / 4).max(200),
        overlap: (cfg.overlap / 2).max(30),
        min_chunk: (cfg.min_chunk / 2).max(40),
        strategy: ChunkStrategy::Recursive,
    };
    let mut chunks = Vec::new();
    let mut seq = 0usize;
    let mut parent_chunks: Vec<Chunk> = Vec::new();
    // 先登记父块
    for p in &parents {
        let pc = Chunk {
            seq,
            content: p.content.clone(),
            char_start: p.char_start,
            char_end: p.char_end,
            token_count: p.token_count,
            section: None,
            page_no: None,
            parent_id: None,
        };
        chunks.push(pc.clone());
        parent_chunks.push(pc);
        seq += 1;
    }
    // 再切子块：子块内容为父块范围内的细粒度片段，parent_id 指向父块 seq
    for p in &parents {
        let children = chunk_recursive(&p.content, &child_cfg);
        for c in children {
            if c.content.trim().is_empty() {
                continue;
            }
            chunks.push(Chunk {
                seq,
                content: c.content,
                char_start: p.char_start + c.char_start,
                char_end: p.char_start + c.char_end,
                token_count: c.token_count,
                section: None,
                page_no: None,
                parent_id: Some(p.seq as i64),
            });
            seq += 1;
        }
    }
    // 若子块为空（父块很小），直接用父块兜底
    if chunks.len() == parent_chunks.len() {
        return parent_chunks;
    }
    chunks
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub seq: usize,
    pub content: String,
    pub char_start: usize,
    pub char_end: usize,
    pub token_count: usize,
    /// 所属章节标题路径（标题感知分块时填充）
    pub section: Option<String>,
    /// 页码（PDF 预留，暂为 None）
    pub page_no: Option<i64>,
    /// 父块 seq（父子分块中子块关联父块；父块为 None）
    pub parent_id: Option<i64>,
}

/// 在 [start, preferred_end] 范围内寻找最近的句子/段落边界
pub(crate) fn find_break_point(chars: &[char], start: usize, preferred_end: usize) -> usize {
    // 优先段落换行
    for i in (start..preferred_end).rev() {
        if chars[i] == '\n' {
            return i + 1;
        }
    }
    // 其次句末标点
    for i in (start..preferred_end).rev() {
        if matches!(chars[i], '。' | '.' | '！' | '!' | '？' | '?' | '；' | ';') {
            return i + 1;
        }
    }
    preferred_end
}

pub(crate) fn estimate_tokens(s: &str) -> usize {
    // 粗略估算：中文按字，英文按词
    let cjk = s
        .chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .count();
    let words = s.split_whitespace().count();
    cjk + words
}
