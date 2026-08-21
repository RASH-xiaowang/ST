// ============================================================
// Harness — 配置可移植性（DSH 扩展：MCP/技能/预设等导入导出）
//
// 打包/合并「预设 + 技能 + MCP 服务器 + LSP 服务器 + 钩子」为
// 单一 JSON 配置束（bundle），支持文件导出/导入与粘贴导入：
// - 导出：读取各注册表现状 → 序列化（给定 path 时写文件）
// - 导入：按 id 合并（同 id 覆盖、新 id 追加），拒绝空 id/空名称
//   等无效条目；导入后刷新 MCP 工具注册
// ============================================================

use serde::{Deserialize, Serialize};

/// 配置束（可移植的 Harness 配置快照）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct HarnessBundle {
    #[serde(default)]
    pub presets: Vec<super::preset::HarnessPreset>,
    #[serde(default)]
    pub skills: Vec<super::skill::SkillInfo>,
    #[serde(default)]
    pub mcp_servers: Vec<super::mcp::McpServerConfig>,
    #[serde(default)]
    pub lsp_servers: Vec<super::lsp::LspServerConfig>,
    #[serde(default)]
    pub hooks: Vec<super::hooks::HarnessHook>,
}

/// 导出配置束：无 path 时返回 JSON 文本；有 path 时写入文件并返回路径
#[tauri::command]
pub async fn harness_export_bundle(path: Option<String>) -> Result<String, String> {
    let bundle = HarnessBundle {
        presets: super::preset::presets_store().lock().unwrap().clone(),
        skills: super::skill::skills_store().lock().unwrap().clone(),
        mcp_servers: super::mcp::mcp_store().lock().unwrap().clone(),
        lsp_servers: super::lsp::lsp_store().lock().unwrap().clone(),
        hooks: super::hooks::hooks_store().lock().unwrap().clone(),
    };
    let json =
        serde_json::to_string_pretty(&bundle).map_err(|e| format!("序列化配置束失败: {e}"))?;
    match path {
        Some(p) if !p.trim().is_empty() => {
            std::fs::write(&p, json.as_bytes()).map_err(|e| format!("写入配置束失败: {e}"))?;
            Ok(p)
        }
        _ => Ok(json),
    }
}

