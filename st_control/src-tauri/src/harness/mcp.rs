// ============================================================
// Harness — MCP 客户端（DSH mcp 迁移）
//
// 模型上下文协议客户端：为配置的外部 MCP 服务器（stdio：
// 命令 + 参数）建立 JSON-RPC 会话，tools/list 后把工具注册进
// Harness 工具注册表（命名 mcp_<server>_<tool>），调用时
// 无状态派生新会话执行 tools/call（每次调用独立启动服务器进程）。
// 配置持久化：data/harness/mcp.json（原子写）。
// ============================================================

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::sync::{Mutex, OnceLock};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub enabled: bool,
    /// 额外环境变量（DSH mcp env 配置项；与凭据注入合并）
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// 服务器工作目录（DSH mcp cwd 配置项；空 = 继承）
    #[serde(default)]
    pub cwd: Option<String>,
}

fn mcp_path() -> std::path::PathBuf {
    crate::common::st_data_dir()
        .join("harness")
        .join("mcp.json")
}

pub(crate) fn mcp_store() -> &'static Mutex<Vec<McpServerConfig>> {
    static M: OnceLock<Mutex<Vec<McpServerConfig>>> = OnceLock::new();
    M.get_or_init(|| {
        let list = std::fs::read_to_string(mcp_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Mutex::new(list)
    })
}

pub(crate) fn persist(list: &[McpServerConfig]) -> Result<(), String> {
    let path = mcp_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建 MCP 目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(list).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换失败: {}", e))?;
    Ok(())
}

/// stdio JSON-RPC 会话（无状态派生）
struct McpSession {
    child: std::process::Child,
    reader: std::io::BufReader<std::process::ChildStdout>,
    writer: std::process::ChildStdin,
    next_id: u64,
}

impl McpSession {
    fn spawn(config: &McpServerConfig) -> Result<Self, String> {
        let mut cmd = std::process::Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        // 凭据注入（credentials：HARNESS_CREDENTIAL_<KEY>）+ 额外环境变量
        crate::harness::credentials::inject_env(&mut cmd);
        for (k, v) in &config.env {
            cmd.env(k, v);
        }
        if let Some(cwd) = &config.cwd {
            if !cwd.is_empty() {
                cmd.current_dir(cwd);
            }
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("启动 MCP 服务器失败: {}", e))?;
        let reader = BufReader::new(child.stdout.take().ok_or("无法打开 MCP 服务器 stdout")?);
        let writer = child.stdin.take().ok_or("无法打开 MCP 服务器 stdin")?;
        Ok(McpSession {
            child,
            reader,
            writer,
            next_id: 1,
        })
    }

    fn send(&mut self, method: &str, params: serde_json::Value) -> Result<(), String> {
        let id = self.next_id;
        self.next_id += 1;
        let line =
            serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(self.writer, "{}", line).map_err(|e| format!("写入 MCP 请求失败: {}", e))?;
        self.writer
            .flush()
            .map_err(|e| format!("刷新 MCP 请求失败: {}", e))
    }

    fn recv(&mut self, want_id: u64) -> Result<serde_json::Value, String> {
        loop {
            let mut line = String::new();
            let n = self
                .reader
                .read_line(&mut line)
                .map_err(|e| format!("读取 MCP 响应失败: {}", e))?;
            if n == 0 {
                return Err("MCP 服务器已关闭".to_string());
            }
            let v: serde_json::Value = serde_json::from_str(line.trim())
                .map_err(|e| format!("解析 MCP 响应失败: {}", e))?;
            // 跳过通知（无 id），只取目标 id 的响应
            if v.get("id").and_then(|i| i.as_u64()) == Some(want_id) {
                return Ok(v);
            }
        }
    }

    fn request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.send(method, params)?;
        let resp = self.recv(id)?;
        if let Some(err) = resp.get("error") {
            return Err(format!(
                "MCP 错误: {}",
                err.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("未知")
            ));
        }
        Ok(resp
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 初始化会话：initialize + tools/list → (工具名, 输入 schema) 列表
fn list_tools(config: &McpServerConfig) -> Result<Vec<(String, serde_json::Value)>, String> {
    let mut session = McpSession::spawn(config)?;
    session.request(
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "st-harness", "version": "1.0.0" },
        }),
    )?;
    session.send("notifications/initialized", serde_json::json!({}))?;
    let tools = session.request("tools/list", serde_json::json!({}))?;
    let list: Vec<(String, serde_json::Value)> = tools
        .get("tools")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let name = t.get("name").and_then(|n| n.as_str())?.to_string();
                    let schema = t.get("inputSchema").cloned().unwrap_or_else(
                        || serde_json::json!({ "type": "object", "properties": {} }),
                    );
                    Some((name, schema))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(list)
}

