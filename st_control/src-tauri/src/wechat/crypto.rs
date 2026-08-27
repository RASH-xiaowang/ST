//! SQLCipher 4 数据库解密模块
//!
//! WeChat 4.x 数据库加密参数:
//!   SQLCipher 4, AES-256-CBC, HMAC-SHA512, reserve=80, page_size=4096
//!
//! 页面布局 (4096 bytes):
//!   [16B salt] [3984B encrypted payload] [16B IV] [64B HMAC]
//!
//! 本模块提供所有加解密常量和核心函数,
//! 由 monitor / db_cache / keys 等模块统一引用。

use aes::cipher::{block_padding::NoPadding, BlockDecrypt, BlockDecryptMut, KeyInit, KeyIvInit};
use aes::Aes256;
use cbc::Decryptor as CbcDecryptor;
use hmac::{Hmac, Mac};
use sha2::Sha512;

// ============ 加密常量 ============

pub const PAGE_SZ: usize = 4096;
pub const KEY_SZ: usize = 32;
pub const SALT_SZ: usize = 16;
pub const IV_SZ: usize = 16;
pub const HMAC_SZ: usize = 64;
pub const RESERVE_SZ: usize = 80; // IV(16) + HMAC(64)
pub const SQLITE_HDR: &[u8] = b"SQLite format 3\x00";
pub const PBKDF2_ITERS: u32 = 256_000;
pub const WAL_HEADER_SZ: usize = 32;
pub const WAL_FRAME_HEADER_SZ: usize = 24;

type Aes256Cbc = CbcDecryptor<aes::Aes256>;

// ============ 密钥派生 ============

/// 从 raw key 派生真正的 AES 加密密钥。
///
/// 支持两种格式:
/// - v4.0 (默认): raw_key 已经是最终的 AES 密钥，直接使用
/// - wx_key_v4.1: raw_key 是 PBKDF2 的 passphrase，
///   需要用 per-DB salt 派生: PBKDF2-HMAC-SHA512(passphrase, salt, 256000)
pub fn derive_enc_key(raw_key: &[u8], salt: &[u8], key_format: Option<&str>) -> Vec<u8> {
    match key_format {
        Some("wx_key_v4.1") => {
            let mut key = vec![0u8; KEY_SZ];
            let _ = pbkdf2::pbkdf2::<Hmac<Sha512>>(raw_key, salt, PBKDF2_ITERS, &mut key);
            key
        }
        _ => raw_key.to_vec(),
    }
}

/// 从 enc_key 派生 HMAC 密钥 (用于 page-1 完整性验证)
pub fn derive_mac_key(enc_key: &[u8], salt: &[u8]) -> Vec<u8> {
    let mac_salt: Vec<u8> = salt.iter().map(|b| b ^ 0x3A).collect();
    let mut mac_key = vec![0u8; KEY_SZ];
    let _ = pbkdf2::pbkdf2::<Hmac<Sha512>>(enc_key, &mac_salt, 2, &mut mac_key);
    mac_key
}

// ============ AES-256-CBC 解密 ============

fn aes256_cbc_decrypt(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
    let mut buf = data.to_vec();
    let dec = Aes256Cbc::new(key.into(), iv.into());
    let buf_view = dec
        .decrypt_padded_mut::<NoPadding>(&mut buf)
        .expect("AES-CBC 解密失败");
    let len = buf_view.len();
    buf.truncate(len);
    buf
}

fn aes256_ecb_decrypt_block(key: &[u8], block: &[u8]) -> Vec<u8> {
    use aes::cipher::generic_array::GenericArray;
    let cipher = Aes256::new_from_slice(key).expect("AES-256 key 长度错误");
    let mut ga = GenericArray::clone_from_slice(&block[..16]);
    cipher.decrypt_block(&mut ga);
    ga.to_vec()
}

// ============ 页面解密 ============

