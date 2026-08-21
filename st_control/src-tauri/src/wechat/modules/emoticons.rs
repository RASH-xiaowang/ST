//! 表情模块 - 对应 PC 微信「表情管理」
//!
//! 数据来源：`emoticon/emoticon.db`
//! - `kStoreEmoticonPackageTable`   表情商店已下载的表情包
//! - `kStoreEmoticonFilesTable`     表情包内单个表情文件
//! - `kNonStoreEmoticonTable`       自定义（收藏/添加的单个）表情
//! - `kFavEmoticonOrderTable`       收藏表情顺序
//! - `kCustomEmoticonOrderTable`    自定义表情顺序
//!
//! 与 PC 微信一致的逻辑：
//! - 表情包：显示名称、数量、来源
//! - 自定义表情：按添加时间/顺序展示

use super::common;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// 表情包
#[derive(Debug, Clone, Serialize)]
pub struct EmoticonPackage {
    /// 表情包 ID（product_id）
    pub package_id: String,
    /// 名称
    pub name: String,
    /// 表情数量
    pub count: i64,
    /// 状态
    pub status: i64,
    /// 原始数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
}

/// 单个表情
#[derive(Debug, Clone, Serialize)]
pub struct EmoticonItem {
    /// 表情 MD5（唯一标识）
    pub md5: String,
    /// 类型（1=图片 2=GIF 等）
    #[serde(rename = "type")]
    pub item_type: i64,
    /// 大小（字节）
    pub size: i64,
    /// 大小显示
    pub size_label: String,
    /// 所属表情包
    pub package_id: String,
    /// 原始数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
}

/// 表情总览
#[derive(Debug, Serialize)]
pub struct EmoticonOverview {
    pub packages: Vec<EmoticonPackage>,
    pub custom: Vec<EmoticonItem>,
    pub store_files: Vec<EmoticonItem>,
}

/// 本地静态表情包分类
#[derive(Debug, Clone, Serialize)]
pub struct StaticEmoticonCategory {
    pub category: String,
    pub label: String,
    pub files: Vec<StaticEmoticonFile>,
}

/// 本地静态表情文件
#[derive(Debug, Clone, Serialize)]
pub struct StaticEmoticonFile {
    pub name: String,
    pub path: String,
}

/// 本地静态表情清单（随应用打包）
///
/// 资源位于 `public/emoticons/`，由 Vite/Tauri 作为静态资源发布，
/// 前端通过 `/emoticons/{category}/{name}` 直接访问。
/// 清单在编译期嵌入 Rust，避免运行时扫描路径不确定性。
const STATIC_EMOTICON_MANIFEST: &str =
    include_str!("../../../../public/wechat/emoticons/manifest.json");

// ============ 自定义表情图片下载与本地缓存 ============

/// 单次 CDN 下载超时
const EMOTICON_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);

/// 同一 md5 的下载互斥锁，避免并发重复下载
static EMOTICON_LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();

