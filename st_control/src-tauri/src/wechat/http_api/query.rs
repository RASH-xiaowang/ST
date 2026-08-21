// ============================================================
// HTTP API — 数据查询（会话/消息/通讯录/群成员）
// 自 http_api.rs 拆分：健康检查 + 只读数据查询接口。
// ============================================================

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::wechat::config::WeChatConfig;
use crate::wechat::modules::{common, contacts, messages, sessions};

use super::{
    cache_key, check_auth, load_cfg, parse_i64, parse_time, parse_usize, ApiError, ApiResult,
    ApiServerState,
};

// ============ 1. 健康检查 ============

pub(crate) async fn health(State(state): State<Arc<ApiServerState>>) -> Json<serde_json::Value> {
    let monitor_running = state.monitor.is_running();
    let db_ready = WeChatConfig::load()
        .map(|c| c.decrypted_dir.join("session").join("session.db").exists())
        .unwrap_or(false);
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "uptimeSeconds": state.started.elapsed().as_secs(),
        "port": state.current_port(),
        "enabled": state.is_enabled(),
        "auth": state.current_token().is_some(),
        "monitor": { "running": monitor_running },
        "database": { "ready": db_ready },
    }))
}

// ============ 2. 会话列表 ============

pub(crate) async fn get_sessions(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
    body: Option<Json<serde_json::Value>>,
) -> ApiResult<Json<serde_json::Value>> {
    check_auth(&state, &headers, &q, body.as_ref().map(|b| &b.0))?;
    let body_map: HashMap<String, String> = body
        .as_ref()
        .and_then(|b| serde_json::from_value(b.0.clone()).ok())
        .unwrap_or_default();
    let mut params = body_map;
    for (k, v) in &q {
        params.insert(k.clone(), v.clone());
    }
    let key = cache_key("/api/v1/sessions", &params);
    if let Some(resp) = state.cached(&key, Duration::from_secs(2)) {
        return Ok(resp);
    }
    let get = |k: &str| params.get(k).cloned();

    let keyword = get("keyword").unwrap_or_default().to_lowercase();
    let limit = get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100usize)
        .min(1000);
    let offset = get("offset").and_then(|v| v.parse().ok()).unwrap_or(0usize);

    let cfg = load_cfg()?;
    let list = tokio::task::spawn_blocking(move || sessions::get_session_list(&cfg.decrypted_dir))
        .await
        .map_err(|e| ApiError::internal(format!("查询任务失败: {}", e)))?
        .map_err(ApiError::internal)?;

    let mut out = Vec::new();
    for s in list {
        if !keyword.is_empty()
            && !s.username.to_lowercase().contains(&keyword)
            && !s.name.to_lowercase().contains(&keyword)
        {
            continue;
        }
        out.push(serde_json::json!({
            "username": s.username,
            "displayName": s.name,
            "type": if s.username.ends_with("@chatroom") { "group" } else { "private" },
            "lastTimestamp": s.ts,
            "summary": s.summary,
            "unreadCount": s.unread_count,
            "draft": s.draft,
        }));
    }
    let total = out.len();
    let page: Vec<_> = out.into_iter().skip(offset).take(limit).collect();
    let resp = Json(serde_json::json!({
        "success": true,
        "total": total,
        "count": page.len(),
        "offset": offset,
        "hasMore": offset + page.len() < total,
        "sessions": page,
    }));
    state.store(&key, resp.0.clone());
    Ok(resp)
}

// ============ 3. 会话消息 ============

