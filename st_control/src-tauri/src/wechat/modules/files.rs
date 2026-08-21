//! 文件管理模块 - 对应 PC 微信「文件管理 / 聊天文件」
//!
//! 数据来源：`hardlink/hardlink.db`
//! - `image_hardlink_info_v4`   图片（md5 / file_name / file_size / modify_time / dir1 / dir2）
//! - `video_hardlink_info_v4`   视频（.mp4 与 .jpg 封面是两行、md5 不同，按文件名基名合并）
//! - `file_hardlink_info_v4`    文件
//! - `old_file_hardlink_info`   旧版文件
//!
//! `dir1`/`dir2` 是 `dir2id` 表的 rowid，映射目录名：
//! - 图片：`msg/attach/<dir1>/<dir2>/Img/<file_name>`
//! - 视频：`msg/video/<dir1>/<file_name>`（封面为同名 .jpg）
//! - 文件：`msg/file/<dir1>/<file_name>`

use super::common;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 文件条目
#[derive(Debug, Clone, Serialize)]
pub struct ResourceFile {
    /// MD5（唯一标识）
    pub md5: String,
    /// 文件名
    pub file_name: String,
    /// 大小（字节）
    pub file_size: i64,
    /// 大小显示
    pub size_label: String,
    /// 修改时间（Unix 秒）
    pub modify_time: i64,
    /// 时间显示
    pub time: String,
    /// 分类：image / video / file
    pub category: String,
    /// 扩展名
    pub ext: String,
    /// 已解析的本地真实路径（可能为空：文件已不在本地）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// 视频封面本地路径（视频条目专用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_path: Option<String>,
}

/// 文件总览
#[derive(Debug, Serialize)]
pub struct ResourceFilesOverview {
    pub images: Vec<ResourceFile>,
    pub videos: Vec<ResourceFile>,
    pub files: Vec<ResourceFile>,
    pub total_size: i64,
    pub total_size_label: String,
    /// 各类真实总数（去重后）
    pub images_total: i64,
    pub videos_total: i64,
    pub files_total: i64,
}

/// dir2id 表：rowid → 目录名（月份或会话 hash）
fn read_dir2id(conn: &rusqlite::Connection) -> HashMap<i64, String> {
    let mut map = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT rowid, username FROM dir2id") {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1).unwrap_or_default(),
            ))
        }) {
            for row in rows.flatten() {
                map.insert(row.0, row.1);
            }
        }
    }
    map
}

/// 按分类生成真实路径候选（按命中概率排序）
fn candidate_paths(
    base_dir: &Path,
    category: &str,
    file_name: &str,
    n1: &str,
    n2: &str,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    match category {
        "image" => {
            for (a, b) in [(n1, n2), (n2, n1)] {
                push(
                    base_dir
                        .join("msg")
                        .join("attach")
                        .join(a)
                        .join(b)
                        .join("Img")
                        .join(file_name),
                );
                push(
                    base_dir
                        .join("msg")
                        .join("attach")
                        .join(a)
                        .join(b)
                        .join(file_name),
                );
            }
            push(
                base_dir
                    .join("msg")
                    .join("attach")
                    .join(n1)
                    .join("Img")
                    .join(file_name),
            );
            push(base_dir.join("msg").join("attach").join(n1).join(file_name));
            push(base_dir.join("msg").join("attach").join(n2).join(file_name));
        }
        "video" => {
            push(base_dir.join("msg").join("video").join(n1).join(file_name));
            push(base_dir.join("msg").join("video").join(n2).join(file_name));
            push(base_dir.join("msg").join("video").join(file_name));
        }
        _ => {
            push(base_dir.join("msg").join("file").join(n1).join(file_name));
            push(base_dir.join("msg").join("file").join(n2).join(file_name));
            push(base_dir.join("msg").join("file").join(file_name));
            push(
                base_dir
                    .join("msg")
                    .join("attach")
                    .join(n1)
                    .join(n2)
                    .join(file_name),
            );
            push(base_dir.join("msg").join("attach").join(n1).join(file_name));
            push(base_dir.join("msg").join("attach").join(n2).join(file_name));
        }
    }
    out
}

fn first_existing(cands: &[PathBuf]) -> Option<PathBuf> {
    cands.iter().find(|p| p.is_file()).cloned()
}

