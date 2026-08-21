// ============================================================
// Harness — 交互能力（DSH interaction 迁移）
//
// 审批门控：危险工具执行前提交审批请求，经 tauri 事件
// `harness-approval-requested` 推前端；用户批准/拒绝/超时。
// 「会话内记住批准」：信任键 (session_id, tool, 参数指纹)，TTL 30 分钟，
// 期间同会话同工具**相同参数**不再弹审批（M8：仅完全相同参数免审，
// 防止「记住一次 → 任意命令免审批执行」）；清空信任随删除会话联动。
// ============================================================

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::Emitter;

struct Pending {
    /// pending / approved / rejected
    status: String,
}

fn pendings() -> &'static Mutex<HashMap<String, Pending>> {
    static P: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

const TRUST_TTL_SECS: u64 = 1800;

/// 信任键：(session_id, tool, 参数指纹)
/// 参数指纹 = 规范化 JSON（serde_json Map 默认 BTreeMap，键已排序）的 sha256——
/// 同参数恒定；信任键不驻留原始参数内容（避免内存保留敏感载荷）。
type TrustKey = (String, String, String);

fn trusted() -> &'static Mutex<HashMap<TrustKey, Instant>> {
    static T: OnceLock<Mutex<HashMap<TrustKey, Instant>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 计算参数指纹（M8：信任键含参数，仅相同参数的命令免审批）。
/// 精华参数：exec_command 只取 command/cwd（模型可能附加 justification 等
/// 说明字段，不应破坏「同命令免审」语义）；其余工具取全量规范 JSON
/// （serde_json Map 默认 BTreeMap，键已排序，键序无关）。
fn args_fingerprint(tool: &str, args: &Value) -> String {
    use sha2::Digest;
    let essential: Value = if tool == "exec_command" || tool == "exec_command#danger-full-access" {
        let mut m = serde_json::Map::new();
        if let Some(c) = args.get("command") {
            m.insert("command".to_string(), c.clone());
        }
        if let Some(c) = args.get("cwd") {
            m.insert("cwd".to_string(), c.clone());
        }
        Value::Object(m)
    } else {
        args.clone()
    };
    let mut h = sha2::Sha256::new();
    h.update(essential.to_string().as_bytes());
    format!("{:x}", h.finalize())
}

fn is_trusted(session_id: &str, tool: &str, args: &Value) -> bool {
    let now = Instant::now();
    let fp = args_fingerprint(tool, args);
    let mut m = trusted().lock().unwrap();
    m.retain(|_, t| now.duration_since(*t).as_secs() < TRUST_TTL_SECS);
    m.contains_key(&(session_id.to_string(), tool.to_string(), fp))
}

/// 审批请求（最长 10 分钟）；信任命中直接放行。
/// 返回 Err 即被拒绝/超时，由工具管道转为失败结果。
pub async fn request_approval(
    app: &tauri::AppHandle,
    session_id: &str,
    tool: &str,
    args: &Value,
) -> Result<(), String> {
    if is_trusted(session_id, tool, args) {
        return Ok(());
    }
    let id = format!("hapr-{}", uuid::Uuid::new_v4().simple());
    let description = format!(
        "工具「{}」需要你的批准：{}",
        tool,
        crate::llm::agent::truncate_str(&args.to_string(), 200)
    );
    pendings().lock().unwrap().insert(
        id.clone(),
        Pending {
            status: "pending".to_string(),
        },
    );
    let _ = app.emit(
        "harness-approval-requested",
        json!({
            "id": id,
            "session_id": session_id,
            "tool": tool,
            "description": description,
            "arguments": args.to_string(),
        }),
    );

    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        // 用户「停止」回合：立即退出等待（M3：不能等到审批超时才返回）
        if crate::harness::agent::is_cancelled(session_id) {
            pendings().lock().unwrap().remove(&id);
            return Err("回合已停止，审批取消".to_string());
        }
        // 「记住并批准」可在弹窗出现后随时生效（同参数指纹才放行）
        if is_trusted(session_id, tool, args) {
            pendings().lock().unwrap().remove(&id);
            return Ok(());
        }
        let status = pendings()
            .lock()
            .unwrap()
            .get(&id)
            .map(|p| p.status.clone())
            .unwrap_or_else(|| "cancelled".to_string());
        match status.as_str() {
            "approved" => {
                pendings().lock().unwrap().remove(&id);
                return Ok(());
            }
            "rejected" => {
                pendings().lock().unwrap().remove(&id);
                return Err("用户拒绝了该操作".to_string());
            }
            _ => {}
        }
        if Instant::now() > deadline {
            pendings().lock().unwrap().remove(&id);
            return Err("审批超时（10 分钟）".to_string());
        }
    }
}

fn set_status(id: &str, status: &str) -> bool {
    let mut m = pendings().lock().unwrap();
    if let Some(p) = m.get_mut(id) {
        if p.status == "pending" {
            p.status = status.to_string();
            return true;
        }
    }
    false
}

/// 批准一个待审批的工具调用
#[tauri::command]
pub async fn approve_harness_tool(id: String) -> Result<bool, String> {
    Ok(set_status(&id, "approved"))
}

