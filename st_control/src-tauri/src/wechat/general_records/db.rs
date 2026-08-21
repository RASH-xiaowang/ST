// ============================================================
// 微信 general.db 记录查询 — 数据库辅助域
// 自 general_records.rs 拆分：路径定位/只读连接/参数钳制/行转 JSON。
// ============================================================

use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

pub(crate) const MAX_LIMIT: i64 = 200;

pub(crate) fn general_db_path() -> Option<PathBuf> {
    let cfg = crate::wechat::config::WeChatConfig::load().ok()?;
    let p = cfg.decrypted_dir.join("general").join("general.db");
    p.is_file().then_some(p)
}

pub(crate) fn open_general() -> Option<Connection> {
    let p = general_db_path()?;
    Connection::open_with_flags(
        &p,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
}

pub(crate) fn clamp(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    let limit = limit.unwrap_or(80).clamp(1, MAX_LIMIT);
    let offset = offset.unwrap_or(0).max(0);
    (limit, offset)
}

pub(crate) fn rows_to_json(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::types::ToSql],
) -> Vec<serde_json::Value> {
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows = match stmt.query(params) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    while let Ok(Some(row)) = rows.next() {
        let mut obj = serde_json::Map::new();
        for (i, name) in col_names.iter().enumerate() {
            let v = match row.get_ref(i) {
                Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
                Ok(rusqlite::types::ValueRef::Integer(n)) => serde_json::json!(n),
                Ok(rusqlite::types::ValueRef::Real(f)) => serde_json::json!(f),
                Ok(rusqlite::types::ValueRef::Text(t)) => {
                    serde_json::Value::String(String::from_utf8_lossy(t).to_string())
                }
                Ok(rusqlite::types::ValueRef::Blob(b)) => {
                    serde_json::Value::String(crate::wechat::modules::common::decode_blob_text(b))
                }
                Err(_) => serde_json::Value::Null,
            };
            obj.insert(name.clone(), v);
        }
        out.push(serde_json::Value::Object(obj));
    }
    out
}

pub(crate) fn total(conn: &Connection, table: &str) -> i64 {
    let sql = format!("SELECT COUNT(*) FROM \"{}\"", table);
    conn.query_row(&sql, [], |r| r.get::<_, i64>(0))
        .unwrap_or(0)
}
