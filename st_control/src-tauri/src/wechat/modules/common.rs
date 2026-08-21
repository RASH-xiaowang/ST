//! 微信数据管理 - 公共基础设施
//!
//! 提供所有功能模块共享的底层能力：
//! - 只读数据库连接（严格保护数据库现有数据）
//! - 文本解码（UTF-8 / GBK 回退）
//! - WCDB zstd 解压
//! - 消息表名哈希（MD5，与 PC 微信一致）
//! - 系统账号显示名（与 PC 微信客户端一致）
//! - PC 微信风格的时间格式化
//! - 轻量 XML 解析工具

use rusqlite::{Connection, Row};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ============ 文件签名（缓存失效判断）============

/// 文件签名（mtime + 长度）——跨调用缓存失效判断的基础类型
pub type DirSig = (SystemTime, u64);
/// 数据库签名对（主库 + WAL）
pub type DbSigPair = (Option<DirSig>, Option<DirSig>);
/// 目录内文件签名列表（文件/视频索引缓存值）
pub type DirFileSigList = Option<Vec<(String, DirSig)>>;
/// 媒体消息行（message_content / compress_content / create_time）——file.rs、video.rs 共用
pub struct MediaRow(pub Option<Vec<u8>>, pub Option<Vec<u8>>, pub i64);

/// 返回文件的 (修改时间, 长度) 签名；文件不存在时返回 None。
///
/// 用于跨调用缓存（联系人显示名 / 通讯录 / 消息分库元数据）的失效判断：
/// 微信监控或手动刷新会原子替换解密库文件，mtime/长度变化即视为数据已更新。
pub fn file_sig(p: &Path) -> Option<DirSig> {
    std::fs::metadata(p)
        .ok()
        .and_then(|m| m.modified().ok().map(|t| (t, m.len())))
}

/// 目录签名（mtime + 子项数；收敛 file.rs / voice/video.rs 重复实现）
pub fn dir_sig(dir: &Path) -> Option<DirSig> {
    let count = std::fs::read_dir(dir).ok().map(|rd| rd.count() as u64)?;
    std::fs::metadata(dir)
        .and_then(|m| m.modified())
        .ok()
        .map(|t| (t, count))
}

/// 月份目录名校验（YYYY-MM；收敛 file.rs / voice/video.rs 重复实现）
pub fn is_month_dir_name(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() == 7
        && b[4] == b'-'
        && name[..4].bytes().all(|c| c.is_ascii_digit())
        && name[5..7].bytes().all(|c| c.is_ascii_digit())
}

/// 数据库签名（主库 + WAL），任一变化即失效
pub fn db_sig(db: &Path) -> DbSigPair {
    (file_sig(db), file_sig(&db.with_extension("db-wal")))
}

// ============ 只读数据库连接（数据保护核心）============

/// 以**只读**方式打开解密后的 SQLite 数据库。
///
/// 保护措施：
/// 1. `SQLITE_OPEN_READ_ONLY` - 文件句柄只读，OS 层面禁止写入
/// 2. `PRAGMA query_only = ON` - 引擎层面拒绝任何写操作
/// 3. 不触碰原始加密数据库，也不写解密副本
pub fn open_readonly_db(path: &Path) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.execute_batch(
        "PRAGMA query_only = ON;
         PRAGMA temp_store = MEMORY;",
    )?;
    Ok(conn)
}

/// 判断表是否存在
pub fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |row| row.get::<_, i64>(0),
    )
    .map(|c| c > 0)
    .unwrap_or(false)
}

/// 获取表的全部列名
pub fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut cols = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&format!(
        "PRAGMA table_info(\"{}\")",
        table.replace('"', "")
    )) {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(1)) {
            for c in rows.flatten() {
                cols.push(c);
            }
        }
    }
    cols
}

