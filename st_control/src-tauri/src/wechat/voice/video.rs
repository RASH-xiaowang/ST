// ============================================================
// 微信语音模块 — 视频/封面消息解析
// 自 voice.rs 拆分：视频文件定位（XML/目录索引/hardlink/hash）、
// 封面解析与缓存。
// ============================================================

use crate::wechat::modules::common::{dir_sig, is_month_dir_name, DirFileSigList, DirSig};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use super::message_server_id;

/// 会话消息定位 key（username + local_id）
type UsernameLocalId = (String, i64);
/// 视频目录三签名（视频根 / 封面根 / 消息文件根）
type VideoDirsSig = (DirSig, DirSig, DirSig);
/// 视频文件定位缓存条目（视频/封面 + 目录三签名）
type VideoPathCacheEntry = (VideoFiles, Option<VideoDirsSig>);
/// 封面解析缓存条目（封面路径 + 视频目录扩展签名）
type CoverPathCacheEntry = (std::path::PathBuf, DirFileSigList);
/// 视频目录索引条目（目录签名 + 子目录条目表）
type VideoDirIndexEntry = (
    DirFileSigList,
    std::collections::HashMap<String, VideoDirEntry>,
);

/// 视频文件定位缓存：username+local_id → (视频/封面, 视频目录+hardlink 签名)，变化时失效
static VIDEO_PATH_CACHE: OnceLock<
    Mutex<std::collections::HashMap<UsernameLocalId, VideoPathCacheEntry>>,
> = OnceLock::new();

/// 封面解析缓存：username+local_id → (封面路径, 视频目录扩展签名)
static COVER_PATH_CACHE: OnceLock<
    Mutex<std::collections::HashMap<UsernameLocalId, CoverPathCacheEntry>>,
> = OnceLock::new();

// 消息 local_id → server_id 内存缓存（避免每次扫全部消息分库）
// ============ 视频消息解析（消息 → 附件视频/封面）============

/// 解析结果：视频文件 + 可选封面（`_thumb.jpg` / 同名 jpg）
#[derive(Debug, Clone, Default)]
pub struct VideoFiles {
    pub video: std::path::PathBuf,
    pub thumb: Option<std::path::PathBuf>,
    pub cover: Option<std::path::PathBuf>,
}

/// 消息表按 local_id 取视频消息 XML（type=43），返回 (xml, create_time)
fn message_video_xml(decrypted_dir: &Path, username: &str, local_id: i64) -> Option<(String, i64)> {
    let table = crate::wechat::modules::common::msg_table_name(username);
    let msg_dir = decrypted_dir.join("message");
    let Ok(entries) = std::fs::read_dir(&msg_dir) else {
        return None;
    };
    let mut dbs: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("db")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| {
                        (n.starts_with("message_") || n.starts_with("biz_message_"))
                            && !n.contains("fts")
                            && !n.contains("resource")
                            && !n.contains("media")
                    })
                    .unwrap_or(false)
        })
        .collect();
    dbs.sort();
    for db in dbs {
        let Ok(conn) = Connection::open_with_flags(
            &db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) else {
            continue;
        };
        let sql = format!(
            "SELECT message_content, compress_content, create_time FROM \"{}\" WHERE local_id = ?1 AND local_type = 43 LIMIT 1",
            table
        );
        let row: Option<crate::wechat::modules::common::MediaRow> =
            conn.prepare(&sql).ok().and_then(|mut stmt| {
                stmt.query_row(rusqlite::params![local_id], |r| {
                    Ok(crate::wechat::modules::common::MediaRow(
                        crate::wechat::modules::common::get_bytes(r, 0),
                        crate::wechat::modules::common::get_bytes(r, 1),
                        r.get::<_, i64>(2).unwrap_or(0),
                    ))
                })
                .optional()
                .ok()
                .flatten()
            });
        drop(conn);
        if let Some(crate::wechat::modules::common::MediaRow(c1, c2, create_time)) = row {
            let xml = c1
                .or(c2)
                .map(|b| crate::wechat::modules::common::decode_blob_text(&b))?;
            return Some((xml, create_time));
        }
    }
    None
}

fn xml_md5(xml: &str, tag: &str) -> Option<String> {
    crate::wechat::modules::common::xml_tag_text(xml, tag)
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
}

/// 时间戳 → `YYYY-MM`（用于 hardlink dir1 缺失时按消息时间定位视频目录）
/// 解析视频消息 XML 的 md5 候选（按优先级）
///
/// PC 微信 4.x 的视频消息 `<videomsg>` 里 md5 是规范媒体 md5，
/// 与 `hardlink/hardlink.db` 的 `video_hardlink_info_v4.md5` 一致；
/// newmd5 / rawmd5 / originsourcemd5 是转码/原始文件变体。
fn video_md5_candidates(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |s: Option<String>| {
        if let Some(s) = s {
            if !out.contains(&s) {
                out.push(s);
            }
        }
    };
    // 属性优先（4.x 主流格式：<videomsg md5="..." newmd5="..." rawmd5="...">）
    push(videomsg_attr(xml, "md5"));
    push(videomsg_attr(xml, "newmd5"));
    push(videomsg_attr(xml, "rawmd5"));
    push(videomsg_attr(xml, "originsourcemd5"));
    // 文本标签兜底（旧格式 / appmsg 内嵌 videomsg）
    push(xml_md5(xml, "videomd5"));
    push(xml_md5(xml, "videofilemd5"));
    out
}

