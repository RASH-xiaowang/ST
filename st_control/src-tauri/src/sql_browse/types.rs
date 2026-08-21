// ============================================================
// SQLite 表浏览 — 数据类型
// 自 sql_browse.rs 拆分：列信息/分页数据/查询参数。
// ============================================================

use serde::{Deserialize, Serialize};

/// 列信息（PRAGMA table_info）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub cid: usize,
    pub name: String,
    pub col_type: String,
    pub not_null: bool,
    pub default: Option<String>,
    pub pk: bool,
}

/// 分页表数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableData {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<serde_json::Value>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub next_cursor: Option<String>,
    pub prev_cursor: Option<String>,
}

/// 表查询参数（分页/过滤/排序；cursor+direction 为 keyset 分页，recount=false 时跳过 COUNT）
pub struct TableQueryParams {
    pub table: String,
    pub page: usize,
    pub page_size: usize,
    pub order_col: String,
    pub order_dir: String,
    pub filter: String,
    pub recount: bool,
    pub cursor: Option<String>,
    pub direction: String,
}