/// 从 Row 中按索引读取原始字节，兼容 TEXT 与 BLOB 两类列
///
/// 为什么需要这个函数？
/// 微信数据库 schema 中 `summary`、`draft`、`message_content`、`content` 等字段
/// 虽然文档标注为 TEXT，但实际数据可能：
/// - 未压缩时存储为 TEXT（普通 UTF-8 字符串）
/// - 压缩时存储为 BLOB（含 zstd 二进制数据，可能含 NULL 字节）
///
/// 而 rusqlite 的 `Vec<u8>::column_result` 只能读 BLOB，读 TEXT 会返回错误，
/// 导致 `query_map().flatten()` 静默丢弃全部行（出现"数据明明存在却显示为空"的假象）。
///
/// 本函数先 `get_ref` 再分支匹配，TEXT 与 BLOB 都能正确返回底层字节。
pub fn get_bytes(row: &Row<'_>, idx: usize) -> Option<Vec<u8>> {
    use rusqlite::types::ValueRef;
    match row.get_ref(idx).ok()? {
        ValueRef::Text(t) => Some(t.to_vec()),
        ValueRef::Blob(b) => Some(b.to_vec()),
        _ => None,
    }
}

/// 通用表数据转储（列名 + 行数组），用于结构不确定的表
pub fn dump_table(
    conn: &Connection,
    table: &str,
    order_by: Option<&str>,
    limit: usize,
) -> Option<(Vec<String>, Vec<Vec<serde_json::Value>>)> {
    if !table_exists(conn, table) {
        return None;
    }
    let cols = table_columns(conn, table);
    if cols.is_empty() {
        return None;
    }
    let order = match order_by {
        Some(c) if cols.iter().any(|x| x == c) => format!(" ORDER BY \"{}\" DESC", c),
        _ => String::new(),
    };
    let sql = format!(
        "SELECT * FROM \"{}\"{} LIMIT {}",
        table.replace('"', ""),
        order,
        limit
    );
    let mut stmt = conn.prepare(&sql).ok()?;
    let col_count = cols.len();
    let rows = stmt
        .query_map([], |row| {
            let mut vals = Vec::with_capacity(col_count);
            for i in 0..col_count {
                vals.push(sql_value_to_json(row.get_ref(i)?));
            }
            Ok(vals)
        })
        .ok()?;
    let mut result = Vec::new();
    for r in rows.flatten() {
        result.push(r);
    }
    Some((cols, result))
}

/// SQLite 值 → JSON（BLOB 尝试按文本解码，二进制则标注长度）
pub fn sql_value_to_json(v: rusqlite::types::ValueRef) -> serde_json::Value {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(i) => serde_json::json!(i),
        ValueRef::Real(f) => serde_json::json!(f),
        ValueRef::Text(t) => serde_json::Value::String(String::from_utf8_lossy(t).to_string()),
        ValueRef::Blob(b) => {
            // 先尝试 zstd 解压再解码文本
            let decompressed = try_decompress(b);
            let buf = decompressed.as_deref().unwrap_or(b);
            if let Ok(s) = std::str::from_utf8(buf) {
                if buf.len() <= 4096 {
                    return serde_json::Value::String(s.to_string());
                }
                return serde_json::Value::String(format!(
                    "{}…(共{}字节)",
                    &s[..s.floor_char_boundary(4096)],
                    buf.len()
                ));
            }
            serde_json::Value::String(format!("[二进制 {} 字节]", b.len()))
        }
    }
}

// ============ 文本解码 ============

/// 智能解码微信文本：优先 UTF-8，失败回退 GBK
pub fn decode_wechat_text(data: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(data) {
        return s.to_string();
    }
    encoding_rs::GBK.decode(data).0.to_string()
}

/// 尝试 zstd 解压（WCDB 压缩字段，magic = 0x28B52FFD）
pub fn try_decompress(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() >= 4 && data[0] == 0x28 && data[1] == 0xB5 && data[2] == 0x2F && data[3] == 0xFD {
        zstd::stream::decode_all(data).ok()
    } else {
        None
    }
}