/// 去掉扩展名（小写比较用）
fn split_ext(name: &str) -> (String, String) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.len() <= 10 => {
            (stem.to_string(), ext.to_lowercase())
        }
        _ => (name.to_string(), String::new()),
    }
}

/// 读取图片表（按 md5 去重、解析真实路径）
fn read_image_entries(
    conn: &rusqlite::Connection,
    table: &str,
    limit: usize,
    base_dir: &Path,
    dir2id: &HashMap<i64, String>,
) -> Vec<ResourceFile> {
    let mut items: Vec<ResourceFile> = Vec::new();
    if !common::table_exists(conn, table) {
        return items;
    }
    let sql = format!(
        "SELECT md5, file_name, file_size, modify_time, dir1, dir2 FROM \"{}\" ORDER BY modify_time DESC LIMIT ?1",
        table.replace('"', "")
    );
    let mut seen = std::collections::HashSet::new();
    if let Ok(mut stmt) = conn.prepare(&sql) {
        if let Ok(rows) = stmt.query_map(rusqlite::params![limit as i64], |r| {
            Ok((
                r.get::<_, String>(0).unwrap_or_default(),
                r.get::<_, String>(1).unwrap_or_default(),
                r.get::<_, i64>(2).unwrap_or(0),
                r.get::<_, i64>(3).unwrap_or(0),
                r.get::<_, i64>(4).unwrap_or(0),
                r.get::<_, i64>(5).unwrap_or(0),
            ))
        }) {
            for r in rows.flatten() {
                if r.0.is_empty() || !seen.insert(r.0.clone()) {
                    continue;
                }
                let (_, ext) = split_ext(&r.1);
                let n1 = dir2id.get(&r.4).cloned().unwrap_or_default();
                let n2 = dir2id.get(&r.5).cloned().unwrap_or_default();
                let path = first_existing(&candidate_paths(base_dir, "image", &r.1, &n1, &n2));
                items.push(ResourceFile {
                    md5: r.0,
                    file_name: r.1.clone(),
                    file_size: r.2,
                    size_label: common::format_file_size(r.2),
                    modify_time: r.3,
                    time: common::format_date_time(r.3),
                    category: "image".to_string(),
                    ext,
                    path: path.map(|p| p.to_string_lossy().to_string()),
                    cover_path: None,
                });
            }
        }
    }
    items.truncate(limit);
    items
}