/// 拒绝一个待审批的工具调用
#[tauri::command]
pub async fn reject_harness_tool(id: String) -> Result<bool, String> {
    Ok(set_status(&id, "rejected"))
}

/// 会话内记住批准：同 (session, tool, 参数指纹) 有效期内不再弹审批
/// （M8：arguments 参与指纹，仅完全相同参数的命令免审）
#[tauri::command]
pub async fn trust_harness_tool(
    session_id: String,
    tool: String,
    arguments: String,
) -> Result<(), String> {
    if tool.trim().is_empty() {
        return Err("工具名不能为空".to_string());
    }
    let args: Value = serde_json::from_str(&arguments).unwrap_or_else(|_| json!({}));
    let fp = args_fingerprint(&tool, &args);
    trusted()
        .lock()
        .unwrap()
        .insert((session_id, tool, fp), Instant::now());
    Ok(())
}

/// 清空某会话的信任记录（删除会话时联动）
pub fn clear_trust_for_session(session_id: &str) {
    trusted()
        .lock()
        .unwrap()
        .retain(|(sid, _, _), _| sid != session_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_scoped_per_session_and_args() {
        let sid = format!("t-{}", uuid::Uuid::new_v4().simple());
        let args1 = json!({ "command": "echo hello" });
        let args2 = json!({ "command": "echo hello", "justification": "用户要求" });
        let args3 = json!({ "command": "echo world" });
        trusted().lock().unwrap().insert(
            (
                sid.clone(),
                "exec_command".to_string(),
                args_fingerprint("exec_command", &args1),
            ),
            Instant::now(),
        );
        // 同命令命中（附加 justification 等说明字段不破坏「同命令免审」）
        assert!(is_trusted(&sid, "exec_command", &args2));
        // 不同命令不命中（M8：参数指纹参与信任键）
        assert!(!is_trusted(&sid, "exec_command", &args3));
        assert!(!is_trusted("other-session", "exec_command", &args1));
        clear_trust_for_session(&sid);
        assert!(!is_trusted(&sid, "exec_command", &args1));
    }

    #[test]
    fn fingerprint_stable_regardless_of_key_order() {
        let a = json!({ "command": "x", "cwd": "d" });
        let b = json!({ "cwd": "d", "command": "x" });
        assert_eq!(
            args_fingerprint("exec_command", &a),
            args_fingerprint("exec_command", &b)
        );
        assert_ne!(
            args_fingerprint("exec_command", &json!({ "command": "x" })),
            args_fingerprint("exec_command", &json!({ "command": "y" }))
        );
        // 非 exec 工具：全量参数参与（附加字段改变指纹）
        assert_ne!(
            args_fingerprint("write_file", &json!({ "path": "a", "content": "x" })),
            args_fingerprint("write_file", &json!({ "path": "a", "content": "y" }))
        );
    }

    #[test]
    fn trust_expires_after_ttl() {
        // 信任 TTL 30 分钟：过期条目在 is_trusted 惰性清理后不再命中
        let sid = format!("t-{}", uuid::Uuid::new_v4().simple());
        let args = json!({ "command": "echo ttl" });
        trusted().lock().unwrap().insert(
            (
                sid.clone(),
                "exec_command".to_string(),
                args_fingerprint("exec_command", &args),
            ),
            // 已过期（TTL + 1s 之前）
            Instant::now() - Duration::from_secs(TRUST_TTL_SECS + 1),
        );
        // is_trusted 先 retain 清理过期项 → 不再命中
        assert!(!is_trusted(&sid, "exec_command", &args));
        // 清理后信任表应已移除该键
        assert!(!trusted().lock().unwrap().contains_key(&(
            sid.clone(),
            "exec_command".to_string(),
            args_fingerprint("exec_command", &args)
        )));
        clear_trust_for_session(&sid);
    }

    #[test]
    fn set_status_transitions_pending_only() {
        // 审批状态机：仅 pending 可转换；已批准/已拒绝/不存在 → false
        let id = format!("ap-{}", uuid::Uuid::new_v4().simple());
        pendings().lock().unwrap().insert(
            id.clone(),
            Pending {
                status: "pending".into(),
            },
        );
        // pending → approved
        assert!(set_status(&id, "approved"));
        assert_eq!(
            pendings().lock().unwrap().get(&id).unwrap().status,
            "approved"
        );
        // 已批准不可再转换（如重复批准/拒绝）
        assert!(!set_status(&id, "rejected"));
        assert_eq!(
            pendings().lock().unwrap().get(&id).unwrap().status,
            "approved"
        );
        // 新条目 pending → rejected
        let id2 = format!("ap-{}", uuid::Uuid::new_v4().simple());
        pendings().lock().unwrap().insert(
            id2.clone(),
            Pending {
                status: "pending".into(),
            },
        );
        assert!(set_status(&id2, "rejected"));
        assert_eq!(
            pendings().lock().unwrap().get(&id2).unwrap().status,
            "rejected"
        );
        // 不存在的 id → false
        assert!(!set_status("no-such-id", "approved"));
        // 清理
        pendings().lock().unwrap().remove(&id);
        pendings().lock().unwrap().remove(&id2);
    }
}
