// ============================================================
// Harness — 技能能力（DSH skill 迁移）
//
// 技能 = 目录约定（data/harness/skills/<id>/SKILL.md）下的说明文档：
// - 注册表加载本地技能目录；IPC 列表/保存/删除
// - 模型工具 skill_list（可用技能目录）与 skill_load（读取技能内容）
// - frontmatter（--- 块）：name / description / disable-model-invocation
//   （DSH invocation 策略：禁模型调用时 skill_list 不展示、skill_load 拒绝）
// - /skill <id> 用户手势：内容注入下一回合系统提示词
// ============================================================

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    /// SKILL.md 内容（截断预览用）
    #[serde(default)]
    pub content: String,
    /// 是否允许模型调用（DSH invocation 策略）
    #[serde(default = "default_true")]
    pub model_invocable: bool,
}

fn default_true() -> bool {
    true
}

fn skills_dir() -> std::path::PathBuf {
    crate::common::st_data_dir().join("harness").join("skills")
}

fn skill_path(id: &str) -> std::path::PathBuf {
    skills_dir().join(id).join("SKILL.md")
}

pub(crate) fn skills_store() -> &'static Mutex<Vec<SkillInfo>> {
    static S: OnceLock<Mutex<Vec<SkillInfo>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(load_skills()))
}

/// 解析 frontmatter（--- 包裹的 name / description / disable-model-invocation），
/// 返回 (正文, 名称, 描述, 是否允许模型调用)。无 frontmatter 时按标题/首行推导。
fn parse_frontmatter(content: &str) -> (String, String, String, bool) {
    let Some(body) = content.strip_prefix("---\n") else {
        // 无 frontmatter：首行 # 为名称，次行为描述
        let name = content
            .lines()
            .find(|l| l.starts_with("# "))
            .map(|l| l.trim_start_matches("# ").trim().to_string())
            .unwrap_or_default();
        let description = content
            .lines()
            .skip(1)
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string();
        return (content.to_string(), name, description, true);
    };
    let Some(end) = body.find("\n---") else {
        return (content.to_string(), String::new(), String::new(), true);
    };
    let frontmatter = &body[..end];
    let rest = body[end + 4..].trim_start_matches('\n');
    let mut name = String::new();
    let mut description = String::new();
    let mut disable_model = false;
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("name:") {
            name = v.trim().trim_matches('"').to_string();
        } else if let Some(v) = line.strip_prefix("description:") {
            description = v.trim().trim_matches('"').to_string();
        } else if let Some(v) = line.strip_prefix("disable-model-invocation:") {
            disable_model = v.trim().eq_ignore_ascii_case("true");
        }
    }
    if name.is_empty() {
        name = rest
            .lines()
            .find(|l| l.starts_with("# "))
            .map(|l| l.trim_start_matches("# ").trim().to_string())
            .unwrap_or_default();
    }
    (rest.to_string(), name, description, !disable_model)
}

