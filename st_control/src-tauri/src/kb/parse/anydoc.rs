// ============================================================
// 文档解析 — anydoc 引擎（doc/ppt/xls/odt/rtf/epub/pdf → GFM）
// 自 parse.rs 拆分：多格式统一走 Firecrawl anydoc 转换器。
// ============================================================

use super::split_into_sections;
use super::ParsedDoc;

/// 使用 anydoc（Firecrawl 开源的纯 Rust 文档 → GFM Markdown 转换器，MIT）解析。
/// 覆盖 doc/docx/docm/ppt/pps/pot/pptx/pptm/ppsx/ppsm/xls/xlsx/xlsm/xlsb/
/// odt/ods/odp/rtf/epub/pdf；输出 Markdown，可直接用于标题感知分片。
/// 优先按扩展名指定解析器，失败时再按内容自动识别（容错错误/缺失扩展名）。
pub(crate) fn parse_with_anydoc(file_type: &str, data: &[u8]) -> Result<ParsedDoc, String> {
    let fmt = anydoc::Format::from_extension(file_type);
    let md = anydoc::to_markdown_bytes(data, fmt)
        .or_else(|_| anydoc::to_markdown_bytes(data, None))
        .map_err(|e| format!("{} 解析失败: {}", file_type, e))?;
    if md.trim().is_empty() {
        return Err(format!(
            "{} 未提取到文本内容（可能为空文档或加密）",
            file_type
        ));
    }
    Ok(split_into_sections(&md))
}
