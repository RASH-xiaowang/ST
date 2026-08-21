// ============================================================
// 微信图片 .dat 解密 — 加密原语层
// 自 image.rs 拆分：V1/V2/XOR 格式常量、AES/XOR 解密、格式检测
// 与 MD5 解析缓存。
// ============================================================

use aes::cipher::{BlockDecrypt, KeyInit};
use aes::Aes128;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// ============ 常量 ============

const V2_MAGIC: &[u8] = b"\x07\x08V2"; // 前 4 字节快速检测
const V2_MAGIC_FULL: &[u8] = b"\x07\x08V2\x08\x07";
const V1_MAGIC_FULL: &[u8] = b"\x07\x08V1\x08\x07";

// ============ MD5 解析缓存 ============

/// MD5 缓存条目（解析时间 + 图片 MD5）
struct Md5CacheEntry {
    parsed_at: Instant,
    md5: String,
}

/// (username, local_id) → MD5 缓存条目
///
/// 图片消息的 MD5 不会随消息变化，长 TTL 安全。命中后直接走文件解码，
/// 跳过 message_resource.db 的 mtime 检测/全量重解和消息表扫描，
/// 避免微信持续写入资源库时每个图片请求都触发大库解密（图片加载延迟主因）。
static MD5_CACHE: OnceLock<Mutex<HashMap<(String, i64), Md5CacheEntry>>> = OnceLock::new();
const MD5_CACHE_TTL: Duration = Duration::from_secs(1800);
const MD5_CACHE_MAX: usize = 8000;

pub(crate) fn cached_md5(username: &str, local_id: i64) -> Option<String> {
    let cache = MD5_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    let e = guard.get(&(username.to_string(), local_id))?;
    if e.parsed_at.elapsed() > MD5_CACHE_TTL {
        None
    } else {
        Some(e.md5.clone())
    }
}

pub(crate) fn store_md5(username: &str, local_id: i64, md5: String) {
    let cache = MD5_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if guard.len() >= MD5_CACHE_MAX {
        guard.clear();
    }
    guard.insert(
        (username.to_string(), local_id),
        Md5CacheEntry {
            parsed_at: Instant::now(),
            md5,
        },
    );
}

/// V1 固定 AES key (md5("0")[:16])
const V1_AES_KEY: &[u8] = b"cfcd208495d565ef";

/// 常见图片格式的 magic bytes (按长度降序)
const IMAGE_MAGIC: &[(&str, &[u8])] = &[
    ("png", &[0x89, 0x50, 0x4E, 0x47]),
    ("gif", &[0x47, 0x49, 0x46, 0x38]),
    ("tif", &[0x49, 0x49, 0x2A, 0x00]),
    ("webp", &[0x52, 0x49, 0x46, 0x46]),
    ("jpg", &[0xFF, 0xD8, 0xFF]),
];

// ============ 工具函数 ============

/// 检测是否是 V2 加密格式
#[allow(dead_code)] // 供测试与潜在外部调用使用（pub API 保留）
pub fn is_v2_format(data: &[u8]) -> bool {
    data.len() >= 4 && data[..4] == *V2_MAGIC
}

/// AES-ECB 解密单块 (16 字节)
pub(crate) fn aes128_ecb_decrypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    use aes::cipher::generic_array::GenericArray;
    let cipher = Aes128::new_from_slice(key).expect("AES-128 key 长度错误");
    let mut result = Vec::with_capacity(data.len());

    for chunk in data.chunks(16) {
        if chunk.len() < 16 {
            // 最后不足一块，直接追加
            result.extend_from_slice(chunk);
            break;
        }
        let mut ga = GenericArray::clone_from_slice(chunk);
        cipher.decrypt_block(&mut ga);
        result.extend_from_slice(&ga);
    }
    result
}

