//! 通讯录模块 - 对应 PC 微信「通讯录」界面
//!
//! 数据来源：`contact/contact.db`
//! - `contact`           联系人主表（好友 / 群聊 / 公众号 / 群成员）
//! - `chat_room`         群聊附加信息（群主）
//! - `chatroom_member`   群成员关系（room_id → contact.id）
//! - `contact_label`     标签定义
//! - `stranger`          陌生人
//!
//! contact.local_type 语义（依据数据库分析文档）：
//!   0 = 普通好友  1 = 群聊  2 = 公众号  3 = 群成员  4 = 已删除

use super::common;
use crate::wechat::modules::common::DbSigPair;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

// ============ 缓存基础设施（mtime 感知，避免重复全表扫描）============

/// 显示名缓存条目
struct DisplayNamesEntry {
    sig: DbSigPair,
    names: HashMap<String, String>,
}

static DISPLAY_NAMES_CACHE: OnceLock<Mutex<Option<DisplayNamesEntry>>> = OnceLock::new();

/// 通讯录全量缓存条目（Arc 共享，分页时无需克隆整个通讯录）
struct ContactBookEntry {
    sig: DbSigPair,
    book: Arc<ContactBook>,
}

static CONTACT_BOOK_CACHE: OnceLock<Mutex<Option<ContactBookEntry>>> = OnceLock::new();

/// 通讯录条目（与 PC 微信通讯录一致）
#[derive(Debug, Clone, Serialize)]
pub struct ContactEntry {
    /// 微信内部用户名（wxid / @chatroom / gh_）
    pub username: String,
    /// 备注名（我设置的）
    pub remark: String,
    /// 昵称
    pub nick_name: String,
    /// 显示名（PC 规则：备注 > 昵称 > username）
    pub display_name: String,
    /// 微信号（用户自定义 ID）
    pub alias: String,
    /// 原始类型
    pub local_type: i64,
    /// 类型中文名
    pub local_type_label: String,
    /// 分类：friend / group / official / system / member / deleted
    pub category: String,
    /// 拼音首字母（PC 通讯录分组依据）
    pub initial: String,
    /// 全拼
    pub quan_pin: String,
    /// 头像 URL（远程）
    pub avatar_url: String,
    /// 描述/签名
    pub description: String,
    /// 群成员数（仅群聊）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<i64>,
    /// 群主 username（仅群聊）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// 群主显示名（仅群聊，已解析备注/昵称，供列表直接展示）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_name: Option<String>,
    /// 标签 ID 列表
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_ids: Option<String>,
    /// 所在群聊显示名（仅群成员，前端按群分组展示用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    /// 所在群聊 username（仅群成员，资料卡「所在群」点击跳转用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_username: Option<String>,
}

/// 通讯录总览
#[derive(Debug, Clone, Serialize)]
pub struct ContactBook {
    pub contacts: Vec<ContactEntry>,
    /// 标签定义（label_id → 名称）
    pub labels: Vec<serde_json::Value>,
    /// 分类统计
    pub stats: serde_json::Value,
}

/// 联系人分类（前端筛选用）。
///
/// 根据 contact 表 local_type 字段的精确语义：
///   local_type: 0=好友 1=群聊 2=公众号 3=群成员 4=已删除
///   + username 后缀/前缀做辅助判断
///
/// 六分类完全互斥：
///   friend     = local_type=0 的个人好友
///   group      = @chatroom 后缀的群聊
///   official   = gh_ 开头的公众号（子类型从 biz_info.type 区分 公众号/服务号）
///   enterprise = @openim 后缀的企业微信联系人
///   member     = local_type=3 的群成员
///   deleted    = delete_flag≠0 或 local_type=4（隐藏）
///   system     = 内置通知账号（隐藏）
fn category_of(local_type: i64, username: &str, delete_flag: i64) -> &'static str {
    if delete_flag != 0 || local_type == 4 {
        return "deleted";
    }
    if username.ends_with("@chatroom") {
        return "group";
    }
    if common::is_official_account(username) {
        return "official";
    }
    if common::is_builtin_account(username) {
        return "system";
    }
    if username.ends_with("@kefu.openim") {
        return "service";
    }
    if username.ends_with("@openim") {
        return "enterprise";
    }
    // 微信 4.x：local_type=1 → 真实好友；
    // 3 → 群成员；0/2 等 → 未加好友的联系人（归入 member，避免误算成好友）
    if local_type == 1 {
        return "friend";
    }
    if local_type == 3 {
        return "member";
    }
    "member"
}

