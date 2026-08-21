// ============================================================
// 微信 IPC — 消息编辑域（编辑 + 原始字段编辑）
// 依赖：helpers / modules::common / edit_store / config / rusqlite
// ============================================================

use crate::wechat::handlers::helpers;

// ============================================================
// 消息编辑（迁移自 WeChatDataAnalysis：本地修改解密副本，支持恢复）
// ============================================================

/// 定位消息所在的分库与表名（返回 (db_path, table_name)）
fn find_message_db(
    decrypted_dir: &std::path::Path,
    username: &str,
) -> Result<(std::path::PathBuf, String), String> {
    let table = crate::wechat::modules::common::msg_table_name(username);
    let mut dbs = crate::wechat::modules::common::find_db_files(decrypted_dir, "message_");
    dbs.extend(crate::wechat::modules::common::find_db_files(
        decrypted_dir,
        "biz_message_",
    ));
    dbs.sort();
    dbs.dedup();
    dbs.retain(|p| crate::wechat::modules::common::is_message_shard_file(p));
    for p in dbs {
        if let Ok(conn) = helpers::open_writable_db(&p) {
            if crate::wechat::modules::common::table_exists(&conn, &table) {
                return Ok((p, table));
            }
        }
    }
    Err("未找到该会话的消息数据库（请先解密）".to_string())
}

/// 读取消息原始内容（返回 (content_type, bytes)）
fn read_message_content(
    conn: &rusqlite::Connection,
    table: &str,
    local_id: i64,
) -> Result<(String, Vec<u8>), String> {
    use rusqlite::types::ValueRef;
    let sql = format!(
        "SELECT message_content FROM \"{}\" WHERE local_id=?1 LIMIT 1",
        table
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map(rusqlite::params![local_id], |r| {
            let v = r.get_ref(0)?;
            Ok(match v {
                ValueRef::Text(t) => ("text".to_string(), t.to_vec()),
                ValueRef::Blob(b) => ("blob".to_string(), b.to_vec()),
                _ => ("null".to_string(), Vec::new()),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.next()
        .ok_or_else(|| "消息不存在".to_string())?
        .map_err(|e| e.to_string())
}

/// 查询某条消息的编辑状态（是否被本地修改过）
#[tauri::command]
pub async fn get_chat_edit_status(
    username: String,
    local_id: i64,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let account = cfg.wxid().unwrap_or_default();
        let (db_path, table) = find_message_db(&cfg.decrypted_dir, &username)?;
        let db_stem = db_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(
            match crate::wechat::edit_store::get_edit_status(
                &account, &username, &db_stem, &table, local_id,
            ) {
                Some(v) => v,
                None => serde_json::json!({ "modified": false }),
            },
        )
    })
    .await
}

/// 列出某会话已编辑过的消息（用于前端加载时标记“已编辑”徽标）
#[tauri::command]
pub async fn list_session_edited_messages(username: String) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let account = cfg.wxid().unwrap_or_default();
        let items =
            crate::wechat::edit_store::list_session_edits(&account, &username).unwrap_or_default();
        Ok(serde_json::json!({ "items": items }))
    })
    .await
}

/// 本地修改消息内容（仅解密副本，微信源库不受影响）
#[tauri::command]
pub async fn edit_chat_message(
    username: String,
    local_id: i64,
    new_text: String,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let account = cfg.wxid().unwrap_or_default();
        let (db_path, table) = find_message_db(&cfg.decrypted_dir, &username)?;
        let conn = helpers::open_writable_db(&db_path)?;

        let (content_type, orig_bytes) = read_message_content(&conn, &table, local_id)?;
        // 首次编辑前保存原始快照（hex），用于恢复
        let snapshot = serde_json::json!({
            "content_type": content_type,
            "message_content_hex": hex::encode(&orig_bytes),
        });
        let db_stem = db_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        // 写入新内容：保持与原始存储格式一致（BLOB=zstd 压缩 / TEXT=明文）
        let sql = format!(
            "UPDATE \"{}\" SET message_content=?1 WHERE local_id=?2",
            table
        );
        if content_type == "blob" {
            let compressed = zstd::stream::encode_all(std::io::Cursor::new(new_text.as_bytes()), 0)
                .map_err(|e| format!("压缩消息内容失败: {}", e))?;
            conn.execute(&sql, rusqlite::params![compressed, local_id])
                .map_err(|e| e.to_string())?;
        } else {
            conn.execute(&sql, rusqlite::params![new_text.clone(), local_id])
                .map_err(|e| e.to_string())?;
        }

        crate::wechat::edit_store::record_edit(
            &account,
            &username,
            &db_stem,
            &table,
            local_id,
            &snapshot.to_string(),
        )?;

        Ok(serde_json::json!({
            "ok": true,
            "updated": { "username": username, "local_id": local_id, "text": new_text }
        }))
    })
    .await
}

