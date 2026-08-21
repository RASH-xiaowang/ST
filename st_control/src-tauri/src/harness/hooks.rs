// ============================================================
// Harness — 外部钩子桥（DSH hooks 迁移）
//
// 事件钩子：turn_start / turn_end / tool_executed 三类事件触发用户
// 配置的命令（PowerShell），异步执行不阻塞会话循环：
// - 环境变量注入：HARNESS_EVENT / HARNESS_SESSION
// - stdin 传入 JSON 载荷（tool_executed 含工具名/参数/结果摘要）
// - 单次执行上限 10 秒（超时放弃）；结果经 tauri 事件
//   `harness-hook-fired` 回传前端展示
// 持久化：data/harness/hooks.json（原子写）。
// ============================================================

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use tauri::Emitter;

/// 钩子事件类型（白名单；含 Claude Code / Codex 方言事件）
pub const HOOK_EVENTS: [&str; 10] = [
    "turn_start",
    "turn_end",
    "tool_executed",
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "SubagentStart",
    "SubagentStop",
];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HarnessHook {
    pub id: String,
    /// turn_start / turn_end / tool_executed / SessionStart / UserPromptSubmit /
    /// PreToolUse / PostToolUse / Stop / SubagentStart / SubagentStop
    pub event: String,
    /// 匹配器（CC 方言）：空 = 全部命中；非空 = 载荷 JSON 文本包含该子串才触发
    #[serde(default)]
    pub matcher: String,
    pub command: String,
    pub enabled: bool,
}

fn hooks_path() -> std::path::PathBuf {
    crate::common::st_data_dir()
        .join("harness")
        .join("hooks.json")
}

pub(crate) fn hooks_store() -> &'static Mutex<Vec<HarnessHook>> {
    static H: OnceLock<Mutex<Vec<HarnessHook>>> = OnceLock::new();
    H.get_or_init(|| {
        let list = std::fs::read_to_string(hooks_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Mutex::new(list)
    })
}

pub(crate) fn persist(list: &[HarnessHook]) -> Result<(), String> {
    let path = hooks_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建钩子目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(list).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换失败: {}", e))?;
    Ok(())
}

/// 触发事件：异步执行全部匹配的启用钩子，结果经 tauri 事件回传
pub fn fire(app: &tauri::AppHandle, event: &str, session_id: &str, payload: serde_json::Value) {
    let hooks: Vec<HarnessHook> = hooks_store()
        .lock()
        .unwrap()
        .iter()
        .filter(|h| h.enabled && h.event == event && !h.command.trim().is_empty())
        .filter(|h| matcher_matches(&h.matcher, &payload))
        .cloned()
        .collect();
    for hook in hooks {
        let app2 = app.clone();
        let session2 = session_id.to_string();
        let payload2 = payload.clone();
        let event2 = event.to_string();
        tauri::async_runtime::spawn(async move {
            let out = run_hook(&hook, &event2, &session2, &payload2).await;
            let (ok, output) = match out {
                Ok(t) => (true, t),
                Err(e) => (false, e),
            };
            let _ = app2.emit(
                "harness-hook-fired",
                json!({
                    "id": hook.id,
                    "event": event2,
                    "ok": ok,
                    "output": crate::llm::agent::truncate_str(&output, 500),
                }),
            );
        });
    }
}

/// 匹配器：空 = 全部命中；非空 = 载荷 JSON 文本包含该子串
fn matcher_matches(matcher: &str, payload: &serde_json::Value) -> bool {
    matcher.trim().is_empty() || payload.to_string().contains(matcher)
}

/// 决策钩子（CC/Codex 方言 PreToolUse 等）：同步执行匹配钩子并解析
/// 钩子 stdout 的 JSON 决策 {"decision":"deny"|"ask","reason":...}。
/// 返回 (decision, reason)；无钩子命中或无有效决策 → None。
pub async fn fire_decision(
    app: &tauri::AppHandle,
    event: &str,
    session_id: &str,
    payload: serde_json::Value,
) -> Option<(String, String)> {
    let hooks: Vec<HarnessHook> = hooks_store()
        .lock()
        .unwrap()
        .iter()
        .filter(|h| h.enabled && h.event == event && !h.command.trim().is_empty())
        .filter(|h| matcher_matches(&h.matcher, &payload))
        .cloned()
        .collect();
    for hook in hooks {
        let out = run_hook(&hook, event, session_id, &payload).await;
        let text = match out {
            Ok(t) => t,
            Err(e) => {
                let _ = app.emit(
                    "harness-hook-fired",
                    json!({ "id": hook.id, "event": event, "ok": false, "output": e }),
                );
                continue;
            }
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
            if let Some(decision) = v.get("decision").and_then(|d| d.as_str()) {
                let reason = v
                    .get("reason")
                    .and_then(|r| r.as_str())
                    .unwrap_or("")
                    .to_string();
                let _ = app.emit(
                    "harness-hook-fired",
                    json!({ "id": hook.id, "event": event, "ok": true, "output": text }),
                );
                return Some((decision.to_string(), reason));
            }
        }
    }
    None
}

