// ============================================================
// 知识库管理 — 知识库 / 权限 / ACL
// 自 handlers.rs 拆分：知识库 CRUD、ACL、用户/成员/角色管理。
// ============================================================

use crate::kb::db::KbDatabase;
use rusqlite::params_from_iter;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::cleanup_orphan_file_objects;

// ─── 权限 ACL ───

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AclInput {
    pub scope: String, // document / folder / kb
    pub doc_id: Option<i64>,
    pub dir_id: Option<i64>,
    pub kb_id: Option<i64>,
    pub grantee_type: String, // user / role / public
    pub user_id: Option<i64>,
    pub role_id: Option<i64>,
    pub effect: String, // allow / deny
}

#[tauri::command]
pub async fn kb_set_acl(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    input: AclInput,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let kb_id = input.kb_id.ok_or("缺少 kb_id")?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可设置 ACL".to_string());
    }
    let conn = db.conn_lock();
    // 唯一约束限制：先删除同键旧规则再插入（支持切换 allow/deny 与撤销）
    conn.execute(
        "DELETE FROM kb_acl WHERE scope=?1
            AND COALESCE(doc_id,0)=COALESCE(?2,0)
            AND COALESCE(dir_id,0)=COALESCE(?3,0)
            AND COALESCE(kb_id,0)=COALESCE(?4,0)
            AND grantee_type=?5
            AND COALESCE(user_id,0)=COALESCE(?6,0)
            AND COALESCE(role_id,0)=COALESCE(?7,0)",
        rusqlite::params![
            input.scope,
            input.doc_id,
            input.dir_id,
            input.kb_id,
            input.grantee_type,
            input.user_id,
            input.role_id
        ],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO kb_acl (scope, doc_id, dir_id, kb_id, grantee_type, user_id, role_id, effect, created_by)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        rusqlite::params![input.scope, input.doc_id, input.dir_id, input.kb_id, input.grantee_type, input.user_id, input.role_id, input.effect, uid],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

// ─── 权限 ACL 查询 ───

#[tauri::command]
pub async fn kb_get_acl(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
    scope: Option<String>,
    doc_id: Option<i64>,
    dir_id: Option<i64>,
) -> Result<Vec<serde_json::Value>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可查看 ACL".to_string());
    }
    let conn = db.conn_lock();
    let mut sql = String::from(
        "SELECT id, scope, doc_id, dir_id, kb_id, grantee_type, user_id, role_id, effect, created_by, created_at
         FROM kb_acl WHERE kb_id = ?1"
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(kb_id)];
    if let Some(s) = &scope {
        sql.push_str(" AND scope = ?2");
        params.push(Box::new(s.clone()));
    }
    if let Some(d) = doc_id {
        sql.push_str(" AND doc_id = ?3");
        params.push(Box::new(d));
    }
    if let Some(d) = dir_id {
        sql.push_str(" AND dir_id = ?4");
        params.push(Box::new(d));
    }
    sql.push_str(" ORDER BY id DESC");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "scope": row.get::<_, String>(1)?,
                "docId": row.get::<_, Option<i64>>(2)?,
                "dirId": row.get::<_, Option<i64>>(3)?,
                "kbId": row.get::<_, Option<i64>>(4)?,
                "granteeType": row.get::<_, String>(5)?,
                "userId": row.get::<_, Option<i64>>(6)?,
                "roleId": row.get::<_, Option<i64>>(7)?,
                "effect": row.get::<_, String>(8)?,
                "createdBy": row.get::<_, i64>(9)?,
                "createdAt": row.get::<_, String>(10)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

// ════════════════════════════════════════════════════════════
// 用户管理（admin）与知识库成员管理
// ════════════════════════════════════════════════════════════

