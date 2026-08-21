// ============================================================
// DB 解密缓存层 — 文件一致性域
// 自 db_cache.rs 拆分：健康校验/快照暂存/清理/原子替换。
// ============================================================

use std::path::Path;
use std::time::Duration;

/// 超过该大小（128MB）的源库改用「单复制 + mtime/size 稳定校验」，
/// 避免双复制对大库造成 GB 级 I/O；小库仍用双复制逐字节比对。
const STAGE_DOUBLE_COPY_LIMIT: u64 = 128 * 1024 * 1024;

/// 校验解密副本是否为健康 SQLite：文件头可解析且 sqlite_master 可读。
///
/// 微信源库在持续写入时，一次性读取可能拿到不一致快照，解密结果虽然
/// 以 "SQLite format 3" 开头但内部页损坏（表现为 0 表）。此检查在发布
/// 前拦截这类坏副本。
pub(crate) fn sqlite_healthy(path: &Path) -> bool {
    let conn = match rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return false,
    };
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .is_ok()
}

/// 暂存单个源文件的一致性快照（大小分级）：
/// - ≤128MB：双复制 + 逐字节比对（与 monitor 的 stage_stable_copy 一致）
/// - >128MB：单复制 + 源 mtime/size 复制前后一致校验（微信 checkpoint 会
///   > 更新文件 mtime，能捕获绝大多数写入窗口）
pub(crate) fn stage_one(src: &Path, dst: &Path) -> std::io::Result<()> {
    let size = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    if size <= STAGE_DOUBLE_COPY_LIMIT {
        return crate::wechat::monitor::stage_stable_copy(src, dst);
    }
    let sig = |p: &Path| crate::wechat::modules::common::file_sig(p);
    for attempt in 0..3u32 {
        let before = sig(src);
        let _ = std::fs::remove_file(dst);
        std::fs::copy(src, dst)?;
        let after = sig(src);
        if before == after {
            return Ok(());
        }
        if attempt < 2 {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::WouldBlock,
        "源库写入中，快照不稳定",
    ))
}

/// 暂存主库 + WAL 快照（任一不稳定即整体失败，调用方重试）
pub(crate) fn stage_source_snapshot(
    db_path: &Path,
    wal_path: &Path,
    staging_db: &Path,
    staging_wal: &Path,
) -> std::io::Result<()> {
    stage_one(db_path, staging_db)?;
    let _ = std::fs::remove_file(staging_wal);
    if wal_path.exists() {
        stage_one(wal_path, staging_wal)?;
    }
    Ok(())
}

/// 清理 db_cache 暂存文件
pub(crate) fn cleanup_db_staging(paths: &[&Path]) {
    for p in paths {
        let _ = std::fs::remove_file(p);
        let _ = std::fs::remove_file(p.with_extension("stage_a"));
        let _ = std::fs::remove_file(p.with_extension("stage_b"));
    }
}

/// 原子替换解密副本：同目录先删旧文件再 rename。
/// 失败时旧副本要么不存在（下次重建），要么保持原样，不会出现半成品。
pub(crate) fn replace_decrypted(temp: &Path, out: &Path) -> std::io::Result<()> {
    // 同时清掉旧副本及其 WAL/SHM 残留（可写打开遗留，会导致 SQLite 误判）
    let _ = std::fs::remove_file(out.with_extension("db-wal"));
    let _ = std::fs::remove_file(out.with_extension("db-shm"));
    // 旧副本可能被短生命周期查询连接短暂占用（Windows 无 FILE_SHARE_DELETE）：
    // 删除+重命名做几次短暂重试，避免因一次竞争失败导致整轮解密被丢弃
    for attempt in 0..5u32 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        let _ = std::fs::remove_file(out);
        if std::fs::rename(temp, out).is_ok() {
            return Ok(());
        }
        if !temp.exists() {
            break; // temp 已被成功移动
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        format!("替换解密副本失败（文件被占用）: {}", out.display()),
    ))
}
