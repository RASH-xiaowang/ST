//! 微信文件消息（appmsg type=6）附件定位
//!
//! PC 微信 4.x 把接收/发送的文件附件以**原始文件名**存放在
//! `msg/file/<YYYY-MM>/`（全局），部分数据也会落在
//! `msg/attach/<会话哈希>/<YYYY-MM>/`。
//!
//! 本模块按消息 XML 的 `title` / `md5` / `totallen` 定位真实文件：
//! 1. 标题精确匹配（忽略大小写）
//! 2. md5 文件名（`<md5>.<ext>`）
//! 3. 文件大小匹配（totallen）
//!
//! 找不到时返回“应打开的存储目录”（消息所在月份目录），
//! 供前端实现“打不开就打开所在目录”。

use crate::wechat::modules::common::{dir_sig, is_month_dir_name, DirFileSigList, DirSig};
use rusqlite::OptionalExtension;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// 解析结果
#[derive(Debug, Clone)]
pub struct ResolvedWechatFile {
    /// 命中的真实文件路径（None = 未找到原文件）
    pub path: Option<PathBuf>,
    /// 兜底目录（打开所在目录）
    pub dir: PathBuf,
    pub title: String,
    pub file_ext: String,
    pub file_size: i64,
    pub found: bool,
}

/// 目录索引：小写文件名 → 候选文件路径（月份目录新 → 旧）
type FileIndex = HashMap<String, Vec<PathBuf>>;

/// 目录索引缓存：根目录 → (签名, 索引)
static FILE_INDEX_CACHE: OnceLock<Mutex<HashMap<PathBuf, (DirFileSigList, FileIndex)>>> =
    OnceLock::new();

/// 根目录签名：根 + 各月份子目录的 (名称, mtime, 条目数)。
/// 新文件下载进已有月份目录时根目录 mtime 不变，必须看子目录签名才能失效。
fn root_sig(root: &Path) -> DirFileSigList {
    let mut sigs = vec![(".".to_string(), dir_sig(root)?)];
    if let Ok(entries) = std::fs::read_dir(root) {
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

fn build_index(root: &Path) -> FileIndex {
    let mut index: FileIndex = HashMap::new();
    let mut dirs = vec![root.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut months: Vec<PathBuf> = entries
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
    for dir in dirs {
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
            index.entry(name.to_lowercase()).or_default().push(path);
        }
    }
    index
}

fn file_index(root: &Path) -> FileIndex {
    let sig = root_sig(root);
    let cache = FILE_INDEX_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some((saved, index)) = guard.get(root) {
            if *saved == sig {
                return index.clone();
            }
        }
    }
    let index = build_index(root);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(root.to_path_buf(), (sig, index.clone()));
    }
    index
}

fn clean_cdata(s: String) -> String {
    s.replace("<![CDATA[", "")
        .replace("]]>", "")
        .trim()
        .to_string()
}

fn xml_text(xml: &str, tag: &str) -> String {
    crate::wechat::modules::common::xml_tag_text(xml, tag)
        .map(clean_cdata)
        .unwrap_or_default()
}

/// 从消息库读取文件消息 XML（local_type 低 32 位 = 49 且 appmsg type=6），
/// 返回 (xml, create_time)
fn message_file_xml(decrypted_dir: &Path, username: &str, local_id: i64) -> Option<(String, i64)> {
    let table = crate::wechat::modules::common::msg_table_name(username);
    let msg_dir = decrypted_dir.join("message");
    let Ok(entries) = std::fs::read_dir(&msg_dir) else {
        return None;
    };
    let mut dbs: Vec<PathBuf> = entries
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
        let Ok(conn) = rusqlite::Connection::open_with_flags(
            &db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) else {
            continue;
        };
        let sql = format!(
            "SELECT message_content, compress_content, create_time FROM \"{}\" \
             WHERE local_id = ?1 AND local_type % 4294967296 = 49 LIMIT 1",
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
            if xml.contains("<type>6</type>") {
                return Some((xml, create_time));
            }
        }
    }
    None
}

fn push_unique(v: &mut Vec<PathBuf>, p: PathBuf, seen: &mut HashSet<PathBuf>) {
    if seen.insert(p.clone()) {
        v.push(p);
    }
}

/// 在月份目录里按文件大小（totallen）匹配候选
fn size_hits(root: &Path, totallen: i64) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if totallen <= 0 {
        return out;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    let mut months: Vec<PathBuf> = entries
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
    for d in months {
        let Ok(es) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in es.flatten() {
            let p = e.path();
            if p.is_file()
                && p.metadata()
                    .map(|m| m.len() as i64 == totallen)
                    .unwrap_or(false)
            {
                out.push(p);
            }
        }
    }
    out
}

