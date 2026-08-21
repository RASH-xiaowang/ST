// ============================================================
// SQLite 表浏览 — SQL 执行
// 自 sql_browse.rs 拆分：读写判断与安全执行。
// ============================================================

use rusqlite::Connection;

use super::friendly_db_error;
use super::inspect::row_to_json;

fn first_keyword(sql: &str) -> String {
    let mut s = sql.trim_start();
    while s.starts_with("--") {
        if let Some(nl) = s.find('\n') {
            s = s[nl + 1..].trim_start();
        } else {
            break;
        }
    }
    s.chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase()
}

/// 执行 SQL。
/// - readonly=true（外部库）仅允许 SELECT / WITH / PRAGMA / EXPLAIN / VALUES
/// - 查询返回 columns+rows（最多 limit 行）；写语句返回 affected
pub fn execute_sql(
    conn: &Connection,
    sql: &str,
    limit: usize,
    readonly: bool,
) -> Result<serde_json::Value, String> {
    let sql = sql.trim();
    if sql.is_empty() {
        return Err("SQL 为空".to_string());
    }
    let kw = first_keyword(sql);
    let is_query = matches!(
        kw.as_str(),
        "select" | "with" | "pragma" | "explain" | "values"
    );
    if readonly && !is_query {
        return Err(format!(
            "外部数据库只读，仅支持 SELECT / WITH / PRAGMA / EXPLAIN / VALUES（收到: {}）",
            kw.to_uppercase()
        ));
    }
    if is_query {
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| format!("SQL 解析失败: {}", friendly_db_error(&e)))?;
        let col_names: Vec<String> = (0..stmt.column_count())
            .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
            .collect();
        let mut rows = Vec::new();
        {
            let mut q = stmt
                .query([])
                .map_err(|e| format!("SQL 执行失败: {}", friendly_db_error(&e)))?;
            let mut n = 0usize;
            while let Some(row) = q
                .next()
                .map_err(|e| format!("读取结果失败: {}", friendly_db_error(&e)))?
            {
                let v = row_to_json(row, &col_names)
                    .map_err(|e| format!("读取结果失败: {}", friendly_db_error(&e)))?;
                rows.push(v);
                n += 1;
                if n >= limit {
                    break;
                }
            }
        }
        return Ok(serde_json::json!({
            "kind": "query",
            "columns": col_names,
            "rows": rows,
            "truncated": rows.len() >= limit,
        }));
    }
    let affected = conn
        .execute(sql, [])
        .map_err(|e| format!("SQL 执行失败: {}", friendly_db_error(&e)))?;
    Ok(serde_json::json!({ "kind": "write", "affected": affected }))
}
