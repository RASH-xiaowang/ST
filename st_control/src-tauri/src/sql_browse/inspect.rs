// ============================================================
// SQLite 表浏览 — 表详情 / 完整性 / 列统计
// 自 sql_browse.rs 拆分：DDL/外键/详情/完整性/抽样统计。
// ============================================================

use rusqlite::{params, Connection};

use super::convert::blob_to_preview;
use super::query::table_schema;
use super::utils::{friendly_db_error, safe_name};

// ═══════════════════════════════════════════════════════════
// 增强能力：表详情 / 完整性 / 列统计
// ═══════════════════════════════════════════════════════════

/// 行 → JSON 对象（与 query_table 相同的值语义）
pub(crate) fn row_to_json(
    row: &rusqlite::Row,
    col_names: &[String],
) -> rusqlite::Result<serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (i, name) in col_names.iter().enumerate() {
        let val: serde_json::Value = match row.get_ref(i) {
            Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
            Ok(rusqlite::types::ValueRef::Integer(n)) => serde_json::Value::String(n.to_string()),
            Ok(rusqlite::types::ValueRef::Real(f)) => {
                serde_json::Value::String(if f.fract() == 0.0 && f.is_finite() {
                    format!("{}", f as i64)
                } else {
                    f.to_string()
                })
            }
            Ok(rusqlite::types::ValueRef::Text(s)) => {
                serde_json::Value::String(String::from_utf8_lossy(s).to_string())
            }
            Ok(rusqlite::types::ValueRef::Blob(b)) => serde_json::Value::String(blob_to_preview(b)),
            Err(_) => serde_json::Value::Null,
        };
        map.insert(name.clone(), val);
    }
    Ok(serde_json::Value::Object(map))
}

/// 获取表的建表 DDL（sqlite_master.sql；虚拟表可能无 DDL）
pub fn table_ddl(conn: &Connection, table: &str) -> Result<String, String> {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type IN ('table','view') AND name = ?1",
        params![table],
        |r| r.get::<_, Option<String>>(0),
    )
    .map_err(|e| format!("读取建表语句失败: {}", friendly_db_error(&e)))?
    .ok_or_else(|| format!("未找到表: {}", table))
}

/// 解析建表语句中的外键引用（简单文本扫描，微信库基本无 FK）
fn parse_fk_refs(ddl: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = ddl;
    while let Some(i) = rest.to_ascii_lowercase().find("references") {
        rest = &rest[i + "references".len()..];
        let t = rest.trim_start();
        let t = t.trim_start_matches(['"', '`', '[']);
        let name: String = t
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// 表详情：DDL + 索引（含列）+ 触发器 + 外键引用
pub fn table_detail(conn: &Connection, table: &str) -> Result<serde_json::Value, String> {
    let ddl = table_ddl(conn, table).unwrap_or_default();

    // 索引列表
    let mut indexes: Vec<serde_json::Value> = Vec::new();
    {
        let mut stmt = conn
            .prepare(&format!("PRAGMA index_list({})", safe_name(table)))
            .map_err(|e| format!("读取索引列表失败: {}", friendly_db_error(&e)))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "seq": r.get::<_, i64>(0)?,
                    "name": r.get::<_, String>(1)?,
                    "unique": r.get::<_, bool>(2)?,
                    "origin": r.get::<_, String>(3)?,
                    "partial": r.get::<_, bool>(4)?,
                }))
            })
            .map_err(|e| format!("读取索引列表失败: {}", friendly_db_error(&e)))?;
        for v in rows.flatten() {
            indexes.push(v);
        }
    }
    // 每个索引的列
    for idx in indexes.iter_mut() {
        let name = idx
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let cols: Vec<String> = conn
            .prepare(&format!("PRAGMA index_info({})", safe_name(&name)))
            .and_then(|mut stmt| {
                let rows = stmt.query_map([], |r| r.get::<_, Option<String>>(2))?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .collect();
        idx["columns"] = serde_json::json!(cols);
    }

    // 触发器
    let mut triggers: Vec<serde_json::Value> = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT name, sql FROM sqlite_master WHERE type='trigger' AND tbl_name = ?1 ORDER BY name")
            .map_err(|e| format!("读取触发器失败: {}", friendly_db_error(&e)))?;
        let rows = stmt
            .query_map(params![table], |r| {
                Ok(serde_json::json!({
                    "name": r.get::<_, String>(0)?,
                    "sql": r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                }))
            })
            .map_err(|e| format!("读取触发器失败: {}", friendly_db_error(&e)))?;
        for v in rows.flatten() {
            triggers.push(v);
        }
    }

    let fks = parse_fk_refs(&ddl);
    Ok(serde_json::json!({
        "table": table,
        "ddl": ddl,
        "indexes": indexes,
        "triggers": triggers,
        "foreign_keys": fks,
    }))
}

