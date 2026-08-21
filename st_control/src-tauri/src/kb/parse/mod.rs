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

/// 解析文档二进制为纯文本
pub fn parse_document(file_type: &str, data: &[u8]) -> Result<ParsedDoc, String> {
    let ft = file_type.to_lowercase();
    match ft.as_str() {
        "txt" | "md" | "markdown" | "csv" | "json" | "log" => {
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
            // 图片：走系统 OCR 识别文字（Windows 目标）
            #[cfg(target_os = "windows")]
            {
                let text = crate::kb::ocr::ocr_image(data)?;
                if text.trim().is_empty() {
                    return Err(
                        "图片 OCR 未识别出文字（请确认图片清晰且系统已安装 OCR 语言包）"
                            .to_string(),
                    );
                }
                Ok(split_into_sections(&text))
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = data;
                Err("图片解析（OCR）目前仅支持 Windows".to_string())
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
/// 返回写入的分片 id 列表
pub fn save_chunks(
    db: &KbDatabase,
    kb_id: i64,
    doc_id: i64,
    version_id: i64,
    chunks: &[Chunk],
) -> Result<Vec<i64>, String> {
    let conn = db.conn_lock();
    let mut ids = Vec::with_capacity(chunks.len());
    // 第一遍：插入父块，记录 seq -> id 映射（供子块关联）
    let mut parent_ids: std::collections::HashMap<usize, i64> = std::collections::HashMap::new();
    for c in chunks {
        if c.parent_id.is_some() {
            continue;
        }
        conn.execute(
            "INSERT INTO document_chunks (kb_id, doc_id, version_id, seq, content, section, page_no, char_start, char_end, token_count)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![kb_id, doc_id, version_id, c.seq as i64, c.content, c.section, c.page_no, c.char_start as i64, c.char_end as i64, c.token_count as i64],
        ).map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid();
        parent_ids.insert(c.seq, id);
        // 同步 FTS 索引（写入侧做汉字间隔预处理，保证中文子串可检索）
        conn.execute(
            "INSERT INTO chunks_fts (rowid, content) VALUES (?1, ?2)",
            params![id, crate::kb::cjk_spaced(&c.content)],
        )
        .map_err(|e| e.to_string())?;
        ids.push(id);
    }
    // 第二遍：插入子块（parent_id 关联父块真实 id）
    for c in chunks {
        let Some(ps) = c.parent_id else { continue };
        let pid = parent_ids.get(&(ps as usize)).copied();
        conn.execute(
            "INSERT INTO document_chunks (kb_id, doc_id, version_id, seq, content, section, page_no, char_start, char_end, token_count, parent_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![kb_id, doc_id, version_id, c.seq as i64, c.content, c.section, c.page_no, c.char_start as i64, c.char_end as i64, c.token_count as i64, pid],
        ).map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO chunks_fts (rowid, content) VALUES (?1, ?2)",
            params![id, crate::kb::cjk_spaced(&c.content)],
        )
        .map_err(|e| e.to_string())?;
        ids.push(id);
    }
    // 按 seq 恢复原始顺序（父块与子块交错顺序可能与传入顺序不同）
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
#[cfg(test)]
mod tests;
