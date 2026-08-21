// ============================================================
// 社交关系图谱 — 图谱构建
// 自 insights.rs 拆分：共群关系、成员映射、自我账号收集、
// 节点/边组装。
// ============================================================

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::api::graph_cache_path;
use super::types::{GraphEdge, GraphNode, SharedMember};
use super::{emit_graph_final, emit_progress, msg_stats_cached, GraphEmitCtx};
use crate::wechat::config::normalize_wxid_dir;
use crate::wechat::modules::{common, contacts, sessions};

/// 群成员共群关系：room -> members（仅保留选中联系人），统计两人共同群数
pub(crate) fn shared_group_pairs(
    decrypted: &Path,
    selected_contacts: &HashSet<String>,
) -> HashMap<(String, String), (i64, HashSet<String>)> {
    let mut result: HashMap<(String, String), (i64, HashSet<String>)> = HashMap::new();
    let db = decrypted.join("contact").join("contact.db");
    let Ok(conn) = common::open_readonly_db(&db) else {
        return result;
    };
    if !common::table_exists(&conn, "chat_room") || !common::table_exists(&conn, "chatroom_member")
    {
        return result;
    }
    let mut room_group: HashMap<i64, String> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, username FROM chat_room") {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        }) {
            for r in rows.flatten() {
                if !r.1.is_empty() {
                    room_group.insert(r.0, r.1);
                }
            }
        }
    }
    let mut id_username: HashMap<i64, String> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, username FROM contact") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        {
            for r in rows.flatten() {
                id_username.insert(r.0, r.1);
            }
        }
    }
    if let Ok(mut stmt) = conn.prepare("SELECT room_id, member_id FROM chatroom_member") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))) {
            let mut room_members: HashMap<i64, Vec<String>> = HashMap::new();
            for r in rows.flatten() {
                if let Some(u) = id_username.get(&r.1) {
                    if selected_contacts.contains(u) {
                        room_members.entry(r.0).or_default().push(u.clone());
                    }
                }
            }
            for (room_id, members) in room_members {
                let Some(group) = room_group.get(&room_id).cloned() else {
                    continue;
                };
                let mut uniq: Vec<String> = members;
                uniq.sort();
                uniq.dedup();
                for i in 0..uniq.len() {
                    for j in (i + 1)..uniq.len() {
                        let key = if uniq[i] < uniq[j] {
                            (uniq[i].clone(), uniq[j].clone())
                        } else {
                            (uniq[j].clone(), uniq[i].clone())
                        };
                        let e = result.entry(key).or_insert((0, HashSet::new()));
                        e.0 += 1;
                        e.1.insert(group.clone());
                    }
                }
            }
        }
    }
    result
}

/// 成员 → 所在群列表（全量，用于共同群边与群命中统计）。
/// 微信 chatroom_member 表以 contact 表 id 关联成员，先做 id→username 映射。
pub(crate) fn member_group_map(decrypted: &Path) -> HashMap<String, Vec<String>> {
    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    let db = decrypted.join("contact").join("contact.db");
    let Ok(conn) = common::open_readonly_db(&db) else {
        return result;
    };
    if !common::table_exists(&conn, "chat_room") || !common::table_exists(&conn, "chatroom_member")
    {
        return result;
    }
    let mut room_group: HashMap<i64, String> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, username FROM chat_room") {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        }) {
            for r in rows.flatten() {
                if !r.1.is_empty() {
                    room_group.insert(r.0, r.1);
                }
            }
        }
    }
    let mut id_username: HashMap<i64, String> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, username FROM contact") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        {
            for r in rows.flatten() {
                id_username.insert(r.0, r.1);
            }
        }
    }
    if let Ok(mut stmt) = conn.prepare("SELECT room_id, member_id FROM chatroom_member") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))) {
            for r in rows.flatten() {
                let Some(username) = id_username.get(&r.1) else {
                    continue;
                };
                let Some(group) = room_group.get(&r.0).cloned() else {
                    continue;
                };
                let list = result.entry(username.clone()).or_default();
                if !list.contains(&group) {
                    list.push(group);
                }
            }
        }
    }
    result
}

