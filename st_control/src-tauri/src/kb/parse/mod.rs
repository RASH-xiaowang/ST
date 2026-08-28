// ============================================================
// 文档解析与分片 (Parsing & Chunking)
// 支持：txt/md/csv/json/log（原生）
//       doc/docx/docm/ppt/pps/pot/pptx/pptm/ppsx/ppsm/xls/xlsx/xlsm/xlsb/
//       odt/ods/odp/rtf/epub/pdf（anydoc 引擎 → GFM Markdown，MIT）
//       png/jpg/jpeg/gif/webp/bmp（Windows OCR）
// 分片策略：recursive（递归字符分片，按段落/句子边界，带重叠窗口）
//          title（标题感知：按 Markdown 标题层级切分，分片保留章节上下文）
//          parent_child（父子分块：父块粗粒度用于回答上下文，子块细粒度用于检索命中）
// ============================================================

use crate::kb::db::KbDatabase;

mod docx;
use docx::parse_docx;

mod pdf;
use pdf::parse_pdf;

mod xlsx;
use rusqlite::params;

mod chunk;
pub use chunk::{chunk_text, Chunk};

mod anydoc;
use anydoc::parse_with_anydoc;
use xlsx::parse_xlsx;

/// 文档解析结果（纯文本 + 逻辑分段元信息）
#[derive(Debug, Clone)]
pub struct ParsedDoc {
    pub text: String,
    /// 逻辑分段（按空行/标题切分），用于分片边界参考
    pub sections: Vec<SectionSpan>,
}

#[derive(Debug, Clone)]
pub struct SectionSpan {
    pub title: Option<String>,
    pub char_start: usize,
    pub char_end: usize,
}

/// 分块策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ChunkStrategy {
    /// 递归字符分片（默认，按段落/句子边界 + 重叠窗口）
    #[default]
    Recursive,
    /// 标题感知：按 Markdown 标题层级切分，分片保留章节标题作为上下文
    Title,
    /// 父子分块：父块粗粒度（chunk_size×3）用于回答上下文，子块细粒度（chunk_size/4）用于检索命中
    ParentChild,
}

impl std::str::FromStr for ChunkStrategy {
    type Err = String;

    /// 解析分块策略；未知值回退 Recursive（与历史 from_str 语义一致）
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "title" => ChunkStrategy::Title,
            "parent_child" | "parent-child" | "parentchild" => ChunkStrategy::ParentChild,
            _ => ChunkStrategy::Recursive,
        })
    }
}

/// 分片配置
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    pub chunk_size: usize, // 目标字符数
    pub overlap: usize,    // 重叠字符数
    pub min_chunk: usize,  // 最小分片长度
    pub strategy: ChunkStrategy,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        ChunkConfig {
            chunk_size: 800,
            overlap: 120,
            min_chunk: 100,
            strategy: ChunkStrategy::Recursive,
        }
    }
}

/// 二进制格式魔数嗅探：声明类型与实际内容不符时尽早拒绝，返回清晰错误。
/// 文本类格式不做魔数校验（按 UTF-8 宽松解析）。
pub(crate) fn sniff_format_magic(file_type: &str, data: &[u8]) -> Result<(), String> {
    let ft = file_type.to_lowercase();
    let starts_with = |needle: &[u8]| data.starts_with(needle);
    let is_zip = starts_with(b"PK\x03\x04") || starts_with(b"PK\x05\x06");
    let ok = match ft.as_str() {
        // ZIP 容器（Office 2007+ / ODF / EPUB）
        "docx" | "xlsx" | "pptx" | "docm" | "pptm" | "ppsx" | "xlsm" | "xlsb" | "odt" | "ods"
        | "odp" | "epub" => is_zip,
        "pdf" => starts_with(b"%PDF-"),
        "png" => starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => starts_with(b"\xFF\xD8\xFF"),
        "gif" => starts_with(b"GIF8"),
        "webp" => starts_with(b"RIFF") && data.len() >= 12 && data[8..12] == *b"WEBP",
        "bmp" => starts_with(b"BM"),
        // 文本类/其它：不做魔数校验
        _ => return Ok(()),
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "文件类型「{}」与实际内容不符（魔数不匹配），已拒绝",
            file_type
        ))
    }
}

