// ============================================================
// 朋友圈图片解密模块 — 视频解析域
// 自 sns_image.rs 拆分：视频下载 + 头部解密 + 缓存。
// ============================================================

use std::path::{Path, PathBuf};
use std::time::Duration;

use md5::{Digest, Md5};

use super::{normalize_cdn_url, Isaac64};

const MAX_VIDEO_BYTES: usize = 200 * 1024 * 1024;
/// 微信朋友圈视频只加密文件前 128KB
const VIDEO_DECRYPT_HEAD: usize = 128 * 1024;

/// 视频缓存文件 key（标准化 URL 的 MD5，用于磁盘缓存与 HTTP 播放路由）
pub fn moment_video_file_key(url: &str) -> String {
    format!("{:x}", Md5::digest(normalize_cdn_url(url, "").as_bytes()))
}

/// 下载并解密朋友圈视频，返回本地 MP4 路径
///
/// 微信朋友圈视频整体下载，但只加密文件**前 128KB**：
/// 用 XML `<enc key="...">` 作为 ISAAC-64 种子解密头部后即成为可播放 MP4。
/// 结果缓存在 `decoded_image_dir/moments_video/<md5(url)>.mp4`。
pub fn resolve_moment_video(
    url: &str,
    key: &str,
    decoded_image_dir: &Path,
) -> Result<PathBuf, String> {
    if url.trim().is_empty() {
        return Err("视频 URL 为空".to_string());
    }
    let cache_dir = decoded_image_dir.join("moments_video");
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("创建视频缓存目录失败: {}", e))?;
    let file_key = moment_video_file_key(url);
    let out = cache_dir.join(format!("{}.mp4", file_key));
    if out.is_file() {
        return Ok(out);
    }

    let target = normalize_cdn_url(url, "");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("MicroMessenger Client")
        // 直连腾讯 CDN：忽略环境/系统代理（本机残留代理配置会导致连接被拒）
        .no_proxy()
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    let resp = client
        .get(&target)
        .send()
        .map_err(|e| format!("下载视频失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("视频下载失败: HTTP {}", resp.status()));
    }

    let tmp = cache_dir.join(format!("{}.{}.tmp", file_key, std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let copied = {
        use std::io::Read;
        let mut file =
            std::fs::File::create(&tmp).map_err(|e| format!("创建临时文件失败: {}", e))?;
        let mut limited = resp.take(MAX_VIDEO_BYTES as u64 + 1);
        let n = std::io::copy(&mut limited, &mut file)
            .map_err(|e| format!("写入视频文件失败: {}", e))?;
        n as usize
    };
    if copied > MAX_VIDEO_BYTES {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("视频过大 ({} bytes)", copied));
    }
    if copied < 12 {
        let _ = std::fs::remove_file(&tmp);
        return Err("视频文件过小".to_string());
    }

    let read_head = |p: &Path| -> Result<[u8; 12], String> {
        use std::io::Read;
        let mut f = std::fs::File::open(p).map_err(|e| format!("读取视频文件失败: {}", e))?;
        let mut head = [0u8; 12];
        f.read_exact(&mut head)
            .map_err(|e| format!("读取视频头部失败: {}", e))?;
        Ok(head)
    };
    let is_mp4 = |head: &[u8; 12]| head.len() >= 8 && &head[4..8] == b"ftyp";

    let mut head = read_head(&tmp)?;
    if !is_mp4(&head) {
        // 解密前 128KB（ISAAC-64 与图片同算法，key 为 <enc key>）
        let key = key.trim();
        if key.is_empty() {
            let _ = std::fs::remove_file(&tmp);
            return Err("视频非 MP4 且缺少解密 key".to_string());
        }
        let seed = key
            .parse::<u64>()
            .map_err(|_| format!("无效的视频解密 key: {}", key))?;
        {
            use std::io::{Read, Seek, SeekFrom, Write};
            let mut f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&tmp)
                .map_err(|e| format!("打开视频文件失败: {}", e))?;
            let n = copied.min(VIDEO_DECRYPT_HEAD);
            let mut buf = vec![0u8; n];
            f.read_exact(&mut buf)
                .map_err(|e| format!("读取视频数据失败: {}", e))?;
            let keystream = Isaac64::new(seed).keystream(n);
            for (b, k) in buf.iter_mut().zip(keystream.iter()) {
                *b ^= k;
            }
            f.seek(SeekFrom::Start(0))
                .map_err(|e| format!("定位视频文件失败: {}", e))?;
            f.write_all(&buf)
                .map_err(|e| format!("写入解密数据失败: {}", e))?;
        }
        head = read_head(&tmp)?;
    }
    if !is_mp4(&head) {
        let _ = std::fs::remove_file(&tmp);
        return Err("解密后无法识别 MP4（key 可能失效）".to_string());
    }

    std::fs::rename(&tmp, &out).map_err(|e| format!("保存视频失败: {}", e))?;
    Ok(out)
}
