// ============================================================
// Harness — 预设组合与会话作用域（DSH preset + core/scope 迁移）
//
// 预设（data/harness/presets/presets.json，原子写）：
// - 名称/描述
// - disabled_tools：本预设禁用的工具（会话作用域过滤）
// - overrides：按工具覆盖（requires_approval / timeout_secs）
// - prompt_sections：附加 prompt 分区（随会话上下文注入）
// 作用域语义：全局工具注册表为基底，预设按「禁用集 + 覆盖表」过滤/
// 改写；默认预设来自用户设置（settings.preset_id）。
// 文件格式采用 JSON（DSH 的 cordis.yml 语法解析兼容留待后续评估，
// 避免为仅此一处引入 YAML 解析依赖）。
// ============================================================

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

/// 工具覆盖项（None = 不覆盖，沿用全局定义）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ToolOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_approval: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

/// 预设附加 prompt 分区
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PresetPromptSection {
    pub order: i32,
    pub title: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HarnessPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub disabled_tools: Vec<String>,
    #[serde(default)]
    pub overrides: HashMap<String, ToolOverride>,
    #[serde(default)]
    pub prompt_sections: Vec<PresetPromptSection>,
    pub created_at: String,
    pub updated_at: String,
}

/// 会话作用域（apply 预设后的有效工具集合）
#[derive(Clone, Debug, Default)]
pub struct SessionScope {
    pub disabled: HashSet<String>,
    pub overrides: HashMap<String, ToolOverride>,
    pub prompt_sections: Vec<PresetPromptSection>,
    pub preset_name: String,
}

impl SessionScope {
    pub fn is_disabled(&self, name: &str) -> bool {
        self.disabled.contains(name)
    }

    pub fn tool_timeout(&self, name: &str, fallback: u64) -> u64 {
        self.overrides
            .get(name)
            .and_then(|o| o.timeout_secs)
            .unwrap_or(fallback)
            .clamp(1, 300)
    }

    /// 覆盖审批要求（None = 沿用全局）
    pub fn requires_approval_override(&self, name: &str) -> Option<bool> {
        self.overrides.get(name).and_then(|o| o.requires_approval)
    }
}

fn presets_path() -> std::path::PathBuf {
    crate::common::st_data_dir()
        .join("harness")
        .join("presets")
        .join("presets.json")
}

pub(crate) fn presets_store() -> &'static Mutex<Vec<HarnessPreset>> {
    static P: OnceLock<Mutex<Vec<HarnessPreset>>> = OnceLock::new();
    P.get_or_init(|| {
        let list = std::fs::read_to_string(presets_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Mutex::new(list)
    })
}

pub(crate) fn persist(list: &[HarnessPreset]) -> Result<(), String> {
    let path = presets_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建预设目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(list).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换失败: {}", e))?;
    Ok(())
}

fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// 首次启动种子示例预设（DSH examples 迁移：示例即开即用）
pub(crate) fn seed_examples() {
    let mut list = presets_store().lock().unwrap();
    if !list.is_empty() {
        return;
    }
    let now = now_iso();
    list.push(HarnessPreset {
        id: "preset-example-readonly".to_string(),
        name: "示例-只读办公".to_string(),
        description: "禁用命令执行与写入类工具，附加办公提示词（示例预设）".to_string(),
        disabled_tools: vec![
            "exec_command".to_string(),
            "write_file".to_string(),
            "task".to_string(),
        ],
        overrides: HashMap::new(),
        prompt_sections: vec![PresetPromptSection {
            order: 10,
            title: "office".to_string(),
            content: "只回答与办公文档/资料检索相关的问题，不主动执行写操作。".to_string(),
        }],
        created_at: now.clone(),
        updated_at: now,
    });
    let _ = persist(&list);
}

/// 按预设 id 计算会话作用域；None/不存在 → 空作用域（全局基底不变）
pub fn scope_for_preset(preset_id: Option<&str>) -> SessionScope {
    let Some(pid) = preset_id.filter(|s| !s.is_empty()) else {
        return SessionScope::default();
    };
    let list = presets_store().lock().unwrap();
    let Some(p) = list.iter().find(|x| x.id == pid) else {
        return SessionScope::default();
    };
    SessionScope {
        disabled: p.disabled_tools.iter().cloned().collect(),
        overrides: p.overrides.clone(),
        prompt_sections: p.prompt_sections.clone(),
        preset_name: p.name.clone(),
    }
}