/// 单 zip 条目解压后的大小上限（防御 zip-bomb：小压缩包超高倍展开）
pub(crate) const MAX_ZIP_ENTRY_BYTES: usize = 64 * 1024 * 1024; // 64 MB
/// zip 条目数量上限
pub(crate) const MAX_ZIP_ENTRIES: usize = 4096;
/// 解压后总大小上限（防御海量条目合计超大）
pub(crate) const MAX_ZIP_TOTAL_BYTES: usize = 256 * 1024 * 1024; // 256 MB
/// 文本类文件解析大小上限（防御超大文件全量转 String 导致内存溢出）
pub(crate) const MAX_TEXT_PARSE_BYTES: usize = 64 * 1024 * 1024; // 64 MB

/// 读取 zip 条目为文本，带解压大小上限；超限视为压缩炸弹拒绝。
pub(crate) fn read_zip_entry_text<R: std::io::Read>(
    mut r: R,
    name: &str,
) -> Result<String, String> {
    use std::io::Read as _;
    let mut buf = String::new();
    let n = r
        .by_ref()
        .take((MAX_ZIP_ENTRY_BYTES + 1) as u64)
        .read_to_string(&mut buf)
        .map_err(|e| format!("读取 {} 失败: {}", name, e))?;
    if n > MAX_ZIP_ENTRY_BYTES || buf.len() > MAX_ZIP_ENTRY_BYTES {
        return Err(format!(
            "{} 解压后过大（超过 {} MB），可能为压缩炸弹，已拒绝",
            name,
            MAX_ZIP_ENTRY_BYTES / 1024 / 1024
        ));
    }
    Ok(buf)
}

/// 校验 zip 条目数量上限（防御海量小条目炸弹）。
pub(crate) fn check_zip_entry_count(
    zip: &zip::ZipArchive<std::io::Cursor<&[u8]>>,
) -> Result<(), String> {
    if zip.len() > MAX_ZIP_ENTRIES {
        return Err(format!(
            "压缩包条目过多（{} 个，上限 {}），已拒绝",
            zip.len(),
            MAX_ZIP_ENTRIES
        ));
    }
    Ok(())
}

/// 在解压之前读取 zip 中央目录的元数据（不解压内容），
/// 预检单条目解压大小与总大小上限。任何解析器（含 anydoc）之前调用，
/// 使 zip-bomb 在发生大内存展开前就被拒绝。
pub(crate) fn guard_zip_bomb(data: &[u8]) -> Result<(), String> {
    if !(data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06")) {
        return Ok(()); // 非 zip 容器，交给其它解析路径
    }
    let cursor = std::io::Cursor::new(data);
    let mut zip = zip::ZipArchive::new(cursor).map_err(|e| format!("zip 解析失败: {}", e))?;
    check_zip_entry_count(&zip)?;
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        let f = zip
            .by_index(i)
            .map_err(|e| format!("读取 zip 条目失败: {}", e))?;
        let size = f.size();
        if size > MAX_ZIP_ENTRY_BYTES as u64 {
            return Err(format!(
                "压缩包条目过大（解压约 {} MB，上限 {} MB），可能为压缩炸弹，已拒绝",
                size / 1024 / 1024,
                MAX_ZIP_ENTRY_BYTES / 1024 / 1024
            ));
        }
        total += size;
        if total > MAX_ZIP_TOTAL_BYTES as u64 {
            return Err(format!(
                "压缩包解压总量过大（{} MB，上限 {} MB），已拒绝",
                total / 1024 / 1024,
                MAX_ZIP_TOTAL_BYTES / 1024 / 1024
            ));
        }
    }
    Ok(())
}

