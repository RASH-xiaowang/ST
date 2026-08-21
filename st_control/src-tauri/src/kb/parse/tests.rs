// ============================================================
// 文档解析与分片 — 单元测试
// ============================================================

use super::chunk::{estimate_tokens, find_break_point};
use super::pdf::extract_pdf_jpeg_streams;
use super::*;

#[test]
fn test_strategy_from_str() {
    assert_eq!(
        "recursive".parse::<ChunkStrategy>().unwrap(),
        ChunkStrategy::Recursive
    );
    assert_eq!(
        "TITLE".parse::<ChunkStrategy>().unwrap(),
        ChunkStrategy::Title
    );
    assert_eq!(
        "parent_child".parse::<ChunkStrategy>().unwrap(),
        ChunkStrategy::ParentChild
    );
    assert_eq!(
        "parent-child".parse::<ChunkStrategy>().unwrap(),
        ChunkStrategy::ParentChild
    );
    assert_eq!(
        "parentchild".parse::<ChunkStrategy>().unwrap(),
        ChunkStrategy::ParentChild
    );
    assert_eq!(
        "unknown".parse::<ChunkStrategy>().unwrap(),
        ChunkStrategy::Recursive
    );
}

#[test]
fn test_default_config() {
    let cfg = ChunkConfig::default();
    assert_eq!(cfg.chunk_size, 800);
    assert_eq!(cfg.overlap, 120);
    assert_eq!(cfg.min_chunk, 100);
    assert_eq!(cfg.strategy, ChunkStrategy::Recursive);
}

#[test]
fn test_recursive_chunking_limits_and_seq() {
    let text = "这是一个测试段落，用于验证递归分片的基本行为。".repeat(100);
    let cfg = ChunkConfig::default();
    let chunks = chunk_text(&text, &cfg);
    assert!(!chunks.is_empty(), "长文本应产生分片");
    for (i, c) in chunks.iter().enumerate() {
        assert_eq!(c.seq, i, "seq 必须连续");
        assert!(
            c.content.chars().count() <= cfg.chunk_size,
            "分片长度不得超过 chunk_size"
        );
        assert!(!c.content.trim().is_empty());
        assert!(c.parent_id.is_none(), "recursive 策略无父子关联");
    }
}

#[test]
fn test_recursive_single_chunk() {
    let text = "a".repeat(500);
    let chunks = chunk_text(&text, &ChunkConfig::default());
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].content, "a".repeat(500));
}

#[test]
fn test_recursive_empty_text() {
    assert!(chunk_text("", &ChunkConfig::default()).is_empty());
}

#[test]
fn test_recursive_overlap() {
    let text = "x".repeat(2000);
    let cfg = ChunkConfig {
        chunk_size: 800,
        overlap: 120,
        min_chunk: 100,
        strategy: ChunkStrategy::Recursive,
    };
    let chunks = chunk_text(&text, &cfg);
    assert!(chunks.len() >= 2, "2000 字符应产生多个分片");
    for w in chunks.windows(2) {
        assert!(w[1].char_start < w[0].char_end, "相邻分片应存在重叠窗口");
    }
}

#[test]
fn test_recursive_tiny_text_no_chunk() {
    // 内容长度不足 min_chunk/2 时不产出分片（防碎片）
    assert!(chunk_text("短。", &ChunkConfig::default()).is_empty());
}

#[test]
fn test_title_strategy_section_prefix() {
    let text =
        "# 第一章\n第一章的正文内容。\n## 第一节\n第一节的正文内容。\n# 第二章\n第二章的正文内容。";
    let cfg = ChunkConfig {
        chunk_size: 200,
        overlap: 20,
        min_chunk: 20,
        strategy: ChunkStrategy::Title,
    };
    let chunks = chunk_text(text, &cfg);
    assert!(!chunks.is_empty(), "应产生分片");
    assert!(
        chunks.iter().all(|c| c.section.is_some()),
        "title 策略所有分片应携带章节"
    );
    let joined: String = chunks.iter().map(|c| c.content.as_str()).collect();
    assert!(joined.contains("第一章"), "应包含章节前缀");
    assert!(joined.contains("第二章"), "应包含章节前缀");
    assert!(
        chunks
            .iter()
            .any(|c| c.section.as_deref() == Some("第一章 / 第一节")),
        "二级标题应拼接为章节路径"
    );
}

