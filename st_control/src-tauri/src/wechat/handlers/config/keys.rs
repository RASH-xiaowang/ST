// ============================================================
// 微信 IPC — 密钥校验域
// 依赖：helpers（scan_db_files）/ crypto / keys（完全限定）
// ============================================================

use crate::wechat::handlers::helpers;

// ─── 密钥校验 ───

#[tauri::command]
pub async fn verify_database_key(
    db_path: String,
    enc_key_hex: String,
) -> Result<serde_json::Value, String> {
    use crate::wechat::crypto;
    let path = std::path::Path::new(&db_path);
    if !path.exists() {
        return Err(format!("数据库文件不存在: {}", db_path));
    }
    let page1 = std::fs::read(path).map_err(|e| format!("读取数据库失败: {}", e))?;
    if page1.len() < crypto::PAGE_SZ {
        return Err(format!("文件太小 ({})，不是有效的数据库文件", page1.len()));
    }
    let page1 = &page1[..crypto::PAGE_SZ];
    let wx_key_bin = match hex::decode(enc_key_hex.trim()) {
        Ok(b) if b.len() == crypto::KEY_SZ => b,
        _ => {
            return Ok(
                serde_json::json!({ "valid": false, "format": null, "aes_ok": false, "hmac_ok": false }),
            )
        }
    };
    let (hmac_ok, aes_ok) = crypto::verify_key(&wx_key_bin, page1);
    let valid = hmac_ok && aes_ok;
    log::debug!(
        "[verify_db] hmac={} aes={} valid={}",
        hmac_ok,
        aes_ok,
        valid
    );
    Ok(
        serde_json::json!({ "valid": valid, "format": if valid { serde_json::Value::String("wx_key_v4.1".into()) } else { serde_json::Value::Null }, "aes_ok": aes_ok, "hmac_ok": hmac_ok, "size": 32 }),
    )
}

#[tauri::command]
pub async fn generate_keys_file(
    app: tauri::AppHandle,
    db_dir: String,
    keys_file: String,
    enc_key_hex: String,
    key_format: Option<String>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        generate_keys_file_impl(app, db_dir, keys_file, enc_key_hex, key_format)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 生成 all_keys.json 的实际实现（供「全自动获取」流程复用）
pub(crate) fn generate_keys_file_impl(
    app: tauri::AppHandle,
    db_dir: String,
    keys_file: String,
    enc_key_hex: String,
    key_format: Option<String>,
) -> Result<serde_json::Value, String> {
    use crate::wechat::crypto;
    use std::collections::HashMap;
    use std::io::Write;
    use std::path::Path;
    let wx_key_bin =
        hex::decode(enc_key_hex.trim()).map_err(|e| format!("密钥 hex 解码失败: {}", e))?;
    if wx_key_bin.len() != crypto::KEY_SZ {
        return Err(format!(
            "密钥长度必须是 32 字节（64 个 hex 字符），当前 {} 字符",
            enc_key_hex.trim().len()
        ));
    }
    let dir = Path::new(&db_dir);
    if !dir.is_dir() {
        return Err(format!("目录不存在: {}", db_dir));
    }
    let db_files = helpers::scan_db_files(dir);
    if db_files.is_empty() {
        return Err(format!("{} 下未找到任何 .db 文件", db_dir));
    }

    // 并行验证：每个库的 PBKDF2 派生（256k 轮）是固定 ~3s CPU 开销，
    // 串行 N 个库 = N×3s。并行后 ≈ 3s + IO。
    let total = db_files.len() as u32;
    let entries = std::sync::Mutex::new(HashMap::new());
    let errors = std::sync::Mutex::new(Vec::new());
    let valid = std::sync::atomic::AtomicU32::new(0);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let processed = std::sync::atomic::AtomicU64::new(0);
    let key_hex = enc_key_hex.trim().to_string();
    let parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8)
        .min(db_files.len().max(1));

    std::thread::scope(|scope| {
        for _ in 0..parallelism {
            let entries = &entries;
            let errors = &errors;
            let valid = &valid;
            let next = &next;
            let processed = &processed;
            let db_files = &db_files;
            let key_hex = &key_hex;
            let app = app.clone();
            scope.spawn(move || loop {
                let idx = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if idx >= db_files.len() {
                    break;
                }
                let rel_path = &db_files[idx];
                match verify_one_db(dir, rel_path, key_hex) {
                    Ok((norm, entry)) => {
                        valid.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        entries.lock().unwrap().insert(norm, entry);
                    }
                    Err(e) => errors.lock().unwrap().push(format!("{}: {}", rel_path, e)),
                }
                let done = processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                super::emit_op_progress(
                    &app,
                    "verify_db",
                    done,
                    total as u64,
                    &format!("正在校验 {}/{} 个数据库…", done, total),
                );
            });
        }
    });
    super::emit_op_progress(&app, "verify_db", total as u64, total as u64, "校验完成");

    let mut entries = entries.into_inner().unwrap();
    let errors = errors.into_inner().unwrap();
    let valid = valid.load(std::sync::atomic::Ordering::Relaxed);
    entries.insert(
        "_key_format".to_string(),
        serde_json::Value::String(key_format.unwrap_or_else(|| "wx_key_v4.1".to_string())),
    );
    entries.insert(
        "_db_dir".to_string(),
        serde_json::Value::String(db_dir.clone()),
    );
    let keys_path = std::path::Path::new(&keys_file);
    if let Some(parent) = keys_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let json =
        serde_json::to_string_pretty(&entries).map_err(|e| format!("JSON 序列化失败: {}", e))?;
    let mut file =
        std::fs::File::create(keys_path).map_err(|e| format!("创建密钥文件失败: {}", e))?;
    file.write_all(json.as_bytes())
        .map_err(|e| format!("写入密钥文件失败: {}", e))?;
    log::info!(
        "已生成 all_keys.json: {} 个数据库, {} 个验证通过",
        total,
        valid
    );
    Ok(
        serde_json::json!({ "total": total, "valid": valid, "errors": errors, "keys_file": keys_file }),
    )
}

