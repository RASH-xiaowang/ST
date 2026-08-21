// ============================================================
// 微信 IPC — 辅助函数（共享于各子模块之间）
// ============================================================

use std::sync::{Mutex, OnceLock};

/// 在阻塞线程池执行 CPU/IO 密集任务，避免占用 tokio 工作线程。
///
/// 前端会并发触发大量数据库查询（会话/消息/通讯录/头像），
/// 若直接在 async handler 内同步执行 SQLite 读取，会耗尽 tokio 工作线程，
/// 导致其它异步任务（WebSocket、监控推送）出现延迟。
pub async fn run_blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("后台任务执行失败: {}", e))?
}

/// 单飞行锁：同一时间只允许一个库刷新解密，避免前端轮询与手动刷新并发写冲突
static REFRESH_DECRYPT_LOCK: Mutex<()> = Mutex::new(());

/// session.db 专用刷新锁：与 monitor 的实时解密/手动刷新互斥，
/// 但不被其它库（resource/sns 等）的慢速解密阻塞，避免拖慢消息推送。
static SESSION_REFRESH_LOCK: Mutex<()> = Mutex::new(());

/// 获取 session.db 专用刷新锁（monitor 实时解密 / 手动刷新共用）
pub(crate) fn session_refresh_lock() -> std::sync::MutexGuard<'static, ()> {
    SESSION_REFRESH_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// 若源库（主库或 WAL）比解密副本新，则重新解密（全量 + WAL patch，原子替换）。
///
/// `rel_key` 为 all_keys.json 中的相对路径（如 `"sns/sns.db"`）。
/// 返回是否发生了重新解密。空闲轮询时仅做两次 stat，开销可忽略。
pub fn refresh_decrypted_db(
    cfg: &crate::wechat::config::WeChatConfig,
    rel_key: &str,
) -> Result<bool, String> {
    use std::io::Read;

    let src = cfg.db_dir.join(rel_key);
    if !src.exists() {
        return Err(format!("源数据库不存在: {}", src.display()));
    }
    let out = cfg.decrypted_dir.join(rel_key);
    let wal = src.with_extension("db-wal");

    let src_db_mtime = crate::wechat::modules::common::file_sig(&src).map(|(t, _)| t);
    let src_wal_mtime = crate::wechat::modules::common::file_sig(&wal).map(|(t, _)| t);
    let src_newest = match (src_db_mtime, src_wal_mtime) {
        (Some(a), Some(b)) => Some(if a > b { a } else { b }),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    let out_mtime = crate::wechat::modules::common::file_sig(&out).map(|(t, _)| t);

    // 副本已是最新：直接跳过（轮询热路径，无锁无 I/O）
    if let (Some(s), Some(o)) = (src_newest, out_mtime) {
        if s <= o {
            return Ok(false);
        }
    }

    let _guard = REFRESH_DECRYPT_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // 等锁期间其它调用可能已刷新，再次校验
    let out_mtime2 = crate::wechat::modules::common::file_sig(&out).map(|(t, _)| t);
    if let (Some(s), Some(o)) = (src_newest, out_mtime2) {
        if s <= o {
            return Ok(false);
        }
    }

    let keys = crate::wechat::keys::Keys::from_file(&cfg.keys_file)
        .map_err(|e| format!("读取密钥文件失败: {}", e))?;
    let key_info = keys
        .get_key_info(rel_key)
        .ok_or_else(|| format!("密钥文件缺少 {}", rel_key))?;

    // 派生加密密钥（兼容 v4.0 / wx_key_v4.1）
    let enc_key = if keys.key_format.as_deref() == Some("wx_key_v4.1") {
        let mut f =
            std::fs::File::open(&src).map_err(|e| format!("打开 {} 失败: {}", src.display(), e))?;
        let mut salt = vec![0u8; crate::wechat::crypto::SALT_SZ];
        f.read_exact(&mut salt)
            .map_err(|e| format!("读取 salt 失败: {}", e))?;
        crate::wechat::crypto::derive_enc_key(
            &hex::decode(&key_info.enc_key).map_err(|e| format!("hex 解码失败: {}", e))?,
            &salt,
            keys.key_format.as_deref(),
        )
    } else {
        hex::decode(&key_info.enc_key).map_err(|e| format!("hex 解码失败: {}", e))?
    };

    // 解密到临时文件（全量 + WAL），再原子替换，避免中途读到损坏文件
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let temp = out.with_extension("db.refresh_temp");
    crate::wechat::crypto::full_decrypt(&src, &temp, &enc_key)
        .map_err(|e| format!("解密 {} 失败: {}", rel_key, e))?;
    if wal.exists() {
        if let Err(e) = crate::wechat::crypto::decrypt_wal(&wal, &temp, &enc_key) {
            log::warn!("[refresh] WAL 增量解密失败 {}: {}", rel_key, e);
        }
    }
    // 健康校验：源库被写入中断时解密结果可能损坏，丢弃临时文件等待下轮
    if !crate::wechat::db_cache::sqlite_healthy(&temp) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("解密结果无效（源库可能正在被写入）: {}", rel_key));
    }
    // 替换主库并清掉旧副本的 -wal/-shm（SQLite 打开副本时可能生成）
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_extension("db-wal"));
    let _ = std::fs::remove_file(out.with_extension("db-shm"));
    std::fs::rename(&temp, &out).map_err(|e| format!("替换解密文件失败: {}", e))?;
    log::info!("[refresh] 已重新解密 {}", rel_key);
    Ok(true)
}

