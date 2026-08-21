// ============================================================
// Harness — 终端能力（DSH terminal 迁移）
//
// 持久终端会话：保持工作目录（cwd）与输入/输出日志的会话实体。
// 每次 send 在会话 cwd 下执行命令，命令尾部追加定位标记捕获新 cwd
// （状态保持语义；非 PTY 交互终端——DSH 的 PTY 后端留待后续评估）。
// 会话列表持久化（data/harness/terminals.json），日志为运行时状态。
// ============================================================

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

/// cwd 定位标记（命令尾部注入，解析输出末尾行）
const CWD_MARKER: &str = "__HNS_CWD__";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TerminalSession {
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub created_at: String,
}

/// 终端日志条目（运行时状态，不持久化）
#[derive(Serialize, Clone, Debug)]
pub struct TerminalLogEntry {
    pub input: String,
    pub output: String,
}

fn terminals_path() -> std::path::PathBuf {
    crate::common::st_data_dir()
        .join("harness")
        .join("terminals.json")
}

fn sessions_store() -> &'static Mutex<Vec<TerminalSession>> {
    static S: OnceLock<Mutex<Vec<TerminalSession>>> = OnceLock::new();
    S.get_or_init(|| {
        let list = std::fs::read_to_string(terminals_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Mutex::new(list)
    })
}

fn logs_store() -> &'static Mutex<std::collections::HashMap<String, Vec<TerminalLogEntry>>> {
    static L: OnceLock<Mutex<std::collections::HashMap<String, Vec<TerminalLogEntry>>>> =
        OnceLock::new();
    L.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn persist_sessions(list: &[TerminalSession]) -> Result<(), String> {
    let path = terminals_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建终端目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(list).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换失败: {}", e))?;
    Ok(())
}

// ─── IPC ───

#[tauri::command]
pub async fn list_harness_terminals() -> Result<Vec<TerminalSession>, String> {
    Ok(sessions_store().lock().unwrap().clone())
}

/// 新建终端会话（cwd 默认当前工作区目录；DSH workspace 迁移）
#[tauri::command]
pub async fn create_harness_terminal(name: Option<String>) -> Result<TerminalSession, String> {
    let mut list = sessions_store().lock().unwrap();
    let ws = crate::harness::workspace::current();
    let cwd = crate::harness::workspace::workspace_dir(&ws.dir);
    std::fs::create_dir_all(&cwd).map_err(|e| format!("创建工作区目录失败: {}", e))?;
    let session = TerminalSession {
        id: format!("term-{}", uuid::Uuid::new_v4().simple()),
        name: name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "终端".to_string()),
        cwd: cwd.display().to_string(),
        created_at: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    };
    list.push(session.clone());
    persist_sessions(&list)?;
    Ok(session)
}

#[tauri::command]
pub async fn delete_harness_terminal(id: String) -> Result<(), String> {
    // PTY 进程一并回收（防止孤儿 shell 常驻）
    super::pty::stop(&id);
    let mut list = sessions_store().lock().unwrap();
    let before = list.len();
    list.retain(|t| t.id != id);
    if list.len() == before {
        return Err("指定的终端会话不存在".to_string());
    }
    logs_store().lock().unwrap().remove(&id);
    persist_sessions(&list)
}

/// 终端日志（运行时状态）
#[tauri::command]
pub async fn harness_terminal_logs(id: String) -> Result<Vec<TerminalLogEntry>, String> {
    let logs = logs_store().lock().unwrap();
    Ok(logs.get(&id).cloned().unwrap_or_default())
}

/// 在会话 cwd 下执行命令，更新 cwd 与日志
#[tauri::command]
pub async fn harness_terminal_send(id: String, input: String) -> Result<String, String> {
    send_regular(&id, &input)
}