/// 按密钥长度选择 AES-128/192/256，整段 ECB 解密（微信 CDN 原图格式），
/// 解密后去除 PKCS7 填充。
pub(crate) fn aes_ecb_decrypt_file(key: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    use aes::cipher::generic_array::GenericArray;
    use aes::{Aes128, Aes192, Aes256};

    if data.is_empty() {
        return Err("CDN 图片数据为空".to_string());
    }
    if !data.len().is_multiple_of(16) {
        return Err(format!(
            "CDN 图片密文长度 {} 不是 16 字节整数倍（可能不是 AES 加密数据）",
            data.len()
        ));
    }
    fn ecb_decrypt_blocks<C>(key: &[u8], data: &[u8]) -> Result<Vec<u8>, String>
    where
        C: aes::cipher::BlockDecrypt + aes::cipher::KeyInit,
    {
        let cipher = C::new_from_slice(key).map_err(|_| "AES 密钥长度错误".to_string())?;
        let mut dec = Vec::with_capacity(data.len());
        for chunk in data.chunks(16) {
            let mut ga = GenericArray::clone_from_slice(chunk);
            cipher.decrypt_block(&mut ga);
            dec.extend_from_slice(&ga);
        }
        Ok(dec)
    }

    let dec = match key.len() {
        16 => ecb_decrypt_blocks::<Aes128>(key, data)?,
        24 => ecb_decrypt_blocks::<Aes192>(key, data)?,
        32 => ecb_decrypt_blocks::<Aes256>(key, data)?,
        n => return Err(format!("aeskey 长度 {} 不受支持（需 16/24/32 字节）", n)),
    };
    Ok(pkcs7_unpad(&dec).to_vec())
}

/// 解析图片消息 XML 中的 aeskey：
/// 优先按 hex（32/48/64 字符 → 16/24/32 字节），其次按原始字节长度识别。
/// 注意：32 字符非 hex 串与 hex 候选长度冲突，且真实微信 aeskey 为
/// 32 位 hex（AES-128），因此 32 字符非 hex 串视为非法，避免把
/// zzzz... 之类的垃圾串误判为 AES-256 原始密钥。
pub(crate) fn decode_cdn_aes_key(aeskey: &str) -> Option<Vec<u8>> {
    let trimmed = aeskey.trim();
    if trimmed.len().is_multiple_of(2)
        && trimmed.len() >= 32
        && trimmed.len() <= 64
        && trimmed.chars().all(|c| c.is_ascii_hexdigit())
    {
        if let Ok(bytes) = hex::decode(trimmed) {
            if matches!(bytes.len(), 16 | 24 | 32) {
                return Some(bytes);
            }
        }
    }
    let raw = trimmed.as_bytes();
    if matches!(raw.len(), 16 | 24) {
        return Some(raw.to_vec());
    }
    None
}

/// PKCS7 去填充
fn pkcs7_unpad(data: &[u8]) -> &[u8] {
    if data.is_empty() {
        return data;
    }
    let pad_byte = data[data.len() - 1];
    let pad_len = pad_byte as usize;
    if pad_len == 0 || pad_len > 16 || pad_len > data.len() {
        return data; // 非 PKCS7，原样返回
    }
    if data[data.len() - pad_len..].iter().all(|&b| b == pad_byte) {
        &data[..data.len() - pad_len]
    } else {
        data
    }
}

/// 计算 AES 对齐块大小 (PKCS7)
pub(crate) fn aligned_aes_block_size(aes_size: usize) -> usize {
    if aes_size.is_multiple_of(16) {
        aes_size + 16
    } else {
        aes_size + (16 - aes_size % 16)
    }
}

// ============ 格式检测 ============

/// 通过 XOR 解密后的文件头检测图片格式
pub fn detect_image_format(header: &[u8]) -> &'static str {
    for (fmt, magic) in IMAGE_MAGIC {
        if header.len() >= magic.len() && header[..magic.len()] == **magic {
            return fmt;
        }
    }
    // BMP (2 字节)
    if header.len() >= 2 && header[..2] == [0x42, 0x4D] {
        return "bmp";
    }
    "bin"
}

/// 自动检测 XOR key
pub fn detect_xor_key(data: &[u8]) -> Option<u8> {
    if data.len() < 4 || data[..4] == *V2_MAGIC {
        return None;
    }

    for &(_, magic) in IMAGE_MAGIC {
        let key = data[0] ^ magic[0];
        let mut ok = true;
        for (i, &m) in magic.iter().enumerate() {
            if i >= data.len() {
                break;
            }
            if (data[i] ^ key) != m {
                ok = false;
                break;
            }
        }
        if ok {
            return Some(key);
        }
    }
    None
}

// ============ 解密核心 ============

/// 解密单个 .dat 文件，自动检测格式
///
/// Returns `(output_path, format)` 或错误
pub fn decrypt_dat_file(
    dat_path: &Path,
    out_path: Option<&Path>,
    aes_key: Option<&[u8]>,
    xor_key: u8,
) -> Result<(PathBuf, &'static str), String> {
    let data = std::fs::read(dat_path).map_err(|e| format!("读取文件失败: {}", e))?;
    if data.len() < 6 {
        return Err("文件太小".to_string());
    }

    let sig = &data[..6];

    if sig == V2_MAGIC_FULL || sig == V1_MAGIC_FULL {
        decrypt_v2(
            &data,
            sig == V1_MAGIC_FULL,
            out_path,
            aes_key,
            xor_key,
            dat_path,
        )
    } else {
        decrypt_xor(&data, out_path, dat_path)
    }
}

