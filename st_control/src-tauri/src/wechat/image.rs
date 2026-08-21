//! 微信图片 .dat 文件解密模块
//!
//! 支持三种加密格式:
//!   - 旧格式: 单字节 XOR 加密，自动检测 key
//!   - V1 格式: AES-128-ECB + XOR，固定 key
//!   - V2 格式: AES-128-ECB + XOR，需从微信内存提取 AES key

use crate::wechat::db_cache::MonitorDBCache;
use std::path::PathBuf;

mod crypto;
pub(crate) use crypto::*;
mod resolve;
pub(crate) use resolve::*;

// ============ Protobuf MD5 提取 ============

/// 从 message_resource.db 的 packed_info (protobuf) 中提取文件 MD5
pub fn extract_md5_from_packed_info(blob: &[u8]) -> Option<String> {
    // 查找 protobuf 标记: \x12\x22\x0a\x20 + 32 字节 hex MD5
    let marker = b"\x12\x22\x0a\x20";
    if let Some(idx) = blob.windows(marker.len()).position(|w| w == marker) {
        let start = idx + marker.len();
        if start + 32 <= blob.len() {
            let md5_bytes = &blob[start..start + 32];
            // 验证是合法 hex
            if md5_bytes.iter().all(|&b| b.is_ascii_hexdigit()) {
                return std::str::from_utf8(md5_bytes).ok().map(|s| s.to_string());
            }
        }
    }

    // 备用: 扫描 32 字节连续 hex
    let hex_set: [u8; 16] = *b"0123456789abcdef";
    let mut i = 0;
    while i + 32 <= blob.len() {
        let candidate = &blob[i..i + 32];
        if candidate.iter().all(|&b| hex_set.contains(&b)) {
            return std::str::from_utf8(candidate).ok().map(|s| s.to_string());
        }
        i += 1;
    }

    None
}

// ============ ImageResolver ============

/// 图片解析结果
#[derive(Debug)]
pub struct ImageResult {
    pub success: bool,
    pub path: Option<PathBuf>,
    pub format: Option<String>,
    pub md5: Option<String>,
    pub error: Option<String>,
}

/// 封装从 local_id 到图片文件的完整解析链
pub struct ImageResolver {
    wechat_base_dir: PathBuf,
    decoded_image_dir: PathBuf,
    db_cache: std::sync::Arc<MonitorDBCache>,
    aes_key: Option<Vec<u8>>,
    xor_key: u8,
}

impl ImageResolver {
    pub fn new(
        wechat_base_dir: PathBuf,
        decoded_image_dir: PathBuf,
        db_cache: std::sync::Arc<MonitorDBCache>,
        aes_key: Option<Vec<u8>>,
        xor_key: u8,
    ) -> Self {
        Self {
            wechat_base_dir,
            decoded_image_dir,
            db_cache,
            aes_key,
            xor_key,
        }
    }

    /// 通过 (username, local_id) 获取图片 MD5
    pub fn get_image_md5(&self, username: &str, local_id: i64) -> Option<String> {
        let res_path = self.db_cache.get("message/message_resource.db").ok()??;
        get_image_md5_from_db(&res_path, username, local_id)
    }

    /// 在 attach 目录下查找 .dat 文件
    pub fn find_dat_files(&self, username: &str, file_md5: &str) -> Vec<PathBuf> {
        find_dat_files(&self.wechat_base_dir, username, file_md5)
    }

    /// 解密图片并返回 base64 data URL（供实时推送直接内嵌）
    pub fn decode_image_data_url(&self, username: &str, local_id: i64) -> Option<String> {
        let file_md5 = self.get_image_md5(username, local_id)?;
        let dats = self.find_dat_files(username, &file_md5);
        let best = select_best_dat(&dats)?;
        decode_dat_to_data_url(
            &best,
            &self.decoded_image_dir.join(username),
            &file_md5,
            self.aes_key.as_deref(),
            self.xor_key,
        )
    }

