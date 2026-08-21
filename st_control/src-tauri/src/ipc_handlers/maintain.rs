// ============================================================
// 数据库增强能力 IPC — 详情 / 完整性 / SQL / 统计 / 导出 / 备份
// 依赖：db / sql_browse（完全限定），零顶层导入
// ============================================================

// ─────────────────────────────────────────────
// 数据库增强能力（表详情 / 完整性 / SQL / 统计 / 导出 / 备份）
// ─────────────────────────────────────────────

/// 打开连接：外部库只读；内置库可读写（同一文件另开连接，WAL 兼容）
fn open_conn(db_path: Option<String>, db: &crate::db::Database) -> Result<(String, bool), String> {
    match db_path {
        Some(p) => {
            crate::external_db::ensure_allowed_path(&p, &super::allowed_db_roots(db))?;
            Ok((p, true))
        }
        None => Ok((db.path().to_string_lossy().to_string(), false)),
    }
}

/// 表详情：DDL + 索引 + 触发器 + 外键
#[tauri::command]
pub async fn get_table_detail(
    db: tauri::State<'_, crate::db::Database>,
    db_path: Option<String>,
    table: String,
) -> Result<serde_json::Value, String> {
    let (p, external) = open_conn(db_path, &db)?;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = if external {
            crate::external_db::open_db_readonly(&p)?
        } else {
            rusqlite::Connection::open(&p).map_err(|e| format!("打开数据库失败: {}", e))?
        };
        crate::sql_browse::table_detail(&conn, &table)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 数据库完整性检查
#[tauri::command]
pub async fn db_integrity(
    db: tauri::State<'_, crate::db::Database>,
    db_path: Option<String>,
) -> Result<serde_json::Value, String> {
    let (p, external) = open_conn(db_path, &db)?;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = if external {
            crate::external_db::open_db_readonly(&p)?
        } else {
            rusqlite::Connection::open(&p).map_err(|e| format!("打开数据库失败: {}", e))?
        };
        crate::sql_browse::db_integrity(&conn)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 执行 SQL（外部库强制只读）
#[tauri::command]
pub async fn run_sql(
    db: tauri::State<'_, crate::db::Database>,
    db_path: Option<String>,
    sql: String,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    let (p, external) = open_conn(db_path, &db)?;
    let limit = limit.unwrap_or(500).clamp(1, 5000);
    tauri::async_runtime::spawn_blocking(move || {
        let conn = if external {
            crate::external_db::open_db_readonly(&p)?
        } else {
            rusqlite::Connection::open(&p).map_err(|e| format!("打开数据库失败: {}", e))?
        };
        crate::sql_browse::execute_sql(&conn, &sql, limit, external)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 列统计（抽样）
#[tauri::command]
pub async fn table_stats(
    db: tauri::State<'_, crate::db::Database>,
    db_path: Option<String>,
    table: String,
    sample: Option<usize>,
) -> Result<serde_json::Value, String> {
    let (p, external) = open_conn(db_path, &db)?;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = if external {
            crate::external_db::open_db_readonly(&p)?
        } else {
            rusqlite::Connection::open(&p).map_err(|e| format!("打开数据库失败: {}", e))?
        };
        crate::sql_browse::table_stats(&conn, &table, sample.unwrap_or(2000))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 整表导出为 CSV（分块流式写入，支持内外库）
#[tauri::command]
pub async fn export_table_csv(
    db: tauri::State<'_, crate::db::Database>,
    db_path: Option<String>,
    table: String,
    path: String,
) -> Result<serde_json::Value, String> {
    if path.trim().is_empty() {
        return Err("保存路径为空".to_string());
    }
    let (p, external) = open_conn(db_path, &db)?;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = if external {
            crate::external_db::open_db_readonly(&p)?
        } else {
            rusqlite::Connection::open(&p).map_err(|e| format!("打开数据库失败: {}", e))?
        };
        let count =
            crate::sql_browse::export_table_to_csv(&conn, &table, std::path::Path::new(&path))?;
        Ok(serde_json::json!({ "path": path, "count": count }))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 备份内置库（先 checkpoint 再复制到指定路径）
#[tauri::command]
pub async fn backup_internal_db(
    db: tauri::State<'_, crate::db::Database>,
    path: String,
) -> Result<serde_json::Value, String> {
    if path.trim().is_empty() {
        return Err("保存路径为空".to_string());
    }
    db.checkpoint()
        .map_err(|e| format!("checkpoint 失败: {}", e))?;
    let src = db.path();
    let target = std::path::PathBuf::from(&path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    std::fs::copy(&src, &target).map_err(|e| format!("备份失败: {}", e))?;
    log::info!("[db-backup] 已备份 control.db → {}", target.display());
    Ok(serde_json::json!({
        "path": target.to_string_lossy().to_string(),
        "size_bytes": std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0),
    }))
}

/// 准备恢复：把备份复制为 control.db.restore（需关闭应用后替换生效）
#[tauri::command]
pub async fn restore_internal_db(
    db: tauri::State<'_, crate::db::Database>,
    backup_path: String,
) -> Result<serde_json::Value, String> {
    let backup = std::path::PathBuf::from(&backup_path);
    if !backup.exists() {
        return Err("备份文件不存在".to_string());
    }
    let target = db.path().with_extension("db.restore");
    std::fs::copy(&backup, &target).map_err(|e| format!("复制备份失败: {}", e))?;
    log::info!("[db-restore] 已生成恢复文件 {}", target.display());
    Ok(serde_json::json!({
        "restore_path": target.to_string_lossy().to_string(),
        "hint": "请先关闭应用，再将此文件改名为 control.db 覆盖原文件，然后重新启动。",
    }))
}