/// 解析文档二进制为纯文本
pub fn parse_document(file_type: &str, data: &[u8]) -> Result<ParsedDoc, String> {
    let ft = file_type.to_lowercase();
    // 安全：二进制格式先做魔数嗅探，声明类型与内容不符时尽早拒绝
    sniff_format_magic(&ft, data)?;
    // 安全：zip 容器在 anydoc/解压之前按元数据预检，拦截压缩炸弹
    guard_zip_bomb(data)?;
    match ft.as_str() {
        "txt" | "md" | "markdown" | "csv" | "json" | "log" | "py" | "js" | "ts" | "rs" | "go"
        | "java" | "c" | "cpp" | "h" | "hpp" | "rb" | "sh" | "sql" | "xml" | "yaml" | "yml"
        | "toml" | "ini" | "cfg" | "env" | "dockerfile" | "makefile" | "html" | "css" | "scss"
        | "less" => {
            if data.len() > MAX_TEXT_PARSE_BYTES {
                return Err(format!(
                    "文本类文件过大（{}MB，上限 {}MB），请拆分后上传",
                    data.len() / 1024 / 1024,
                    MAX_TEXT_PARSE_BYTES / 1024 / 1024
                ));
            }
            let text = String::from_utf8_lossy(data).to_string();
            Ok(split_into_sections(&text))
        }
        // anydoc 优先，失败时回退到既有简易解析器（容错结构不完整的文件）
        "docx" => parse_with_anydoc(&ft, data).or_else(|_| parse_docx(data)),
        "xlsx" => parse_with_anydoc(&ft, data).or_else(|_| parse_xlsx(data)),
        // PDF：anydoc（pdf-inspector）质量更高；扫描件/异常文件回退到简易提取 + OCR
        "pdf" => parse_with_anydoc(&ft, data).or_else(|_| parse_pdf(data)),
        // anydoc 原生覆盖：旧版 Office / ODF / RTF / EPUB
        "doc" | "docm" | "ppt" | "pps" | "pot" | "pptx" | "pptm" | "ppsx" | "ppsm" | "xls"
        | "xlsm" | "xlsb" | "odt" | "ods" | "odp" | "rtf" | "epub" => parse_with_anydoc(&ft, data),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => {
            // 图片：尝试 OCR 识别文字，失败则存储空内容（图片仍可查看）
            #[cfg(target_os = "windows")]
            {
                match crate::kb::ocr::ocr_image(data) {
                    Ok(text) if !text.trim().is_empty() => Ok(split_into_sections(&text)),
                    _ => {
                        // OCR 失败或未识别出文字：允许上传，内容为空
                        // 图片仍可在文档详情中查看，用户可手动编辑补充描述
                        Ok(ParsedDoc {
                            text: String::new(),
                            sections: vec![],
                        })
                    }
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                // 非 Windows：允许上传，内容为空
                Ok(ParsedDoc {
                    text: String::new(),
                    sections: vec![],
                })
            }
        }
        _ => Err(format!("暂不支持的文件类型: {}", file_type)),
    }
}
/// 将文本按 Markdown 标题切分为逻辑分段（保留字符偏移）。
/// 语义：标题行收尾上一段（title 为上一标题），标题行之后的内容归属该标题；
/// 首行即标题时会在开头保留一个空的边界段（与既有测试契约一致）。
pub(crate) fn split_into_sections(text: &str) -> ParsedDoc {
    let mut sections = Vec::new();
    let bytes = text.as_bytes();
    let mut seg_start = 0usize; // 当前段起始（字节偏移）
    let mut current_title: Option<String> = None;
    let mut i = 0usize;
    loop {
        // 找行结束（\n 或 EOF）
        let mut j = i;
        while j < bytes.len() && bytes[j] != b'\n' {
            j += 1;
        }
        let line = &text[i..j];
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            // 标题行：收尾当前段（标题行之前的部分），即使为空也记录边界
            sections.push(SectionSpan {
                title: current_title.take(),
                char_start: text[..seg_start].chars().count(),
                char_end: text[..i].chars().count(),
            });
            current_title = Some(trimmed.trim_start_matches('#').trim().to_string());
            seg_start = j + 1; // 标题行之后（含换行）
        }
        if j >= bytes.len() {
            break; // EOF（最后一行后无换行，不再扫描）
        }
        i = j + 1;
    }
    // 收尾最后一段
    let tail = &text[seg_start.min(bytes.len())..];
    if !tail.trim().is_empty() {
        sections.push(SectionSpan {
            title: current_title,
            char_start: text[..seg_start].chars().count(),
            char_end: text.chars().count(),
        });
    }
    if sections.is_empty() {
        sections.push(SectionSpan {
            title: None,
            char_start: 0,
            char_end: text.chars().count(),
        });
    }
    ParsedDoc {
        text: text.to_string(),
        sections,
    }
}

