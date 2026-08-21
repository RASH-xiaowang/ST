// ============================================================
// 微信 IPC — 图片解密域
// 依赖：image / auto_key / keys（完全限定），零顶层导入
// ============================================================

// ─── 图片解密 ───

#[tauri::command]
pub async fn verify_image_key(
    app: tauri::AppHandle,
    db_dir: String,
    aes_key_hex: String,
    xor_key_str: String,
) -> Result<serde_json::Value, String> {
    use crate::wechat::image;
    let db_base = std::path::Path::new(&db_dir)
        .parent()
        .ok_or_else(|| "无法从数据库目录解析微信根路径".to_string())?;
    let image_roots = [
        db_base.join("msg").join("attach"),
        db_base.join("msg").join("image"),
    ];
    let mut candidates = Vec::new();
    for root in &image_roots {
        if !root.is_dir() {
            continue;
        }
        let mut dirs_to_scan = vec![root.clone()];
        while let Some(dir) = dirs_to_scan.pop() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        dirs_to_scan.push(p);
                    } else if p.extension().is_some_and(|ext| ext == "dat") {
                        candidates.push(p);
                    }
                }
            }
        }
    }
    let mut best_map: std::collections::HashMap<String, std::path::PathBuf> =
        std::collections::HashMap::new();
    for p in &candidates {
        let stem = match p.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => continue,
        };
        let base = stem
            .strip_suffix("_h")
            .or_else(|| stem.strip_suffix("_t"))
            .unwrap_or(stem)
            .to_string();
        let priority = if stem.ends_with("_h") {
            2
        } else if stem.ends_with("_t") {
            0
        } else {
            1
        };
        if let Some(existing) = best_map.get(&base) {
            let es = existing.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let ep = if es.ends_with("_h") {
                2
            } else if es.ends_with("_t") {
                0
            } else {
                1
            };
            if priority > ep {
                best_map.insert(base, p.clone());
            }
        } else {
            best_map.insert(base, p.clone());
        }
    }
    candidates = best_map.into_values().collect();
    if candidates.is_empty() {
        return Err("未找到任何 .dat 缓存图片文件".to_string());
    }
    let aes_key = if aes_key_hex.trim().len() == 16 {
        Some(aes_key_hex.trim().as_bytes().to_vec())
    } else {
        None
    };
    let xor_key = xor_key_str.parse::<u8>().unwrap_or(0x00);
    let tested = candidates.len().min(10);
    super::emit_op_progress(
        &app,
        "verify_img",
        0,
        tested as u64,
        &format!("已扫描到 {} 个图片缓存，开始测试…", candidates.len()),
    );
    let mut success = false;
    let mut detected_fmt = String::new();
    for (i, f) in candidates[..tested].iter().enumerate() {
        super::emit_op_progress(
            &app,
            "verify_img",
            (i + 1) as u64,
            tested as u64,
            &format!("正在测试图片密钥 {}/{}…", i + 1, tested),
        );
        match image::decrypt_dat_file(f, None, aes_key.as_deref(), xor_key) {
            Ok((out, fmt)) => {
                success = true;
                detected_fmt = fmt.to_string();
                let _ = std::fs::remove_file(&out);
                break;
            }
            Err(_) => continue,
        }
    }
    super::emit_op_progress(&app, "verify_img", tested as u64, tested as u64, "校验完成");
    Ok(
        serde_json::json!({ "valid": success, "format": detected_fmt, "total_cached": candidates.len(), "test_attempts": tested }),
    )
}

#[tauri::command]
pub async fn decode_all_images(
    app: tauri::AppHandle,
    db_dir: String,
    output_dir: String,
    aes_key_hex: String,
    xor_key_str: String,
) -> Result<serde_json::Value, String> {
    use crate::wechat::image;
    let db_base = std::path::Path::new(&db_dir)
        .parent()
        .ok_or_else(|| "无法从数据库目录解析微信根路径".to_string())?;
    let image_roots = [
        db_base.join("msg").join("attach"),
        db_base.join("msg").join("image"),
    ];
    let mut dat_files = Vec::new();
    for root in &image_roots {
        if !root.is_dir() {
            continue;
        }
        let mut dirs_to_scan = vec![root.clone()];
        while let Some(dir) = dirs_to_scan.pop() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        dirs_to_scan.push(p);
                    } else if p.extension().is_some_and(|ext| ext == "dat") {
                        dat_files.push(p);
                    }
                }
            }
        }
    }
    if dat_files.is_empty() {
        return Err("未找到任何 .dat 缓存图片文件".to_string());
    }
    let out_base = std::path::Path::new(&output_dir);
    std::fs::create_dir_all(out_base).map_err(|e| format!("创建输出目录失败: {}", e))?;
    let aes_key = if aes_key_hex.trim().len() == 16 {
        Some(aes_key_hex.trim().as_bytes().to_vec())
    } else {
        None
    };
    let xor_key = xor_key_str.parse::<u8>().unwrap_or(0x00);
    let mut best_map: std::collections::HashMap<String, std::path::PathBuf> =
        std::collections::HashMap::new();
    for p in &dat_files {
        let stem = match p.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => continue,
        };
        let base = stem
            .strip_suffix("_h")
            .or_else(|| stem.strip_suffix("_t"))
            .unwrap_or(stem)
            .to_string();
        let priority = if stem.ends_with("_h") {
            2
        } else if stem.ends_with("_t") {
            0
        } else {
            1
        };
        match best_map.get(&base) {
            Some(existing) => {
                let es = existing.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let ep = if es.ends_with("_h") {
                    2
                } else if es.ends_with("_t") {
                    0
                } else {
                    1
                };
                if priority > ep {
                    best_map.insert(base, p.clone());
                }
            }
            None => {
                best_map.insert(base, p.clone());
            }
        }
    }
    dat_files = best_map.into_values().collect();
    let total = dat_files.len() as u32;
    let mut decoded = 0u32;
    let mut errors: Vec<String> = Vec::new();
    super::emit_op_progress(
        &app,
        "decode_img",
        0,
        total as u64,
        &format!("准备解码 {} 个图片…", total),
    );
    // 进度推送频率：每 ~1%（至少每 50 个）一次，避免大量文件时事件风暴
    let step = (total as u64 / 100).max(50).max(1);
    for (i, dat_path) in dat_files.iter().enumerate() {
        match image::decrypt_dat_file(dat_path, Some(out_base), aes_key.as_deref(), xor_key) {
            Ok(_) => {
                decoded += 1;
            }
            Err(e) => {
                if errors.len() < 100 {
                    errors.push(format!("{}: {}", dat_path.display(), e));
                }
            }
        }
        let done = (i + 1) as u64;
        if done.is_multiple_of(step) || done == total as u64 {
            super::emit_op_progress(
                &app,
                "decode_img",
                done,
                total as u64,
                &format!("正在解码 {}/{} 个图片…", done, total),
            );
        }
    }
    super::emit_op_progress(&app, "decode_img", total as u64, total as u64, "解码完成");
    Ok(
        serde_json::json!({ "total": total, "decoded": decoded, "errors": errors, "output_dir": output_dir }),
    )
}
