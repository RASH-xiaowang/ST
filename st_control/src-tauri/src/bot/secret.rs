// ============================================================
// bot_token 本地加密存储
// 首次启动生成随机 32 字节密钥文件（应用数据目录），
// 使用 AES-256-CBC 加密，避免 token 明文落库。
// ============================================================

use aes::Aes256;
use base64::Engine;
use cbc::{Decryptor, Encryptor};
use cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use rand::RngCore;
use std::path::Path;

pub struct TokenCipher {
    key: [u8; 32],
}

impl TokenCipher {
    pub fn load(data_dir: &Path) -> Result<Self, String> {
        let path = data_dir.join("bot_secret.key");
        let key = if path.exists() {
            let s = std::fs::read_to_string(&path).map_err(|e| format!("读取密钥文件失败: {e}"))?;
            let s = s.trim();
            let mut key = [0u8; 32];
            hex::decode_to_slice(s, &mut key).map_err(|e| format!("密钥文件格式错误: {e}"))?;
            key
        } else {
            let mut key = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut key);
            let hex_str = hex::encode(key);
            std::fs::write(&path, &hex_str).map_err(|e| format!("写入密钥文件失败: {e}"))?;
            key
        };
        Ok(Self { key })
    }

    pub fn encrypt(&self, plain: &str) -> Result<String, String> {
        let mut iv = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut iv);
        // PKCS7 始终至少补一个块：缓冲必须预留填充空间，
        // 否则明文长度非 16 倍数时 encrypt_padded_mut 报 Padding error
        let padded_len = (plain.len() / 16 + 1) * 16;
        let mut buf = vec![0u8; padded_len];
        buf[..plain.len()].copy_from_slice(plain.as_bytes());
        let ct = Encryptor::<Aes256>::new_from_slices(&self.key, &iv)
            .map_err(|e| format!("密钥初始化失败: {e}"))?
            .encrypt_padded_mut::<Pkcs7>(&mut buf, plain.len())
            .map_err(|e| format!("加密失败: {e}"))?;
        let mut out = Vec::with_capacity(16 + ct.len());
        out.extend_from_slice(&iv);
        out.extend_from_slice(ct);
        Ok(base64::engine::general_purpose::STANDARD.encode(out))
    }

    pub fn decrypt(&self, enc: &str) -> Result<String, String> {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(enc)
            .map_err(|e| format!("token base64 解码失败: {e}"))?;
        if raw.len() <= 16 {
            return Err("token 密文长度异常".to_string());
        }
        let (iv, ct) = raw.split_at(16);
        let mut buf = ct.to_vec();
        let pt = Decryptor::<Aes256>::new_from_slices(&self.key, iv)
            .map_err(|e| format!("密钥初始化失败: {e}"))?
            .decrypt_padded_mut::<Pkcs7>(&mut buf)
            .map_err(|e| format!("解密失败: {e}"))?;
        String::from_utf8(pt.to_vec()).map_err(|e| format!("解密结果非 UTF-8: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cipher() -> TokenCipher {
        TokenCipher { key: [0x42u8; 32] }
    }

    #[test]
    fn roundtrip_various_lengths() {
        let c = cipher();
        for len in [0usize, 1, 15, 16, 17, 30, 31, 32, 48, 64, 128] {
            let plain: String = (0..len).map(|i| (b'a' + (i % 26) as u8) as char).collect();
            let enc = c
                .encrypt(&plain)
                .unwrap_or_else(|e| panic!("len {len} encrypt: {e}"));
            let dec = c
                .decrypt(&enc)
                .unwrap_or_else(|e| panic!("len {len} decrypt: {e}"));
            assert_eq!(dec, plain, "len {len} roundtrip mismatch");
        }
    }
}
