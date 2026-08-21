// ============================================================
// 年度总结 — 数据类型
// 自 annual.rs 拆分：瞬间/榜单/汇总结构。
// ============================================================

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MomentItem {
    pub ts: i64,
    pub time: String,
    pub date: String,
    pub username: String,
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopItem {
    pub key: String,
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnnualSummary {
    pub year: i32,
    pub total_messages: i64,
    pub text_messages: i64,
    pub active_days: i64,
    pub total_chars: i64,
    pub avg_chars: f64,
    pub kind_counts: Vec<serde_json::Value>,
    pub monthly_counts: Vec<i64>,
    pub heatmap: serde_json::Value,
    pub top_contacts: Vec<TopItem>,
    pub top_groups: Vec<TopItem>,
    pub top_phrases: Vec<TopItem>,
    pub top_emojis: Vec<TopItem>,
    pub earliest: Option<MomentItem>,
    pub latest: Option<MomentItem>,
}
