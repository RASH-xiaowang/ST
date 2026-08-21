// ============================================================
// 文档解析 — PDF
// 自 parse.rs 拆分：文本流提取、扫描件 Windows OCR 回退、
// 内嵌 JPEG 流提取。
// ============================================================

use super::split_into_sections;
use super::ParsedDoc;

/// 简易 PDF 文本提取（基于常见 "BT ... (text) Tj" 模式）
/// 注意：仅覆盖文本型 PDF，扫描版图片 PDF 需 OCR（后续接入 Python sidecar）
pub(crate) fn parse_pdf(data: &[u8]) -> Result<ParsedDoc, String> {
    let raw = String::from_utf8_lossy(data);
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    let s: String = chars.by_ref().collect();
    // 提取括号括起来的文本流
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b')' {
                i += 1;
            }
            // 按 UTF-8 字节段整体转文本，避免逐字节转 char 产生乱码
            let text = String::from_utf8_lossy(&bytes[start..i]).to_string();
            let cleaned = text.replace("\\(", "(").replace("\\)", ")");
            if !cleaned.trim().is_empty() {
                out.push_str(&cleaned);
                out.push(' ');
            }
            if i < bytes.len() {
                i += 1; // 跳过 ')'
            }
            continue;
        }
        i += 1;
    }
    if out.trim().is_empty() {
        return ocr_pdf_fallback(data);
    }
    Ok(split_into_sections(&out))
}

/// 扫描件回退：提取 PDF 内嵌 JPEG（/DCTDecode 流）并调用 Windows OCR 识别。
/// Windows 目标使用系统 OCR（Windows.Media.Ocr，无需额外依赖）；其他平台给出明确提示。
fn ocr_pdf_fallback(data: &[u8]) -> Result<ParsedDoc, String> {
    #[cfg(target_os = "windows")]
    {
        let images = extract_pdf_jpeg_streams(data);
        if images.is_empty() {
            return Err(
                "PDF 未提取到文本，也未发现可 OCR 的图片（可能为加密，或图片非 JPEG 编码）"
                    .to_string(),
            );
        }
        let mut ocr_text = String::new();
        let mut ok_pages = 0usize;
        for img in &images {
            match crate::kb::ocr::ocr_image(img) {
                Ok(t) => {
                    if !t.trim().is_empty() {
                        ocr_text.push_str(&t);
                        if !t.ends_with('\n') {
                            ocr_text.push('\n');
                        }
                        ok_pages += 1;
                    }
                }
                Err(e) => log::warn!("OCR 单页识别失败: {}", e),
            }
        }
        if ok_pages > 0 {
            return Ok(split_into_sections(&ocr_text));
        }
        Err(format!(
            "PDF 共 {} 页图片，OCR 均未识别出文字（请检查系统是否安装简体中文 OCR 语言包）",
            images.len()
        ))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = data;
        Err("PDF 未提取到文本（扫描件 OCR 目前仅支持 Windows）".to_string())
    }
}

/// 从 PDF 中提取内嵌 JPEG 流（/DCTDecode ... stream ... endstream，截到 JPEG EOI）
pub(crate) fn extract_pdf_jpeg_streams(data: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 10 < data.len() {
        if &data[i..i + 10] == b"/DCTDecode" {
            let mut j = i + 10;
            while j + 6 < data.len() && &data[j..j + 6] != b"stream" {
                j += 1;
            }
            if j + 6 >= data.len() {
                break;
            }
            j += 6;
            // 跳过 stream 后的行首换行
            while j < data.len() && (data[j] == b'\r' || data[j] == b'\n') {
                j += 1;
            }
            let mut k = j;
            while k + 9 < data.len() && &data[k..k + 9] != b"endstream" {
                k += 1;
            }
            if k + 9 >= data.len() {
                break;
            }
            let seg = &data[j..k];
            // 截到 JPEG EOI（FF D9），并校验 SOI（FF D8）
            if let Some(rel) = seg.windows(2).rposition(|w| w[0] == 0xFF && w[1] == 0xD9) {
                let img = &seg[..rel + 2];
                if img.len() > 64 && img[0] == 0xFF && img[1] == 0xD8 {
                    out.push(img.to_vec());
                }
            }
            i = k + 9;
        } else {
            i += 1;
        }
    }
    out
}