/// 调用一个工具（无状态派生新会话）
fn call_tool(config: &McpServerConfig, tool: &str, arguments: &str) -> Result<String, String> {
    let mut session = McpSession::spawn(config)?;
    session.request(
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "st-harness", "version": "1.0.0" },
        }),
    )?;
    session.send("notifications/initialized", serde_json::json!({}))?;
    let args: serde_json::Value =
        serde_json::from_str(arguments).unwrap_or_else(|_| serde_json::json!({}));
    let result = session.request(
        "tools/call",
        serde_json::json!({ "name": tool, "arguments": args }),
    )?;
    let content = result
        .get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    c.get("text")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| result.to_string());
    Ok(content)
}

/// 刷新注册：把全部启用服务器工具注册进 Harness 工具注册表
pub fn refresh_registry() -> Result<usize, String> {
    let configs: Vec<McpServerConfig> = mcp_store()
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.enabled)
        .cloned()
        .collect();
    let mut count = 0;
    for config in configs {
        let tools = list_tools(&config)?;
        for (tool, schema) in tools {
            let harness_name = format!("mcp_{}_{}", config.id, tool);
            let server = config.clone();
            // schema 透传（DSH mcp-client 语义）：模型可见服务器声明的参数结构；
            // 非法 schema 回退空对象
            let parameters = if schema.is_object() {
                schema
            } else {
                serde_json::json!({ "type": "object", "properties": {} })
            };
            let spec = crate::llm::agent::ToolSpec {
                name: harness_name.clone(),
                description: format!("MCP 工具（服务器 {}）：{}", config.name, tool),
                parameters,
                requires_approval: false,
                run: crate::llm::agent::ToolRunner::Dyn(std::sync::Arc::new(move |_app, args| {
                    call_tool(&server, &tool, &args.to_string())
                })),
            };
            crate::harness::tools::register_tool(spec);
            count += 1;
        }
    }
    Ok(count)
}

// ─── IPC ───

#[tauri::command]
pub async fn list_harness_mcp_servers() -> Result<Vec<McpServerConfig>, String> {
    Ok(mcp_store().lock().unwrap().clone())
}

/// 全量保存 MCP 服务器配置并刷新工具注册
#[tauri::command]
pub async fn save_harness_mcp_servers(
    servers: Vec<McpServerConfig>,
) -> Result<Vec<McpServerConfig>, String> {
    for s in &servers {
        if s.command.trim().is_empty() {
            return Err(format!("服务器「{}」的命令不能为空", s.name));
        }
    }
    {
        let mut list = mcp_store().lock().unwrap();
        *list = servers.clone();
    }
    persist(&servers)?;
    match refresh_registry() {
        Ok(n) => {
            log::info!("[harness] MCP 工具注册刷新完成（{} 个工具）", n);
            Ok(servers)
        }
        Err(e) => Err(format!("MCP 工具注册刷新失败: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_config_shape() {
        let c = McpServerConfig {
            id: "s1".into(),
            name: "测试".into(),
            command: "powershell.exe".into(),
            args: vec!["-File".into(), "server.ps1".into()],
            enabled: true,
            env: std::collections::HashMap::new(),
            cwd: None,
        };
        assert_eq!(c.args.len(), 2);
        assert!(c.enabled);
        // env/cwd 序列化向后兼容（旧配置无字段）
        let json = serde_json::to_string(&c).unwrap();
        let back: McpServerConfig = serde_json::from_str(&json).unwrap();
        assert!(back.env.is_empty());
        assert!(back.cwd.is_none());
        let legacy = r#"{"id":"s2","name":"旧","command":"cmd","args":[],"enabled":true}"#;
        let old: McpServerConfig = serde_json::from_str(legacy).unwrap();
        assert!(old.env.is_empty());
        assert!(old.cwd.is_none());
    }

    #[test]
    fn mcp_tool_naming_and_schema_passthrough() {
        // 工具命名 mcp_<id>_<tool> 与 schema 透传/回退（与 refresh_registry 一致）
        let config = McpServerConfig {
            id: "srv-a".into(),
            name: "测试服务器".into(),
            command: "x".into(),
            args: vec![],
            enabled: true,
            env: Default::default(),
            cwd: None,
        };
        // 命名规则
        let tool = "read_file";
        let harness_name = format!("mcp_{}_{}", config.id, tool);
        assert_eq!(harness_name, "mcp_srv-a_read_file");
        // schema 透传：对象 schema 原样保留
        let good: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"],
        });
        let parameters = if good.is_object() {
            good.clone()
        } else {
            serde_json::json!({ "type": "object", "properties": {} })
        };
        assert_eq!(parameters["required"][0], "path");
        // schema 回退：非法（非对象）→ 空对象
        let bad: serde_json::Value = serde_json::json!("not-a-schema");
        let parameters = if bad.is_object() {
            bad
        } else {
            serde_json::json!({ "type": "object", "properties": {} })
        };
        assert!(
            parameters.get("properties").is_some(),
            "非法 schema 应回退空对象"
        );
    }
}
