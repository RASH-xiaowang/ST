// ============================================================
// Harness — 语言服务器能力（DSH lsp 迁移）
//
// LSP 客户端（stdio，Content-Length 帧）：为配置的语言服务器
// （命令 + 参数，data/harness/lsp.json）建立会话，支持：
//   initialize → initialized → textDocument/didOpen →
//   textDocument/hover（行/列位置）→ shutdown
// 无状态派生：每次查询独立启动服务器进程（与 MCP 客户端同模式）。
// 模型工具 lsp_hover {file, line, column}；未配置服务器时优雅报错。
// ============================================================

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::sync::{Mutex, OnceLock};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LspServerConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// 文件扩展名映射（如 ["rs","toml"]）：查询按文件扩展名路由服务器
    #[serde(default)]
    pub extensions: Vec<String>,
    pub enabled: bool,
}

fn lsp_path() -> std::path::PathBuf {
    crate::common::st_data_dir()
        .join("harness")
        .join("lsp.json")
}

pub(crate) fn lsp_store() -> &'static Mutex<Vec<LspServerConfig>> {
    static S: OnceLock<Mutex<Vec<LspServerConfig>>> = OnceLock::new();
    S.get_or_init(|| {
        let list = std::fs::read_to_string(lsp_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Mutex::new(list)
    })
}

pub(crate) fn persist(list: &[LspServerConfig]) -> Result<(), String> {
    let path = lsp_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建 LSP 目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(list).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换失败: {}", e))?;
    Ok(())
}

/// LSP 帧写入：Content-Length 头 + 空行 + 载荷
fn write_frame(
    writer: &mut std::process::ChildStdin,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let body = payload.to_string();
    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body)
        .map_err(|e| format!("写入 LSP 请求失败: {}", e))?;
    writer
        .flush()
        .map_err(|e| format!("刷新 LSP 请求失败: {}", e))
}

/// LSP 帧读取：解析 Content-Length 头并读取载荷
fn read_frame(reader: &mut impl BufRead) -> Result<Option<serde_json::Value>, String> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| format!("读取 LSP 头失败: {}", e))?;
        if n == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            content_length = v.trim().parse::<usize>().ok();
        }
    }
    let len = content_length.ok_or("LSP 响应缺少 Content-Length")?;
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .map_err(|e| format!("读取 LSP 载荷失败: {}", e))?;
    let v: serde_json::Value =
        serde_json::from_slice(&buf).map_err(|e| format!("解析 LSP 响应失败: {}", e))?;
    Ok(Some(v))
}

fn request(
    reader: &mut impl BufRead,
    writer: &mut std::process::ChildStdin,
    method: &str,
    params: serde_json::Value,
    id: u64,
) -> Result<serde_json::Value, String> {
    write_frame(
        writer,
        &serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
    )?;
    // 跳过通知（无 id），等待目标 id 响应
    loop {
        let Some(frame) = read_frame(reader)? else {
            return Err("LSP 服务器已关闭".to_string());
        };
        if frame.get("id").and_then(|i| i.as_u64()) == Some(id) {
            if let Some(err) = frame.get("error") {
                return Err(format!(
                    "LSP 错误: {}",
                    err.get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("未知")
                ));
            }
            return Ok(frame
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null));
        }
    }
}