/// 从 `<videomsg ...>` 取属性（PC 微信 4.x 的 md5 是属性）
fn videomsg_attr(xml: &str, attr: &str) -> Option<String> {
    crate::wechat::modules::common::xml_tag_attr(xml, "videomsg", attr)
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
}

/// 归一化视频查找键：去扩展名 / `_thumb` 后缀 / `_raw` 变体（对齐 WeChatDataAnalysis）
fn normalize_video_key(value: &str) -> Option<String> {
    let mut text = value.trim().to_lowercase();
    if text.is_empty() {
        return None;
    }
    // 取路径最后一段
    if let Some(idx) = text.rfind(['\\', '/']) {
        text = text[idx + 1..].to_string();
    }
    // 去扩展名
    for ext in [
        "mp4", "m4v", "mov", "avi", "mkv", "flv", "jpg", "jpeg", "png", "gif", "webp", "dat",
    ] {
        if let Some(stripped) = text.strip_suffix(&format!(".{}", ext)) {
            text = stripped.to_string();
            break;
        }
    }
    if let Some(stripped) = text.strip_suffix("_thumb") {
        text = stripped.to_string();
    }
    if text.is_empty() {
        return None;
    }
    // 16-64 位 hex 直接归一化；否则提取首个 32 位 hex 段
    let is_hex = |s: &str| s.chars().all(|c| c.is_ascii_hexdigit());
    let raw = text.ends_with("_raw");
    let core = if raw {
        &text[..text.len() - 4]
    } else {
        &text[..]
    };
    if is_hex(core) && (16..=64).contains(&core.len()) {
        return Some(format!(
            "{}{}",
            core.to_lowercase(),
            if raw { "_raw" } else { "" }
        ));
    }
    if let Some(start) = find_hex32(core) {
        return Some(start.to_lowercase());
    }
    Some(text)
}

fn find_hex32(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 32 <= bytes.len() {
        let slice = &s[i..i + 32];
        if slice.chars().all(|c| c.is_ascii_hexdigit()) {
            // 确保不是更长 hex 串的中间段（后面不是 hex）
            let after = s[i + 32..].chars().next();
            if after.is_none_or(|c| !c.is_ascii_hexdigit()) {
                return Some(slice);
            }
        }
        i += 1;
    }
    None
}

/// `msg/video` 目录索引条目
#[derive(Debug, Clone, Default)]
struct VideoDirEntry {
    video: Option<std::path::PathBuf>,
    thumb: Option<std::path::PathBuf>,
    cover: Option<std::path::PathBuf>,
}

/// 视频目录索引缓存：base_dir → (目录签名, 索引)
static VIDEO_DIR_INDEX: OnceLock<
    Mutex<std::collections::HashMap<std::path::PathBuf, VideoDirIndexEntry>>,
> = OnceLock::new();

/// 扩展目录签名：根目录 + 各月份子目录的 (名称, mtime, 条目数)。
/// 新视频下载进已有月份目录时根目录 mtime 不变，必须看子目录签名才能失效。
fn video_root_sig(video_root: &Path) -> DirFileSigList {
    let mut sigs = vec![(".".to_string(), dir_sig(video_root)?)];
    if let Ok(entries) = std::fs::read_dir(video_root) {
        let mut months: Vec<(String, DirSig)> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let name = path.file_name().and_then(|n| n.to_str())?.to_string();
                if path.is_dir() && is_month_dir_name(&name) {
                    dir_sig(&path).map(|s| (name, s))
                } else {
                    None
                }
            })
            .collect();
        months.sort();
        sigs.extend(months);
    }
    Some(sigs)
}