#[test]
fn test_title_strategy_falls_back_to_recursive() {
    // 无标题的纯文本应回退到递归分片
    let text = "没有标题的普通文本。".repeat(50);
    let cfg = ChunkConfig {
        chunk_size: 200,
        overlap: 20,
        min_chunk: 20,
        strategy: ChunkStrategy::Title,
    };
    let chunks = chunk_text(&text, &cfg);
    assert!(!chunks.is_empty());
}

#[test]
fn test_parent_child_links() {
    let text = "这是一段用于验证父子分块策略的测试内容，包含足够多的文字以产生多个父块与子块。"
        .repeat(200);
    let cfg = ChunkConfig {
        chunk_size: 800,
        overlap: 120,
        min_chunk: 100,
        strategy: ChunkStrategy::ParentChild,
    };
    let chunks = chunk_text(&text, &cfg);
    assert!(!chunks.is_empty());
    let parents: Vec<&Chunk> = chunks.iter().filter(|c| c.parent_id.is_none()).collect();
    let children: Vec<&Chunk> = chunks.iter().filter(|c| c.parent_id.is_some()).collect();
    assert!(!parents.is_empty(), "应存在父块");
    assert!(!children.is_empty(), "应存在子块");
    assert!(children.len() > parents.len(), "子块数量应多于父块");
    let parent_seqs: std::collections::HashSet<i64> =
        parents.iter().map(|c| c.seq as i64).collect();
    for c in &children {
        assert!(
            parent_seqs.contains(&c.parent_id.unwrap()),
            "子块 parent_id 应指向存在的父块 seq"
        );
    }
}

#[test]
fn test_parent_child_tiny_text_no_chunk() {
    // 极短文本连父块都无法生成（低于 min_chunk/2），返回空（防碎片）
    let text = "极短文本。";
    let cfg = ChunkConfig {
        chunk_size: 800,
        overlap: 120,
        min_chunk: 100,
        strategy: ChunkStrategy::ParentChild,
    };
    assert!(chunk_text(text, &cfg).is_empty());
}

// ---------- 边界查找 ----------

#[test]
fn test_find_break_point_newline_first() {
    let chars: Vec<char> = "第一段内容\n第二段内容。".chars().collect();
    // \n 位于索引 5，断点应落在其后的 6
    assert_eq!(find_break_point(&chars, 0, 8), 6);
}

#[test]
fn test_find_break_point_punctuation() {
    let chars: Vec<char> = "第一句话。第二句话。第三句话。".chars().collect();
    // 从 preferred_end 反向找最近的句末标点：
    // 索引 9 是第二个句号（第三句话之前），断点为 10
    assert_eq!(find_break_point(&chars, 0, 12), 10);
}

#[test]
fn test_find_break_point_punctuation_short_range() {
    let chars: Vec<char> = "第一句话。第二句话。第三句话。".chars().collect();
    // preferred_end=8 覆盖到第二句内部，最近标点是索引 4 的句号，断点为 5
    assert_eq!(find_break_point(&chars, 0, 8), 5);
}

#[test]
fn test_find_break_point_fallback() {
    // 无换行无标点时直接返回 preferred_end
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyz".chars().collect();
    assert_eq!(find_break_point(&chars, 0, 10), 10);
    assert_eq!(find_break_point(&chars, 5, 10), 10);
}

#[test]
fn test_find_break_point_negative_none() {
    // 起始位置本身无边界时，fallback 到 end
    let chars: Vec<char> = "中文字符".chars().collect();
    assert_eq!(find_break_point(&chars, 0, 3), 3);
}

// ---------- token 估算 ----------

#[test]
fn test_estimate_tokens() {
    assert_eq!(estimate_tokens(""), 0);
    assert_eq!(estimate_tokens("hello world"), 2); // 英文按词
    assert_eq!(estimate_tokens("你好世界"), 5); // 中文按字 + 词
    assert_eq!(estimate_tokens("你好 world"), 4); // 中英混合
}

// ---------- 文档解析 ----------

#[test]
fn test_parse_document_txt_md_csv_log() {
    let txt = parse_document("txt", b"hello\nworld").unwrap();
    assert!(txt.text.contains("hello"));
    let md = parse_document("md", "# 标题\n正文内容".as_bytes()).unwrap();
    assert_eq!(md.sections.len(), 2, "标题段 + 尾部段");
    assert_eq!(md.sections[1].title.as_deref(), Some("标题"));
    let csv = parse_document("csv", b"a,b\n1,2").unwrap();
    assert!(csv.text.contains("1,2"));
    let log = parse_document("log", b"INFO something").unwrap();
    assert!(log.text.contains("INFO"));
}

