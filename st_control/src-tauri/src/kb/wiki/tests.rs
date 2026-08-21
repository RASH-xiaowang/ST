// ════════════════════════════════════════════════════════════
// Wiki 单元测试
// 覆盖：utils（slugify/链接提取/截断）、query（snippet 多字节边界）、
// extract（refined pages 解析与回退）。
// ════════════════════════════════════════════════════════════

use super::extract::parse_refined_pages;
use super::query::{link_snippet, plain_snippet};
use super::utils::{extract_wiki_links, slugify, truncate_for_llm};

#[test]
fn test_slugify() {
    assert_eq!(slugify("Hello World"), "hello-world");
    assert_eq!(slugify("知识库 架构"), "知识库-架构");
    assert_eq!(slugify("   "), "page");
    assert_eq!(slugify("A/B_C"), "a-b_c");
}

#[test]
fn test_extract_wiki_links() {
    let md = "参考 [[架构设计]] 与 [[权限模型]]，参见 [[架构设计]]。";
    let links = extract_wiki_links(md);
    assert_eq!(links.len(), 2, "应有两个不同目标");
    let map: std::collections::HashMap<_, _> = links.into_iter().collect();
    assert_eq!(map.get("架构设计"), Some(&2), "重复引用应计数 2");
    assert_eq!(map.get("权限模型"), Some(&1));
}

#[test]
fn test_extract_wiki_links_no_links() {
    assert!(extract_wiki_links("没有链接的普通文本").is_empty());
}

#[test]
fn test_parse_refined_pages() {
    let raw = "intro<<<PAGE>>>\n架构总览\n本文档介绍整体架构。\n---\n# 架构\n核心是 [[分片策略]]。\n<<<END>>>\n<<<PAGE>>>\n分片策略\n分片细节。\n---\n正文内容。\n<<<END>>>";
    let pages = parse_refined_pages(raw).unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].title, "架构总览");
    assert_eq!(pages[0].summary, "本文档介绍整体架构。");
    assert!(pages[0].content.contains("分片策略"));
    assert_eq!(pages[1].title, "分片策略");
}

#[test]
fn test_parse_refined_pages_fallback() {
    let raw = "模型没有按格式输出，直接给了一段话。";
    let pages = parse_refined_pages(raw).unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].title, "知识库总览");
    assert!(pages[0].content.contains("没有按格式"));
}

#[test]
fn test_truncate_for_llm_short_text() {
    let s = "短文本";
    assert_eq!(truncate_for_llm(s, 1000), "短文本");
}

#[test]
fn test_truncate_for_llm_long_text() {
    let s = "第一句。".repeat(300);
    let t = truncate_for_llm(&s, 500);
    assert!(t.chars().count() <= 500 + 200, "截断后不应超过上限太多");
    assert!(t.contains("已截断"), "应带有截断提示");
}

#[test]
fn test_truncate_breaks_at_sentence() {
    let s = format!("{}。", "句子".repeat(300));
    let t = truncate_for_llm(&s, 400);
    assert!(t.ends_with("…（内容过长已截断，如需完整提炼请先压缩文档）"));
}

#[test]
fn test_link_snippet_multibyte_no_panic() {
    // 中文长行：按字节 87 截断必须回退到字符边界，不 panic
    let md = format!(
        "[[测试页]]{}",
        "很长的中文内容段落，用于验证片段截断逻辑不会在非 UTF-8 边界 panic。".repeat(10)
    );
    let s = link_snippet(&md, "测试页");
    assert!(s.is_some());
    assert!(s
        .as_deref()
        .unwrap()
        .chars()
        .all(|c| c.is_alphanumeric() || c.is_whitespace() || "[[：。，！？…]]".contains(c)));
}

#[test]
fn test_link_snippet_lowercase_length_change_skips() {
    // İ → i̇ 会让 to_lowercase 改变字节长度：应安全跳过而非越界 panic
    let md = "参考 [[İçerik]] 的内容。";
    assert!(link_snippet(md, "i̇çerik").is_none());
    assert!(plain_snippet(md, "i̇çerik").is_none());
}