/// 构建 `msg/video`（含 `YYYY-MM` 子目录）的文件索引，带签名缓存
fn video_dir_index(wechat_base_dir: &Path) -> std::collections::HashMap<String, VideoDirEntry> {
    let video_root = wechat_base_dir.join("msg").join("video");
    let sig = video_root_sig(&video_root);
    let cache = VIDEO_DIR_INDEX.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    {
        if let Ok(guard) = cache.lock() {
            if let Some((saved_sig, index)) = guard.get(&video_root) {
                if *saved_sig == sig {
                    return index.clone();
                }
            }
        }
    }

    let mut index: std::collections::HashMap<String, VideoDirEntry> =
        std::collections::HashMap::new();
    let mut scan_dirs: Vec<std::path::PathBuf> = vec![video_root.clone()];
    if let Ok(entries) = std::fs::read_dir(&video_root) {
        let mut months: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(is_month_dir_name)
                        .unwrap_or(false)
            })
            .collect();
        months.sort();
        scan_dirs.extend(months);
    }
    for dir in scan_dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let lower = name.to_lowercase();
            let is_video =
                lower.ends_with(".mp4") || lower.ends_with(".m4v") || lower.ends_with(".mov");
            let is_image = lower.ends_with(".jpg")
                || lower.ends_with(".jpeg")
                || lower.ends_with(".png")
                || lower.ends_with(".webp");
            if !is_video && !is_image {
                continue;
            }
            let is_thumb = lower.contains("_thumb");
            let stem = if is_video {
                lower
                    .trim_end_matches(".mp4")
                    .trim_end_matches(".m4v")
                    .trim_end_matches(".mov")
            } else if is_thumb {
                lower
                    .trim_end_matches(".jpg")
                    .trim_end_matches(".jpeg")
                    .trim_end_matches(".png")
                    .trim_end_matches(".webp")
                    .trim_end_matches("_thumb")
            } else {
                lower
                    .trim_end_matches(".jpg")
                    .trim_end_matches(".jpeg")
                    .trim_end_matches(".png")
                    .trim_end_matches(".webp")
            };
            let Some(key) = normalize_video_key(stem) else {
                continue;
            };
            let entry = index.entry(key.clone()).or_default();
            if is_video {
                if entry.video.is_none() {
                    entry.video = Some(path.clone());
                }
            } else if is_thumb {
                if entry.thumb.is_none() {
                    entry.thumb = Some(path.clone());
                }
            } else if entry.cover.is_none() {
                entry.cover = Some(path.clone());
            }
            // `_raw` 变体归一到同一 key
            if let Some(stripped) = key.strip_suffix("_raw") {
                let e = index.entry(stripped.to_string()).or_default();
                if is_video && e.video.is_none() {
                    e.video = Some(path.clone());
                }
            } else {
                let e = index.entry(format!("{}_raw", key)).or_default();
                if is_video && e.video.is_none() {
                    e.video = Some(path.clone());
                }
            }
        }
    }
    if let Ok(mut guard) = cache.lock() {
        guard.insert(video_root, (sig, index.clone()));
    }
    index
}

/// 通过解密后的 `hardlink/hardlink.db` 把 XML md5 映射为本地文件名 + 月份目录
fn hardlink_video_lookup(decrypted_dir: &Path, md5: &str) -> Vec<(String, Option<String>)> {
    let mut results: Vec<(String, Option<String>)> = Vec::new();
    let db_path = decrypted_dir.join("hardlink").join("hardlink.db");
    if !db_path.is_file() {
        return results;
    }
    let Ok(conn) = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return results;
    };
    // md5 精确匹配优先，file_name 含 md5 兜底（部分构建文件名即 md5）
    for sql in [
        "SELECT file_name, dir1 FROM video_hardlink_info_v4 WHERE lower(md5) = ?1 ORDER BY modify_time DESC LIMIT 8".to_string(),
        "SELECT file_name, dir1 FROM video_hardlink_info_v4 WHERE lower(file_name) LIKE ?1 ORDER BY modify_time DESC LIMIT 8".to_string(),
    ] {
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let like = format!("%{}%", md5);
            let rows = stmt
                .query_map(rusqlite::params![if sql.contains("LIKE") { like.as_str() } else { md5 }], |r| {
                    Ok((r.get::<_, String>(0).unwrap_or_default(), r.get::<_, Option<i64>>(1).ok().flatten()))
                })
                .ok();
            if let Some(rows) = rows {
                for row in rows.flatten() {
                    let month = row.1.and_then(|dir_id| {
                        conn.query_row(
                            "SELECT username FROM dir2id WHERE rowid = ?1",
                            rusqlite::params![dir_id],
                            |r| r.get::<_, String>(0),
                        )
                        .ok()
                    })
                    .filter(|m| is_month_dir_name(m));
                    let item = (row.0, month);
                    if !results.contains(&item) {
                        results.push(item);
                    }
                }
            }
        }
    }
    drop(conn);
    results
}

