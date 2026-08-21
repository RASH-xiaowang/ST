// ============================================================
// Harness — Web 能力（DSH web 迁移）
//
// 能力接缝三角色：
// - Service Definition：WebService::search / fetch
// - Service Provider：Bing（cn/www 双域兜底，解析 b_algo 条目）+
//   HTTP 抓取（去标签正文，8KB 截断）
// - Consumer：内置工具 web_search / fetch_web_page（AI 聊天与
//   Harness 共用同一实现；实现自 llm/agent.rs 上移至本模块）
// ============================================================

use serde_json::json;
use std::time::Duration;

/// Web 能力服务（Bing 搜索 + DeepSeek 搜索提供商 + 网页抓取）
pub struct WebService;

/// Bing 搜索：返回标题/链接/摘要 JSON 数组（最多 8 条；cn/www 双域兜底）
fn search_bing(query: &str) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("缺少 query 参数".to_string());
    }
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36")
        .timeout(Duration::from_secs(15))
        .no_proxy()
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut last_err = String::new();
    for base in ["https://cn.bing.com/search", "https://www.bing.com/search"] {
        let url = format!("{}?q={}&mkt=zh-CN&setlang=zh-CN", base, urlencoding(query));
        match client.get(&url).send() {
            Ok(resp) if resp.status().is_success() => match resp.text() {
                Ok(html) => {
                    results = parse_bing_results(&html);
                    if !results.is_empty() {
                        break;
                    }
                    last_err = "Bing 未返回可解析的结果".to_string();
                }
                Err(e) => last_err = format!("读取响应失败: {}", e),
            },
            Ok(resp) => last_err = format!("搜索返回 HTTP {}", resp.status()),
            Err(e) => last_err = format!("请求搜索失败: {}", e),
        }
    }
    if results.is_empty() {
        return Err(format!("搜索失败: {}", last_err));
    }
    serde_json::to_string(&results).map_err(|e| e.to_string())
}

