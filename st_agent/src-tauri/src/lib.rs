// Suppress Windows linker info messages
#![allow(linker_messages)]

mod task_manager;
mod role_store;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // 锁定窗口：禁止全屏模式，禁止调整窗口尺寸
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_resizable(false);
                let _ = window.set_fullscreen(false);
                log::info!("Agent 窗口已锁定：禁止全屏和缩放");

                // 定期强制检查全屏状态（防止 devtools / 其他绕过方式）
                let window_clone = window.clone();
                std::thread::spawn(move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        if let Ok(true) = window_clone.is_fullscreen() {
                            log::warn!("检测到全屏尝试，正在强制退出全屏");
                            let _ = window_clone.set_fullscreen(false);
                        }
                    }
                });
            }

            // 初始化任务管理器（自动创建默认目录）
            let manager = task_manager::TaskManager::new()
                .expect("初始化任务管理器失败");
            let manager = Arc::new(manager);

            log::info!("任务存储路径: {}", manager.get_current_path().display());

            app.manage(manager);

            // 初始化 AI 角色存储（跨模块共享，供大模型管理「全局调用」检索）
            let role_store = Arc::new(role_store::RoleStore::new().expect("初始化 AI 角色存储失败"));
            app.manage(role_store);
            log::info!("AI 角色存储路径: {}", role_store::role_file().display());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc_get_task_path,
            ipc_set_task_path,
            ipc_get_hostname,
            ipc_save_task,
            ipc_open_folder,
            ipc_get_task_statuses,
            ipc_update_task_status,
            ipc_get_task_files_by_status,
            ipc_get_task_file_content,
            role_store::role_list,
            role_store::role_get,
            role_store::role_save,
            role_store::role_delete,
        ])
        .run(tauri::generate_context!())
        .expect("启动 ST Agent 失败");
}

// ============================================================
// IPC 命令
// ============================================================

/// 获取当前任务存储路径信息
#[tauri::command]
fn ipc_get_task_path(
    manager: tauri::State<'_, Arc<task_manager::TaskManager>>,
) -> Result<task_manager::PathInfo, String> {
    manager.get_path_info()
}

/// 设置新的任务存储路径（自动迁移数据）
#[tauri::command]
fn ipc_set_task_path(
    manager: tauri::State<'_, Arc<task_manager::TaskManager>>,
    path: String,
) -> Result<task_manager::PathInfo, String> {
    manager.set_path(&path)
}

/// 保存接收到的任务到任务存储路径
#[tauri::command]
fn ipc_save_task(
    manager: tauri::State<'_, Arc<task_manager::TaskManager>>,
    task_id: String,
    method: String,
    payload: serde_json::Value,
) -> Result<String, String> {
    manager.save_task(&task_id, &method, &payload)
}

/// 获取任务状态汇总（已完成 / 失败 / 待执行数量）
#[tauri::command]
fn ipc_get_task_statuses(
    manager: tauri::State<'_, Arc<task_manager::TaskManager>>,
) -> Result<task_manager::TaskStatusSummary, String> {
    manager.count_task_statuses()
}

/// 更新指定任务的状态
#[tauri::command]
fn ipc_update_task_status(
    manager: tauri::State<'_, Arc<task_manager::TaskManager>>,
    task_id: String,
    status: String,
) -> Result<(), String> {
    manager.update_task_status(&task_id, &status)
}

/// 获取 Agent 主机名
#[tauri::command]
fn ipc_get_hostname() -> Result<String, String> {
    let name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    Ok(name)
}

/// 在系统文件管理器中打开指定路径
#[tauri::command]
fn ipc_open_folder(path: String) -> Result<(), String> {
    use std::path::Path;
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("路径不存在: {}", path));
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: explorer 直接接收裸路径（不要加引号，否则解析异常）
        if path.starts_with('/') || path.starts_with('\\') {
            // WSL / UNC 风格路径 → 用 cmd /c start 处理
            std::process::Command::new("cmd")
                .args(["/c", "start", "", &path])
                .spawn()
                .map_err(|e| format!("打开失败: {}", e))?;
        } else {
            std::process::Command::new("explorer")
                .arg(&path)
                .spawn()
                .map_err(|e| format!("打开失败: {}", e))?;
        }
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("打开失败: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("打开失败: {}", e))?;
    }
    Ok(())
}

/// 按状态查询任务文件列表
#[tauri::command]
fn ipc_get_task_files_by_status(
    manager: tauri::State<'_, Arc<task_manager::TaskManager>>,
    status: String,
) -> Result<Vec<task_manager::TaskFileEntry>, String> {
    manager.get_files_by_status(&status)
}

/// 读取指定任务文件的完整 JSON 内容
#[tauri::command]
fn ipc_get_task_file_content(
    manager: tauri::State<'_, Arc<task_manager::TaskManager>>,
    file_path: String,
) -> Result<String, String> {
    manager.get_file_content(&file_path)
}
