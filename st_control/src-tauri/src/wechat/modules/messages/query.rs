// ============================================================
// 聊天消息 — 查询编排与转账状态
// 自 messages.rs 拆分：游标分库查询、转账状态映射、
// 会话消息分页聚合。
// ============================================================

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::parse::parse_display_content;
use super::shards::{open_shards, MsgShard};
use super::types::{ChatMessage, MessagePage};
use crate::wechat::modules::common;
use crate::wechat::modules::contacts;

struct RawRow<'a> {
    local_id: i64,
    server_id: i64,
    sort_seq: i64,
    create_time: i64,
    local_type: i64,
    real_sender_id: i64,
    content: Option<Vec<u8>>,
    name2id: &'a HashMap<i64, String>,
}

/// 单笔转账状态映射值（最早 sort, 最新 sort, 最新 paysubtype）
type TransferStatus = (i64, i64, String);
/// 转账状态缓存条目（分库签名列表 + 状态映射）
struct TransferStatusEntry {
    sigs: Vec<Option<common::DirSig>>,
    map: HashMap<String, TransferStatus>,
}
/// 转账缓存 key（分库路径 + 会话用户名）
type TransferCacheKey = (PathBuf, String);

/// 从单个分库中查询一批消息行（游标模式）。
///
/// * `cursor` — 此前已加载的最小 sort_seq（不含），None 表示取最新
/// * `limit` — 每库最多取多少行
fn query_shard_rows<'a>(
    shard: &'a MsgShard,
    table: &str,
    cursor: Option<i64>,
    limit: i64,
) -> Vec<RawRow<'a>> {
    let cols = common::table_columns(&shard.conn, table);
    let has = |c: &str| cols.iter().any(|x| x == c);
    let sel = |c: &str, dft: &str| {
        if has(c) {
            format!("\"{}\"", c)
        } else {
            dft.to_string()
        }
    };
    let order_col = if has("sort_seq") {
        "sort_seq"
    } else {
        "local_id"
    };
    let where_clause = match cursor {
        Some(c) if has("sort_seq") => format!("WHERE \"sort_seq\" < {c}"),
        Some(c) => format!("WHERE \"local_id\" < {c}"),
        None => String::new(),
    };
    let sql = format!(
        "SELECT {lid}, {sid}, {seq}, {ct}, {lt}, {rs}, {mc} \
         FROM \"{table}\" {where_clause} ORDER BY {ord} DESC LIMIT ?1",
        lid = sel("local_id", "0"),
        sid = sel("server_id", "0"),
        seq = sel("sort_seq", "local_id"),
        ct = sel("create_time", "0"),
        lt = sel("local_type", "0"),
        rs = sel("real_sender_id", "0"),
        mc = sel("message_content", "NULL"),
        table = table,
        ord = order_col,
    );

    // 终端日志：打印实际执行的 SQL 和参数（帮助调试消息加载问题）
    log::debug!(
        "[msg_query] cursor={:?} limit={} table={} db={} sql={}",
        cursor,
        limit,
        table,
        shard
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?"),
        sql
    );

    let mut result = Vec::new();
    if let Ok(mut stmt) = shard.conn.prepare(&sql) {
        if let Ok(rows) = stmt.query_map(rusqlite::params![limit], |row| {
            // 全部按 Option 读取：任何字段为 NULL 都不应丢弃整条消息
            // message_content 在压缩时为 BLOB，未压缩时为 TEXT，
            // 直接 Vec<u8> 读取会在 TEXT 行上报错导致整行被丢弃
            Ok::<_, rusqlite::Error>((
                row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                row.get::<_, Option<i64>>(5)?.unwrap_or(0),
                common::get_bytes(row, 6),
            ))
        }) {
            for r in rows.flatten() {
                result.push(RawRow {
                    local_id: r.0,
                    server_id: r.1,
                    sort_seq: r.2,
                    create_time: r.3,
                    local_type: r.4,
                    real_sender_id: r.5,
                    content: r.6,
                    name2id: &shard.name2id,
                });
            }
        }
    }
    log::debug!("[msg_query] 结果 {} 行 (flatten后的实际行数)", result.len());
    result
}