/// 计算拼音首字母（A-Z / #）
fn initial_of(remark_initial: &str, nick_initial: &str, display: &str) -> String {
    let raw = if !remark_initial.is_empty() {
        remark_initial
    } else if !nick_initial.is_empty() {
        nick_initial
    } else {
        display
    };
    let ch = raw.chars().next().unwrap_or('#');
    let up = ch.to_ascii_uppercase();
    if up.is_ascii_alphabetic() {
        up.to_string()
    } else {
        "#".to_string()
    }
}

/// 加载 username → 显示名 映射（备注 > 昵称），供其他模块使用
///
/// 带 mtime 感知缓存：会话列表 / 消息 / 朋友圈 / 收藏每次查询都会调用本函数，
/// 重复全表扫描 contact.db 是浏览链路的最大热点之一。
pub fn load_display_names(contact_db: &Path) -> HashMap<String, String> {
    let sig = common::db_sig(contact_db);
    let cache = DISPLAY_NAMES_CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = guard.as_ref() {
        if entry.sig == sig {
            return entry.names.clone();
        }
    }
    let names = load_display_names_uncached(contact_db);
    *guard = Some(DisplayNamesEntry {
        sig,
        names: names.clone(),
    });
    names
}

fn load_display_names_uncached(contact_db: &Path) -> HashMap<String, String> {
    let mut names = HashMap::new();
    if !contact_db.exists() {
        log::warn!("[contact] contact.db 不存在: {}", contact_db.display());
        return names;
    }
    let conn = match common::open_readonly_db(contact_db) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[contact] 无法打开 contact.db: {}", e);
            return names;
        }
    };
    if !common::table_exists(&conn, "contact") {
        log::warn!("[contact] contact 表不存在");
        return names;
    }
    if let Ok(mut stmt) = conn.prepare("SELECT username, nick_name, remark FROM contact") {
        if let Ok(rows) = stmt.query_map([], |row| {
            let username: String = row.get(0)?;
            let nick: Option<String> = row.get(1)?;
            let remark: Option<String> = row.get(2)?;
            Ok((username, nick, remark))
        }) {
            for r in rows.flatten() {
                let display =
                    r.2.filter(|s| !s.is_empty())
                        .or(r.1.filter(|s| !s.is_empty()))
                        .unwrap_or_else(|| r.0.clone());
                names.insert(r.0, display);
            }
        }
    }
    log::info!("[contact] 加载了 {} 个联系人名称", names.len());
    names
}

/// 读取完整通讯录（带 mtime 感知缓存，Arc 共享避免每次分页重复扫描）
pub fn get_contacts(decrypted_dir: &Path) -> Result<ContactBook, String> {
    get_contacts_arc(decrypted_dir).map(|b| (*b).clone())
}

/// 获取缓存通讯录的 Arc 引用（分页 / 导出直接过滤，不克隆整个通讯录）
fn get_contacts_arc(decrypted_dir: &Path) -> Result<Arc<ContactBook>, String> {
    let db_path = decrypted_dir.join("contact").join("contact.db");
    let sig = common::db_sig(&db_path);
    let cache = CONTACT_BOOK_CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = guard.as_ref() {
        if entry.sig == sig {
            return Ok(entry.book.clone());
        }
    }
    let book = Arc::new(get_contacts_uncached(decrypted_dir)?);
    *guard = Some(ContactBookEntry {
        sig,
        book: book.clone(),
    });
    Ok(book)
}

