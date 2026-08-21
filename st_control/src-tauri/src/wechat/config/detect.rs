// ============================================================
// 微信配置 — 目录自动检测与账号扫描
// 自 config.rs 拆分：跨平台 xwechat 数据目录定位、
// 账号枚举（wxid + db_storage + 活跃度）。
// ============================================================

use std::path::{Path, PathBuf};

use super::DetectedAccount;

pub fn auto_detect_db_dir() -> Option<PathBuf> {
    let os = std::env::consts::OS;
    match os {
        "windows" => auto_detect_windows(),
        "linux" => auto_detect_linux(),
        "macos" => auto_detect_macos(),
        _ => None,
    }
}

/// Windows: 从 %APPDATA%\Tencent\xwechat\config\*.ini 读取数据目录
pub(crate) fn auto_detect_windows() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    let config_dir = Path::new(&appdata)
        .join("Tencent")
        .join("xwechat")
        .join("config");

    if !config_dir.is_dir() {
        return None;
    }

    // 读取所有 .ini 文件，从中提取路径
    let mut data_roots: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&config_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("ini") {
                continue;
            }
            // 读取 ini 内容（可能 UTF-8 或 GBK）
            let content = read_ini_content(&path)?;
            let content = content.trim();
            if !content.is_empty()
                && !content.contains('\n')
                && !content.contains('\r')
                && Path::new(content).is_dir()
            {
                data_roots.push(PathBuf::from(content));
            }
        }
    }

    // 在根目录下搜索 xwechat_files\*\db_storage
    let mut candidates: Vec<PathBuf> = Vec::new();
    for root in &data_roots {
        let pattern = root.join("xwechat_files");
        if let Ok(entries) = std::fs::read_dir(&pattern) {
            for entry in entries.flatten() {
                let db_storage = entry.path().join("db_storage");
                if db_storage.is_dir() && !candidates.contains(&db_storage) {
                    candidates.push(db_storage);
                }
            }
        }
    }

    choose_candidate(candidates)
}

/// Linux: 搜索 ~/Documents/xwechat_files/*/db_storage
pub(crate) fn auto_detect_linux() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let search_root = home.join("Documents").join("xwechat_files");

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&search_root) {
        for entry in entries.flatten() {
            let db_storage = entry.path().join("db_storage");
            if db_storage.is_dir() && !candidates.contains(&db_storage) {
                candidates.push(db_storage);
            }
        }
    }

    // 按 message 目录 mtime 降序（最近活跃优先）
    candidates.sort_by(|a, b| {
        let ma = dir_mtime(&a.join("message")).unwrap_or(0);
        let mb = dir_mtime(&b.join("message")).unwrap_or(0);
        mb.cmp(&ma)
    });

    choose_candidate(candidates)
}

/// macOS: 搜索 ~/Library/Containers/.../xwechat_files/*/db_storage
pub(crate) fn auto_detect_macos() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let base = home
        .join("Library")
        .join("Containers")
        .join("com.tencent.xinWeChat")
        .join("Data")
        .join("Documents")
        .join("xwechat_files");

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            let db_storage = entry.path().join("db_storage");
            if db_storage.is_dir() && !candidates.contains(&db_storage) {
                candidates.push(db_storage);
            }
        }
    }

    candidates.sort_by(|a, b| {
        let ma = dir_mtime(&a.join("message")).unwrap_or(0);
        let mb = dir_mtime(&b.join("message")).unwrap_or(0);
        mb.cmp(&ma)
    });

    choose_candidate(candidates)
}

// ============ 辅助函数 ============

