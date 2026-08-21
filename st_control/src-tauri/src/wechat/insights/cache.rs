// ============================================================
// 微信关系图谱 — 会话统计持久缓存层
// 自 insights.rs 拆分：磁盘/内存缓存、目录签名与分库枚举。
// ============================================================

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use crate::wechat::modules::common;

use super::stats::{collect_active_days, collect_msg_counts, SessionStats};
use super::GraphEmitCtx;

/// 磁盘缓存载荷
#[derive(serde::Deserialize, serde::Serialize)]
struct StatsPayload {
    sig: String,
    /// username -> [count, active_days, max_ts]
    stats: HashMap<String, Vec<i64>>,
    /// username -> 命中分库相对路径（如 "message/message_0.db"）
    table_dbs: HashMap<String, Vec<String>>,
    built_at: String,
}

/// 内存缓存（带签名失效）
struct StatsCache {
    sig: String,
    stats: SessionStats,
}

static STATS_CACHE: OnceLock<Mutex<Option<StatsCache>>> = OnceLock::new();
static STATS_IO_LOCK: Mutex<()> = Mutex::new(());

fn stats_cache_path() -> PathBuf {
    crate::wechat::config::default_st_result_dir().join("wechat_graph_stats.json")
}

pub(crate) fn message_shards(decrypted: &Path) -> Vec<PathBuf> {
    let mut dbs = common::find_db_files(decrypted, "message_");
    dbs.extend(common::find_db_files(decrypted, "biz_message_"));
    dbs.sort();
    dbs.dedup();
    dbs.retain(|p| common::is_message_shard_file(p));
    dbs
}

/// 目录签名：分库（含 WAL）的 (名称, mtime 毫秒, 长度) 拼 md5，跨进程稳定
fn dir_signature(decrypted: &Path) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    for p in message_shards(decrypted) {
        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            hasher.update(name.as_bytes());
        }
        for f in [p.clone(), p.with_extension("db-wal")] {
            if let Some((t, len)) = common::file_sig(&f) {
                if let Ok(d) = t.duration_since(SystemTime::UNIX_EPOCH) {
                    hasher.update(d.as_millis().to_string().as_bytes());
                }
                hasher.update(len.to_string().as_bytes());
            }
        }
    }
    format!("{:x}", hasher.finalize())
}

fn load_stats_from_disk(sig: &str) -> Option<(SessionStats, HashMap<String, Vec<String>>)> {
    let text = std::fs::read_to_string(stats_cache_path()).ok()?;
    let p: StatsPayload = serde_json::from_str(&text).ok()?;
    if p.sig != sig {
        return None;
    }
    let stats = p
        .stats
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                (
                    v.first().copied().unwrap_or(0),
                    v.get(1).copied().unwrap_or(0),
                    v.get(2).copied().unwrap_or(0),
                ),
            )
        })
        .collect();
    Some((stats, p.table_dbs))
}

fn save_stats_to_disk(sig: &str, stats: &SessionStats, table_dbs: &HashMap<String, Vec<String>>) {
    let _g = STATS_IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let payload = StatsPayload {
        sig: sig.to_string(),
        stats: stats
            .iter()
            .map(|(k, v)| (k.clone(), vec![v.0, v.1, v.2]))
            .collect(),
        table_dbs: table_dbs.clone(),
        built_at: chrono::Local::now().to_rfc3339(),
    };
    if let Ok(json) = serde_json::to_string(&payload) {
        let path = stats_cache_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// 获取会话统计：内存缓存 → 磁盘缓存 → 并行扫描（两阶段），并回写缓存
pub(crate) fn msg_stats_cached(
    decrypted: &Path,
    target_count: usize,
    progress: Option<&tauri::AppHandle>,
    emit: Option<&GraphEmitCtx>,
) -> SessionStats {
    let sig = dir_signature(decrypted);
    let cache = STATS_CACHE.get_or_init(|| Mutex::new(None));
    {
        let guard = cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = guard.as_ref() {
            if entry.sig == sig {
                return entry.stats.clone();
            }
        }
    }

    let (mut stats, table_dbs) = match load_stats_from_disk(&sig) {
        Some(v) => v,
        None => {
            let usernames = crate::wechat::annual::load_session_usernames(decrypted);
            let table_owner: HashMap<String, String> = usernames
                .iter()
                .map(|u| (common::msg_table_name(u), u.clone()))
                .collect();
            let (counts, table_dbs) = collect_msg_counts(decrypted, &table_owner, progress, emit);
            let stats: SessionStats = counts
                .into_iter()
                .map(|(u, (c, t))| (u, (c, 0, t)))
                .collect();
            (stats, table_dbs)
        }
    };

    // 需要展示的会话（按消息量排序取前 N），仅对缺活跃天数的补算
    let mut ranked: Vec<(String, i64)> = stats
        .iter()
        .filter(|(_, v)| v.0 > 0)
        .map(|(u, v)| (u.clone(), v.0))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.truncate(target_count);
    let missing: Vec<String> = ranked
        .iter()
        .filter(|(u, _)| stats.get(u).map(|v| v.1 <= 0).unwrap_or(false))
        .map(|(u, _)| u.clone())
        .collect();
    if !missing.is_empty() {
        let days = collect_active_days(decrypted, &missing, &table_dbs, progress, emit);
        for (u, d) in days {
            if let Some(v) = stats.get_mut(&u) {
                v.1 = d;
            }
        }
        save_stats_to_disk(&sig, &stats, &table_dbs);
    }

    *cache.lock().unwrap_or_else(|e| e.into_inner()) = Some(StatsCache {
        sig,
        stats: stats.clone(),
    });
    stats
}