/// 同一笔转账在库中常有两行：发起行（paysubtype=1）+ 状态更新行（paysubtype=3/4）。
///
/// 出现规则（与微信客户端一致）：
/// - 单向出现：同一 transferid 的多行记录属于同一笔转账（发起行 + 收款/退还状态更新行），
///   界面只显示一条 —— 保留最早行（发起方/左右位置），状态取最新一行；
///   状态更新行不单独显示，即使它出现在当前页（转账卡片永远位于发起行的时间位置）。
/// - 双向出现：不同 transferid 是不同笔转账，各自显示；单行记录（未收款或仅存一行）也只显示一条。
/// - 仅存一行且该行是状态更新行（发起行缺失的异常数据）：行内发送者是收款方，
///   气泡方向按发起方取反，文案仍按方向区分（我发出→已被接收，我收到→已收款）。
///
/// 这里构建 transfer_id → (最早 sort, 最新 sort, 最新 paysubtype) 的映射（按分库签名缓存）。
static TRANSFER_STATUS_CACHE: OnceLock<Mutex<HashMap<TransferCacheKey, TransferStatusEntry>>> =
    OnceLock::new();

fn transfer_status_map(
    decrypted_dir: &Path,
    username: &str,
    shards: &[MsgShard],
    table: &str,
) -> HashMap<String, TransferStatus> {
    let key = (decrypted_dir.to_path_buf(), username.to_string());
    let sigs: Vec<Option<common::DirSig>> =
        shards.iter().map(|s| common::file_sig(&s.path)).collect();
    let cache = TRANSFER_STATUS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(entry) = guard.get(&key) {
            if entry.sigs == sigs {
                return entry.map.clone();
            }
        }
    }

    let mut map: HashMap<String, TransferStatus> = HashMap::new();
    for shard in shards {
        let cols = common::table_columns(&shard.conn, table);
        let sort_col = if cols.iter().any(|c| c == "sort_seq") {
            "sort_seq"
        } else {
            "local_id"
        };
        let sql = format!(
            "SELECT {sort}, local_type, message_content, compress_content FROM \"{t}\" WHERE local_type % {mask} = 49",
            sort = sort_col,
            t = table,
            mask = 1i64 << 32
        );
        let Ok(mut stmt) = shard.conn.prepare(&sql) else {
            continue;
        };
        let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                common::get_bytes(r, 2),
                common::get_bytes(r, 3),
            ))
        }) else {
            continue;
        };
        for row in rows.flatten() {
            let bytes = row.2.or(row.3);
            let Some(bytes) = bytes else {
                continue;
            };
            let xml = common::decode_blob_text(&bytes);
            if let Some(crate::wechat::media::RichMedia::Transfer {
                transfer_id,
                paysubtype,
                ..
            }) = crate::wechat::media::parse_rich_content(&xml, 49)
            {
                if transfer_id.is_empty() {
                    continue;
                }
                let entry = map
                    .entry(transfer_id)
                    .or_insert_with(|| (row.0, row.0, paysubtype.clone()));
                if row.0 < entry.0 {
                    entry.0 = row.0;
                }
                if row.0 > entry.1 {
                    entry.1 = row.0;
                    entry.2 = paysubtype.clone();
                }
            }
        }
    }
    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            key,
            TransferStatusEntry {
                sigs,
                map: map.clone(),
            },
        );
    }
    map
}

