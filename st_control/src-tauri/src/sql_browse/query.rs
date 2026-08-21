// ============================================================
// SQLite 表浏览 — 查询
// 自 sql_browse.rs 拆分：表列表、结构、分页查询（keyset）。
// ============================================================

use rusqlite::Connection;

use super::convert::blob_to_preview;
use super::types::{ColumnInfo, TableData, TableQueryParams};
use super::utils::{escape_like, friendly_db_error, safe_name};

/// 列出数据库中所有表（含下划线开头 / 系统表，保证全部可见）
pub fn list_tables(conn: &Connection) -> Result<Vec<String>, String> {
    // 先用 count 确认 sqlite_master 是否可访问
    let total_tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("访问 sqlite_master 失败: {}", friendly_db_error(&e)))?;

    // 尝试查询 sqlite_schema (SQLite 3.33+ 新名称)
    let total_tables_alt: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(-1);

    let use_schema = if total_tables > 0 {
        "sqlite_master"
    } else if total_tables_alt > 0 {
        "sqlite_schema"
    } else {
        return Err(format!(
            "数据库异常: sqlite_master={} sqlite_schema={}，可能不是标准SQLite数据库",
            total_tables, total_tables_alt
        ));
    };

    let mut stmt = conn
        .prepare(&format!(
            "SELECT name FROM {} WHERE type='table' ORDER BY name",
            use_schema
        ))
        .map_err(|e| format!("查询表列表失败: {}", e))?;

    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("读取表列表失败: {}", friendly_db_error(&e)))?;

    let mut tables = Vec::new();
    for r in rows {
        tables.push(r.map_err(|e| format!("读取表名失败: {}", e))?);
    }
    Ok(tables)
}

/// 获取表的列信息
pub fn table_schema(conn: &Connection, table: &str) -> Result<Vec<ColumnInfo>, String> {
    let safe_table = safe_name(table);
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", safe_table))
        .map_err(|e| format!("获取表结构失败: {}", friendly_db_error(&e)))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(ColumnInfo {
                cid: row.get::<_, i64>(0)? as usize,
                name: row.get(1)?,
                col_type: row.get::<_, String>(2)?,
                not_null: row.get::<_, bool>(3)?,
                default: row.get::<_, Option<String>>(4)?,
                pk: row.get::<_, bool>(5)?,
            })
        })
        .map_err(|e| format!("读取表结构失败: {}", friendly_db_error(&e)))?;

    let mut cols = Vec::new();
    for r in rows {
        cols.push(r.map_err(|e| format!("读取列信息失败: {}", e))?);
    }
    Ok(cols)
}