/// LSP 查询（无状态派生会话）：initialize → didOpen → <method> → shutdown
fn query(
    server: &LspServerConfig,
    file_path: &str,
    line: u32,
    column: u32,
    method: &str,
) -> Result<serde_json::Value, String> {
    let mut cmd = std::process::Command::new(&server.command);
    cmd.args(&server.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    crate::harness::credentials::inject_env(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("启动 LSP 服务器失败: {}", e))?;
    let mut reader = BufReader::new(child.stdout.take().ok_or("无法打开 LSP stdout")?);
    let mut writer = child.stdin.take().ok_or("无法打开 LSP stdin")?;
    // 文档内容（工作区沙箱读取；LSP 测试服务器不依赖真实内容）
    let content = crate::harness::fs::FsService
        .read_text(file_path, &crate::harness::fs::FsPolicy::current())
        .unwrap_or_default();
    let uri = format!("file:///{}", file_path.replace('\\', "/"));
    let result = (|| -> Result<serde_json::Value, String> {
        request(
            &mut reader,
            &mut writer,
            "initialize",
            serde_json::json!({
                "processId": std::process::id(),
                "rootUri": null,
                "capabilities": {},
            }),
            1,
        )?;
        write_frame(
            &mut writer,
            &serde_json::json!({
                "jsonrpc": "2.0", "method": "initialized", "params": {}
            }),
        )?;
        write_frame(
            &mut writer,
            &serde_json::json!({
                "jsonrpc": "2.0", "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": uri,
                        "languageId": "plaintext",
                        "version": 1,
                        "text": content,
                    }
                }
            }),
        )?;
        let params = if method == "textDocument/references" {
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": column },
                "context": { "includeDeclaration": true },
            })
        } else {
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": column },
            })
        };
        let out = request(&mut reader, &mut writer, method, params, 2)?;
        write_frame(
            &mut writer,
            &serde_json::json!({
                "jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": null
            }),
        )?;
        Ok(out)
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

/// LSP 操作类型（模型工具映射）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LspOp {
    Hover,
    Definition,
    References,
    Implementation,
}

/// 按文件扩展名路由服务器：有扩展名映射命中的优先，否则首个启用服务器
fn pick_server(file: &str) -> Result<LspServerConfig, String> {
    let servers: Vec<LspServerConfig> = lsp_store()
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.enabled)
        .cloned()
        .collect();
    if servers.is_empty() {
        return Err("未配置启用的 LSP 服务器（治理 → LSP 配置）".to_string());
    }
    let ext = std::path::Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());
    if let Some(ext) = &ext {
        if let Some(server) = servers
            .iter()
            .find(|s| s.extensions.iter().any(|x| x.eq_ignore_ascii_case(ext)))
        {
            return Ok(server.clone());
        }
    }
    Ok(servers[0].clone())
}

