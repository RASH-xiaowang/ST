// ============================================================
// 文档解析 — docx（WordprocessingML）
// 自 parse.rs 拆分：zip 解压取 document.xml、<w:t> 文本抽取。
// ============================================================

use super::split_into_sections;
use super::ParsedDoc;

/// 简易 docx 解析：解压取 word/document.xml，抽取 <w:t> 文本
pub(crate) fn parse_docx(data: &[u8]) -> Result<ParsedDoc, String> {
    // 简易 zip 扫描：定位 word/document.xml 并记录文本（避免引入 zip 依赖的复杂 API）
    let xml = extract_docx_document_xml(data)?;
    let text = extract_text_from_word_xml(&xml);
    Ok(split_into_sections(&text))
}

/// 从 docx(zip) 中读取 word/document.xml 字节（使用 zip 包）
fn extract_docx_document_xml(data: &[u8]) -> Result<String, String> {
    use std::io::Read;
    let cursor = std::io::Cursor::new(data);
    let mut zip = zip::ZipArchive::new(cursor).map_err(|e| format!("docx 解压失败: {}", e))?;
    let mut file = zip
        .by_name("word/document.xml")
        .map_err(|_| "未找到 word/document.xml".to_string())?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)
        .map_err(|e| format!("读取 document.xml 失败: {}", e))?;
    Ok(buf)
}

/// 从 WordprocessingML 提取 <w:t> 文本，按段落补换行
fn extract_text_from_word_xml(xml: &str) -> String {
    let mut out = String::new();
    let mut in_para = false;
    let bytes = xml.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"<w:t") && !bytes[i..].starts_with(b"</w:t>") {
            // 跳到标签结束的 '>'
            while i < bytes.len() && bytes[i] != b'>' {
                i += 1;
            }
            i += 1;
            // 收集 <w:t ...> 与 </w:t> 之间的字节（UTF-8 原样保留，避免逐字节转 char 产生乱码）
            let start = i;
            while i < bytes.len() && !bytes[i..].starts_with(b"</w:t>") {
                i += 1;
            }
            out.push_str(&String::from_utf8_lossy(&bytes[start..i]));
            // 跳过 </w:t>
            if i < bytes.len() {
                i += 6;
            }
            continue;
        }
        if bytes[i..].starts_with(b"<w:p") && !bytes[i..].starts_with(b"</w:p>") {
            in_para = true;
            i += 4;
            continue;
        }
        if bytes[i..].starts_with(b"</w:p>") && in_para {
            out.push('\n');
            in_para = false;
            i += 6;
            continue;
        }
        i += 1;
    }
    out
}