/// 微信消息日志（全局环形缓冲区，最多 500 条）
pub fn wechat_message_log() -> &'static Mutex<Vec<serde_json::Value>> {
    static LOG: OnceLock<Mutex<Vec<serde_json::Value>>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(Vec::with_capacity(500)))
}

/// 推送一条微信消息到日志
pub fn push_wechat_message(msg: serde_json::Value) {
    let mut log = wechat_message_log().lock().unwrap();
    log.push(msg);
    let len = log.len();
    if len > 500 {
        log.drain(0..len - 500);
    }
}

/// 以可写模式打开 SQLite 数据库
pub fn open_writable_db(path: &std::path::Path) -> Result<rusqlite::Connection, String> {
    rusqlite::Connection::open(path)
        .map_err(|e| format!("打开数据库失败 {}: {}", path.display(), e))
}

/// 导出 CSV 字段转义
pub fn csv_cell(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\"").replace(['\r', '\n'], " "))
}

/// 获取导出目录（decrypted 同级的 exports/）
pub fn exports_dir(
    cfg: &crate::wechat::config::WeChatConfig,
) -> Result<std::path::PathBuf, String> {
    let dir = cfg
        .decrypted_dir
        .parent()
        .unwrap_or(&cfg.decrypted_dir)
        .join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建导出目录失败: {}", e))?;
    Ok(dir)
}

/// 写入导出文件（CSV 会自动加 BOM）
pub fn write_export_file(
    dir: &std::path::Path,
    filename: &str,
    content: &str,
    is_csv: bool,
) -> Result<std::path::PathBuf, String> {
    write_export_file_at(&dir.join(filename), content, is_csv)
}

/// 写入导出文件到指定路径（CSV 会自动加 BOM；自动创建父目录）
pub fn write_export_file_at(
    filepath: &std::path::Path,
    content: &str,
    is_csv: bool,
) -> Result<std::path::PathBuf, String> {
    if let Some(parent) = filepath.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let data = if is_csv {
        format!("\u{feff}{}", content)
    } else {
        content.to_string()
    };
    std::fs::write(filepath, data.as_bytes()).map_err(|e| format!("写入文件失败: {}", e))?;
    Ok(filepath.to_path_buf())
}

/// 当前时间戳（Unix 秒）
pub fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

/// 递归扫描目录下所有 .db 文件的相对路径
pub fn scan_db_files(dir: &std::path::Path) -> Vec<String> {
    let mut results = Vec::new();
    scan_dir_recursive(dir, dir, &mut results);
    results
}

fn scan_dir_recursive(base: &std::path::Path, dir: &std::path::Path, results: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_dir_recursive(base, &path, results);
            } else if path.extension().and_then(|e| e.to_str()) == Some("db") {
                if let Ok(rel) = path.strip_prefix(base) {
                    results.push(rel.to_string_lossy().to_string());
                }
            }
        }
    }
}

/// 推送操作进度事件（前端 WeChatConfig 进度条监听 wechat-op-progress）
/// 收敛 config.rs 门面与 archive.rs 的重复实现。
pub(crate) fn emit_op_progress(
    app: &tauri::AppHandle,
    op: &str,
    done: u64,
    total: u64,
    message: &str,
) {
    use tauri::Emitter;
    let percent = if total == 0 {
        0u32
    } else {
        (done as f64 * 100.0 / total as f64).round() as u32
    };
    let _ = app.emit(
        "wechat-op-progress",
        serde_json::json!({
            "op": op,
            "done": done,
            "total": total,
            "percent": percent.min(100),
            "message": message,
        }),
    );
}
