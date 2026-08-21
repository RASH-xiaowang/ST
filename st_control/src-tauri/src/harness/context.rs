// ============================================================
// Harness — 请求上下文（DSH context 迁移）
//
// 上下文提供者注册表：每个提供者从会话状态投影一段上下文块，
// 每轮组装进系统提示词（模型可见 ⟺ 落日志：全部来源均为日志
// 投影状态，可重建）。
// 默认提供者：会话目标 / 计划模式 / 待办列表摘要。
// ============================================================

use std::sync::{Mutex, OnceLock};

/// 上下文块
pub struct ContextBlock {
    pub title: String,
    pub content: String,
}

/// 提供者：从会话状态投影上下文块（None = 不贡献）
type Provider = fn(&crate::harness::session::SessionState) -> Option<ContextBlock>;

fn providers() -> &'static Mutex<Vec<Provider>> {
    static P: OnceLock<Mutex<Vec<Provider>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(vec![default_provider]))
}

/// 注册上下文提供者（扩展接入点）
#[allow(dead_code)]
pub fn add_provider(p: Provider) {
    providers().lock().unwrap().push(p);
}

/// 组装全部上下文块（默认提供者 + 扩展）
pub fn assemble(state: &crate::harness::session::SessionState) -> String {
    let providers = providers().lock().unwrap().clone();
    let mut out: Vec<String> = Vec::new();
    for p in providers {
        if let Some(block) = p(state) {
            out.push(format!("[{}]\n{}", block.title, block.content));
        }
    }
    out.join("\n\n")
}

/// 默认提供者：目标 / 计划模式 / 待办（日志投影状态）
fn default_provider(state: &crate::harness::session::SessionState) -> Option<ContextBlock> {
    let mut parts: Vec<String> = Vec::new();
    if !state.goal.is_empty() {
        parts.push(format!("当前会话目标：{}", state.goal));
    }
    if state.plan_mode {
        let plan = if state.plan_text.is_empty() {
            String::new()
        } else {
            format!("（方案：{}）", state.plan_text)
        };
        parts.push(format!("当前处于计划模式{}：仅只读工具可用。", plan));
    }
    if !state.todos.is_empty() {
        let todo_text = state
            .todos
            .iter()
            .map(|t| format!("- [{}] {}", t.status, t.content))
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!("当前待办列表：\n{}", todo_text));
    }
    if parts.is_empty() {
        None
    } else {
        Some(ContextBlock {
            title: "session-context".to_string(),
            content: parts.join("\n"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_assembles_from_state() {
        let state = crate::harness::session::SessionState {
            plan_mode: true,
            plan_text: String::new(),
            goal: "测试目标".into(),
            goal_status: "active".into(),
            goal_revision: 1,
            goal_blocked_reason: String::new(),
            goal_max_rounds: None,
            todos: vec![crate::harness::session::TodoItem {
                id: "t1".into(),
                content: "事项".into(),
                status: "pending".into(),
            }],
        };
        let ctx = assemble(&state);
        assert!(ctx.contains("测试目标"));
        assert!(ctx.contains("计划模式"));
        assert!(ctx.contains("事项"));
    }

    #[test]
    fn empty_state_gives_no_context() {
        let ctx = assemble(&crate::harness::session::SessionState::default());
        assert!(ctx.is_empty());
    }

    #[test]
    fn custom_provider_injects_block() {
        // 可插拔提供者：add_provider 追加自定义提供者（DSH context
        // 注入语义）。验证注册表可插拔（+1 且恢复）；组装行为由
        // context_assembles_from_state 覆盖。
        let before = providers().lock().unwrap().len();
        add_provider(|_s| {
            Some(ContextBlock {
                title: "custom".into(),
                content: "自定义指令".into(),
            })
        });
        {
            let mut list = providers().lock().unwrap();
            assert_eq!(list.len(), before + 1, "add_provider 应追加提供者");
            list.truncate(before);
        }
        // 恢复后：默认提供者仍在（其余测试依赖）
        assert!(providers().lock().unwrap().len() >= 1, "默认提供者应保留");
    }

    #[test]
    fn plan_mode_detail_and_goal_priority() {
        // 计划模式含方案文本；目标 + 计划 + 待办同时出现时全部注入
        let state = crate::harness::session::SessionState {
            plan_mode: true,
            plan_text: "先读后写".into(),
            goal: "目标A".into(),
            goal_status: "active".into(),
            goal_revision: 1,
            goal_blocked_reason: String::new(),
            goal_max_rounds: None,
            todos: vec![crate::harness::session::TodoItem {
                id: "t".into(),
                content: "事项B".into(),
                status: "in_progress".into(),
            }],
        };
        let ctx = assemble(&state);
        assert!(ctx.contains("先读后写"), "应含方案文本: {ctx}");
        assert!(ctx.contains("目标A") && ctx.contains("事项B"));
        assert!(ctx.contains("[in_progress]"), "待办应带状态: {ctx}");
    }
}