/// 验证单个数据库的密钥并构造密钥条目。
///
/// PBKDF2-HMAC-SHA512 256k 轮派生是 CPU 密集操作（每库约 3s），
/// 由调用方并行调度。
fn verify_one_db(
    dir: &std::path::Path,
    rel_path: &str,
    key_hex: &str,
) -> Result<(String, serde_json::Value), String> {
    use crate::wechat::crypto;

    let full_path = dir.join(rel_path);
    let mut page1_buf = vec![0u8; crypto::PAGE_SZ];
    {
        use std::io::Read;
        let mut f = std::fs::File::open(&full_path).map_err(|e| format!("打开失败: {}", e))?;
        f.read_exact(&mut page1_buf)
            .map_err(|e| format!("读取失败: {}", e))?;
    }
    let page1 = &page1_buf;
    let salt = &page1[..crypto::SALT_SZ];
    let file_size = std::fs::metadata(&full_path).map(|m| m.len()).unwrap_or(0);
    let wx_key_bin = match hex::decode(key_hex.trim()) {
        Ok(b) if b.len() == crypto::KEY_SZ => b,
        _ => return Err("密钥长度错误".to_string()),
    };
    if crypto::derive_and_verify(&wx_key_bin, page1).is_none() {
        return Err("密钥验证未通过".to_string());
    }
    let size_mb = (file_size as f64) / (1024.0 * 1024.0);
    let normalized = rel_path.replace('\\', "/");
    let mut entry = serde_json::Map::new();
    entry.insert(
        "enc_key".to_string(),
        serde_json::Value::String(key_hex.trim().to_string()),
    );
    entry.insert(
        "salt".to_string(),
        serde_json::Value::String(hex::encode(salt)),
    );
    entry.insert(
        "size_mb".to_string(),
        serde_json::json!((size_mb * 100.0).round() / 100.0),
    );
    Ok((normalized, serde_json::Value::Object(entry)))
}