fn get_contacts_uncached(decrypted_dir: &Path) -> Result<ContactBook, String> {
    let db_path = decrypted_dir.join("contact").join("contact.db");
    if !db_path.exists() {
        return Err(format!("联系人数据库未解密: {}", db_path.display()));
    }
    let conn = common::open_readonly_db(&db_path).map_err(|e| format!("打开失败: {}", e))?;
    if !common::table_exists(&conn, "contact") {
        return Err("contact 表不存在".to_string());
    }
    let cols = common::table_columns(&conn, "contact");
    let has = |c: &str| cols.iter().any(|x| x == c);

    // 群成员统计 room_id → 人数
    let mut member_counts: HashMap<i64, i64> = HashMap::new();
    if common::table_exists(&conn, "chatroom_member") {
        if let Ok(mut stmt) =
            conn.prepare("SELECT room_id, count(*) FROM chatroom_member GROUP BY room_id")
        {
            if let Ok(rows) =
                stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            {
                for r in rows.flatten() {
                    member_counts.insert(r.0, r.1);
                }
            }
        }
    }

    // 群主 id → owner username
    let mut owners: HashMap<i64, String> = HashMap::new();
    if common::table_exists(&conn, "chat_room") {
        if let Ok(mut stmt) = conn.prepare("SELECT id, owner FROM chat_room") {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
            }) {
                for r in rows.flatten() {
                    if let Some(o) = r.1 {
                        owners.insert(r.0, o);
                    }
                }
            }
        }
    }

    // username → 显示名（复用全局缓存），用于解析群主显示名
    let display_names = load_display_names(&db_path);

    // 群成员归属：member contact.id → 所在群 contact.id（room_id 即群聊行的 contact.id）
    let mut member_rooms: HashMap<i64, i64> = HashMap::new();
    if common::table_exists(&conn, "chatroom_member") {
        if let Ok(mut stmt) = conn.prepare("SELECT room_id, member_id FROM chatroom_member") {
            if let Ok(rows) =
                stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            {
                for r in rows.flatten() {
                    member_rooms.insert(r.1, r.0);
                }
            }
        }
    }

    // 群 contact.id → 群 username（资料卡「所在群」跳转用）
    let mut room_usernames: HashMap<i64, String> = HashMap::new();
    if common::table_exists(&conn, "chat_room") {
        if let Ok(mut stmt) = conn.prepare("SELECT id, username FROM chat_room") {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
            }) {
                for r in rows.flatten() {
                    if let Some(u) = r.1 {
                        room_usernames.insert(r.0, u);
                    }
                }
            }
        }
    }

    // 标签定义
    let mut labels = Vec::new();
    if common::table_exists(&conn, "contact_label") {
        if let Some((cols, rows)) =
            common::dump_table(&conn, "contact_label", Some("label_id_"), 200)
        {
            for row in rows {
                let mut obj = serde_json::Map::new();
                for (i, c) in cols.iter().enumerate() {
                    obj.insert(
                        c.clone(),
                        row.get(i).cloned().unwrap_or(serde_json::Value::Null),
                    );
                }
                labels.push(serde_json::Value::Object(obj));
            }
            labels.reverse(); // dump 按 DESC，恢复升序
        }
    }

    // 企业微信 @openim 好友的 domain 集合（用于 enterprise 分类）
    // 公众号类型映射（biz_info 表：订阅号 vs 服务号）
    let mut biz_official_type: HashMap<String, i64> = HashMap::new();
    if common::table_exists(&conn, "biz_info") {
        let bi_cols = common::table_columns(&conn, "biz_info");
        if bi_cols.iter().any(|c| c == "type") && bi_cols.iter().any(|c| c == "username") {
            if let Ok(mut stmt) = conn.prepare("SELECT username, type FROM biz_info") {
                if let Ok(rows) = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                }) {
                    for r in rows.flatten() {
                        biz_official_type.insert(r.0, r.1);
                    }
                }
            }
        }
    }

    // 主查询
    let sel = |c: &str| {
        if has(c) {
            format!("\"{}\"", c)
        } else {
            "NULL".to_string()
        }
    };

    // 群 contact.id → 群显示名（备注 > 昵称 > username），供群成员条目标注所在群
    let mut room_display: HashMap<i64, String> = HashMap::new();
    if common::table_exists(&conn, "chat_room") && has("remark") && has("nick_name") {
        let room_sql = format!(
            "SELECT c.{id}, COALESCE(NULLIF(c.{remark}, ''), NULLIF(c.{nick}, ''), cr.username) \
             FROM chat_room cr JOIN contact c ON c.username = cr.username",
            id = sel("id"),
            remark = sel("remark"),
            nick = sel("nick_name"),
        );
        if let Ok(mut stmt) = conn.prepare(&room_sql) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            }) {
                for r in rows.flatten() {
                    room_display.insert(r.0, r.1);
                }
            }
        }
    }

    let sql = format!(
        "SELECT {id}, {username}, {nick}, {remark}, {alias}, {lt}, {df}, \
         {qpn}, {rpn}, {rpi}, {pi}, {big}, {small}, {desc}, {lb} \
         FROM contact ORDER BY rowid ASC",
        id = sel("id"),
        username = sel("username"),
        nick = sel("nick_name"),
        remark = sel("remark"),
        alias = sel("alias"),
        lt = sel("local_type"),
        df = sel("delete_flag"),
        qpn = sel("quan_pin"),
        rpn = sel("remark_quan_pin"),
        rpi = sel("remark_pin_yin_initial"),
        pi = sel("pin_yin_initial"),
        big = sel("big_head_url"),
        small = sel("small_head_url"),
        desc = sel("description"),
        lb = sel("label_id_list"),
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("查询失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<i64>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, Option<String>>(14)?,
            ))
        })
        .map_err(|e| format!("读取失败: {}", e))?;

    let mut contacts = Vec::new();
    let mut stats = serde_json::json!({"friend": 0, "enterprise": 0, "group": 0, "official": 0, "service": 0, "member": 0, "system": 0, "deleted": 0});
    let mut lt_counts = (0i64, 0i64, 0i64, 0i64, 0i64); // local_type 0,1,2,3,4+
                                                        // 本机当前账号不出现在通讯录里（微信客户端也不会把自己算进好友），
                                                        // 保证通讯录面板与关系图谱的好友口径一致
    let self_username = crate::wechat::config::WeChatConfig::load()
        .ok()
        .and_then(|c| c.wxid());

    for r in rows.flatten() {
        let id = r.0.unwrap_or(0);
        let username = r.1.unwrap_or_default();
        if username.is_empty() {
            continue;
        }
        if let Some(s) = self_username.as_deref() {
            if username == s {
                continue;
            }
        }
        let nick = r.2.unwrap_or_default();
        let remark = r.3.unwrap_or_default();
        let alias = r.4.unwrap_or_default();
        let local_type = r.5.unwrap_or(0);
        let delete_flag = r.6.unwrap_or(0);
        let mut category = category_of(local_type, &username, delete_flag).to_string();
        // 进一步从 official 中拆分出 service（服务号），前端需要分开显示
        if category == "official" {
            let biz_type = biz_official_type.get(&username).copied();
            if biz_type == Some(1) || biz_type == Some(3) || biz_type == Some(5) {
                category = "service".to_string();
            }
        }
        let category_label = match category.as_str() {
            "friend" => "联系人",
            "enterprise" => "企业微信联系人",
            "group" => "群聊",
            "service" => "服务号",
            "official" => "公众号",
            "member" => "群成员",
            "system" => "系统",
            "deleted" => "已删除",
            _ => "其他",
        };
        let display = common::system_account_name(&username)
            .map(|s| s.to_string())
            .or_else(|| {
                if !remark.is_empty() {
                    Some(remark.clone())
                } else {
                    None
                }
            })
            .or_else(|| {
                if !nick.is_empty() {
                    Some(nick.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| username.clone());
        let initial = initial_of(
            &r.9.clone().unwrap_or_default(),
            &r.10.clone().unwrap_or_default(),
            &display,
        );
        // 统计 local_type 分布
        match local_type {
            0 => lt_counts.0 += 1,
            1 => lt_counts.1 += 1,
            2 => lt_counts.2 += 1,
            3 => lt_counts.3 += 1,
            _ => lt_counts.4 += 1,
        }
        let is_group = category == "group";
        let n = stats.get(&category).and_then(|v| v.as_i64()).unwrap_or(0);
        stats[category.as_str()] = serde_json::json!(n + 1);

        // 群成员：查其所在群显示名与 username（分组展示 + 资料卡跳转）
        let (group_name, group_username) = if category == "member" {
            let rid = member_rooms.get(&id).copied();
            (
                rid.and_then(|rid| room_display.get(&rid).cloned()),
                rid.and_then(|rid| room_usernames.get(&rid).cloned()),
            )
        } else {
            (None, None)
        };

        contacts.push(ContactEntry {
            username: username.clone(),
            remark,
            nick_name: nick,
            display_name: display,
            alias,
            local_type,
            local_type_label: category_label.to_string(),
            category,
            initial,
            quan_pin: r.7.or(r.8).unwrap_or_default(),
            avatar_url: r.12.clone().or_else(|| r.11.clone()).unwrap_or_default(),
            description: r.13.unwrap_or_default(),
            member_count: if is_group {
                member_counts.get(&id).copied()
            } else {
                None
            },
            owner: if is_group {
                owners.get(&id).cloned()
            } else {
                None
            },
            owner_name: if is_group {
                owners
                    .get(&id)
                    .and_then(|o| display_names.get(o).cloned())
                    .or_else(|| owners.get(&id).cloned())
            } else {
                None
            },
            label_ids: r.14,
            group_name,
            group_username,
        });
    }

    // PC 通讯录排序：按显示名拼音（这里用 initial + quan_pin 近似）
    contacts.sort_by(|a, b| {
        a.initial
            .cmp(&b.initial)
            .then(a.quan_pin.cmp(&b.quan_pin))
            .then(a.display_name.cmp(&b.display_name))
    });

    log::info!(
        "[contact_stats] friend={}, enterprise={}, group={}, official={}, service={}, member={}, system={}, deleted={}",
        stats.get("friend").and_then(|v| v.as_i64()).unwrap_or(0),
        stats.get("enterprise").and_then(|v| v.as_i64()).unwrap_or(0),
        stats.get("group").and_then(|v| v.as_i64()).unwrap_or(0),
        stats.get("official").and_then(|v| v.as_i64()).unwrap_or(0),
        stats.get("service").and_then(|v| v.as_i64()).unwrap_or(0),
        stats.get("member").and_then(|v| v.as_i64()).unwrap_or(0),
        stats.get("system").and_then(|v| v.as_i64()).unwrap_or(0),
        stats.get("deleted").and_then(|v| v.as_i64()).unwrap_or(0),
    );
    log::info!(
        "[contact_debug] local_type 分布: 0={}, 1={}, 2={}, 3={}, 4+={} ; contact 总数={}",
        lt_counts.0,
        lt_counts.1,
        lt_counts.2,
        lt_counts.3,
        lt_counts.4,
        contacts.len(),
    );

    Ok(ContactBook {
        contacts,
        labels,
        stats,
    })
}

/// 通讯录分页结果（懒加载使用）
#[derive(Debug, Serialize)]
pub struct ContactPage {
    /// 当前页联系人
    pub contacts: Vec<ContactEntry>,
    /// 该分类下联系人总数
    pub total: usize,
    /// 是否还有更多数据可加载
    #[serde(rename = "has_more")]
    pub has_more: bool,
}

/// 按分类分页获取联系人（懒加载，支持全库关键词搜索）。
///
/// 复用 `get_contacts` 的全量内存数据，按 `category` 过滤后再分页切片，
/// 避免每次分页都重新扫描整个数据库。
/// `query` 非空时对 显示名/昵称/备注/微信号/username/全拼 做不区分大小写的子串匹配，
/// 前端输入搜索词即可跨页搜索全部联系人（不限于已加载的分页）。
pub fn get_contacts_page(
    decrypted_dir: &Path,
    category: &str,
    offset: usize,
    limit: usize,
    query: Option<&str>,
) -> Result<ContactPage, String> {
    // 使用 Arc 缓存引用，过滤时只克隆本页条目，避免每次翻页克隆整个通讯录
    let book = get_contacts_arc(decrypted_dir)?;
    // "全部"只显示用户可见的六个分类（friend/member/enterprise/group/official/service），
    // 排除 system/deleted 等隐藏分类
    let visible_cats = [
        "friend",
        "member",
        "enterprise",
        "group",
        "official",
        "service",
    ];
    let q = query
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    let matches = |c: &ContactEntry| -> bool {
        let Some(q) = &q else { return true };
        c.display_name.to_lowercase().contains(q)
            || c.nick_name.to_lowercase().contains(q)
            || c.remark.to_lowercase().contains(q)
            || c.alias.to_lowercase().contains(q)
            || c.username.to_lowercase().contains(q)
            || c.quan_pin.to_lowercase().contains(q)
    };
    let cat_ok = |c: &ContactEntry| -> bool {
        if category == "all" {
            visible_cats.contains(&c.category.as_str())
        } else {
            c.category == category
        }
    };
    let filtered: Vec<&ContactEntry> = book
        .contacts
        .iter()
        .filter(|c| cat_ok(c) && matches(c))
        .collect();
    let total = filtered.len();
    let page: Vec<ContactEntry> = filtered
        .into_iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect();
    let has_more = offset + page.len() < total;
    Ok(ContactPage {
        contacts: page,
        total,
        has_more,
    })
}

/// 按 username 查询单个联系人资料（供聊天窗口「资料卡」使用）
pub fn get_contact_profile(decrypted_dir: &Path, username: &str) -> Option<ContactEntry> {
    let db = decrypted_dir.join("contact").join("contact.db");
    if !db.is_file() {
        return None;
    }
    let conn = rusqlite::Connection::open_with_flags(
        &db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let raw = {
        let mut found: Option<serde_json::Value> = None;
        for col in ["userName", "username", "UserName"] {
            let sql = format!("SELECT * FROM contact WHERE {} = ?1 LIMIT 1", col);
            if let Ok(mut stmt) = conn.prepare(&sql) {
                let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
                if let Ok(mut rows) = stmt.query_map([username], |row| {
                    let mut obj = serde_json::Map::new();
                    for (i, c) in cols.iter().enumerate() {
                        let v = match row.get_ref(i) {
                            Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
                            Ok(rusqlite::types::ValueRef::Integer(n)) => serde_json::json!(n),
                            Ok(rusqlite::types::ValueRef::Real(f)) => serde_json::json!(f),
                            Ok(rusqlite::types::ValueRef::Text(t)) => {
                                serde_json::Value::String(String::from_utf8_lossy(t).to_string())
                            }
                            Ok(rusqlite::types::ValueRef::Blob(b)) => {
                                serde_json::Value::String(common::decode_blob_text(b))
                            }
                            Err(_) => serde_json::Value::Null,
                        };
                        obj.insert(c.clone(), v);
                    }
                    Ok(serde_json::Value::Object(obj))
                }) {
                    if let Some(Ok(obj)) = rows.next() {
                        found = Some(obj);
                        break;
                    }
                }
            }
        }
        found
    };
    contact_from_raw(&raw?)
}

fn contact_from_raw(raw: &serde_json::Value) -> Option<ContactEntry> {
    let get = |keys: &[&str]| -> String {
        keys.iter()
            .find_map(|k| raw.get(*k).and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string()
    };
    let username = get(&["userName", "username", "UserName"]);
    if username.is_empty() {
        return None;
    }
    Some(ContactEntry {
        username,
        remark: get(&["remark", "Remark"]),
        nick_name: get(&["nickName", "NickName", "nickname"]),
        display_name: get(&["displayName", "DisplayName"]),
        alias: get(&["alias", "Alias"]),
        local_type: raw.get("type").and_then(|v| v.as_i64()).unwrap_or(0),
        local_type_label: String::new(),
        category: String::new(),
        initial: String::new(),
        quan_pin: String::new(),
        avatar_url: get(&["avatarUrl", "AvatarUrl", "avatar_url"]),
        description: get(&["description", "Description", "signature", "Signature"]),
        member_count: None,
        owner: None,
        owner_name: None,
        label_ids: None,
        group_name: None,
        group_username: None,
    })
}