/// 解码微信 BLOB 字段（自动处理 zstd 压缩 + UTF-8/GBK）
pub fn decode_blob_text(data: &[u8]) -> String {
    if let Some(dec) = try_decompress(data) {
        if let Ok(s) = String::from_utf8(dec) {
            return s;
        }
    }
    decode_wechat_text(data)
}

// ============ 消息表名哈希 ============

/// 与 PC 微信一致的消息表名：`Msg_` + MD5(username)
pub fn msg_table_name(username: &str) -> String {
    use md5::{Digest, Md5};
    let digest = Md5::digest(username.as_bytes());
    format!("Msg_{:x}", digest)
}

// ============ 系统账号显示名（与 PC 微信客户端一致）============

/// 微信系统/特殊账号 → PC 客户端显示名
pub fn system_account_name(username: &str) -> Option<&'static str> {
    match username {
        "filehelper" => Some("文件传输助手"),
        "fmessage" => Some("朋友推荐消息"),
        "qmessage" => Some("QQ离线消息"),
        "qqmail" => Some("QQ邮箱提醒"),
        "medianote" => Some("语音记事本"),
        "newsapp" => Some("腾讯新闻"),
        "weixin" | "wechat" => Some("微信团队"),
        "notification_messages" => Some("服务通知"),
        "notifymessage" => Some("微信支付"),
        "payutil" => Some("微信支付"),
        "servicemsg" => Some("服务通知"),
        "masssendapp" => Some("群发助手"),
        "voiceinput" => Some("语音输入"),
        "feedsapp" => Some("朋友圈"),
        "floatbottle" => Some("漂流瓶"),
        "shakeapp" => Some("摇一摇"),
        "lbsapp" => Some("附近的人"),
        "officialaccounts" => Some("公众号"),
        "helper_entry" => Some("辅助功能"),
        "blogapp" => Some("微博阅读"),
        "linkedinplugin" => Some("LinkedIn"),
        "facebookapp" => Some("Facebook"),
        "qqsync" => Some("通讯录同步助手"),
        "weibo" => Some("微博"),
        "downloaderapp" => Some("下载助手"),
        "meibiapp" => Some("美币"),
        "openimopenapi" => Some("开放平台"),
        "wxid_w6dpcannoq1f22" => Some("微信运动"),
        _ => None,
    }
}

/// 是否公众号（PC 中公众号会话以 gh_ 开头）
pub fn is_official_account(username: &str) -> bool {
    username.starts_with("gh_")
}

/// 是否系统通知类账号（不在 PC 通讯录“联系人”中展示）
pub fn is_builtin_account(username: &str) -> bool {
    system_account_name(username).is_some()
}

// ============ PC 微信风格时间格式化 ============

fn local_datetime(ts: i64) -> Option<chrono::DateTime<chrono::Local>> {
    chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.with_timezone(&chrono::Local))
}

/// 会话列表时间（与 PC 微信一致）：
/// - 今天     → `HH:MM`
/// - 昨天     → `昨天`
/// - 一周内   → `星期X`
/// - 今年内   → `M月D日`
/// - 更早     → `YYYY年M月D日`
pub fn format_session_time(ts: i64) -> String {
    use chrono::{Datelike, Local};
    let dt = match local_datetime(ts) {
        Some(d) => d,
        None => return String::new(),
    };
    let now = Local::now();
    let today = now.date_naive();
    let date = dt.date_naive();
    let days = (today - date).num_days();

    if days <= 0 {
        dt.format("%H:%M").to_string()
    } else if days == 1 {
        "昨天".to_string()
    } else if days < 7 {
        let week = [
            "星期日",
            "星期一",
            "星期二",
            "星期三",
            "星期四",
            "星期五",
            "星期六",
        ];
        week[date.weekday().num_days_from_sunday() as usize].to_string()
    } else if date.year() == today.year() {
        format!("{}月{}日", date.month(), date.day())
    } else {
        format!("{}年{}月{}日", date.year(), date.month(), date.day())
    }
}

