// ============================================================
// 文档解析 — xlsx
// 自 parse.rs 拆分：共享字符串表与工作表文本提取、
// 单元格标签解析。
// ============================================================

use super::split_into_sections;
use super::ParsedDoc;

/// 简易 xlsx 解析：读取共享字符串表与第一个工作表，按行提取单元格文本。
/// 复用现有 zip crate，不引入新依赖；仅覆盖常见文本型表格。
pub(crate) fn parse_xlsx(data: &[u8]) -> Result<ParsedDoc, String> {
    let cursor = std::io::Cursor::new(data);
    let mut zip = zip::ZipArchive::new(cursor).map_err(|e| format!("xlsx 解压失败: {}", e))?;
    // 安全：条目数量与单条目解压大小都设上限，防御 zip-bomb
    super::check_zip_entry_count(&zip)?;
    // 共享字符串表（shared strings）
    let shared: Vec<String> = if let Ok(f) = zip.by_name("xl/sharedStrings.xml") {
        let buf = super::read_zip_entry_text(f, "xl/sharedStrings.xml")?;
        extract_xlsx_shared_strings(&buf)
    } else {
        Vec::new()
    };
    // 第一个工作表
    let f = zip
        .by_name("xl/worksheets/sheet1.xml")
        .map_err(|_| "未找到工作表 xl/worksheets/sheet1.xml".to_string())?;
    let buf = super::read_zip_entry_text(f, "xl/worksheets/sheet1.xml")?;
    let text = extract_xlsx_sheet_text(&buf, &shared);
    if text.trim().is_empty() {
        return Err("xlsx 未提取到文本内容（可能为空表或结构不受支持）".to_string());
    }
    Ok(split_into_sections(&text))
}

/// 提取 sharedStrings.xml 中每个 <si> 的纯文本（按出现顺序）
fn extract_xlsx_shared_strings(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = xml.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"<si") && !bytes[i..].starts_with(b"</si>") {
            if let Some(end_rel) = xml[i..].find("</si>") {
                let seg = &xml[i..i + end_rel];
                let mut text = String::new();
                // 拼接该 si 内所有 <t> 文本（富文本 run 分段）
                let mut k = 0usize;
                let sbytes = seg.as_bytes();
                while k < sbytes.len() {
                    if sbytes[k..].starts_with(b"<t") && !sbytes[k..].starts_with(b"</t>") {
                        if let Some(t) = extract_tag_content(seg, k, "t") {
                            text.push_str(&t);
                            k += t.len(); // 近似推进（仅用于避免死循环，find 兜底）
                        }
                    }
                    k += 1;
                }
                out.push(text);
                i += end_rel + 5;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// 提取单元格文本：shared（<v> 指向共享串索引）/ inlineStr（<t> 内联）/ 普通（<v> 数值）
fn extract_xlsx_sheet_text(xml: &str, shared: &[String]) -> String {
    let mut out = String::new();
    let bytes = xml.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // 单元格开始标签 <c ...>（避免误匹配 <cols>/<col>）
        if (bytes[i..].starts_with(b"<c ")
            || bytes[i..].starts_with(b"<c>")
            || bytes[i..].starts_with(b"<c/"))
            && !bytes[i..].starts_with(b"</c>")
        {
            let mut j = i;
            while j < bytes.len() && bytes[j] != b'>' {
                j += 1;
            }
            if j >= bytes.len() {
                break;
            }
            let tag = &xml[i..j];
            let is_shared = tag.contains("t=\"s\"") || tag.contains("t='s'");
            let is_inline = tag.contains("t=\"inlineStr\"") || tag.contains("t='inlineStr'");
            let cell_end = xml[j..].find("</c>").map(|k| j + k).unwrap_or(bytes.len());
            let inner = &xml[j + 1..cell_end];
            let val = if is_shared {
                extract_tag_content(inner, 0, "v")
                    .and_then(|s| s.trim().parse::<usize>().ok())
                    .and_then(|idx| shared.get(idx))
                    .cloned()
                    .unwrap_or_default()
            } else if is_inline {
                extract_tag_content(inner, 0, "t").unwrap_or_default()
            } else {
                extract_tag_content(inner, 0, "v").unwrap_or_default()
            };
            if !val.trim().is_empty() {
                out.push_str(val.trim());
                out.push('\t');
            }
            i = cell_end + 4;
            continue;
        }
        if bytes[i..].starts_with(b"</row>") {
            out.push('\n');
            i += 6;
            continue;
        }
        i += 1;
    }
    out
}

/// 从 from 位置开始查找 <tag>...</tag> 的文本内容（tag 不含命名空间前缀，如 "t"/"v"）
fn extract_tag_content(xml: &str, from: usize, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let bytes = xml.as_bytes();
    let mut i = from.min(bytes.len());
    while i + open.len() <= bytes.len() {
        if bytes[i..].starts_with(open.as_bytes()) {
            // 跳到标签结束的 '>'
            let mut j = i;
            while j < bytes.len() && bytes[j] != b'>' {
                j += 1;
            }
            let content_start = j + 1;
            let rest = &xml[content_start..];
            if let Some(end) = rest.find(&close) {
                return Some(rest[..end].to_string());
            }
            return None;
        }
        i += 1;
    }
    None
}