/// 读取视频表：.mp4 与 .jpg 封面合并为一条视频记录
fn read_video_entries(
    conn: &rusqlite::Connection,
    table: &str,
    limit: usize,
    base_dir: &Path,
    dir2id: &HashMap<i64, String>,
) -> Vec<ResourceFile> {
    if !common::table_exists(conn, table) {
        return Vec::new();
    }
    let sql = format!(
        "SELECT md5, file_name, file_size, modify_time, dir1, dir2 FROM \"{}\" ORDER BY modify_time DESC LIMIT ?1",
        table.replace('"', "")
    );
    // 基名 → 封面路径
    let mut covers: HashMap<String, String> = HashMap::new();
    let mut videos: Vec<ResourceFile> = Vec::new();
    let video_exts = ["mp4", "m4v", "mov", "avi", "mkv", "webm"];
    let cover_exts = ["jpg", "jpeg", "png", "webp", "bmp", "gif"];
    if let Ok(mut stmt) = conn.prepare(&sql) {
        if let Ok(rows) = stmt.query_map(rusqlite::params![limit as i64 * 2], |r| {
            Ok((
                r.get::<_, String>(0).unwrap_or_default(),
                r.get::<_, String>(1).unwrap_or_default(),
                r.get::<_, i64>(2).unwrap_or(0),
                r.get::<_, i64>(3).unwrap_or(0),
                r.get::<_, i64>(4).unwrap_or(0),
                r.get::<_, i64>(5).unwrap_or(0),
            ))
        }) {
            for r in rows.flatten() {
                let (stem, ext) = split_ext(&r.1);
                let n1 = dir2id.get(&r.4).cloned().unwrap_or_default();
                let n2 = dir2id.get(&r.5).cloned().unwrap_or_default();
                let path = first_existing(&candidate_paths(base_dir, "video", &r.1, &n1, &n2));
                if video_exts.contains(&ext.as_str()) {
                    videos.push(ResourceFile {
                        md5: r.0,
                        file_name: r.1.clone(),
                        file_size: r.2,
                        size_label: common::format_file_size(r.2),
                        modify_time: r.3,
                        time: common::format_date_time(r.3),
                        category: "video".to_string(),
                        ext,
                        path: path.map(|p| p.to_string_lossy().to_string()),
                        cover_path: None,
                    });
                } else if cover_exts.contains(&ext.as_str()) && !stem.is_empty() {
                    if let Some(p) = path {
                        covers
                            .entry(stem)
                            .or_insert_with(|| p.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    for v in &mut videos {
        let (stem, _) = split_ext(&v.file_name);
        v.cover_path = covers.get(&stem).cloned();
    }
    videos.truncate(limit);
    videos
}

/// 读取文件表（合并新旧表、按 md5 去重、解析真实路径）
fn read_file_entries(
    conn: &rusqlite::Connection,
    tables: &[&str],
    limit: usize,
    base_dir: &Path,
    dir2id: &HashMap<i64, String>,
) -> Vec<ResourceFile> {
    let mut items: Vec<ResourceFile> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for table in tables {
        if !common::table_exists(conn, table) {
            continue;
        }
        let sql = format!(
            "SELECT md5, file_name, file_size, modify_time, dir1, dir2 FROM \"{}\" ORDER BY modify_time DESC LIMIT ?1",
            table.replace('"', "")
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map(rusqlite::params![limit as i64], |r| {
                Ok((
                    r.get::<_, String>(0).unwrap_or_default(),
                    r.get::<_, String>(1).unwrap_or_default(),
                    r.get::<_, i64>(2).unwrap_or(0),
                    r.get::<_, i64>(3).unwrap_or(0),
                    r.get::<_, i64>(4).unwrap_or(0),
                    r.get::<_, i64>(5).unwrap_or(0),
                ))
            }) {
                for r in rows.flatten() {
                    if r.0.is_empty() || !seen.insert(r.0.clone()) {
                        continue;
                    }
                    let (_, ext) = split_ext(&r.1);
                    let n1 = dir2id.get(&r.4).cloned().unwrap_or_default();
                    let n2 = dir2id.get(&r.5).cloned().unwrap_or_default();
                    let path = first_existing(&candidate_paths(base_dir, "file", &r.1, &n1, &n2));
                    items.push(ResourceFile {
                        md5: r.0,
                        file_name: r.1.clone(),
                        file_size: r.2,
                        size_label: common::format_file_size(r.2),
                        modify_time: r.3,
                        time: common::format_date_time(r.3),
                        category: "file".to_string(),
                        ext,
                        path: path.map(|p| p.to_string_lossy().to_string()),
                        cover_path: None,
                    });
                }
            }
        }
    }
    items.sort_by_key(|a| std::cmp::Reverse(a.modify_time));
    items.truncate(limit);
    items
}

/// 表统计（去重后的数量与总大小）
fn table_stats(conn: &rusqlite::Connection, table: &str, video: bool) -> (i64, i64) {
    if !common::table_exists(conn, table) {
        return (0, 0);
    }
    let sql = if video {
        format!(
            "SELECT COUNT(*), IFNULL(SUM(file_size),0) FROM \"{}\" WHERE lower(file_name) NOT LIKE '%.jpg' AND lower(file_name) NOT LIKE '%.jpeg' AND lower(file_name) NOT LIKE '%.png' AND lower(file_name) NOT LIKE '%.webp' AND lower(file_name) NOT LIKE '%.bmp' AND lower(file_name) NOT LIKE '%.gif'",
            table.replace('"', "")
        )
    } else {
        format!(
            "SELECT COUNT(*), IFNULL(SUM(file_size),0) FROM (SELECT md5, file_size FROM \"{}\" GROUP BY md5)",
            table.replace('"', "")
        )
    };
    conn.query_row(&sql, [], |r| {
        Ok((
            r.get::<_, i64>(0).unwrap_or(0),
            r.get::<_, i64>(1).unwrap_or(0),
        ))
    })
    .unwrap_or((0, 0))
}

/// 读取全部资源文件
pub fn get_resource_files(
    decrypted_dir: &Path,
    wechat_base_dir: &Path,
    limit_per_category: usize,
) -> Result<ResourceFilesOverview, String> {
    let db_path = decrypted_dir.join("hardlink").join("hardlink.db");
    if !db_path.exists() {
        return Err(format!("文件数据库未解密: {}", db_path.display()));
    }
    let conn = common::open_readonly_db(&db_path).map_err(|e| format!("打开失败: {}", e))?;
    let dir2id = read_dir2id(&conn);

    let images = read_image_entries(
        &conn,
        "image_hardlink_info_v4",
        limit_per_category,
        wechat_base_dir,
        &dir2id,
    );
    let videos = read_video_entries(
        &conn,
        "video_hardlink_info_v4",
        limit_per_category,
        wechat_base_dir,
        &dir2id,
    );
    let files = read_file_entries(
        &conn,
        &["file_hardlink_info_v4", "old_file_hardlink_info"],
        limit_per_category,
        wechat_base_dir,
        &dir2id,
    );

    let (images_total, images_size) = table_stats(&conn, "image_hardlink_info_v4", false);
    let (videos_total, videos_size) = table_stats(&conn, "video_hardlink_info_v4", true);
    let (files_total, files_size) = table_stats(&conn, "file_hardlink_info_v4", false);
    let total_size = images_size + videos_size + files_size;

    Ok(ResourceFilesOverview {
        images,
        videos,
        files,
        total_size,
        total_size_label: common::format_file_size(total_size),
        images_total,
        videos_total,
        files_total,
    })
}

/// 按 md5 解析真实文件路径（图片 / 视频 / 文件均可）。
///
/// 【唯一性】严格限定在 wechat_base_dir（当前账号目录）内查找：
/// 每个账号的文件只能来自自己的目录。绝不扫描兄弟 wxid_* 账号目录——
/// 不同账号收到过同一文件时（md5 相同）跨账号扫描会命中别人账号的文件，
/// 造成文件串号。若配置的账号目录与解密库账号不一致，由面板顶部
/// 「账号不一致」提示用户修正配置，而不是静默去其他账号目录捞文件。
pub fn resolve_file_path(
    decrypted_dir: &Path,
    wechat_base_dir: &Path,
    md5: &str,
) -> Option<PathBuf> {
    let db_path = decrypted_dir.join("hardlink").join("hardlink.db");
    let conn = common::open_readonly_db(&db_path).ok()?;
    let dir2id = read_dir2id(&conn);
    let md5l = md5.trim().to_lowercase();
    if md5l.len() != 32 {
        return None;
    }
    // 1) 先跨 4 张 hardlink 表收集候选行（与根目录解耦）
    let mut rows: Vec<(&'static str, String, String, String)> = Vec::new();
    for (table, category) in [
        ("image_hardlink_info_v4", "image"),
        ("video_hardlink_info_v4", "video"),
        ("file_hardlink_info_v4", "file"),
        ("old_file_hardlink_info", "file"),
    ] {
        if !common::table_exists(&conn, table) {
            continue;
        }
        let sql = format!(
            "SELECT file_name, dir1, dir2 FROM \"{}\" WHERE lower(md5) = ?1 ORDER BY modify_time DESC LIMIT 8",
            table.replace('"', "")
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(qrows) = stmt.query_map(rusqlite::params![md5l], |r| {
                Ok((
                    r.get::<_, String>(0).unwrap_or_default(),
                    r.get::<_, i64>(1).unwrap_or(0),
                    r.get::<_, i64>(2).unwrap_or(0),
                ))
            }) {
                for row in qrows.flatten() {
                    rows.push((
                        category,
                        row.0,
                        dir2id.get(&row.1).cloned().unwrap_or_default(),
                        dir2id.get(&row.2).cloned().unwrap_or_default(),
                    ));
                }
            }
        }
    }
    if rows.is_empty() {
        return None;
    }
    // 2) 仅当前账号根目录内求第一条真实存在的路径（不跨账号）
    for (category, file_name, n1, n2) in &rows {
        if let Some(p) = first_existing(&candidate_paths(
            wechat_base_dir,
            category,
            file_name,
            n1,
            n2,
        )) {
            return Some(p);
        }
    }
    None
}

/// 按视频 md5 解析封面路径（优先 hardlink 里的同名图片行，其次视频旁的 jpg）
pub fn resolve_video_cover_path(
    decrypted_dir: &Path,
    wechat_base_dir: &Path,
    md5: &str,
) -> Option<PathBuf> {
    let db_path = decrypted_dir.join("hardlink").join("hardlink.db");
    let conn = common::open_readonly_db(&db_path).ok()?;
    let dir2id = read_dir2id(&conn);
    let md5l = md5.trim().to_lowercase();
    if md5l.len() != 32 {
        return None;
    }
    let cover_exts = ["jpg", "jpeg", "png", "webp", "bmp"];
    let video_table = "video_hardlink_info_v4";
    if !common::table_exists(&conn, video_table) {
        return None;
    }
    // 1) 找视频行，拿到基名
    let sql = format!(
        "SELECT file_name FROM \"{}\" WHERE lower(md5) = ?1 ORDER BY modify_time DESC LIMIT 1",
        video_table.replace('"', "")
    );
    let stem = conn
        .query_row(&sql, rusqlite::params![md5l], |r| r.get::<_, String>(0))
        .ok()
        .map(|n| split_ext(&n).0)?;
    if stem.is_empty() {
        return None;
    }
    // 2) hardlink 中找同名图片行
    let like = format!("{}%", stem);
    let sql = format!(
        "SELECT file_name, dir1, dir2 FROM \"{}\" WHERE lower(file_name) LIKE ?1 ORDER BY modify_time DESC LIMIT 12",
        video_table.replace('"', "")
    );
    if let Ok(mut stmt) = conn.prepare(&sql) {
        if let Ok(rows) = stmt.query_map(rusqlite::params![like], |r| {
            Ok((
                r.get::<_, String>(0).unwrap_or_default(),
                r.get::<_, i64>(1).unwrap_or(0),
                r.get::<_, i64>(2).unwrap_or(0),
            ))
        }) {
            for row in rows.flatten() {
                let (s, ext) = split_ext(&row.0);
                if s != stem || !cover_exts.contains(&ext.as_str()) {
                    continue;
                }
                let n1 = dir2id.get(&row.1).cloned().unwrap_or_default();
                let n2 = dir2id.get(&row.2).cloned().unwrap_or_default();
                if let Some(p) =
                    first_existing(&candidate_paths(wechat_base_dir, "video", &row.0, &n1, &n2))
                {
                    return Some(p);
                }
            }
        }
    }
    // 3) 视频文件旁探测
    if let Some(vp) = resolve_file_path(decrypted_dir, wechat_base_dir, &md5l) {
        let dir = vp.parent()?;
        for cand in [
            format!("{}.jpg", stem),
            format!("{}.jpeg", stem),
            format!("{}.png", stem),
            format!("{}_thumb.jpg", stem),
        ] {
            let p = dir.join(&cand);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 唯一性回归：兄弟账号目录存在同名文件时，resolve_file_path
    /// 必须只返回当前账号目录内的路径，绝不跨账号串文件。
    #[test]
    fn resolve_file_path_stays_in_current_account() {
        let root = std::env::temp_dir().join(format!("st-uniq-file-{}", std::process::id()));
        let base = root.join("wxid_aaa");
        let sibling = root.join("wxid_bbb");
        let decrypted = root.join("decrypted");
        let hardlink = decrypted.join("hardlink");
        std::fs::create_dir_all(&hardlink).expect("创建测试目录");
        let conn = rusqlite::Connection::open(hardlink.join("hardlink.db")).expect("建库");
        conn.execute_batch(
            "CREATE TABLE dir2id (username TEXT);
             CREATE TABLE image_hardlink_info_v4
               (md5 TEXT, file_name TEXT, dir1 INTEGER, dir2 INTEGER, modify_time INTEGER);",
        )
        .expect("建表");
        conn.execute(
            "INSERT INTO dir2id (rowid, username) VALUES (1, 'h1'), (2, '2026-08')",
            [],
        )
        .expect("插入 dir2id");
        conn.execute(
            "INSERT INTO image_hardlink_info_v4 (md5, file_name, dir1, dir2, modify_time)
             VALUES (?1, ?2, 1, 2, 100)",
            rusqlite::params![
                "aabbccddeeff00112233445566778899",
                "aabbccddeeff00112233445566778899.dat"
            ],
        )
        .expect("插入 hardlink 行");
        drop(conn);
        // 两个账号目录都放同名文件：命中必须限定在当前账号
        for b in [&base, &sibling] {
            let img = b
                .join("msg")
                .join("attach")
                .join("h1")
                .join("2026-08")
                .join("Img");
            std::fs::create_dir_all(&img).expect("创建测试目录");
            std::fs::write(img.join("aabbccddeeff00112233445566778899.dat"), b"x")
                .expect("写入测试文件");
        }
        let got = resolve_file_path(&decrypted, &base, "aabbccddeeff00112233445566778899")
            .expect("应能解析路径");
        assert!(got.starts_with(&base), "必须命中当前账号路径: {:?}", got);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    #[ignore = "需要真实解密 hardlink.db 与微信数据目录（ST_WECHAT_BASE）"]
    fn real_resource_files_roundtrip() {
        let base = crate::common::wechat_data_dir();
        let decrypted = base.join("decrypted");
        let db = decrypted.join("hardlink").join("hardlink.db");
        if !db.is_file() {
            eprintln!("跳过：未找到 {}", db.display());
            return;
        }
        let Ok(wx) = std::env::var("ST_WECHAT_BASE") else {
            eprintln!("跳过：未设置 ST_WECHAT_BASE");
            return;
        };
        let wx = PathBuf::from(wx);
        let ov = get_resource_files(&decrypted, &wx, 120).expect("读取文件总览");
        assert!(ov.images_total >= 0);
        assert!(ov.videos_total >= 0);
        assert!(ov.files_total >= 0);
        // 视频条目不应包含 .jpg 封面行
        for v in &ov.videos {
            assert_ne!(v.ext, "jpg", "视频条目混入了封面行: {}", v.file_name);
        }
        // 图片/文件按 md5 去重
        let im: std::collections::HashSet<_> = ov.images.iter().map(|f| f.md5.as_str()).collect();
        assert_eq!(im.len(), ov.images.len());
        let fl: std::collections::HashSet<_> = ov.files.iter().map(|f| f.md5.as_str()).collect();
        assert_eq!(fl.len(), ov.files.len());
        // 至少一条路径已解析
        let resolved = ov
            .images
            .iter()
            .chain(ov.videos.iter())
            .chain(ov.files.iter())
            .filter(|f| f.path.is_some())
            .count();
        assert!(resolved > 0, "没有任何文件路径被解析");
        // 已解析路径确实存在于磁盘
        let first = ov
            .images
            .iter()
            .chain(ov.videos.iter())
            .chain(ov.files.iter())
            .find(|f| f.path.is_some());
        if let Some(f) = first {
            let p = PathBuf::from(f.path.as_ref().unwrap());
            assert!(p.is_file(), "解析路径不存在: {}", p.display());
            assert_eq!(resolve_file_path(&decrypted, &wx, &f.md5), Some(p));
        }
        // 视频封面解析（有封面样本时）
        if let Some(v) = ov.videos.iter().find(|v| v.cover_path.is_some()) {
            let cv = resolve_video_cover_path(&decrypted, &wx, &v.md5);
            assert!(cv.is_some(), "视频 {} 封面解析失败", v.file_name);
            assert!(cv.unwrap().is_file());
        } else {
            eprintln!("跳过封面断言：无封面样本");
        }
    }

    #[test]
    #[ignore = "需要真实解密库与微信数据目录（ST_WECHAT_BASE）"]
    fn real_image_decode_roundtrip() {
        let base = crate::common::wechat_data_dir();
        let decrypted = base.join("decrypted");
        let db = decrypted.join("hardlink").join("hardlink.db");
        if !db.is_file() {
            eprintln!("跳过：未找到 {}", db.display());
            return;
        }
        let Ok(wx) = std::env::var("ST_WECHAT_BASE") else {
            eprintln!("跳过：未设置 ST_WECHAT_BASE");
            return;
        };
        let wx = PathBuf::from(wx);
        let cfg = crate::wechat::config::WeChatConfig::load().expect("加载微信配置");
        let ov = get_resource_files(&decrypted, &wx, 50).expect("读取文件总览");
        let img = ov
            .images
            .iter()
            .find(|f| f.path.is_some())
            .expect("无图片样本");
        let aes = cfg
            .image_aes_key
            .as_ref()
            .filter(|k| k.len() == 16)
            .map(|k| k.as_bytes().to_vec());
        let cache_dir = cfg.decoded_image_dir.join("files_images_test");
        let data_url = crate::wechat::image::decode_dat_to_data_url(
            Path::new(img.path.as_ref().unwrap()),
            &cache_dir,
            &img.md5,
            aes.as_deref(),
            cfg.image_xor_key,
        )
        .expect("图片解密失败");
        assert!(
            data_url.starts_with("data:image/"),
            "解密结果不是图片 data url"
        );
    }
}
