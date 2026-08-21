// ============================================================
// 账号归档导出（迁移自 WeChatDataAnalysis 的导出归档 ZIP）
// 将解密后的数据库与本地资源目录打包为 ZIP，便于备份/迁移。
// ============================================================

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::wechat::handlers::helpers;

fn walk_files(root: &Path, include_resources: bool, skip_dirs: &[&str]) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if skip_dirs.contains(&name.as_str()) {
                    continue;
                }
                stack.push(path);
            } else if path.is_file() {
                let is_db = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| {
                        let e = e.to_lowercase();
                        e == "db" || e == "sqlite" || e == "sqlite3" || e == "db3"
                    })
                    .unwrap_or(false)
                    || name == "_source.json"
                    || name == "_media_keys.json";
                if is_db || include_resources {
                    let rel = path
                        .strip_prefix(root)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| name.clone());
                    out.push((path, rel));
                }
            }
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// 将解密目录打包为 ZIP。
///
/// * `output_dir` — 指定保存目录（前端选择）；缺省为 `<st_result>/exports`
/// * `include_resources` — 是否包含资源文件；false 时仅打包 .db 数据库与元信息文件
pub fn export_archive(
    app: &tauri::AppHandle,
    decrypted_dir: &Path,
    output_dir: Option<String>,
    include_resources: bool,
) -> Result<serde_json::Value, String> {
    if !decrypted_dir.is_dir() {
        return Err("解密目录不存在，请先完成数据库解密".to_string());
    }

    let out_dir = match output_dir {
        Some(d) if !d.trim().is_empty() => PathBuf::from(d.trim()),
        _ => crate::wechat::config::default_st_result_dir().join("exports"),
    };
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("创建导出目录失败: {}", e))?;

    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = format!("wechat_archive_{}.zip", stamp);
    let zip_path = out_dir.join(&filename);
    let tmp_path = zip_path.with_extension("zip.tmp");

    let skip_dirs = ["exports", "monitor_cache"];
    let files = walk_files(decrypted_dir, include_resources, &skip_dirs);
    if files.is_empty() {
        return Err("没有可导出的文件".to_string());
    }
    let total_bytes: u64 = files
        .iter()
        .map(|(p, _)| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .sum();

    helpers::emit_op_progress(
        app,
        "archive",
        1,
        files.len() as u64,
        &format!(
            "准备打包 {} 个文件（{} MB）",
            files.len(),
            total_bytes / 1024 / 1024
        ),
    );

    let file = std::fs::File::create(&tmp_path).map_err(|e| format!("创建归档文件失败: {}", e))?;
    let mut zw = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);

    let mut done: u64 = 0;
    for (path, rel) in &files {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                log::warn!("[archive] 跳过 {}: {}", path.display(), e);
                continue;
            }
        };
        zw.start_file(rel.clone(), opts)
            .map_err(|e| format!("写入 {} 失败: {}", rel, e))?;
        zw.write_all(&data)
            .map_err(|e| format!("写入 {} 失败: {}", rel, e))?;
        done += 1;
        if done.is_multiple_of(25) || done == files.len() as u64 {
            helpers::emit_op_progress(
                app,
                "archive",
                done,
                files.len() as u64,
                &format!("已打包 {}/{} 个文件", done, files.len()),
            );
        }
    }
    let mut writer = zw
        .finish()
        .map_err(|e| format!("完成 ZIP 归档失败: {}", e))?;
    writer.flush().map_err(|e| format!("写入归档失败: {}", e))?;
    drop(writer);

    if zip_path.exists() {
        let _ = std::fs::remove_file(&zip_path);
    }
    std::fs::rename(&tmp_path, &zip_path).map_err(|e| format!("移动归档文件失败: {}", e))?;
    helpers::emit_op_progress(
        app,
        "archive",
        files.len() as u64,
        files.len() as u64,
        "归档完成",
    );

    Ok(serde_json::json!({
        "path": zip_path.to_string_lossy().to_string(),
        "filename": filename,
        "file_count": files.len(),
        "total_bytes": total_bytes,
    }))
}
