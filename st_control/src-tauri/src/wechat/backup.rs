//! 微信数据加密备份与恢复（备份管家）
//!
//! 文件格式 `.stbak`：
//! ```text
//! [5B magic "STWB1"][16B salt][16B IV][32B HMAC-SHA256][AES-256-CBC 密文]
//! ```
//! - 密钥派生：PBKDF2-HMAC-SHA512(passphrase, salt, 256000) → 32B AES 密钥
//! - HMAC 密钥：PBKDF2-HMAC-SHA512(passphrase, salt, 1000) → 32B
//! - HMAC 覆盖 (salt || iv || ciphertext)，防止密文被篡改
//!
//! 备份内容为解密库 ZIP（仅数据库，不含本地资源附件）；恢复时先解密到临时 ZIP，
//! 再走 `import_backup` 的结构校验与复制流程，写入本地解密区。

use crate::wechat::handlers::helpers;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Sha512};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const BACKUP_MAGIC: &[u8] = b"STWB1";
const SALT_SZ: usize = 16;
const IV_SZ: usize = 16;
const HMAC_SZ: usize = 32;
const AES_ITERS: u32 = 256_000;
const HMAC_ITERS: u32 = 1_000;

type Aes256Cbc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

fn derive_key(pass: &[u8], salt: &[u8], iters: u32) -> Vec<u8> {
    let mut key = vec![0u8; 32];
    let _ = pbkdf2::pbkdf2::<Hmac<Sha512>>(pass, salt, iters, &mut key);
    key
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}

fn hmac(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC key 长度错误");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// 加密产物（salt / iv / hmac / ciphertext）
struct EncryptedPayload {
    salt: [u8; SALT_SZ],
    iv: [u8; IV_SZ],
    hmac: [u8; HMAC_SZ],
    ciphertext: Vec<u8>,
}

/// 加密字节流
fn encrypt_bytes(data: &[u8], pass: &str) -> Result<EncryptedPayload, String> {
    use aes::cipher::block_padding::Pkcs7;
    use aes::cipher::{BlockEncryptMut, KeyIvInit};

    let salt = random_bytes::<SALT_SZ>();
    let iv = random_bytes::<IV_SZ>();
    let key = derive_key(pass.as_bytes(), &salt, AES_ITERS);
    let mut buf = vec![0u8; data.len() + 16];
    buf[..data.len()].copy_from_slice(data);
    let cipher = Aes256Cbc::new(key.as_slice().into(), iv.as_slice().into());
    let ct = cipher
        .encrypt_padded_mut::<Pkcs7>(&mut buf, data.len())
        .map_err(|e| format!("AES 加密失败: {}", e))?
        .to_vec();
    let mut signed = Vec::with_capacity(SALT_SZ + IV_SZ + ct.len());
    signed.extend_from_slice(&salt);
    signed.extend_from_slice(&iv);
    signed.extend_from_slice(&ct);
    let mac_key = derive_key(pass.as_bytes(), &salt, HMAC_ITERS);
    let mac = hmac(&mac_key, &signed);
    Ok(EncryptedPayload {
        salt,
        iv,
        hmac: mac,
        ciphertext: ct,
    })
}

/// 解密字节流（校验 HMAC 后 AES-CBC 解密）
fn decrypt_bytes(body: &[u8], pass: &str) -> Result<Vec<u8>, String> {
    use aes::cipher::block_padding::Pkcs7;
    use aes::cipher::{BlockDecryptMut, KeyIvInit};

    if body.len() < SALT_SZ + IV_SZ + HMAC_SZ + 1 {
        return Err("备份文件损坏（长度不足）".to_string());
    }
    let salt = &body[..SALT_SZ];
    let iv = &body[SALT_SZ..SALT_SZ + IV_SZ];
    let stored_mac = &body[SALT_SZ + IV_SZ..SALT_SZ + IV_SZ + HMAC_SZ];
    let ct = &body[SALT_SZ + IV_SZ + HMAC_SZ..];

    let mut signed = Vec::with_capacity(SALT_SZ + IV_SZ + ct.len());
    signed.extend_from_slice(salt);
    signed.extend_from_slice(iv);
    signed.extend_from_slice(ct);
    let mac_key = derive_key(pass.as_bytes(), salt, HMAC_ITERS);
    let mac = hmac(&mac_key, &signed);
    if mac.as_slice() != stored_mac {
        return Err("备份校验失败：口令错误或文件被篡改".to_string());
    }

    let key = derive_key(pass.as_bytes(), salt, AES_ITERS);
    let mut buf = ct.to_vec();
    let cipher = Aes256CbcDec::new(key.as_slice().into(), iv.into());
    let pt = cipher
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| format!("AES 解密失败（口令错误？）: {}", e))?;
    Ok(pt.to_vec())
}

