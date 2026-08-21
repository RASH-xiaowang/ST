//! AES-128-ECB + PKCS7 加解密，以及 aes_key 三种编码兼容解析

use aes::Aes128;
use base64::Engine;
use cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyInit};

type Aes128EcbEnc = ecb::Encryptor<Aes128>;
type Aes128EcbDec = ecb::Decryptor<Aes128>;

/// 加密（AES-128-ECB + PKCS7）
pub fn encrypt(plaintext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>, String> {
    let padded_len = padded_size(plaintext.len());
    let mut buf = vec![0u8; padded_len];
    buf[..plaintext.len()].copy_from_slice(plaintext);
    let ct = Aes128EcbEnc::new(key.into())
        .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
        .map_err(|e| format!("AES-ECB 加密失败: {e}"))?;
    Ok(ct.to_vec())
}

/// 解密（AES-128-ECB + PKCS7）
pub fn decrypt(ciphertext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>, String> {
    let mut buf = ciphertext.to_vec();
    let pt = Aes128EcbDec::new(key.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| format!("AES-ECB 解密失败: {e}"))?;
    Ok(pt.to_vec())
}

/// PKCS7 补齐后的密文大小
pub fn padded_size(plaintext_size: usize) -> usize {
    (plaintext_size + 1).div_ceil(16) * 16
}

/// 解析 aes_key（兼容三种格式）
/// 1) 直接 hex（32 字符）
/// 2) base64(hex 字符串)
/// 3) base64(原始 16 字节)
pub fn parse_aes_key(input: &str) -> Result<[u8; 16], String> {
    let input = input.trim();
    // 格式 1：纯 hex 32 字符
    if input.len() == 32 && input.chars().all(|c| c.is_ascii_hexdigit()) {
        let mut key = [0u8; 16];
        hex::decode_to_slice(input, &mut key).map_err(|e| format!("hex 解析失败: {e}"))?;
        return Ok(key);
    }

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| format!("aes_key base64 解码失败: {e}"))?;

    // 格式 3：原始 16 字节
    if decoded.len() == 16 {
        let mut key = [0u8; 16];
        key.copy_from_slice(&decoded);
        return Ok(key);
    }

    // 格式 2：base64(hex 字符串)
    if decoded.len() == 32 {
        let hex_str =
            std::str::from_utf8(&decoded).map_err(|_| "aes_key hex 非 UTF-8".to_string())?;
        if hex_str.len() == 32 && hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
            let mut key = [0u8; 16];
            hex::decode_to_slice(hex_str, &mut key).map_err(|e| format!("hex 解析失败: {e}"))?;
            return Ok(key);
        }
    }

    Err(format!(
        "aes_key 必须为 16 字节原始值或 32 字符 hex（解码后 {} 字节）",
        decoded.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = [0xABu8; 16];
        let ct = encrypt(b"hello world!", &key).unwrap();
        assert_eq!(decrypt(&ct, &key).unwrap(), b"hello world!");
    }

    #[test]
    fn padded() {
        assert_eq!(padded_size(0), 16);
        assert_eq!(padded_size(16), 32);
        assert_eq!(padded_size(17), 32);
    }

    #[test]
    fn parse_hex() {
        let key = parse_aes_key("0123456789abcdef0123456789abcdef").unwrap();
        assert_eq!(key[0], 0x01);
        assert_eq!(key[15], 0xef);
    }

    #[test]
    fn parse_b64_hex() {
        let hex_str = "0123456789abcdef0123456789abcdef";
        let b64 = base64::engine::general_purpose::STANDARD.encode(hex_str.as_bytes());
        let key = parse_aes_key(&b64).unwrap();
        assert_eq!(key, hex::decode(hex_str).unwrap().as_slice());
    }

    #[test]
    fn parse_b64_raw() {
        let raw = [0x01u8; 16];
        let b64 = base64::engine::general_purpose::STANDARD.encode(raw);
        let key = parse_aes_key(&b64).unwrap();
        assert_eq!(key, raw);
    }
}