pub(crate) async fn get_messages(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
    body: Option<Json<serde_json::Value>>,
) -> ApiResult<Json<serde_json::Value>> {
    check_auth(&state, &headers, &q, body.as_ref().map(|b| &b.0))?;
    let body_map: HashMap<String, String> = body
        .as_ref()
        .and_then(|b| serde_json::from_value(b.0.clone()).ok())
        .unwrap_or_default();
    // query 优先，body 兜底
    let mut params = body_map;
    for (k, v) in &q {
        params.insert(k.clone(), v.clone());
    }

    let key = cache_key("/api/v1/messages", &params);
    if let Some(resp) = state.cached(&key, Duration::from_secs(1)) {
        return Ok(resp);
    }
    let talker = params
        .get("talker")
        .cloned()
        .ok_or_else(|| ApiError::bad_request("缺少必填参数 talker"))?;
    let limit = parse_usize(&params, "limit", 100, 1000);
    let cursor = parse_i64(&params, "cursor", parse_i64(&params, "before_sort_seq", 0));
    let keyword = params
        .get("keyword")
        .cloned()
        .unwrap_or_default()
        .to_lowercase();
    let start = parse_time(&params, "start");
    let end = parse_time(&params, "end");

    let cfg = load_cfg()?;
    let dir = cfg.decrypted_dir.clone();
    let tk = talker.clone();
    let self_name = cfg.wxid().unwrap_or_default();
    let cursor_opt = if cursor > 0 { Some(cursor) } else { None };
    let page = tokio::task::spawn_blocking(move || {
        messages::get_conversation_messages(&dir, &tk, &self_name, cursor_opt, limit)
    })
    .await
    .map_err(|e| ApiError::internal(format!("查询任务失败: {}", e)))?
    .map_err(|e| {
        if e.contains("不存在") || e.contains("未找到") {
            ApiError::not_found(e)
        } else {
            ApiError::internal(e)
        }
    })?;

    let base_url = format!("/api/v1/media/{}/", talker);
    let mut out = Vec::new();
    for m in page.messages {
        if let Some(s) = start {
            if m.ts < s {
                continue;
            }
        }
        if let Some(e) = end {
            if m.ts > e {
                continue;
            }
        }
        if !keyword.is_empty() && !m.text.to_lowercase().contains(&keyword) {
            continue;
        }
        out.push(map_message(&m, &base_url));
    }

    let resp = Json(serde_json::json!({
        "success": true,
        "talker": talker,
        "chatName": page.chat_name,
        "total": page.total,
        "count": out.len(),
        "hasMore": page.has_more,
        "nextCursor": if page.has_more { serde_json::json!(page.next_cursor) } else { serde_json::Value::Null },
        "messages": out,
    }));
    state.store(&key, resp.0.clone());
    Ok(resp)
}

pub(crate) fn map_message(m: &messages::ChatMessage, media_base: &str) -> serde_json::Value {
    let media_url = if m.msg_type == 3 {
        Some(format!("{}{}", media_base, m.local_id))
    } else {
        m.image_url.clone()
    };
    serde_json::json!({
        "localId": m.local_id,
        "serverId": m.server_id,
        "sortSeq": m.sort_seq,
        "createTime": m.ts,
        "time": m.time,
        "isSend": if m.is_self { 1 } else { 0 },
        "type": m.msg_type,
        "typeLabel": m.type_label,
        "isNotice": m.is_notice,
        "senderUsername": m.sender_username,
        "senderName": m.sender_name,
        "content": m.text,
        "rich": m.rich,
        "mediaUrl": media_url,
    })
}

// ============ 3.1 会话消息（ChatLab Pull 兼容 + 增量拉取）============