/// 恢复消息到首次编辑前的原始内容
#[tauri::command]
pub async fn reset_edited_message(
    username: String,
    local_id: i64,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let account = cfg.wxid().unwrap_or_default();
        let (db_path, table) = find_message_db(&cfg.decrypted_dir, &username)?;
        let db_stem = db_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        let snapshot = crate::wechat::edit_store::get_original_snapshot(
            &account, &username, &db_stem, &table, local_id,
        )
        .ok_or_else(|| "该消息没有可恢复的编辑记录".to_string())?;
        let v: serde_json::Value = serde_json::from_str(&snapshot).map_err(|e| e.to_string())?;
        let conn = helpers::open_writable_db(&db_path)?;
        let snapshot_type = v.get("type").and_then(|x| x.as_str()).unwrap_or("content");
        if snapshot_type == "raw" {
            if let Some(cols) = v.get("columns").and_then(|x| x.as_object()) {
                for (col, val) in cols {
                    if col == "local_id" {
                        continue;
                    }
                    let bind = json_to_sql_value(val)?;
                    let sql = format!("UPDATE \"{}\" SET \"{}\"=?1 WHERE local_id=?2", table, col);
                    conn.execute(&sql, rusqlite::params![bind, local_id])
                        .map_err(|e| e.to_string())?;
                }
            }
        } else {
            let hex_str = v
                .get("message_content_hex")
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let bytes = hex::decode(hex_str).map_err(|e| format!("原始快照损坏: {}", e))?;
            let content_type = v
                .get("content_type")
                .and_then(|x| x.as_str())
                .unwrap_or("blob");
            let sql = format!(
                "UPDATE \"{}\" SET message_content=?1 WHERE local_id=?2",
                table
            );
            if content_type == "text" {
                let text = String::from_utf8(bytes).unwrap_or_default();
                conn.execute(&sql, rusqlite::params![text, local_id])
                    .map_err(|e| e.to_string())?;
            } else {
                conn.execute(&sql, rusqlite::params![bytes, local_id])
                    .map_err(|e| e.to_string())?;
            }
        }
        crate::wechat::edit_store::delete_edit(&account, &username, &db_stem, &table, local_id)?;

        Ok(serde_json::json!({ "ok": true }))
    })
    .await
}

// ============================================================
// 消息原始字段编辑（迁移自 WeChatDataAnalysis 的字段编辑弹窗）
// ============================================================

/// JSON 值 → SQLite 绑定值（字符串 "0x.." 视为 BLOB 十六进制）
fn json_to_sql_value(v: &serde_json::Value) -> Result<rusqlite::types::Value, String> {
    Ok(match v {
        serde_json::Value::Null => rusqlite::types::Value::Null,
        serde_json::Value::Bool(b) => rusqlite::types::Value::Integer(*b as i64),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                rusqlite::types::Value::Integer(i)
            } else if let Some(u) = n.as_u64() {
                rusqlite::types::Value::Integer(u.min(i64::MAX as u64) as i64)
            } else if let Some(f) = n.as_f64() {
                rusqlite::types::Value::Real(f)
            } else {
                rusqlite::types::Value::Null
            }
        }
        serde_json::Value::String(s) => {
            if let Some(hex_part) = s.strip_prefix("0x") {
                if !hex_part.is_empty()
                    && hex_part.len() % 2 == 0
                    && hex_part.chars().all(|c| c.is_ascii_hexdigit())
                {
                    rusqlite::types::Value::Blob(hex::decode(hex_part).unwrap_or_default())
                } else {
                    rusqlite::types::Value::Text(s.clone())
                }
            } else {
                rusqlite::types::Value::Text(s.clone())
            }
        }
        serde_json::Value::Array(_) => return Err("数组类型不支持直接写入".to_string()),
        serde_json::Value::Object(_) => return Err("对象类型不支持直接写入".to_string()),
    })
}

/// 读取消息完整原始行（BLOB 列以 "0x.." 十六进制字符串表示）
fn read_full_row(
    conn: &rusqlite::Connection,
    table: &str,
    local_id: i64,
) -> Result<serde_json::Value, String> {
    use rusqlite::types::ValueRef;
    let sql = format!("SELECT * FROM \"{}\" WHERE local_id=?1 LIMIT 1", table);
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows = stmt
        .query_map(rusqlite::params![local_id], |row| {
            let mut map = serde_json::Map::new();
            for (i, name) in cols.iter().enumerate() {
                let jv = match row.get_ref(i)? {
                    ValueRef::Null => serde_json::Value::Null,
                    ValueRef::Integer(x) => serde_json::json!(x),
                    ValueRef::Real(x) => serde_json::json!(x),
                    ValueRef::Text(t) => {
                        serde_json::Value::String(String::from_utf8_lossy(t).to_string())
                    }
                    ValueRef::Blob(b) => serde_json::Value::String(format!("0x{}", hex::encode(b))),
                };
                map.insert(name.clone(), jv);
            }
            Ok::<_, rusqlite::Error>(serde_json::Value::Object(map))
        })
        .map_err(|e| e.to_string())?;
    rows.next()
        .ok_or_else(|| "消息不存在".to_string())?
        .map_err(|e| e.to_string())
}

