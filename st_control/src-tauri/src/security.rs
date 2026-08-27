// Copyright (c) 2026 ST Team - MIT License
// See LICENSE file in the project root for full license information.

// ============================================================
// 应用级数据安全模块
//
// 功能：
//  1. 统一密钥管理：复用 bot_secret.key 作为应用主密钥
//  2. 敏感配置加密：LLM API Key / 外部服务 Token 等字段加密存储
//  3. 数据库完整性校验：SHA-256 校验和检测篡改
//  4. 安全审计日志：密钥访问 / 解密操作记录
//
// 设计原则：
//  - 不加密可搜索内容（文档/分块），避免破坏 FTS5 全文检索
//  - 仅加密「凭证类」数据（API Key / Token / Secret）
//  - 主密钥文件权限由操作系统保护（Windows ACL）
// ============================================================

use std::path::Path;
use std::sync::Mutex;

/// 应用级加密器（复用 bot_secret.key 主密钥）
#[allow(dead_code)]
pub struct AppCipher {
    cipher: crate::bot::secret::TokenCipher,
}

static APP_CIPHER: Mutex<Option<AppCipher>> = Mutex::new(None);

#[allow(dead_code)]
impl AppCipher {
    /// 初始化（应用启动时调用一次）
    pub fn init(data_dir: &Path) -> Result<(), String> {
        let cipher = crate::bot::secret::TokenCipher::load(data_dir)?;
        let mut guard = APP_CIPHER.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(AppCipher { cipher });
        log::info!("[security] 应用加密器已初始化");
        Ok(())
    }

    /// 加密敏感字符串（API Key / Token 等）
    pub fn encrypt(plain: &str) -> Result<String, String> {
        let guard = APP_CIPHER.lock().unwrap_or_else(|e| e.into_inner());
        let app = guard
            .as_ref()
            .ok_or("加密器未初始化，请先调用 AppCipher::init")?;
        app.cipher.encrypt(plain)
    }

    /// 解密敏感字符串
    pub fn decrypt(enc: &str) -> Result<String, String> {
        let guard = APP_CIPHER.lock().unwrap_or_else(|e| e.into_inner());
        let app = guard
            .as_ref()
            .ok_or("加密器未初始化，请先调用 AppCipher::init")?;
        app.cipher.decrypt(enc)
    }

    /// 检查加密器是否已初始化
    pub fn is_ready() -> bool {
        APP_CIPHER
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }
}

/// 数据库完整性校验（SHA-256）
///
/// 对 SQLite 数据库文件计算 SHA-256 校验和，
/// 可用于检测数据库文件是否被篡改。
pub fn compute_db_checksum(db_path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let data = std::fs::read(db_path).map_err(|e| format!("读取数据库文件失败: {}", e))?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

/// 验证数据库完整性
///
/// 比对当前数据库文件的 SHA-256 与预期值，
/// 返回 Ok(true) 表示完整，Ok(false) 表示已变更/篡改。
#[allow(dead_code)]
pub fn verify_db_integrity(db_path: &Path, expected_hash: &str) -> Result<bool, String> {
    let current = compute_db_checksum(db_path)?;
    Ok(current == expected_hash)
}

/// 知识库数据库完整性检查（综合）
///
/// 返回：
///  - 数据库文件是否存在
///  - 文件大小
///  - SHA-256 校验和
///  - WAL 文件状态
///  - 基本 SQLite 完整性检查（PRAGMA integrity_check）
pub fn kb_integrity_report(db_path: &Path) -> serde_json::Value {
    let exists = db_path.exists();
    if !exists {
        return serde_json::json!({
            "exists": false,
            "error": "数据库文件不存在"
        });
    }

    let metadata = std::fs::metadata(db_path).ok();
    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
    let checksum = compute_db_checksum(db_path).unwrap_or_default();

    // WAL 文件状态
    let wal_path = db_path.with_extension("db-wal");
    let shm_path = db_path.with_extension("db-shm");
    let wal_exists = wal_path.exists();
    let wal_size = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);

    // SQLite integrity_check（只读，不修改数据库）
    let integrity = match rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(conn) => match conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)) {
            Ok(result) => result,
            Err(e) => format!("integrity_check 执行失败: {}", e),
        },
        Err(e) => format!("打开数据库失败: {}", e),
    };

    serde_json::json!({
        "exists": true,
        "sizeBytes": size,
        "checksumSha256": checksum,
        "wal": {
            "exists": wal_exists,
            "sizeBytes": wal_size,
        },
        "shmExists": shm_path.exists(),
        "integrityCheck": integrity,
        "healthy": integrity == "ok",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_checksum_consistent() {
        // 创建临时文件测试校验和一致性
        let dir = std::env::temp_dir().join("st_security_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.db");
        std::fs::write(&path, b"test data for checksum").ok();

        let hash1 = compute_db_checksum(&path).unwrap();
        let hash2 = compute_db_checksum(&path).unwrap();
        assert_eq!(hash1, hash2, "相同文件校验和应一致");
        assert_eq!(hash1.len(), 64, "SHA-256 应为 64 个十六进制字符");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_db_checksum_detects_change() {
        let dir = std::env::temp_dir().join("st_security_test2");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test2.db");
        std::fs::write(&path, b"original data").ok();

        let hash1 = compute_db_checksum(&path).unwrap();
        std::fs::write(&path, b"modified data").ok();
        let hash2 = compute_db_checksum(&path).unwrap();
        assert_ne!(hash1, hash2, "修改后校验和应不同");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_verify_db_integrity() {
        let dir = std::env::temp_dir().join("st_security_test3");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test3.db");
        std::fs::write(&path, b"integrity test data").ok();

        let hash = compute_db_checksum(&path).unwrap();
        assert!(verify_db_integrity(&path, &hash).unwrap());
        assert!(!verify_db_integrity(&path, "wrong_hash").unwrap());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_kb_integrity_report_missing() {
        let path = Path::new("/nonexistent/path/db.sqlite");
        let report = kb_integrity_report(path);
        assert_eq!(report["exists"], false);
    }
}