/// 定位文件消息的附件。
///
/// 返回 None 表示该消息不是文件消息；否则始终返回一个可打开的路径或目录。
pub fn resolve_wechat_file(
    decrypted_dir: &Path,
    wechat_base_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<ResolvedWechatFile> {
    let (xml, create_time) = message_file_xml(decrypted_dir, username, local_id)?;
    let title = xml_text(&xml, "title");
    let md5 = xml_text(&xml, "md5").trim().to_lowercase();
    let file_ext = xml_text(&xml, "fileext").trim().to_lowercase();
    let totallen: i64 = xml_text(&xml, "totallen").trim().parse().unwrap_or(0);
    let month = crate::wechat::modules::common::month_of(create_time);

    let mut roots = vec![
        wechat_base_dir.join("msg").join("file"),
        wechat_base_dir
            .join("msg")
            .join("attach")
            .join(crate::wechat::modules::common::msg_table_name(username)),
    ];

    let mut exact: Vec<PathBuf> = Vec::new();
    let mut md5_hits: Vec<PathBuf> = Vec::new();
    let mut size_hits_all: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let lower_title = title.to_lowercase();

    for root in roots.drain(..) {
        if !root.is_dir() {
            continue;
        }
        let index = file_index(&root);
        if !lower_title.is_empty() {
            if let Some(list) = index.get(&lower_title) {
                for p in list {
                    push_unique(&mut exact, p.clone(), &mut seen);
                }
            }
        }
        if !md5.is_empty() {
            for name in [format!("{}.{}", md5, file_ext), md5.clone()] {
                if let Some(list) = index.get(&name) {
                    for p in list {
                        push_unique(&mut md5_hits, p.clone(), &mut seen);
                    }
                }
            }
        }
        if totallen > 0 {
            for p in size_hits(&root, totallen) {
                push_unique(&mut size_hits_all, p, &mut seen);
            }
        }
    }

    let path = exact
        .first()
        .or_else(|| md5_hits.first())
        .or_else(|| size_hits_all.first())
        .cloned();

    // 兜底目录：命中文件所在目录 > 消息月份目录 > msg/file 根目录
    let dir = path
        .as_ref()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .or_else(|| {
            if month.is_empty() {
                None
            } else {
                Some(wechat_base_dir.join("msg").join("file").join(&month))
            }
        })
        .or_else(|| Some(wechat_base_dir.join("msg").join("file")))
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| wechat_base_dir.join("msg").join("file"));

    Some(ResolvedWechatFile {
        found: path.is_some(),
        path,
        dir,
        title,
        file_ext,
        file_size: totallen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_month_dir_name() {
        assert!(is_month_dir_name("2026-06"));
        assert!(!is_month_dir_name("2026-6"));
        assert!(!is_month_dir_name("attach"));
        assert!(!is_month_dir_name("20266-06"));
    }

    /// 真实数据：文件消息应能解析出 title/md5/大小，且大多能在
    /// `msg/file/<YYYY-MM>/` 找到同名文件。
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_resolve_wechat_file() {
        let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        // 用已知会话（ST_王国宁 私聊，含较多文件消息）遍历最近文件消息
        let Ok(conn) = rusqlite::Connection::open_with_flags(
            cfg.decrypted_dir.join("message").join("message_0.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) else {
            return;
        };
        let table = crate::wechat::modules::common::msg_table_name("wxid_umyqa86if3lm22");
        let sql = format!(
            "SELECT local_id, create_time, message_content, compress_content FROM \"{}\" \
             WHERE local_type % 4294967296 = 49 ORDER BY local_id DESC LIMIT 200",
            table
        );
        let mut checked = 0usize;
        let mut found = 0usize;
        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0).unwrap_or(0),
                    r.get::<_, i64>(1).unwrap_or(0),
                    crate::wechat::modules::common::get_bytes(r, 2),
                    crate::wechat::modules::common::get_bytes(r, 3),
                ))
            }) {
                for row in rows.flatten() {
                    let bytes = row.2.or(row.3);
                    let Some(bytes) = bytes else { continue };
                    let xml = crate::wechat::modules::common::decode_blob_text(&bytes);
                    if !xml.contains("<type>6</type>") {
                        continue;
                    }
                    checked += 1;
                    let Some(res) = resolve_wechat_file(
                        &cfg.decrypted_dir,
                        &cfg.wechat_base_dir,
                        "wxid_umyqa86if3lm22",
                        row.0,
                    ) else {
                        continue;
                    };
                    if res.found {
                        found += 1;
                    }
                }
            }
        }
        drop(conn);
        eprintln!("文件消息 {} 条，定位到 {} 条", checked, found);
        // smoke 依赖真实微信数据：无该账号数据时跳过（CI/无数据环境不失败）
        if checked == 0 {
            eprintln!("会话中无文件消息，跳过");
            return;
        }
        // 至少能定位到一部分（真实数据大多数已下载）
        assert!(found > 0, "应至少定位到 1 个文件附件");
    }
}
