// ============================================================
// Harness — 代理指令上下文（DSH context/agent-instructions 迁移）
//
// 每个回合开始扫描工作区（data/agent_workspace）内的 AGENTS.md /
// CLAUDE.md（含子目录，深度 ≤ 5），把内容作为 <system-reminder>
// 分区注入系统提示词（预算封顶）；文件删除后下次扫描自动移除。
// 模型可经 session_ref 工具引用其他会话的投影快照（DSH
// context/session-reference 等价：跨会话引用）。
// ============================================================

use std::sync::{Mutex, OnceLock};

/// 发现的指令文件（相对工作区路径 → 内容）
type Instructions = Vec<(String, String)>;

fn store() -> &'static Mutex<Instructions> {
    static S: OnceLock<Mutex<Instructions>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Vec::new()))
}

/// 注入预算：总字符数上限
const INJECT_BUDGET_CHARS: usize = 24 * 1024;
/// 单文件读取上限
const FILE_CAP_CHARS: usize = 32 * 1024;
/// 扫描深度上限
const SCAN_DEPTH: usize = 5;
/// 文件数上限
const FILE_COUNT_CAP: usize = 16;

const INSTRUCTION_NAMES: [&str; 2] = ["AGENTS.md", "CLAUDE.md"];

/// 重新扫描工作区指令文件（每回合开始调用一次；跟随当前工作区，
/// 默认工作区 = 应用项目根：自维护时读取项目级 AGENTS.md/CLAUDE.md）。
/// 返回发现的指令文件相对路径列表（模型可见 ⟺ 落日志：注入来源随事件持久化）
pub fn rescan() -> Vec<String> {
    let root = crate::harness::workspace::sandbox_root();
    let mut found: Instructions = Vec::new();
    let mut stack: Vec<(std::path::PathBuf, usize)> = vec![(root.clone(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > SCAN_DEPTH || found.len() >= FILE_COUNT_CAP {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                if name.starts_with('.') {
                    continue; // 跳过隐藏目录（如 .git/.svn）
                }
                stack.push((e.path(), depth + 1));
                continue;
            }
            // 大小写不敏感匹配（Windows 语义）
            if !INSTRUCTION_NAMES
                .iter()
                .any(|n| name.eq_ignore_ascii_case(n))
            {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(e.path()) else {
                continue;
            };
            let rel = e
                .path()
                .strip_prefix(&root)
                .unwrap_or_else(|_| std::path::Path::new(&name))
                .to_string_lossy()
                .replace('\\', "/");
            found.push((rel, text));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    let files: Vec<String> = found.iter().map(|(rel, _)| rel.clone()).collect();
    *store().lock().unwrap() = found;
    files
}

/// 组装注入分区（预算内；超预算按序截断）
pub fn inject() -> String {
    let items = store().lock().unwrap().clone();
    if items.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut budget = INJECT_BUDGET_CHARS;
    for (rel, text) in items {
        let text: String = text.chars().take(FILE_CAP_CHARS).collect();
        let section = format!("<system-reminder>\n文件：{rel}\n{text}\n</system-reminder>\n");
        if section.len() > budget {
            // 预算用尽：截断本段并停止
            out.push_str(&section.chars().take(budget).collect::<String>());
            break;
        }
        budget -= section.len();
        out.push_str(&section);
    }
    out
}

/// 跨会话引用快照（DSH session-reference 等价）：
/// 读取目标会话的展示投影，压缩为引用文本
pub fn session_ref(session_id: &str, max_chars: usize) -> Result<String, String> {
    let store =
        crate::harness::registry::get::<crate::harness::session::SessionStore>("harness.sessions")
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    let msgs = store.derive_display_messages(session_id)?;
    let mut text = String::new();
    for m in msgs {
        let role = match m {
            crate::harness::session::DisplayMessage::User { .. } => "用户",
            crate::harness::session::DisplayMessage::Assistant { .. } => "助手",
            crate::harness::session::DisplayMessage::MetaLine { .. } => "会话",
        };
        let content = match &m {
            crate::harness::session::DisplayMessage::User { content, .. } => content,
            crate::harness::session::DisplayMessage::Assistant { content, .. } => content,
            crate::harness::session::DisplayMessage::MetaLine { title, .. } => title,
        };
        text.push_str(&format!("[{role}] {content}\n"));
    }
    let cap = max_chars.clamp(512, 8192);
    Ok(if text.chars().count() > cap {
        format!(
            "{}…（引用已截断，共 {} 字符）",
            text.chars().take(cap).collect::<String>(),
            text.chars().count()
        )
    } else {
        text
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rescan_finds_and_injects_instructions() {
        let root = crate::llm::agent::workspace_root();
        std::fs::create_dir_all(root.join("ins-test/sub")).unwrap();
        std::fs::write(root.join("AGENTS.md"), "# 根指令\n根内容").unwrap();
        std::fs::write(root.join("ins-test/sub/AGENTS.md"), "# 子指令\n子内容").unwrap();
        let files = rescan();
        assert!(files.len() >= 2);
        assert!(files.iter().any(|f| f.contains("AGENTS.md")));
        let injected = inject();
        assert!(injected.contains("system-reminder"));
        assert!(injected.contains("根内容") && injected.contains("子内容"));
        // 删除后重扫移除
        std::fs::remove_file(root.join("AGENTS.md")).unwrap();
        std::fs::remove_dir_all(root.join("ins-test")).unwrap();
        let n2 = rescan();
        assert!(n2.len() < files.len());
    }

    #[test]
    fn inject_empty_when_none() {
        {
            *store().lock().unwrap() = Vec::new();
        }
        assert_eq!(inject(), "");
    }

    #[test]
    fn inject_budget_truncates_and_stops() {
        // 注入预算封顶：累计超预算后按序截断并停止（与 inject 分支一致）
        // 直接写 store 构造数据（不经 rescan，避免依赖工作区文件）
        let big = "x".repeat(INJECT_BUDGET_CHARS + 100);
        {
            let mut s = store().lock().unwrap();
            s.clear();
            s.push(("a.md".into(), big.clone()));
            s.push(("b.md".into(), "后续文件内容".into()));
        }
        let out = inject();
        // 第一段超预算：注入被截断且第二段完全未注入
        assert!(
            out.contains("a.md"),
            "应含第一段: {:?}",
            &out[..out.len().min(80)]
        );
        assert!(
            !out.contains("b.md"),
            "预算用尽后应停止: {:?}",
            &out[..out.len().min(120)]
        );
        assert!(out.len() <= INJECT_BUDGET_CHARS + 64, "注入不应超预算太多");
        // 清理
        store().lock().unwrap().clear();
    }

    #[test]
    fn file_cap_limits_single_file() {
        // 单文件读取上限：内容按字符截断到 FILE_CAP_CHARS
        let big = "z".repeat(FILE_CAP_CHARS + 50);
        {
            let mut s = store().lock().unwrap();
            s.clear();
            s.push(("cap.md".into(), big));
        }
        let out = inject();
        assert!(out.contains("cap.md"));
        assert!(
            out.len() <= FILE_CAP_CHARS + 200,
            "单文件内容应被截断: {}",
            out.len()
        );
        store().lock().unwrap().clear();
    }
}