/// 判断当前用户是否全局管理员（拥有 admin 角色或 is_admin 标记）
fn is_global_admin(db: &KbDatabase, uid: i64) -> bool {
    let conn = db.conn_lock();
    conn.query_row(
        "SELECT 1 FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = ?1 AND r.name = 'admin' LIMIT 1",
        rusqlite::params![uid], |_| Ok(true),
    )
    .unwrap_or(false)
    || conn.query_row("SELECT is_admin FROM users WHERE id = ?1", rusqlite::params![uid], |r| r.get::<_, i64>(0)).unwrap_or(0) == 1
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct UserItem {
    pub id: i64,
    pub username: String,
    pub displayName: Option<String>,
    pub isAdmin: bool,
}

/// 列出所有用户（仅全局管理员）
#[tauri::command]
pub async fn kb_list_users(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
) -> Result<Vec<UserItem>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !is_global_admin(&db, uid) {
        return Err("无权限：仅全局管理员可查看用户列表".to_string());
    }
    let conn = db.conn_lock();
    let mut stmt = conn
        .prepare("SELECT id, username, display_name, is_admin FROM users ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(UserItem {
                id: row.get(0)?,
                username: row.get(1)?,
                displayName: row.get(2)?,
                isAdmin: row.get::<_, i64>(3)? == 1,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 创建用户（仅全局管理员）
#[tauri::command]
pub async fn kb_create_user(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    username: String,
    display_name: Option<String>,
    password: String,
) -> Result<i64, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !is_global_admin(&db, uid) {
        return Err("无权限：仅全局管理员可创建用户".to_string());
    }
    let uname = username.trim().to_string();
    if uname.is_empty() {
        return Err("用户名不能为空".to_string());
    }
    let hash = crate::kb::auth::hash_password(&password)?;
    let conn = db.conn_lock();
    conn.execute(
        "INSERT INTO users (username, display_name, password_hash) VALUES (?1,?2,?3)",
        rusqlite::params![uname, display_name, hash],
    )
    .map_err(|e| format!("创建失败（用户名可能已存在）: {}", e))?;
    Ok(conn.last_insert_rowid())
}

/// 修改自己的密码
#[tauri::command]
pub async fn kb_change_password(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    old_password: String,
    new_password: String,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let conn = db.conn_lock();
    let cur_hash: String = conn
        .query_row(
            "SELECT password_hash FROM users WHERE id = ?1",
            rusqlite::params![uid],
            |r| r.get(0),
        )
        .map_err(|_| "用户不存在".to_string())?;
    if !crate::kb::auth::verify_password(&old_password, &cur_hash) {
        return Err("原密码错误".to_string());
    }
    let new_hash = crate::kb::auth::hash_password(&new_password)?;
    conn.execute(
        "UPDATE users SET password_hash = ?1 WHERE id = ?2",
        rusqlite::params![new_hash, uid],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 删除用户（仅全局管理员；删除前解除其知识库/文档归属引用，其余引用由外键级联清理）
#[tauri::command]
pub async fn kb_delete_user(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    user_id: i64,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !is_global_admin(&db, uid) {
        return Err("无权限：仅全局管理员可删除用户".to_string());
    }
    if user_id == uid {
        return Err("不能删除当前登录账号".to_string());
    }
    let conn = db.conn_lock();
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !exists {
        return Err("用户不存在".to_string());
    }
    // 解除无级联的外键引用（其余表如 kb_members / user_roles / qa_sessions 等由 ON DELETE CASCADE 自动清理）
    conn.execute(
        "UPDATE knowledge_bases SET owner_id = NULL WHERE owner_id = ?1",
        rusqlite::params![user_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE documents SET created_by = NULL WHERE created_by = ?1",
        rusqlite::params![user_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE document_versions SET created_by = NULL WHERE created_by = ?1",
        rusqlite::params![user_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE kb_acl SET created_by = NULL WHERE created_by = ?1",
        rusqlite::params![user_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE search_logs SET user_id = NULL WHERE user_id = ?1",
        rusqlite::params![user_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM users WHERE id = ?1",
        rusqlite::params![user_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 重置任意用户密码（仅全局管理员）
#[tauri::command]
pub async fn kb_reset_password(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    user_id: i64,
    new_password: String,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !is_global_admin(&db, uid) {
        return Err("无权限：仅全局管理员可重置密码".to_string());
    }
    if new_password.is_empty() {
        return Err("新密码不能为空".to_string());
    }
    let hash = crate::kb::auth::hash_password(&new_password)?;
    let conn = db.conn_lock();
    let affected = conn
        .execute(
            "UPDATE users SET password_hash = ?1 WHERE id = ?2",
            rusqlite::params![hash, user_id],
        )
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("用户不存在".to_string());
    }
    Ok(())
}

/// 设置/取消全局管理员（仅全局管理员；不允许取消自己的管理员权限）
#[tauri::command]
pub async fn kb_set_admin(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    user_id: i64,
    is_admin: bool,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !is_global_admin(&db, uid) {
        return Err("无权限：仅全局管理员可调整管理员".to_string());
    }
    if user_id == uid && !is_admin {
        return Err("不能取消自己的管理员权限".to_string());
    }
    let flag = if is_admin { 1 } else { 0 };
    let conn = db.conn_lock();
    let affected = conn
        .execute(
            "UPDATE users SET is_admin = ?1 WHERE id = ?2",
            rusqlite::params![flag, user_id],
        )
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("用户不存在".to_string());
    }
    // 同步 user_roles 中的 admin 角色（roles 表中存在该角色时生效）
    if is_admin {
        let _ = conn.execute(
            "INSERT OR IGNORE INTO user_roles (user_id, role_id) SELECT ?1, id FROM roles WHERE name = 'admin'",
            rusqlite::params![user_id],
        );
    } else {
        let _ = conn.execute(
            "DELETE FROM user_roles WHERE user_id = ?1 AND role_id IN (SELECT id FROM roles WHERE name = 'admin')",
            rusqlite::params![user_id],
        );
    }
    Ok(())
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct RoleItem {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
}

/// 列出所有角色（用于成员管理/ACL 授权时选择）
#[tauri::command]
pub async fn kb_list_roles(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
) -> Result<Vec<RoleItem>, String> {
    session.get().ok_or("请先登录知识库")?;
    let conn = db.conn_lock();
    let mut stmt = conn
        .prepare("SELECT id, name, description FROM roles ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(RoleItem {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct MemberItem {
    pub userId: i64,
    pub username: String,
    pub displayName: Option<String>,
    pub role: String,
}

/// 列出知识库成员
#[tauri::command]
pub async fn kb_list_members(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
) -> Result<Vec<MemberItem>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_access_kb(&db, kb_id, uid) {
        return Err("无权限：你无权访问该知识库".to_string());
    }
    let conn = db.conn_lock();
    let mut stmt = conn
        .prepare(
            "SELECT u.id, u.username, u.display_name, m.role
         FROM kb_members m JOIN users u ON u.id = m.user_id
         WHERE m.kb_id = ?1 ORDER BY m.role = 'owner' DESC, u.username",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![kb_id], |row| {
            Ok(MemberItem {
                userId: row.get(0)?,
                username: row.get(1)?,
                displayName: row.get(2)?,
                role: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 添加知识库成员（owner/admin）
#[tauri::command]
pub async fn kb_add_member(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
    user_id: i64,
    role: String,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可管理成员".to_string());
    }
    let role = match role.as_str() {
        "owner" | "admin" | "editor" | "viewer" => role,
        _ => "viewer".to_string(),
    };
    let conn = db.conn_lock();
    conn.execute(
        "INSERT INTO kb_members (kb_id, user_id, role) VALUES (?1,?2,?3)
         ON CONFLICT(kb_id, user_id) DO UPDATE SET role = excluded.role",
        rusqlite::params![kb_id, user_id, role],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 移除知识库成员（owner/admin；不允许移除 owner）
#[tauri::command]
pub async fn kb_remove_member(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
    user_id: i64,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可管理成员".to_string());
    }
    let conn = db.conn_lock();
    // 保护：不允许移除 owner
    let role: Option<String> = conn
        .query_row(
            "SELECT role FROM kb_members WHERE kb_id = ?1 AND user_id = ?2",
            rusqlite::params![kb_id, user_id],
            |r| r.get(0),
        )
        .ok();
    if role.as_deref() == Some("owner") {
        return Err("不能移除知识库 owner".to_string());
    }
    conn.execute(
        "DELETE FROM kb_members WHERE kb_id = ?1 AND user_id = ?2",
        rusqlite::params![kb_id, user_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 修改成员角色（owner/admin；不允许修改 owner）
#[tauri::command]
pub async fn kb_update_member_role(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
    user_id: i64,
    role: String,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可管理成员".to_string());
    }
    let role = match role.as_str() {
        "owner" | "admin" | "editor" | "viewer" => role,
        _ => "viewer".to_string(),
    };
    let conn = db.conn_lock();
    let cur: Option<String> = conn
        .query_row(
            "SELECT role FROM kb_members WHERE kb_id = ?1 AND user_id = ?2",
            rusqlite::params![kb_id, user_id],
            |r| r.get(0),
        )
        .ok();
    if cur.as_deref() == Some("owner") && role != "owner" {
        return Err("不能降级知识库 owner".to_string());
    }
    conn.execute(
        "UPDATE kb_members SET role = ?3 WHERE kb_id = ?1 AND user_id = ?2",
        rusqlite::params![kb_id, user_id, role],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ─── 知识库 CRUD ───

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct KbSummary {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Option<i64>,
    pub pinned: bool,
    pub isSystem: bool,
    pub docCount: i64,
    pub created_at: String,
}

#[tauri::command]
pub async fn kb_create(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    name: String,
    description: Option<String>,
) -> Result<KbSummary, String> {
    let uid = session
        .get()
        .map(|u| u.id)
        .ok_or("请先登录知识库（点击右上角登录）")?;
    let conn = db.conn_lock();
    conn.execute(
        "INSERT INTO knowledge_bases (name, description, owner_id) VALUES (?1,?2,?3)",
        rusqlite::params![name, description, uid],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    // 创建者自动成为 owner 成员
    conn.execute(
        "INSERT OR IGNORE INTO kb_members (kb_id, user_id, role) VALUES (?1,?2,'owner')",
        rusqlite::params![id, uid],
    )
    .map_err(|e| e.to_string())?;
    Ok(KbSummary {
        id,
        name,
        description,
        owner_id: Some(uid),
        pinned: false,
        isSystem: false,
        docCount: 0,
        created_at: String::new(),
    })
}

/// 首次启动确保存在「系统知识库」（知识收集的核心载体，不可删除/重命名）
pub fn ensure_system_kb(db: &KbDatabase) {
    let conn = db.conn_lock();
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_bases WHERE is_system = 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if exists == 0 {
        let _ = conn.execute(
            "INSERT INTO knowledge_bases (name, description, is_system, owner_id)
             VALUES ('系统知识库', '知识收集的核心载体，用于统一归档、解析与检索', 1, 1)",
            [],
        );
    }
}

#[tauri::command]
pub async fn kb_list(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    user_id: i64,
) -> Result<Vec<KbSummary>, String> {
    // 以登录态为准（前端传入的 user_id 仅作兜底）
    let uid = session.get().map(|u| u.id).unwrap_or(user_id);
    // 与检索侧一致：用 visible_kb_ids（成员 ∪ ACL allow − deny）过滤
    let visible = crate::kb::retrieval::visible_kb_ids(&db, uid);
    if visible.is_empty() {
        return Ok(Vec::new());
    }
    let conn = db.conn_lock();
    let placeholders = visible.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT k.id, k.name, k.description, k.owner_id, k.created_at, k.pinned, k.is_system,
                (SELECT COUNT(*) FROM documents d WHERE d.kb_id = k.id) AS doc_count
         FROM knowledge_bases k
         WHERE k.id IN ({})
         ORDER BY k.pinned DESC, k.is_system DESC, k.updated_at DESC, k.id DESC",
        placeholders
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let binds: Vec<&dyn rusqlite::types::ToSql> = visible
        .iter()
        .map(|v| v as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = stmt
        .query_map(binds.as_slice(), |row| {
            Ok(KbSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                owner_id: row.get(3)?,
                created_at: row.get(4)?,
                pinned: row.get::<_, i64>(5)? != 0,
                isSystem: row.get::<_, i64>(6)? != 0,
                docCount: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[tauri::command]
pub async fn kb_delete(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可删除知识库".to_string());
    }
    if is_system_kb(&db, kb_id) {
        return Err("系统知识库不可删除".to_string());
    }
    let conn = db.conn_lock();
    // 先清理该知识库的检索日志（search_logs.kb_id 无级联删除，残留会导致外键约束失败）
    conn.execute(
        "DELETE FROM search_logs WHERE kb_id = ?1",
        rusqlite::params![kb_id],
    )
    .map_err(|e| e.to_string())?;
    // 清理 Wiki 页面 FTS 索引（wiki_pages 由外键级联删除，但普通 FTS 表不会自动清理）
    conn.execute(
        "DELETE FROM wiki_pages_fts WHERE rowid IN (SELECT id FROM wiki_pages WHERE kb_id = ?1)",
        rusqlite::params![kb_id],
    )
    .map_err(|e| e.to_string())?;
    // 先清理该知识库全部文档的 FTS 索引（chunks/versions 由外键级联删除），
    // 并收集其引用的 file_object_id 供删除后清理孤儿 BLOB
    let doc_ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM documents WHERE kb_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![kb_id], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let fo_ids: Vec<i64> = {
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT dv.file_object_id FROM document_versions dv
                 JOIN documents d ON d.id = dv.doc_id WHERE d.kb_id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![kb_id], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    for doc_id in &doc_ids {
        conn.execute(
            "DELETE FROM chunks_fts WHERE rowid IN (SELECT id FROM document_chunks WHERE doc_id = ?1)",
            rusqlite::params![doc_id],
        ).map_err(|e| e.to_string())?;
    }
    conn.execute(
        "DELETE FROM knowledge_bases WHERE id = ?1",
        rusqlite::params![kb_id],
    )
    .map_err(|e| e.to_string())?;
    cleanup_orphan_file_objects(&conn, &fo_ids)?;
    Ok(())
}

/// 编辑知识库（名称/描述；仅 owner/admin）
#[tauri::command]
pub async fn kb_update(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
    name: String,
    description: Option<String>,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可编辑".to_string());
    }
    if is_system_kb(&db, kb_id) {
        return Err("系统知识库不可重命名".to_string());
    }
    let n = name.trim().to_string();
    if n.is_empty() {
        return Err("知识库名称不能为空".to_string());
    }
    let conn = db.conn_lock();
    conn.execute(
        "UPDATE knowledge_bases SET name = ?1, description = ?2, updated_at = datetime('now') WHERE id = ?3",
        rusqlite::params![n, description, kb_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 判断是否为系统知识库
fn is_system_kb(db: &KbDatabase, kb_id: i64) -> bool {
    let conn = db.conn_lock();
    conn.query_row(
        "SELECT is_system FROM knowledge_bases WHERE id = ?1",
        rusqlite::params![kb_id],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        != 0
}

/// 置顶 / 取消置顶知识库（仅 owner/admin）
#[tauri::command]
pub async fn kb_set_pin(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    kb_id: i64,
    pinned: bool,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可置顶".to_string());
    }
    let conn = db.conn_lock();
    conn.execute(
        "UPDATE knowledge_bases SET pinned = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![pinned as i64, kb_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