/// 快速探测：在 `msg/video` 及月份目录里按精确文件名匹配
fn fast_probe_video(
    wechat_base_dir: &Path,
    md5: &str,
    want_thumb: bool,
) -> Option<std::path::PathBuf> {
    let video_root = wechat_base_dir.join("msg").join("video");
    let mut dirs = vec![video_root.clone()];
    if let Ok(entries) = std::fs::read_dir(&video_root) {
        let mut months: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(is_month_dir_name)
                        .unwrap_or(false)
            })
            .collect();
        months.sort_by(|a, b| b.cmp(a)); // 新月份优先
        dirs.extend(months);
    }
    let names: Vec<String> = if want_thumb {
        ["jpg", "jpeg", "png", "webp", "dat"]
            .iter()
            .flat_map(|ext| vec![format!("{}_thumb.{}", md5, ext), format!("{}.{}", md5, ext)])
            .collect()
    } else {
        ["mp4", "m4v", "mov", "dat"]
            .iter()
            .map(|ext| format!("{}.{}", md5, ext))
            .collect()
    };
    for dir in dirs {
        for name in &names {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// 递归扫描 `msg/video` 找文件名包含 md5 的文件（最后兜底）
fn scan_video_by_md5(
    wechat_base_dir: &Path,
    md5: &str,
    want_thumb: bool,
) -> Option<std::path::PathBuf> {
    let video_root = wechat_base_dir.join("msg").join("video");
    if !video_root.is_dir() {
        return None;
    }
    let mut stack = vec![video_root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let lower = name.to_lowercase();
            let ok_ext = if want_thumb {
                lower.ends_with(".jpg")
                    || lower.ends_with(".jpeg")
                    || lower.ends_with(".png")
                    || lower.ends_with(".webp")
            } else {
                lower.ends_with(".mp4") || lower.ends_with(".m4v") || lower.ends_with(".mov")
            };
            if ok_ext && lower.contains(md5) {
                if want_thumb && !lower.contains("_thumb") {
                    continue;
                }
                return Some(path);
            }
        }
    }
    None
}

/// 由索引/探测/hardlink 组合解析 (video, thumb, cover)
fn resolve_video_paths_uncached(
    wechat_base_dir: &Path,
    decrypted_dir: &Path,
    md5: &str,
    create_time: i64,
) -> Option<VideoFiles> {
    let index = video_dir_index(wechat_base_dir);

    // 1) hardlink.db 权威映射：XML md5 → 本地文件名（+ 月份目录）
    for (file_name, month) in hardlink_video_lookup(decrypted_dir, md5) {
        let norm = normalize_video_key(&file_name);
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Some(m) = &month {
            let month_dir = wechat_base_dir.join("msg").join("video").join(m);
            candidates.push(month_dir.join(&file_name));
        }
        if let Some(n) = &norm {
            if let Some(entry) = index.get(n) {
                if let Some(v) = &entry.video {
                    candidates.push(v.clone());
                }
            }
            // 文件名带 `_raw` 时，本地可能只存规范名
            if let Some(stripped) = n.strip_suffix("_raw") {
                if let Some(entry) = index.get(stripped) {
                    if let Some(v) = &entry.video {
                        candidates.push(v.clone());
                    }
                }
            }
        }
        // 消息时间所在月份兜底（hardlink dir1 缺失/过期时）
        let msg_month = crate::wechat::modules::common::month_of(create_time);
        if !msg_month.is_empty() {
            let month = msg_month.clone();
            candidates.push(
                wechat_base_dir
                    .join("msg")
                    .join("video")
                    .join(&month)
                    .join(&file_name),
            );
        }
        for cand in candidates {
            if cand.is_file() {
                let stem = cand.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let dir = cand.parent().unwrap_or(Path::new("."));
                let thumb = if stem.ends_with("_raw") {
                    dir.join(format!("{}_thumb.jpg", stem.trim_end_matches("_raw")))
                } else {
                    dir.join(format!("{}_thumb.jpg", stem))
                };
                let cover = if stem.ends_with("_raw") {
                    dir.join(format!("{}.jpg", stem.trim_end_matches("_raw")))
                } else {
                    dir.join(format!("{}.jpg", stem))
                };
                return Some(VideoFiles {
                    video: cand,
                    thumb: thumb.is_file().then_some(thumb),
                    cover: cover.is_file().then_some(cover),
                });
            }
        }
    }

    // 2) 索引按 md5 直查（部分构建本地文件名即 XML md5）
    let mut keys: Vec<String> = vec![md5.to_string()];
    if let Some(stripped) = md5.strip_suffix("_raw") {
        keys.push(stripped.to_string());
    } else {
        keys.push(format!("{}_raw", md5));
    }
    for key in &keys {
        if let Some(entry) = index.get(key) {
            if let Some(v) = &entry.video {
                return Some(VideoFiles {
                    video: v.clone(),
                    thumb: entry.thumb.clone(),
                    cover: entry.cover.clone(),
                });
            }
        }
    }

    // 3) 精确文件名探测
    if let Some(v) = fast_probe_video(wechat_base_dir, md5, false) {
        let stem = v
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let dir = v.parent().unwrap_or(Path::new("."));
        let base_stem = stem.trim_end_matches("_raw").to_string();
        let thumb = dir.join(format!("{}_thumb.jpg", base_stem));
        let cover = dir.join(format!("{}.jpg", base_stem));
        return Some(VideoFiles {
            video: v,
            thumb: thumb.is_file().then_some(thumb),
            cover: cover.is_file().then_some(cover),
        });
    }

    // 4) 递归扫描兜底
    if let Some(v) = scan_video_by_md5(wechat_base_dir, md5, false) {
        let stem = v
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let dir = v.parent().unwrap_or(Path::new("."));
        let base_stem = stem.trim_end_matches("_raw").to_string();
        let thumb = dir.join(format!("{}_thumb.jpg", base_stem));
        let cover = dir.join(format!("{}.jpg", base_stem));
        return Some(VideoFiles {
            video: v,
            thumb: thumb.is_file().then_some(thumb),
            cover: cover.is_file().then_some(cover),
        });
    }
    None
}

/// 从 `message_resource.db` 取消息关联的本地资源 hash。
///
/// `MessageResourceInfo.packed_info` 里存的是本地文件名 hash（如
/// `\x12"\n <32hex>`），是「消息 → 本地视频文件」的权威映射，
/// 比 XML md5 更可靠（新版本微信的 XML md5 与 hardlink md5 常不一致）。
fn resource_video_hash(decrypted_dir: &Path, username: &str, local_id: i64) -> Option<String> {
    let db_path = decrypted_dir.join("message").join("message_resource.db");
    if !db_path.is_file() {
        return None;
    }
    let conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let chat_id: Option<i64> = conn
        .prepare("SELECT rowid FROM ChatName2Id WHERE user_name = ?1")
        .ok()
        .and_then(|mut stmt| {
            stmt.query_row(rusqlite::params![username], |r| r.get::<_, i64>(0))
                .optional()
                .ok()
                .flatten()
        });
    let extract = |packed: Vec<u8>| -> Option<String> {
        let text = String::from_utf8_lossy(&packed);
        // 取首个 32 位十六进制段
        let mut cur = String::new();
        for ch in text.chars() {
            if ch.is_ascii_hexdigit() {
                cur.push(ch);
                if cur.len() == 32 {
                    return Some(cur);
                }
            } else {
                cur.clear();
            }
        }
        None
    };
    let mut hash = None;
    if let Some(cid) = chat_id {
        hash = conn
            .prepare(
                "SELECT packed_info FROM MessageResourceInfo WHERE chat_id = ?1 AND message_local_id = ?2 AND message_local_type = 43 LIMIT 1",
            )
            .ok()
            .and_then(|mut stmt| {
                stmt.query_row(rusqlite::params![cid, local_id], |r| {
                    Ok::<_, rusqlite::Error>(
                        crate::wechat::modules::common::get_bytes(r, 0).unwrap_or_default(),
                    )
                })
                .ok()
            })
            .and_then(extract);
    }
    // svr_id 兜底（chat_id 缺失/未匹配时）
    if hash.is_none() {
        if let Some(svr_id) = message_server_id(decrypted_dir, username, local_id) {
            hash = conn
                .prepare(
                    "SELECT packed_info FROM MessageResourceInfo WHERE message_svr_id = ?1 AND message_local_type = 43 LIMIT 1",
                )
                .ok()
                .and_then(|mut stmt| {
                    stmt.query_row(rusqlite::params![svr_id], |r| {
                        Ok::<_, rusqlite::Error>(
                            crate::wechat::modules::common::get_bytes(r, 0).unwrap_or_default(),
                        )
                    })
                    .ok()
                })
                .and_then(extract);
        }
    }
    drop(conn);
    hash
}

/// 按资源 hash（本地文件名 stem）定位视频文件 + 同 stem 封面
fn resolve_video_paths_by_hash(wechat_base_dir: &Path, hash: &str) -> Option<VideoFiles> {
    let index = video_dir_index(wechat_base_dir);
    let mut keys = vec![hash.to_string()];
    if let Some(stripped) = hash.strip_suffix("_raw") {
        keys.push(stripped.to_string());
    } else {
        keys.push(format!("{}_raw", hash));
    }
    for key in &keys {
        if let Some(entry) = index.get(key) {
            if let Some(v) = &entry.video {
                return Some(VideoFiles {
                    video: v.clone(),
                    thumb: entry.thumb.clone(),
                    cover: entry.cover.clone(),
                });
            }
        }
    }
    if let Some(v) = fast_probe_video(wechat_base_dir, hash, false) {
        let stem = v
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let dir = v.parent().unwrap_or(Path::new("."));
        let base_stem = stem.trim_end_matches("_raw").to_string();
        let thumb = dir.join(format!("{}_thumb.jpg", base_stem));
        let cover = dir.join(format!("{}.jpg", base_stem));
        return Some(VideoFiles {
            video: v,
            thumb: thumb.is_file().then_some(thumb),
            cover: cover.is_file().then_some(cover),
        });
    }
    if let Some(v) = scan_video_by_md5(wechat_base_dir, hash, false) {
        let stem = v
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let dir = v.parent().unwrap_or(Path::new("."));
        let base_stem = stem.trim_end_matches("_raw").to_string();
        let thumb = dir.join(format!("{}_thumb.jpg", base_stem));
        let cover = dir.join(format!("{}.jpg", base_stem));
        return Some(VideoFiles {
            video: v,
            thumb: thumb.is_file().then_some(thumb),
            cover: cover.is_file().then_some(cover),
        });
    }
    None
}

/// 缩略图索引：按 mtime 排序 + 按大小分组（封面查找 O(log N)，13k 文件只建一次）
#[derive(Clone, Default)]
struct ThumbIndex {
    by_mtime: Vec<(std::path::PathBuf, SystemTime, u64)>,
    by_size: std::collections::HashMap<u64, Vec<(std::path::PathBuf, SystemTime)>>,
}

/// 缩略图索引缓存：video_root → (扩展签名, 索引)
static THUMB_FILE_CACHE: OnceLock<
    Mutex<std::collections::HashMap<std::path::PathBuf, (DirFileSigList, ThumbIndex)>>,
> = OnceLock::new();

/// 收集 `msg/video` 下全部 `*_thumb.*` 文件并建索引（带签名缓存）
fn thumb_index(wechat_base_dir: &Path) -> ThumbIndex {
    let video_root = wechat_base_dir.join("msg").join("video");
    let sig = video_root_sig(&video_root);
    let cache = THUMB_FILE_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    {
        if let Ok(guard) = cache.lock() {
            if let Some((saved_sig, idx)) = guard.get(&video_root) {
                if *saved_sig == sig {
                    return idx.clone();
                }
            }
        }
    }
    let mut idx = ThumbIndex::default();
    let mut stack = vec![video_root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let low = name.to_lowercase();
            if low.contains("_thumb")
                && (low.ends_with(".jpg")
                    || low.ends_with(".jpeg")
                    || low.ends_with(".png")
                    || low.ends_with(".webp"))
            {
                if let (Ok(md), Ok(mt)) = (
                    std::fs::metadata(&p),
                    std::fs::metadata(&p).and_then(|m| m.modified()),
                ) {
                    let sz = md.len();
                    idx.by_mtime.push((p.clone(), mt, sz));
                    idx.by_size.entry(sz).or_default().push((p, mt));
                }
            }
        }
    }
    idx.by_mtime.sort_by_key(|a| a.1);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(video_root, (sig, idx.clone()));
    }
    idx
}

