// ============================================================
// 知识库登录态（轻量级用户会话）
// 仅依赖 users / roles / user_roles 表，会话持久化到 kb_session.json。
// 设计目标：替换写死的 CURRENT_USER 占位，提供"当前用户"真实身份，
// 并支撑 ACL 与可见性判定。非完整账号系统，默认 seed admin / 测试员。
// ============================================================

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// bcrypt 默认成本因子
const BCRYPT_COST: u32 = 10;

/// 对明文密码做 bcrypt 哈希
pub fn hash_password(pw: &str) -> Result<String, String> {
    bcrypt::hash(pw, BCRYPT_COST).map_err(|e| format!("密码哈希失败: {}", e))
}

/// 校验明文密码与哈希是否匹配（空哈希视为"未设置密码"，直接放行）
pub fn verify_password(pw: &str, hash: &str) -> bool {
    if hash.is_empty() {
        return true; // 兼容旧库：无密码时仅用户名登录
    }
    bcrypt::verify(pw, hash).unwrap_or(false)
}

#[derive(Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
    pub displayName: Option<String>,
    pub isAdmin: bool,
}

#[derive(Default)]
pub struct UserSession {
    inner: Mutex<Option<CurrentUser>>,
}

impl UserSession {
    /// 从磁盘恢复上次登录态
    pub fn load() -> Self {
        let s = UserSession {
            inner: Mutex::new(None),
        };
        if let Ok(text) = std::fs::read_to_string(Self::session_path()) {
            if let Ok(u) = serde_json::from_str::<CurrentUser>(&text) {
                *s.inner.lock().unwrap_or_else(|e| e.into_inner()) = Some(u);
            }
        }
        s
    }

    fn session_path() -> PathBuf {
        crate::common::st_data_dir().join("kb_session.json")
    }

    pub fn get(&self) -> Option<CurrentUser> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 设置当前用户并持久化
    pub fn set(&self, u: CurrentUser) {
        *self.inner.lock().unwrap_or_else(|e| e.into_inner()) = Some(u.clone());
        let _ = std::fs::write(
            Self::session_path(),
            serde_json::to_string(&u).unwrap_or_default(),
        );
    }

    pub fn clear(&self) {
        *self.inner.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let _ = std::fs::remove_file(Self::session_path());
    }
}

#[tauri::command]
pub async fn kb_login(
    db: tauri::State<'_, crate::kb::db::KbDatabase>,
    session: tauri::State<'_, UserSession>,
    username: Option<String>,
    password: Option<String>,
) -> Result<CurrentUser, String> {
    let name = username
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "admin".to_string());

    // seed 默认用户
    crate::kb::auth::ensure_seed_users(&db);

    let conn = db.conn_lock();
    // 大小写不敏感匹配，并取出密码哈希
    let row: Option<(i64, String, Option<String>, String)> = conn
        .query_row(
            "SELECT id, username, display_name, password_hash FROM users WHERE lower(username) = lower(?1)",
            rusqlite::params![name],
            |r| Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
            )),
        )
        .ok();

    let (id, uname, display, pw_hash) = match row {
        Some(r) => r,
        None => return Err(format!("用户 '{}' 不存在，可用：admin / 测试员", name)),
    };

    // 密码校验（空哈希表示未设置密码，仅用户名登录）
    let provided = password.unwrap_or_default();
    if !verify_password(&provided, &pw_hash) {
        return Err("密码错误".to_string());
    }

    // 是否为 admin（拥有 admin 角色，或 is_admin 标记）
    let is_admin: bool = conn
        .query_row(
            "SELECT 1 FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = ?1 AND r.name = 'admin' LIMIT 1",
            rusqlite::params![id],
            |_| Ok(true),
        )
        .unwrap_or(false)
        || conn
            .query_row("SELECT is_admin FROM users WHERE id = ?1", rusqlite::params![id], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
            == 1;

    let user = CurrentUser {
        id,
        username: uname,
        displayName: display,
        isAdmin: is_admin,
    };
    session.set(user.clone());
    Ok(user)
}

#[tauri::command]
pub async fn kb_logout(session: tauri::State<'_, UserSession>) -> Result<(), String> {
    session.clear();
    Ok(())
}

#[tauri::command]
pub async fn kb_current_user(
    session: tauri::State<'_, UserSession>,
) -> Result<Option<CurrentUser>, String> {
    Ok(session.get())
}

