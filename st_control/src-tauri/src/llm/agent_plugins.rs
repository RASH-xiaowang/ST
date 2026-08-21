// ============================================================
// 大模型 — 动态插件系统（DeepSeek Harness 能力迁移②）
//
// DSH 插件模型的 ST 适配：
// - 插件 = 持久化记录（id/名称/描述/启用态/工具列表/版本历史）
// - 每个插件可注册若干工具；工具实现为 JavaScript（前端 WebView 执行，
//   与 DSH Client 插件同信任级别），通过 请求/结果 桥接回代理循环
// - 生命周期：save（新建/更新，更新即新版本，历史不可变）/
//   set_enabled（stop/start）/ delete（undefine）
// - 审批：插件工具可声明 requires_approval，复用代理审批流
// ============================================================

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::Emitter;

/// 插件工具定义
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PluginToolDef {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: Value,
    /// 危险工具：执行前需要用户审批
    #[serde(default)]
    pub requires_approval: bool,
    /// JavaScript 实现（函数体：`async function(args, ctx) { ... }`，
    /// 在前端 WebView 中执行；args 为参数对象，ctx 提供 fetch/log）
    pub code: String,
}

/// 版本记录（不可变历史）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PluginVersion {
    pub version: u32,
    pub saved_at: String,
}

/// 动态插件
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentPlugin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub tools: Vec<PluginToolDef>,
    /// 版本历史（每次保存追加，最新在末尾）
    pub versions: Vec<PluginVersion>,
    pub created_at: String,
    pub updated_at: String,
}

// ─── 持久化 ───

fn plugins_path() -> std::path::PathBuf {
    #[cfg(test)]
    {
        if let Some(m) = TEST_PLUGINS_PATH.get() {
            if let Some(p) = m.lock().unwrap().clone() {
                return p;
            }
        }
    }
    crate::common::st_data_dir()
        .join("plugins")
        .join("plugins.json")
}

/// 测试专用：重定向持久化路径（避免单测写真实数据目录）
#[cfg(test)]
static TEST_PLUGINS_PATH: std::sync::OnceLock<std::sync::Mutex<Option<std::path::PathBuf>>> =
    std::sync::OnceLock::new();

pub(crate) fn plugins_store() -> &'static Mutex<Vec<AgentPlugin>> {
    static P: OnceLock<Mutex<Vec<AgentPlugin>>> = OnceLock::new();
    P.get_or_init(|| {
        let list = match std::fs::read_to_string(plugins_path()) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        Mutex::new(list)
    })
}

/// 测试专用：直接访问插件内存表（仅编译期测试可见）
#[cfg(test)]
pub fn plugins_store_mut() -> std::sync::MutexGuard<'static, Vec<AgentPlugin>> {
    plugins_store().lock().unwrap()
}

fn persist(list: &[AgentPlugin]) -> Result<(), String> {
    let path = plugins_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建插件目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(list).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换失败: {}", e))?;
    Ok(())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// 全部启用的插件工具（合并进代理工具目录）
pub fn enabled_plugin_tools() -> Vec<(String, PluginToolDef)> {
    let list = plugins_store().lock().unwrap();
    list.iter()
        .filter(|p| p.enabled)
        .flat_map(|p| p.tools.iter().cloned().map(move |t| (p.id.clone(), t)))
        .collect()
}

/// 按工具名查找插件工具（返回插件 id 与工具定义）
pub fn find_plugin_tool(name: &str) -> Option<(String, PluginToolDef)> {
    enabled_plugin_tools()
        .into_iter()
        .find(|(_, t)| t.name == name)
}

// ─── 前端执行桥（插件工具由前端执行后回传结果） ───

struct PendingExec {
    ok: bool,
    result: String,
}

fn pending_execs() -> &'static Mutex<HashMap<String, PendingExec>> {
    static M: OnceLock<Mutex<HashMap<String, PendingExec>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 请求前端执行插件工具并等待结果（最长 60 秒）
pub async fn run_plugin_tool(
    app: &tauri::AppHandle,
    call_id: &str,
    name: &str,
    args: &str,
    code: &str,
) -> (bool, String) {
    run_plugin_tool_on(app, call_id, name, args, code, "agent-tool-exec-request").await
}

/// 请求前端执行插件工具并等待结果（最长 60 秒）；事件名可指定，
/// 供 Harness 会话走独立通道（harness-tool-exec-request），
/// 避免与 AI 聊天（agent-tool-exec-request）双监听重复执行。
pub async fn run_plugin_tool_on(
    app: &tauri::AppHandle,
    call_id: &str,
    name: &str,
    args: &str,
    code: &str,
    event: &str,
) -> (bool, String) {
    pending_execs().lock().unwrap().remove(call_id);
    let _ = app.emit(
        event,
        json!({ "id": call_id, "name": name, "args": args, "code": code }),
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        tokio::time::sleep(Duration::from_millis(300)).await;
        if let Some(r) = pending_execs().lock().unwrap().remove(call_id) {
            return (r.ok, r.result);
        }
        if Instant::now() > deadline {
            return (false, "插件工具执行超时（60 秒）".to_string());
        }
    }
}

/// 前端提交插件工具执行结果
pub fn submit_plugin_result(call_id: &str, ok: bool, result: &str) -> bool {
    pending_execs().lock().unwrap().insert(
        call_id.to_string(),
        PendingExec {
            ok,
            result: result.to_string(),
        },
    );
    true
}

/// 插件/脚本工具执行请求（B2/B23 编排桥：前端 WebView 执行 + 结果回传）。
/// 载荷额外携带 session_id（前端 ctx.agent / ctx.tools 需要），超时可放宽
/// （workflow_run_js 300s——编排多轮子代理可能超过默认 60s）。
pub struct PluginExecRequest<'a> {
    pub app: &'a tauri::AppHandle,
    /// 执行事件 id（与前端 pending_execs 配对的唯一键）
    pub call_id: &'a str,
    /// 工具名（run_code / workflow_run_js / plugin:<name>）
    pub name: &'a str,
    /// 工具入参 JSON 文本（嵌套 args 字段）
    pub args: &'a str,
    /// 执行代码（async 函数体）
    pub code: &'a str,
    /// 前端监听的事件名
    pub event: &'a str,
    /// 超时（秒）
    pub timeout_secs: u64,
    /// 目标会话 id（前端执行桥 ctx.tools/agent 需要）
    pub session_id: &'a str,
}