/// 在按 mtime 排序的缩略图里找消息时间 ±窗口内的最近文件（二分 + 邻域扫描）
fn nearest_thumb_by_time(
    by_mtime: &[(std::path::PathBuf, SystemTime, u64)],
    create_time: i64,
    window: i64,
    want_size: Option<u64>,
) -> Option<std::path::PathBuf> {
    let target = create_time;
    let secs_of = |t: SystemTime| -> i64 {
        t.duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    };
    let mut lo = 0usize;
    let mut hi = by_mtime.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if secs_of(by_mtime[mid].1) < target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let mut best: Option<(std::path::PathBuf, i64)> = None;
    // 向左右扩展检查窗口内文件
    let mut i = lo;
    while i < by_mtime.len() {
        let dt = (secs_of(by_mtime[i].1) - target).abs();
        if dt > window {
            break;
        }
        if want_size.is_none_or(|sz| by_mtime[i].2 == sz)
            && best.as_ref().map(|(_, s)| dt < *s).unwrap_or(true)
        {
            best = Some((by_mtime[i].0.clone(), dt));
        }
        i += 1;
    }
    let mut i = lo as i64 - 1;
    while i >= 0 {
        let idx = i as usize;
        let dt = (secs_of(by_mtime[idx].1) - target).abs();
        if dt > window {
            break;
        }
        if want_size.is_none_or(|sz| by_mtime[idx].2 == sz)
            && best.as_ref().map(|(_, s)| dt < *s).unwrap_or(true)
        {
            best = Some((by_mtime[idx].0.clone(), dt));
        }
        i -= 1;
    }
    best.map(|(p, _)| p)
}

