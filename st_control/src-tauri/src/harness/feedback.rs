// ============================================================
// Harness — 反馈能力（DSH feedback 迁移）
//
// 会话反馈：好/差评 + 可选评论（SQLite 持久化）。
// 供后续会话质量评估与回放优化。
// ============================================================

use serde::{Deserialize, Serialize};

/// 反馈记录
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FeedbackRecord {
    pub id: i64,
    pub session_id: String,
    pub rating: String,
    pub comment: String,
    /// 助手消息序号（消息级反馈；None = 会话级）
    pub message_seq: Option<i64>,
    pub created_at: String,
}

/// 存储服务：经 SessionStore 同一数据库
pub fn submit(
    session_id: &str,
    rating: &str,
    comment: &str,
    message_seq: Option<i64>,
) -> Result<(), String> {
    let store =
        crate::harness::registry::get::<crate::harness::session::SessionStore>("harness.sessions")
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    store.submit_feedback(session_id, rating, comment, message_seq)
}

pub fn list() -> Result<Vec<FeedbackRecord>, String> {
    let store =
        crate::harness::registry::get::<crate::harness::session::SessionStore>("harness.sessions")
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    store.list_feedback()
}

#[tauri::command]
pub async fn harness_submit_feedback(
    session_id: String,
    rating: String,
    comment: Option<String>,
    message_seq: Option<i64>,
) -> Result<(), String> {
    let rating = rating.trim().to_string();
    if !matches!(rating.as_str(), "good" | "bad" | "") {
        return Err("评分只能是 good 或 bad".to_string());
    }
    submit(
        &session_id,
        &rating,
        comment.unwrap_or_default().trim(),
        message_seq,
    )
}

#[tauri::command]
pub async fn harness_list_feedback() -> Result<Vec<FeedbackRecord>, String> {
    list()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 评分校验：仅 good / bad / 空 合法（命令入口行为）
    #[tokio::test]
    async fn rating_validation_good_bad_only() {
        let ok = harness_submit_feedback("h-test".into(), "good".into(), None, None).await;
        // 命令入口先校验评分再触碰运行时（单测环境无注册表）：
        // 合法评分应走到「运行时未初始化」，非法评分应在校验层即拒绝
        let invalid = harness_submit_feedback("h-test".into(), "meh".into(), None, None)
            .await
            .unwrap_err();
        assert!(
            invalid.contains("只能是 good 或 bad"),
            "非法评分应被校验拒绝: {invalid}"
        );
        // 合法评分（good）不被校验拒绝——单测环境应报运行时未初始化
        let err = ok.unwrap_err();
        assert!(
            err.contains("未初始化"),
            "合法评分应越过校验、止于运行时检查: {err}"
        );
    }
}