#[test]
fn test_parse_document_unsupported() {
    assert!(parse_document("exe", b"\x00\x01").is_err());
    assert!(parse_document("", b"").is_err());
}

#[test]
fn test_parse_pdf_text_extraction() {
    let pdf = b"BT (Hello World) Tj ET";
    let doc = parse_pdf(pdf).unwrap();
    assert!(doc.text.contains("Hello World"));
}

#[test]
fn test_parse_pdf_utf8_text_extraction() {
    // 回归：UTF-8 多字节文本不应逐字节转 char 变成乱码
    let pdf = "BT (中文内容 hello) Tj ET".as_bytes().to_vec();
    let doc = parse_pdf(&pdf).unwrap();
    assert!(
        doc.text.contains("中文内容"),
        "PDF 中文应完整保留，实际: {}",
        doc.text
    );
}

#[test]
fn test_extract_pdf_jpeg_streams() {
    // 构造含一个 DCTDecode 内嵌 JPEG 的伪 PDF 字节
    let mut jpeg = vec![0xFFu8, 0xD8]; // SOI
    jpeg.extend_from_slice(&[0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F']); // APP0
    jpeg.extend_from_slice(&vec![0x00u8; 80]); // 填充数据段（模拟压缩数据，超过最小长度启发式）
    jpeg.push(0xFF);
    jpeg.push(0xD9); // EOI
    let mut pdf = Vec::new();
    pdf.extend_from_slice(
        b"%PDF-1.4\n1 0 obj\n<< /Subtype /Image /Width 1 /Height 1 /Filter /DCTDecode >>\nstream\n",
    );
    pdf.extend_from_slice(&jpeg);
    pdf.extend_from_slice(b"\nendstream\nendobj\n%%EOF");
    let imgs = extract_pdf_jpeg_streams(&pdf);
    assert_eq!(imgs.len(), 1, "应提取到 1 个 JPEG 流");
    assert_eq!(imgs[0], jpeg, "提取内容应与内嵌 JPEG 一致");
    // 无图片的 PDF 提取为空
    assert!(extract_pdf_jpeg_streams(b"%PDF-1.4\n%%EOF").is_empty());
}

#[test]
fn test_parse_pdf_no_text_returns_err() {
    // 无文本流（扫描件场景）应报错提示 OCR
    let pdf = b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\n%%EOF";
    let err = parse_pdf(pdf).unwrap_err();
    assert!(err.contains("OCR"), "错误信息应提示需 OCR，实际: {}", err);
}

#[test]
fn test_parse_docx_extracts_text() {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zw.start_file("word/document.xml", opts).unwrap();
        zw.write_all(b"<w:document><w:body><w:p><w:t>Hello Docx</w:t></w:p></w:body></w:document>")
            .unwrap();
        zw.finish().unwrap();
    }
    let doc = parse_document("docx", &buf).unwrap();
    assert!(doc.text.contains("Hello Docx"));
}

#[test]
fn test_parse_docx_chinese_text() {
    // 回归：<w:t> 内容为 UTF-8 时不能逐字节转 char
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zw.start_file("word/document.xml", opts).unwrap();
        zw.write_all(
                "<w:document><w:body><w:p><w:t xml:space=\"preserve\">中文段落内容</w:t></w:p></w:body></w:document>".as_bytes(),
            )
            .unwrap();
        zw.finish().unwrap();
    }
    let doc = parse_document("docx", &buf).unwrap();
    assert!(
        doc.text.contains("中文段落内容"),
        "docx 中文应完整保留，实际: {}",
        doc.text
    );
}

#[test]
fn test_parse_docx_missing_document_xml() {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zw.start_file("other.txt", opts).unwrap();
        zw.write_all(b"nope").unwrap();
        zw.finish().unwrap();
    }
    let err = parse_document("docx", &buf).unwrap_err();
    assert!(
        err.contains("未找到"),
        "应提示缺少 word/document.xml，实际: {}",
        err
    );
}

