// ============================================================
// 大模型管理模块 — AI 角色外部调用接口（读取端）
//
// 角色由 Agent 模块（st_agent）定义并持久化到共享文件
// `st_role/roles.json`。本模块提供只读接口 get_ai_roles，
// 供「全局调用」检索并调用这些已定义的 AI 角色，实现跨模块角色复用
// 与统一调度。
//
// 路径需与 st_agent 后端的 role_store.rs 保持一致。
// ============================================================

use serde::{Deserialize, Serialize};

/// 与 st_agent 后端的 role_store.rs 保持一致：使用系统数据目录下的共享文件，
/// 让 Control 端能够读取 Agent 端配置的 AI 角色。
///
/// 统一目录方案后优先读 `<应用基目录>/data/roles/roles.json`；
/// 旧位置 %APPDATA%/st_role 仍作为回退（st_agent 旧版本写入处，迁移前兼容）。
fn role_dir() -> std::path::PathBuf {
    crate::common::role_data_dir()
}
fn role_file() -> std::path::PathBuf {
    let new_path = role_dir().join("roles.json");
    if new_path.is_file() {
        return new_path;
    }
    if let Some(legacy) = dirs::data_dir() {
        let legacy_path = legacy.join("st_role").join("roles.json");
        if legacy_path.is_file() {
            return legacy_path;
        }
    }
    new_path
}

fn default_true() -> bool {
    true
}
fn default_temperature() -> f64 {
    0.7
}
fn default_max_tokens() -> u64 {
    2048
}
fn default_one() -> f64 {
    1.0
}
fn default_empty() -> String {
    String::new()
}

/// 与 st_agent role_store::AiRole 保持一致的镜像结构（只读消费端）。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AiRole {
    #[serde(default = "default_empty")]
    pub id: String,
    #[serde(default = "default_empty")]
    pub name: String,
    #[serde(default = "default_empty")]
    pub emoji: String,
    #[serde(default = "default_empty")]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_empty")]
    pub system_prompt: String,
    #[serde(default)]
    pub preferred_provider_name: Option<String>,
    #[serde(default)]
    pub preferred_model: Option<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u64,
    #[serde(default = "default_one")]
    pub top_p: f64,
    #[serde(default)]
    pub presence_penalty: f64,
    #[serde(default)]
    pub frequency_penalty: f64,
    #[serde(default)]
    pub behavior_constraints: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_empty")]
    pub response_language: String,
    #[serde(default = "default_empty")]
    pub knowledge_context: String,
    #[serde(default = "default_empty")]
    pub created_at: String,
    #[serde(default = "default_empty")]
    pub updated_at: String,
}

