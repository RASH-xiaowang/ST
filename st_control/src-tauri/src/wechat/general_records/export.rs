// ============================================================
// 微信 general.db 记录查询 — CSV 导出域
// 自 general_records.rs 拆分：整表导出为 CSV 文本。
// ============================================================

use super::{open_general, rows_to_json};

/// 导出记录为 CSV 文本（当前 kind 的全部数据）
pub fn export_records_csv(kind: &str) -> Result<String, String> {
    let conn = open_general().ok_or_else(|| "未找到解密后的 general.db".to_string())?;
    let (sql, cols): (&str, &[&str]) = match kind {
        "revokes" => (
            "SELECT local_id, batch_id, msg_unique_id, session_name, msg_local_id, msg_create_time \
             FROM revokebatchmessage ORDER BY msg_create_time DESC",
            &["local_id", "batch_id", "msg_unique_id", "session_name", "msg_local_id", "msg_create_time"],
        ),
        "transfers" => (
            "SELECT transfer_id, transcation_id, session_name, pay_sub_type, pay_receiver, pay_payer, \
                    begin_transfer_time, last_modified_time \
             FROM transferTable ORDER BY begin_transfer_time DESC",
            &["transfer_id", "transcation_id", "session_name", "pay_sub_type", "pay_receiver", "pay_payer", "begin_transfer_time", "last_modified_time"],
        ),
        "redpackets" => (
            "SELECT message_server_id, session_name, sender_user_name, send_id, hb_status, hb_type, receive_status \
             FROM redEnvelopeTable ORDER BY message_server_id DESC",
            &["message_server_id", "session_name", "sender_user_name", "send_id", "hb_status", "hb_type", "receive_status"],
        ),
        "finder" => (
            "SELECT finder_live_id, finder_username, live_status, replay_status, charge_flag \
             FROM wcfinderlivestatus ORDER BY finder_live_id DESC",
            &["finder_live_id", "finder_username", "live_status", "replay_status", "charge_flag"],
        ),
        "miniprograms" => (
            "SELECT w.user_name, w.type, w.app_id, COALESCE(a.last_update_time, 0) AS last_update_time \
             FROM wacontact w LEFT JOIN WeAppBizAttrSyncBufferTableV02 a ON a.user_name = w.user_name \
             ORDER BY COALESCE(a.last_update_time, 0) DESC",
            &["user_name", "type", "app_id", "last_update_time"],
        ),
        _ => return Err(format!("未知记录类型: {}", kind)),
    };
    let items = rows_to_json(&conn, sql, &[]);
    let mut lines = vec![cols.join(",")];
    for item in &items {
        let row: Vec<String> = cols
            .iter()
            .map(|c| {
                let v = item.get(*c).map(|v| v.to_string()).unwrap_or_default();
                format!("\"{}\"", v.replace('"', "\"\""))
            })
            .collect();
        lines.push(row.join(","));
    }
    Ok(lines.join("\n"))
}