/// 读取消息原始行（供字段编辑弹窗）
#[tauri::command]
pub async fn get_message_raw_row(
    username: String,
    local_id: i64,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let (db_path, table) = find_message_db(&cfg.decrypted_dir, &username)?;
        let conn = helpers::open_writable_db(&db_path)?;
        let row = read_full_row(&conn, &table, local_id)?;
        let db_stem = db_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(serde_json::json!({ "row": row, "db": db_stem, "table": table }))
    })
    .await
}

/// 安全列白名单（不勾选危险模式时仅允许修改这些字段）
const RAW_EDIT_SAFE_COLUMNS: &[&str] = &[
    "message_content",
    "local_type",
    "create_time",
    "server_id",
    "sort_seq",
    "real_sender_id",
    "compress_content",
    "status",
    "is_encrypt",
];

/// 修改消息原始字段（首次修改前自动保存整行快照，可恢复）
#[tauri::command]
pub async fn update_message_raw_fields(
    username: String,
    local_id: i64,
    edits: serde_json::Value,
    unsafe_edit: Option<bool>,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        let account = cfg.wxid().unwrap_or_default();
        let (db_path, table) = find_message_db(&cfg.decrypted_dir, &username)?;
        let db_stem = db_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        let conn = helpers::open_writable_db(&db_path)?;

        let obj = edits
            .as_object()
            .ok_or_else(|| "edits 必须是 JSON 对象".to_string())?
            .clone();
        if obj.is_empty() {
            return Err("edits 不能为空".to_string());
        }
        let cols = crate::wechat::modules::common::table_columns(&conn, &table);
        let unsafe_mode = unsafe_edit.unwrap_or(false);
        for key in obj.keys() {
            if key == "local_id" {
                return Err("不允许修改主键 local_id".to_string());
            }
            if !cols.contains(key) {
                return Err(format!("列 {} 不存在", key));
            }
            if !unsafe_mode && !RAW_EDIT_SAFE_COLUMNS.contains(&key.as_str()) {
                return Err(format!("修改列 {} 需要勾选“高级（危险）模式”", key));
            }
        }

        // 快照：优先合并既有 content 快照（保留真正的原始 message_content）
        let row_json = read_full_row(&conn, &table, local_id)?;
        let existing = crate::wechat::edit_store::get_original_snapshot(
            &account, &username, &db_stem, &table, local_id,
        );
        let snapshot: serde_json::Value = match existing {
            Some(old) => {
                let ov: serde_json::Value =
                    serde_json::from_str(&old).unwrap_or(serde_json::Value::Null);
                if ov.get("type").and_then(|x| x.as_str()) == Some("raw") {
                    ov
                } else {
                    // content 快照 → 升级为 raw：message_content 用原始快照，其余用当前值
                    let mut cols = row_json.clone();
                    if let Some(hex) = ov.get("message_content_hex").and_then(|x| x.as_str()) {
                        if let Ok(bytes) = hex::decode(hex) {
                            if let Some(m) = cols.as_object_mut() {
                                m.insert(
                                    "message_content".to_string(),
                                    serde_json::json!(format!("0x{}", hex::encode(&bytes))),
                                );
                            }
                        }
                    }
                    serde_json::json!({ "type": "raw", "columns": cols })
                }
            }
            None => serde_json::json!({ "type": "raw", "columns": row_json }),
        };
        // 覆盖旧记录（保留合并后的原始快照）
        let _ =
            crate::wechat::edit_store::delete_edit(&account, &username, &db_stem, &table, local_id);
        crate::wechat::edit_store::record_edit(
            &account,
            &username,
            &db_stem,
            &table,
            local_id,
            &snapshot.to_string(),
        )?;

        // 逐列写入
        for (key, val) in &obj {
            if key == "message_content" {
                // 当前为 BLOB 且传入普通文本 → zstd 压缩存储
                if let Some(cur) = row_json.get("message_content").and_then(|x| x.as_str()) {
                    if cur.starts_with("0x") {
                        if let Some(s) = val.as_str() {
                            let sql = format!(
                                "UPDATE \"{}\" SET message_content=?1 WHERE local_id=?2",
                                table
                            );
                            if let Some(hex_str) = s.strip_prefix("0x") {
                                let bytes = hex::decode(hex_str)
                                    .map_err(|e| format!("BLOB 十六进制格式错误: {}", e))?;
                                conn.execute(&sql, rusqlite::params![bytes, local_id])
                                    .map_err(|e| e.to_string())?;
                            } else {
                                let compressed =
                                    zstd::stream::encode_all(std::io::Cursor::new(s.as_bytes()), 0)
                                        .map_err(|e| format!("压缩消息内容失败: {}", e))?;
                                conn.execute(&sql, rusqlite::params![compressed, local_id])
                                    .map_err(|e| e.to_string())?;
                            }
                            continue;
                        }
                    }
                }
            }
            let bind = json_to_sql_value(val)?;
            let sql = format!("UPDATE \"{}\" SET \"{}\"=?1 WHERE local_id=?2", table, key);
            conn.execute(&sql, rusqlite::params![bind, local_id])
                .map_err(|e| e.to_string())?;
        }

        Ok(serde_json::json!({ "ok": true }))
    })
    .await
}
