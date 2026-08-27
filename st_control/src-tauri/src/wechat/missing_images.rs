//! 微信图片缺失统计
//!
//! 扫描解密消息库中的图片消息（local_type=3），按三类统计：
//! - `local_ok`：本地 attach .dat 或解码缓存中按任一 md5 变体（md5 /
//!   originsourcemd5 / hdmd5）可找到 → 直接可显示
//! - `cdn_possible`：本地无文件但消息含 cdnbigimgurl → 可走 CDN 原图下载
//! - `missing`：本地无文件且仅有中图 fileid（c3o.re 网关不响应）→ 确实缺失
//!
//! 输出每会话汇总 + 缺失明细，供界面展示与 CSV 导出。

use crate::wechat::config::WeChatConfig;
use crate::wechat::handlers::helpers;
use crate::wechat::modules::common;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// 单会话图片统计
#[derive(Debug, Clone, Serialize)]
pub struct ChatMissingStat {
    pub username: String,
    pub name: String,
    pub total_images: u64,
    pub local_ok: u64,
    pub cdn_possible: u64,
    pub missing: u64,
}

/// 缺失图片明细（导出用）
#[derive(Debug, Clone, Serialize)]
pub struct MissingImageDetail {
    pub username: String,
    pub name: String,
    pub local_id: i64,
    pub md5: String,
}

/// 扫描报告
pub struct MissingImageReport {
    pub scanned_at: String,
    pub total_images: u64,
    pub local_ok: u64,
    pub cdn_possible: u64,
    pub missing: u64,
    pub chats: Vec<ChatMissingStat>,
    pub details: Vec<MissingImageDetail>,
}

fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_files(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// 收集本地可用的图片 md5（attach .dat 文件名 + 解码缓存文件名，取前 32 位小写）
fn collect_local_md5s(cfg: &WeChatConfig) -> HashSet<String> {
    let mut set = HashSet::new();
    let mut files: Vec<PathBuf> = Vec::new();

    let base = &cfg.wechat_base_dir;
    let attach = base.join("msg").join("attach");
    if attach.is_dir() {
        walk_files(&attach, &mut files);
    } else if let Ok(entries) = std::fs::read_dir(base) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                let a = p.join("msg").join("attach");
                if a.is_dir() {
                    walk_files(&a, &mut files);
                }
            }
        }
    }
    for f in &files {
        if f.extension().and_then(|e| e.to_str()) == Some("dat") {
            let name = f
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if name.len() >= 32 {
                // 安全截断：按字符边界切分，避免多字节 UTF-8 字符 panic
                set.insert(name.chars().take(32).collect());
            }
        }
    }

    let mut decoded: Vec<PathBuf> = Vec::new();
    walk_files(&cfg.decoded_image_dir, &mut decoded);
    for f in decoded {
        let name = f
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if name.len() >= 32 {
            set.insert(name.chars().take(32).collect());
        }
    }
    set
}

/// 从解密 session 库加载 username → 显示名，并返回 md5(username) → username 映射
fn load_sessions(decrypted: &Path) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut names: HashMap<String, String> = HashMap::new();
    let mut md5_to_user: HashMap<String, String> = HashMap::new();
    let session_dir = decrypted.join("session");
    let Ok(entries) = std::fs::read_dir(&session_dir) else {
        return (names, md5_to_user);
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("db") {
            continue;
        }
        let Ok(conn) =
            rusqlite::Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        // SessionTable: username
        if let Ok(mut stmt) = conn.prepare("SELECT username FROM SessionTable") {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                for u in rows.flatten() {
                    let h = common::msg_table_name(&u);
                    md5_to_user.insert(h, u.clone());
                    names.entry(u).or_default();
                }
            }
        }
        // SessionNoContactInfoTable: session_title
        if let Ok(mut stmt) =
            conn.prepare("SELECT username, session_title FROM SessionNoContactInfoTable")
        {
            if let Ok(rows) =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            {
                for row in rows.flatten() {
                    let (u, title) = row;
                    if !title.trim().is_empty() {
                        names.insert(u, title);
                    }
                }
            }
        }
        let _ = conn.close();
    }
    (names, md5_to_user)
}