/// 聊天消息时间分隔条（与 PC 微信一致）：
/// - 今天   → `HH:MM`
/// - 昨天   → `昨天 HH:MM`
/// - 一周内 → `星期X HH:MM`
/// - 更早   → `YYYY年M月D日 HH:MM`
pub fn format_msg_divider_time(ts: i64) -> String {
    use chrono::{Datelike, Local};
    let dt = match local_datetime(ts) {
        Some(d) => d,
        None => return String::new(),
    };
    let now = Local::now();
    let today = now.date_naive();
    let date = dt.date_naive();
    let days = (today - date).num_days();
    let hm = dt.format("%H:%M").to_string();

    if days <= 0 {
        hm
    } else if days == 1 {
        format!("昨天 {}", hm)
    } else if days < 7 {
        let week = [
            "星期日",
            "星期一",
            "星期二",
            "星期三",
            "星期四",
            "星期五",
            "星期六",
        ];
        format!(
            "{} {}",
            week[date.weekday().num_days_from_sunday() as usize],
            hm
        )
    } else if date.year() == today.year() {
        format!("{}月{}日 {}", date.month(), date.day(), hm)
    } else {
        format!("{}年{}月{}日 {}", date.year(), date.month(), date.day(), hm)
    }
}

/// 完整时间 `YYYY-MM-DD HH:MM:SS`
pub fn format_full_time(ts: i64) -> String {
    local_datetime(ts)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

/// 通用时间 `YYYY-MM-DD HH:MM`
pub fn format_date_time(ts: i64) -> String {
    local_datetime(ts)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

/// 文件大小人性化显示（与 PC 微信一致）
pub fn format_file_size(size: i64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", size as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.2} GB", size as f64 / 1024.0 / 1024.0 / 1024.0)
    }
}

// ============ 消息类型（与 PC 微信一致）============

/// 规范化消息类型：4.0 库中部分 local_type 带高位标志
pub fn normalize_msg_type(local_type: i64) -> i64 {
    if local_type > (1i64 << 32) {
        local_type % (1i64 << 32)
    } else {
        local_type
    }
}

/// 消息类型的会话列表摘要占位符（与 PC 微信一致）
pub fn msg_type_placeholder(local_type: i64) -> &'static str {
    match normalize_msg_type(local_type) {
        1 => "文本",
        3 => "图片",
        34 => "语音",
        42 => "名片",
        43 => "视频",
        47 => "表情",
        48 => "位置",
        49 => "链接",
        50 => "语音通话",
        244 | 246 => "文件",
        10000 => "系统消息",
        10002 => "撤回消息",
        859832288 | 922746960 => "拍一拍",
        244135593199 => "小程序",
        _ => "未知消息",
    }
}

// ============ 轻量 XML 工具 ============

/// 提取 XML 标签文本 `<tag>...</tag>`
pub fn xml_tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let s = xml.find(&open)?;
    let content_start = s + open.len();
    let e = xml[content_start..].find(&close)?;
    Some(xml[content_start..content_start + e].to_string())
}

/// 提取 XML 标签属性 `<tag attr="value" ...>`
pub fn xml_tag_attr(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let tag_start = xml.find(&format!("<{} ", tag))?;
    let tag_end = xml[tag_start..].find('>')?;
    let tag_str = &xml[tag_start..tag_start + tag_end];
    let search = format!("{}=\"", attr);
    let attr_start = tag_str.find(&search)?;
    let value_start = attr_start + search.len();
    let value_end = tag_str[value_start..].find('"')?;
    Some(tag_str[value_start..value_start + value_end].to_string())
}

/// 提取嵌套标签文本 `<outer><inner>text</inner></outer>`
pub fn xml_nested_text(xml: &str, outer: &str, inner: &str) -> Option<String> {
    let outer_start = xml.find(&format!("<{}", outer))?;
    let outer_close = format!("</{}>", outer);
    let outer_end = xml[outer_start..].find(&outer_close)?;
    let outer_str = &xml[outer_start..outer_start + outer_end];
    xml_tag_text(outer_str, inner)
}

