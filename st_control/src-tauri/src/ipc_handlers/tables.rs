// ============================================================
// 内部表浏览 / CRUD IPC
// 依赖：db / sql_browse（完全限定），零顶层导入
// ============================================================

// ─── 表浏览 / CRUD ───

#[tauri::command]
pub async fn list_tables(db: tauri::State<'_, crate::db::Database>) -> Result<Vec<String>, String> {
    db.list_tables().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn table_schema(
    db: tauri::State<'_, crate::db::Database>,
    table: String,
) -> Result<Vec<crate::db::ColumnInfo>, String> {
    db.table_schema(&table).map_err(|e| e.to_string())
}

#[tauri::command]
// IPC 契约要求扁平参数（前端固定传参顺序），参数对象收敛不适用于 command 入口
#[allow(clippy::too_many_arguments)]
pub async fn query_table(
    db: tauri::State<'_, crate::db::Database>,
    table: String,
    page: usize,
    page_size: usize,
    order_col: String,
    order_dir: String,
    filter: String,
    recount: bool,
    cursor: Option<String>,
    direction: String,
) -> Result<crate::db::TableData, String> {
    db.query_table(&crate::sql_browse::TableQueryParams {
        table,
        page,
        page_size,
        order_col,
        order_dir,
        filter,
        recount,
        cursor,
        direction,
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn insert_row(
    db: tauri::State<'_, crate::db::Database>,
    table: String,
    data: serde_json::Value,
) -> Result<i64, String> {
    let map = data.as_object().ok_or("data 须为对象")?.clone();
    db.insert_row(&table, &map).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_row(
    db: tauri::State<'_, crate::db::Database>,
    table: String,
    rowid: i64,
    data: serde_json::Value,
) -> Result<(), String> {
    let map = data.as_object().ok_or("data 须为对象")?.clone();
    db.update_row(&table, rowid, &map)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_row(
    db: tauri::State<'_, crate::db::Database>,
    table: String,
    rowid: i64,
) -> Result<(), String> {
    db.delete_row(&table, rowid).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cleanup_old_data(
    db: tauri::State<'_, crate::db::Database>,
) -> Result<crate::db::CleanupResult, String> {
    db.cleanup_old_data().map_err(|e| e.to_string())
}
