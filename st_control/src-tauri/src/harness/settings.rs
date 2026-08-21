// ============================================================
// Harness — 用户设置能力（DSH settings 迁移）
//
// 设置持久化（data/harness/settings.json，原子写）：
// - 最近使用的提供方/模型（界面选择记忆）
// - 工具执行超时（秒，5~300，默认 30）与最大工具轮次（1~12，默认 6）：
//   guard 的可配置项（DSH 约定：部署可变项是校验过的 Config 字段）
// - 默认预设（preset 组合：空 = 不使用）
// ============================================================

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

/// 工具超时默认值（秒）
pub const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 30;
/// 最大工具轮次默认值
pub const DEFAULT_MAX_ROUNDS: usize = 6;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct HarnessSettings {
    /// 最近使用的提供方 id（空 = 回退全局默认）
    #[serde(default)]
    pub last_provider_id: String,
    /// 最近使用的模型 id（空 = 提供方默认模型）
    #[serde(default)]
    pub last_model: String,
    /// 工具执行超时（秒）；None = 默认 30
    #[serde(default)]
    pub tool_timeout_secs: Option<u64>,
    /// 最大工具轮次；None = 默认 6
    #[serde(default)]
    pub max_agent_rounds: Option<usize>,
    /// 默认预设 id（""/None = 不启用预设组合）
    #[serde(default)]
    pub preset_id: Option<String>,
    /// 受限执行世界：允许访问 agent_workspace 之外（默认 false）
    #[serde(default)]
    pub allow_workspace_escape: bool,
    /// 沙箱模式（DSH sandbox 三模式）：
    /// read-only=只读工具；workspace-write=工作区内读写（默认）；
    /// danger-full-access=工作区外全权（等价 allow_workspace_escape）
    #[serde(default = "default_sandbox_mode")]
    pub sandbox_mode: String,
    /// 当前工作区 id（""/default = 默认工作区；DSH workspace 迁移）
    #[serde(default)]
    pub workspace_id: String,
    /// 上下文压缩预算（token 估算；None = 默认 24000）
    #[serde(default)]
    pub context_budget_tokens: Option<u64>,
    /// 启用上下文压缩（默认 true）
    #[serde(default = "default_true")]
    pub enable_compaction: bool,
    /// 繁忙时 Enter 键行为（DSH ui-conversation busyEnter）：
    /// queue=排队发送（默认）；steer=插话发送（新消息排到队首）
    #[serde(default)]
    pub busy_enter: Option<String>,
    /// 会话级推理等级（DSH reasoningEffort：off / high / max 等；
    /// 空 = 跟随提供方部署默认；请求时透传 reasoning_effort 参数）
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// 联网搜索提供商（DSH web 提供商缝）：bing（默认）/ deepseek
    #[serde(default)]
    pub web_search_provider: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_sandbox_mode() -> String {
    "workspace-write".to_string()
}

impl HarnessSettings {
    /// 有效沙箱模式（非法值回退默认）
    pub fn effective_sandbox_mode(&self) -> String {
        match self.sandbox_mode.as_str() {
            "read-only" | "workspace-write" | "danger-full-access" => self.sandbox_mode.clone(),
            _ => default_sandbox_mode(),
        }
    }

    /// 是否允许工作区外访问（danger-full-access 或旧的布尔开关）
    pub fn effective_workspace_escape(&self) -> bool {
        self.allow_workspace_escape || self.effective_sandbox_mode() == "danger-full-access"
    }

    /// 有效工具超时（校验范围内）
    pub fn effective_timeout_secs(&self) -> u64 {
        self.tool_timeout_secs
            .unwrap_or(DEFAULT_TOOL_TIMEOUT_SECS)
            .clamp(5, 300)
    }

    /// 有效最大轮次（校验范围内）
    pub fn effective_max_rounds(&self) -> usize {
        self.max_agent_rounds
            .unwrap_or(DEFAULT_MAX_ROUNDS)
            .clamp(1, 12)
    }

    /// 有效压缩预算（token 估算；4000~128000）
    pub fn effective_budget_tokens(&self) -> u64 {
        self.context_budget_tokens
            .unwrap_or(24_000)
            .clamp(4_000, 128_000)
    }

    /// 有效繁忙 Enter 行为（非法值回退 queue）
    pub fn effective_busy_enter(&self) -> String {
        match self.busy_enter.as_deref() {
            Some("steer") => "steer".to_string(),
            _ => "queue".to_string(),
        }
    }

    /// 有效联网搜索提供商（非法值回退 bing）
    pub fn effective_web_search_provider(&self) -> String {
        match self.web_search_provider.as_deref() {
            Some("deepseek") => "deepseek".to_string(),
            _ => "bing".to_string(),
        }
    }
}

fn settings_path() -> std::path::PathBuf {
    crate::common::st_data_dir()
        .join("harness")
        .join("settings.json")
}

fn settings_store() -> &'static Mutex<HarnessSettings> {
    static S: OnceLock<Mutex<HarnessSettings>> = OnceLock::new();
    S.get_or_init(|| {
        let s = std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Mutex::new(s)
    })
}