/// 从目录加载全部技能（目录约定：<id>/SKILL.md，支持 frontmatter）
fn load_skills() -> Vec<SkillInfo> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(skills_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        let Ok(content) = std::fs::read_to_string(skill_path(&id)) else {
            continue;
        };
        let (body, name, description, model_invocable) = parse_frontmatter(&content);
        out.push(SkillInfo {
            id,
            name,
            description,
            content: body,
            model_invocable,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// 保存技能（新建或覆盖 SKILL.md；元数据以磁盘内容解析为准）
/// DSH isSkillName 语义：技能 id 须为 kebab-case（^[a-z0-9]+(-[a-z0-9]+)*$），
/// 用作目录/文件名防路径注入
fn is_valid_skill_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut prev_dash = false;
    for &b in bytes {
        match b {
            b'a'..=b'z' | b'0'..=b'9' => prev_dash = false,
            b'-' => {
                // 不允许连续/首尾连字符
                if prev_dash || bytes[0] == b'-' {
                    return false;
                }
                prev_dash = true;
            }
            _ => return false,
        }
    }
    !prev_dash // 尾连字符拒绝
}

pub fn save_skill(skill: &SkillInfo) -> Result<SkillInfo, String> {
    if skill.id.trim().is_empty() {
        return Err("技能 id 不能为空".to_string());
    }
    // DSH 名称语法：kebab-case（安全：id 用于目录/文件名）
    if !is_valid_skill_id(skill.id.trim()) {
        return Err("技能 id 须为 kebab-case（小写字母/数字/连字符，如 my-skill）".to_string());
    }
    if skill.content.trim().is_empty() {
        return Err("技能内容不能为空".to_string());
    }
    let path = skill_path(skill.id.trim());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建技能目录失败: {}", e))?;
    }
    std::fs::write(&path, skill.content.as_bytes()).map_err(|e| format!("写入技能失败: {}", e))?;
    let mut store = skills_store().lock().unwrap();
    store.retain(|s| s.id != skill.id);
    let (body, name, description, model_invocable) = parse_frontmatter(&skill.content);
    let saved = SkillInfo {
        id: skill.id.trim().to_string(),
        name,
        description,
        content: body,
        model_invocable,
    };
    store.push(saved.clone());
    store.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(saved)
}

/// 删除技能
pub fn delete_skill(id: &str) -> Result<(), String> {
    let path = skills_dir().join(id);
    if path.exists() {
        std::fs::remove_dir_all(&path).map_err(|e| format!("删除技能失败: {}", e))?;
    }
    skills_store().lock().unwrap().retain(|s| s.id != id);
    Ok(())
}

// ─── IPC ───

#[tauri::command]
pub async fn list_harness_skills() -> Result<Vec<SkillInfo>, String> {
    Ok(skills_store().lock().unwrap().clone())
}

#[tauri::command]
pub async fn save_harness_skill(skill: SkillInfo) -> Result<SkillInfo, String> {
    save_skill(&skill)
}

#[tauri::command]
pub async fn delete_harness_skill(id: String) -> Result<(), String> {
    delete_skill(&id)
}

/// 技能工具：skill_list（供模型了解可用技能；仅模型可调用技能）
pub fn skill_list_result() -> Result<String, String> {
    let skills: Vec<SkillInfo> = skills_store()
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.model_invocable)
        .cloned()
        .collect();
    if skills.is_empty() {
        return Ok("（暂无已安装技能）".to_string());
    }
    Ok(skills
        .iter()
        .map(|s| format!("- {}：{}", s.id, s.description))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// 技能工具：skill_load（读取技能内容；禁模型调用技能拒绝）
pub fn skill_load_result(name: &str) -> Result<String, String> {
    let skills = skills_store().lock().unwrap();
    let Some(skill) = skills.iter().find(|s| s.id == name) else {
        return Err(format!("未找到技能「{}」", name));
    };
    if !skill.model_invocable {
        return Err(format!(
            "技能「{}」已禁用模型调用（disable-model-invocation），请用户经 /skill 手势使用",
            name
        ));
    }
    Ok(skill.content.clone())
}

// ─── 用户手势注入（/skill <id>：下一回合系统提示词注入） ───

fn pending_injections() -> &'static Mutex<Vec<String>> {
    static P: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(Vec::new()))
}

/// 把技能内容注入下一回合系统提示词
/// 待注入条目携带技能 id（格式 `{session}\u{1f}{skill_id}\u{1f}{content}`），
/// 使回合注入时能随事件落日志（模型可见 ⟺ 落日志）。
pub fn inject_next(session_id: &str, skill_id: &str) -> Result<(), String> {
    let skills = skills_store().lock().unwrap();
    let Some(skill) = skills.iter().find(|s| s.id == skill_id) else {
        return Err(format!("未找到技能「{}」", skill_id));
    };
    pending_injections().lock().unwrap().push(format!(
        "{session_id}\u{1f}{skill_id}\u{1f}{}",
        skill.content
    ));
    Ok(())
}