/// 独立解析封面：不依赖视频文件是否存在。
///
/// 优先级：视频同 stem 封面 → 资源库 hash 封面 → cdnthumblength 大小+时间精确匹配
/// → 消息时间最近邻缩略图。
fn resolve_message_cover_impl(
    wechat_base_dir: &Path,
    decrypted_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<std::path::PathBuf> {
    let cache_key = (username.to_string(), local_id);
    let sig = video_root_sig(&wechat_base_dir.join("msg").join("video"));
    let cache = COVER_PATH_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some((path, saved_sig)) = guard.get(&cache_key) {
            if *saved_sig == sig && path.is_file() {
                return Some(path.clone());
            }
        }
    }

    let (xml, create_time) = message_video_xml(decrypted_dir, username, local_id)?;
    let mut result = None;
    // 1) 视频文件同 stem 封面（视频已下载时）
    if let Some(files) =
        resolve_message_video_files_impl(wechat_base_dir, decrypted_dir, username, local_id)
    {
        result = files.thumb.or(files.cover);
    }
    // 2) 资源库 hash → 同 stem 封面
    if result.is_none() {
        if let Some(hash) = resource_video_hash(decrypted_dir, username, local_id) {
            let index = video_dir_index(wechat_base_dir);
            if let Some(entry) = index.get(&hash) {
                result = entry.thumb.clone().or_else(|| entry.cover.clone());
            }
            if result.is_none() {
                if let Some(v) = fast_probe_video(wechat_base_dir, &hash, true) {
                    result = Some(v);
                }
            }
        }
    }
    // 3) cdnthumblength 大小 + 消息时间精确匹配（缩略图在收消息时同秒生成）
    if result.is_none() {
        let want_size = videomsg_attr(&xml, "cdnthumblength")
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|s| *s > 0);
        let idx = thumb_index(wechat_base_dir);
        if let Some(sz) = want_size {
            result = idx.by_size.get(&sz).and_then(|list| {
                let target = create_time;
                let secs_of = |t: SystemTime| -> i64 {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0)
                };
                list.iter()
                    .filter(|(_, mt)| (secs_of(*mt) - target).abs() <= 600)
                    .min_by_key(|(_, mt)| (secs_of(*mt) - target).abs())
                    .map(|(p, _)| p.clone())
            });
        }
        // 4) 消息时间最近邻缩略图（±10 分钟，二分查找）
        if result.is_none() {
            result = nearest_thumb_by_time(&idx.by_mtime, create_time, 600, None);
        }
    }
    if let Some(p) = result.clone() {
        if let Ok(mut guard) = cache.lock() {
            guard.insert(cache_key, (p, sig));
        }
    }
    result
}