#[tauri::command]
pub async fn decrypt_all_databases(
    app: tauri::AppHandle,
    keys_file: String,
    db_dir: String,
    decrypted_dir: String,
) -> Result<serde_json::Value, String> {
    use crate::wechat::keys::Keys;
    use std::path::Path;
    let keys_path = Path::new(&keys_file);
    if !keys_path.exists() {
        return Err("密钥文件不存在，请先执行「校验数据库密钥」".to_string());
    }
    let keys = Keys::from_file(keys_path).map_err(|e| format!("读取密钥文件失败: {}", e))?;
    let src_base = Path::new(&db_dir);
    let out_base = Path::new(&decrypted_dir);
    std::fs::create_dir_all(out_base).map_err(|e| format!("创建输出目录失败: {}", e))?;

    // 并行解密：每个库的密钥派生（PBKDF2 256k 轮）是固定 ~3s 开销，
    // 串行 18 个库约 70s。使用动态工作队列 + 按 CPU 核数自适应并发，
    // 大库小库负载均衡，总耗时 ≈ 最长任务 + 队列分摊，显著提速。
    // 各库写入各自的临时文件 + 原子替换，互不冲突。
    let entries: Vec<(String, String)> = keys
        .entries
        .iter()
        .map(|(k, v)| (k.clone(), v.enc_key.clone()))
        .collect();
    // 大库优先派发：让 message_0 / message_fts 等大任务尽早被 worker 领取并行
    let mut sized: Vec<(u64, usize)> = entries
        .iter()
        .enumerate()
        .map(|(i, (p, _))| {
            let sz = std::fs::metadata(src_base.join(p))
                .map(|m| m.len())
                .unwrap_or(0);
            (sz, i)
        })
        .collect();
    sized.sort_by_key(|a| std::cmp::Reverse(a.0));
    let order: Vec<usize> = sized.iter().map(|(_, i)| *i).collect();
    let tasks: Vec<(String, String)> = order.iter().map(|i| entries[*i].clone()).collect();
    let total = entries.len() as u32;
    let decrypted = std::sync::atomic::AtomicU32::new(0);
    let processed = std::sync::atomic::AtomicU64::new(0);
    // 并发度：CPU 核数自适应，上限 6（PBKDF2 是 CPU 密集，超过核数收益递减；
    // 同时避免过多线程争抢大库磁盘 IO）
    let parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let worker_count = parallelism.min(6).min(entries.len().max(1));

    use std::sync::mpsc;
    let (result_tx, result_rx) = mpsc::channel::<(String, Result<u32, String>)>();
    let next_task = std::sync::atomic::AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let tx = result_tx.clone();
            let decrypted = &decrypted;
            let next = &next_task;
            let tasks = &tasks;
            let processed = &processed;
            let app = app.clone();
            scope.spawn(move || {
                loop {
                    // 原子取下一个任务：真正的并行负载均衡（无锁阻塞）
                    let idx = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if idx >= tasks.len() {
                        break;
                    }
                    let (rel_path, enc_key) = &tasks[idx];
                    let r = decrypt_one_db(src_base, out_base, rel_path, enc_key);
                    if r.is_ok() {
                        decrypted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    if tx.send((rel_path.clone(), r)).is_err() {
                        break;
                    }
                    let done = processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    super::emit_op_progress(
                        &app,
                        "decrypt_all",
                        done,
                        total as u64,
                        &format!("正在解密 {}/{} 个数据库…", done, total),
                    );
                }
            });
        }
    });
    super::emit_op_progress(&app, "decrypt_all", total as u64, total as u64, "解密完成");
    drop(result_tx);

    let mut msgs: Vec<_> = result_rx.iter().collect();
    // 结果按路径排序，错误列表稳定可读
    msgs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut errors = Vec::new();
    for (rel_path, r) in msgs {
        match r {
            Ok(pages) => log::info!("解密成功: {} ({} 页)", rel_path, pages),
            Err(e) => errors.push(format!("{}: {}", rel_path, e)),
        }
    }
    let ok = decrypted.load(std::sync::atomic::Ordering::Relaxed);
    log::info!("批量解密完成: {}/{} 成功", ok, total);
    Ok(serde_json::json!({ "total": total, "decrypted": ok, "wal_patched": 0, "errors": errors }))
}

/// 解密单个数据库：密钥派生 → 临时文件全量解密 → WAL patch → 健康校验 → 原子替换。
fn decrypt_one_db(
    src_base: &std::path::Path,
    out_base: &std::path::Path,
    rel_path: &str,
    enc_key_hex: &str,
) -> Result<u32, String> {
    use crate::wechat::crypto::{self, decrypt_wal, full_decrypt};

    let wx_key_bin = hex::decode(enc_key_hex).map_err(|e| format!("密钥 hex 解码失败: {}", e))?;
    if wx_key_bin.len() != crypto::KEY_SZ {
        return Err("密钥长度错误".to_string());
    }
    let src = src_base.join(rel_path);
    let out = out_base.join(rel_path);
    if !src.exists() {
        return Err("源文件不存在".to_string());
    }
    let page1 = {
        use std::io::Read;
        let mut f = std::fs::File::open(&src).map_err(|e| format!("打开失败: {}", e))?;
        let mut buf = vec![0u8; crypto::PAGE_SZ];
        f.read_exact(&mut buf)
            .map_err(|e| format!("读取失败: {}", e))?;
        buf
    };
    let derived_key = crypto::derive_and_verify(&wx_key_bin, &page1)
        .ok_or_else(|| "密钥验证未通过".to_string())?;

    let mut wal_src = src_base.join(rel_path);
    wal_src.set_extension("db-wal");
    let temp = out.with_extension("db.decrypt_tmp");
    let pages = full_decrypt(&src, &temp, &derived_key).map_err(|e| format!("解密失败: {}", e))?;
    if wal_src.exists() {
        if let Err(e) = decrypt_wal(&wal_src, &temp, &derived_key) {
            log::warn!("WAL 解密失败 {}: {}", rel_path, e);
        }
    }
    // 健康校验：源被写入中断时解密结果无效，丢弃 temp 下轮重试
    if !crate::wechat::db_cache::sqlite_healthy(&temp) {
        let _ = std::fs::remove_file(&temp);
        return Err("解密结果无效（源库可能正在被写入）".to_string());
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_extension("db-wal"));
    let _ = std::fs::remove_file(out.with_extension("db-shm"));
    std::fs::rename(&temp, &out).map_err(|e| format!("替换解密文件失败: {}", e))?;
    Ok(pages)
}