/// 执行单个钩子：stdin 传 JSON 载荷，上限 10 秒
async fn run_hook(
    hook: &HarnessHook,
    event: &str,
    session_id: &str,
    payload: &serde_json::Value,
) -> Result<String, String> {
    let command = hook.command.clone();
    let session = session_id.to_string();
    let payload_json = payload.to_string();
    let event = event.to_string();
    let fut = tauri::async_runtime::spawn_blocking(move || {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut cmd = Command::new("powershell.exe");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &command])
            .env("HARNESS_EVENT", &event)
            .env("HARNESS_SESSION", &session)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // 凭据注入（credentials：HARNESS_CREDENTIAL_<KEY>）
        crate::harness::credentials::inject_env(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("启动钩子命令失败: {}", e))?;
        {
            let mut stdin = child.stdin.take().ok_or("无法打开钩子 stdin")?;
            let _ = stdin.write_all(payload_json.as_bytes());
        }
        let mut out = String::new();
        let mut err = String::new();
        if let Some(mut so) = child.stdout.take() {
            use std::io::Read;
            let mut buf = Vec::new();
            let _ = so.read_to_end(&mut buf);
            out = String::from_utf8_lossy(&buf).into_owned();
        }
        if let Some(mut se) = child.stderr.take() {
            use std::io::Read;
            let mut buf = Vec::new();
            let _ = se.read_to_end(&mut buf);
            err = String::from_utf8_lossy(&buf).into_owned();
        }
        let _ = child.wait();
        if !err.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("[stderr] ");
            out.push_str(&err);
        }
        Ok::<String, String>(out)
    });
    match tokio::time::timeout(std::time::Duration::from_secs(10), fut).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => Err(format!("钩子执行异常: {}", e)),
        Err(_) => Err("钩子执行超时（10 秒），已放弃等待".to_string()),
    }
}

// ─── IPC ───

#[tauri::command]
pub async fn list_harness_hooks() -> Result<Vec<HarnessHook>, String> {
    Ok(hooks_store().lock().unwrap().clone())
}

/// 全量保存钩子列表（校验事件白名单与命令非空）
#[tauri::command]
pub async fn save_harness_hooks(hooks: Vec<HarnessHook>) -> Result<Vec<HarnessHook>, String> {
    for h in &hooks {
        if !HOOK_EVENTS.contains(&h.event.as_str()) {
            return Err(format!("不支持的钩子事件: {}", h.event));
        }
        if h.enabled && h.command.trim().is_empty() {
            return Err("启用的钩子命令不能为空".to_string());
        }
    }
    {
        let mut store = hooks_store().lock().unwrap();
        *store = hooks.clone();
    }
    persist(&hooks)?;
    Ok(hooks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_events_whitelist() {
        assert!(HOOK_EVENTS.contains(&"turn_start"));
        assert!(HOOK_EVENTS.contains(&"turn_end"));
        assert!(HOOK_EVENTS.contains(&"tool_executed"));
        assert!(!HOOK_EVENTS.contains(&"nope"));
    }

    #[test]
    fn matcher_empty_or_substring_match() {
        // 空 matcher = 全部命中；非空 = 载荷 JSON 文本包含子串才命中
        let payload = json!({ "tool": "exec_command", "command": "dir" });
        assert!(matcher_matches("", &payload), "空 matcher 应全部命中");
        assert!(
            matcher_matches("exec_command", &payload),
            "包含 exec_command 应命中"
        );
        assert!(
            matcher_matches("   ", &payload),
            "空白 matcher 视同空应命中"
        );
        assert!(
            !matcher_matches("write_file", &payload),
            "不含 write_file 应不命中"
        );
    }

    #[test]
    fn fire_decision_parses_deny_ask_ignores_invalid() {
        // 决策解析纯逻辑：从钩子 stdout 提取 {"decision":..., "reason":...}
        fn parse(text: &str) -> Option<(String, String)> {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            let v: serde_json::Value = serde_json::from_str(text).ok()?;
            let decision = v.get("decision").and_then(|d| d.as_str())?;
            let reason = v
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            Some((decision.to_string(), reason))
        }
        // deny + reason
        assert_eq!(
            parse(r#"{"decision":"deny","reason":"危险命令"}"#),
            Some(("deny".to_string(), "危险命令".to_string()))
        );
        // ask 无 reason → 空
        assert_eq!(
            parse(r#"{"decision":"ask"}"#),
            Some(("ask".to_string(), String::new()))
        );
        // 无 decision 字段 / 非 JSON / 空 → None（fire_decision 继续下个钩子）
        assert_eq!(parse(r#"{"note":"hi"}"#), None);
        assert_eq!(parse("not json"), None);
        assert_eq!(parse("  "), None);
    }
}
