// ============================================================
// 社交关系图谱 — 数据类型
// 自 insights.rs 拆分：节点/共同成员/边。
// ============================================================

use serde::Serialize;

/// 关系图节点
#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    /// self / contact / group / official
    pub kind: String,
    pub msg_count: i64,
    pub active_days: i64,
    pub last_ts: i64,
    /// 群成员数（仅群聊）
    pub member_count: i64,
    /// 与我共同的群数（联系人节点）
    #[serde(default)]
    pub group_count: usize,
    /// 与我共同的群 code 列表（联系人节点；前端据此推导共同群边 / 群共现边）
    #[serde(default)]
    pub group_codes: Vec<String>,
    /// 是否为好友（联系人节点）
    #[serde(default)]
    pub is_friend: bool,
    /// 该群命中的已选联系人数量（群节点）
    #[serde(default)]
    pub shared_count: i64,
    /// 头像 URL（远程）
    #[serde(default)]
    pub avatar_url: String,
    /// 该群命中的已选联系人明细（群节点；按消息量取前 N 名）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_members: Vec<SharedMember>,
}

/// 群节点中的共同成员明细（供「群聊网络」详情与榜单展示）
#[derive(Debug, Clone, Serialize)]
pub struct SharedMember {
    pub username: String,
    pub name: String,
    pub is_friend: bool,
    pub msg_count: i64,
}

/// 关系图边
#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub weight: i64,
    pub msg_count: i64,
    pub active_days: i64,
    pub last_ts: i64,
    /// 关系来源：message / group
    pub kinds: Vec<String>,
}