    /// 完整流程: local_id → MD5 → .dat → 解密
    pub fn decode_image(&self, username: &str, local_id: i64) -> ImageResult {
        // 1. 获取 MD5
        let file_md5 = match self.get_image_md5(username, local_id) {
            Some(m) => m,
            None => {
                return ImageResult {
                    success: false,
                    path: None,
                    format: None,
                    md5: None,
                    error: Some(format!(
                        "无法找到图片 MD5: username={} local_id={}",
                        username, local_id
                    )),
                };
            }
        };

        // 2. 找 .dat 文件
        let dat_files = self.find_dat_files(username, &file_md5);
        if dat_files.is_empty() {
            return ImageResult {
                success: false,
                path: None,
                format: None,
                md5: Some(file_md5.clone()),
                error: Some(format!("找不到 .dat 文件 (MD5={})", file_md5)),
            };
        }

        let selected = match select_best_dat(&dat_files) {
            Some(p) => p,
            None => {
                return ImageResult {
                    success: false,
                    path: None,
                    format: None,
                    md5: Some(file_md5),
                    error: Some("无可用 .dat 文件".to_string()),
                };
            }
        };
        let selected = selected.as_path();
        let out_dir = self.decoded_image_dir.join(username);
        std::fs::create_dir_all(&out_dir).ok();
        let out_path = out_dir.join(format!("{}.tmp", file_md5));

        // 3. 解密
        let aes_ref = self.aes_key.as_deref();
        match decrypt_dat_file(selected, Some(&out_path), aes_ref, self.xor_key) {
            Ok((tmp_path, fmt)) => {
                let final_path = out_dir.join(format!("{}.{}", file_md5, fmt));
                if final_path.exists() {
                    std::fs::remove_file(&final_path).ok();
                }
                std::fs::rename(&tmp_path, &final_path).ok();
                ImageResult {
                    success: true,
                    path: Some(final_path),
                    format: Some(fmt.to_string()),
                    md5: Some(file_md5),
                    error: None,
                }
            }
            Err(e) => ImageResult {
                success: false,
                path: None,
                format: None,
                md5: Some(file_md5),
                error: Some(format!("解密失败: {}", e)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wechat::modules::common;

    /// 诊断：从本机解密消息库找最近一条图片消息，解析 CDN fileid/aeskey。
    #[test]
    #[cfg(target_os = "windows")]
    fn diag_cdn_info_lookup() {
        let Some(cfg) = crate::wechat::config::WeChatConfig::load().ok() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let Some(account_dir_name) = cfg
            .db_dir
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
        else {
            eprintln!("无法从 db_dir 推导 wxid 目录名");
            return;
        };
        // 消息表名哈希用干净 wxid（wxid_xxx，去掉 _f312 后缀）
        let username = crate::wechat::auto_key::clean_wxid(&account_dir_name);
        println!("账号目录名(username): {}", username);
        let table = common::msg_table_name(&username);
        let decrypted_dir = cfg.decrypted_dir.clone();
        let msg_dir = decrypted_dir.join("message");
        if !msg_dir.is_dir() {
            eprintln!("解密消息目录不存在: {}", msg_dir.display());
            return;
        }
        // 在 message_*.db 中找该账号的 Msg_ 表，取最近一条图片消息
        use rusqlite::Connection;
        let mut found = false;
        if let Ok(entries) = std::fs::read_dir(&msg_dir) {
            let mut dbs: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().and_then(|e| e.to_str()) == Some("db")
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.starts_with("message_"))
                            .unwrap_or(false)
                })
                .collect();
            dbs.sort();
            for db in dbs {
                let Ok(conn) =
                    Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                else {
                    continue;
                };
                let sql = format!(
                    "SELECT local_id, message_content, compress_content FROM \"{}\" \
                     WHERE message_content IS NOT NULL ORDER BY create_time DESC LIMIT 300",
                    table
                );
                let rows: Vec<(i64, Option<Vec<u8>>, Option<Vec<u8>>)> = conn
                    .prepare(&sql)
                    .ok()
                    .and_then(|mut stmt| {
                        stmt.query_map([], |r| {
                            Ok((
                                r.get::<_, i64>(0)?,
                                common::get_bytes(r, 1),
                                common::get_bytes(r, 2),
                            ))
                        })
                        .ok()
                        .map(|rows| rows.flatten().collect())
                    })
                    .unwrap_or_default();
                drop(conn);
                for (local_id, content, compressed) in rows {
                    let xml: String = content
                        .or(compressed)
                        .map(|b| common::decode_blob_text(&b))
                        .unwrap_or_default();
                    if !xml.contains("cdnbigimgurl") && !xml.contains("<img") {
                        continue;
                    }
                    println!(
                        "库={} 表={} local_id={} xml_len={}",
                        db.file_name().unwrap_or_default().to_string_lossy(),
                        table,
                        local_id,
                        xml.len()
                    );
                    if !xml.is_empty() {
                        println!("  XML 片段: {}", &xml[..xml.len().min(400)]);
                    }
                    let info = crate::wechat::cdn_image::lookup_image_cdn_info(
                        &decrypted_dir,
                        &username,
                        local_id,
                    );
                    println!("  CDN lookup 结果: {:?}", info);
                    found = true;
                    break;
                }
            }
        }
        if !found {
            eprintln!("未找到图片消息");
        }
    }

    /// 冒烟测试：对最近一条图片消息实际走 CDN 下载原图（需网络 + 微信内部配置）。
    #[test]
    #[cfg(target_os = "windows")]
    #[ignore = "需要访问外网 CDN 服务"]
    fn smoke_cdn_download() {
        let Some(cfg) = crate::wechat::config::WeChatConfig::load().ok() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let Some(account_dir_name) = cfg
            .db_dir
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
        else {
            return;
        };
        let username = crate::wechat::auto_key::clean_wxid(&account_dir_name);
        let decrypted_dir = cfg.decrypted_dir.clone();
        let decoded_dir = cfg.decoded_image_dir.clone();
        let base_dir = cfg.wechat_base_dir.clone();

        // 找最近一条图片消息 local_id
        let table = common::msg_table_name(&username);
        let msg_dir = decrypted_dir.join("message");
        let mut target: Option<i64> = None;
        if let Ok(entries) = std::fs::read_dir(&msg_dir) {
            let mut dbs: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().and_then(|e| e.to_str()) == Some("db")
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| {
                                n.starts_with("message_")
                                    && !n.contains("fts")
                                    && !n.contains("resource")
                            })
                            .unwrap_or(false)
                })
                .collect();
            dbs.sort();
            for db in dbs {
                let Ok(conn) = rusqlite::Connection::open_with_flags(
                    &db,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                ) else {
                    continue;
                };
                let sql = format!(
                    "SELECT local_id, message_content, compress_content FROM \"{}\" \
                     WHERE message_content IS NOT NULL ORDER BY create_time DESC LIMIT 500",
                    table
                );
                let rows: Vec<(i64, Option<Vec<u8>>, Option<Vec<u8>>)> = conn
                    .prepare(&sql)
                    .ok()
                    .and_then(|mut stmt| {
                        stmt.query_map([], |r| {
                            Ok((
                                r.get::<_, i64>(0)?,
                                common::get_bytes(r, 1),
                                common::get_bytes(r, 2),
                            ))
                        })
                        .ok()
                        .map(|rows| rows.flatten().collect())
                    })
                    .unwrap_or_default();
                drop(conn);
                for (local_id, content, compressed) in rows {
                    let xml: String = content
                        .or(compressed)
                        .map(|b| common::decode_blob_text(&b))
                        .unwrap_or_default();
                    if xml.contains("cdnbigimgurl") {
                        target = Some(local_id);
                        break;
                    }
                }
                if target.is_some() {
                    break;
                }
            }
        }
        let Some(local_id) = target else {
            eprintln!("未找到带 cdnbigimgurl 的图片消息");
            return;
        };
        println!("目标图片 local_id={} username={}", local_id, username);
        let (fileid, aeskey, _has_big) =
            crate::wechat::cdn_image::lookup_image_cdn_info(&decrypted_dir, &username, local_id)
                .unwrap();
        // 完整 CDN 回退（查 XML → 下载 → 缓存）
        println!(
            "CDN info: fileid={}… aeskey={}",
            &fileid[..fileid.len().min(16)],
            aeskey
        );
        let bytes = crate::wechat::cdn_image::try_cdn_fallback(
            &base_dir,
            &decrypted_dir,
            &decoded_dir.join(&username),
            &username,
            local_id,
        );
        match bytes {
            Some(b) => {
                println!(
                    "CDN 原图下载成功: {} 字节，格式 {}",
                    b.len(),
                    detect_image_format(&b)
                );
                assert!(b.len() > 1000, "原图字节数异常偏小");
            }
            None => panic!("CDN 原图下载失败"),
        }
    }

    #[test]
    fn test_detect_xor_key_jpg() {
        // jpg magic: FF D8 FF
        let data = vec![0xFF ^ 0x88, 0xD8 ^ 0x88, 0xFF ^ 0x88, 0x00, 0x01, 0x02];
        assert_eq!(detect_xor_key(&data), Some(0x88));
    }

    #[test]
    fn test_detect_image_format() {
        assert_eq!(detect_image_format(b"\xFF\xD8\xFF\xE0"), "jpg");
        assert_eq!(detect_image_format(b"\x89PNG\r\n"), "png");
        assert_eq!(detect_image_format(b"GIF89a"), "gif");
        assert_eq!(detect_image_format(b"BM\x00\x00"), "bmp");
    }

    #[test]
    fn test_is_v2_format() {
        let v2 = b"\x07\x08V2\x08\x07...";
        assert!(is_v2_format(v2));
        assert!(!is_v2_format(b"plain data"));
    }

    #[test]
    fn test_aligned_block_size() {
        assert_eq!(aligned_aes_block_size(16), 32);
        assert_eq!(aligned_aes_block_size(15), 16);
        assert_eq!(aligned_aes_block_size(32), 48);
    }

    #[test]
    fn test_extract_md5() {
        // Protobuf 格式: \x12\x22\x0a\x20 + 32 hex chars
        let mut blob = b"\x12\x22\x0a\x20".to_vec();
        blob.extend_from_slice(b"abcdef1234567890abcdef1234567890");
        let md5 = extract_md5_from_packed_info(&blob);
        assert_eq!(md5.as_deref(), Some("abcdef1234567890abcdef1234567890"));
    }

    #[test]
    fn test_xor_decrypt_roundtrip() {
        let dir = std::env::temp_dir().join("img_test_xor");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 创建一个 XOR 加密的 PNG
        let png_header = b"\x89PNG\r\n\x1a\n";
        let encrypted: Vec<u8> = png_header.iter().map(|&b| b ^ 0x12).collect();
        let dat = dir.join("test.dat");
        std::fs::write(&dat, &encrypted).unwrap();

        let (out, fmt) = decrypt_dat_file(&dat, None, None, 0x12).unwrap();
        assert_eq!(fmt, "png");
        assert!(out.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_aes_ecb_file_roundtrip() {
        use aes::cipher::generic_array::GenericArray;
        use aes::cipher::BlockEncrypt;
        use aes::cipher::KeyInit;
        use aes::Aes128Enc;

        let key = hex::decode("a1b2c3d4e5f60718293a4b5c6d7e8f90").unwrap();
        let plain = b"hello wechat image !".to_vec();

        // 手工构造 AES-128-ECB + PKCS7 密文
        let mut padded = plain.clone();
        let pad = 16 - padded.len() % 16;
        padded.extend(std::iter::repeat(pad as u8).take(pad));
        let cipher = Aes128Enc::new_from_slice(&key).unwrap();
        let mut ct = Vec::with_capacity(padded.len());
        for chunk in padded.chunks(16) {
            let mut ga = GenericArray::clone_from_slice(chunk);
            cipher.encrypt_block(&mut ga);
            ct.extend_from_slice(&ga);
        }

        let dec = aes_ecb_decrypt_file(&key, &ct).unwrap();
        assert_eq!(dec, plain);
        // 非 16 倍数密文应报错
        assert!(aes_ecb_decrypt_file(&key, &ct[..ct.len() - 1]).is_err());
    }

    #[test]
    fn test_decode_cdn_aes_key() {
        // 32 位 hex → 16 字节
        assert_eq!(
            decode_cdn_aes_key("a1b2c3d4e5f60718293a4b5c6d7e8f90").unwrap(),
            hex::decode("a1b2c3d4e5f60718293a4b5c6d7e8f90").unwrap()
        );
        // 原始 16 字节
        let raw: Vec<u8> = (0u8..16).collect();
        assert_eq!(
            decode_cdn_aes_key(&String::from_utf8_lossy(&raw)).unwrap(),
            raw
        );
        // 非法
        assert!(decode_cdn_aes_key("abc").is_none());
        assert!(decode_cdn_aes_key("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_none());
    }
}