pub(crate) async fn get_session_messages(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<serde_json::Value>> {
    check_auth(&state, &headers, &q, None)?;
    let key = cache_key(&format!("/api/v1/sessions/{}/messages", id), &q);
    if let Some(resp) = state.cached(&key, Duration::from_secs(1)) {
        return Ok(resp);
    }
    let since = parse_time(&q, "since").unwrap_or(0);
    let end = parse_time(&q, "end").unwrap_or(i64::MAX);
    let limit = parse_usize(&q, "limit", 500, 5000);

    let cfg = load_cfg()?;
    let dir = cfg.decrypted_dir.clone();
    let tk = id.clone();
    let self_name = cfg.wxid().unwrap_or_default();
    let collected = tokio::task::spawn_blocking(
        move || -> Result<(Vec<messages::ChatMessage>, bool, String), String> {
            let mut all: Vec<messages::ChatMessage> = Vec::new();
            let mut cursor: Option<i64> = None;
            let mut chat_name = String::new();
            // 最多翻 20 页，避免异常会话导致死循环
            for _ in 0..20 {
                let page = messages::get_conversation_messages(&dir, &tk, &self_name, cursor, 500)?;
                chat_name = page.chat_name.clone();
                let oldest_ts = page.messages.first().map(|m| m.ts).unwrap_or(i64::MAX);
                for m in page.messages.into_iter() {
                    if m.ts > since && m.ts <= end {
                        all.push(m);
                    }
                }
                if all.len() >= limit || !page.has_more {
                    return Ok((all, page.has_more && oldest_ts > since, chat_name));
                }
                // 本页最老消息已早于 since，停止
                if oldest_ts <= since {
                    return Ok((all, false, chat_name));
                }
                cursor = Some(page.next_cursor);
            }
            Ok((all, true, chat_name))
        },
    )
    .await
    .map_err(|e| ApiError::internal(format!("查询任务失败: {}", e)))?
    .map_err(|e| {
        if e.contains("不存在") || e.contains("未找到") {
            ApiError::not_found(e)
        } else {
            ApiError::internal(e)
        }
    })?;

    let (msgs, has_more, chat_name) = collected;
    let limited: Vec<_> = msgs.into_iter().take(limit).collect();
    let watermark = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let next_cursor = limited.last().map(|m| m.sort_seq);
    let base_url = format!("/api/v1/media/{}/", id);
    let items: Vec<_> = limited.iter().map(|m| map_message(m, &base_url)).collect();

    let resp = Json(serde_json::json!({
        "success": true,
        "chatlab": { "version": "0.0.2", "generator": "st_control" },
        "meta": {
            "id": id,
            "name": chat_name,
            "platform": "wechat",
            "type": if id.ends_with("@chatroom") { "group" } else { "private" },
        },
        "count": items.len(),
        "messages": items,
        "sync": {
            "hasMore": has_more,
            "nextCursor": next_cursor,
            "watermark": watermark,
        },
    }));
    state.store(&key, resp.0.clone());
    Ok(resp)
}

// ============ 4. 联系人列表 ============

pub(crate) async fn get_contacts(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
    body: Option<Json<serde_json::Value>>,
) -> ApiResult<Json<serde_json::Value>> {
    check_auth(&state, &headers, &q, body.as_ref().map(|b| &b.0))?;
    let body_map: HashMap<String, String> = body
        .as_ref()
        .and_then(|b| serde_json::from_value(b.0.clone()).ok())
        .unwrap_or_default();
    let mut params = body_map;
    for (k, v) in &q {
        params.insert(k.clone(), v.clone());
    }
    let key = cache_key("/api/v1/contacts", &params);
    if let Some(resp) = state.cached(&key, Duration::from_secs(5)) {
        return Ok(resp);
    }
    let get = |k: &str| params.get(k).cloned();

    let keyword = get("keyword").unwrap_or_default().to_lowercase();
    let category = get("category").unwrap_or_default();
    let limit = get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100usize)
        .min(5000);
    let offset = get("offset").and_then(|v| v.parse().ok()).unwrap_or(0usize);

    let cfg = load_cfg()?;
    let book = tokio::task::spawn_blocking(move || contacts::get_contacts(&cfg.decrypted_dir))
        .await
        .map_err(|e| ApiError::internal(format!("查询任务失败: {}", e)))?
        .map_err(ApiError::internal)?;

    let mut out = Vec::new();
    for c in book.contacts {
        if !category.is_empty() && c.category != category {
            continue;
        }
        if !keyword.is_empty() {
            let hit = c.username.to_lowercase().contains(&keyword)
                || c.nick_name.to_lowercase().contains(&keyword)
                || c.remark.to_lowercase().contains(&keyword)
                || c.display_name.to_lowercase().contains(&keyword)
                || c.alias.to_lowercase().contains(&keyword);
            if !hit {
                continue;
            }
        }
        out.push(serde_json::to_value(&c).unwrap_or_default());
    }
    let total = out.len();
    let page: Vec<_> = out.into_iter().skip(offset).take(limit).collect();
    let resp = Json(serde_json::json!({
        "success": true,
        "total": total,
        "count": page.len(),
        "offset": offset,
        "hasMore": offset + page.len() < total,
        "contacts": page,
    }));
    state.store(&key, resp.0.clone());
    Ok(resp)
}

// ============ 5. 群成员列表 ============

pub(crate) async fn get_group_members(
    State(state): State<Arc<ApiServerState>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
    body: Option<Json<serde_json::Value>>,
) -> ApiResult<Json<serde_json::Value>> {
    check_auth(&state, &headers, &q, body.as_ref().map(|b| &b.0))?;
    let body_map: HashMap<String, String> = body
        .as_ref()
        .and_then(|b| serde_json::from_value(b.0.clone()).ok())
        .unwrap_or_default();
    let chatroom_id = q
        .get("chatroomId")
        .or_else(|| q.get("talker"))
        .or_else(|| body_map.get("chatroomId"))
        .or_else(|| body_map.get("talker"))
        .cloned()
        .ok_or_else(|| ApiError::bad_request("缺少必填参数 chatroomId"))?;

    let mut params = body_map;
    for (k, v) in &q {
        params.insert(k.clone(), v.clone());
    }
    let key = cache_key(&format!("/api/v1/group-members/{}", chatroom_id), &params);
    if let Some(resp) = state.cached(&key, Duration::from_secs(2)) {
        return Ok(resp);
    }

    let cfg = load_cfg()?;
    let dir = cfg.decrypted_dir.clone();
    let room = chatroom_id.clone();
    let members = tokio::task::spawn_blocking(move || query_group_members(&dir, &room))
        .await
        .map_err(|e| ApiError::internal(format!("查询任务失败: {}", e)))??;

    let resp = Json(serde_json::json!({
        "success": true,
        "chatroomId": chatroom_id,
        "count": members.len(),
        "members": members,
    }));
    state.store(&key, resp.0.clone());
    Ok(resp)
}

