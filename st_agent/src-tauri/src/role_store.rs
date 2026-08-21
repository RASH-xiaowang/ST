// ============================================================
// Agent 模块 — AI 角色定位（跨模块角色存储）
//
// 本模块负责任管理：对各类 AI 角色进行增删改查，并将其持久化到
// 一个与「大模型管理」模块约定的共享文件中。角色的“系统提示词”与
// 采样参数（temperature / max_tokens / top_p / 惩罚项）、行为约束等
// 配置项对标大模型的 system prompt，用于对角色进行精准定义与行为约束。
//
// 该共享文件即为跨模块的“外部调用接口”：大模型管理模块的「全局调用」
// 直接读取此文件，检索并调用这里定义的 AI 角色，实现角色复用与统一调度。
// ============================================================

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

/// 与大模型管理模块约定的跨模块角色共享目录 / 文件。
/// 注意：路径需与 st_control 后端的 ai_role.rs 保持一致。
/// 使用系统数据目录，保证 Agent 与 Control 两个独立 Tauri 应用能读写同一文件。
fn role_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("st_role")
}
pub fn role_file() -> PathBuf {
    role_dir().join("roles.json")
}

// ── 默认值辅助函数（保证旧数据 / 部分字段缺失时不报错） ──
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

/// 单个 AI 角色定义。配置项对标大模型 system prompt 与调用参数。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AiRole {
    /// 角色唯一 ID（为空时由后端自动生成）
    #[serde(default = "default_empty")]
    pub id: String,
    /// 角色名称
    #[serde(default = "default_empty")]
    pub name: String,
    /// 头像（emoji 或短文本）
    #[serde(default = "default_empty")]
    pub emoji: String,
    /// 角色简介
    #[serde(default = "default_empty")]
    pub description: String,
    /// 是否启用（禁用后全局调用不可检索到）
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 系统提示词（核心，对标 system prompt）
    #[serde(default = "default_empty")]
    pub system_prompt: String,
    /// 偏好提供方名称（可选，全局调用尝试匹配）
    #[serde(default)]
    pub preferred_provider_name: Option<String>,
    /// 偏好模型（可选，全局调用尝试匹配）
    #[serde(default)]
    pub preferred_model: Option<String>,
    /// 温度（对标调用参数）
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    /// 单次最大生成 token
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u64,
    /// top_p
    #[serde(default = "default_one")]
    pub top_p: f64,
    /// 存在惩罚
    #[serde(default)]
    pub presence_penalty: f64,
    /// 频率惩罚
    #[serde(default)]
    pub frequency_penalty: f64,
    /// 行为约束（禁止 / 必须事项，对标 system prompt 中的规范）
    #[serde(default)]
    pub behavior_constraints: Vec<String>,
    /// 能力标签
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// 回复语言约束（如：中文 / English / 跟随用户）
    #[serde(default = "default_empty")]
    pub response_language: String,
    /// 背景知识 / 上下文注入
    #[serde(default = "default_empty")]
    pub knowledge_context: String,
    /// 创建时间（RFC3339）
    #[serde(default = "default_empty")]
    pub created_at: String,
    /// 更新时间（RFC3339）
    #[serde(default = "default_empty")]
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Default)]
struct RoleRegistry {
    #[serde(default)]
    roles: Vec<AiRole>,
}

pub struct RoleStore {
    roles: Mutex<Vec<AiRole>>,
}

impl RoleStore {
    pub fn new() -> Result<Self, String> {
        let role_file_path = role_file();
        let _ = fs::create_dir_all(role_dir());
        let roles = if role_file_path.exists() {
            match fs::read_to_string(&role_file_path) {
                Ok(s) if !s.trim().is_empty() => serde_json::from_str::<RoleRegistry>(&s)
                    .map(|r| r.roles)
                    .unwrap_or_default(),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        Ok(Self {
            roles: Mutex::new(roles),
        })
    }

    fn persist(roles: &[AiRole]) -> Result<(), String> {
        let role_file_path = role_file();
        let _ = fs::create_dir_all(role_dir());
        let registry = RoleRegistry {
            roles: roles.to_vec(),
        };
        let s = serde_json::to_string_pretty(&registry).map_err(|e| e.to_string())?;
        fs::write(role_file_path, s).map_err(|e| e.to_string())
    }

    pub fn list(&self) -> Vec<AiRole> {
        self.roles.lock().unwrap().clone()
    }

    pub fn get(&self, id: &str) -> Option<AiRole> {
        self.roles
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.id == id)
            .cloned()
    }

    pub fn save(&self, mut role: AiRole) -> Result<AiRole, String> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut guard = self.roles.lock().unwrap();

        if role.id.trim().is_empty() {
            role.id = format!("role_{}", chrono::Utc::now().timestamp_millis());
            role.created_at = now.clone();
        } else if let Some(existing) = guard.iter().find(|r| r.id == role.id) {
            role.created_at = existing.created_at.clone();
        } else {
            role.created_at = now.clone();
        }
        role.updated_at = now;

        // 清理空项，规范化可选字段
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

        if let Some(slot) = guard.iter_mut().find(|r| r.id == role.id) {
            *slot = role.clone();
        } else {
            guard.push(role.clone());
        }
        Self::persist(&guard)?;
        Ok(role)
    }

    pub fn delete(&self, id: &str) -> Result<bool, String> {
        let mut guard = self.roles.lock().unwrap();
        let before = guard.len();
        guard.retain(|r| r.id != id);
        let removed = guard.len() != before;
        Self::persist(&guard)?;
        Ok(removed)
    }
}

// ============================================================
// IPC 命令（外部调用接口）
// ============================================================

/// 列出全部 AI 角色
#[tauri::command]
pub fn role_list(store: tauri::State<'_, std::sync::Arc<RoleStore>>) -> Vec<AiRole> {
    store.list()
}

/// 获取单个角色详情
#[tauri::command]
pub fn role_get(
    id: String,
    store: tauri::State<'_, std::sync::Arc<RoleStore>>,
) -> Option<AiRole> {
    store.get(&id)
}

/// 新增或更新角色（按 id upsert）
#[tauri::command]
pub fn role_save(
    role: AiRole,
    store: tauri::State<'_, std::sync::Arc<RoleStore>>,
) -> Result<AiRole, String> {
    store.save(role)
}

/// 删除角色
#[tauri::command]
pub fn role_delete(
    id: String,
    store: tauri::State<'_, std::sync::Arc<RoleStore>>,
) -> Result<bool, String> {
    store.delete(&id)
}