/// 读取 .ini 文件，处理 UTF-8 和 GBK 编码
pub(crate) fn read_ini_content(path: &Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;

    // Windows ini 常带 null 填充字节与 \r，先清理再解码
    let cleaned: Vec<u8> = data.into_iter().filter(|&b| b != 0 && b != b'\r').collect();

    // 尝试 UTF-8
    if let Ok(s) = String::from_utf8(cleaned.clone()) {
        return Some(s);
    }

    // 回退 GBK
    let (s, _, _) = encoding_rs::GBK.decode(&cleaned);
    let s = s.into_owned();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// 获取目录的 mtime
pub(crate) fn dir_mtime(path: &Path) -> Option<u64> {
    let target = if path.is_dir() { path } else { path.parent()? };
    let meta = std::fs::metadata(target).ok()?;
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// 多候选时选择：按 message 目录 mtime 降序取最近活跃的账号
/// （≈当前登录使用的账号），而非任意第一个。
pub(crate) fn choose_candidate(mut candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.sort_by(|a, b| {
        let ma = dir_mtime(&a.join("message")).unwrap_or(0);
        let mb = dir_mtime(&b.join("message")).unwrap_or(0);
        mb.cmp(&ma)
    });
    candidates.into_iter().next()
}

/// 收集本机可能的 xwechat_files 根目录（供密钥扫描等枚举账号的场景）。
///
/// 【可移植】不再硬编码 `E:\Tencent\...` / `C:\Users\Administrator\...`：
/// 从微信配置 ini 指向的数据根 + 用户文档目录收集，部署到任意客户电脑均可命中。
pub fn candidate_xwechat_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    // 1) 微信配置 ini 指向的数据根
    if let Ok(appdata) = std::env::var("APPDATA") {
        let config_dir = Path::new(&appdata)
            .join("Tencent")
            .join("xwechat")
            .join("config");
        if let Ok(entries) = std::fs::read_dir(&config_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("ini") {
                    continue;
                }
                if let Some(content) = read_ini_content(&path) {
                    let content = content.trim();
                    if !content.is_empty()
                        && !content.contains('\n')
                        && !content.contains('\r')
                        && Path::new(content).is_dir()
                    {
                        roots.push(PathBuf::from(content).join("xwechat_files"));
                    }
                }
            }
        }
    }
    // 2) 用户文档目录（微信数据目录的常见位置之一）
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Documents").join("xwechat_files"));
    }
    roots.sort();
    roots.dedup();
    roots.retain(|p| p.is_dir());
    roots
}

pub fn detect_accounts() -> Vec<DetectedAccount> {
    let os = std::env::consts::OS;
    let search_roots = match os {
        "windows" => {
            let mut roots = Vec::new();
            if let Ok(appdata) = std::env::var("APPDATA") {
                let config_dir = Path::new(&appdata)
                    .join("Tencent")
                    .join("xwechat")
                    .join("config");
                if let Ok(entries) = std::fs::read_dir(&config_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) != Some("ini") {
                            continue;
                        }
                        if let Some(content) = read_ini_content(&path) {
                            let content = content.trim();
                            if !content.is_empty()
                                && !content.contains('\n')
                                && !content.contains('\r')
                                && Path::new(content).is_dir()
                            {
                                roots.push(PathBuf::from(content).join("xwechat_files"));
                            }
                        }
                    }
                }
            }
            roots
        }
        "linux" => dirs::home_dir()
            .map(|h| vec![h.join("Documents").join("xwechat_files")])
            .unwrap_or_default(),
        "macos" => dirs::home_dir()
            .map(|h| {
                vec![h
                    .join("Library")
                    .join("Containers")
                    .join("com.tencent.xinWeChat")
                    .join("Data")
                    .join("Documents")
                    .join("xwechat_files")]
            })
            .unwrap_or_default(),
        _ => vec![],
    };

    // 去重搜索根，避免同一目录被重复扫描
    let mut uniq_roots = search_roots.clone();
    uniq_roots.sort();
    uniq_roots.dedup();

    let mut accounts = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for root in &uniq_roots {
        for acc in scan_accounts_in_dir(root) {
            // 同一账号目录只保留一条记录（防止多搜索根重叠）
            if seen.insert(acc.db_dir.clone()) {
                accounts.push(acc);
            }
        }
    }

    // 按最近活跃时间降序
    accounts.sort_by_key(|a| std::cmp::Reverse(a.last_active));
    accounts
}

/// 在指定 xwechat_files 根目录下扫描账号子目录 → db_storage
///
/// `base_dir` 例如 `E:\Tencent\Weixin\xwechat_files`，
/// 其下每个子目录应包含 `db_storage`：
///
/// ```text
/// xwechat_files/
///   ├── wxid_xxx_xzy/        ← 账号目录
///   │   └── db_storage/      ← 加密数据库目录
///   └── wxid_yyy_abc/
///       └── db_storage/
/// ```
pub fn scan_accounts(xwechat_root: &Path) -> Vec<DetectedAccount> {
    scan_accounts_in_dir(xwechat_root)
}

/// 内部：从给定的目录中扫描子目录中的 db_storage
pub(crate) fn scan_accounts_in_dir(root: &Path) -> Vec<DetectedAccount> {
    let mut accounts = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let db_storage = entry.path().join("db_storage");
            if !db_storage.is_dir() {
                continue;
            }
            let wxid = entry.file_name().to_str().unwrap_or("").to_string();
            if wxid.is_empty() {
                continue;
            }
            let base_dir = entry.path().to_string_lossy().to_string();
            let last_active = dir_mtime(&db_storage.join("message")).unwrap_or(0);
            accounts.push(DetectedAccount {
                wxid,
                db_dir: db_storage.to_string_lossy().to_string(),
                base_dir,
                last_active,
            });
        }
    }
    accounts.sort_by_key(|a| std::cmp::Reverse(a.last_active));
    accounts
}
