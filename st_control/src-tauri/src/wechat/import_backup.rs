//! 微信备份导入（对标 WeChatDataAnalysis 的 import_decrypted）
//!
//! 支持两种输入：
//! - 本应用导出的账号归档 ZIP（`wechat_archive_*.zip`，内含 decrypted 目录相对路径）
//! - 已解密的微信备份目录（账号目录或 output 根目录）
//!
//! 导入 = 结构校验 → 复制到本地解密数据区（decrypted_dir），不会修改原备份。

use std::path::{Path, PathBuf};

/// 校验输入是否像微信解密数据（含核心库目录）
fn looks_like_decrypted(root: &Path) -> Result<(), String> {
    let mut found = 0usize;
    for cand in [
        "message/message_0.db",
        "message/message_1.db",
        "contact/contact.db",
        "session/session.db",
        "general/general.db",
        "message/message.db",
        "contact.db",
    ] {
        if root.join(cand).is_file() {
            found += 1;
        }
    }
    if found == 0 {
        return Err(
            "所选目录/压缩包不是有效的微信解密数据（未找到 message/contact/session 等核心库）"
                .to_string(),
        );
    }
    Ok(())
}

fn sanitize_rel(rel: &str) -> Option<PathBuf> {
    let p = Path::new(rel);
    if p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return None;
    }
    Some(p.to_path_buf())
}

/// 从 ZIP 导入
fn import_zip(zip_path: &Path, into: &Path) -> Result<usize, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("打开归档失败: {}", e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("读取 ZIP 失败: {}", e))?;
    let mut imported = 0usize;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("读取 ZIP 条目失败: {}", e))?;
        let name = entry.name().to_string();
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = sanitize_rel(&name) else {
            continue;
        };
        // 跳过临时/索引文件
        let lower = name.to_lowercase();
        if lower.ends_with("-wal") || lower.ends_with("-shm") || lower.contains("decrypt_tmp") {
            continue;
        }
        let target = into.join(&rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
        }
        let mut out =
            std::fs::File::create(&target).map_err(|e| format!("创建 {} 失败: {}", name, e))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("写入 {} 失败: {}", name, e))?;
        imported += 1;
    }
    Ok(imported)
}

/// 从目录导入（递归复制，跳过 -wal/-shm/decrypt_tmp）
fn import_dir(src: &Path, into: &Path) -> Result<usize, String> {
    let mut imported = 0usize;
    let mut stack = vec![src.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).map_err(|e| format!("读取 {} 失败: {}", dir.display(), e))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if name == "exports" || name == "monitor_cache" {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let lower = name.to_lowercase();
            if lower.ends_with("-wal") || lower.ends_with("-shm") || lower.contains("decrypt_tmp") {
                continue;
            }
            let rel = path
                .strip_prefix(src)
                .map_err(|_| "路径计算失败".to_string())?;
            let target = into.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
            }
            std::fs::copy(&path, &target)
                .map_err(|e| format!("复制 {} 失败: {}", path.display(), e))?;
            imported += 1;
        }
    }
    Ok(imported)
}

/// 导入微信备份到本地解密数据区。
/// `source` 为 ZIP 或目录；返回 (导入文件数, 目标目录)。
pub fn import_wechat_backup(source: &Path) -> Result<serde_json::Value, String> {
    if !source.exists() {
        return Err(format!("导入路径不存在: {}", source.display()));
    }
    let cfg =
        crate::wechat::config::WeChatConfig::load().map_err(|e| format!("读取配置失败: {}", e))?;
    let into = cfg.decrypted_dir.clone();
    std::fs::create_dir_all(&into).map_err(|e| format!("创建解密目录失败: {}", e))?;

    let imported = if source.is_file()
        && source
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("zip"))
            .unwrap_or(false)
    {
        // ZIP：先解到临时目录校验结构，再复制
        let tmp = std::env::temp_dir().join(format!("wx_import_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {}", e))?;
        let n = import_zip(source, &tmp)?;
        if n == 0 {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err("ZIP 内没有可导入的文件".to_string());
        }
        looks_like_decrypted(&tmp)?;
        let moved = import_dir(&tmp, &into)?;
        let _ = std::fs::remove_dir_all(&tmp);
        moved
    } else if source.is_dir() {
        looks_like_decrypted(source)?;
        import_dir(source, &into)?
    } else {
        return Err("请选择账号归档 ZIP 或已解密的微信备份目录".to_string());
    };

    log::info!(
        "[import] 导入完成: {} 个文件 → {}",
        imported,
        into.display()
    );
    Ok(serde_json::json!({
        "imported": imported,
        "target": into.to_string_lossy().to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 目录导入：假解密目录 → 复制到目标
    #[test]
    fn test_import_dir_copy() {
        let base = std::env::temp_dir().join("wx_import_test");
        let src = base.join("src");
        let dst = base.join("dst");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(src.join("message")).unwrap();
        std::fs::create_dir_all(src.join("contact")).unwrap();
        std::fs::write(src.join("message").join("message_0.db"), b"fake-db-1").unwrap();
        std::fs::write(src.join("contact").join("contact.db"), b"fake-db-2").unwrap();
        std::fs::write(src.join("message").join("message_0.db-wal"), b"skip").unwrap();

        let n = import_dir(&src, &dst).unwrap();
        assert_eq!(n, 2);
        assert!(dst.join("message").join("message_0.db").is_file());
        assert!(dst.join("contact").join("contact.db").is_file());
        assert!(!dst.join("message").join("message_0.db-wal").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// 结构校验：非微信目录应报错
    #[test]
    fn test_looks_like_decrypted_rejects() {
        let base = std::env::temp_dir().join("wx_import_test2");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("readme.txt"), b"hi").unwrap();
        assert!(looks_like_decrypted(&base).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
