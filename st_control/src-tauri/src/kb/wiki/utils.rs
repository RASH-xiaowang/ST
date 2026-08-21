// ════════════════════════════════════════════════════════════
// Wiki 工具函数
// 自 wiki.rs 拆分：[[链接]] 提取、slugify、LLM 截断与空串转 None。
// ════════════════════════════════════════════════════════════

// ────────────────────────────────────────────────────────────
// 工具
// ────────────────────────────────────────────────────────────

/// 提取 Markdown 中的 [[标题]] 链接（带出现次数）
pub(crate) fn extract_wiki_links(md: &str) -> Vec<(String, usize)> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let bytes = md.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            if let Some(close) = md[i + 2..].find("]]") {
                let title = md[i + 2..i + 2 + close].trim().to_string();
                if !title.is_empty() {
                    *counts.entry(title).or_insert(0) += 1;
                }
                i += 2 + close + 2;
                continue;
            }
        }
        i += 1;
    }
    counts.into_iter().collect()
}

/// URL 友好的 slug：小写、去空格、非字母数字替换为 `-`
pub(crate) fn slugify(title: &str) -> String {
    let mut out = String::new();
    for c in title.trim().chars() {
        if c.is_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    // 连续分隔符合并为单个 '-'
    let mut slug = String::new();
    let mut prev_dash = false;
    for c in out.trim_matches('-').to_lowercase().chars() {
        if c == '-' {
            if !prev_dash {
                slug.push(c);
            }
            prev_dash = true;
        } else {
            slug.push(c);
            prev_dash = false;
        }
    }
    if slug.is_empty() {
        "page".to_string()
    } else {
        slug
    }
}

/// 超长文本截断（按字符）
pub(crate) fn truncate_for_llm(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        return s.to_string();
    }
    let mut cut = chars[..max_chars].to_vec();
    // 尝试在最后一个句号/换行处断开
    let mut break_at = max_chars;
    for idx in (max_chars.saturating_sub(200)..max_chars).rev() {
        if matches!(chars[idx], '。' | '！' | '？' | '\n' | '.') {
            break_at = idx + 1;
            break;
        }
    }
    cut.truncate(break_at);
    let mut s: String = cut.into_iter().collect();
    s.push_str("\n\n…（内容过长已截断，如需完整提炼请先压缩文档）");
    s
}

/// 辅助：空字符串 → None
pub(crate) trait OptionNone {
    fn into_none(self) -> Option<String>;
}
impl OptionNone for String {
    fn into_none(self) -> Option<String> {
        if self.is_empty() {
            None
        } else {
            Some(self)
        }
    }
}