/// V1/V2 格式解密 (AES-ECB + XOR)
fn decrypt_v2(
    data: &[u8],
    is_v1: bool,
    out_path: Option<&Path>,
    aes_key: Option<&[u8]>,
    xor_key: u8,
    dat_path: &Path,
) -> Result<(PathBuf, &'static str), String> {
    let aes_key = if is_v1 {
        V1_AES_KEY
    } else {
        aes_key.ok_or_else(|| "V2 格式需要 AES key".to_string())?
    };

    if data.len() < 15 {
        return Err("V2 文件头不完整".to_string());
    }

    // 解析 header: [6B sig][4B aes_size LE][4B xor_size LE][1B pad]
    let aes_size = u32::from_le_bytes([data[6], data[7], data[8], data[9]]) as usize;
    let xor_size = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;

    let aligned = aligned_aes_block_size(aes_size);
    let offset = 15;

    if offset + aligned > data.len() {
        return Err("数据不足 AES 块".to_string());
    }

    // AES-ECB 解密
    let aes_data = &data[offset..offset + aligned];
    let dec_aes = aes128_ecb_decrypt(aes_key, aes_data);
    let dec_aes = pkcs7_unpad(&dec_aes);

    let mut offset = offset + aligned;

    // Raw 部分 (不加密)
    let raw_end = data.len() - xor_size;
    let raw_data = if offset < raw_end {
        &data[offset..raw_end]
    } else {
        &[]
    };
    offset = raw_end;

    // XOR 部分
    let xor_data = &data[offset..];
    let dec_xor: Vec<u8> = xor_data.iter().map(|&b| b ^ xor_key).collect();

    // 拼接
    let mut decrypted = Vec::with_capacity(dec_aes.len() + raw_data.len() + dec_xor.len());
    decrypted.extend_from_slice(dec_aes);
    decrypted.extend_from_slice(raw_data);
    decrypted.extend_from_slice(&dec_xor);

    // 检测格式
    let fmt = if decrypted.len() >= 4 && decrypted[..4] == *b"wxgf" {
        "hevc"
    } else {
        let hdr = if decrypted.len() > 16 {
            &decrypted[..16]
        } else {
            &decrypted
        };
        let f = detect_image_format(hdr);
        if f == "bin" {
            return Err("解密后无法识别图片格式 (可能是密钥错误)".to_string());
        }
        f
    };

    let out = resolve_out_path(out_path, dat_path, fmt);
    std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| format!("创建目录失败: {}", e))?;
    std::fs::write(&out, &decrypted).map_err(|e| format!("写入失败: {}", e))?;

    Ok((out, fmt))
}

/// 旧 XOR 格式解密
fn decrypt_xor(
    data: &[u8],
    out_path: Option<&Path>,
    dat_path: &Path,
) -> Result<(PathBuf, &'static str), String> {
    let key = detect_xor_key(data).ok_or("无法检测 XOR key")?;
    let decrypted: Vec<u8> = data.iter().map(|&b| b ^ key).collect();

    let hdr = if decrypted.len() > 16 {
        &decrypted[..16]
    } else {
        &decrypted
    };
    let fmt = detect_image_format(hdr);
    if fmt == "bin" {
        return Err("XOR 解密后无法识别图片格式".to_string());
    }

    let out = resolve_out_path(out_path, dat_path, fmt);
    std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| format!("创建目录失败: {}", e))?;
    std::fs::write(&out, &decrypted).map_err(|e| format!("写入失败: {}", e))?;

    Ok((out, fmt))
}

/// 确定输出路径
fn resolve_out_path(out_path: Option<&Path>, dat_path: &Path, fmt: &str) -> PathBuf {
    let mut name = dat_path.file_stem().unwrap().to_string_lossy().to_string();
    for suffix in &["_t", "_h"] {
        if name.ends_with(suffix) {
            name = name[..name.len() - suffix.len()].to_string();
            break;
        }
    }
    let file_name = format!("{}.{}", name, fmt);
    if let Some(p) = out_path {
        p.join(&file_name)
    } else {
        dat_path.with_file_name(&file_name)
    }
}