/// DeepSeek 搜索（DSH web-search-deepseek 语义）：Anthropic 兼容 Messages API
/// 加原生 web_search 服务器工具，解析 web_search_tool_result 结构化块。
/// 凭据与端点取自全局配置中第一个 DeepSeek 类型提供方（base_url 或名称含
/// deepseek 的启用提供方）。
fn search_deepseek(query: &str) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("缺少 query 参数".to_string());
    }
    let cfg = crate::llm::config::load_config();
    // DeepSeek 提供方识别：base_url 或名称含 deepseek（DeepSeek 在 ST 中属
    // OpenAI 兼容类型，无独立 ProviderType 变体）
    let provider = cfg
        .providers
        .iter()
        .find(|p| {
            p.enabled
                && (p.base_url.to_lowercase().contains("deepseek")
                    || p.name.to_lowercase().contains("deepseek"))
        })
        .cloned()
        .ok_or_else(|| "未配置 DeepSeek 提供方（搜索提供商需要其 API 密钥与端点）".to_string())?;
    let api_key = provider.api_key.trim();
    if api_key.is_empty() {
        return Err("DeepSeek 提供方未配置 API 密钥".to_string());
    }
    // Anthropic 兼容端点：{base}/anthropic/v1/messages（与 chat-completions base 区分）
    let base = provider.base_url.trim_end_matches('/');
    let endpoint = if base.ends_with("/anthropic/v1") || base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/anthropic/v1/messages")
    };
    let body = json!({
        "model": provider.default_model.clone(),
        "max_tokens": 2048,
        "messages": [{ "role": "user", "content": format!("Perform a web search for the query: {query}") }],
        "tools": [{ "type": "web_search_20250305", "name": "web_search", "max_uses": 5 }],
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    let resp = client
        .post(&endpoint)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .map_err(|e| format!("DeepSeek 搜索请求失败: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        return Err(format!("DeepSeek 搜索返回 HTTP {}: {}", status, text));
    }
    let text = resp.text().map_err(|e| e.to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("解析 DeepSeek 搜索响应失败: {}", e))?;
    parse_deepseek_results(&value)
}

/// DeepSeek 搜索响应解析：提取 web_search_tool_result 结构化块
/// （不信任模型散文）；空 url 跳过；无结果报错；最多 8 条。
/// 独立函数便于单测（与 search_deepseek 共用）。
fn parse_deepseek_results(value: &serde_json::Value) -> Result<String, String> {
    let mut results: Vec<serde_json::Value> = Vec::new();
    if let Some(blocks) = value.get("content").and_then(|c| c.as_array()) {
        for block in blocks {
            if block.get("type").and_then(|t| t.as_str()) != Some("web_search_tool_result") {
                continue;
            }
            if let Some(items) = block.get("web_search_result").and_then(|r| r.as_array()) {
                for item in items {
                    let url = item.get("url").and_then(|u| u.as_str()).unwrap_or("");
                    let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("");
                    if url.is_empty() {
                        continue;
                    }
                    results.push(json!({
                        "title": title,
                        "url": url,
                        "snippet": "",
                    }));
                }
            }
        }
    }
    if results.is_empty() {
        return Err("DeepSeek 搜索未返回结构化结果（web_search_tool_result 缺失）".to_string());
    }
    results.truncate(8);
    serde_json::to_string(&results).map_err(|e| e.to_string())
}

impl WebService {
    /// 联网搜索（提供商缝：settings.web_search_provider 选择 bing / deepseek；
    /// DSH web 能力接缝的 provider 可插拔语义）
    pub fn search(&self, query: &str) -> Result<String, String> {
        let provider = crate::harness::settings::current().effective_web_search_provider();
        if provider == "deepseek" {
            return search_deepseek(query);
        }
        search_bing(query)
    }

    /// 抓取网页正文（去标签，8KB 截断；仅 http/https，拒绝内嵌凭据）
    pub fn fetch(&self, url: &str) -> Result<String, String> {
        let url = url.trim();
        if url.is_empty() {
            return Err("缺少 url 参数".to_string());
        }
        // DSH web-fetch 卫生校验：协议解析 + 无内嵌凭据（user:pass@）
        let parsed = reqwest::Url::parse(url).map_err(|e| format!("URL 无效: {}", e))?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err("仅支持 http/https 链接".to_string());
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("URL 不允许包含凭据（user:pass@）".to_string());
        }
        let client = reqwest::blocking::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36")
            .timeout(Duration::from_secs(15))
            .no_proxy()
            .build()
            .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
        let resp = client
            .get(parsed)
            .send()
            .map_err(|e| format!("请求失败: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("网页返回 HTTP {}", resp.status()));
        }
        let body = resp.text().map_err(|e| format!("读取响应失败: {}", e))?;
        let compact = strip_tags(&body)
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(truncate_chars(&compact, 8000))
    }
}

/// 注册 Web 能力（Cordis-lite 服务）
pub fn provide_service() -> crate::harness::registry::Disposer {
    crate::harness::registry::provide("harness.web", std::sync::Arc::new(WebService))
}

/// 解析 Bing HTML 结果页的 b_algo 条目（标题 + 链接 + 摘要）
fn parse_bing_results(html: &str) -> Vec<serde_json::Value> {
    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut rest = html;
    for _ in 0..16 {
        let Some(li) = rest.find("<li class=\"b_algo") else {
            break;
        };
        let Some(h2) = rest[li..].find("<h2") else {
            rest = &rest[li + 10..];
            continue;
        };
        let seg = &rest[li + h2..];
        let Some(a) = seg.find("<a ") else {
            rest = &rest[li + h2 + 4..];
            continue;
        };
        let Some(href_rel) = seg[a..].find("href=\"") else {
            rest = &rest[li + h2 + 4..];
            continue;
        };
        let href_abs = a + href_rel + 6;
        let Some(href_end) = seg[href_abs..].find('"') else {
            break;
        };
        let url = seg[href_abs..href_abs + href_end].to_string();
        let Some(t0) = seg[href_abs + href_end..].find('>') else {
            break;
        };
        let title_start = href_abs + href_end + t0 + 1;
        let Some(t_end) = seg[title_start..].find("</a>") else {
            break;
        };
        let title = decode_entities(&strip_tags(&seg[title_start..title_start + t_end]))
            .trim()
            .to_string();
        let after = title_start + t_end;
        if title.is_empty() {
            rest = &rest[li + h2 + after..];
            continue;
        }
        let snippet = match seg[after..].find("<p") {
            Some(p0) => {
                let p_abs = after + p0;
                let Some(g) = seg[p_abs..].find('>') else {
                    break;
                };
                let s0 = p_abs + g + 1;
                let Some(pe) = seg[s0..].find("</p>") else {
                    break;
                };
                decode_entities(&strip_tags(&seg[s0..s0 + pe]))
            }
            None => String::new(),
        };
        out.push(json!({ "title": title, "url": url, "snippet": snippet.trim() }));
        rest = &rest[li + h2 + after..];
        if out.len() >= 8 {
            break;
        }
    }
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn urlencoding(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
        .replace("&hellip;", "…")
}

fn truncate_chars(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        s.to_string()
    } else {
        format!("{}…", chars[..n].iter().collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bing_results_parsed() {
        let html = r#"<html><body>
        <li class="b_algo" data-id iid=SERP.1><link rel="stylesheet" href="x.css"/></li>
        <li class="b_algo" data-id iid=SERP.2><h2 class=""><a target="_blank" href="https://example.com/a"><strong>标题</strong>一</a></h2>
        <div class="b_caption"><p class="b_lineclamp2">这是<b>摘要</b>内容一。</p></div></li>
        <li class="b_algo"><h2><a target="_blank" href="https://example.com/b">标题二</a></h2>
        <div class="b_caption"><p>摘要内容二。</p></div></li>
        </body></html>"#;
        let out = parse_bing_results(html);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["title"], "标题一");
        assert_eq!(out[0]["url"], "https://example.com/a");
        assert_eq!(out[0]["snippet"], "这是摘要内容一。");
    }

    #[test]
    fn fetch_rejects_non_http() {
        let svc = WebService;
        assert!(svc.fetch("file:///etc/passwd").is_err());
        assert!(svc.fetch("").is_err());
        // DSH 卫生校验：内嵌凭据拒绝（user:pass@）
        assert!(
            svc.fetch("https://user:pass@example.com/page").is_err(),
            "内嵌凭据应拒绝"
        );
        assert!(
            svc.fetch("https://user@example.com/page").is_err(),
            "用户名也应拒绝"
        );
        // 合法 URL 通过校验层（不报凭据校验错误；请求结果依赖网络——
        // 校验放行后的传输层错误可能含 URL 字样，故仅断言不含凭据字样）
        let r = svc.fetch("https://example.com");
        if let Err(e) = r {
            assert!(!e.contains("凭据"), "合法 URL 不应报凭据校验错误: {e}");
        }
    }

    #[test]
    fn deepseek_results_parsed_from_structured_blocks() {
        // web_search_tool_result 结构化块提取（B17）：混合块只取工具结果；
        // 空 url 跳过；最多 8 条
        let resp = json!({
            "content": [
                { "type": "text", "text": "模型散文（忽略）" },
                { "type": "web_search_tool_result", "web_search_result": [
                    { "url": "https://a.com", "title": "A" },
                    { "url": "", "title": "空url跳过" },
                    { "url": "https://b.com", "title": "B" },
                ]},
                { "type": "other", "x": 1 },
            ]
        });
        let out = parse_deepseek_results(&resp).unwrap();
        let arr: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        assert_eq!(arr.len(), 2, "应提取 2 条（空 url 跳过）: {out}");
        assert_eq!(arr[0]["url"], "https://a.com");
        assert_eq!(arr[1]["title"], "B");
        // 无结果块 → 报错
        let empty = json!({ "content": [ { "type": "text", "text": "hi" } ] });
        assert!(parse_deepseek_results(&empty).is_err());
        // 超 8 条截断
        let many_items: Vec<serde_json::Value> = (0..12)
            .map(|i| json!({ "url": format!("https://{i}.com"), "title": format!("T{i}") }))
            .collect();
        let many = json!({ "content": [ { "type": "web_search_tool_result", "web_search_result": many_items } ] });
        let out = parse_deepseek_results(&many).unwrap();
        let arr: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        assert_eq!(arr.len(), 8, "结果应截断到 8 条");
    }

    #[test]
    fn html_helpers_strip_tags_and_decode_entities() {
        // 抓取正文 / Bing 解析共用的 HTML 辅助
        assert_eq!(strip_tags("<p>hello <b>world</b></p>"), "hello world");
        assert_eq!(
            strip_tags("a < b > c"),
            "a  c",
            "标签外 < > 也被吞（简单实现）"
        );
        // 标签外内容保留（简单实现：仅吞 <...> 段，script 内文本保留）
        assert_eq!(strip_tags("<script>alert(1)</script>safe"), "alert(1)safe");
        // 实体解码
        assert_eq!(
            decode_entities(
                "a&amp;b &lt;x&gt; &quot;q&quot; &#x27;it&#x27; nbsp&nbsp; end&hellip;"
            ),
            "a&b <x> \"q\" 'it' nbsp  end…"
        );
        // url 编码：保留字母数字与 -_.~，其余 %XX
        assert_eq!(urlencoding("a b/c"), "a%20b%2Fc");
        assert_eq!(urlencoding("中文"), "%E4%B8%AD%E6%96%87");
        // 字符截断（字符边界，中文安全；超长必加省略号）
        assert_eq!(truncate_chars("abcdef", 3), "abc…");
        assert_eq!(truncate_chars("abcdef", 10), "abcdef");
        assert_eq!(truncate_chars("中文测试", 2), "中文…");
    }
}