/// 导入配置束：path（读取文件）或 json（直接粘贴文本）二选一；
/// 返回合并条目数。按 id 合并：同 id 覆盖、新 id 追加。
#[tauri::command]
pub async fn harness_import_bundle(
    path: Option<String>,
    json: Option<String>,
) -> Result<usize, String> {
    let text = match (path, json) {
        (Some(p), _) if !p.trim().is_empty() => {
            std::fs::read_to_string(&p).map_err(|e| format!("读取配置束失败: {e}"))?
        }
        (_, Some(j)) if !j.trim().is_empty() => j,
        _ => return Err("缺少配置束来源（文件路径或 JSON 文本）".to_string()),
    };
    let bundle: HarnessBundle =
        serde_json::from_str(&text).map_err(|e| format!("配置束 JSON 解析失败: {e}"))?;
    let mut count = 0usize;

    // 预设：同 id 覆盖
    {
        let mut list = super::preset::presets_store().lock().unwrap();
        for p in &bundle.presets {
            if p.id.trim().is_empty() || p.name.trim().is_empty() {
                continue;
            }
            list.retain(|x| x.id != p.id);
            list.push(p.clone());
            count += 1;
        }
        list.sort_by(|a, b| a.id.cmp(&b.id));
        super::preset::persist(&list)?;
    }
    // 技能：save_skill 写 SKILL.md（同 id 覆盖）
    for s in &bundle.skills {
        if s.id.trim().is_empty() {
            continue;
        }
        super::skill::save_skill(s)?;
        count += 1;
    }
    // MCP：同 id 覆盖 + 刷新工具注册（先释放锁再刷新，避免 refresh_registry
    // 内再次获取同一把 Mutex 造成死锁——std Mutex 不可重入）
    {
        let mut list = super::mcp::mcp_store().lock().unwrap();
        for m in &bundle.mcp_servers {
            if m.id.trim().is_empty() || m.command.trim().is_empty() {
                continue;
            }
            list.retain(|x| x.id != m.id);
            list.push(m.clone());
            count += 1;
        }
        super::mcp::persist(&list)?;
    }
    let _ = super::mcp::refresh_registry();
    // LSP：同 id 覆盖
    {
        let mut list = super::lsp::lsp_store().lock().unwrap();
        for s in &bundle.lsp_servers {
            if s.id.trim().is_empty() || s.command.trim().is_empty() {
                continue;
            }
            list.retain(|x| x.id != s.id);
            list.push(s.clone());
            count += 1;
        }
        super::lsp::persist(&list)?;
    }
    // 钩子：同 id 覆盖
    {
        let mut list = super::hooks::hooks_store().lock().unwrap();
        for h in &bundle.hooks {
            if h.id.trim().is_empty() {
                continue;
            }
            list.retain(|x| x.id != h.id);
            list.push(h.clone());
            count += 1;
        }
        super::hooks::persist(&list)?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_roundtrip_serialization() {
        let b = HarnessBundle::default();
        let json = serde_json::to_string(&b).unwrap();
        let back: HarnessBundle = serde_json::from_str(&json).unwrap();
        assert!(back.presets.is_empty());
        assert!(back.skills.is_empty());
        assert!(back.mcp_servers.is_empty());
        assert!(back.lsp_servers.is_empty());
        assert!(back.hooks.is_empty());
    }

    #[test]
    fn bundle_rejects_garbage() {
        let bad: Result<HarnessBundle, _> = serde_json::from_str("{ not json");
        assert!(bad.is_err());
    }

    #[test]
    fn import_validation_skips_invalid_entries() {
        // 导入校验（与 harness_import_bundle 分支一致）：
        // 各条目空 id/空名/空命令被跳过，不计入合并数
        let mut count = 0usize;
        // 预设：空 id 或空名跳过
        for p in &[
            crate::harness::preset::HarnessPreset {
                id: String::new(),
                name: "n".into(),
                description: String::new(),
                disabled_tools: vec![],
                overrides: Default::default(),
                prompt_sections: vec![],
                created_at: String::new(),
                updated_at: String::new(),
            },
            crate::harness::preset::HarnessPreset {
                id: "ok".into(),
                name: String::new(),
                description: String::new(),
                disabled_tools: vec![],
                overrides: Default::default(),
                prompt_sections: vec![],
                created_at: String::new(),
                updated_at: String::new(),
            },
            crate::harness::preset::HarnessPreset {
                id: "good".into(),
                name: "名".into(),
                description: String::new(),
                disabled_tools: vec![],
                overrides: Default::default(),
                prompt_sections: vec![],
                created_at: String::new(),
                updated_at: String::new(),
            },
        ] {
            if p.id.trim().is_empty() || p.name.trim().is_empty() {
                continue;
            }
            count += 1;
        }
        assert_eq!(count, 1, "仅 id 与 name 均非空的预设计入: {count}");
        // MCP：空 id 或空命令跳过
        let mut count = 0usize;
        for m in &[
            crate::harness::mcp::McpServerConfig {
                id: String::new(),
                name: "s".into(),
                command: "cmd".into(),
                args: vec![],
                enabled: true,
                env: Default::default(),
                cwd: None,
            },
            crate::harness::mcp::McpServerConfig {
                id: "m1".into(),
                name: "s".into(),
                command: String::new(),
                args: vec![],
                enabled: true,
                env: Default::default(),
                cwd: None,
            },
            crate::harness::mcp::McpServerConfig {
                id: "m2".into(),
                name: "s".into(),
                command: "cmd".into(),
                args: vec![],
                enabled: true,
                env: Default::default(),
                cwd: None,
            },
        ] {
            if m.id.trim().is_empty() || m.command.trim().is_empty() {
                continue;
            }
            count += 1;
        }
        assert_eq!(count, 1, "仅 id 与 command 均非空的 MCP 计入: {count}");
    }
}
