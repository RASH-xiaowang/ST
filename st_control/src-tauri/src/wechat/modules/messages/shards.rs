// ============================================================
// 聊天消息 — 分库管理与索引缓存
// 自 messages.rs 拆分：分库类型、只读连接、会话分库索引。
// ============================================================

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use rusqlite::Connection;

use crate::wechat::modules::common;

/// 一个可查询的消息分库
pub(crate) struct MsgShard {
    pub(crate) path: PathBuf,
    pub(crate) conn: Connection,
    /// real_sender_id → username
    pub(crate) name2id: HashMap<i64, String>,
    /// 本库中该会话的消息数
    pub(crate) count: i64,
}

/// 分库元数据（不含连接，可跨查询复用；含文件签名用于失效判断）
pub(crate) struct ShardMeta {
    path: PathBuf,
    count: i64,
    name2id: HashMap<i64, String>,
}

/// 某会话的分库索引（整个缓存条目原子替换）
pub(crate) struct ShardIndexEntry {
    table: String,
    files: Vec<PathBuf>,
    sigs: Vec<Option<(SystemTime, u64)>>,
    metas: Vec<ShardMeta>,
}

/// 分库索引缓存 key（分库路径 + 会话用户名）
pub(crate) type ShardCacheKey = (PathBuf, String);

pub(crate) static SHARD_INDEX_CACHE: OnceLock<Mutex<HashMap<ShardCacheKey, Arc<ShardIndexEntry>>>> =
    OnceLock::new();

/// 最多缓存多少个不同会话的分库索引（LRU 兜底：超出后整体清空，代价为下次重建）
pub(crate) const SHARD_INDEX_MAX_ENTRIES: usize = 64;

/// 从元数据打开只读连接（复用缓存时跳过 COUNT / Name2Id 等重活）
pub(crate) fn open_shard_from_meta(meta: &ShardMeta) -> Option<MsgShard> {
    let conn = common::open_readonly_db(&meta.path).ok()?;
    Some(MsgShard {
        path: meta.path.clone(),
        conn,
        name2id: meta.name2id.clone(),
        count: meta.count,
    })
}

/// 加载某库的 Name2Id 映射
pub(crate) fn load_name2id(conn: &Connection) -> HashMap<i64, String> {
    let mut map = HashMap::new();
    if !common::table_exists(conn, "Name2Id") {
        return map;
    }
    if let Ok(mut stmt) = conn.prepare("SELECT rowid, user_name FROM Name2Id") {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        }) {
            for r in rows.flatten() {
                map.insert(r.0, r.1);
            }
        }
    }
    map
}

