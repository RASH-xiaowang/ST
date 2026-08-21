// ============================================================
// 消息通道 — Tauri IPC 命令（前端界面调用）
// ============================================================

use serde_json::{json, Value};
use std::sync::Arc;
use tauri::State;

use super::manager::{AccountContact, BotManager, BotStatusSummary, QrView};

#[tauri::command]
pub fn bot_list_accounts(state: State<'_, Arc<BotManager>>) -> Result<Vec<Value>, String> {
    Ok(state
        .list_accounts()
        .iter()
        .map(|a| {
            let config_json = if a.platform == "wechat" {
                String::new()
            } else {
                state.channel_config_plain(a).unwrap_or_default()
            };
            json!({
                "id": a.id,
                "botId": a.bot_id,
                "name": a.name,
                "ownerId": a.owner_id,
                "platform": a.platform,
                "targetId": a.target_id,
                "configJson": config_json,
                "baseUrl": a.base_url,
                "cdnBaseUrl": a.cdn_base_url,
                "status": a.status,
                "connectedAt": a.connected_at,
                "expiresAt": a.expires_at,
                "lastActiveAt": a.last_active_at,
                "lastError": a.last_error,
                "createdAt": a.created_at,
            })
        })
        .collect())
}

#[tauri::command]
pub async fn bot_start_qr(
    state: State<'_, Arc<BotManager>>,
    account_id: Option<i64>,
) -> Result<QrView, String> {
    state.start_qr(account_id).await
}

#[tauri::command]
pub async fn bot_poll_qr(
    state: State<'_, Arc<BotManager>>,
    session_id: String,
) -> Result<Value, String> {
    state.poll_qr(&session_id).await
}

#[tauri::command]
pub fn bot_cancel_qr(state: State<'_, Arc<BotManager>>, session_id: String) {
    state.cancel_qr(&session_id);
}

#[tauri::command]
pub fn bot_rename_account(
    state: State<'_, Arc<BotManager>>,
    id: i64,
    name: String,
) -> Result<(), String> {
    state.rename_account(id, name)
}

#[tauri::command]
pub fn bot_unbind_account(state: State<'_, Arc<BotManager>>, id: i64) -> Result<(), String> {
    state.unbind_account(id)
}

#[tauri::command]
pub fn bot_status_summary(state: State<'_, Arc<BotManager>>) -> Result<BotStatusSummary, String> {
    Ok(state.status_summary())
}

#[tauri::command]
pub fn bot_add_channel(
    state: State<'_, Arc<BotManager>>,
    platform: String,
    name: String,
    config: String,
    target_id: String,
) -> Result<i64, String> {
    state.add_channel_account(&platform, name, config, target_id)
}

#[tauri::command]
pub fn bot_update_channel(
    state: State<'_, Arc<BotManager>>,
    id: i64,
    name: String,
    config: String,
    target_id: String,
) -> Result<(), String> {
    state.update_channel_account(id, name, config, target_id)
}

#[tauri::command]
pub async fn bot_test_channel(
    state: State<'_, Arc<BotManager>>,
    account_id: i64,
) -> Result<(), String> {
    state.test_channel(account_id).await
}

#[tauri::command]
pub async fn bot_send_text(
    state: State<'_, Arc<BotManager>>,
    account_id: i64,
    to: String,
    text: String,
) -> Result<String, String> {
    state.send_text(account_id, &to, &text).await
}

#[tauri::command]
pub async fn bot_send_media(
    state: State<'_, Arc<BotManager>>,
    account_id: i64,
    to: String,
    path: String,
) -> Result<String, String> {
    state
        .send_media(account_id, &to, std::path::Path::new(&path))
        .await
}

#[tauri::command]
pub fn bot_list_contacts(
    state: State<'_, Arc<BotManager>>,
    account_id: i64,
) -> Result<Vec<AccountContact>, String> {
    Ok(state.list_contacts(account_id))
}

/// QQ 官方机器人：列出网关自动收集到的 openid 目标（用户/群）
#[tauri::command]
pub fn bot_list_qqbot_contacts(
    state: State<'_, Arc<BotManager>>,
    account_id: i64,
) -> Result<Vec<Value>, String> {
    let conn = state.conn()?;
    let items =
        super::db::list_qqbot_contacts(&conn, account_id, 200).map_err(|e| e.to_string())?;
    Ok(items
        .into_iter()
        .map(|c| {
            json!({
                "id": c.id,
                "kind": c.kind,
                "openid": c.openid,
                "display": c.display,
                "lastContent": c.last_content,
                "lastEventId": c.last_event_id,
                "lastSeenAt": c.last_seen_at,
            })
        })
        .collect())
}

#[tauri::command]
pub fn bot_list_logs(
    state: State<'_, Arc<BotManager>>,
    account_id: i64,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<Value, String> {
    let (items, total) = state.list_logs(account_id, page.unwrap_or(1), page_size.unwrap_or(50))?;
    Ok(json!({ "items": items, "total": total }))
}

#[tauri::command]
pub fn bot_clear_logs(state: State<'_, Arc<BotManager>>, account_id: i64) -> Result<(), String> {
    let conn = state.conn()?;
    conn.execute(
        "DELETE FROM bot_logs WHERE account_id=?1",
        rusqlite::params![account_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