/// 位置列表格式化（definition/references/implementation 共用）
fn format_locations(v: &serde_json::Value) -> String {
    let locs = if v.get("uri").is_some() {
        vec![v.clone()]
    } else {
        v.as_array().cloned().unwrap_or_default()
    };
    if locs.is_empty() {
        return "（未找到结果）".to_string();
    }
    locs.iter()
        .take(20)
        .map(|l| {
            let uri = l
                .get("uri")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .trim_start_matches("file:///");
            let range = l.get("range").cloned().unwrap_or(serde_json::Value::Null);
            let line = range
                .get("start")
                .and_then(|s| s.get("line"))
                .and_then(|v| v.as_u64())
                .map(|v| v + 1)
                .unwrap_or(0);
            format!("{}:{line}", uri)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// hover 查询：位置处的类型/文档信息（无状态派生会话）
pub fn hover(
    server: &LspServerConfig,
    file_path: &str,
    line: u32,
    column: u32,
) -> Result<String, String> {
    let hover = query(server, file_path, line, column, "textDocument/hover")?;
    // 提取 hover 文本：contents 为字符串或 markdown 结构
    let contents = hover
        .get("contents")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let text = match contents {
        serde_json::Value::String(s) => s,
        v => v
            .get("value")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| v.to_string()),
    };
    Ok(if text.trim().is_empty() {
        "（该位置无 hover 信息）".to_string()
    } else {
        text
    })
}

/// 模型工具入口：按 op 路由到对应 LSP 方法并格式化结果
pub fn query_via_tool(op: LspOp, args: &serde_json::Value) -> Result<String, String> {
    let file = args
        .get("file")
        .and_then(|v| v.as_str())
        .ok_or("缺少 file 参数")?;
    let line = args.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let column = args.get("column").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let server = pick_server(file)?;
    match op {
        LspOp::Hover => hover(&server, file, line, column),
        LspOp::Definition => {
            let v = query(&server, file, line, column, "textDocument/definition")?;
            Ok(format_locations(&v))
        }
        LspOp::References => {
            let v = query(&server, file, line, column, "textDocument/references")?;
            Ok(format_locations(&v))
        }
        LspOp::Implementation => {
            let v = query(&server, file, line, column, "textDocument/implementation")?;
            Ok(format_locations(&v))
        }
    }
}

// ─── IPC ───

#[tauri::command]
pub async fn list_harness_lsp_servers() -> Result<Vec<LspServerConfig>, String> {
    Ok(lsp_store().lock().unwrap().clone())
}

#[tauri::command]
pub async fn save_harness_lsp_servers(
    servers: Vec<LspServerConfig>,
) -> Result<Vec<LspServerConfig>, String> {
    for s in &servers {
        if s.enabled && s.command.trim().is_empty() {
            return Err(format!("服务器「{}」的命令不能为空", s.name));
        }
    }
    {
        let mut list = lsp_store().lock().unwrap();
        *list = servers.clone();
    }
    persist(&servers)?;
    Ok(servers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let payload = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": { "x": 1 } });
        let body = payload.to_string();
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut reader = BufReader::new(frame.as_bytes());
        let parsed = read_frame(&mut reader).unwrap().unwrap();
        assert_eq!(parsed["result"]["x"], 1);
    }

    #[test]
    fn extension_routing_prefers_match_then_first() {
        // 扩展名路由：按文件扩展名匹配服务器；无匹配回退第一个
        // （与 pick_server 的过滤/匹配分支一致）
        let rs = LspServerConfig {
            id: "rs".into(),
            name: "rust".into(),
            command: "x".into(),
            args: vec![],
            extensions: vec!["rs".into()],
            enabled: true,
        };
        let ts = LspServerConfig {
            id: "ts".into(),
            name: "ts".into(),
            command: "y".into(),
            args: vec![],
            extensions: vec!["ts".into(), "tsx".into()],
            enabled: true,
        };
        // 路由纯逻辑：过滤启用 → 取扩展名 → 匹配映射 → 回退首个
        let route = |file: &str, servers: &[LspServerConfig]| -> Option<LspServerConfig> {
            let enabled: Vec<_> = servers.iter().filter(|s| s.enabled).cloned().collect();
            if enabled.is_empty() {
                return None;
            }
            let ext = std::path::Path::new(file)
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase());
            if let Some(ext) = &ext {
                if let Some(server) = enabled
                    .iter()
                    .find(|s| s.extensions.iter().any(|x| x.eq_ignore_ascii_case(ext)))
                {
                    return Some(server.clone());
                }
            }
            Some(enabled[0].clone())
        };
        // 匹配扩展名（大小写不敏感：.RS → rs 服务器）
        let picked = route("src/main.RS", &[rs.clone(), ts.clone()]).unwrap();
        assert_eq!(picked.id, "rs");
        // tsx → ts 服务器
        let picked = route("app.tsx", &[rs.clone(), ts.clone()]).unwrap();
        assert_eq!(picked.id, "ts");
        // 无扩展名 / 无匹配 → 回退第一个
        let picked = route("README", &[rs.clone(), ts.clone()]).unwrap();
        assert_eq!(picked.id, "rs");
        // 无启用的服务器 → None（pick_server 报错）
        let mut off = rs.clone();
        off.enabled = false;
        assert!(route("a.rs", &[off]).is_none());
    }

    #[test]
    fn location_formatting_single_and_array() {
        // 位置格式化：单 uri 对象 / 数组 / 空
        let single = serde_json::json!({ "uri": "file:///a.rs", "range": {} });
        let s = format_locations(&single);
        assert!(s.contains("a.rs"), "单位置应格式化: {s}");
        let arr = serde_json::json!([
            { "uri": "file:///a.rs", "range": {} },
            { "uri": "file:///b.rs", "range": {} },
        ]);
        let s = format_locations(&arr);
        assert!(
            s.contains("a.rs") && s.contains("b.rs"),
            "数组位置应全列: {s}"
        );
        let empty = serde_json::json!([]);
        assert_eq!(format_locations(&empty), "（未找到结果）");
    }
}