/// 会话作用域（默认预设来自用户设置；会话级预设覆盖全局）
pub fn scope_for_session() -> SessionScope {
    let preset = crate::harness::settings::current().preset_id;
    scope_for_preset(preset.as_deref())
}

/// 指定会话的作用域（每会话预设优先，否则全局默认）
pub fn scope_for_session_id(session_id: &str) -> SessionScope {
    let store =
        crate::harness::registry::get::<crate::harness::session::SessionStore>("harness.sessions");
    let session_preset = store
        .and_then(|s| s.preset_id(session_id).ok().flatten())
        .filter(|p| !p.is_empty());
    if let Some(p) = session_preset {
        return scope_for_preset(Some(p.as_str()));
    }
    scope_for_session()
}

// ─── IPC ───

#[tauri::command]
pub async fn list_harness_presets() -> Result<Vec<HarnessPreset>, String> {
    Ok(presets_store().lock().unwrap().clone())
}

/// 新建或更新预设（id 为空 → 新建）
#[tauri::command]
pub async fn save_harness_preset(preset: HarnessPreset) -> Result<HarnessPreset, String> {
    if preset.name.trim().is_empty() {
        return Err("预设名称不能为空".to_string());
    }
    for t in &preset.disabled_tools {
        if t.trim().is_empty() {
            return Err("禁用的工具名不能为空".to_string());
        }
    }
    let mut list = presets_store().lock().unwrap();
    let now = now_iso();
    let saved = if preset.id.is_empty() {
        let mut p = preset;
        p.id = format!("preset-{}", uuid::Uuid::new_v4().simple());
        p.created_at = now.clone();
        p.updated_at = now.clone();
        list.push(p.clone());
        p
    } else {
        let Some(existing) = list.iter().find(|p| p.id == preset.id) else {
            return Err("指定的预设不存在".to_string());
        };
        let mut p = preset;
        p.created_at = existing.created_at.clone();
        p.updated_at = now;
        let idx = list.iter().position(|x| x.id == p.id).unwrap();
        list[idx] = p.clone();
        p
    };
    persist(&list)?;
    Ok(saved)
}

#[tauri::command]
pub async fn delete_harness_preset(id: String) -> Result<(), String> {
    let mut list = presets_store().lock().unwrap();
    let before = list.len();
    list.retain(|p| p.id != id);
    if list.len() == before {
        return Err("指定的预设不存在".to_string());
    }
    persist(&list)
}

/// 当前会话作用域信息（默认预设应用结果；前端展示/诊断用）
#[derive(Serialize, Clone, Debug)]
pub struct ScopeInfo {
    pub preset_name: String,
    pub disabled_tools: Vec<String>,
}

#[tauri::command]
pub async fn get_harness_scope() -> Result<ScopeInfo, String> {
    let scope = scope_for_session();
    let mut disabled: Vec<String> = scope.disabled.iter().cloned().collect();
    disabled.sort();
    Ok(ScopeInfo {
        preset_name: scope.preset_name,
        disabled_tools: disabled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_preset(name: &str) -> HarnessPreset {
        HarnessPreset {
            id: String::new(),
            name: name.to_string(),
            description: String::new(),
            disabled_tools: vec!["web_search".to_string()],
            overrides: HashMap::from([(
                "exec_command".to_string(),
                ToolOverride {
                    requires_approval: Some(false),
                    timeout_secs: Some(5),
                },
            )]),
            prompt_sections: vec![PresetPromptSection {
                order: 5,
                title: "p".into(),
                content: "C".into(),
            }],
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn scope_applies_disable_and_overrides() {
        let mut p = sample_preset("t");
        p.id = format!("t-{}", uuid::Uuid::new_v4().simple());
        {
            let mut list = presets_store().lock().unwrap();
            list.retain(|x| x.id != p.id);
            list.push(p.clone());
        }
        let scope = scope_for_preset(Some(&p.id));
        assert!(scope.is_disabled("web_search"));
        assert!(!scope.is_disabled("read_file"));
        assert_eq!(
            scope.requires_approval_override("exec_command"),
            Some(false)
        );
        assert_eq!(scope.tool_timeout("exec_command", 30), 5);
        assert_eq!(scope.tool_timeout("read_file", 30), 30);
        assert_eq!(scope.prompt_sections.len(), 1);
        {
            let mut list = presets_store().lock().unwrap();
            list.retain(|x| x.id != p.id);
        }
    }

    #[test]
    fn missing_preset_gives_empty_scope() {
        let scope = scope_for_preset(Some("preset-nonexistent"));
        assert!(!scope.is_disabled("web_search"));
        assert!(scope.prompt_sections.is_empty());
    }
}
