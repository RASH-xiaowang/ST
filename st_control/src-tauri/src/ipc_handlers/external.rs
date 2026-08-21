// ============================================================
// 外部数据库浏览 / CRUD IPC
// 依赖：db / sql_browse（完全限定），零顶层导入
// ============================================================

// ─────────────────────────────────────────────
// 外部数据库浏览/CRUD IPC
// ─────────────────────────────────────────────

#[tauri::command]
pub async fn scan_external_dbs(
    db: tauri::State<'_, crate::db::Database>,
    dir: String,
) -> Result<Vec<crate::external_db::DbFileInfo>, String> {
    crate::external_db::ensure_allowed_path(&dir, &super::allowed_db_roots(&db))?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = std::path::Path::new(&dir);
        // 目录不存在（如旧版本持久化的 %APPDATA% 路径已被统一目录方案迁移）
        // 时返回空列表而非报错：避免前端扫描循环对每个失效目录刷错误提示
        if !path.is_dir() {
            return Ok(Vec::new());
        }
        crate::external_db::scan_db_files(path)
    })
    .await
    .map_err(|e| format!("扫描任务失败: {}", e))?
}

#[tauri::command]
pub async fn check_db_header(
    db: tauri::State<'_, crate::db::Database>,
    db_path: String,
) -> Result<serde_json::Value, String> {
    crate::external_db::ensure_allowed_path(&db_path, &super::allowed_db_roots(&db))?;
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        use std::path::Path;
        let p = Path::new(&db_path);
        if !p.exists() { return Err(format!("文件不存在: {}", db_path)); }
        // 只读取文件头前 100 字节，避免大文件全量读入内存
        let mut file = std::fs::File::open(p).map_err(|e| format!("打开文件失败: {}", e))?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        let mut buf = vec![0u8; size.min(100) as usize];
        let n = file.read(&mut buf).map_err(|e| format!("读取文件失败: {}", e))?;
        buf.truncate(n);
        let header_bytes: Vec<String> = buf.iter().map(|b| format!("{:02X}", b)).collect();
        let magic_ok = buf.len() >= 16 && &buf[..16] == b"SQLite format 3\0";
        let human: String = buf.iter().map(|b| { if b.is_ascii_graphic() || *b == b' ' { *b as char } else { '.' } }).collect();
        Ok(serde_json::json!({ "path": db_path, "size_bytes": size, "is_sqlite": magic_ok, "header_hex": header_bytes.join(" "), "header_text": human, "page_count": if size >= 4096 { size / 4096 } else { 0 } }))
    }).await.map_err(|e| format!("任务执行失败: {}", e))?
}

#[tauri::command]
pub async fn external_list_tables(
    db: tauri::State<'_, crate::db::Database>,
    db_path: String,
) -> Result<Vec<String>, String> {
    crate::external_db::ensure_allowed_path(&db_path, &super::allowed_db_roots(&db))?;
    tauri::async_runtime::spawn_blocking(move || crate::external_db::list_tables(&db_path))
        .await
        .map_err(|e| format!("任务执行失败: {}", e))?
}

#[tauri::command]
pub async fn external_table_schema(
    db: tauri::State<'_, crate::db::Database>,
    db_path: String,
    table: String,
) -> Result<Vec<crate::external_db::ColumnInfo>, String> {
    crate::external_db::ensure_allowed_path(&db_path, &super::allowed_db_roots(&db))?;
    tauri::async_runtime::spawn_blocking(move || crate::external_db::table_schema(&db_path, &table))
        .await
        .map_err(|e| format!("任务执行失败: {}", e))?
}

#[tauri::command]
// IPC 契约要求扁平参数（前端固定传参顺序），参数对象收敛不适用于 command 入口
#[allow(clippy::too_many_arguments)]
pub async fn external_query_table(
    db: tauri::State<'_, crate::db::Database>,
    db_path: String,
    table: String,
    page: usize,
    page_size: usize,
    order_col: String,
    order_dir: String,
    filter: String,
    recount: bool,
    cursor: Option<String>,
    direction: String,
) -> Result<crate::external_db::TableData, String> {
    crate::external_db::ensure_allowed_path(&db_path, &super::allowed_db_roots(&db))?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::external_db::query_table(
            &db_path,
            &crate::sql_browse::TableQueryParams {
                table,
                page,
                page_size,
                order_col,
                order_dir,
                filter,
                recount,
                cursor,
                direction,
            },
        )
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

#[tauri::command]
pub async fn get_cell_value(
    db: tauri::State<'_, crate::db::Database>,
    db_path: Option<String>,
    table: String,
    rowid: i64,
    column: String,
) -> Result<serde_json::Value, String> {
    if let Some(p) = &db_path {
        crate::external_db::ensure_allowed_path(p, &super::allowed_db_roots(&db))?;
    }
    let value = if let Some(p) = &db_path {
        let p = p.clone();
        tauri::async_runtime::spawn_blocking(move || {
            crate::external_db::read_cell(&p, &table, rowid, &column)
        })
        .await
        .map_err(|e| format!("任务执行失败: {}", e))?
        .map_err(|e| e.to_string())?
    } else {
        db.read_cell(&table, rowid, &column)
            .map_err(|e| e.to_string())?
    };
    Ok(crate::external_db::cell_value_to_json(&value))
}

/// 写入导出文件（内容为 base64，支持文本/二进制），供前端保存 CSV / BLOB 等
#[tauri::command]
pub async fn write_file(path: String, content_b64: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use base64::Engine as _;
        if path.trim().is_empty() {
            return Err("保存路径为空".to_string());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&content_b64)
            .map_err(|e| format!("内容解码失败: {}", e))?;
        std::fs::write(&path, &bytes).map_err(|e| format!("写入文件失败: {}", e))?;
        log::info!("已写入导出文件: {}", path);
        Ok(())
    })
    .await
    .map_err(|e| format!("写入任务失败: {}", e))?
}