/// 收集本机微信数据根目录下全部账号目录的真实 wxid
/// （当前账号 + 同一用户在本机登录过的其他账号实例），用于把「我自己」从联系人中排除。
pub(crate) fn collect_self_accounts(
    wechat_base_dir: &Path,
    self_username: &str,
) -> HashSet<String> {
    let mut set = HashSet::new();
    set.insert(normalize_wxid_dir(self_username));
    if let Some(root) = wechat_base_dir.parent() {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("wxid_") {
                    set.insert(normalize_wxid_dir(&name));
                }
            }
        }
    }
    set
}

/// 生成关系图谱数据
pub fn build_relationship_graph(
    decrypted: &Path,
    wechat_base_dir: &Path,
    self_username: &str,
    contact_limit: Option<usize>,
    group_limit: Option<usize>,
    progress: Option<&tauri::AppHandle>,
) -> Result<serde_json::Value, String> {
    emit_progress(progress, "build", 0, 1, "开始构建关系图谱…");
    // 先加载会话元数据（名称/类型），供增量推送与最终组装使用
    let session_list = sessions::get_session_list(decrypted).unwrap_or_default();
    let mut name_map: HashMap<String, String> = HashMap::new();
    let mut is_group_map: HashMap<String, bool> = HashMap::new();
    let mut is_official_map: HashMap<String, bool> = HashMap::new();
    for s in &session_list {
        name_map.insert(s.username.clone(), s.name.clone());
        is_group_map.insert(s.username.clone(), s.is_group);
        is_official_map.insert(s.username.clone(), s.is_official);
    }
    let emit_ctx = progress.map(|app| {
        std::sync::Arc::new(GraphEmitCtx {
            app: app.clone(),
            self_username: self_username.to_string(),
            name_map: name_map.clone(),
            is_group_map: is_group_map.clone(),
            is_official_map: is_official_map.clone(),
        })
    });
    // None = 全部：target_count 用 usize::MAX，ranked.truncate 不截断 → 全部会话都算活跃天数
    let target_count = contact_limit
        .unwrap_or(usize::MAX)
        .saturating_add(group_limit.unwrap_or(usize::MAX));
    let stats = msg_stats_cached(decrypted, target_count, progress, emit_ctx.as_deref());
    emit_progress(progress, "build", 1, 1, "统计完成，正在组装图谱…");
    let contact_db = decrypted.join("contact").join("contact.db");
    let display_names = contacts::load_display_names(&contact_db);
    // 本机已知的全部微信账号（含当前账号）＝“我自己”，不出现在联系人节点中，
    // 避免同一个用户的多账号在社交图谱里被当作普通联系人重复展示
    let self_accounts = collect_self_accounts(wechat_base_dir, self_username);
    // 通讯录元数据：群成员数 / 好友标记 / 头像
    let mut member_counts: HashMap<String, i64> = HashMap::new();
    let mut contact_meta: HashMap<String, (bool, String)> = HashMap::new();
    // 真实通讯录口径（与通讯录面板「全部」一致：friend/member/enterprise/
    // group/official/service 六个可见分类合计）：供前端展示总数
    const VISIBLE_CATS: [&str; 6] = [
        "friend",
        "member",
        "enterprise",
        "group",
        "official",
        "service",
    ];
    let mut contact_book_total = 0usize;
    let mut contact_book_friends = 0usize;
    let mut contact_book_members = 0usize;
    let mut contact_book_official = 0usize;
    if let Ok(book) = contacts::get_contacts(decrypted) {
        for c in book.contacts {
            if VISIBLE_CATS.contains(&c.category.as_str()) {
                contact_book_total += 1;
                match c.category.as_str() {
                    // 真实好友数 = local_type=1 且排除本机当前账号
                    // （微信自身的好友列表不会把自己算进去）
                    "friend" if c.username != self_username => contact_book_friends += 1,
                    "member" => contact_book_members += 1,
                    "official" => contact_book_official += 1,
                    _ => {}
                }
            }
            if let Some(n) = c.member_count {
                member_counts.insert(c.username.clone(), n);
            }
            contact_meta.insert(c.username.clone(), (c.category == "friend", c.avatar_url));
        }
    }
    // 成员 → 群列表（共同群边 / 群命中统计）
    let member_groups = member_group_map(decrypted);

    // 按消息量排序选节点；全量好友（即使无消息记录）也必须进入图谱
    let all_friends: Vec<(String, (i64, i64, i64))> = contact_meta
        .iter()
        .filter(|(u, (is_friend, _))| *is_friend && *u != self_username)
        .map(|(u, _)| {
            let s = stats.get(u).copied().unwrap_or((0, 0, 0));
            (u.clone(), s)
        })
        .collect();
    let mut contacts_sorted: Vec<(String, (i64, i64, i64))> = stats
        .iter()
        .filter(|(u, _)| !u.ends_with("@chatroom") && !u.starts_with("gh_"))
        .map(|(u, s)| (u.clone(), *s))
        .collect();
    for (u, s) in &all_friends {
        if !contacts_sorted.iter().any(|(x, _)| x == u) {
            contacts_sorted.push((u.clone(), *s));
        }
    }
    contacts_sorted.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
    let mut groups_sorted: Vec<(String, (i64, i64, i64))> = stats
        .iter()
        .filter(|(u, _)| u.ends_with("@chatroom"))
        .map(|(u, s)| (u.clone(), *s))
        .collect();
    groups_sorted.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
    if let Some(n) = contact_limit {
        contacts_sorted.truncate(n.max(1));
    }
    if let Some(n) = group_limit {
        groups_sorted.truncate(n.max(1));
    }

    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut selected_contacts: HashSet<String> = HashSet::new();
    let mut selected_groups: HashSet<String> = HashSet::new();

    // 「我」的头像：优先从本地头像库（head_image.db）取 data URL，
    // 取不到时回退通讯录远程头像地址；两者皆无则为空
    let self_avatar = crate::wechat::config::WeChatConfig::load()
        .ok()
        .and_then(|cfg| {
            let aes_key: Option<Vec<u8>> = cfg
                .image_aes_key
                .as_ref()
                .filter(|k| k.len() == 16)
                .map(|k| k.as_bytes().to_vec());
            let v = crate::wechat::modules::avatar::get_user_avatar(
                decrypted,
                wechat_base_dir,
                self_username,
                aes_key.as_deref(),
                cfg.image_xor_key,
            );
            v.get("data")
                .and_then(|d| d.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();

    nodes.push(GraphNode {
        id: self_username.to_string(),
        label: "我".to_string(),
        kind: "self".to_string(),
        msg_count: 0,
        active_days: 0,
        last_ts: 0,
        member_count: 0,
        group_count: 0,
        group_codes: Vec::new(),
        is_friend: true,
        shared_count: 0,
        avatar_url: self_avatar.clone(),
        shared_members: Vec::new(),
    });

    for (u, s) in contacts_sorted {
        // 只排除当前账号本体：本机其他账号若出现在当前通讯录中，
        // 说明确实被添加为好友（如「我的另一个号」），应正常展示
        if u == self_username {
            continue;
        }
        let label = name_map
            .get(&u)
            .cloned()
            .or_else(|| display_names.get(&u).cloned())
            .unwrap_or_else(|| u.clone());
        let kind = if is_official_map.get(&u).copied().unwrap_or(false) {
            "official"
        } else {
            "contact"
        };
        let (is_friend, avatar_url) = contact_meta.get(&u).cloned().unwrap_or_default();
        let mut group_codes: Vec<String> = member_groups.get(&u).cloned().unwrap_or_default();
        group_codes.sort();
        nodes.push(GraphNode {
            id: u.clone(),
            label,
            kind: kind.to_string(),
            msg_count: s.0,
            active_days: s.1,
            last_ts: s.2,
            member_count: 0,
            group_count: group_codes.len(),
            group_codes,
            is_friend,
            shared_count: 0,
            avatar_url,
            shared_members: Vec::new(),
        });
        selected_contacts.insert(u);
    }
    for (u, s) in groups_sorted {
        let label = name_map
            .get(&u)
            .cloned()
            .or_else(|| display_names.get(&u).cloned())
            .unwrap_or_else(|| u.clone());
        // 命中成员明细：在已选联系人中，取共同在该群的人，按消息量排序取前 8
        let mut shared: Vec<(String, i64)> = selected_contacts
            .iter()
            .filter(|c| member_groups.get(*c).is_some_and(|g| g.contains(&u)))
            .map(|c| {
                let mc = stats.get(c).map(|x| x.0).unwrap_or(0);
                (c.clone(), mc)
            })
            .collect();
        shared.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let shared_count = shared.len() as i64;
        let shared_members: Vec<SharedMember> = shared
            .into_iter()
            .take(8)
            .map(|(c, mc)| {
                let name = name_map
                    .get(&c)
                    .cloned()
                    .or_else(|| display_names.get(&c).cloned())
                    .unwrap_or_else(|| c.clone());
                let (is_friend, _) = contact_meta.get(&c).cloned().unwrap_or_default();
                SharedMember {
                    username: c,
                    name,
                    is_friend,
                    msg_count: mc,
                }
            })
            .collect();
        let (_, avatar_url) = contact_meta.get(&u).cloned().unwrap_or_default();
        nodes.push(GraphNode {
            id: u.clone(),
            label,
            kind: "group".to_string(),
            msg_count: s.0,
            active_days: s.1,
            last_ts: s.2,
            member_count: member_counts.get(&u).copied().unwrap_or(0),
            group_count: 0,
            group_codes: Vec::new(),
            is_friend: false,
            shared_count,
            avatar_url,
            shared_members,
        });
        selected_groups.insert(u);
    }
    let _ = &selected_groups;

    // 边：我 → 联系人/群（消息强度）
    let mut edges: Vec<GraphEdge> = Vec::new();
    for n in nodes.iter().filter(|n| n.kind != "self") {
        edges.push(GraphEdge {
            source: self_username.to_string(),
            target: n.id.clone(),
            weight: n.msg_count.max(1),
            msg_count: n.msg_count,
            active_days: n.active_days,
            last_ts: n.last_ts,
            kinds: vec!["message".to_string()],
        });
    }

    // 边：联系人 ↔ 联系人（共群）
    for ((a, b), (groups, _)) in shared_group_pairs(decrypted, &selected_contacts) {
        if selected_contacts.contains(&a) && selected_contacts.contains(&b) {
            edges.push(GraphEdge {
                source: a,
                target: b,
                weight: groups,
                msg_count: 0,
                active_days: 0,
                last_ts: 0,
                kinds: vec!["group".to_string()],
            });
        }
    }

    // 汇总
    let total_msgs: i64 = stats.values().map(|s| s.0).sum();
    let total_contacts = stats
        .keys()
        .filter(|u| !u.ends_with("@chatroom") && !u.starts_with("gh_"))
        .count();
    let total_groups = stats.keys().filter(|u| u.ends_with("@chatroom")).count();
    let top_relations: Vec<serde_json::Value> = edges
        .iter()
        .filter(|e| e.source == self_username)
        .filter(|e| e.kinds.contains(&"message".to_string()))
        .take(5)
        .map(|e| {
            let label = nodes
                .iter()
                .find(|n| n.id == e.target)
                .map(|n| n.label.clone())
                .unwrap_or_else(|| e.target.clone());
            serde_json::json!({
                "username": e.target,
                "name": label,
                "msg_count": e.msg_count,
                "active_days": e.active_days,
            })
        })
        .collect();
    let mut sorted_self_accounts: Vec<String> = self_accounts.iter().cloned().collect();
    sorted_self_accounts.sort();
    // 全部群 code → 群名（供前端展示「共同群」列表，不限于已选群节点；
    // 会话库缺失的群名再从通讯录补）
    let mut group_names: HashMap<String, String> = name_map
        .iter()
        .filter(|(u, _)| u.ends_with("@chatroom"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (u, name) in display_names.iter() {
        if u.ends_with("@chatroom") {
            group_names.entry(u.clone()).or_insert_with(|| name.clone());
        }
    }

    let data = serde_json::json!({
        "self": self_username,
        "self_avatar": self_avatar,
        "self_accounts": {
            "wxids": sorted_self_accounts,
            "current": self_username,
        },
        "group_names": group_names,
        "nodes": nodes,
        "edges": edges,
        "summary": {
            "total_contacts": total_contacts,
            "total_groups": total_groups,
            "total_messages": total_msgs,
            "contact_book_total": contact_book_total,
            "contact_book_friends": contact_book_friends,
            "contact_book_members": contact_book_members,
            "contact_book_official": contact_book_official,
            "selected_contacts": selected_contacts.len(),
            "selected_groups": selected_groups.len(),
            "top_relations": top_relations,
        },
    });
    // 组装完成：推送完整数据，前端以 finalData 覆盖增量结果
    if let Some(app) = progress {
        emit_graph_final(app, &data);
    }
    // 落盘缓存：下次进入图谱可先秒开上次结果，再由前端后台刷新
    let cache_path = graph_cache_path();
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_vec(&data) {
        if std::fs::write(&cache_path, json).is_err() {
            log::warn!("[graph] 关系图谱缓存写入失败");
        }
    }
    Ok(data)
}
