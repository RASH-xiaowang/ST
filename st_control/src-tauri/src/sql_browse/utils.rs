// ============================================================
// SQLite 表浏览 — 工具函数
// 自 sql_browse.rs 拆分：标识符转义、LIKE 通配符转义、友好错误。
// ============================================================

/// 标准 SQLite 标识符转义：双引号包裹，内部双引号双写
pub fn safe_name(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// 转义 LIKE 通配符，配合 ESCAPE '\' 使用
pub fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 把底层 SQLite 错误转成更友好、可操作的中文提示
pub fn friendly_db_error(e: &rusqlite::Error) -> String {
    if let rusqlite::Error::SqliteFailure(ffi_err, msg) = e {
        let code = ffi_err.extended_code & 0xff;
        let detail = msg.clone().unwrap_or_default();
        if code == 11 || code == 26 {
            // SQLITE_CORRUPT / SQLITE_NOTADB
            return format!("数据库文件已损坏或正在被写入（可能由微信解密进程重写导致），暂时无法读取。可稍后重试，或检查/重新生成该数据库文件。{}", detail);
        }
        if code == 5 || code == 6 {
            // SQLITE_BUSY / SQLITE_LOCKED
            return format!("数据库被其它进程占用，请稍后重试。{}", detail);
        }
        if code == 14 {
            // SQLITE_CANTOPEN
            return format!("无法打开数据库文件（可能不存在或无权限）。{}", detail);
        }
    }
    e.to_string()
}
