// ============================================================
// 微信 general.db 记录查询 — 列表查询域
// 自 general_records.rs 拆分：撤回/转账/红包/视频号/小程序/好友验证。
// ============================================================

use super::{clamp, open_general, rows_to_json, total};

/// 撤回消息缓存（revokebatchmessage）
pub fn list_revokes(
    limit: Option<i64>,
    offset: Option<i64>,
    q: Option<String>,
) -> Result<serde_json::Value, String> {
    let conn = open_general().ok_or_else(|| "未找到解密后的 general.db".to_string())?;
    let (limit, offset) = clamp(limit, offset);
    let kw = q.unwrap_or_default();
    let where_sql = if kw.is_empty() {
        String::new()
    } else {
        format!(
            " WHERE session_name LIKE '%{}%' OR msg_unique_id LIKE '%{}%'",
            kw.replace('\'', "''"),
            kw.replace('\'', "''")
        )
    };
    let sql = format!(
        "SELECT local_id, batch_id, msg_unique_id, session_name, msg_local_id, msg_create_time \
         FROM revokebatchmessage{} ORDER BY msg_create_time DESC LIMIT ?1 OFFSET ?2",
        where_sql
    );
    let items = rows_to_json(&conn, &sql, &[&limit, &offset]);
    let total = total(&conn, "revokebatchmessage");
    Ok(serde_json::json!({ "items": items, "total": total }))
}

/// 转账记录（transferTable）
pub fn list_transfers(
    limit: Option<i64>,
    offset: Option<i64>,
    q: Option<String>,
) -> Result<serde_json::Value, String> {
    let conn = open_general().ok_or_else(|| "未找到解密后的 general.db".to_string())?;
    let (limit, offset) = clamp(limit, offset);
    // 类型词（红包/转账）不会出现在记录字段里，不能当作 LIKE 过滤条件，
    // 否则用户搜「转账」会得到 0 条（与 AI 问答检索同源问题）
    let kw = q.unwrap_or_default();
    let kw = if is_record_type_stopword(&kw) {
        String::new()
    } else {
        kw
    };
    let where_sql = if kw.is_empty() {
        String::new()
    } else {
        format!(
            " WHERE session_name LIKE '%{}%' OR transfer_id LIKE '%{}%'",
            kw.replace('\'', "''"),
            kw.replace('\'', "''")
        )
    };
    let sql = format!(
        "SELECT transfer_id, transcation_id, message_server_id, second_message_server_id, \
                session_name, pay_sub_type, pay_receiver, pay_payer, begin_transfer_time, \
                last_modified_time, invalid_time, last_update_time, delay_confirm_flag \
         FROM transferTable{} ORDER BY begin_transfer_time DESC LIMIT ?1 OFFSET ?2",
        where_sql
    );
    let items = rows_to_json(&conn, &sql, &[&limit, &offset]);
    let total = total(&conn, "transferTable");
    Ok(serde_json::json!({ "items": items, "total": total }))
}

/// 红包记录（redEnvelopeTable）
pub fn list_red_envelopes(
    limit: Option<i64>,
    offset: Option<i64>,
    q: Option<String>,
) -> Result<serde_json::Value, String> {
    let conn = open_general().ok_or_else(|| "未找到解密后的 general.db".to_string())?;
    let (limit, offset) = clamp(limit, offset);
    let kw = q.unwrap_or_default();
    let kw = if is_record_type_stopword(&kw) {
        String::new()
    } else {
        kw
    };
    let where_sql = if kw.is_empty() {
        String::new()
    } else {
        format!(
            " WHERE session_name LIKE '%{}%' OR sender_user_name LIKE '%{}%'",
            kw.replace('\'', "''"),
            kw.replace('\'', "''")
        )
    };
    let sql = format!(
        "SELECT message_server_id, session_name, sender_user_name, native_url, send_id, \
                scene_id, hb_status, hb_type, receive_status \
         FROM redEnvelopeTable{} ORDER BY message_server_id DESC LIMIT ?1 OFFSET ?2",
        where_sql
    );
    let items = rows_to_json(&conn, &sql, &[&limit, &offset]);
    let total = total(&conn, "redEnvelopeTable");
    Ok(serde_json::json!({ "items": items, "total": total }))
}

