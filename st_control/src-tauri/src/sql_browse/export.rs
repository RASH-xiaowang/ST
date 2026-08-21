// ============================================================
// SQLite 表浏览 — CSV 导出
// 自 sql_browse.rs 拆分：整表分块流式导出（BOM + 转义）。
// ============================================================

use rusqlite::Connection;

use super::{friendly_db_error, safe_name, table_schema};

/// CSV 单元格转义（引号包裹 + 双引号转义 + 换行转空格）
fn csv_escape(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\"").replace(['\r', '\n'], " "))
}

/// 整表导出为 CSV（分块流式写入，避免大表占用内存/IPC 消息）
pub fn export_table_to_csv(
    conn: &Connection,
    table: &str,
    filepath: &std::path::Path,
) -> Result<usize, String> {
    use std::io::Write;
    let cols = table_schema(conn, table)?;
    if cols.is_empty() {
        return Err(format!("表 {} 无列", table));
    }
    if let Some(parent) = filepath.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let mut file = std::fs::File::create(filepath).map_err(|e| format!("创建文件失败: {}", e))?;
    file.write_all(b"\xEF\xBB\xBF")
        .map_err(|e| format!("写入文件失败: {}", e))?;
    let header = cols
        .iter()
        .map(|c| csv_escape(&c.name))
        .collect::<Vec<_>>()
        .join(",");
    file.write_all(header.as_bytes())
        .map_err(|e| format!("写入文件失败: {}", e))?;
    file.write_all(b"\r\n")
        .map_err(|e| format!("写入文件失败: {}", e))?;

    let safe_table = safe_name(table);
    let batch = 2000usize;
    let mut offset = 0usize;
    let mut total = 0usize;
    loop {
        let sql = format!("SELECT * FROM {} LIMIT ?1 OFFSET ?2", safe_table);
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("查询数据失败: {}", friendly_db_error(&e)))?;
        let mut q = stmt
            .query(rusqlite::params![batch as i64, offset as i64])
            .map_err(|e| format!("查询数据失败: {}", friendly_db_error(&e)))?;
        let mut got = 0usize;
        while let Some(row) = q
            .next()
            .map_err(|e| format!("读取数据失败: {}", friendly_db_error(&e)))?
        {
            got += 1;
            total += 1;
            let cells: Vec<String> = cols
                .iter()
                .enumerate()
                .map(|(i, _)| match row.get_ref(i) {
                    Ok(rusqlite::types::ValueRef::Null) => "".to_string(),
                    Ok(rusqlite::types::ValueRef::Integer(n)) => n.to_string(),
                    Ok(rusqlite::types::ValueRef::Real(f)) => {
                        if f.fract() == 0.0 && f.is_finite() {
                            format!("{}", f as i64)
                        } else {
                            f.to_string()
                        }
                    }
                    Ok(rusqlite::types::ValueRef::Text(s)) => {
                        csv_escape(&String::from_utf8_lossy(s))
                    }
                    Ok(rusqlite::types::ValueRef::Blob(b)) => {
                        csv_escape(&format!("[BLOB {}B]", b.len()))
                    }
                    Err(_) => "".to_string(),
                })
                .collect();
            file.write_all(cells.join(",").as_bytes())
                .map_err(|e| format!("写入文件失败: {}", e))?;
            file.write_all(b"\r\n")
                .map_err(|e| format!("写入文件失败: {}", e))?;
        }
        if got < batch {
            break;
        }
        offset += batch;
    }
    Ok(total)
}