fn persist(s: &HarnessSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建设置目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(s).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换失败: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn get_harness_settings() -> Result<HarnessSettings, String> {
    Ok(settings_store().lock().unwrap().clone())
}

#[tauri::command]
pub async fn save_harness_settings(settings: HarnessSettings) -> Result<HarnessSettings, String> {
    // 校验：越界值失败响亮（DSH「misconfiguration fails loud」）
    if let Some(t) = settings.tool_timeout_secs {
        if !(5..=300).contains(&t) {
            return Err("工具超时需在 5~300 秒之间".to_string());
        }
    }
    if let Some(r) = settings.max_agent_rounds {
        if !(1..=12).contains(&r) {
            return Err("最大工具轮次需在 1~12 之间".to_string());
        }
    }
    if let Some(b) = settings.context_budget_tokens {
        if !(4000..=128000).contains(&b) {
            return Err("上下文压缩预算需在 4000~128000 之间".to_string());
        }
    }
    let mut s = settings_store().lock().unwrap();
    *s = settings.clone();
    persist(&s)?;
    Ok(settings)
}

/// 当前有效设置（无 IPC 开销的内部读取）
pub fn current() -> HarnessSettings {
    settings_store().lock().unwrap().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_in_memory() {
        let s = HarnessSettings {
            last_provider_id: "p1".into(),
            last_model: "m1".into(),
            tool_timeout_secs: Some(20),
            max_agent_rounds: Some(4),
            preset_id: Some("preset-1".into()),
            allow_workspace_escape: false,
            sandbox_mode: "workspace-write".into(),
            workspace_id: String::new(),
            context_budget_tokens: Some(8000),
            enable_compaction: true,
            busy_enter: Some("steer".into()),
            reasoning_effort: Some("high".into()),
            web_search_provider: Some("deepseek".into()),
        };
        {
            let mut m = settings_store().lock().unwrap();
            *m = s.clone();
        }
        let loaded = settings_store().lock().unwrap().clone();
        assert_eq!(loaded.last_provider_id, "p1");
        assert_eq!(loaded.effective_timeout_secs(), 20);
        assert_eq!(loaded.effective_busy_enter(), "steer");
        assert_eq!(loaded.effective_web_search_provider(), "deepseek");
    }

    #[test]
    fn effective_values_clamp_out_of_range() {
        let s = HarnessSettings {
            tool_timeout_secs: Some(9999),
            max_agent_rounds: Some(0),
            ..Default::default()
        };
        assert_eq!(s.effective_timeout_secs(), 300);
        assert_eq!(s.effective_max_rounds(), 1);
        let d = HarnessSettings::default();
        assert_eq!(d.effective_timeout_secs(), DEFAULT_TOOL_TIMEOUT_SECS);
        assert_eq!(d.effective_max_rounds(), DEFAULT_MAX_ROUNDS);
    }

    #[test]
    fn sandbox_mode_semantics() {
        let mut s = HarnessSettings::default();
        assert_eq!(s.effective_sandbox_mode(), "workspace-write");
        assert!(!s.effective_workspace_escape());
        s.sandbox_mode = "danger-full-access".into();
        assert!(s.effective_workspace_escape());
        s.sandbox_mode = "garbage".into();
        assert_eq!(s.effective_sandbox_mode(), "workspace-write");
        s.sandbox_mode = "read-only".into();
        assert!(!s.effective_workspace_escape());
    }

    #[tokio::test]
    async fn save_rejects_out_of_range_fails_loud() {
        // DSH「misconfiguration fails loud」：越界值拒绝保存并响亮报错
        // （不静默钳制，避免用户不知情地改小超时/预算）
        let base = HarnessSettings::default();
        let mut too_small = base.clone();
        too_small.tool_timeout_secs = Some(1); // < 5
        let err = save_harness_settings(too_small).await.unwrap_err();
        assert!(err.contains("工具超时"), "超时越界应拒绝: {err}");
        let mut too_big = base.clone();
        too_big.max_agent_rounds = Some(99); // > 12
        let err = save_harness_settings(too_big).await.unwrap_err();
        assert!(err.contains("工具轮次"), "轮次越界应拒绝: {err}");
        let mut bad_budget = base.clone();
        bad_budget.context_budget_tokens = Some(100); // < 4000
        let err = save_harness_settings(bad_budget).await.unwrap_err();
        assert!(err.contains("压缩预算"), "预算越界应拒绝: {err}");
    }

    #[test]
    fn web_search_provider_invalid_falls_back_to_bing() {
        // 提供商缝：仅 deepseek 有效，其余（含非法值/空）回退 bing
        let mut s = HarnessSettings::default();
        s.web_search_provider = Some("deepseek".into());
        assert_eq!(s.effective_web_search_provider(), "deepseek");
        s.web_search_provider = Some("garbage".into());
        assert_eq!(s.effective_web_search_provider(), "bing");
        s.web_search_provider = None;
        assert_eq!(s.effective_web_search_provider(), "bing");
        s.web_search_provider = Some("".into());
        assert_eq!(s.effective_web_search_provider(), "bing");
    }
}