fn read_whole(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("读取文件失败 {}: {}", path.display(), e))
}

fn write_whole(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    std::fs::write(path, data).map_err(|e| format!("写入文件失败 {}: {}", path.display(), e))
}

/// 创建加密备份：打包解密库 ZIP → 加密为 .stbak
pub fn create_encrypted_backup(
    app: &tauri::AppHandle,
    passphrase: &str,
    output_dir: &str,
) -> Result<serde_json::Value, String> {
    if passphrase.trim().len() < 4 {
        return Err("口令至少 4 位".to_string());
    }
    let cfg =
        crate::wechat::config::WeChatConfig::load().map_err(|e| format!("读取配置失败: {}", e))?;
    if !cfg.decrypted_dir.is_dir() {
        return Err("解密目录不存在，请先完成数据库解密".to_string());
    }
    let out_dir = PathBuf::from(output_dir.trim());
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("创建输出目录失败: {}", e))?;

    // 1) 用现有归档逻辑生成解密库 ZIP（仅数据库）
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let tmp_zip_dir =
        std::env::temp_dir().join(format!("st_backup_{}_{}", std::process::id(), stamp));
    std::fs::create_dir_all(&tmp_zip_dir).map_err(|e| format!("创建临时目录失败: {}", e))?;
    let archive = crate::wechat::archive::export_archive(
        app,
        &cfg.decrypted_dir,
        Some(tmp_zip_dir.to_string_lossy().to_string()),
        false,
    )?;
    let zip_path = PathBuf::from(
        archive
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "归档失败：未返回路径".to_string())?,
    );
    let zip_bytes = read_whole(&zip_path)?;

    // 2) 加密
    let EncryptedPayload {
        salt,
        iv,
        hmac: mac,
        ciphertext: ct,
    } = encrypt_bytes(&zip_bytes, passphrase)?;
    let mut out = Vec::with_capacity(BACKUP_MAGIC.len() + SALT_SZ + IV_SZ + HMAC_SZ + ct.len());
    out.extend_from_slice(BACKUP_MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&iv);
    out.extend_from_slice(&mac);
    out.extend_from_slice(&ct);

    let filename = format!("wechat_backup_{}.stbak", stamp);
    let out_path = out_dir.join(&filename);
    write_whole(&out_path, &out)?;

    // 清理临时 ZIP
    let _ = std::fs::remove_file(&zip_path);
    let _ = std::fs::remove_dir_all(&tmp_zip_dir);

    let size = out.len() as u64;
    let file_count = archive
        .get("file_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    log::info!(
        "[backup] 加密备份完成: {} ({} 文件, {} 字节)",
        out_path.display(),
        file_count,
        size
    );
    Ok(serde_json::json!({
        "path": out_path.to_string_lossy().to_string(),
        "filename": filename,
        "size": size,
        "file_count": file_count,
        "created_at": chrono::Local::now().to_rfc3339(),
    }))
}

/// 恢复加密备份：解密 → 临时 ZIP → 导入本地解密区
pub fn restore_encrypted_backup(path: &str, passphrase: &str) -> Result<serde_json::Value, String> {
    let p = PathBuf::from(path.trim());
    if !p.is_file() {
        return Err(format!("备份文件不存在: {}", p.display()));
    }
    let bytes = read_whole(&p)?;
    if !bytes.starts_with(BACKUP_MAGIC) {
        return Err("不是有效的加密备份文件（缺少 STWB1 标识）".to_string());
    }
    let body = &bytes[BACKUP_MAGIC.len()..];
    let zip_bytes = decrypt_bytes(body, passphrase)?;

    let tmp = std::env::temp_dir().join(format!("st_restore_{}.zip", std::process::id()));
    write_whole(&tmp, &zip_bytes)?;
    let result = crate::wechat::import_backup::import_wechat_backup(&tmp);
    let _ = std::fs::remove_file(&tmp);
    let imported = result?;
    Ok(serde_json::json!({
        "restored": true,
        "imported": imported.get("imported").cloned().unwrap_or(serde_json::Value::Null),
        "target": imported.get("target").cloned().unwrap_or(serde_json::Value::Null),
    }))
}