/// 私有化部署（单机、无权限控制）：返回默认管理员身份，
/// 供启动时自动登录。优先找 username='admin' 或 id=1 的用户。
pub fn default_admin(db: &crate::kb::db::KbDatabase) -> Option<CurrentUser> {
    let conn = db.conn_lock();
    let row: Option<(i64, String, Option<String>)> = conn
        .query_row(
            "SELECT id, username, display_name FROM users WHERE lower(username) = 'admin' OR id = 1 ORDER BY id LIMIT 1",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)),
        )
        .ok();
    let (id, username, display_name) = row?;
    let is_admin: bool = conn
        .query_row(
            "SELECT 1 FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = ?1 AND r.name = 'admin' LIMIT 1",
            rusqlite::params![id],
            |_| Ok(true),
        )
        .unwrap_or(false)
        || conn
            .query_row("SELECT is_admin FROM users WHERE id = ?1", rusqlite::params![id], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
            == 1;
    Some(CurrentUser {
        id,
        username,
        displayName: display_name,
        isAdmin: is_admin,
    })
}

/// 首次启动时确保默认用户与角色存在
pub fn ensure_seed_users(db: &crate::kb::db::KbDatabase) {
    let conn = db.conn_lock();

    // 兼容旧库：若缺少新字段则补充（已建库但处于旧 schema 时）
    migrate_schema(&conn);

    let admin_exists: bool = conn
        .query_row(
            "SELECT 1 FROM users WHERE username='admin' LIMIT 1",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if admin_exists {
        // 已有用户，但可能仍是空密码哈希：给 admin/测试员补默认密码，便于演示登录
        ensure_seed_password(&conn, "admin", "admin123");
        ensure_seed_password(&conn, "测试员", "test123");
        return;
    }
    let _ = conn.execute("INSERT OR IGNORE INTO roles (name, description) VALUES ('admin','管理员'),('editor','编辑'),('viewer','只读')", []);
    let admin_hash = hash_password("admin123").unwrap_or_default();
    let tester_hash = hash_password("test123").unwrap_or_default();
    let _ = conn.execute(
        "INSERT OR IGNORE INTO users (username, display_name, password_hash) VALUES ('admin','管理员',?1),('测试员','测试用户',?2)",
        rusqlite::params![admin_hash, tester_hash],
    );
    // admin -> admin 角色
    let admin_id: i64 = conn
        .query_row("SELECT id FROM users WHERE username='admin'", [], |r| {
            r.get(0)
        })
        .unwrap_or(1);
    let role_admin: i64 = conn
        .query_row("SELECT id FROM roles WHERE name='admin'", [], |r| r.get(0))
        .unwrap_or(1);
    let _ = conn.execute(
        "INSERT OR IGNORE INTO user_roles (user_id, role_id) VALUES (?1,?2)",
        rusqlite::params![admin_id, role_admin],
    );
}

/// 兼容旧库：为 users 表补加 password_hash / is_admin 列
fn migrate_schema(conn: &rusqlite::Connection) {
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(users)")
        .and_then(|mut st| {
            let rows = st.query_map([], |r| r.get::<_, String>(1))?;
            let mut v = Vec::new();
            for c in rows.flatten() {
                v.push(c);
            }
            Ok(v)
        })
        .unwrap_or_default();
    if !cols.iter().any(|c| c == "password_hash") {
        let _ = conn.execute(
            "ALTER TABLE users ADD COLUMN password_hash TEXT NOT NULL DEFAULT ''",
            [],
        );
    }
    if !cols.iter().any(|c| c == "is_admin") {
        let _ = conn.execute(
            "ALTER TABLE users ADD COLUMN is_admin INTEGER NOT NULL DEFAULT 0",
            [],
        );
    }
}

/// 仅当用户当前密码哈希为空时，补设一个默认密码（便于首次体验）
fn ensure_seed_password(conn: &rusqlite::Connection, username: &str, default_pw: &str) {
    let hash: String = conn
        .query_row(
            "SELECT password_hash FROM users WHERE username = ?1",
            rusqlite::params![username],
            |r| r.get(0),
        )
        .unwrap_or_default();
    if hash.is_empty() {
        let new_hash = hash_password(default_pw).unwrap_or_default();
        let _ = conn.execute(
            "UPDATE users SET password_hash = ?1 WHERE username = ?2",
            rusqlite::params![new_hash, username],
        );
    }
}