/// 去除 XML 标签得到纯文本
pub fn strip_xml_tags(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut in_tag = false;
    for ch in xml.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

// ============ 路径工具 ============

/// 在解密目录中查找匹配前缀的数据库文件（如 message_*.db）
pub fn find_db_files(decrypted_dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut results = Vec::new();
    collect_db_files(decrypted_dir, prefix, &mut results, 0);
    results.sort();
    results
}

/// 严格判断文件名是否为消息分片库：`message_<数字>.db` 或 `biz_message_<数字>.db`
///
/// 用前缀扫描时会把 message_fts.db（全文索引）、message_resource.db（资源索引）
/// 等辅助库误纳入分库探测——这些库永远不含 Msg_ 表，探测只会产生噪音日志
/// 并浪费大库打开/解密开销。真正的消息分片库文件名一定是 前缀+纯数字+.db。
pub fn is_message_shard_file(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return false,
    };
    for prefix in ["message_", "biz_message_"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            if let Some(idx) = rest.strip_suffix(".db") {
                if !idx.is_empty() && idx.chars().all(|c| c.is_ascii_digit()) {
                    return true;
                }
            }
        }
    }
    false
}

fn collect_db_files(dir: &Path, prefix: &str, results: &mut Vec<PathBuf>, depth: u32) {
    if depth > 3 {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_db_files(&path, prefix, results, depth + 1);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with(prefix) && name.ends_with(".db") {
                    results.push(path);
                }
            }
        }
    }
}

/// 当前 Unix 毫秒时间戳（自 daily_summary / edit_store 收敛的共享实现）
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 消息表时间列归一化 SQL 表达式：毫秒 → 秒（自 annual / daily_summary 收敛）
pub fn ts_expr() -> &'static str {
    "CASE WHEN create_time > 1000000000000 THEN CAST(create_time/1000 AS INTEGER) ELSE create_time END"
}

/// 时间戳 → YYYY-MM 月份目录名（自 file / voice::video 收敛）
pub fn month_of(ts: i64) -> String {
    if ts <= 0 {
        return String::new();
    }
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msg_table_name() {
        // 与标准 MD5 对照（MD5("abc")、MD5("")）
        assert_eq!(
            msg_table_name("abc"),
            "Msg_900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(msg_table_name(""), "Msg_d41d8cd98f00b204e9800998ecf8427e");
        assert!(msg_table_name("wxid_abc@chatroom").starts_with("Msg_"));
    }

    #[test]
    fn test_system_account_name() {
        assert_eq!(system_account_name("filehelper"), Some("文件传输助手"));
        assert_eq!(system_account_name("weixin"), Some("微信团队"));
        assert_eq!(system_account_name("wxid_abc"), None);
    }

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(2048), "2.0 KB");
        assert_eq!(format_file_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn test_normalize_msg_type() {
        assert_eq!(normalize_msg_type(1), 1);
        assert_eq!(normalize_msg_type(4294967297), 1); // 高位标志清除
    }

    #[test]
    fn test_strip_xml_tags() {
        assert_eq!(strip_xml_tags("<a>你好<b>世界</b></a>"), "你好世界");
    }

    #[test]
    fn test_xml_helpers() {
        let xml = r#"<msg><appmsg type="5"><title>文章</title></appmsg></msg>"#;
        assert_eq!(xml_tag_text(xml, "title"), Some("文章".to_string()));
        assert_eq!(xml_tag_attr(xml, "appmsg", "type"), Some("5".to_string()));
        assert_eq!(
            xml_nested_text(xml, "appmsg", "title"),
            Some("文章".to_string())
        );
    }

    #[test]
    fn test_decode_blob_text_plain() {
        assert_eq!(decode_blob_text("你好".as_bytes()), "你好");
    }
}