/// 数据库完整性检查：PRAGMA integrity_check + foreign_key_check
pub fn db_integrity(conn: &Connection) -> Result<serde_json::Value, String> {
    let integrity: Vec<String> = conn
        .prepare("PRAGMA integrity_check")
        .map_err(|e| format!("完整性检查失败: {}", friendly_db_error(&e)))?
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("完整性检查失败: {}", friendly_db_error(&e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("完整性检查失败: {}", friendly_db_error(&e)))?;

    let mut fk = Vec::new();
    if let Ok(mut stmt) = conn.prepare("PRAGMA foreign_key_check") {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok(serde_json::json!({
                "table": r.get::<_, String>(0)?,
                "rowid": r.get::<_, i64>(1)?,
                "parent": r.get::<_, String>(2)?,
                "fkid": r.get::<_, i64>(3)?,
            }))
        }) {
            for v in rows.flatten() {
                fk.push(v);
            }
        }
    }
    Ok(serde_json::json!({ "integrity": integrity, "foreign_keys": fk }))
}

/// 列统计（抽样）：null 比例、数值 min/max/sum、文本 TOP 值
pub fn table_stats(
    conn: &Connection,
    table: &str,
    sample: usize,
) -> Result<serde_json::Value, String> {
    let cols = table_schema(conn, table)?;
    let safe_table = safe_name(table);
    let sample = sample.clamp(1, 20000);

    struct ColStat {
        name: String,
        col_type: String,
        sample: usize,
        non_null: usize,
        null_count: usize,
        is_numeric: bool,
        min: Option<f64>,
        max: Option<f64>,
        sum: f64,
        tops: std::collections::HashMap<String, usize>,
    }
    let mut stats: Vec<ColStat> = cols
        .iter()
        .map(|c| ColStat {
            name: c.name.clone(),
            col_type: c.col_type.clone(),
            sample: 0,
            non_null: 0,
            null_count: 0,
            is_numeric: false,
            min: None,
            max: None,
            sum: 0.0,
            tops: std::collections::HashMap::new(),
        })
        .collect();

    let sql = format!("SELECT * FROM {} ORDER BY rowid LIMIT ?1", safe_table);
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("查询数据失败: {}", friendly_db_error(&e)))?;
    let col_names: Vec<String> = (0..stmt.column_count())
        .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
        .collect();
    let mut q = stmt
        .query([sample as i64])
        .map_err(|e| format!("查询数据失败: {}", friendly_db_error(&e)))?;
    while let Some(row) = q
        .next()
        .map_err(|e| format!("读取数据失败: {}", friendly_db_error(&e)))?
    {
        for (i, name) in col_names.iter().enumerate() {
            let Some(ci) = cols.iter().position(|c| c.name == *name) else {
                continue;
            };
            let st = &mut stats[ci];
            st.sample += 1;
            match row.get_ref(i) {
                Ok(rusqlite::types::ValueRef::Null) => st.null_count += 1,
                Ok(rusqlite::types::ValueRef::Integer(n)) => {
                    st.non_null += 1;
                    st.is_numeric = true;
                    let v = n as f64;
                    st.min = Some(st.min.map_or(v, |m: f64| m.min(v)));
                    st.max = Some(st.max.map_or(v, |m: f64| m.max(v)));
                    st.sum += v;
                    *st.tops.entry(n.to_string()).or_insert(0) += 1;
                }
                Ok(rusqlite::types::ValueRef::Real(f)) => {
                    st.non_null += 1;
                    st.is_numeric = true;
                    st.min = Some(st.min.map_or(f, |m: f64| m.min(f)));
                    st.max = Some(st.max.map_or(f, |m: f64| m.max(f)));
                    st.sum += f;
                    *st.tops.entry(f.to_string()).or_insert(0) += 1;
                }
                Ok(rusqlite::types::ValueRef::Text(s)) => {
                    st.non_null += 1;
                    let t = String::from_utf8_lossy(s);
                    if t.len() <= 64 {
                        *st.tops.entry(t.to_string()).or_insert(0) += 1;
                    }
                }
                Ok(rusqlite::types::ValueRef::Blob(_)) => st.non_null += 1,
                Err(_) => {}
            }
        }
    }

    let mut result = Vec::new();
    for st in stats.iter_mut() {
        let mut top: Vec<serde_json::Value> = st
            .tops
            .iter()
            .map(|(k, v)| serde_json::json!({ "value": k, "count": v }))
            .collect();
        top.sort_by(|a, b| b["count"].as_i64().cmp(&a["count"].as_i64()));
        top.truncate(5);
        result.push(serde_json::json!({
            "name": st.name,
            "type": st.col_type,
            "sample": st.sample,
            "non_null": st.non_null,
            "null_count": st.null_count,
            "null_pct": if st.sample > 0 { (st.null_count as f64 / st.sample as f64 * 100.0).round() } else { 0.0 },
            "is_numeric": st.is_numeric,
            "min": st.min,
            "max": st.max,
            "sum": if st.is_numeric { Some(st.sum) } else { None },
            "top": top,
        }));
    }
    Ok(serde_json::json!({ "table": table, "sample": sample, "columns": result }))
}
