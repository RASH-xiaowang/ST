// ============================================================
// 社交关系图谱 — 高性能会话统计
// 自 insights.rs 拆分：并行分库扫描 + 两阶段聚合。
// ============================================================

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use super::{emit_days_chunk, emit_graph_chunk, emit_progress, message_shards, GraphEmitCtx};
use crate::wechat::modules::common;

// ============ 高性能会话统计（并行分库扫描 + 两阶段聚合 + 持久缓存） ============

/// 会话统计：username -> (消息数, 活跃天数, 最近时间)
pub(crate) type SessionStats = HashMap<String, (i64, i64, i64)>;
/// 阶段一统计：username -> (消息数, 最近时间)
pub(crate) type CountMap = HashMap<String, (i64, i64)>;

/// 阶段一：并行统计每个分库中已知会话表的消息数与最近时间，并记录表→分库映射
pub(crate) fn collect_msg_counts(
    decrypted: &Path,
    table_owner: &HashMap<String, String>,
    app: Option<&tauri::AppHandle>,
    emit: Option<&GraphEmitCtx>,
) -> (CountMap, HashMap<String, Vec<String>>) {
    let dbs = message_shards(decrypted);
    let n = dbs.len();
    if n == 0 {
        return (HashMap::new(), HashMap::new());
    }
    let workers = std::thread::available_parallelism()
        .map(|x| x.get())
        .unwrap_or(4)
        .clamp(1, 8)
        .min(n);
    let merged: Mutex<(CountMap, HashMap<String, Vec<String>>)> =
        Mutex::new((HashMap::new(), HashMap::new()));
    let done = std::sync::atomic::AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for w in 0..workers {
            let dbs = &dbs;
            let merged = &merged;
            let done = &done;
            let app = app.cloned();
            scope.spawn(move || {
                let mut local_counts: CountMap = HashMap::new();
                let mut local_dbs: HashMap<String, Vec<String>> = HashMap::new();
                let mut i = w;
                while i < dbs.len() {
                    let db = &dbs[i];
                    let rel = db
                        .strip_prefix(decrypted)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_default();
                    // 每个分库单独统计，便于按分库增量推送（避免重复推已统计会话）
                    let mut db_counts: CountMap = HashMap::new();
                    if let Ok(conn) = common::open_readonly_db(db) {
                        if let Ok(mut stmt) = conn.prepare(
                            "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'Msg\\_%' ESCAPE '\\'",
                        ) {
                            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                                for t in rows.filter_map(|r| r.ok()) {
                                    let Some(username) = table_owner.get(&t) else {
                                        continue;
                                    };
                                    let sql = format!(
                                        "SELECT COUNT(*), IFNULL(MAX(create_time),0) FROM \"{}\"",
                                        t.replace('"', "\"\"")
                                    );
                                    if let Ok(row) = conn.query_row(&sql, [], |r| {
                                        Ok((
                                            r.get::<_, i64>(0).unwrap_or(0),
                                            r.get::<_, i64>(1).unwrap_or(0),
                                        ))
                                    }) {
                                        let e = db_counts.entry(username.clone()).or_insert((0, 0));
                                        e.0 += row.0;
                                        e.1 = e.1.max(row.1);
                                    }
                                }
                            }
                        }
                    }
                    // 增量推送：该分库的节点/边已就绪
                    let d = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if let Some(ctx) = emit {
                        emit_graph_chunk(ctx, &db_counts, d, n);
                    }
                    // 合并进 worker 局部结果
                    for (u, v) in db_counts {
                        let e = local_counts.entry(u.clone()).or_insert((0, 0));
                        e.0 += v.0;
                        e.1 = e.1.max(v.1);
                        local_dbs.entry(u).or_default().push(rel.clone());
                    }
                    i += workers;
                    if emit.is_none() {
                        emit_progress(
                            app.as_ref(),
                            "scan",
                            d,
                            n,
                            &format!("扫描分库 {}/{}…", d, n),
                        );
                    }
                }
                let mut g = merged.lock().unwrap();
                for (u, v) in local_counts {
                    let e = g.0.entry(u.clone()).or_insert((0, 0));
                    e.0 += v.0;
                    e.1 = e.1.max(v.1);
                }
                for (u, v) in local_dbs {
                    g.1.entry(u).or_default().extend(v);
                }
            });
        }
    });
    merged.into_inner().unwrap()
}

/// 阶段二：对指定会话并行计算活跃天数（仅对需要展示的会话做昂贵的 DISTINCT date）
pub(crate) fn collect_active_days(
    decrypted: &Path,
    targets: &[String],
    table_dbs: &HashMap<String, Vec<String>>,
    app: Option<&tauri::AppHandle>,
    emit: Option<&GraphEmitCtx>,
) -> HashMap<String, i64> {
    if targets.is_empty() {
        return HashMap::new();
    }
    let workers = std::thread::available_parallelism()
        .map(|x| x.get())
        .unwrap_or(4)
        .clamp(1, 8)
        .min(targets.len());
    let days: Mutex<HashMap<String, i64>> = Mutex::new(HashMap::new());
    let done = std::sync::atomic::AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for w in 0..workers {
            let days = &days;
            let done = &done;
            let app = app.cloned();
            scope.spawn(move || {
                let mut i = w;
                let mut batch: Vec<serde_json::Value> = Vec::new();
                while i < targets.len() {
                    let username = &targets[i];
                    let table = common::msg_table_name(username);
                    let mut max_days = 0i64;
                    if let Some(rels) = table_dbs.get(username) {
                        for rel in rels {
                            let path = decrypted.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                            if let Ok(conn) = common::open_readonly_db(&path) {
                                let sql = format!(
                                    "SELECT COUNT(DISTINCT date(datetime(create_time,'unixepoch','localtime'))) \
                                     FROM \"{}\"",
                                    table.replace('"', "\"\"")
                                );
                                if let Ok(d) = conn.query_row(&sql, [], |r| r.get::<_, i64>(0)) {
                                    max_days = max_days.max(d);
                                }
                            }
                        }
                    }
                    if max_days > 0 {
                        days.lock().unwrap().insert(username.clone(), max_days);
                    }
                    i += workers;
                    let d = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if let Some(ctx) = emit {
                        batch.push(serde_json::json!({
                            "username": username,
                            "active_days": max_days,
                        }));
                        if batch.len() >= 5 || d == targets.len() {
                            emit_days_chunk(&ctx.app, &batch, d, targets.len());
                            batch.clear();
                        }
                    } else {
                        emit_progress(
                            app.as_ref(),
                            "days",
                            d,
                            targets.len(),
                            &format!("计算活跃天数 {}/{}…", d, targets.len()),
                        );
                    }
                }
            });
        }
    });
    days.into_inner().unwrap()
}
