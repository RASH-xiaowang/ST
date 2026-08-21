// ============================================================
// Harness — 工作区注册表（DSH workspace 迁移）
//
// WorkspaceRegistry：多工作区实体（id/title/目录/状态）。目录约定：
// 默认工作区（dir=""）= 应用项目根（沙箱锚点 = 应用根，模型可读写
// 自身源码实现自维护）；显式创建工作区 = agent_workspace 下的子目录。
// 当前工作区为全局设置（settings.workspace_id），终端/Shell 默认 cwd、
// exec_command 锚点与 fs 相对路径锚点跟随当前工作区。
// create/list/delete/rename/archive + 模型工具 workspace_list /
// workspace_create / workspace_switch。
// ============================================================

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorkspaceEntity {
    pub id: String,
    pub title: String,
    /// 相对 agent_workspace 的子目录名（"" = 默认工作区 = 应用项目根）
    pub dir: String,
    /// active / archived
    pub status: String,
    pub created_at: String,
}

fn workspaces_path() -> std::path::PathBuf {
    crate::common::st_data_dir()
        .join("harness")
        .join("workspaces.json")
}

fn store() -> &'static Mutex<Vec<WorkspaceEntity>> {
    static S: OnceLock<Mutex<Vec<WorkspaceEntity>>> = OnceLock::new();
    S.get_or_init(|| {
        let list = std::fs::read_to_string(workspaces_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Mutex::new(list)
    })
}

fn persist(list: &[WorkspaceEntity]) -> Result<(), String> {
    let path = workspaces_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建工作区目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(list).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换失败: {}", e))?;
    Ok(())
}

/// 默认工作区（工作区根 = 应用项目根，自维护源码工作区）
pub fn default_workspace() -> WorkspaceEntity {
    WorkspaceEntity {
        id: "default".to_string(),
        title: "项目根（自维护）".to_string(),
        dir: String::new(),
        status: "active".to_string(),
        created_at: String::new(),
    }
}

/// 工作区实际目录：默认工作区 = 应用项目根（放大工作路径，可读写自身
/// 源码）；显式工作区 = agent_workspace/<安全子目录名>（防 .. 逃逸）
pub fn workspace_dir(dir: &str) -> std::path::PathBuf {
    if dir.is_empty() {
        return crate::common::app_base_dir();
    }
    let root = crate::llm::agent::workspace_root();
    // 仅允许安全的相对子目录名，防 .. 逃逸
    let clean: String = dir
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    root.join(clean)
}

/// 当前沙箱根（fs/shell/exec/指令扫描的锚点；跟随当前工作区）
pub fn sandbox_root() -> std::path::PathBuf {
    let cur = current();
    workspace_dir(&cur.dir)
}

/// 当前工作区实体（settings.workspace_id 解析；未命中回退默认）
pub fn current() -> WorkspaceEntity {
    let id = crate::harness::settings::current().workspace_id.clone();
    store()
        .lock()
        .unwrap()
        .iter()
        .find(|w| w.id == id && w.status == "active")
        .cloned()
        .unwrap_or_else(default_workspace)
}

/// 创建（目录 = agent_workspace/<id>，id 自动生成）
pub fn create(title: &str) -> Result<WorkspaceEntity, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("工作区名称不能为空".to_string());
    }
    let id = format!("ws-{}", uuid::Uuid::new_v4().simple());
    let dir = id.clone();
    let entity = WorkspaceEntity {
        id: id.clone(),
        title: title.to_string(),
        dir: dir.clone(),
        status: "active".to_string(),
        created_at: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    };
    std::fs::create_dir_all(workspace_dir(&dir))
        .map_err(|e| format!("创建工作区目录失败: {}", e))?;
    let mut list = store().lock().unwrap();
    list.push(entity.clone());
    persist(&list)?;
    Ok(entity)
}

pub fn list() -> Vec<WorkspaceEntity> {
    let mut v = vec![default_workspace()];
    v.extend(store().lock().unwrap().iter().cloned());
    v
}

/// 删除（目录随之一并删除；默认工作区不可删除）
pub fn delete(id: &str) -> Result<(), String> {
    if id == "default" {
        return Err("默认工作区不可删除".to_string());
    }
    let mut list = store().lock().unwrap();
    let Some(w) = list.iter().find(|w| w.id == id).cloned() else {
        return Err("指定的工作区不存在".to_string());
    };
    if !w.dir.is_empty() {
        let d = workspace_dir(&w.dir);
        let _ = std::fs::remove_dir_all(&d);
    }
    list.retain(|x| x.id != id);
    persist(&list)
}

/// 归档/恢复
pub fn set_status(id: &str, status: &str) -> Result<(), String> {
    let mut list = store().lock().unwrap();
    let Some(w) = list.iter_mut().find(|w| w.id == id) else {
        return Err("指定的工作区不存在".to_string());
    };
    w.status = match status {
        "active" | "archived" => status.to_string(),
        _ => return Err("无效状态".to_string()),
    };
    persist(&list)
}

// ─── IPC ───

#[tauri::command]
pub async fn list_harness_workspaces() -> Result<Vec<WorkspaceEntity>, String> {
    Ok(list())
}

#[tauri::command]
pub async fn create_harness_workspace(title: String) -> Result<WorkspaceEntity, String> {
    create(&title)
}

#[tauri::command]
pub async fn delete_harness_workspace(id: String) -> Result<(), String> {
    delete(&id)
}

#[tauri::command]
pub async fn set_harness_workspace_status(id: String, status: String) -> Result<(), String> {
    set_status(&id, &status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_create_list_delete() {
        let w = create("测试工作区").unwrap();
        assert!(w.id.starts_with("ws-"));
        assert!(list().iter().any(|x| x.id == w.id));
        assert!(workspace_dir(&w.dir).exists());
        set_status(&w.id, "archived").unwrap();
        assert!(
            list()
                .iter()
                .find(|x| x.id == w.id)
                .map(|x| x.status.as_str())
                == Some("archived")
        );
        delete(&w.id).unwrap();
        assert!(!list().iter().any(|x| x.id == w.id));
    }

    #[test]
    fn default_workspace_is_project_root() {
        let d = default_workspace();
        assert_eq!(d.id, "default");
        assert_eq!(workspace_dir(""), crate::common::app_base_dir());
        assert_eq!(sandbox_root(), crate::common::app_base_dir());
    }

    #[test]
    fn workspace_dir_sanitizes_and_blocks_escape() {
        // 目录名清理：非字母数字/-/_ → 下划线；锚定在 workspace_root 内
        let root = crate::llm::agent::workspace_root();
        // 路径逃逸尝试被清理（. 与 / 全变下划线，不会越出根）
        let p = workspace_dir("../evil");
        let fname = p.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(fname, "___evil", ".. 与 / 应清理为下划线: {fname}");
        assert!(
            p.starts_with(&root),
            "工作区目录必须锚定在根内: {}",
            p.display()
        );
        // 正常名称保留
        let p = workspace_dir("ws-abc_1");
        let fname = p.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(fname, "ws-abc_1");
        // 空 = app 根（默认工作区）
        assert_eq!(workspace_dir(""), crate::common::app_base_dir());
    }

    #[test]
    fn default_workspace_cannot_be_deleted() {
        // 默认工作区保护：删除被拒绝
        let err = delete("default").unwrap_err();
        assert!(err.contains("默认工作区"), "默认工作区不可删除: {err}");
    }
}