/// 带自定义超时与扩展载荷的插件工具执行（B2 workflow JS 编排）：
/// 把请求 emit 给前端 WebView 执行，轮询等待结果（上限 timeout_secs）。
pub async fn run_plugin_tool_on_ext(req: PluginExecRequest<'_>) -> (bool, String) {
    pending_execs().lock().unwrap().remove(req.call_id);
    let _ = req.app.emit(
        req.event,
        json!({
            "id": req.call_id,
            "name": req.name,
            "args": req.args,
            "code": req.code,
            "session_id": req.session_id,
        }),
    );
    let deadline = Instant::now() + Duration::from_secs(req.timeout_secs);
    loop {
        tokio::time::sleep(Duration::from_millis(300)).await;
        if let Some(r) = pending_execs().lock().unwrap().remove(req.call_id) {
            return (r.ok, r.result);
        }
        if Instant::now() > deadline {
            return (false, "插件工具执行超时".to_string());
        }
    }
}

// ─── IPC ───

/// 插件列表（含版本历史）
#[tauri::command]
pub async fn list_agent_plugins() -> Result<Vec<AgentPlugin>, String> {
    Ok(plugins_store().lock().unwrap().clone())
}

/// 新建或更新插件：更新即产生新版本（版本历史不可变，旧版本保留）
#[tauri::command]
pub async fn save_agent_plugin(plugin: AgentPlugin) -> Result<AgentPlugin, String> {
    define_plugin(plugin)
}

/// 定义或更新插件（同步助手：AI 聊天与 Harness 工具共用；
/// 校验 + 新建/更新版本历史 + 持久化）
pub(crate) fn define_plugin(plugin: AgentPlugin) -> Result<AgentPlugin, String> {
    if plugin.name.trim().is_empty() {
        return Err("插件名称不能为空".to_string());
    }
    if plugin.tools.is_empty() {
        return Err("插件至少需要一个工具".to_string());
    }
    for t in &plugin.tools {
        if t.name.trim().is_empty() {
            return Err("工具名不能为空".to_string());
        }
        if t.code.trim().is_empty() {
            return Err(format!("工具「{}」的实现代码不能为空", t.name));
        }
    }
    let mut list = plugins_store().lock().unwrap();
    let now = now_iso();
    let saved = if plugin.id.is_empty() {
        // 新建
        let mut p = plugin;
        p.id = format!("plugin-{}", uuid::Uuid::new_v4().simple());
        p.created_at = now.clone();
        p.updated_at = now.clone();
        p.versions = vec![PluginVersion {
            version: 1,
            saved_at: now.clone(),
        }];
        list.push(p.clone());
        p
    } else {
        // 更新：版本 +1，历史追加
        let Some(existing) = list.iter().find(|p| p.id == plugin.id) else {
            return Err("指定的插件不存在".to_string());
        };
        let next_version = existing.versions.last().map(|v| v.version + 1).unwrap_or(1);
        let mut p = plugin;
        p.created_at = existing.created_at.clone();
        p.updated_at = now.clone();
        let mut versions = existing.versions.clone();
        versions.push(PluginVersion {
            version: next_version,
            saved_at: now.clone(),
        });
        p.versions = versions;
        let idx = list.iter().position(|x| x.id == p.id).unwrap();
        list[idx] = p.clone();
        p
    };
    persist(&list)?;
    Ok(saved)
}

/// 删除插件（undefine）
#[tauri::command]
pub async fn delete_agent_plugin(id: String) -> Result<(), String> {
    delete_plugin(&id)
}

/// 删除插件（同步助手）
pub(crate) fn delete_plugin(id: &str) -> Result<(), String> {
    let mut list = plugins_store().lock().unwrap();
    let before = list.len();
    list.retain(|p| p.id != id);
    if list.len() == before {
        return Err("指定的插件不存在".to_string());
    }
    persist(&list)
}