/// 读取与某会话的聊天记录（游标分页，按时间正序返回）
///
/// * `before_sort_seq` — 游标：此前已加载的最小 sort_seq，传入 None 表示加载最新页
/// * `page_size` — 每页行数（默认 10）
/// * 返回的 messages 按时间升序（PC 聊天窗口从上到下）
pub fn get_conversation_messages(
    decrypted_dir: &Path,
    username: &str,
    self_username: &str,
    before_sort_seq: Option<i64>,
    page_size: usize,
) -> Result<MessagePage, String> {
    log::info!(
        "[msg] get_conversation_messages username={} cursor={:?} page_size={}",
        username,
        before_sort_seq,
        page_size
    );
    let shards = open_shards(decrypted_dir, username);
    if shards.is_empty() {
        return Ok(MessagePage {
            messages: vec![],
            total: 0,
            page: 0,
            page_size,
            has_more: false,
            next_cursor: 0,
            chat_name: username.to_string(),
            self_username: self_username.to_string(),
        });
    }

    let table = common::msg_table_name(username);
    let is_group = username.ends_with("@chatroom") || username.contains("@im.chatroom");
    let page_size = page_size.max(1);

    // 通讯录显示名（每会话只加载一次，shards[0] 路径可代表基础路径）
    let contact_db = decrypted_dir.join("contact").join("contact.db");
    let contact_names = contacts::load_display_names(&contact_db);

    // 游标分页核心：每个分库多读 1 条（page_size+1）探测是否还有更多
    let limit_per_shard = (page_size + 1) as i64;
    let mut merged: Vec<RawRow> = Vec::new();
    for shard in &shards {
        let rows = query_shard_rows(shard, &table, before_sort_seq, limit_per_shard);
        merged.extend(rows);
    }

    // 全局按 sort_seq 降序
    merged.sort_by(|a, b| {
        b.sort_seq
            .cmp(&a.sort_seq)
            .then(b.local_id.cmp(&a.local_id))
    });

    // has_more: 合并后总条数超过 page_size 说明还有更多历史消息
    let has_more = merged.len() > page_size;
    // 取最新 page_size 条（即最早页的末尾）
    let page_rows: Vec<_> = merged.into_iter().take(page_size).collect();
    // 本页中最小的 sort_seq 作为下一页游标
    let next_cursor = page_rows.last().map(|r| r.sort_seq).unwrap_or(0);

    // 统计总数（仅展示用）
    let total: usize = shards.iter().map(|s| s.count as usize).sum();

    let mut messages = Vec::with_capacity(page_rows.len());
    for row in page_rows.into_iter().rev() {
        let msg_type = common::normalize_msg_type(row.local_type);
        let raw_content = row
            .content
            .as_deref()
            .map(common::decode_blob_text)
            .unwrap_or_default();

        let mut sender_username = row
            .name2id
            .get(&row.real_sender_id)
            .cloned()
            .unwrap_or_default();
        let mut sender_from_prefix: Option<String> = None;
        let (text, rich) =
            parse_display_content(msg_type, &raw_content, is_group, &mut sender_from_prefix);
        if sender_username.is_empty() {
            if let Some(p) = &sender_from_prefix {
                sender_username = p.clone();
            }
        }

        // is_self 判断：
        // 1. 优先用 self_username 精确匹配（需与数据库中实际 wxid 一致）
        // 2. 私聊 fallback：发送者不是对方 username 则为自己
        //    但 sender_username 为空时不做 fallback（避免误判）
        let is_self = if !self_username.is_empty() {
            sender_username == self_username
        } else {
            !is_group && !sender_username.is_empty() && sender_username != username
        };

        let sender_name = if is_group {
            contact_names
                .get(&sender_username)
                .cloned()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| sender_username.clone())
        } else {
            String::new()
        };

        let is_notice = msg_type == 10000 || msg_type == 10002;

        let text = if text.is_empty() && rich.is_none() {
            match msg_type {
                3 => "[图片]".to_string(),
                34 => "[语音]".to_string(),
                43 => "[视频]".to_string(),
                47 => "[表情]".to_string(),
                42 => "[名片]".to_string(),
                48 => "[位置]".to_string(),
                _ => text,
            }
        } else {
            text
        };

        messages.push(ChatMessage {
            local_id: row.local_id,
            server_id: row.server_id,
            sort_seq: row.sort_seq,
            ts: row.create_time,
            time: common::format_full_time(row.create_time),
            divider: common::format_msg_divider_time(row.create_time),
            is_self,
            msg_type,
            type_label: common::msg_type_placeholder(row.local_type).to_string(),
            text,
            sender_username,
            sender_name,
            is_notice,
            rich,
            image_url: None,
        });
    }

    // 同一笔转账的「发起行 + 状态更新行」合并为一条：
    // 保留最早行的发送方/位置，用最新行的 paysubtype 决定状态文案与颜色；
    // 状态更新行单向出现（不新增气泡），不同 transferid 各自双向显示。
    let has_transfer = messages.iter().any(|m| {
        m.rich
            .as_ref()
            .and_then(|r| r.get("type"))
            .and_then(|t| t.as_str())
            == Some("transfer")
    });
    if has_transfer {
        let tmap = transfer_status_map(decrypted_dir, username, &shards, &table);
        let mut kept: Vec<ChatMessage> = Vec::with_capacity(messages.len());
        for mut msg in messages {
            let tid = msg
                .rich
                .as_ref()
                .and_then(|r| r.get("transfer_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());
            if let Some(tid) = tid {
                if let Some(&(min_sort, max_sort, ref max_ps)) = tmap.get(&tid) {
                    if msg.sort_seq != min_sort {
                        // 状态更新行不单独显示
                        continue;
                    }
                    // 仅存一行且该行是状态更新行：行内发送者是收款方，
                    // 气泡方向取反，让卡片回到付款方一侧并显示付款方文案。
                    if min_sort == max_sort && crate::wechat::media::is_transfer_status_type(max_ps)
                    {
                        msg.is_self = !msg.is_self;
                    }
                    if let Some(r) = msg.rich.as_mut() {
                        r["paysubtype"] = serde_json::json!(max_ps);
                        r["direction"] = serde_json::json!(
                            crate::wechat::media::transfer_status_label(msg.is_self, max_ps)
                        );
                    }
                }
            }
            kept.push(msg);
        }
        messages = kept;
    }

    log::info!(
        "[msg] 返回 {} 条消息 has_more={} next_cursor={} total={}",
        messages.len(),
        has_more,
        next_cursor,
        total
    );

    Ok(MessagePage {
        messages,
        total,
        page: 0,
        page_size,
        has_more,
        next_cursor,
        chat_name: username.to_string(),
        self_username: self_username.to_string(),
    })
}

/// 会话消息构成统计：各消息类型条数（按数量降序），供聊天头部画像展示。
///
/// 复用 open_shards 的分库索引缓存（含 mtime 失效），每个分库
/// 一条 `GROUP BY local_type`，类型经 normalize_msg_type 归一并
/// 用 msg_type_placeholder 映射中文标签。
pub fn get_session_message_type_stats(
    decrypted_dir: &Path,
    username: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let shards = open_shards(decrypted_dir, username);
    let table = common::msg_table_name(username);
    let mut type_counts: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
    for shard in &shards {
        if !common::table_exists(&shard.conn, &table) {
            continue;
        }
        let sql = format!(
            "SELECT local_type, COUNT(*) FROM \"{}\" GROUP BY local_type",
            table
        );
        if let Ok(mut stmt) = shard.conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                ))
            }) {
                for r in rows.flatten() {
                    *type_counts
                        .entry(common::normalize_msg_type(r.0))
                        .or_insert(0) += r.1;
                }
            }
        }
    }
    let mut list: Vec<(i64, i64)> = type_counts.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(list
        .into_iter()
        .map(|(t, c)| {
            serde_json::json!({
                "type": t,
                "label": common::msg_type_placeholder(t),
                "count": c,
            })
        })
        .collect())
}