/// 完整解析：XML md5 → 本地视频/封面，带 (username, local_id) 结果缓存
fn resolve_message_video_files_impl(
    wechat_base_dir: &Path,
    decrypted_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<VideoFiles> {
    let cache_key = (username.to_string(), local_id);
    // 失效签名：视频目录 + hardlink.db
    let video_root = wechat_base_dir.join("msg").join("video");
    let sig_video = dir_sig(&video_root);
    let sig_hardlink = dir_sig(&decrypted_dir.join("hardlink").join("hardlink.db"));
    let sig_resource = dir_sig(&decrypted_dir.join("message").join("message_resource.db"));
    let sig = sig_video
        .zip(sig_hardlink)
        .zip(sig_resource)
        .map(|((a, b), c)| (a, b, c));
    let cache = VIDEO_PATH_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some((files, saved_sig)) = guard.get(&cache_key) {
            if *saved_sig == sig && files.video.is_file() {
                return Some(files.clone());
            }
        }
    }

    let (xml, create_time) = message_video_xml(decrypted_dir, username, local_id)?;
    let mut result = None;
    // 1) message_resource.db 权威映射（带索引的快速查询，XML md5 与 hardlink 失配时仍可用）
    if result.is_none() {
        if let Some(hash) = resource_video_hash(decrypted_dir, username, local_id) {
            result = resolve_video_paths_by_hash(wechat_base_dir, &hash);
        }
    }
    // 2) XML md5 → hardlink（老数据/资源库缺失时兜底）
    if result.is_none() {
        for md5 in video_md5_candidates(&xml) {
            if let Some(files) =
                resolve_video_paths_uncached(wechat_base_dir, decrypted_dir, &md5, create_time)
            {
                result = Some(files);
                break;
            }
        }
    }
    if let Some(files) = result.clone() {
        if let Ok(mut guard) = cache.lock() {
            guard.insert(cache_key, (files, sig));
        }
    }
    result
}

