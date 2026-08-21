//! 微信数据隐私体检模块
//!
//! `scan_privacy_risks`：在本地聊天记录中扫描常见敏感信息
//! （手机号 / 身份证号 / 银行卡号 / 邮箱 / 密码口令 / 地址），
//! 按类别聚合命中样本，并输出 TOP 联系人/群的风险分布。
//!
//! 设计原则：
//! - 只读解密副本，扫描结果仅在内存/响应中返回，不落盘；
//! - 带行数预算（上限 60 万行），避免大库全量扫描卡顿；
//! - 每类命中样本上限 200 条，保证响应可控。

use crate::wechat::handlers::helpers;
use crate::wechat::modules::common;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

/// 敏感信息类别
struct PrivacyCategory {
    key: &'static str,
    label: &'static str,
    icon: &'static str,
    regex: &'static str,
    /// 是否大小写不敏感
    case_insensitive: bool,
}

const CATEGORIES: &[PrivacyCategory] = &[
    PrivacyCategory {
        key: "phone",
        label: "手机号",
        icon: "📱",
        regex: r"1[3-9]\d{9}",
        case_insensitive: false,
    },
    PrivacyCategory {
        key: "id_card",
        label: "身份证号",
        icon: "🪪",
        regex: r"[1-9]\d{5}(?:19|20)\d{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])\d{3}[\dXx]",
        case_insensitive: false,
    },
    PrivacyCategory {
        key: "bank_card",
        label: "银行卡号",
        icon: "💳",
        regex: r"(?:62\d{14,17}|[45]\d{15,18})",
        case_insensitive: false,
    },
    PrivacyCategory {
        key: "email",
        label: "邮箱",
        icon: "✉️",
        regex: r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
        case_insensitive: true,
    },
    PrivacyCategory {
        key: "password",
        label: "密码口令",
        icon: "🔑",
        regex: r"(?:密码|口令|pwd|password|passwd)\s*[=:：]\s*[A-Za-z0-9@#$%^&*!_.-]{4,32}",
        case_insensitive: true,
    },
    PrivacyCategory {
        key: "address",
        label: "地址信息",
        icon: "📍",
        regex: r"(?:住在|地址|住址|小区|门牌号|大厦|宿舍)[^\n，。！？]{2,60}",
        case_insensitive: false,
    },
];

/// 命中样本
struct Hit {
    username: String,
    name: String,
    local_id: i64,
    ts: i64,
    time: String,
    snippet: String,
}

/// 生成命中片段：以首个匹配为中心截取上下文
fn make_snippet(text: &str, matched: &str) -> String {
    let idx = text.find(matched).unwrap_or(0);
    let chars: Vec<char> = text.chars().collect();
    let char_pos = text[..idx].chars().count();
    let start = char_pos.saturating_sub(24);
    let end = (char_pos + matched.chars().count() + 36).min(chars.len());
    let mid: String = chars[start..end].iter().collect();
    if start > 0 {
        format!("…{}", mid)
    } else {
        mid
    }
}