/// 群成员查询（对 chat_room / chatroom_member 做运行时表结构适配）
pub(crate) fn query_group_members(
    decrypted_dir: &std::path::Path,
    chatroom_id: &str,
) -> ApiResult<Vec<serde_json::Value>> {
    let db_path = decrypted_dir.join("contact").join("contact.db");
    if !db_path.exists() {
        return Err(ApiError::not_found(format!(
            "联系人数据库未解密: {}",
            db_path.display()
        )));
    }
    let conn = common::open_readonly_db(&db_path)
        .map_err(|e| ApiError::internal(format!("打开失败: {}", e)))?;
    if !common::table_exists(&conn, "chat_room") || !common::table_exists(&conn, "chatroom_member")
    {
        return Err(ApiError::not_found(
            "群成员表不存在（chat_room/chatroom_member）",
        ));
    }

    // 1. 定位 room_id：chat_room 中寻找用户名匹配的列
    let room_cols = common::table_columns(&conn, "chat_room");
    let name_col = ["username", "user_name", "chatroom_id", "chatroom", "name"]
        .iter()
        .find(|c| room_cols.iter().any(|x| x == **c))
        .map(|s| s.to_string())
        .ok_or_else(|| ApiError::internal("chat_room 表缺少用户名列"))?;
    let room_id: i64 = conn
        .query_row(
            &format!("SELECT id FROM chat_room WHERE \"{}\" = ?1", name_col),
            [chatroom_id],
            |r| r.get(0),
        )
        .map_err(|_| ApiError::not_found(format!("群聊不存在: {}", chatroom_id)))?;

    // 群主
    let owner: Option<String> = conn
        .query_row(
            "SELECT owner FROM chat_room WHERE id = ?1",
            [room_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();

    // 2. chatroom_member → contact 关联列适配
    let m_cols = common::table_columns(&conn, "chatroom_member");
    let link_col = ["contact_id", "member_id", "contactid", "user_id", "userid"]
        .iter()
        .find(|c| m_cols.iter().any(|x| x == **c))
        .map(|s| s.to_string())
        .ok_or_else(|| ApiError::internal("chatroom_member 表缺少联系人关联列"))?;

    // 3. 群昵称列（可选）
    let nick_col = ["display_name", "nick_name", "nickname", "group_nick_name"]
        .iter()
        .find(|c| m_cols.iter().any(|x| x == **c))
        .map(|s| s.to_string());

    let sql = format!(
        "SELECT c.username, c.nick_name, c.remark, c.alias, c.big_head_url{} \
         FROM chatroom_member m JOIN contact c ON c.id = m.\"{}\" \
         WHERE m.room_id = ?1",
        nick_col
            .as_ref()
            .map(|c| format!(", m.\"{}\"", c))
            .unwrap_or_default(),
        link_col
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| ApiError::internal(format!("查询失败: {}", e)))?;
    let rows = stmt
        .query_map([room_id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                if nick_col.is_some() {
                    row.get::<_, Option<String>>(5)?
                } else {
                    None
                },
            ))
        })
        .map_err(|e| ApiError::internal(format!("读取失败: {}", e)))?;

    let mut members = Vec::new();
    for r in rows.flatten() {
        let (username, nick, remark, alias, avatar, group_nick) = r;
        let uname = username.unwrap_or_default();
        let display = remark
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| group_nick.clone().filter(|s| !s.is_empty()))
            .or_else(|| nick.clone().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| uname.clone());
        members.push(serde_json::json!({
            "wxid": uname,
            "displayName": display,
            "nickname": nick.unwrap_or_default(),
            "remark": remark.unwrap_or_default(),
            "alias": alias.unwrap_or_default(),
            "groupNickname": group_nick.unwrap_or_default(),
            "avatarUrl": avatar.unwrap_or_default(),
            "isOwner": owner.as_deref() == Some(uname.as_str()),
        }));
    }
    Ok(members)
}