/// 收集所有包含该会话消息的分库
pub(crate) fn open_shards(decrypted_dir: &Path, username: &str) -> Vec<MsgShard> {
    let table = common::msg_table_name(username);
    let mut dbs = common::find_db_files(decrypted_dir, "message_");
    dbs.extend(common::find_db_files(decrypted_dir, "biz_message_"));
    dbs.sort();
    dbs.dedup();

    // 排除 monitor_cache 目录（那是实时监控的缓存副本，不是独立数据源）
    dbs.retain(|p| !p.to_string_lossy().contains("monitor_cache"));
    // 只保留真正的消息分片库 message_<数字>.db / biz_message_<数字>.db，
    // 排除 message_fts.db / message_resource.db 等辅助库（永远不含 Msg_ 表，
    // 扫进去只会产生"找不到表"的噪音日志并浪费大库打开开销）
    dbs.retain(|p| common::is_message_shard_file(p));

    // 文件签名：任一分库被监控/手动刷新重写（mtime/长度变化）即整表失效重建
    let sigs: Vec<Option<(SystemTime, u64)>> = dbs.iter().map(|p| common::file_sig(p)).collect();

    let cache = SHARD_INDEX_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (decrypted_dir.to_path_buf(), username.to_string());
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());

    // 命中：文件列表与签名完全一致 → 复用元数据，跳过所有 DB 打开 / COUNT / Name2Id
    let reuse = guard
        .get(&key)
        .filter(|e| e.table == table && e.files == dbs && e.sigs == sigs)
        .cloned();
    if let Some(entry) = reuse {
        // 先释放全局锁再打开连接，避免不同会话的并发翻页相互阻塞
        drop(guard);
        log::debug!(
            "[msg_shard] 缓存命中 {} 分库 {} 个 (username={})",
            table,
            entry.metas.len(),
            username
        );
        return entry
            .metas
            .iter()
            .filter_map(open_shard_from_meta)
            .collect();
    }

    log::info!(
        "[msg_shard] 重建分库索引: {} 个候选文件 (username={}, table={})",
        dbs.len(),
        username,
        table
    );

    let mut metas = Vec::new();
    for path in &dbs {
        let conn = match common::open_readonly_db(path) {
            Ok(c) => c,
            Err(_) => {
                log::warn!("[msg_shard] 无法打开数据库: {}", path.display());
                continue;
            }
        };
        if !common::table_exists(&conn, &table) {
            let tbl_count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let has_msg: i64 = conn
                .query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name LIKE 'Msg\\_%' ESCAPE '\\'", [], |r| r.get(0))
                .unwrap_or(0);
            let fname = path.file_name().unwrap_or_default().to_string_lossy();
            log::info!(
                "[msg_shard] 跳过 {}：目标表 {} 不存在（该库共 {} 表，Msg_ 前缀 {} 张）",
                fname,
                table,
                tbl_count,
                has_msg,
            );
            // 整库 0 表 = 解密副本损坏或写入半成品（db_cache 正在重写 / 历史残留）。
            // 删除副本让监控或下次查询经 db_cache 全量重建，避免"0 个分库命中"
            // 导致会话消息永久显示为空。
            if tbl_count == 0 {
                log::warn!(
                    "[msg_shard] {} 副本无任何表（损坏/解密半成品），删除等待重建",
                    path.display()
                );
                drop(conn);
                let _ = std::fs::remove_file(path);
                let _ = std::fs::remove_file(path.with_extension("db-wal"));
                let _ = std::fs::remove_file(path.with_extension("db-shm"));
                continue;
            }
            // message_0.db 有 45690 页但 0 张 Msg_ 表 → 密钥可能不正确
            // （HMAC 校验通过但解密内容为脏数据）。不自动删除（删了不重建更糟），
            // 请用户检查 all_keys.json 中 message/message_0.db 的密钥，手动重新解密。
            if has_msg > 0 {
                let names: Vec<String> = conn
                    .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'Msg\\_%' ESCAPE '\\' LIMIT 5")
                    .unwrap()
                    .query_map([], |r| r.get::<_, String>(0))
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
                log::info!("  [msg_shard]   Msg_ 表示例: {}", names.join(", "));
            }
            continue;
        }
        let count: i64 = conn
            .query_row(&format!("SELECT count(*) FROM \"{}\"", table), [], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        if count == 0 {
            log::debug!("[msg_shard] 跳过(空表): {}", path.display());
            continue;
        }
        let name2id = load_name2id(&conn);
        log::info!(
            "[msg_shard] 命中: {} (table={}, rows={})",
            path.display(),
            table,
            count
        );
        metas.push(ShardMeta {
            path: path.clone(),
            count,
            name2id,
        });
    }

    let entry = Arc::new(ShardIndexEntry {
        table,
        files: dbs,
        sigs,
        metas,
    });
    // 缓存条目数上限兜底：清空后下次调用重建（低频、可接受）
    if guard.len() >= SHARD_INDEX_MAX_ENTRIES {
        guard.clear();
    }
    guard.insert(key, entry.clone());
    log::info!(
        "[msg_shard] 总计 {} 个分库命中会话={}",
        entry.metas.len(),
        username
    );
    entry
        .metas
        .iter()
        .filter_map(open_shard_from_meta)
        .collect()
}