/// 取回并清空本会话的待注入技能内容；返回 (注入上下文, 注入的技能 id 列表)。
/// 调用方应把 id 列表随 SkillInjected 事件落日志，保证回放可重建本轮模型输入。
pub fn drain_injections(session_id: &str) -> (String, Vec<String>) {
    let mut pending = pending_injections().lock().unwrap();
    let mut out = String::new();
    let mut ids = Vec::new();
    let mut kept = Vec::new();
    for entry in pending.drain(..) {
        // session / skill_id / content 三段；技能正文含分隔符时按前两段切分即可
        let mut parts = entry.splitn(3, '\u{1f}');
        let sid = parts.next().unwrap_or_default();
        if sid == session_id {
            let skill_id = parts.next().unwrap_or_default().to_string();
            let content = parts.next().unwrap_or_default();
            ids.push(skill_id);
            out.push_str("<system-reminder>\n[技能]\n");
            out.push_str(content);
            out.push_str("\n</system-reminder>\n");
        } else {
            kept.push(entry);
        }
    }
    *pending = kept;
    (out, ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_save_load_delete_roundtrip() {
        let id = format!("test-skill-{}", uuid::Uuid::new_v4().simple());
        let skill = SkillInfo {
            id: id.clone(),
            name: "测试技能".into(),
            description: "用于测试".into(),
            content: "# 测试技能\n\n这是测试内容。".into(),
            model_invocable: true,
        };
        let saved = save_skill(&skill).unwrap();
        assert_eq!(saved.id, id);
        assert!(skill_load_result(&id).unwrap().contains("这是测试内容"));
        assert!(skill_list_result().unwrap().contains(&id));
        delete_skill(&id).unwrap();
        assert!(skill_load_result(&id).is_err());
    }

    #[test]
    fn frontmatter_parses_and_gates_model_invocation() {
        let id = format!("fm-skill-{}", uuid::Uuid::new_v4().simple());
        let content = "---\nname: 门控技能\ndescription: 仅用户手势\ndisable-model-invocation: true\n---\n\n# 门控技能\n\n正文内容。";
        let (body, name, description, invocable) = parse_frontmatter(content);
        assert_eq!(name, "门控技能");
        assert_eq!(description, "仅用户手势");
        assert!(!invocable);
        assert!(body.contains("正文内容"));
        let skill = SkillInfo {
            id: id.clone(),
            name: String::new(),
            description: String::new(),
            content: content.to_string(),
            model_invocable: true,
        };
        let saved = save_skill(&skill).unwrap();
        assert!(!saved.model_invocable);
        assert!(!skill_list_result().unwrap().contains(&id));
        assert!(skill_load_result(&id).is_err());
        delete_skill(&id).unwrap();
    }

    #[test]
    fn skill_injection_returns_context_and_ids() {
        let id = format!("inj-skill-{}", uuid::Uuid::new_v4().simple());
        let skill = SkillInfo {
            id: id.clone(),
            name: "注入技能".into(),
            description: "测试注入".into(),
            content: "# 注入技能\n\n待注入正文。".into(),
            model_invocable: true,
        };
        save_skill(&skill).unwrap();
        let sid = format!("s-{}", uuid::Uuid::new_v4().simple());
        inject_next(&sid, &id).unwrap();
        let (ctx, ids) = drain_injections(&sid);
        assert_eq!(ids, vec![id.clone()], "注入的技能 id 应返回");
        assert!(ctx.contains("[技能]"));
        assert!(ctx.contains("待注入正文"));
        // 已清空：再次取回为空
        let (ctx2, ids2) = drain_injections(&sid);
        assert!(ctx2.is_empty() && ids2.is_empty());
        delete_skill(&id).unwrap();
    }

    #[test]
    fn skill_injection_scoped_per_session() {
        let id = format!("inj2-skill-{}", uuid::Uuid::new_v4().simple());
        save_skill(&SkillInfo {
            id: id.clone(),
            name: "作用域".into(),
            description: "测试".into(),
            content: "会话 A 注入内容".into(),
            model_invocable: true,
        })
        .unwrap();
        inject_next("session-a", &id).unwrap();
        // B 会话取回不到 A 的注入（条目保留给 A）
        let (ctx_b, ids_b) = drain_injections("session-b");
        assert!(ctx_b.is_empty() && ids_b.is_empty());
        let (ctx_a, ids_a) = drain_injections("session-a");
        assert_eq!(ids_a, vec![id.clone()]);
        assert!(ctx_a.contains("会话 A 注入内容"));
        delete_skill(&id).unwrap();
    }

    #[test]
    fn frontmatter_defaults_model_invocable_when_not_disabled() {
        // 门控相反场景：frontmatter 缺 disable-model-invocation → 默认可
        // 由模型调用（default_true 语义）；显式 false 等同缺省
        let content = "---\nname: 默认可调\n---\n\n# 正文";
        let (body, name, _desc, invocable) = parse_frontmatter(content);
        assert_eq!(name, "默认可调");
        assert!(invocable, "缺省应默认可调用");
        assert!(body.contains("正文"));
        // 显式 false = 缺省（可调用）
        let content2 = "---\ndisable-model-invocation: false\n---\n\n正文";
        let (_body, _n, _d, invocable2) = parse_frontmatter(content2);
        assert!(invocable2, "显式 false 应可调用");
        // 无 frontmatter 纯正文 → 缺省值
        let (_body, _n, _d, invocable3) = parse_frontmatter("纯正文无元数据");
        assert!(invocable3, "无 frontmatter 应默认可调用");
    }

    #[test]
    fn skill_id_requires_kebab_case() {
        // DSH isSkillName 语义：合法 kebab-case 通过
        assert!(is_valid_skill_id("my-skill"));
        assert!(is_valid_skill_id("skill1"));
        assert!(is_valid_skill_id("a-b-c"));
        assert!(is_valid_skill_id("a"));
        // 非法：大写/空格/连续连字符/首尾连字符/空
        assert!(!is_valid_skill_id("My-Skill"), "大写拒绝");
        assert!(!is_valid_skill_id("my skill"), "空格拒绝");
        assert!(!is_valid_skill_id("my--skill"), "连续连字符拒绝");
        assert!(!is_valid_skill_id("-skill"), "首连字符拒绝");
        assert!(!is_valid_skill_id("skill-"), "尾连字符拒绝");
        assert!(!is_valid_skill_id(""), "空拒绝");
        assert!(!is_valid_skill_id("skill_1"), "下划线拒绝（非 kebab）");
    }
}