/// 定位视频消息的本地视频文件（兼容旧签名；优先 `resolve_message_video_files`）
pub fn resolve_message_video_file(
    wechat_base_dir: &Path,
    decrypted_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<std::path::PathBuf> {
    resolve_message_video_files_impl(wechat_base_dir, decrypted_dir, username, local_id)
        .map(|f| f.video)
}

/// 定位视频消息的封面缩略图（`<file>_thumb.jpg`，回退同名 jpg）
pub fn resolve_message_video_thumb(
    wechat_base_dir: &Path,
    decrypted_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<std::path::PathBuf> {
    resolve_message_cover_impl(wechat_base_dir, decrypted_dir, username, local_id)
}

/// 取消息语音并解码为 WAV（浏览器可播）
#[cfg(test)]
mod tests {
    use super::*;

    /// 验证视频文件定位（真实数据）
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_video_resolve() {
        let Some(cfg) = crate::wechat::config::WeChatConfig::load().ok() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let usernames = crate::wechat::annual::load_session_usernames(&cfg.decrypted_dir);
        let msg_dir = cfg.decrypted_dir.join("message");
        let mut resolved = 0usize;
        let mut thumb_ok = 0usize;
        let mut scanned = 0usize;
        if let Ok(entries) = std::fs::read_dir(&msg_dir) {
            let mut dbs: Vec<std::path::PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().and_then(|e| e.to_str()) == Some("db")
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| {
                                (n.starts_with("message_") || n.starts_with("biz_message_"))
                                    && !n.contains("fts")
                                    && !n.contains("resource")
                                    && !n.contains("media")
                            })
                            .unwrap_or(false)
                })
                .collect();
            dbs.sort();
            'outer: for username in &usernames {
                let table = crate::wechat::modules::common::msg_table_name(username);
                for db in &dbs {
                    let Ok(conn) = Connection::open_with_flags(
                        db,
                        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                    ) else {
                        continue;
                    };
                    let sql = format!(
                        "SELECT local_id FROM \"{}\" WHERE local_type = 43 ORDER BY create_time DESC LIMIT 200",
                        table
                    );
                    let lids: Vec<i64> = conn
                        .prepare(&sql)
                        .ok()
                        .and_then(|mut stmt| {
                            stmt.query_map([], |r| r.get::<_, i64>(0))
                                .ok()
                                .map(|rows| rows.flatten().collect())
                        })
                        .unwrap_or_default();
                    drop(conn);
                    for lid in lids {
                        scanned += 1;
                        if let Some(files) = resolve_message_video_files_impl(
                            &cfg.wechat_base_dir,
                            &cfg.decrypted_dir,
                            username,
                            lid,
                        ) {
                            let p = &files.video;
                            println!(
                                "视频 username={} local_id={} → {}（{} 字节）thumb={}",
                                username,
                                lid,
                                p.display(),
                                std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0),
                                files
                                    .thumb
                                    .as_ref()
                                    .map(|t| t.display().to_string())
                                    .unwrap_or_else(|| "无".to_string()),
                            );
                            if files.thumb.is_some() || files.cover.is_some() {
                                thumb_ok += 1;
                            }
                            resolved += 1;
                            if resolved >= 3 {
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }
        println!(
            "扫描 {} 条视频消息，定位到 {} 个本地视频文件（含封面 {} 个）",
            scanned, resolved, thumb_ok
        );
        if scanned == 0 {
            eprintln!("无视频消息，跳过");
            return;
        }
        if resolved == 0 {
            eprintln!("存在视频消息但本地均无缓存文件（可接受，视频未下载），跳过");
            return;
        }
    }

    /// 验证两条新路径：资源库 hash 解析视频；无视频文件的消息仍能解析封面
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_video_cover_and_resource() {
        let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let usernames = crate::wechat::annual::load_session_usernames(&cfg.decrypted_dir);
        let msg_dir = cfg.decrypted_dir.join("message");
        let mut with_video = 0usize;
        let mut covers = 0usize;
        let mut resource_only = 0usize;
        let mut processed = 0usize;
        let mut sample: Option<(String, i64, String)> = None;
        if let Ok(entries) = std::fs::read_dir(&msg_dir) {
            let mut dbs: Vec<std::path::PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().and_then(|e| e.to_str()) == Some("db")
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| {
                                (n.starts_with("message_") || n.starts_with("biz_message_"))
                                    && !n.contains("fts")
                                    && !n.contains("resource")
                                    && !n.contains("media")
                            })
                            .unwrap_or(false)
                })
                .collect();
            dbs.sort();
            'outer: for username in &usernames {
                let table = crate::wechat::modules::common::msg_table_name(username);
                for db in &dbs {
                    let Ok(conn) = Connection::open_with_flags(
                        db,
                        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                    ) else {
                        continue;
                    };
                    let sql = format!(
                        "SELECT local_id FROM \"{}\" WHERE local_type = 43 ORDER BY create_time DESC LIMIT 30",
                        table
                    );
                    let lids: Vec<i64> = conn
                        .prepare(&sql)
                        .ok()
                        .and_then(|mut stmt| {
                            stmt.query_map([], |r| r.get::<_, i64>(0))
                                .ok()
                                .map(|rows| rows.flatten().collect())
                        })
                        .unwrap_or_default();
                    drop(conn);
                    for lid in lids {
                        processed += 1;
                        let video = resolve_message_video_file(
                            &cfg.wechat_base_dir,
                            &cfg.decrypted_dir,
                            username,
                            lid,
                        );
                        let thumb = resolve_message_video_thumb(
                            &cfg.wechat_base_dir,
                            &cfg.decrypted_dir,
                            username,
                            lid,
                        );
                        if video.is_some() {
                            with_video += 1;
                            let xml = message_video_xml(&cfg.decrypted_dir, username, lid);
                            let md5_hit = xml.as_ref().map(|(x, _)| {
                                video_md5_candidates(x).iter().any(|m| {
                                    hardlink_video_lookup(&cfg.decrypted_dir, m).iter().any(
                                        |(f, _)| {
                                            let stem = normalize_video_key(f).unwrap_or_default();
                                            video
                                                .as_ref()
                                                .map(|v| {
                                                    v.file_stem()
                                                        .and_then(|s| s.to_str())
                                                        .map(|s| {
                                                            s.to_lowercase()
                                                                .trim_end_matches("_raw")
                                                                == stem.trim_end_matches("_raw")
                                                        })
                                                        .unwrap_or(false)
                                                })
                                                .unwrap_or(false)
                                        },
                                    )
                                })
                            });
                            if md5_hit == Some(false) && sample.is_none() {
                                resource_only += 1;
                                sample = Some((
                                    username.clone(),
                                    lid,
                                    video
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_default(),
                                ));
                            }
                        }
                        if thumb.is_some() {
                            covers += 1;
                        }
                        if (with_video >= 5 && covers >= 10) || processed >= 200 {
                            break 'outer;
                        }
                    }
                }
            }
        }
        println!(
            "带视频消息 {} 条（其中资源库映射 {} 条示例={:?}），封面总数 {}",
            with_video, resource_only, sample, covers
        );
        if with_video == 0 {
            eprintln!("无本地视频消息，跳过");
            return;
        }
        assert!(covers >= with_video, "每个能解析的视频都应有封面");
    }
}