/// 将 AI 角色的配置项合成为一条系统提示词，供大模型「全局调用」注入消息列表。
pub fn compose_system_prompt(role: &AiRole) -> String {
    let mut sections: Vec<String> = Vec::new();

    let prompt = role.system_prompt.trim();
    if !prompt.is_empty() {
        sections.push(prompt.to_string());
    }

    let constraints: Vec<&str> = role
        .behavior_constraints
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if !constraints.is_empty() {
        sections.push(format!(
            "【行为约束】\n{}",
            constraints
                .iter()
                .map(|c| format!("- {}", c))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    let knowledge = role.knowledge_context.trim();
    if !knowledge.is_empty() {
        sections.push(format!("【背景知识】\n{}", knowledge));
    }

    let lang = role.response_language.trim();
    if !lang.is_empty() && lang != "跟随用户" {
        sections.push(format!("【回复语言】请使用 {} 回复。", lang));
    }

    sections.join("\n\n")
}

/// 检索全部已启用（及全部）的 AI 角色。全局调用端可在前端过滤 enabled。
#[tauri::command]
pub fn get_ai_roles() -> Vec<AiRole> {
    load_roles_from_disk()
}

// ──────────────────── helpers（写权限，st_control 端镜像实现）────────────────────

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn new_role_id() -> String {
    format!("role_{}", chrono::Utc::now().timestamp_millis())
}

fn load_roles_from_disk() -> Vec<AiRole> {
    match std::fs::read_to_string(role_file()) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str::<serde_json::Value>(&s)
            .ok()
            .and_then(|v| v.get("roles").cloned())
            .and_then(|roles| serde_json::from_value::<Vec<AiRole>>(roles).ok())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn save_roles_to_disk(roles: &[AiRole]) -> Result<(), String> {
    let _ = std::fs::create_dir_all(role_dir());
    let value = serde_json::json!({ "roles": roles });
    let s = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(role_file(), s).map_err(|e| e.to_string())
}

/// 获取单个角色详情
#[tauri::command]
pub fn get_ai_role(id: String) -> Option<AiRole> {
    load_roles_from_disk().into_iter().find(|r| r.id == id)
}

/// 新增或更新角色（按 id upsert）
#[tauri::command]
pub fn save_ai_role(mut role: AiRole) -> Result<AiRole, String> {
    let now = now_rfc3339();
    let mut roles = load_roles_from_disk();

    if role.id.trim().is_empty() {
        role.id = new_role_id();
        role.created_at = now.clone();
    } else if let Some(existing) = roles.iter().find(|r| r.id == role.id) {
        role.created_at = existing.created_at.clone();
    } else {
        role.created_at = now.clone();
    }
    role.updated_at = now;

    // 清理空项
    role.behavior_constraints.retain(|c| !c.trim().is_empty());
    role.capabilities.retain(|c| !c.trim().is_empty());
    if let Some(p) = role.preferred_provider_name.as_ref() {
        if p.trim().is_empty() {
            role.preferred_provider_name = None;
        }
    }
    if let Some(m) = role.preferred_model.as_ref() {
        if m.trim().is_empty() {
            role.preferred_model = None;
        }
    }

    if let Some(slot) = roles.iter_mut().find(|r| r.id == role.id) {
        *slot = role.clone();
    } else {
        roles.push(role.clone());
    }
    save_roles_to_disk(&roles)?;
    Ok(role)
}

/// 删除角色
#[tauri::command]
pub fn delete_ai_role(id: String) -> Result<bool, String> {
    let mut roles = load_roles_from_disk();
    let before = roles.len();
    roles.retain(|r| r.id != id);
    let removed = roles.len() != before;
    if removed {
        save_roles_to_disk(&roles)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role() -> AiRole {
        // AiRole 无 Default：用 serde 缺省字段反序列化构造
        serde_json::from_value(serde_json::json!({
            "id": "r1", "name": "测试",
            "system_prompt": "你是测试助手",
            "behavior_constraints": ["不编造", " 简洁回答 "],
            "knowledge_context": "背景资料A",
            "response_language": "中文",
        }))
        .unwrap()
    }

    #[test]
    fn compose_system_prompt_sections() {
        let r = role();
        let prompt = compose_system_prompt(&r);
        assert!(prompt.contains("你是测试助手"), "应含系统提示");
        assert!(prompt.contains("【行为约束】"), "应含行为约束分区");
        assert!(prompt.contains("- 不编造"), "约束应转列表项");
        assert!(prompt.contains("简洁回答"), "约束空白已 trim");
        assert!(prompt.contains("【背景知识】"), "应含背景知识分区");
        assert!(
            prompt.contains("【回复语言】请使用 中文 回复"),
            "应含语言分区"
        );
    }

    #[test]
    fn compose_system_prompt_empty_and_lang_default() {
        // 空角色 → 空提示词
        let empty: AiRole = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(compose_system_prompt(&empty), "");
        // 语言「跟随用户」→ 不注入语言分区
        let mut r = role();
        r.response_language = "跟随用户".into();
        let prompt = compose_system_prompt(&r);
        assert!(!prompt.contains("【回复语言】"), "跟随用户不应注入语言分区");
        // 约束空 → 无约束分区
        let mut r2 = role();
        r2.behavior_constraints = vec![];
        let prompt = compose_system_prompt(&r2);
        assert!(!prompt.contains("【行为约束】"));
    }
}