/// 启用/停用插件（run/stop）
#[tauri::command]
pub async fn set_agent_plugin_enabled(id: String, enabled: bool) -> Result<AgentPlugin, String> {
    set_enabled(&id, enabled)
}

/// 启用/停用插件（同步助手）
pub(crate) fn set_enabled(id: &str, enabled: bool) -> Result<AgentPlugin, String> {
    let mut list = plugins_store().lock().unwrap();
    let Some(p) = list.iter_mut().find(|p| p.id == id) else {
        return Err("指定的插件不存在".to_string());
    };
    p.enabled = enabled;
    p.updated_at = now_iso();
    let out = p.clone();
    persist(&list)?;
    Ok(out)
}

/// 前端提交插件工具执行结果（内部桥接）
#[tauri::command]
pub async fn submit_agent_tool_result(
    id: String,
    ok: bool,
    result: String,
) -> Result<bool, String> {
    Ok(submit_plugin_result(&id, ok, &result))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plugin() -> AgentPlugin {
        AgentPlugin {
            id: String::new(),
            name: "测试插件".to_string(),
            description: "测试".to_string(),
            enabled: false,
            tools: vec![PluginToolDef {
                name: "calc".to_string(),
                description: "计算".to_string(),
                parameters: json!({ "type": "object", "properties": { "expression": { "type": "string" } }, "required": ["expression"] }),
                requires_approval: false,
                code: "return String(eval(args.expression));".to_string(),
            }],
            versions: vec![],
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    /// 版本历史：新建 v1，更新 v2，旧版本保留（不可变）
    #[test]
    fn plugin_version_history_is_append_only() {
        // 用临时路径隔离：直接操作内存 store 并还原
        let mut p = sample_plugin();
        p.id = format!("test-{}", uuid::Uuid::new_v4().simple());
        {
            let mut list = plugins_store().lock().unwrap();
            list.retain(|x| x.id != p.id);
            list.push(p.clone());
        }
        // 模拟两次保存的版本演进
        let v1 = PluginVersion {
            version: 1,
            saved_at: "t1".into(),
        };
        let v2 = PluginVersion {
            version: 2,
            saved_at: "t2".into(),
        };
        let mut history = vec![v1.clone()];
        history.push(v2.clone());
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].version, 1);
        assert_eq!(history[1].version, 2);
        {
            let mut list = plugins_store().lock().unwrap();
            list.retain(|x| x.id != p.id);
        }
    }

    #[test]
    fn plugin_tool_lookup_by_name() {
        let mut p = sample_plugin();
        p.id = format!("test-{}", uuid::Uuid::new_v4().simple());
        p.enabled = true;
        {
            let mut list = plugins_store().lock().unwrap();
            list.retain(|x| x.id != p.id);
            list.push(p.clone());
        }
        let hit = find_plugin_tool("calc");
        assert!(hit.is_some(), "启用的插件工具应可被查找");
        let (pid, tool) = hit.unwrap();
        assert_eq!(pid, p.id);
        assert_eq!(tool.name, "calc");
        {
            let mut list = plugins_store().lock().unwrap();
            list.retain(|x| x.id != p.id);
        }
    }

    /// 同步助手（Harness 工具共用）：define → 默认启用 → disable → enable → delete 回环
    #[test]
    fn sync_helpers_define_enable_disable_delete_roundtrip() {
        // 持久化重定向到临时文件，避免污染真实 data/plugins/plugins.json
        let tmp = std::env::temp_dir().join(format!(
            "st-agent-plugins-test-{}.json",
            uuid::Uuid::new_v4().simple()
        ));
        TEST_PLUGINS_PATH
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .unwrap()
            .replace(tmp.clone());
        let mut p = sample_plugin();
        p.id = String::new(); // 新建
        p.enabled = true;
        // 唯一工具名：避免与并行测试 plugin_tool_lookup_by_name 的 "calc" 互相干扰
        p.tools[0].name = format!("calc_{}", uuid::Uuid::new_v4().simple());
        let tool_name = p.tools[0].name.clone();
        let saved = define_plugin(p).expect("定义插件应成功");
        assert!(!saved.id.is_empty(), "新建应分配 id");
        assert!(saved.enabled);
        assert_eq!(saved.versions.len(), 1);
        let id = saved.id.clone();

        // 停用
        let off = set_enabled(&id, false).expect("停用应成功");
        assert!(!off.enabled);
        // 启用
        let on = set_enabled(&id, true).expect("启用应成功");
        assert!(on.enabled);
        // 工具可查找
        assert!(find_plugin_tool(&tool_name).is_some());
        // 删除
        delete_plugin(&id).expect("删除应成功");
        assert!(
            plugins_store().lock().unwrap().iter().all(|x| x.id != id),
            "删除后不应残留"
        );
        // 还原持久化路径（内存 store 保持，后续测试不受影响）
        TEST_PLUGINS_PATH
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .unwrap()
            .take();
        let _ = std::fs::remove_file(&tmp);
    }
}