/// 解密单个 4096 字节页面，输出标准 SQLite 页面。
///
/// `enc_key` 必须是已经派生好的最终 AES-256 密钥 (32 bytes)。
pub fn decrypt_page(enc_key: &[u8], page_data: &[u8], pgno: u32) -> Vec<u8> {
    assert_eq!(page_data.len(), PAGE_SZ, "页面大小必须为 4096 字节");
    assert_eq!(enc_key.len(), KEY_SZ, "密钥长度必须为 32 字节");

    let iv = &page_data[PAGE_SZ - RESERVE_SZ..PAGE_SZ - RESERVE_SZ + IV_SZ];

    if pgno == 1 {
        // Page 1 的前 16 字节是 salt
        let encrypted = &page_data[SALT_SZ..PAGE_SZ - RESERVE_SZ];
        let decrypted = aes256_cbc_decrypt(enc_key, iv, encrypted);

        let mut page = Vec::with_capacity(PAGE_SZ);
        page.extend_from_slice(SQLITE_HDR);
        page.extend_from_slice(&decrypted);
        page.resize(PAGE_SZ, 0x00);
        page
    } else {
        let encrypted = &page_data[..PAGE_SZ - RESERVE_SZ];
        let decrypted = aes256_cbc_decrypt(enc_key, iv, encrypted);

        let mut page = decrypted;
        page.resize(PAGE_SZ, 0x00);
        page
    }
}

// ============ 全量数据库解密 ============

/// 解密整个数据库文件。
///
/// `enc_key` 必须是已经派生好的最终 AES-256 密钥 (32 bytes)。
/// 返回解密的总页数。
pub fn full_decrypt(
    db_path: &std::path::Path,
    out_path: &std::path::Path,
    enc_key: &[u8],
) -> std::io::Result<u32> {
    use std::io::{BufReader, Read, Write};

    let mut src = BufReader::new(std::fs::File::open(db_path)?);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = std::fs::File::create(out_path)?;

    // 流式逐页解密：避免一次性读入整个大库（数百 MB）导致并行解密时内存峰值过高。
    // 末页不足 4096 字节时补零（与 SQLCipher 页面对齐语义一致）。
    let mut buf = vec![0u8; PAGE_SZ];
    let mut pgno: u32 = 0;
    loop {
        let mut filled = 0usize;
        while filled < PAGE_SZ {
            let n = src.read(&mut buf[filled..])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            break;
        }
        if filled < PAGE_SZ {
            buf[filled..].fill(0x00);
        }
        let decrypted = decrypt_page(enc_key, &buf, pgno + 1);
        out.write_all(&decrypted)?;
        pgno += 1;
    }
    Ok(pgno)
}

// ============ WAL 解密与 Patch ============

/// 解密 WAL 当前有效 frame，patch 到已解密的 DB 副本。
///
/// WAL 是预分配固定大小 (通常 4MB)，包含当前有效 frame 和上一轮遗留的旧 frame。
/// 通过 WAL header 中的 salt 值区分：只有 frame header 的 salt 匹配 WAL header
/// 的才是有效 frame。
///
/// 返回 patch 的页数。
pub fn decrypt_wal(
    wal_path: &std::path::Path,
    out_path: &std::path::Path,
    enc_key: &[u8],
) -> std::io::Result<u32> {
    use std::io::{Seek, SeekFrom, Write};

    let wal_data = match std::fs::read(wal_path) {
        Ok(d) => d,
        Err(_) => return Ok(0),
    };

    if wal_data.len() <= WAL_HEADER_SZ {
        return Ok(0);
    }

    let frame_size = WAL_FRAME_HEADER_SZ + PAGE_SZ; // 24 + 4096 = 4120

    // 解析 WAL header
    let wal_salt1 = u32::from_be_bytes([wal_data[16], wal_data[17], wal_data[18], wal_data[19]]);
    let wal_salt2 = u32::from_be_bytes([wal_data[20], wal_data[21], wal_data[22], wal_data[23]]);

    // 以"只写变化页"的方式打开已解密 DB。
    // 注意：绝不能整库读改写——消息库可达数百 MB，整库读写会让每条
    // 新消息的 WAL patch 都变成秒级 I/O，是实时推送卡顿的主因。
    let mut out = match std::fs::OpenOptions::new().write(true).open(out_path) {
        Ok(f) => f,
        Err(_) => return Ok(0),
    };

    let mut patched = 0u32;
    let mut offset = WAL_HEADER_SZ;
    while offset + frame_size <= wal_data.len() {
        // 解析 frame header
        let fh = &wal_data[offset..offset + WAL_FRAME_HEADER_SZ];
        let pgno = u32::from_be_bytes([fh[0], fh[1], fh[2], fh[3]]);
        let frame_salt1 = u32::from_be_bytes([fh[8], fh[9], fh[10], fh[11]]);
        let frame_salt2 = u32::from_be_bytes([fh[12], fh[13], fh[14], fh[15]]);

        let ep_start = offset + WAL_FRAME_HEADER_SZ;
        let ep = &wal_data[ep_start..ep_start + PAGE_SZ];

        // 校验: pgno 有效 且 salt 匹配当前 WAL 周期
        if pgno == 0 || pgno > 1_000_000 {
            offset += frame_size;
            continue;
        }
        if frame_salt1 != wal_salt1 || frame_salt2 != wal_salt2 {
            offset += frame_size;
            continue;
        }

        let dec = decrypt_page(enc_key, ep, pgno);

        // 原地 patch 该页（seek + 写 4KB，超出 EOF 时文件自动扩展）
        let write_pos = (pgno as u64 - 1) * PAGE_SZ as u64;
        out.seek(SeekFrom::Start(write_pos))?;
        out.write_all(&dec)?;
        patched += 1;
        offset += frame_size;
    }

    if patched > 0 {
        out.flush()?;
    }
    Ok(patched)
}