fn emoticon_lock(md5: &str) -> Arc<Mutex<()>> {
    let map = EMOTICON_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
    if guard.len() >= 512 {
        guard.clear();
    }
    guard
        .entry(md5.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// 表情图片本地缓存目录（位于 decoded_image_dir/emoticons）
pub fn emoticon_cache_dir(decoded_image_dir: &Path) -> PathBuf {
    decoded_image_dir.join("emoticons")
}

/// 按 md5 查询自定义表情的可下载 CDN 地址（按优先级排序，依次尝试）
pub fn find_emoticon_urls(decrypted_dir: &Path, md5: &str) -> Result<Vec<String>, String> {
    let db_path = decrypted_dir.join("emoticon").join("emoticon.db");
    if !db_path.exists() {
        return Err(format!("表情数据库未解密: {}", db_path.display()));
    }
    let conn = common::open_readonly_db(&db_path).map_err(|e| format!("打开失败: {}", e))?;
    let Some((cols, rows)) = common::dump_table(&conn, "kNonStoreEmoticonTable", None, 2000) else {
        return Err("表情表不存在".to_string());
    };
    let target = md5.trim().to_lowercase();
    for row in &rows {
        let row_md5 = pick(row, &cols, &["md5", "md5_", "MD5"])
            .map(|v| as_str(&v).to_lowercase())
            .unwrap_or_default();
        if row_md5 != target {
            continue;
        }
        let mut urls: Vec<String> = Vec::new();
        for name in [
            "cdn_url",
            "thumb_url",
            "encrypt_url",
            "extern_url",
            "tp_url",
        ] {
            if let Some(v) = pick(row, &cols, &[name]) {
                let s = as_str(&v);
                if !s.is_empty()
                    && (s.starts_with("http://") || s.starts_with("https://"))
                    && !urls.contains(&s)
                {
                    urls.push(s);
                }
            }
        }
        if urls.is_empty() {
            return Err(format!("表情 {} 没有可用的 CDN 地址", md5));
        }
        return Ok(urls);
    }
    Err(format!("未找到表情 {}", md5))
}

/// 确保表情已下载并缓存到本地，返回缓存文件路径
pub fn ensure_emoticon_cached(
    decrypted_dir: &Path,
    decoded_image_dir: &Path,
    md5: &str,
) -> Result<PathBuf, String> {
    let md5 = md5.trim().to_lowercase();
    if md5.len() != 32 || !md5.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("无效的表情 MD5".to_string());
    }
    let dir = emoticon_cache_dir(decoded_image_dir);
    let cache_path = dir.join(format!("{}.img", md5));
    if is_valid_cache(&cache_path) {
        return Ok(cache_path);
    }
    // 同一 md5 串行下载，避免重复请求
    let lock = emoticon_lock(&md5);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    if is_valid_cache(&cache_path) {
        return Ok(cache_path);
    }
    let urls = find_emoticon_urls(decrypted_dir, &md5)?;
    let mut last_err = "所有地址均不可用".to_string();
    for url in &urls {
        match download_emoticon(url) {
            Ok(bytes) if is_emoticon_image(&bytes) => {
                std::fs::create_dir_all(&dir).map_err(|e| format!("创建缓存目录失败: {}", e))?;
                std::fs::write(&cache_path, &bytes).map_err(|e| format!("写入缓存失败: {}", e))?;
                return Ok(cache_path);
            }
            Ok(_) => last_err = format!("{} 返回非图片内容", url),
            Err(e) => last_err = e,
        }
    }
    Err(format!("表情 {} 下载失败: {}", md5, last_err))
}

fn is_valid_cache(path: &Path) -> bool {
    if let Ok(meta) = std::fs::metadata(path) {
        return meta.len() > 0;
    }
    false
}

fn download_emoticon(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(EMOTICON_DOWNLOAD_TIMEOUT)
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
        )
        .build()
        .map_err(|e| format!("创建下载客户端失败: {}", e))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("请求失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.bytes()
        .map(|b| b.to_vec())
        .map_err(|e| format!("读取响应失败: {}", e))
}

/// 是否为可显示的图片（PNG / GIF / JPEG / WEBP / BMP）
pub fn is_emoticon_image(bytes: &[u8]) -> bool {
    bytes.len() >= 12
        && (bytes.starts_with(b"\x89PNG\r\n\x1a\n")
            || bytes.starts_with(b"GIF87a")
            || bytes.starts_with(b"GIF89a")
            || bytes.starts_with(b"\xff\xd8\xff")
            || (bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP")
            || bytes.starts_with(b"BM"))
}

/// 根据图片 magic 返回 MIME 类型
pub fn detect_emoticon_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") && bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else {
        "image/png"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_detection_works() {
        assert!(is_emoticon_image(b"\x89PNG\r\n\x1a\n....."));
        assert!(is_emoticon_image(b"GIF89a......."));
        assert!(is_emoticon_image(b"\xff\xd8\xff........."));
        assert!(is_emoticon_image(b"RIFF....WEBP "));
        assert!(!is_emoticon_image(b"plain text"));
        assert_eq!(detect_emoticon_mime(b"\x89PNG\r\n\x1a\n....."), "image/png");
        assert_eq!(detect_emoticon_mime(b"GIF89a......."), "image/gif");
        assert_eq!(detect_emoticon_mime(b"RIFF....WEBP "), "image/webp");
        assert_eq!(detect_emoticon_mime(b"\xff\xd8\xff........."), "image/jpeg");
    }

    #[test]
    #[ignore = "需要真实解密库与 CDN 网络"]
    fn download_real_emoticon_roundtrip() {
        let base = crate::common::wechat_data_dir();
        let decrypted = base.join("decrypted");
        let decoded = base.join("decoded_images");
        let db_path = decrypted.join("emoticon").join("emoticon.db");
        if !db_path.exists() {
            eprintln!("跳过：未找到 {}", db_path.display());
            return;
        }
        let conn = common::open_readonly_db(&db_path).expect("打开 emoticon.db");
        let (cols, rows) =
            common::dump_table(&conn, "kNonStoreEmoticonTable", None, 10).expect("dump 表情表");
        let md5 = pick(&rows[0], &cols, &["md5"])
            .map(|v| as_str(&v))
            .unwrap_or_default();
        assert_eq!(md5.len(), 32, "首个表情缺少 md5");

        let urls = find_emoticon_urls(&decrypted, &md5).expect("找到 CDN 地址");
        assert!(!urls.is_empty());

        let cached = ensure_emoticon_cached(&decrypted, &decoded, &md5).expect("下载并缓存表情");
        assert!(cached.is_file());
        let bytes = std::fs::read(&cached).expect("读取缓存");
        assert!(is_emoticon_image(&bytes), "缓存内容应为图片");
    }
}

/// 获取本地静态表情包分类清单
pub fn get_static_emoticons() -> Result<Vec<StaticEmoticonCategory>, String> {
    let map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(STATIC_EMOTICON_MANIFEST)
            .map_err(|e| format!("解析静态表情清单失败: {}", e))?;

    let mut categories = Vec::new();
    let order = [
        ("face", "表情"),
        ("animal", "动物"),
        ("gesture", "手势"),
        ("blessing", "祝福"),
        ("other", "其他"),
    ];

    for (key, label) in order.iter() {
        if let Some(arr) = map.get(*key).and_then(|v| v.as_array()) {
            let files: Vec<StaticEmoticonFile> = arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(|name| StaticEmoticonFile {
                    name: name.to_string(),
                    path: format!("/wechat/emoticons/{}/{}", key, name),
                })
                .collect();
            if !files.is_empty() {
                categories.push(StaticEmoticonCategory {
                    category: key.to_string(),
                    label: label.to_string(),
                    files,
                });
            }
        }
    }

    Ok(categories)
}

/// 从动态列行中按多个候选列名取值
fn pick(row: &[serde_json::Value], cols: &[String], names: &[&str]) -> Option<serde_json::Value> {
    for name in names {
        if let Some(idx) = cols.iter().position(|c| c == name) {
            if let Some(v) = row.get(idx) {
                if !v.is_null() {
                    return Some(v.clone());
                }
            }
        }
    }
    None
}

fn as_str(v: &serde_json::Value) -> String {
    v.as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| v.to_string())
}

