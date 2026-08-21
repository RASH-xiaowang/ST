// ============================================================
// 微信实时消息监听 — 基础设施辅助层
// 自 monitor.rs 拆分：消息类型工具、数据库连接、联系人/DB 映射、
// 一致性快照暂存。
// ============================================================

use crate::wechat::db_cache::MonitorDBCache;
use std::collections::HashMap;
use std::time::SystemTime;

// ============ 消息类型工具 ============

const MEDIA_TYPE_MAP: [(i32, &str); 6] = [
    (3, "image"),
    (34, "voice"),
    (43, "video"),
    (47, "emoji"),
    (49, "file"),
    (48, "location"),
];

pub(crate) fn media_type(t: i32) -> Option<&'static str> {
    MEDIA_TYPE_MAP
        .iter()
        .find(|&&(k, _)| k == t)
        .map(|&(_, v)| v)
}

pub(crate) fn format_msg_type(t: i32) -> &'static str {
    match t {
        1 => "文本",
        3 => "图片",
        34 => "语音",
        42 => "名片",
        43 => "视频",
        47 => "表情",
        48 => "位置",
        49 => "链接/文件",
        50 => "通话",
        10000 => "系统",
        10002 => "撤回",
        _ => "未知",
    }
}

// ============ 数据库连接 ============

/// 连接解密后的 SQLite 数据库
pub(crate) fn connect_db(path: &std::path::Path) -> Result<rusqlite::Connection, rusqlite::Error> {
    // 必须只读打开：解密副本是 WAL 格式（header 2/2），可写打开会创建
    // -wal/-shm 并占用文件句柄，导致 db_cache 原子替换（remove+rename）
    // 失败或残留旧 -shm 干扰，副本更新不了 → 消息实时链路中断。
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.execute_batch("PRAGMA query_only = ON; PRAGMA temp_store = MEMORY;")?;
    Ok(conn)
}