// ============ HMAC 验证 (page 1) ============

/// 验证 page 1 的 HMAC-SHA512 完整性校验。
///
/// 返回 `true` 表示密钥正确且数据未被篡改。
pub fn verify_page1_hmac(enc_key: &[u8], salt: &[u8], page1_data: &[u8]) -> bool {
    let mac_key = derive_mac_key(enc_key, salt);

    // HMAC 数据: page1[16 .. PAGE_SZ - RESERVE_SZ + IV_SZ]
    // = page1[16 .. 4016 + 16] = page1[16 .. 4032]
    let hmac_data = &page1_data[SALT_SZ..PAGE_SZ - RESERVE_SZ + IV_SZ];
    let stored_hmac = &page1_data[PAGE_SZ - HMAC_SZ..PAGE_SZ];

    let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(&mac_key).expect("HMAC key 长度错误");
    mac.update(hmac_data);
    mac.update(&(1u32).to_le_bytes()); // pgno 小端编码

    let computed = mac.finalize().into_bytes();
    computed.as_slice() == stored_hmac
}

/// 双重验证并返回派生密钥：PBKDF2-SHA512 派生 → HMAC + AES 校验
///
/// 用户输入的是 64 hex 字符的 wx_key_bin（32 字节），
/// 返回派生的 32 字节 AES 密钥（双重验证通过）或 None。
pub fn derive_and_verify(wx_key_bin: &[u8], page1: &[u8]) -> Option<Vec<u8>> {
    if page1.len() < PAGE_SZ || wx_key_bin.len() != KEY_SZ {
        return None;
    }
    let salt = &page1[..SALT_SZ];

    // 1. PBKDF2-SHA512 派生加密密钥（32 字节）
    let mut derived_key = vec![0u8; KEY_SZ];
    let _ = pbkdf2::pbkdf2::<Hmac<Sha512>>(wx_key_bin, salt, PBKDF2_ITERS, &mut derived_key);

    // 2. HMAC 验证（仅比较前 16 字节）
    let mac_key = derive_mac_key(&derived_key, salt);
    let hmac_data = &page1[SALT_SZ..PAGE_SZ - RESERVE_SZ + IV_SZ];
    let stored_hmac = &page1[PAGE_SZ - HMAC_SZ..PAGE_SZ];
    let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(&mac_key).expect("HMAC key 长度错误");
    mac.update(hmac_data);
    mac.update(&(1u32).to_le_bytes());
    let computed = mac.finalize().into_bytes();
    if computed[..] != stored_hmac[..] {
        return None;
    }

    // 3. AES 解密验证（ECB 修正 IV + CBC）
    let first_block_dec = aes256_ecb_decrypt_block(&derived_key, &page1[16..32]);
    let corrected_iv: Vec<u8> = first_block_dec
        .iter()
        .zip(SQLITE_HDR.iter())
        .map(|(a, b)| a ^ b)
        .collect();
    let encrypted = &page1[16..PAGE_SZ - RESERVE_SZ];
    let decrypted = aes256_cbc_decrypt(&derived_key, &corrected_iv, encrypted);
    if !decrypted.starts_with(SQLITE_HDR) {
        return None;
    }

    Some(derived_key)
}