/// 将解析+分片结果写入 document_chunks（embedding_blob 暂为空，由 embed 阶段填充）
/// 使用事务包裹全部写入，显著提升批量插入性能（500 片：~500ms → ~20ms）。
/// 返回写入的分片 id 列表
pub fn save_chunks(
    db: &KbDatabase,
    kb_id: i64,
    doc_id: i64,
    version_id: i64,
    chunks: &[Chunk],
) -> Result<Vec<i64>, String> {
    let conn = db.conn_lock();
    // 事务包裹全部写入，减少磁盘 I/O
    conn.execute_batch("BEGIN TRANSACTION")
        .map_err(|e| e.to_string())?;

    let mut ids = Vec::with_capacity(chunks.len());
    let mut parent_ids: std::collections::HashMap<usize, i64> = std::collections::HashMap::new();

    // 预编译语句（避免逐条解析 SQL）
    let mut stmt_chunk = conn
        .prepare(
            "INSERT INTO document_chunks (kb_id, doc_id, version_id, seq, content, section, page_no, char_start, char_end, token_count, parent_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        )
        .map_err(|e| e.to_string())?;
    let mut stmt_fts = conn
        .prepare("INSERT INTO chunks_fts (rowid, content) VALUES (?1, ?2)")
        .map_err(|e| e.to_string())?;

    // 第一遍：插入父块
    for c in chunks {
        if c.parent_id.is_some() {
            continue;
        }
        stmt_chunk
            .execute(params![
                kb_id,
                doc_id,
                version_id,
                c.seq as i64,
                c.content,
                c.section,
                c.page_no,
                c.char_start as i64,
                c.char_end as i64,
                c.token_count as i64,
                None::<i64>
            ])
            .map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid();
        parent_ids.insert(c.seq, id);
        stmt_fts
            .execute(params![id, crate::kb::cjk_spaced(&c.content)])
            .map_err(|e| e.to_string())?;
        ids.push(id);
    }

    // 第二遍：插入子块
    for c in chunks {
        let Some(ps) = c.parent_id else { continue };
        let pid = parent_ids.get(&(ps as usize)).copied();
        stmt_chunk
            .execute(params![
                kb_id,
                doc_id,
                version_id,
                c.seq as i64,
                c.content,
                c.section,
                c.page_no,
                c.char_start as i64,
                c.char_end as i64,
                c.token_count as i64,
                pid
            ])
            .map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid();
        stmt_fts
            .execute(params![id, crate::kb::cjk_spaced(&c.content)])
            .map_err(|e| e.to_string())?;
        ids.push(id);
    }

    // 释放预编译语句，避免 WAL 锁冲突
    drop(stmt_chunk);
    drop(stmt_fts);

    // 提交事务
    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;

    // 按 seq 恢复原始顺序
    ids.sort_by_key(|id| {
        let seq: i64 = conn
            .query_row(
                "SELECT seq FROM document_chunks WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        seq
    });
    Ok(ids)
}

#[cfg(test)]
mod tests;