/// 分页查询表数据（过滤/排序/keyset 分页；recount=false 时跳过 COUNT 以提升大表翻页性能）
pub fn query_table(conn: &Connection, params: &TableQueryParams) -> Result<TableData, String> {
    let table = &params.table;
    let page = params.page;
    let page_size = params.page_size;
    let order_col = &params.order_col;
    let order_dir = &params.order_dir;
    let filter = &params.filter;
    let recount = params.recount;
    let cursor = params.cursor.clone();
    let direction = params.direction.clone();
    let safe_table = safe_name(table);
    let dir_asc = order_dir != "desc";
    let dir_sql = if dir_asc { "ASC" } else { "DESC" };

    // 列信息（用于校验排序列、构造过滤条件，并作为返回的 columns）
    let cols = {
        let mut stmt0 = conn
            .prepare(&format!("PRAGMA table_info({})", safe_table))
            .map_err(|e| format!("获取列信息失败: {}", e))?;
        let col_rows = stmt0
            .query_map([], |row| {
                Ok(ColumnInfo {
                    cid: row.get::<_, i64>(0)? as usize,
                    name: row.get(1)?,
                    col_type: row.get::<_, String>(2)?,
                    not_null: row.get::<_, bool>(3)?,
                    default: row.get::<_, Option<String>>(4)?,
                    pk: row.get::<_, bool>(5)?,
                })
            })
            .map_err(|e| format!("读取列信息失败: {}", e))?;
        let mut list = Vec::new();
        for r in col_rows {
            list.push(r.map_err(|e| format!("读取列信息失败: {}", e))?);
        }
        list
    };

    // 检测是否支持 rowid（FTS / 虚拟表 / WITHOUT ROWID 可能不支持）
    let has_rid = conn
        .query_row(
            &format!("SELECT rowid FROM {} LIMIT 1", safe_table),
            [],
            |_| Ok(()),
        )
        .is_ok();

    // 解析排序列：显式列必须真实存在，否则回退到 rowid / 第一列
    let order_key: String =
        if !order_col.is_empty() && cols.iter().any(|c| c.name == order_col.as_str()) {
            order_col.to_string()
        } else if has_rid {
            "rowid".to_string()
        } else {
            cols.first().map(|c| c.name.clone()).unwrap_or_default()
        };
    let order_field = if order_key == "rowid" {
        "rowid".to_string()
    } else {
        safe_name(&order_key)
    };

    // 过滤条件（LIKE + ESCAPE，参数化）
    let mut filter_like: Option<String> = None;
    let f = filter.trim();
    if !f.is_empty() {
        filter_like = Some(format!("%{}%", escape_like(f)));
    }

    // keyset 游标解析（仅当有 rowid 且排序键不是 BLOB 时启用）
    let col_is_blob = cols
        .iter()
        .find(|c| c.name == order_key)
        .map(|c| c.col_type.to_uppercase().contains("BLOB"))
        .unwrap_or(false);
    let mut use_cursor = false;
    let mut cursor_value: Option<String> = None;
    let mut cursor_rid: i64 = 0;
    if has_rid && !order_field.is_empty() && !col_is_blob {
        if let Some(cj) = cursor.as_deref() {
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(cj) {
                if arr.len() == 2 {
                    cursor_value = arr[0]
                        .as_str()
                        .map(|s| s.to_string())
                        .or_else(|| arr[0].as_i64().map(|i| i.to_string()))
                        .or_else(|| arr[0].as_f64().map(|f| f.to_string()));
                    cursor_rid = arr[1].as_i64().unwrap_or(0);
                    use_cursor = cursor_value.is_some();
                }
            }
        }
    }
    let go_prev = use_cursor && direction == "prev";

    // 是否存在可 LIKE 的列（BLOB 列跳过：UI 中只显示 hex 预览，扫描无意义）
    let has_filterable_col = cols
        .iter()
        .any(|c| !c.col_type.to_uppercase().contains("BLOB"));
    // 参数序号：filter [cursor value, rowid] limit offset
    let filter_present = filter_like.is_some() && has_filterable_col;
    let mut idx = 1usize;
    let filter_idx: Option<usize> = if filter_present {
        let i = idx;
        idx += 1;
        Some(i)
    } else {
        None
    };
    let cv_idx: Option<usize> = if use_cursor {
        let i = idx;
        idx += 1;
        Some(i)
    } else {
        None
    };
    let cr_idx: Option<usize> = if use_cursor && order_key != "rowid" {
        let i = idx;
        idx += 1;
        Some(i)
    } else {
        None
    };
    let limit_idx = idx;
    idx += 1;
    let offset_idx = idx;

    // 组装 WHERE
    let mut where_parts: Vec<String> = Vec::new();
    let mut filter_conds: Vec<String> = Vec::new();
    if filter_present {
        filter_conds = cols
            .iter()
            .filter(|c| !c.col_type.to_uppercase().contains("BLOB"))
            .map(|c| {
                format!(
                    "{} LIKE ?{} ESCAPE '\\'",
                    safe_name(&c.name),
                    filter_idx.unwrap()
                )
            })
            .collect();
        where_parts.push(format!("({})", filter_conds.join(" OR ")));
    }
    if use_cursor {
        let op = if go_prev {
            if dir_asc {
                "<"
            } else {
                ">"
            }
        } else {
            if dir_asc {
                ">"
            } else {
                "<"
            }
        };
        let cond = if order_key == "rowid" {
            format!("{} {} ?{}", order_field, op, cv_idx.unwrap())
        } else {
            format!(
                "({} {} ?{} OR ({} = ?{} AND rowid {} ?{}))",
                order_field,
                op,
                cv_idx.unwrap(),
                order_field,
                cv_idx.unwrap(),
                op,
                cr_idx.unwrap()
            )
        };
        where_parts.push(cond);
    }
    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_parts.join(" AND "))
    };

    // 排序（keyset prev 时反转方向，取回后 data.reverse() 恢复显示顺序）
    let query_dir = if go_prev {
        if dir_asc {
            "DESC"
        } else {
            "ASC"
        }
    } else {
        dir_sql
    };
    let order_clause = if order_field.is_empty() {
        String::new()
    } else if has_rid {
        format!(
            "ORDER BY {} {}, rowid {}",
            order_field, query_dir, query_dir
        )
    } else {
        format!("ORDER BY {} {}", order_field, query_dir)
    };

    let select_cols = if has_rid {
        "rowid,*".to_string()
    } else {
        "*".to_string()
    };
    let sql = format!(
        "SELECT {} FROM {}{}{} LIMIT ?{} OFFSET ?{}",
        select_cols, safe_table, where_clause, order_clause, limit_idx, offset_idx
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("查询数据失败: {}", friendly_db_error(&e)))?;
    let col_names: Vec<String> = (0..stmt.column_count())
        .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
        .collect();

    let offset = if use_cursor {
        0
    } else {
        page.saturating_mul(page_size)
    };
    let page_size_i = page_size as i64;
    let offset_i = offset as i64;
    let mut param_refs: Vec<&dyn rusqlite::types::ToSql> = Vec::with_capacity(5);
    if filter_present {
        param_refs.push(filter_like.as_ref().unwrap());
    }
    let cv: &str;
    if use_cursor {
        cv = cursor_value.as_deref().unwrap_or("");
        param_refs.push(&cv);
        if order_key != "rowid" {
            param_refs.push(&cursor_rid);
        }
    }
    param_refs.push(&page_size_i);
    param_refs.push(&offset_i);

    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            let mut map = serde_json::Map::new();
            for (i, name) in col_names.iter().enumerate() {
                let val: serde_json::Value = match row.get_ref(i) {
                    Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
                    Ok(rusqlite::types::ValueRef::Integer(n)) => {
                        serde_json::Value::String(n.to_string())
                    }
                    Ok(rusqlite::types::ValueRef::Real(f)) => {
                        if f.fract() == 0.0 && f.is_finite() {
                            serde_json::Value::String(format!("{}", f as i64))
                        } else {
                            serde_json::Value::String(f.to_string())
                        }
                    }
                    Ok(rusqlite::types::ValueRef::Text(s)) => {
                        serde_json::Value::String(String::from_utf8_lossy(s).to_string())
                    }
                    Ok(rusqlite::types::ValueRef::Blob(b)) => {
                        serde_json::Value::String(blob_to_preview(b))
                    }
                    Err(_) => serde_json::Value::Null,
                };
                map.insert(name.clone(), val);
            }
            Ok(map)
        })
        .map_err(|e| format!("读取数据行失败: {}", friendly_db_error(&e)))?;

    let mut data = Vec::new();
    for map in rows.flatten() {
        data.push(serde_json::Value::Object(map));
    }
    if go_prev {
        data.reverse();
    }

    let total = if recount {
        let count_where = if filter_present {
            format!(" WHERE ({})", filter_conds.join(" OR "))
        } else {
            String::new()
        };
        conn.query_row(
            &format!("SELECT COUNT(*) FROM {}{}", safe_table, count_where),
            rusqlite::params_from_iter(filter_like.iter()),
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as usize
    } else {
        0
    };

    // 生成 next/prev 游标（无 rowid / BLOB 排序时不支持 keyset）
    let make_cursor = |row: &serde_json::Map<String, serde_json::Value>| -> Option<String> {
        let v = row
            .get(&order_key)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let rid = row
            .get("rowid")
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        Some(serde_json::json!([v, rid]).to_string())
    };
    let prev_cursor = if has_rid && !order_field.is_empty() && !col_is_blob && !data.is_empty() {
        make_cursor(data[0].as_object().unwrap())
    } else {
        None
    };
    let next_cursor = if has_rid && !order_field.is_empty() && !col_is_blob && !data.is_empty() {
        make_cursor(data[data.len() - 1].as_object().unwrap())
    } else {
        None
    };

    Ok(TableData {
        columns: cols,
        rows: data,
        total,
        page,
        page_size,
        next_cursor,
        prev_cursor,
    })
}