/// 执行隐私扫描
pub fn scan_privacy_risks(decrypted: &Path, row_budget: i64) -> Result<serde_json::Value, String> {
    let t0 = Instant::now();
    let usernames = crate::wechat::annual::load_session_usernames(decrypted);
    let names = crate::wechat::annual::load_display_names(decrypted, &usernames);

    let compiled: Vec<(usize, Regex)> = CATEGORIES
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            regex::RegexBuilder::new(c.regex)
                .case_insensitive(c.case_insensitive)
                .build()
                .ok()
                .map(|re| (i, re))
        })
        .collect();

    // key -> (count, hits)
    let mut counts: HashMap<&'static str, i64> = HashMap::new();
    let mut hits: HashMap<&'static str, Vec<Hit>> = HashMap::new();
    // username -> (category_count_total, per-category counts)
    let mut per_contact: HashMap<String, i64> = HashMap::new();
    let mut scanned_rows: i64 = 0;
    let mut scanned_sessions: i64 = 0;
    let mut budget: i64 = row_budget;

    let mut dbs = common::find_db_files(decrypted, "message_");
    dbs.extend(common::find_db_files(decrypted, "biz_message_"));
    dbs.sort();
    dbs.dedup();
    dbs.retain(|p| common::is_message_shard_file(p));

    'outer: for username in usernames.iter().take(800) {
        if budget <= 0 {
            break;
        }
        let table = common::msg_table_name(username);
        for db in &dbs {
            if budget <= 0 {
                break 'outer;
            }
            let Ok(conn) = common::open_readonly_db(db) else {
                continue;
            };
            if !common::table_exists(&conn, &table) {
                continue;
            }
            let sql = format!(
                "SELECT local_id, create_time, message_content FROM \"{}\" WHERE local_type=1",
                table.replace('"', "\"\"")
            );
            let Ok(mut stmt) = conn.prepare(&sql) else {
                continue;
            };
            let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    common::get_bytes(r, 2)
                        .map(|b| common::decode_blob_text(&b))
                        .unwrap_or_default(),
                ))
            }) else {
                continue;
            };
            for row in rows.flatten() {
                budget -= 1;
                scanned_rows += 1;
                if budget <= 0 {
                    break 'outer;
                }
                let (local_id, ts, text) = row;
                if text.is_empty() || text.starts_with('<') {
                    continue;
                }
                let mut any_hit = false;
                for (ci, re) in &compiled {
                    let cat = &CATEGORIES[*ci];
                    let m = re.find(&text);
                    if m.is_none() {
                        continue;
                    }
                    let matched = m.unwrap().as_str().to_string();
                    *counts.entry(cat.key).or_insert(0) += 1;
                    let hit_list = hits.entry(cat.key).or_default();
                    if hit_list.len() < 200 {
                        hit_list.push(Hit {
                            username: username.clone(),
                            name: names
                                .get(username)
                                .cloned()
                                .unwrap_or_else(|| username.clone()),
                            local_id,
                            ts,
                            time: common::format_full_time(ts),
                            snippet: make_snippet(&text, &matched),
                        });
                    }
                    any_hit = true;
                }
                if any_hit {
                    *per_contact.entry(username.clone()).or_insert(0) += 1;
                }
            }
            scanned_sessions += 1;
        }
    }

    let mut categories: Vec<serde_json::Value> = Vec::new();
    for (i, cat) in CATEGORIES.iter().enumerate() {
        let count = counts.get(cat.key).copied().unwrap_or(0);
        let samples: Vec<serde_json::Value> = hits
            .get(cat.key)
            .map(|hs| {
                hs.iter()
                    .map(|h| {
                        serde_json::json!({
                            "username": h.username,
                            "name": h.name,
                            "local_id": h.local_id,
                            "ts": h.ts,
                            "time": h.time,
                            "snippet": h.snippet,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        categories.push(serde_json::json!({
            "key": cat.key,
            "label": cat.label,
            "icon": cat.icon,
            "count": count,
            "samples": samples,
        }));
        let _ = i;
    }

    // TOP 联系人 / 群
    let mut contact_list: Vec<(String, i64)> = per_contact
        .iter()
        .filter(|(u, _)| !u.ends_with("@chatroom") && !u.starts_with("gh_"))
        .map(|(u, c)| (u.clone(), *c))
        .collect();
    contact_list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut group_list: Vec<(String, i64)> = per_contact
        .iter()
        .filter(|(u, _)| u.ends_with("@chatroom"))
        .map(|(u, c)| (u.clone(), *c))
        .collect();
    group_list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let top_contacts: Vec<serde_json::Value> = contact_list
        .into_iter()
        .take(10)
        .map(|(u, c)| {
            serde_json::json!({
                "username": u,
                "name": names.get(&u).cloned().unwrap_or_else(|| u.clone()),
                "count": c,
            })
        })
        .collect();
    let top_groups: Vec<serde_json::Value> = group_list
        .into_iter()
        .take(10)
        .map(|(u, c)| {
            serde_json::json!({
                "username": u,
                "name": names.get(&u).cloned().unwrap_or_else(|| u.clone()),
                "count": c,
            })
        })
        .collect();

    let total_hits: i64 = counts.values().sum();
    Ok(serde_json::json!({
        "scanned": {
            "rows": scanned_rows,
            "sessions": scanned_sessions,
            "elapsed_ms": t0.elapsed().as_millis() as i64,
            "budget": row_budget,
        },
        "total_hits": total_hits,
        "categories": categories,
        "top_contacts": top_contacts,
        "top_groups": top_groups,
    }))
}

/// IPC：扫描隐私风险
#[tauri::command]
pub async fn scan_privacy_risks_cmd() -> Result<serde_json::Value, String> {
    helpers::run_blocking(move || {
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("读取配置失败: {}", e))?;
        if !cfg
            .decrypted_dir
            .join("session")
            .join("session.db")
            .exists()
        {
            return Err("解密库不存在，请先完成数据库解密".to_string());
        }
        scan_privacy_risks(&cfg.decrypted_dir, 600_000)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile() -> Vec<Regex> {
        CATEGORIES
            .iter()
            .filter_map(|c| Regex::new(c.regex).ok())
            .collect()
    }

    #[test]
    fn patterns_smoke() {
        let re = compile();
        assert!(re[0].is_match("联系我 13812345678 谢谢"));
        assert!(!re[0].is_match("1381234567")); // 少一位
        assert!(re[1].is_match("身份证 11010519900307723X"));
        assert!(re[2].is_match("卡号 6222021234567890123"));
        assert!(re[3].is_match("邮箱 test@example.com"));
        assert!(re[4].is_match("密码: abc123!@#"));
        assert!(re[5].is_match("我住在北京市朝阳区某某小区3栋"));
    }

    /// 真实数据冒烟：解密库存在时应能完成扫描并返回分类
    #[test]
    #[cfg(target_os = "windows")]
    fn scan_real_data_smoke() {
        let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        if !cfg
            .decrypted_dir
            .join("session")
            .join("session.db")
            .exists()
        {
            eprintln!("解密库不存在，跳过");
            return;
        }
        let v = scan_privacy_risks(&cfg.decrypted_dir, 120_000).expect("扫描失败");
        let cats = v
            .get("categories")
            .and_then(|x| x.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let total = v.get("total_hits").and_then(|x| x.as_i64()).unwrap_or(0);
        let rows = v
            .get("scanned")
            .and_then(|s| s.get("rows"))
            .and_then(|x| x.as_i64())
            .unwrap_or(0);
        eprintln!("隐私扫描: 分类 {} 命中 {} 行 {}", cats, total, rows);
        assert_eq!(cats, CATEGORIES.len());
        assert!(rows > 0, "应扫描到消息行");
    }
}