#[test]
fn test_parse_xlsx_extracts_text() {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zw.start_file("xl/sharedStrings.xml", opts).unwrap();
        zw.write_all(
            "<?xml version=\"1.0\"?><sst><si><t>姓名</t></si><si><t>张三</t></si></sst>".as_bytes(),
        )
        .unwrap();
        zw.start_file("xl/worksheets/sheet1.xml", opts).unwrap();
        zw.write_all(
                b"<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>1</v></c></row>\
                  <row r=\"2\"><c r=\"A2\"><v>2024</v></c><c r=\"B2\"><v>100</v></c></row></sheetData></worksheet>",
            )
            .unwrap();
        zw.finish().unwrap();
    }
    let doc = parse_document("xlsx", &buf).unwrap();
    assert!(
        doc.text.contains("姓名"),
        "共享字符串应被提取，实际: {}",
        doc.text
    );
    assert!(doc.text.contains("张三"));
    assert!(doc.text.contains("2024"), "数值单元格应被提取");
    assert!(doc.text.contains("100"));
}

#[test]
fn test_parse_xlsx_missing_sheet_errors() {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("other.txt", opts).unwrap();
        zw.write_all(b"nope").unwrap();
        zw.finish().unwrap();
    }
    let err = parse_document("xlsx", &buf).unwrap_err();
    assert!(err.contains("工作表"), "应提示缺少工作表，实际: {}", err);
}

// ---------- anydoc 多格式解析 ----------

/// 构造结构合法的极简 docx（含 [Content_Types].xml / _rels/.rels / word/document.xml）
fn make_valid_docx(text: &str) -> Vec<u8> {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("[Content_Types].xml", opts).unwrap();
        zw.write_all(
                br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
            )
            .unwrap();
        zw.start_file("_rels/.rels", opts).unwrap();
        zw.write_all(
                br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
            )
            .unwrap();
        zw.start_file("word/document.xml", opts).unwrap();
        let doc = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body><w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>
<w:p><w:r><w:t xml:space="preserve">第二段内容</w:t></w:r></w:p>
</w:body></w:document>"#,
            text
        );
        zw.write_all(doc.as_bytes()).unwrap();
        zw.finish().unwrap();
    }
    buf
}

#[test]
fn test_parse_with_anydoc_docx() {
    let doc = parse_with_anydoc("docx", &make_valid_docx("AnyDoc Word 测试")).unwrap();
    assert!(
        doc.text.contains("AnyDoc Word 测试"),
        "anydoc 应提取 docx 文本，实际: {}",
        doc.text
    );
    assert!(
        doc.text.contains("第二段内容"),
        "anydoc 应保留多段内容，实际: {}",
        doc.text
    );
}

#[test]
fn test_parse_with_anydoc_rtf() {
    let rtf = br#"{\rtf1\ansi\deff0 {\fonttbl {\f0 Arial;}}\f0\fs24 Hello RTF Content}"#;
    let doc = parse_with_anydoc("rtf", rtf).unwrap();
    assert!(
        doc.text.contains("Hello RTF Content"),
        "anydoc 应提取 RTF 文本，实际: {}",
        doc.text
    );
}

#[test]
fn test_parse_document_new_formats_routed() {
    // 合法 docx 走 anydoc 路径成功
    let doc = parse_document("docx", &make_valid_docx("路由测试")).unwrap();
    assert!(doc.text.contains("路由测试"));
    // RTF 走 anydoc 路径成功
    let rtf = parse_document("rtf", "{\\rtf1\\ansi RTF Route Test}".as_bytes()).unwrap();
    assert!(rtf.text.contains("RTF Route Test"), "实际: {}", rtf.text);
}

#[test]
fn test_parse_with_anydoc_rejects_minimal_zip_falls_back() {
    // 结构不完整的 zip（仅含 document.xml）应被 anydoc 拒绝，
    // 但 parse_document 会回退到既有简易 docx 解析器
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zw.start_file("word/document.xml", opts).unwrap();
        zw.write_all(
            b"<w:document><w:body><w:p><w:t>Fallback Docx</w:t></w:p></w:body></w:document>",
        )
        .unwrap();
        zw.finish().unwrap();
    }
    assert!(
        parse_with_anydoc("docx", &buf).is_err(),
        "缺少 OPC 结构的 zip 应被 anydoc 拒绝"
    );
    let doc = parse_document("docx", &buf).unwrap();
    assert!(
        doc.text.contains("Fallback Docx"),
        "应回退到简易解析器，实际: {}",
        doc.text
    );
}
