// ============================================================
// Harness — 用户提问接缝（DSH interaction/user-questions 迁移）
//
// ask_user_question 模型工具：把问题（可带选项）推给用户，
// 等待回答（最长 10 分钟）后把答案文本返回给模型。
// 前端经 tauri 事件 harness-question-requested 渲染问题卡，
// 用户选择选项或输入文本后经 harness_answer_question 应答。
// ============================================================

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::Emitter;

#[derive(Clone)]
struct PendingQuestion {
    /// pending / answered
    status: String,
    answer: String,
}

fn pendings() -> &'static Mutex<HashMap<String, PendingQuestion>> {
    static P: OnceLock<Mutex<HashMap<String, PendingQuestion>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 向用户提问并等待回答（最长 10 分钟）；返回用户给出的答案文本。
/// multi_select = 允许勾选多个选项（前端渲染复选框，答案以「, 」拼接）
pub async fn ask_user(
    app: &tauri::AppHandle,
    session_id: &str,
    question: &str,
    options: &[String],
    multi_select: bool,
) -> Result<String, String> {
    let id = format!("hq-{}", uuid::Uuid::new_v4().simple());
    pendings().lock().unwrap().insert(
        id.clone(),
        PendingQuestion {
            status: "pending".to_string(),
            answer: String::new(),
        },
    );
    let _ = app.emit(
        "harness-question-requested",
        json!({
            "id": id,
            "session_id": session_id,
            "question": question,
            "options": options,
            "multi_select": multi_select,
        }),
    );
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        // 用户「停止」回合：立即退出等待（M3：不等到 10 分钟超时）
        if crate::harness::agent::is_cancelled(session_id) {
            pendings().lock().unwrap().remove(&id);
            return Err("回合已停止，提问取消".to_string());
        }
        let (status, answer) = {
            let m = pendings().lock().unwrap();
            match m.get(&id) {
                Some(p) => (p.status.clone(), p.answer.clone()),
                None => ("cancelled".to_string(), String::new()),
            }
        };
        match status.as_str() {
            "answered" => {
                pendings().lock().unwrap().remove(&id);
                return Ok(answer);
            }
            "cancelled" => {
                return Err("提问已被取消".to_string());
            }
            _ => {}
        }
        if Instant::now() > deadline {
            pendings().lock().unwrap().remove(&id);
            return Err("提问超时（10 分钟）".to_string());
        }
    }
}

/// 用户回答（前端问题卡）
#[tauri::command]
pub async fn harness_answer_question(id: String, answer: String) -> Result<bool, String> {
    let mut m = pendings().lock().unwrap();
    if let Some(p) = m.get_mut(&id) {
        if p.status == "pending" {
            p.status = "answered".to_string();
            p.answer = answer.trim().to_string();
            return Ok(true);
        }
    }
    Ok(false)
}

/// 取消提问（会话删除时联动）
pub fn cancel_session_questions(session_id: &str) {
    // pendings 不按会话索引；这里仅占位说明：提问按 id 应答，
    // 会话删除不影响等待（超时兜底）。保留函数供后续按会话索引扩展。
    let _ = session_id;
}

/// 供模型工具调用（args 兼容 Value）
pub fn options_from_args(args: &Value) -> Vec<String> {
    args.get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|o| o.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_from_args_parses_and_filters() {
        let args = json!({ "options": ["A", 42, "B", null] });
        assert_eq!(
            options_from_args(&args),
            vec!["A".to_string(), "B".to_string()]
        );
        assert!(options_from_args(&json!({})).is_empty());
        assert!(options_from_args(&json!({ "options": [] })).is_empty());
    }

    #[tokio::test]
    async fn answer_question_transitions_pending_to_answered() {
        let id = format!("hq-test-{}", uuid::Uuid::new_v4().simple());
        pendings().lock().unwrap().insert(
            id.clone(),
            PendingQuestion {
                status: "pending".into(),
                answer: String::new(),
            },
        );
        assert!(harness_answer_question(id.clone(), "  用户选择  ".into())
            .await
            .unwrap());
        let p = pendings().lock().unwrap().get(&id).cloned().unwrap();
        assert_eq!(p.status, "answered");
        assert_eq!(p.answer, "用户选择", "答案应去首尾空白");
        // 已应答：重复回答返回 false
        assert!(!harness_answer_question(id.clone(), "again".into())
            .await
            .unwrap());
        pendings().lock().unwrap().remove(&id);
    }
}
