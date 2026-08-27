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
    #[serde(default)]
    pub effect: String, // allow / deny（kb_acl_delete 不传，默认空串）
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

/// 删除单条 ACL 规则（仅删除，不插入）。
/// 此前前端删除复用 kb_set_acl(effect='allow')，而后端是「先删后插」，
/// 导致删除操作把同一条规则又插回去，规则实际从未删除。
#[tauri::command]
pub async fn kb_acl_delete(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    input: AclInput,
) -> Result<(), String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    let kb_id = input.kb_id.ok_or("缺少 kb_id")?;
    if !crate::kb::retrieval::can_manage_kb(&db, kb_id, uid) {
        return Err("无权限：仅知识库 owner/admin 可删除 ACL".to_string());
    }
    let conn = db.conn_lock();
    let deleted = conn
        .execute(
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
    if deleted == 0 {
        return Err("未找到匹配的 ACL 规则".to_string());
    }
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
    // 仅当前 owner 可分配 owner 角色，防止 admin 提升他人为 owner 导致多 owner
    if role == "owner" {
        let cur_role = crate::kb::retrieval::kb_role(&db, kb_id, uid);
        if cur_role.as_deref() != Some("owner") {
            return Err("无权限：仅知识库 owner 可分配 owner 角色".to_string());
        }
    }
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
    _user_id: i64,
) -> Result<Vec<KbSummary>, String> {
    // 以登录态为准，不允许回退到前端传入的 user_id（防越权）
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
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

/// 删除知识库（原子事务 + 全表清理），供 tauri 命令与集成测试复用。
/// 逐表显式清理（不依赖外键级联），保证删除后不残留任何关联数据。
pub(crate) fn delete_kb_clean(db: &KbDatabase, kb_id: i64) -> Result<(), String> {
    let mut conn = db.conn_lock();
    // 整体包在事务里：任何一步失败都整体回滚，绝不留下“删了一半”的知识库残留。
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    // 1) 先收集该知识库的文档 id 与版本引用的 file_object_id（供删除后清理孤儿 BLOB）
    let doc_ids: Vec<i64> = {
        let mut stmt = tx
            .prepare("SELECT id FROM documents WHERE kb_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![kb_id], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let fo_ids: Vec<i64> = {
        let mut stmt = tx
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

    // 2) 显式清理全部关联表（依赖顺序：先子后父；FTS 表无外键必须手动清理）
    //    即使外键级联已开启，显式删除也更稳、更快、不依赖 FK 配置。
    for doc_id in &doc_ids {
        crate::kb::db::fts_delete_chunks_by_doc(&tx, *doc_id)?;
    }
    crate::kb::db::fts_delete_wiki_pages_by_kb(&tx, kb_id)?;
    // 子表（无 kb_id 的按 doc_id / job_id / page_id 关联）
    tx.execute(
        "DELETE FROM qa_messages WHERE session_id IN (SELECT id FROM qa_sessions WHERE kb_id = ?1)",
        rusqlite::params![kb_id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM processing_logs WHERE job_id IN (
            SELECT j.id FROM processing_jobs j JOIN documents d ON d.id = j.doc_id WHERE d.kb_id = ?1)",
        rusqlite::params![kb_id],
    )
    .map_err(|e| e.to_string())?;
    // 直接带 kb_id 的表
    for table in [
        "kb_metric_events",
        "wiki_page_entities",
        "wiki_links",
        "faq_entries",
        "search_logs",
        "qa_sessions",
        "kb_acl",
        "kb_members",
        "kb_directories",
        "documents",
    ] {
        let sql = format!("DELETE FROM {} WHERE kb_id = ?1", table);
        tx.execute(&sql, rusqlite::params![kb_id])
            .map_err(|e| e.to_string())?;
    }
    // 无 kb_id 列、需按 doc_id 关联的表
    for table in [
        "processing_jobs",
        "kb_doc_tags",
        "document_chunks",
        "document_versions",
    ] {
        let sql = format!(
            "DELETE FROM {} WHERE doc_id IN (SELECT id FROM documents WHERE kb_id = ?1)",
            table
        );
        tx.execute(&sql, rusqlite::params![kb_id])
            .map_err(|e| e.to_string())?;
    }

    // 3) 删除知识库本体
    tx.execute(
        "DELETE FROM knowledge_bases WHERE id = ?1",
        rusqlite::params![kb_id],
    )
    .map_err(|e| e.to_string())?;

    // 4) 清理不再被任何版本引用的孤儿原始文件（去重 BLOB）
    cleanup_orphan_file_objects(&tx, &fo_ids)?;

    // 5) 清理残留的批量取消标记（generate_cancel_{kb_id}，位于全局键值表）
    tx.execute(
        "DELETE FROM kb_chunk_settings WHERE key = ?1",
        rusqlite::params![format!("generate_cancel_{}", kb_id)],
    )
    .map_err(|e| e.to_string())?;

    // 6) 复位受影响 AUTOINCREMENT 表的序列，避免删除后新数据从旧 id 继续（不留痕迹）
    for table in [
        "documents",
        "document_chunks",
        "document_versions",
        "kb_acl",
        "kb_directories",
        "file_objects",
        "processing_jobs",
        "processing_logs",
        "qa_sessions",
        "qa_messages",
        "faq_entries",
        "kb_metric_events",
        "wiki_pages",
        "wiki_page_entities",
        "wiki_links",
        "search_logs",
    ] {
        let sql = format!(
            "UPDATE sqlite_sequence SET seq = COALESCE((SELECT MAX(id) FROM {}), 0) WHERE name = ?1",
            table
        );
        let _ = tx.execute(&sql, rusqlite::params![table]);
    }

    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
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
    delete_kb_clean(&db, kb_id)
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

/// 备份知识库数据库（仅全局管理员）
#[tauri::command]
pub async fn kb_backup(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
) -> Result<String, String> {
    let user = session.get().ok_or("请先登录知识库")?;
    if !is_global_admin(&db, user.id) {
        return Err("无权限：仅全局管理员可备份".to_string());
    }
    let path = db.backup()?;
    db.audit_log(
        Some(user.id),
        &user.username,
        "backup",
        "backup",
        None,
        &path.display().to_string(),
    );
    Ok(path.display().to_string())
}

/// 列出备份文件（仅全局管理员）
#[tauri::command]
pub async fn kb_list_backups(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
) -> Result<Vec<(String, u64)>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !is_global_admin(&db, uid) {
        return Err("无权限：仅全局管理员可查看备份".to_string());
    }
    Ok(KbDatabase::list_backups())
}

/// 清理旧备份（仅全局管理员）
#[tauri::command]
pub async fn kb_cleanup_backups(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    keep: usize,
) -> Result<usize, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !is_global_admin(&db, uid) {
        return Err("无权限：仅全局管理员可清理备份".to_string());
    }
    KbDatabase::cleanup_backups(keep)
}

/// 查询审计日志（仅全局管理员）
#[tauri::command]
pub async fn kb_list_audit_logs(
    db: State<'_, KbDatabase>,
    session: State<'_, crate::kb::auth::UserSession>,
    limit: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let uid = session.get().map(|u| u.id).ok_or("请先登录知识库")?;
    if !is_global_admin(&db, uid) {
        return Err("无权限：仅全局管理员可查看审计日志".to_string());
    }
    db.list_audit_logs(limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kb::db::KbDatabase;

    /// 删除知识库后，所有关联表不得残留任何数据（含 FTS、孤儿 BLOB、取消标记）。
    #[test]
    fn delete_kb_leaves_no_residue() {
        let dir = std::env::temp_dir().join(format!(
            "kb_del_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).ok();
        let db_path = dir.join("test.db");
        let db = KbDatabase::open_at(db_path.clone()).expect("open kb db");
        {
            let conn = db.conn_lock();
            conn.execute_batch(
                "INSERT INTO users (id, username, is_admin) VALUES (1, 'tester', 1);
                 INSERT INTO knowledge_bases (id, name, description, owner_id, is_system) VALUES (77,'测试库','',1,0);
                 INSERT INTO kb_directories (kb_id, parent_id, name) VALUES (77, NULL, '目录A');
                 INSERT INTO kb_members (kb_id, user_id, role) VALUES (77, 1, 'owner');
                 INSERT INTO kb_acl (scope, doc_id, dir_id, kb_id, grantee_type, effect) VALUES ('kb', NULL, NULL, 77, 'public', 'allow');
                 INSERT INTO documents (id, kb_id, title, source, status, process_status) VALUES (101, 77, 'doc1.md', 'upload', 'ready', 'ready');
                 INSERT INTO file_objects (id, hash, ext, size, blob_data) VALUES (201, 'abc123', 'md', 10, x'0102');
                 INSERT INTO document_versions (id, doc_id, version_no, file_object_id) VALUES (301, 101, 1, 201);
                 INSERT INTO documents (id, kb_id, title, source, status) VALUES (102, 77, 'doc2.md', 'upload', 'ready');
                 INSERT INTO file_objects (id, hash, ext, size, blob_data) VALUES (202, 'def456', 'md', 10, x'0304');
                 INSERT INTO document_versions (id, doc_id, version_no, file_object_id) VALUES (302, 102, 1, 202);
                 INSERT INTO document_chunks (id, kb_id, doc_id, version_id, seq, content) VALUES (401, 77, 101, 301, 1, 'hello chunk');
                 INSERT INTO chunks_fts (rowid, content) VALUES (401, 'hello chunk');
                 INSERT INTO kb_doc_tags (doc_id, tag) VALUES (101, 'tag1');
                 INSERT INTO processing_jobs (id, doc_id, version_id, stage) VALUES (501, 101, 301, 'done');
                 INSERT INTO processing_logs (job_id, level, message) VALUES (501, 'info', 'ok');
                 INSERT INTO search_logs (kb_id, user_id, query) VALUES (77, 1, 'q1');
                 INSERT INTO wiki_pages (id, kb_id, doc_id, title, slug, status) VALUES (601, 77, 101, '页面A', 'a', 'published');
                 INSERT INTO wiki_page_entities (kb_id, page_id, name) VALUES (77, 601, '实体A');
                 INSERT INTO wiki_links (kb_id, from_page_id, to_page_id, link_type) VALUES (77, 601, 601, 'related');
                 INSERT INTO wiki_pages_fts (rowid, title) VALUES (601, '页面A');
                 INSERT INTO qa_sessions (id, user_id, kb_id, title) VALUES (701, 1, 77, '会话');
                 INSERT INTO qa_messages (session_id, role, content) VALUES (701, 'user', 'hi');
                 INSERT INTO faq_entries (kb_id, question, answer) VALUES (77, 'q', 'a');
                 INSERT INTO kb_metric_events (kb_id, doc_id, event_type) VALUES (77, 101, 'doc_view');
                 INSERT INTO kb_chunk_settings (key, value) VALUES ('generate_cancel_77', '1');
                 INSERT INTO kb_chunk_settings (key, value) VALUES ('strategy', 'recursive');
                 -- 另一个知识库 78 引用 file_object 203：删除 77 时不得误删共享文件
                 INSERT INTO knowledge_bases (id, name, is_system) VALUES (78, '保留库', 0);
                 INSERT INTO documents (id, kb_id, title, source, status) VALUES (103, 78, 'shared.md', 'upload', 'ready');
                 INSERT INTO file_objects (id, hash, ext, size, blob_data) VALUES (203, 'shared', 'md', 1, x'00');
                 INSERT INTO document_versions (id, doc_id, version_no, file_object_id) VALUES (303, 103, 1, 203);
                 INSERT INTO documents (id, kb_id, title, source, status) VALUES (104, 77, 'doc3.md', 'upload', 'ready');
                 INSERT INTO document_versions (id, doc_id, version_no, file_object_id) VALUES (304, 104, 1, 203);",
            )
            .expect("seed data");
        }

        delete_kb_clean(&db, 77).expect("delete kb should succeed");

        let conn = db.conn_lock();
        let checks: &[(&str, &str)] = &[
            (
                "knowledge_bases",
                "SELECT COUNT(*) FROM knowledge_bases WHERE id = 77",
            ),
            (
                "documents",
                "SELECT COUNT(*) FROM documents WHERE kb_id = 77",
            ),
            (
                "document_chunks",
                "SELECT COUNT(*) FROM document_chunks WHERE kb_id = 77",
            ),
            (
                "document_versions",
                "SELECT COUNT(*) FROM document_versions WHERE doc_id IN (101,102,104)",
            ),
            (
                "processing_jobs",
                "SELECT COUNT(*) FROM processing_jobs WHERE doc_id IN (101,102,104)",
            ),
            (
                "processing_logs",
                "SELECT COUNT(*) FROM processing_logs WHERE job_id = 501",
            ),
            (
                "kb_doc_tags",
                "SELECT COUNT(*) FROM kb_doc_tags WHERE doc_id IN (101,102,104)",
            ),
            (
                "kb_directories",
                "SELECT COUNT(*) FROM kb_directories WHERE kb_id = 77",
            ),
            (
                "kb_members",
                "SELECT COUNT(*) FROM kb_members WHERE kb_id = 77",
            ),
            ("kb_acl", "SELECT COUNT(*) FROM kb_acl WHERE kb_id = 77"),
            (
                "search_logs",
                "SELECT COUNT(*) FROM search_logs WHERE kb_id = 77",
            ),
            (
                "faq_entries",
                "SELECT COUNT(*) FROM faq_entries WHERE kb_id = 77",
            ),
            (
                "kb_metric_events",
                "SELECT COUNT(*) FROM kb_metric_events WHERE kb_id = 77",
            ),
            (
                "wiki_pages",
                "SELECT COUNT(*) FROM wiki_pages WHERE kb_id = 77",
            ),
            (
                "wiki_page_entities",
                "SELECT COUNT(*) FROM wiki_page_entities WHERE kb_id = 77",
            ),
            (
                "wiki_links",
                "SELECT COUNT(*) FROM wiki_links WHERE kb_id = 77",
            ),
            (
                "qa_sessions",
                "SELECT COUNT(*) FROM qa_sessions WHERE kb_id = 77",
            ),
            (
                "qa_messages",
                "SELECT COUNT(*) FROM qa_messages WHERE session_id = 701",
            ),
            (
                "chunks_fts",
                "SELECT COUNT(*) FROM chunks_fts WHERE rowid = 401",
            ),
            (
                "wiki_pages_fts",
                "SELECT COUNT(*) FROM wiki_pages_fts WHERE rowid = 601",
            ),
        ];
        for (name, sql) in checks {
            let n: i64 = conn.query_row(sql, [], |r| r.get(0)).unwrap_or(-1);
            assert_eq!(n, 0, "删除后残留: {} count={}", name, n);
        }
        // 孤儿 file_objects（201/202）被清理；共享文件（203）仍被 78 引用 → 保留
        let orphan: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM file_objects WHERE id IN (201,202)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(orphan, 0, "孤儿 file_objects 应被清理");
        let shared: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM file_objects WHERE id = 203",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(shared, 1, "仍被其他库引用的 file_object 应保留");
        // 取消标记清理，全局设置保留
        let flag: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kb_chunk_settings WHERE key='generate_cancel_77'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(flag, 0, "批量取消标记应清理");
        let strategy: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kb_chunk_settings WHERE key='strategy'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(strategy, 1, "全局分块设置应保留");
        // 序列复位：documents 已清空 → seq=0；仍有关联数据的表不重置到 0 以下
        let seq: i64 = conn
            .query_row(
                "SELECT seq FROM sqlite_sequence WHERE name='documents'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(-1);
        assert_eq!(
            seq, 103,
            "documents 序列应复位为现存最大 id（保留库 doc 103 仍在）"
        );
        let kb_seq: i64 = conn
            .query_row(
                "SELECT seq FROM sqlite_sequence WHERE name='knowledge_bases'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(-1);
        assert!(kb_seq >= 78, "knowledge_bases 序列不得低于现存最大 id 78");
        // 保留库 78 的文档仍在
        let keep: i64 = conn
            .query_row("SELECT COUNT(*) FROM documents WHERE id = 103", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(keep, 1, "其他知识库数据不受影响");
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