/// 主入口：扫描全部可达会话的图片消息
pub fn scan(cfg: &WeChatConfig) -> Result<MissingImageReport, String> {
    let t0 = std::time::Instant::now();
    let local_md5s = collect_local_md5s(cfg);
    let (name_map, md5_to_user) = load_sessions(&cfg.decrypted_dir);

    let msg_dir = cfg.decrypted_dir.join("message");
    let Ok(entries) = std::fs::read_dir(&msg_dir) else {
        return Ok(MissingImageReport {
            scanned_at: now_str(),
            total_images: 0,
            local_ok: 0,
            cdn_possible: 0,
            missing: 0,
            chats: Vec::new(),
            details: Vec::new(),
        });
    };
    let mut dbs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("db")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| {
                        n.starts_with("message_") && !n.contains("fts") && !n.contains("resource")
                    })
                    .unwrap_or(false)
        })
        .collect();
    dbs.sort();

    let mut chat_map: HashMap<String, ChatMissingStat> = HashMap::new();
    let mut details: Vec<MissingImageDetail> = Vec::new();
    let mut total_images = 0u64;
    let mut local_ok = 0u64;
    let mut cdn_possible = 0u64;
    let mut missing = 0u64;

    for db in &dbs {
        let Ok(conn) =
            rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        let Ok(mut tables) = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'Msg\\_%' ESCAPE '\\'",
        ) else {
            continue;
        };
        let table_names: Vec<String> = tables
            .query_map([], |r| r.get::<_, String>(0))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default();
        drop(tables);

        for table in table_names {
            let Some(username) = md5_to_user.get(&table).cloned() else {
                continue;
            };
            let stat = chat_map
                .entry(username.clone())
                .or_insert_with(|| ChatMissingStat {
                    username: username.clone(),
                    name: name_map.get(&username).cloned().unwrap_or_default(),
                    total_images: 0,
                    local_ok: 0,
                    cdn_possible: 0,
                    missing: 0,
                });
            let Ok(mut stmt) = conn.prepare(&format!(
                "SELECT message_content, compress_content FROM \"{}\" WHERE local_type=3",
                table
            )) else {
                continue;
            };
            let rows = stmt.query_map([], |r| {
                Ok((common::get_bytes(r, 0), common::get_bytes(r, 1)))
            });
            let Ok(rows) = rows else {
                continue;
            };
            for row in rows.flatten() {
                let (content, compressed) = row;
                let Some(blob) = content.or(compressed) else {
                    continue;
                };
                let xml = common::decode_blob_text(&blob);
                if xml.is_empty() {
                    continue;
                }
                total_images += 1;
                stat.total_images += 1;

                let variants: Vec<String> = ["md5", "originsourcemd5", "hdmd5"]
                    .iter()
                    .filter_map(|a| crate::wechat::cdn_image::extract_xml_value(&xml, a))
                    .map(|v| v.trim().to_lowercase())
                    .filter(|v| v.len() == 32)
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                let has_big =
                    crate::wechat::cdn_image::extract_xml_value(&xml, "cdnbigimgurl").is_some();

                let local_hit = variants.iter().any(|v| local_md5s.contains(v));
                if local_hit {
                    local_ok += 1;
                    stat.local_ok += 1;
                } else if has_big {
                    cdn_possible += 1;
                    stat.cdn_possible += 1;
                } else {
                    missing += 1;
                    stat.missing += 1;
                }
            }
        }
        let _ = conn.close();
    }

    // 第二遍：仅补齐缺失图片的 (local_id) 明细，避免第一遍每行多一次查询
    for db in &dbs {
        let Ok(conn) =
            rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        let Ok(mut tables) = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'Msg\\_%' ESCAPE '\\'",
        ) else {
            continue;
        };
        let table_names: Vec<String> = tables
            .query_map([], |r| r.get::<_, String>(0))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default();
        drop(tables);
        for table in table_names {
            let Some(username) = md5_to_user.get(&table).cloned() else {
                continue;
            };
            let Ok(mut stmt) = conn.prepare(&format!(
                "SELECT local_id, message_content, compress_content FROM \"{}\" WHERE local_type=3",
                table
            )) else {
                continue;
            };
            let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    common::get_bytes(r, 1),
                    common::get_bytes(r, 2),
                ))
            }) else {
                continue;
            };
            for row in rows.flatten() {
                let (local_id, content, compressed) = row;
                let Some(blob) = content.or(compressed) else {
                    continue;
                };
                let xml = common::decode_blob_text(&blob);
                if xml.is_empty() {
                    continue;
                }
                let variants: Vec<String> = ["md5", "originsourcemd5", "hdmd5"]
                    .iter()
                    .filter_map(|a| crate::wechat::cdn_image::extract_xml_value(&xml, a))
                    .map(|v| v.trim().to_lowercase())
                    .filter(|v| v.len() == 32)
                    .collect();
                let local_hit = variants.iter().any(|v| local_md5s.contains(v));
                let has_big =
                    crate::wechat::cdn_image::extract_xml_value(&xml, "cdnbigimgurl").is_some();
                if !local_hit && !has_big {
                    let md5 = variants.first().cloned().unwrap_or_default();
                    details.push(MissingImageDetail {
                        username: username.clone(),
                        name: name_map.get(&username).cloned().unwrap_or_default(),
                        local_id,
                        md5,
                    });
                }
            }
        }
        let _ = conn.close();
    }

    let mut chats: Vec<ChatMissingStat> = chat_map.into_values().collect();
    chats.sort_by(|a, b| {
        b.missing
            .cmp(&a.missing)
            .then_with(|| b.total_images.cmp(&a.total_images))
    });
    details.sort_by(|a, b| {
        a.username
            .cmp(&b.username)
            .then_with(|| a.local_id.cmp(&b.local_id))
    });

    log::info!(
        "[missing_images] 扫描完成: {} 张图 (本地 {} / CDN {} / 缺失 {})，{} 会话，耗时 {:.1}s",
        total_images,
        local_ok,
        cdn_possible,
        missing,
        chats.len(),
        t0.elapsed().as_secs_f64()
    );

    Ok(MissingImageReport {
        scanned_at: now_str(),
        total_images,
        local_ok,
        cdn_possible,
        missing,
        chats,
        details,
    })
}

/// 缺失明细导出 CSV（每行一条缺失图片）
pub fn missing_images_csv(report: &MissingImageReport) -> String {
    let mut s = String::from("会话,显示名,消息ID,md5,原因\n");
    for d in &report.details {
        s.push_str(&format!(
            "{},{},{},{},{}\n",
            helpers::csv_cell(&d.username),
            helpers::csv_cell(&d.name),
            d.local_id,
            helpers::csv_cell(&d.md5),
            "本地与CDN均无（数据缺失）"
        ));
    }
    s
}

fn now_str() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}