/// 普通（非 PTY）终端发送核心：独立进程执行 + cwd 标记定位（模型工具复用）
pub(crate) fn send_regular(id: &str, input: &str) -> Result<String, String> {
    let input = input.trim().to_string();
    if input.is_empty() {
        return Err("输入不能为空".to_string());
    }
    let cwd = {
        let list = sessions_store().lock().unwrap();
        list.iter()
            .find(|t| t.id == id)
            .map(|t| t.cwd.clone())
            .ok_or("指定的终端会话不存在")?
    };
    // 命令尾部注入 cwd 定位标记（最后一行解析新工作目录）
    let effective = format!(
        "{}; Write-Output ('{}' + (Get-Location).Path)",
        input, CWD_MARKER
    );
    let svc = crate::harness::registry::get::<crate::harness::shell::ShellService>("harness.shell")
        .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    let timeout = crate::harness::settings::current().effective_timeout_secs();
    let result = svc.run(&effective, Some(&cwd), timeout);
    // 解析新 cwd（取输出中最后一个标记行；规范化剥离
    // PowerShell FileSystem 提供者前缀与 \\?\ 前缀）
    let new_cwd = result.output.lines().rev().find_map(|l| {
        l.split_once(CWD_MARKER)
            .map(|(_m, p)| normalize_cwd(p.trim()))
    });
    if let Some(c) = &new_cwd {
        let mut list = sessions_store().lock().unwrap();
        if let Some(t) = list.iter_mut().find(|t| t.id == id) {
            t.cwd = c.clone();
        }
        let _ = persist_sessions(&list);
    }
    // 日志：剥离标记行后的输出
    let clean = result
        .output
        .lines()
        .filter(|l| !l.contains(CWD_MARKER))
        .collect::<Vec<_>>()
        .join("\n");
    logs_store()
        .lock()
        .unwrap()
        .entry(id.to_string())
        .or_default()
        .push(TerminalLogEntry {
            input,
            output: clean.clone(),
        });
    if !result.ok {
        return Err(result.output);
    }
    Ok(clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cwd_marker_parse() {
        let output = format!("some output\n{}/E:/ws", CWD_MARKER);
        let cwd = output
            .lines()
            .rev()
            .find_map(|l| l.split_once(CWD_MARKER).map(|(_m, p)| p.trim().to_string()));
        assert_eq!(cwd.as_deref(), Some("/E:/ws"));
        let clean: Vec<&str> = output.lines().filter(|l| !l.contains(CWD_MARKER)).collect();
        assert_eq!(clean, vec!["some output"]);
    }

    #[test]
    fn normalize_cwd_strips_provider_and_long_path_prefixes() {
        // PowerShell 路径规范化：剥离 FileSystem 提供者前缀与 \\?\ 前缀
        assert_eq!(
            normalize_cwd(r"Microsoft.PowerShell.Core\FileSystem::C:\Users\test"),
            r"C:\Users\test"
        );
        assert_eq!(
            normalize_cwd(r"\\?\C:\Program Files\app"),
            r"C:\Program Files\app"
        );
        // 组合：两者都出现
        assert_eq!(
            normalize_cwd(r"Microsoft.PowerShell.Core\FileSystem::\\?\D:\ws"),
            r"D:\ws"
        );
        // 普通路径原样
        assert_eq!(normalize_cwd(r"C:\plain"), r"C:\plain");
        // 标记行提取 + 规范化全链路（与 send_regular 的解析一致）
        let output = format!(
            "done\n{}Microsoft.PowerShell.Core\\FileSystem::C:\\new\\dir",
            CWD_MARKER
        );
        let cwd = output.lines().rev().find_map(|l| {
            l.split_once(CWD_MARKER)
                .map(|(_m, p)| normalize_cwd(p.trim()))
        });
        assert_eq!(cwd.as_deref(), Some(r"C:\new\dir"));
    }
}

/// 规范化 PowerShell 返回的路径：剥离 FileSystem 提供者前缀与 \\?\ 前缀
pub(crate) fn normalize_cwd(p: &str) -> String {
    let p = p
        .strip_prefix("Microsoft.PowerShell.Core\\FileSystem::")
        .unwrap_or(p);
    p.strip_prefix(r"\\?\").unwrap_or(p).to_string()
}

// ─── PTY 协作辅助（harness::pty 调用） ───

/// 读取终端会话的当前工作目录
pub(crate) fn session_cwd(id: &str) -> Result<String, String> {
    let list = sessions_store().lock().unwrap();
    list.iter()
        .find(|t| t.id == id)
        .map(|t| t.cwd.clone())
        .ok_or_else(|| "指定的终端会话不存在".to_string())
}

/// 更新终端会话工作目录（PTY 命令执行后定位）
pub(crate) fn update_cwd(id: &str, cwd: &str) {
    let mut list = sessions_store().lock().unwrap();
    if let Some(t) = list.iter_mut().find(|t| t.id == id) {
        t.cwd = cwd.to_string();
    }
    let _ = persist_sessions(&list);
}

/// 追加终端日志条目（PTY 与普通命令共用视图）
pub(crate) fn push_log(id: &str, input: String, output: String) {
    logs_store()
        .lock()
        .unwrap()
        .entry(id.to_string())
        .or_default()
        .push(TerminalLogEntry { input, output });
}

/// 终端日志快照（terminal_read 模型工具）
pub(crate) fn logs(id: &str) -> Vec<TerminalLogEntry> {
    logs_store()
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .unwrap_or_default()
}