fn as_i64(v: &serde_json::Value) -> i64 {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

/// 读取表情库
pub fn get_emoticons(decrypted_dir: &Path) -> Result<EmoticonOverview, String> {
    let db_path = decrypted_dir.join("emoticon").join("emoticon.db");
    if !db_path.exists() {
        return Err(format!("表情数据库未解密: {}", db_path.display()));
    }
    let conn = common::open_readonly_db(&db_path).map_err(|e| format!("打开失败: {}", e))?;

    // 表情包
    let mut packages = Vec::new();
    if let Some((cols, rows)) = common::dump_table(&conn, "kStoreEmoticonPackageTable", None, 500) {
        for row in &rows {
            let package_id = pick(row, &cols, &["product_id_", "package_id_", "id_"])
                .map(|v| as_str(&v))
                .unwrap_or_default();
            let name = pick(row, &cols, &["name_", "title_"])
                .map(|v| as_str(&v))
                .unwrap_or_default();
            let count = pick(row, &cols, &["count_", "num_"])
                .map(|v| as_i64(&v))
                .unwrap_or(0);
            let status = pick(row, &cols, &["status_", "sub_type_"])
                .map(|v| as_i64(&v))
                .unwrap_or(0);
            let mut obj = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                obj.insert(
                    c.clone(),
                    row.get(i).cloned().unwrap_or(serde_json::Value::Null),
                );
            }
            packages.push(EmoticonPackage {
                package_id,
                name,
                count,
                status,
                raw: Some(serde_json::Value::Object(obj)),
            });
        }
    }

    // 自定义表情
    let mut custom = Vec::new();
    if let Some((cols, rows)) = common::dump_table(&conn, "kNonStoreEmoticonTable", None, 1000) {
        for row in &rows {
            let md5 = pick(row, &cols, &["md5_", "md5", "MD5"])
                .map(|v| as_str(&v))
                .unwrap_or_default();
            let item_type = pick(row, &cols, &["type_", "type"])
                .map(|v| as_i64(&v))
                .unwrap_or(0);
            let size = pick(row, &cols, &["size_", "size", "len_"])
                .map(|v| as_i64(&v))
                .unwrap_or(0);
            // 完整原始行：前端需要 description / attachedtext 等字段映射静态表情
            let mut obj = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                obj.insert(
                    c.clone(),
                    row.get(i).cloned().unwrap_or(serde_json::Value::Null),
                );
            }
            custom.push(EmoticonItem {
                md5,
                item_type,
                size,
                size_label: common::format_file_size(size),
                package_id: String::new(),
                raw: Some(serde_json::Value::Object(obj)),
            });
        }
    }

    // 商店表情文件
    let mut store_files = Vec::new();
    if let Some((cols, rows)) = common::dump_table(&conn, "kStoreEmoticonFilesTable", None, 2000) {
        for row in &rows {
            let md5 = pick(row, &cols, &["md5_", "md5"])
                .map(|v| as_str(&v))
                .unwrap_or_default();
            let item_type = pick(row, &cols, &["type_", "type"])
                .map(|v| as_i64(&v))
                .unwrap_or(0);
            let size = pick(row, &cols, &["size_", "size"])
                .map(|v| as_i64(&v))
                .unwrap_or(0);
            let package_id = pick(row, &cols, &["package_id_", "product_id_"])
                .map(|v| as_str(&v))
                .unwrap_or_default();
            let mut obj = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                obj.insert(
                    c.clone(),
                    row.get(i).cloned().unwrap_or(serde_json::Value::Null),
                );
            }
            store_files.push(EmoticonItem {
                md5,
                item_type,
                size,
                size_label: common::format_file_size(size),
                package_id,
                raw: Some(serde_json::Value::Object(obj)),
            });
        }
    }

    Ok(EmoticonOverview {
        packages,
        custom,
        store_files,
    })
}