/// 列出指定目录下的加密备份文件（按修改时间倒序）
pub fn list_encrypted_backups(dir: &str) -> Result<serde_json::Value, String> {
    let d = PathBuf::from(dir.trim());
    if !d.is_dir() {
        return Err(format!("目录不存在: {}", d.display()));
    }
    let mut items: Vec<serde_json::Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&d) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("stbak") {
                continue;
            }
            let meta = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            items.push(serde_json::json!({
                "name": path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                "path": path.to_string_lossy().to_string(),
                "size": meta.len(),
                "modified": modified,
            }));
        }
    }
    items.sort_by(|a, b| {
        b.get("modified")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .cmp(&a.get("modified").and_then(|v| v.as_i64()).unwrap_or(0))
    });
    Ok(serde_json::json!({ "dir": d.to_string_lossy().to_string(), "items": items }))
}

/// 删除加密备份文件（仅允许 .stbak）
pub fn delete_encrypted_backup(path: &str) -> Result<serde_json::Value, String> {
    let p = PathBuf::from(path.trim());
    if p.extension().and_then(|e| e.to_str()) != Some("stbak") {
        return Err("只允许删除 .stbak 备份文件".to_string());
    }
    if !p.is_file() {
        return Err(format!("备份文件不存在: {}", p.display()));
    }
    std::fs::remove_file(&p).map_err(|e| format!("删除失败: {}", e))?;
    Ok(serde_json::json!({ "deleted": p.to_string_lossy().to_string() }))
}

// ============ IPC ============

#[tauri::command]
pub async fn create_wechat_backup(
    app: tauri::AppHandle,
    passphrase: String,
    output_dir: String,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || create_encrypted_backup(&app, &passphrase, &output_dir)).await
}

#[tauri::command]
pub async fn restore_wechat_backup(
    path: String,
    passphrase: String,
) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || restore_encrypted_backup(&path, &passphrase)).await
}

#[tauri::command]
pub async fn list_wechat_backups(dir: String) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || list_encrypted_backups(&dir)).await
}

#[tauri::command]
pub async fn delete_wechat_backup(path: String) -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || delete_encrypted_backup(&path)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_roundtrip() {
        let data =
            b"hello wechat backup \x00\x01\x02 \xE6\x95\x8F\xE6\x84\x9F\xE6\x95\xB0\xE6\x8D\xAE";
        let EncryptedPayload {
            salt,
            iv,
            hmac: mac,
            ciphertext: ct,
        } = encrypt_bytes(data, "pass-123").expect("加密失败");
        let mut body = Vec::new();
        body.extend_from_slice(&salt);
        body.extend_from_slice(&iv);
        body.extend_from_slice(&mac);
        body.extend_from_slice(&ct);
        let pt = decrypt_bytes(&body, "pass-123").expect("解密失败");
        assert_eq!(pt, data);
    }

    #[test]
    fn wrong_passphrase_rejected() {
        let data = b"secret";
        let EncryptedPayload {
            salt,
            iv,
            hmac: mac,
            ciphertext: ct,
        } = encrypt_bytes(data, "right-pass").expect("加密失败");
        let mut body = Vec::new();
        body.extend_from_slice(&salt);
        body.extend_from_slice(&iv);
        body.extend_from_slice(&mac);
        body.extend_from_slice(&ct);
        assert!(
            decrypt_bytes(&body, "wrong-pass").is_err(),
            "错误口令应解密失败"
        );
    }

    #[test]
    fn tamper_detected() {
        let data = b"secret";
        let EncryptedPayload {
            salt,
            iv,
            hmac: mut mac,
            ciphertext: mut ct,
        } = encrypt_bytes(data, "pass").expect("加密失败");
        ct[0] ^= 0xFF;
        let mut body = Vec::new();
        body.extend_from_slice(&salt);
        body.extend_from_slice(&iv);
        body.extend_from_slice(&mac);
        body.extend_from_slice(&ct);
        assert!(decrypt_bytes(&body, "pass").is_err(), "篡改应被 HMAC 拦截");
        // 还原后应正常
        ct[0] ^= 0xFF;
        mac = [0u8; 32];
        body = Vec::new();
        body.extend_from_slice(&salt);
        body.extend_from_slice(&iv);
        body.extend_from_slice(&mac);
        body.extend_from_slice(&ct);
        assert!(decrypt_bytes(&body, "pass").is_err());
    }
}