// ============ 密钥双重验证 (用于 import_wx_key) ============

/// 验证 wx_key 是否能解密 page 1（含 HMAC + AES 双重验证）。
///
/// 返回 `(hmac_ok, aes_ok)` 二元组。
pub fn verify_key(wx_key_bin: &[u8], page1: &[u8]) -> (bool, bool) {
    if page1.len() < PAGE_SZ {
        return (false, false);
    }

    let salt = &page1[..SALT_SZ];

    // 1. PBKDF2 派生加密密钥
    let mut derived_key = vec![0u8; KEY_SZ];
    let _ = pbkdf2::pbkdf2::<Hmac<Sha512>>(wx_key_bin, salt, PBKDF2_ITERS, &mut derived_key);

    // 2. HMAC 验证
    let mac_key = derive_mac_key(&derived_key, salt);

    // 手工计算 HMAC 并比较
    let hmac_data = &page1[16..PAGE_SZ - RESERVE_SZ + IV_SZ]; // 16..4032
    let stored_hmac = &page1[PAGE_SZ - HMAC_SZ..PAGE_SZ]; // 4032..4096
    let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(&mac_key).expect("HMAC key");
    mac.update(hmac_data);
    mac.update(&(1u32).to_le_bytes());
    let computed = mac.finalize().into_bytes();
    let computed_slice: &[u8] = computed.as_slice();
    let hmac_ok = computed_slice == stored_hmac;

    // 3. AES 解密验证 (修正 IV)
    let first_block_dec = aes256_ecb_decrypt_block(&derived_key, &page1[16..32]);
    let corrected_iv: Vec<u8> = first_block_dec
        .iter()
        .zip(SQLITE_HDR.iter())
        .map(|(a, b)| a ^ b)
        .collect();
    let encrypted = &page1[16..PAGE_SZ - RESERVE_SZ];
    let decrypted = aes256_cbc_decrypt(&derived_key, &corrected_iv, encrypted);
    let aes_ok = decrypted.starts_with(SQLITE_HDR);
    log::debug!(
        "[crypto] verify_key: hmac_ok={}, aes_ok={}",
        hmac_ok,
        aes_ok
    );

    (hmac_ok, aes_ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证常量一致性
    #[test]
    fn test_constants() {
        assert_eq!(PAGE_SZ, 4096);
        assert_eq!(
            SALT_SZ + (PAGE_SZ - RESERVE_SZ - SALT_SZ) + IV_SZ + HMAC_SZ,
            PAGE_SZ
        );
        // page 1 encrypted = 4016 - 16 = 4000 bytes
        assert_eq!(PAGE_SZ - RESERVE_SZ - SALT_SZ, 4000);
        // other page encrypted = 4016 bytes
        assert_eq!(PAGE_SZ - RESERVE_SZ, 4016);
    }

    /// verify_page1_hmac 在无效数据上返回 false
    #[test]
    fn test_verify_page1_hmac_invalid() {
        let enc_key = vec![0xAB; KEY_SZ];
        let salt = vec![0x00; SALT_SZ];
        let page1 = vec![0x00; PAGE_SZ];

        assert!(!verify_page1_hmac(&enc_key, &salt, &page1));
    }

    /// full_decrypt 对空文件返回 0
    #[test]
    fn test_full_decrypt_empty() {
        let dir = std::env::temp_dir().join("wechat_crypto_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let empty_db = dir.join("empty.db");
        let out_db = dir.join("out.db");
        std::fs::File::create(&empty_db).unwrap();

        let enc_key = vec![0x00; KEY_SZ];
        let result = full_decrypt(&empty_db, &out_db, &enc_key);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