/// 读取消息库的 Name2Id 映射（rowid → username）
pub(crate) fn load_name2id(conn: &rusqlite::Connection) -> HashMap<i64, String> {
    let mut map = HashMap::new();
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

// ============ 联系人 / DB 映射 ============

/// 从已解密的 contact.db 加载联系人名 i字典
pub fn load_contact_names(db_path: &std::path::Path) -> HashMap<String, String> {
    let mut names = HashMap::new();
    let conn = match connect_db(db_path) {
        Ok(c) => c,
        Err(_) => return names,
    };
    if let Ok(mut stmt) = conn.prepare("SELECT username, nick_name, remark FROM contact") {
        if let Ok(rows) = stmt.query_map([], |row| {
            let username: String = row.get(0)?;
            let nick: Option<String> = row.get(1)?;
            let remark: Option<String> = row.get(2)?;
            Ok((username, nick, remark))
        }) {
            for r in rows.flatten() {
                let display = r.2.or(r.1).unwrap_or_else(|| r.0.clone());
                names.insert(r.0, display);
            }
        }
    }
    names
}

/// 构建 username → [db_keys] 映射
pub fn build_username_db_map(
    db_cache: &MonitorDBCache,
    db_dir: &std::path::Path,
) -> HashMap<String, Vec<String>> {
    let mut mapping: HashMap<String, Vec<String>> = HashMap::new();

    // 扫描所有 message_<数字>.db：微信 4.x 的聊天数据会按哈希分片到多个库，
    // 历史较多时不止 message_0..4（可能出现 message_5.db、message_6.db …）。
    // 之前固定只扫描 0..5，导致这些库中的群聊/私聊在实时监控中查不到消息，
    // 只能回退到会话摘要，表现为“群聊消息显示不全 / 只显示一部分消息”。
    // 这里改为枚举磁盘上所有 message_<数字>.db，确保覆盖全部分片库。
    let msg_files = crate::wechat::modules::common::find_db_files(db_dir, "message_");
    for path in msg_files {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        // 仅处理 message_<数字>.db，排除其它以 message_ 开头的中间文件（如有）
        let idx_part = match file_name.strip_prefix("message_") {
            Some(s) => s.strip_suffix(".db").unwrap_or(s),
            None => continue,
        };
        if idx_part.parse::<u32>().is_err() {
            continue;
        }
        let rel_key = format!("message/{}", file_name);
        let dec_path = match db_cache.get(&rel_key) {
            Ok(Some(p)) => p,
            _ => continue,
        };
        if !dec_path.exists() {
            continue;
        }
        let conn = match connect_db(&dec_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Ok(mut stmt) = conn.prepare("SELECT user_name FROM Name2Id") {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                for r in rows.flatten() {
                    mapping.entry(r).or_default().push(rel_key.clone());
                }
            }
        };
    }

    // 按 message DB 的 mtime 降序排列（最新的在前）
    for usernames in mapping.values_mut() {
        usernames.sort_by(|a, b| {
            let ma = db_mtime(db_dir, a).unwrap_or(0);
            let mb = db_mtime(db_dir, b).unwrap_or(0);
            mb.cmp(&ma)
        });
    }

    mapping
}

pub(crate) fn db_mtime(db_dir: &std::path::Path, rel_key: &str) -> Option<u64> {
    let path = db_dir.join(
        rel_key
            .replace('\\', "/")
            .replace('/', std::path::MAIN_SEPARATOR_STR),
    );
    let meta = std::fs::metadata(&path).ok()?;
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// 文件 mtime（毫秒），文件不存在时返回 0。
/// 仅做元数据查询，开销极小，适合每轮询周期调用。
pub(crate) fn file_mtime_ms(path: &std::path::Path) -> u64 {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ============ 一致性快照暂存 ============

/// 逐字节比较两个文件是否完全一致
pub(crate) fn files_equal(a: &std::path::Path, b: &std::path::Path) -> bool {
    use std::io::Read;
    let (Ok(m1), Ok(m2)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    if m1.len() != m2.len() {
        return false;
    }
    let (Ok(mut fa), Ok(mut fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else {
        return false;
    };
    let mut buf_a = [0u8; 65536];
    let mut buf_b = [0u8; 65536];
    loop {
        let (Ok(na), Ok(nb)) = (fa.read(&mut buf_a), fb.read(&mut buf_b)) else {
            return false;
        };
        if na != nb || buf_a[..na] != buf_b[..nb] {
            return false;
        }
        if na == 0 {
            return true;
        }
    }
}

/// 双复制暂存一致性快照：连续复制两次并逐字节比对。
///
/// 微信（SQLCipher + WAL）持续写入时，单次 `fs::copy` 可能拿到撕裂页
/// （checkpoint 原地改写主库页、WAL 追加写），解密后 SQLite 头虽在但
/// 内部页损坏。两次独立复制若不一致说明写入窗口内，短暂重试即可拿到
/// 一致快照。成功返回后 `dst` 即稳定副本。
pub(crate) fn stage_stable_copy(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> std::io::Result<()> {
    let a = dst.with_extension("stage_a");
    let b = dst.with_extension("stage_b");
    for _ in 0..2 {
        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);
        std::fs::copy(src, &a)?;
        std::fs::copy(src, &b)?;
        if files_equal(&a, &b) {
            let _ = std::fs::remove_file(dst);
            std::fs::rename(&a, dst)?;
            let _ = std::fs::remove_file(&b);
            return Ok(());
        }
    }
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
    Err(std::io::Error::new(
        std::io::ErrorKind::WouldBlock,
        "源库写入中，快照不稳定",
    ))
}

/// 暂存主库 + WAL 快照；任一不稳定即整体失败（调用方重试）
pub(crate) fn stage_full_snapshot(
    db: &std::path::Path,
    staging_db: &std::path::Path,
    staging_wal: &std::path::Path,
) -> std::io::Result<()> {
    stage_stable_copy(db, staging_db)?;
    let wal = db.with_extension("db-wal");
    let _ = std::fs::remove_file(staging_wal);
    if wal.exists() {
        stage_stable_copy(&wal, staging_wal)?;
    }
    Ok(())
}

/// 清理暂存文件
pub(crate) fn cleanup_staging(paths: &[&std::path::Path]) {
    for p in paths {
        let _ = std::fs::remove_file(p);
        let _ = std::fs::remove_file(p.with_extension("stage_a"));
        let _ = std::fs::remove_file(p.with_extension("stage_b"));
    }
}