/// 记录类型词：这些词是数据源名称，不是记录内容，过滤会误伤全部结果
fn is_record_type_stopword(q: &str) -> bool {
    matches!(
        q.trim().to_lowercase().as_str(),
        "红包"
            | "转账"
            | "转帐"
            | "收款"
            | "付款"
            | "收红包"
            | "发红包"
            | "红包记录"
            | "转账记录"
            | "redpacket"
            | "red_packet"
            | "transfer"
            | "转账明细"
            | "红包明细"
    )
}

/// 视频号直播 / 用户页（wcfinderlivestatus + wcfinderuserpage）
pub fn list_finder(limit: Option<i64>, offset: Option<i64>) -> Result<serde_json::Value, String> {
    let conn = open_general().ok_or_else(|| "未找到解密后的 general.db".to_string())?;
    let (limit, offset) = clamp(limit, offset);
    let sql = "SELECT finder_live_id, finder_username, finder_export_id, live_status, replay_status, charge_flag \
         FROM wcfinderlivestatus ORDER BY finder_live_id DESC LIMIT ?1 OFFSET ?2".to_string();
    let items = rows_to_json(&conn, &sql, &[&limit, &offset]);
    let total = total(&conn, "wcfinderlivestatus");
    Ok(serde_json::json!({ "items": items, "total": total }))
}

/// 小程序（wacontact type 对应小程序 + WeApp 表）
pub fn list_mini_programs(
    limit: Option<i64>,
    offset: Option<i64>,
    q: Option<String>,
) -> Result<serde_json::Value, String> {
    let conn = open_general().ok_or_else(|| "未找到解密后的 general.db".to_string())?;
    let (limit, offset) = clamp(limit, offset);
    let kw = q.unwrap_or_default();
    let kw_esc = kw.replace('\'', "''");
    let where_sql = if kw.is_empty() {
        String::new()
    } else {
        format!(" AND (w.user_name LIKE '%{}%')", kw_esc)
    };
    // wacontact 全部记录 LEFT JOIN WeApp 表拿更新时间（小程序无独立 type 标记）
    let sql = format!(
        "SELECT w.user_name, w.type, w.brand_icon_url, w.external_info, w.app_id, \
                COALESCE(a.last_update_time, 0) AS last_update_time \
         FROM wacontact w \
         LEFT JOIN WeAppBizAttrSyncBufferTableV02 a ON a.user_name = w.user_name \
         WHERE 1=1{} \
         ORDER BY COALESCE(a.last_update_time, 0) DESC, w.user_name ASC LIMIT ?1 OFFSET ?2",
        where_sql
    );
    let mut items = rows_to_json(&conn, &sql, &[&limit, &offset]);
    // 解析 external_info 中的小程序名称
    for item in items.iter_mut() {
        if let Some(ext) = item.get("external_info").and_then(|v| v.as_str()) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(ext) {
                let title = v
                    .get("RegisterSource")
                    .and_then(|r| r.get("NickName"))
                    .or_else(|| v.get("NickName"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                item["nickname"] = serde_json::json!(title);
            }
        }
    }
    let total = items.len() as i64;
    Ok(serde_json::json!({ "items": items, "total": total }))
}

/// 好友验证 / 新朋友记录（FMessageTable）
pub fn list_friend_verifications(
    limit: Option<i64>,
    offset: Option<i64>,
    q: Option<String>,
) -> Result<serde_json::Value, String> {
    let conn = open_general().ok_or_else(|| "未找到解密后的 general.db".to_string())?;
    let (limit, offset) = clamp(limit, offset);
    let kw = q.unwrap_or_default();
    let kw_esc = kw.replace('\'', "''");
    let where_sql = if kw.is_empty() {
        String::new()
    } else {
        format!(
            " WHERE user_name_ LIKE '%{}%' OR remark_ LIKE '%{}%' OR content_ LIKE '%{}%'",
            kw_esc, kw_esc, kw_esc
        )
    };
    let sql = format!(
        "SELECT user_name_, type_, timestamp_, content_, is_sender_, scene_, remark_ \
         FROM FMessageTable{} ORDER BY timestamp_ DESC LIMIT ?1 OFFSET ?2",
        where_sql
    );
    let items = rows_to_json(&conn, &sql, &[&limit, &offset]);
    let total = total(&conn, "FMessageTable");
    Ok(serde_json::json!({ "items": items, "total": total }))
}
