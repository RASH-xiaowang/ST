// ============================================================
// 待回复任务应答 — 查询待回复队列（ilink 渠道路径 + 本机微信监控路径
// + QQ 官方机器人路径）
// 发送成功后标记 replied，失败标记 error，避免死循环重试。
//
// 本机路径说明：本机监控产生的任务 channel 为空字符串。其回复同样
// 经绑定的 ilink 微信账号发出（私聊 wxid 可直接发送，发送层会自动
// 补全 @im.wechat）；群聊任务暂不自动发送（ilink 群回复需要 context_token
// 与 @ 语义），保留 to_reply 状态供人工处理。
//
// QQ 路径说明：channel='qqbot' 的任务由网关事件入库，full_json 内
// qq_reply_to 记录回复目标（"private:openid" / "group:group_openid"），
// local_id 记录官方事件 id（被动回复窗口内优先带 msg_id 回复）。
// ============================================================

use rusqlite::{params, Connection};

/// 待回复条目
#[derive(Debug)]
pub struct PendingReply {
    pub task_id: i64,
    /// 发送账号：ilink 任务取自 full_json；本机任务为 0（由应答器选默认微信账号）
    pub account_id: i64,
    pub peer: String,
    pub reply_text: String,
    pub is_group: bool,
    pub channel: String,
    /// qqbot：回复目标（"private:openid" / "group:group_openid"）
    pub qq_reply_to: String,
    /// qqbot：官方事件 id（被动回复 msg_id）
    pub qq_reply_msg_id: String,
}

/// 查询待回复任务（ilink + 本机 + qqbot 三条路径），按入队顺序返回
pub fn list_pending_reply(conn: &Connection, limit: i64) -> Result<Vec<PendingReply>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, full_json, username, reply_text, is_group, channel
             FROM task_wechat_info
             WHERE status='to_reply' AND reply_text != ''
               AND (channel='ilink' OR channel='' OR channel='qqbot')
             ORDER BY id ASC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        let (id, full_json, peer, reply_text, is_group, channel) = r.map_err(|e| e.to_string())?;
        if peer.is_empty() || reply_text.is_empty() {
            continue;
        }
        let (account_id, qq_reply_to, qq_reply_msg_id) = if channel == "ilink" || channel == "qqbot"
        {
            let parsed = serde_json::from_str::<serde_json::Value>(&full_json).ok();
            let account_id = parsed
                .as_ref()
                .and_then(|v| v.get("account_id").and_then(|x| x.as_i64()))
                .unwrap_or(0);
            let reply_to = parsed
                .as_ref()
                .and_then(|v| v.get("qq_reply_to").and_then(|x| x.as_str()))
                .unwrap_or("")
                .to_string();
            let reply_msg_id = parsed
                .as_ref()
                .and_then(|v| v.get("local_id").and_then(|x| x.as_str()))
                .unwrap_or("")
                .to_string();
            (account_id, reply_to, reply_msg_id)
        } else {
            (0, String::new(), String::new())
        };
        if (channel == "ilink" || channel == "qqbot") && account_id == 0 {
            continue; // 缺少账号上下文，跳过
        }
        out.push(PendingReply {
            task_id: id,
            account_id,
            peer,
            reply_text,
            is_group: is_group != 0,
            channel,
            qq_reply_to,
            qq_reply_msg_id,
        });
    }
    Ok(out)
}

/// 发送成功：标记 replied，ack_id 记录消息 ID
pub fn mark_replied(conn: &Connection, task_id: i64, msg_id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE task_wechat_info SET status='replied', ack_id=?1, error='',
         updated_at=datetime('now','localtime') WHERE id=?2",
        params![msg_id, task_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 发送失败：标记 error 并记录原因
pub fn mark_reply_failed(conn: &Connection, task_id: i64, error: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE task_wechat_info SET status='error', error=?1,
         updated_at=datetime('now','localtime') WHERE id=?2",
        params![error, task_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
