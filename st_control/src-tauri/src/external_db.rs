use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 表浏览/查询共用类型与实现（与内置库共用同一引擎）
pub use crate::sql_browse::{cell_value_to_json, ColumnInfo, TableData};

/// 外部数据库文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbFileInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
}

/// 扫描指定目录下的所有 .db 文件（递归），返回文件信息列表
pub fn scan_db_files(dir: &Path) -> Result<Vec<DbFileInfo>, String> {
    if !dir.exists() {
        return Err(format!("路径不存在: {}", dir.display()));
    }
    if !dir.is_dir() {
        return Err(format!("不是目录: {}", dir.display()));
    }

    let mut results = Vec::new();
    scan_dir_recursive(dir, &mut results)?;
    // 按文件大小降序（大文件在前）
    results.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));
    Ok(results)
}

fn scan_dir_recursive(dir: &Path, results: &mut Vec<DbFileInfo>) -> Result<(), String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("[db-scan] 跳过不可读目录 {}: {}", dir.display(), e);
            return Ok(());
        }
    };
    for entry in entries.flatten() {
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        // 不跟随符号链接，避免目录环导致死循环
        if ft.is_symlink() {
            continue;
        }
        let path = entry.path();
        if ft.is_dir() {
            scan_dir_recursive(&path, results)?;
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| {
                matches!(
                    e.to_ascii_lowercase().as_str(),
                    "db" | "sqlite" | "sqlite3" | "db3" | "sdb"
                )
            })
            .unwrap_or(false)
        {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            results.push(DbFileInfo {
                name,
                path: path.to_string_lossy().to_string(),
                size_bytes: size,
            });
        }
    }
    Ok(())
}

/// 列出数据库中所有表（含下划线开头 / 系统表，保证全部可见）
pub fn list_tables(db_path: &str) -> Result<Vec<String>, String> {
    let conn = open_db_readonly(db_path)?;
    crate::sql_browse::list_tables(&conn)
}

/// 获取表的列信息
pub fn table_schema(db_path: &str, table: &str) -> Result<Vec<ColumnInfo>, String> {
    let conn = open_db_readonly(db_path)?;
    crate::sql_browse::table_schema(&conn, table)
}

/// 分页查询表数据（过滤/排序/keyset 分页；recount=false 时跳过 COUNT 以提升大表翻页性能）
pub fn query_table(
    db_path: &str,
    params: &crate::sql_browse::TableQueryParams,
) -> Result<TableData, String> {
    let conn = open_db_readonly(db_path)?;
    crate::sql_browse::query_table(&conn, params)
}

/// 读取某行某列的原始值（用于查看完整 BLOB / 文本内容）
pub fn read_cell(
    db_path: &str,
    table: &str,
    rowid: i64,
    column: &str,
) -> Result<rusqlite::types::Value, String> {
    let conn = open_db_readonly(db_path)?;
    crate::sql_browse::read_cell(&conn, table, rowid, column)
}

/// 打开外部 SQLite 数据库（只读模式），用于浏览/查询。
/// 不会创建 -wal / -shm 等临时文件；WAL 库要求 -shm/-wal 已存在且可读。
pub fn open_db_readonly(path: &str) -> Result<Connection, String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(format!("数据库文件不存在: {}", path));
    }
    let conn =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| {
            let wal_exists = Path::new(&format!("{}-wal", path)).exists();
            if wal_exists {
                format!(
                    "打开数据库失败: {}（该库为 WAL 模式，请确认 -wal/-shm 文件完整且目录可读）",
                    e
                )
            } else {
                format!("打开数据库失败: {}", e)
            }
        })?;
    // 只读连接也设置 busy_timeout，避免其它进程持锁时立即失败
    let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
    Ok(conn)
}

// ─── 路径白名单 ───

/// 规范化路径用于白名单比较（小写、统一分隔符、去尾部斜杠）
fn normalize_path(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// 校验路径是否位于允许的根目录下；不在白名单则返回错误
pub fn ensure_allowed_path(path: &str, roots: &[String]) -> Result<(), String> {
    let np = normalize_path(path);
    if np.is_empty() {
        return Err("路径为空".to_string());
    }
    for r in roots {
        let nr = normalize_path(r);
        if nr.is_empty() {
            continue;
        }
        if np == nr || np.starts_with(&format!("{}\\", nr)) {
            return Ok(());
        }
    }
    Err(format!("路径不在允许的扫描目录内: {}", path))
}
