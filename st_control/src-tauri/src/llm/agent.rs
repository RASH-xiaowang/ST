// ============================================================
// 大模型 — 代理循环（工具调用，DeepSeek Harness 能力迁移）
//
// 把 DSH 对话中的「模型 + 工具」能力迁移到本应用：
// - 工具注册表：内置工具（web_search / fetch_web_page / get_current_time /
//   search_knowledge_base / 文件读写列目录 / exec_command 命令执行）
// - 审批门控：危险工具执行前弹出审批，支持「会话内记住批准」
// - chat_agent_stream：模型调用 → tool_calls → 执行工具 → 结果回传
//   → 继续循环，直到模型给出最终回答；全程通过 Channel 推事件
// ============================================================

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

use crate::llm::types::ToolCall;

// ─── 工具注册表 ───

/// 工具执行函数（同步；在代理循环内调用，耗时操作请自行控制超时）。
/// app 为可选的 AppHandle：内置工具需要访问应用状态（如知识库）时使用，
/// 测试环境可传 None。
pub(crate) type ToolFn = fn(Option<tauri::AppHandle>, Value) -> Result<String, String>;

/// 工具执行器：函数指针或捕获环境的闭包（MCP 等外部工具需要捕获配置）
#[derive(Clone)]
pub enum ToolRunner {
    Fn(ToolFn),
    Dyn(
        std::sync::Arc<
            dyn Fn(Option<tauri::AppHandle>, Value) -> Result<String, String> + Send + Sync,
        >,
    ),
}

impl ToolRunner {
    pub fn call(&self, app: Option<tauri::AppHandle>, args: Value) -> Result<String, String> {
        match self {
            ToolRunner::Fn(f) => f(app, args),
            ToolRunner::Dyn(f) => f(app, args),
        }
    }
}

pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    /// 危险工具：执行前需要用户审批
    pub requires_approval: bool,
    pub run: ToolRunner,
}

impl Clone for ToolSpec {
    fn clone(&self) -> Self {
        ToolSpec {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self.parameters.clone(),
            requires_approval: self.requires_approval,
            run: self.run.clone(),
        }
    }
}

fn registry() -> &'static Mutex<HashMap<String, ToolSpec>> {
    static REG: OnceLock<Mutex<HashMap<String, ToolSpec>>> = OnceLock::new();
    REG.get_or_init(|| {
        let mut m: HashMap<String, ToolSpec> = HashMap::new();
        for t in builtin_tools() {
            m.insert(t.name.clone(), t);
        }
        Mutex::new(m)
    })
}

/// 注册/覆盖一个工具（插件系统接入点，第二轮启用）
#[allow(dead_code)]
pub fn register_tool(t: ToolSpec) {
    if let Ok(mut m) = registry().lock() {
        m.insert(t.name.clone(), t);
    }
}

/// 全部工具的 OpenAI tools 定义（插件工具优先，同名内置工具被遮蔽；
/// 多个插件间同名时先注册的启用插件生效——与 execute_tool 的解析顺序一致，
/// 避免重复工具名导致上游 API 报错）
pub fn tools_json() -> Value {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut arr: Vec<Value> = Vec::new();
    for (_pid, t) in crate::llm::agent_plugins::enabled_plugin_tools() {
        if seen.insert(t.name.clone()) {
            arr.push(json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            }));
        }
    }
    let m = registry().lock().unwrap();
    for t in m.values() {
        if seen.insert(t.name.clone()) {
            arr.push(json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            }));
        }
    }
    Value::Array(arr)
}

/// 供前端展示的工具目录
#[derive(Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub requires_approval: bool,
}

pub fn tool_infos() -> Vec<ToolInfo> {
    let m = registry().lock().unwrap();
    let mut v: Vec<ToolInfo> = m
        .values()
        .map(|t| ToolInfo {
            name: t.name.clone(),
            description: t.description.clone(),
            requires_approval: t.requires_approval,
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

// ─── 工作区沙箱 ───

/// 工作区子目录根（显式工作区目录存放处；默认工作区在应用项目根）
pub(crate) fn workspace_root() -> std::path::PathBuf {
    crate::common::st_data_dir().join("agent_workspace")
}

/// 把路径规范化并校验必须位于当前沙箱内（默认工作区 = 应用项目根）。
/// 目标文件可尚不存在（写入场景）：规范化其父目录并校验归属。
pub(crate) fn safe_join(user_path: &str) -> Result<std::path::PathBuf, String> {
    use std::path::{Path, PathBuf};
    let root = crate::harness::workspace::sandbox_root();
    std::fs::create_dir_all(&root).map_err(|e| format!("创建工作区失败: {}", e))?;
    if user_path.trim().is_empty() {
        return Ok(root);
    }
    let raw = if Path::new(user_path).is_absolute() {
        PathBuf::from(user_path)
    } else {
        root.join(user_path)
    };
    // 目标已存在（目录列表/读取场景）：直接规范化目标自身并校验归属。
    // 修复：此前一律校验「父目录」，导致列出工作区根本身（父目录在根外）被误拒。
    if raw.exists() {
        let canon = raw
            .canonicalize()
            .map_err(|e| format!("路径不存在或不可访问: {}", e))?;
        let root_canon = root
            .canonicalize()
            .map_err(|e| format!("工作区异常: {}", e))?;
        if !canon.starts_with(&root_canon) {
            return Err("路径超出允许的工作区范围".to_string());
        }
        return Ok(canon);
    }
    let parent = raw
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| raw.clone());
    // 找最深已存在祖先并规范化，校验在根内
    let mut probe = parent.clone();
    while !probe.exists() {
        if !probe.pop() {
            break;
        }
    }
    let canon = probe
        .canonicalize()
        .map_err(|e| format!("路径不存在或不可访问: {}", e))?;
    let root_canon = root
        .canonicalize()
        .map_err(|e| format!("工作区异常: {}", e))?;
    if !canon.starts_with(&root_canon) {
        return Err("路径超出允许的工作区范围".to_string());
    }
    // 补回未存在的相对部分，再次校验防 .. 残留
    let rel = parent
        .strip_prefix(&probe)
        .map(|r| r.to_path_buf())
        .unwrap_or_default();
    let parent_norm = canon.join(rel);
    if !parent_norm.starts_with(&root_canon) {
        return Err("路径超出允许的工作区范围".to_string());
    }
    Ok(parent_norm.join(raw.file_name().unwrap_or_default()))
}

// ─── 内置工具 ───

pub(crate) fn tool_web_search(
    _app: Option<tauri::AppHandle>,
    args: Value,
) -> Result<String, String> {
    // Consumer：联网搜索能力统一经 Harness WebService（DSH 能力接缝）。
    // DSH 2026-08-17 web-search-multiple-queries：接受 queries 数组（最多 4 个），
    // 逐查询搜索并按查询分组标注，URL 跨查询去重；单查询保持原返回格式。
    let queries: Vec<String> = args
        .get("queries")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|q| q.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .or_else(|| {
            args.get("query")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(|s| vec![s.trim().to_string()])
        })
        .unwrap_or_default();
    if queries.is_empty() {
        return Err("缺少 queries 参数（至少一个搜索词）".to_string());
    }
    if queries.len() > 4 {
        return Err("queries 最多 4 个".to_string());
    }
    // 精确重复去重（保留首位置，DSH 语义）
    let mut seen_q = std::collections::HashSet::new();
    let queries: Vec<String> = queries
        .into_iter()
        .filter(|q| seen_q.insert(q.clone()))
        .collect();
    if queries.len() == 1 {
        return crate::harness::web::WebService.search(&queries[0]);
    }
    let mut out: Vec<String> = Vec::new();
    let mut seen_url = std::collections::HashSet::new();
    for q in &queries {
        out.push(format!("### {q}"));
        let raw = crate::harness::web::WebService.search(q)?;
        let parsed: Vec<Value> = serde_json::from_str(&raw).unwrap_or_default();
        let mut shown = 0usize;
        for item in parsed {
            if shown >= 8 {
                break;
            }
            let url = item
                .get("url")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string();
            if !url.is_empty() && !seen_url.insert(url.clone()) {
                continue; // 跨查询 URL 去重
            }
            let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("");
            let snippet = item.get("snippet").and_then(|s| s.as_str()).unwrap_or("");
            out.push(format!("- {title}：{snippet}（{url}）"));
            shown += 1;
        }
    }
    Ok(out.join("\n"))
}
pub(crate) fn tool_read_file(
    _app: Option<tauri::AppHandle>,
    args: Value,
) -> Result<String, String> {
    // Consumer：文件系统能力统一经 Harness FsService（DSH 能力接缝）。
    // DSH read(file_path, offset?, limit?)：1-based 行窗口 + 行号输出
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(200)
        .clamp(1, 2000) as usize;
    let (rows, total) = crate::harness::fs::FsService.read_lines(
        path,
        offset,
        limit,
        &crate::harness::fs::FsPolicy::current(),
    )?;
    if rows.is_empty() {
        return Ok(format!("（共 {total} 行，当前窗口无内容）"));
    }
    let mut out: Vec<String> = rows.iter().map(|(n, t)| format!("{n}: {t}")).collect();
    out.push(format!("…（共 {total} 行，显示 {} 行）", rows.len()));
    Ok(out.join("\n"))
}

pub(crate) fn tool_write_file(
    _app: Option<tauri::AppHandle>,
    args: Value,
) -> Result<String, String> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let n = crate::harness::fs::FsService.write_text(
        path,
        content,
        &crate::harness::fs::FsPolicy::current(),
    )?;
    Ok(format!("已写入 {}（{} 字节）", path, n))
}

pub(crate) fn tool_list_dir(_app: Option<tauri::AppHandle>, args: Value) -> Result<String, String> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let entries =
        crate::harness::fs::FsService.list_dir(path, &crate::harness::fs::FsPolicy::current())?;
    Ok(entries
        .iter()
        .map(|e| {
            if e.is_dir {
                format!("{}/", e.name)
            } else {
                e.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

pub(crate) fn tool_edit_file(
    _app: Option<tauri::AppHandle>,
    args: Value,
) -> Result<String, String> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let old_string = args
        .get("old_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new_string = args
        .get("new_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let replace_all = args
        .get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let n = crate::harness::fs::FsService.edit_text(
        path,
        old_string,
        new_string,
        replace_all,
        &crate::harness::fs::FsPolicy::current(),
    )?;
    Ok(format!("已替换 {n} 处（{path}）"))
}

/// DSH tool-str-replace-editor 迁移：四命令编辑器
/// （view / create / str_replace / insert）。
/// - view：文件 → 带行号视图（支持 view_range=[start,end]，end=-1 到文件尾）；
///   目录 → 2 层深列表（跳过隐藏项与 node_modules 等重目录）
/// - create：file_text 创建新文件（路径已存在时报错，不覆盖）
/// - str_replace：old_str 唯一匹配替换（多处匹配报错列出所在行；new_str 可为空）
/// - insert：在 insert_line 行之后插入 new_str（insert_line ∈ [0, 行数]）
///   输出按 maxOutputChars（默认 16000）截断并标注 <response clipped>。
pub(crate) fn tool_str_replace_editor(
    _app: Option<tauri::AppHandle>,
    args: Value,
) -> Result<String, String> {
    let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let policy = crate::harness::fs::FsPolicy::current();
    match command {
        "view" => {
            let view_range = args
                .get("view_range")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_i64()).collect::<Vec<_>>());
            crate::harness::fs::FsService.str_replace_view(path, view_range, &policy)
        }
        "create" => {
            let file_text = args
                .get("file_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "参数 file_text 为 create 命令所必需".to_string())?;
            crate::harness::fs::FsService.create_if_absent(path, file_text, &policy)
        }
        "str_replace" => {
            let old_str = args
                .get("old_str")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "参数 old_str 为 str_replace 命令所必需".to_string())?;
            let new_str = args.get("new_str").and_then(|v| v.as_str()).unwrap_or("");
            crate::harness::fs::FsService.str_replace(path, old_str, new_str, &policy)
        }
        "insert" => {
            let insert_line = args
                .get("insert_line")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "参数 insert_line 为 insert 命令所必需".to_string())?;
            let new_str = args
                .get("new_str")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "参数 new_str 为 insert 命令所必需".to_string())?;
            crate::harness::fs::FsService.insert_lines(path, insert_line, new_str, &policy)
        }
        other => Err(format!(
            "未知命令: {}（允许 view/create/str_replace/insert）",
            other
        )),
    }
}

pub(crate) fn tool_glob(_app: Option<tauri::AppHandle>, args: Value) -> Result<String, String> {
    let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let hits = crate::harness::fs::FsService.glob(
        pattern,
        path,
        &crate::harness::fs::FsPolicy::current(),
    )?;
    if hits.is_empty() {
        Ok("（无匹配文件）".to_string())
    } else {
        Ok(hits.join("\n"))
    }
}

pub(crate) fn tool_grep(_app: Option<tauri::AppHandle>, args: Value) -> Result<String, String> {
    let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
    if pattern.is_empty() {
        return Err("缺少 pattern 参数".to_string());
    }
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let include = args.get("include").and_then(|v| v.as_str()).unwrap_or("");
    let case_insensitive = args
        .get("case_insensitive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    crate::harness::fs::FsService.grep(
        pattern,
        path,
        include,
        case_insensitive,
        &crate::harness::fs::FsPolicy::current(),
    )
}

pub(crate) fn tool_read_image(
    _app: Option<tauri::AppHandle>,
    args: Value,
) -> Result<String, String> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    crate::harness::fs::FsService.read_image_base64(path, &crate::harness::fs::FsPolicy::current())
}

pub(crate) fn tool_exec_command(
    _app: Option<tauri::AppHandle>,
    args: Value,
) -> Result<String, String> {
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if command.is_empty() {
        return Err("缺少 command 参数".to_string());
    }
    // 超时（L2）：用 Harness 可配置设置（5~300s，默认 30）替代硬编码 30s，
    // 避免「守卫等 300s 但进程 30s 已被杀」的报错与事实不符
    let timeout_secs = crate::harness::settings::current().effective_timeout_secs();
    // 受限执行世界：默认锚定当前工作区目录；sandbox_permissions 升级
    // 由 Harness 会话运行时在派发前处理（审批 + 越界放行），此处仅执行
    let cwd = crate::harness::workspace::current().dir;
    let cwd = crate::harness::workspace::workspace_dir(&cwd);
    let effective = format!("Set-Location -LiteralPath '{}'; {}", cwd.display(), command);
    // 输出重定向到临时文件：进程持续写入不占管道缓冲，避免大输出死锁，
    // 也便于超时终止后仍能取回已产生的部分输出
    let tag = uuid::Uuid::new_v4().simple();
    let out_path = std::env::temp_dir().join(format!("st-exec-{tag}.out"));
    let err_path = std::env::temp_dir().join(format!("st-exec-{tag}.err"));
    let out_file =
        std::fs::File::create(&out_path).map_err(|e| format!("创建输出文件失败: {}", e))?;
    let err_file =
        std::fs::File::create(&err_path).map_err(|e| format!("创建输出文件失败: {}", e))?;
    let mut child = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &effective])
        .stdout(std::process::Stdio::from(out_file))
        .stderr(std::process::Stdio::from(err_file))
        .spawn()
        .map_err(|e| format!("执行失败: {}", e))?;
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() > deadline {
                    // 进程树级终止（DSH subprocess 语义）
                    if !crate::harness::shell::kill_tree(child.id()) {
                        let _ = child.kill();
                    }
                    let _ = child.wait();
                    timed_out = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                if !crate::harness::shell::kill_tree(child.id()) {
                    let _ = child.kill();
                }
                let _ = child.wait();
                let _ = std::fs::remove_file(&out_path);
                let _ = std::fs::remove_file(&err_path);
                return Err(format!("等待命令失败: {}", e));
            }
        }
    }
    let mut text = read_lossy(&out_path).unwrap_or_default();
    let err_text = read_lossy(&err_path).unwrap_or_default();
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&err_path);
    if !err_text.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str("[stderr] ");
        text.push_str(&err_text);
    }
    // 字符边界安全截断：字节偏移落在多字节字符（中文等）中间会 panic
    let text = if text.len() > 8192 {
        let end = text.floor_char_boundary(8192);
        format!("{}…（输出过长已截断）", &text[..end])
    } else {
        text
    };
    if timed_out {
        return Err(format!(
            "命令执行超时（{} 秒），进程已强制终止。已产生的输出：\n{}",
            timeout_secs,
            if text.is_empty() {
                "（无输出）".to_string()
            } else {
                text
            }
        ));
    }
    Ok(if text.is_empty() {
        "命令执行完成（无输出）".to_string()
    } else {
        text
    })
}

fn read_lossy(path: &std::path::Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// 获取当前本地时间（模型需要"现在几点/今天几号"时使用）
pub(crate) fn tool_get_current_time(
    _app: Option<tauri::AppHandle>,
    _args: Value,
) -> Result<String, String> {
    let now = chrono::Local::now();
    Ok(now.format("%Y-%m-%d %H:%M:%S %A（UTC%:z）").to_string())
}

/// 检索本地知识库（BM25/FTS，只读）：返回与查询相关的文档分片
pub(crate) fn tool_search_kb(app: Option<tauri::AppHandle>, args: Value) -> Result<String, String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if query.is_empty() {
        return Err("缺少 query 参数".to_string());
    }
    let top_k = args
        .get("top_k")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .clamp(1, 10) as usize;
    let Some(app) = app else {
        return Err("内部错误：缺少应用上下文".to_string());
    };
    let db = app.state::<crate::kb::db::KbDatabase>();
    // 当前登录用户优先；未登录时回退到默认管理员身份（单机部署）
    let uid = match app.state::<crate::kb::auth::UserSession>().get() {
        Some(u) => u.id,
        None => crate::kb::auth::default_admin(&db)
            .map(|u| u.id)
            .ok_or("知识库尚未初始化用户".to_string())?,
    };
    let visible = crate::kb::retrieval::visible_kb_ids(&db, uid);
    if visible.is_empty() {
        return Err("当前没有可访问的知识库（请先在知识库模块登录并导入文档）".to_string());
    }
    let hits = crate::kb::retrieval::bm25_search(&db, &query, &visible, top_k)?;
    if hits.is_empty() {
        return Ok("[]".to_string());
    }
    let arr: Vec<Value> = hits
        .into_iter()
        .map(|h| {
            json!({
                "doc_title": h.doc_title,
                "score": h.score,
                "content": truncate_str(&h.content, 400),
            })
        })
        .collect();
    serde_json::to_string(&arr).map_err(|e| e.to_string())
}

/// 抓取网页正文（只读）：返回去标签后的纯文本摘要
pub(crate) fn tool_fetch_web_page(
    _app: Option<tauri::AppHandle>,
    args: Value,
) -> Result<String, String> {
    // Consumer：网页抓取能力统一经 Harness WebService
    let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
    crate::harness::web::WebService.fetch(url)
}

pub(crate) fn builtin_tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "web_search".to_string(),
            description: "联网搜索：可一次传入多个搜索词（queries 数组，最多 4 个），逐查询返回标题/链接/摘要并按查询分组标注。当需要最新信息、事实核查或多个角度时使用。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "queries": { "type": "array", "items": { "type": "string" }, "description": "搜索词列表（1-4 个，精确重复自动去重）" },
                },
                "required": ["queries"],
            }),
            requires_approval: false,
            run: ToolRunner::Fn(tool_web_search),
        },
        ToolSpec {
            name: "read_file".to_string(),
            description: "读取当前工作区内指定路径的文本文件内容（默认工作区 = 应用项目根，可读自身源码；越界需政策放行）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "相对或绝对路径" }, "offset": { "type": "integer", "description": "起始行号（1-based，默认 1）" }, "limit": { "type": "integer", "description": "最多返回行数（1-2000，默认 200）" } },
                "required": ["path"],
            }),
            requires_approval: false,
            run: ToolRunner::Fn(tool_read_file),
        },
        ToolSpec {
            name: "write_file".to_string(),
            description: "把文本内容写入当前工作区内指定路径的文件（默认工作区 = 应用项目根，可写自身源码；越界需政策放行）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对或绝对路径" },
                    "content": { "type": "string", "description": "要写入的完整内容" },
                },
                "required": ["path", "content"],
            }),
            requires_approval: false,
            run: ToolRunner::Fn(tool_write_file),
        },
        ToolSpec {
            name: "list_dir".to_string(),
            description: "列出当前工作区内指定目录的文件与子目录（默认工作区 = 应用项目根）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "相对或绝对路径，空为工作区根目录" } },
                "required": ["path"],
            }),
            requires_approval: false,
            run: ToolRunner::Fn(tool_list_dir),
        },
        ToolSpec {
            name: "exec_command".to_string(),
            description: "在本机执行一条 PowerShell 命令并返回输出（危险操作，执行前需用户审批；超时 30 秒自动终止；run_in_background=true 时转为后台作业并立即返回作业 id，可用 job_output/job_kill 管理）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "要执行的 PowerShell 命令" },
                    "run_in_background": { "type": "boolean", "description": "true 时后台执行，返回作业 id（默认 false）" },
                    "sandbox_permissions": { "type": "string", "enum": ["workspace-write", "danger-full-access"], "description": "请求升级执行权限（超出当前沙箱模式时需审批）" },
                    "justification": { "type": "string", "description": "升级权限的理由说明" },
                },
                "required": ["command"],
            }),
            requires_approval: true,
            run: ToolRunner::Fn(tool_exec_command),
        },
        ToolSpec {
            name: "edit_file".to_string(),
            description: "对工作区内文本文件做字面替换编辑：old_string 需唯一匹配（多处匹配时传 replace_all=true）。修改前先 read_file 确认内容。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对或绝对路径" },
                    "old_string": { "type": "string", "description": "待替换的原文（字面匹配）" },
                    "new_string": { "type": "string", "description": "替换后的新文本" },
                    "replace_all": { "type": "boolean", "description": "是否替换全部匹配（默认 false）" },
                },
                "required": ["path", "old_string", "new_string"],
            }),
            requires_approval: false,
            run: ToolRunner::Fn(tool_edit_file),
        },
        ToolSpec {
            name: "str_replace_editor".to_string(),
            description: "查看/创建/编辑文件的四命令编辑器（DSH str_replace_editor）：command 可选 view（带行号查看文件，view_range 可指定行区间；路径为目录时列出 2 层内文件）、create（file_text 创建新文件，已存在则不覆盖）、str_replace（old_str 唯一匹配替换为 new_str，多处匹配时报错列行号）、insert（在 insert_line 行后插入 new_str）。修改前先用 view 确认内容。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "enum": ["view", "create", "str_replace", "insert"], "description": "要执行的命令" },
                    "path": { "type": "string", "description": "文件或目录路径（相对或绝对）" },
                    "file_text": { "type": "string", "description": "create 命令的文件内容" },
                    "insert_line": { "type": "integer", "description": "insert 命令：在指定行号之后插入（0 = 文件开头）" },
                    "new_str": { "type": "string", "description": "str_replace 的新文本 / insert 的插入文本" },
                    "old_str": { "type": "string", "description": "str_replace 的待替换原文（须唯一匹配）" },
                    "view_range": { "type": "array", "items": { "type": "integer" }, "description": "view 命令的行区间 [start, end]，end=-1 表示到文件尾（默认显示全文）" },
                },
                "required": ["command", "path"],
            }),
            requires_approval: false,
            run: ToolRunner::Fn(tool_str_replace_editor),
        },
        ToolSpec {
            name: "glob".to_string(),
            description: "按 glob 模式在工作区内发现文件/目录（支持 **、*、?、[...] 字符类、{a,b} 交替，如 src/**/*.{ts,js}），返回匹配路径列表（最多 200 条）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "glob 模式，如 **/*.rs" },
                    "path": { "type": "string", "description": "搜索根目录（空 = 工作区根，相对路径锚定工作区）" },
                },
                "required": ["pattern"],
            }),
            requires_approval: false,
            run: ToolRunner::Fn(tool_glob),
        },
        ToolSpec {
            name: "grep".to_string(),
            description: "在工作区内按正则表达式搜索文件内容，返回 file:line:内容 列表（最多 200 条；二进制与超大文件自动跳过）。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "正则表达式" },
                    "path": { "type": "string", "description": "文件或目录路径，空为工作区根" },
                    "include": { "type": "string", "description": "正向 glob 过滤器：仅搜索路径匹配该 glob 的文件（如 *.rs）" },
                    "case_insensitive": { "type": "boolean", "description": "true 时忽略大小写（默认 false）" },
                },
                "required": ["pattern"],
            }),
            requires_approval: false,
            run: ToolRunner::Fn(tool_grep),
        },
        ToolSpec {
            name: "read_image".to_string(),
            description: "读取工作区内图片（png/jpg/webp/gif，≤4MB）为 base64 data URL，可作视觉输入引用。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "图片相对或绝对路径" } },
                "required": ["path"],
            }),
            requires_approval: false,
            run: ToolRunner::Fn(tool_read_image),
        },
        ToolSpec {
            name: "get_current_time".to_string(),
            description: "获取当前本地日期与时间（含星期与时区），回答时间相关问题前使用。".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            requires_approval: false,
            run: ToolRunner::Fn(tool_get_current_time),
        },
        ToolSpec {
            name: "search_knowledge_base".to_string(),
            description: "检索本地知识库（BM25 全文检索，只读）：返回与查询最相关的文档分片（标题/得分/内容），用于回答需要企业内部资料的问题。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "检索关键词或问题" },
                    "top_k": { "type": "integer", "description": "返回分片数（1-10，默认 5）" },
                },
                "required": ["query"],
            }),
            requires_approval: false,
            run: ToolRunner::Fn(tool_search_kb),
        },
        ToolSpec {
            name: "fetch_web_page".to_string(),
            description: "抓取指定 http/https 网页并返回去标签后的正文摘要（只读，最多 8000 字符），用于阅读链接内容。".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "url": { "type": "string", "description": "要抓取的网页地址" } },
                "required": ["url"],
            }),
            requires_approval: false,
            run: ToolRunner::Fn(tool_fetch_web_page),
        },
    ]
}

// ─── 审批流 ───

pub struct PendingApproval {
    /// pending / approved / rejected
    pub status: String,
}

fn approvals() -> &'static Mutex<HashMap<String, PendingApproval>> {
    static P: OnceLock<Mutex<HashMap<String, PendingApproval>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

fn new_approval_id() -> String {
    format!("apr-{}", uuid::Uuid::new_v4().simple())
}

/// 「会话内记住批准」的信任有效期（秒）：同一 (提供方, 模型, 工具) 组合在此时间内不再审批
const APPROVAL_TRUST_TTL_SECS: u64 = 1800;

/// 信任键：(provider_id, model, tool)
type TrustKey = (String, String, String);

/// 信任表：键 → 信任开始时间
fn trusted_tools() -> &'static Mutex<HashMap<TrustKey, Instant>> {
    static T: OnceLock<Mutex<HashMap<TrustKey, Instant>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 该工具在当前会话是否已被用户「记住批准」且未过期（过期条目顺手清理）
fn is_trusted(provider_id: &str, model: &str, tool: &str) -> bool {
    let now = Instant::now();
    let mut m = trusted_tools().lock().unwrap();
    m.retain(|_, t| now.duration_since(*t).as_secs() < APPROVAL_TRUST_TTL_SECS);
    m.contains_key(&(provider_id.to_string(), model.to_string(), tool.to_string()))
}

/// 提交审批请求并等待用户决定（最长 10 分钟）。
/// 若工具已被「会话内记住批准」，直接放行不再弹窗。
async fn request_approval(
    app: &tauri::AppHandle,
    provider_id: &str,
    model: &str,
    tool: &str,
    args: &Value,
) -> Result<(), String> {
    if is_trusted(provider_id, model, tool) {
        return Ok(());
    }
    let id = new_approval_id();
    let description = format!(
        "工具「{}」需要你的批准：{}",
        tool,
        truncate_str(&args.to_string(), 200)
    );
    approvals().lock().unwrap().insert(
        id.clone(),
        PendingApproval {
            status: "pending".to_string(),
        },
    );
    let _ = app.emit(
        "agent-approval-requested",
        json!({
            "id": id,
            "tool": tool,
            "description": description,
            "arguments": args.to_string(),
        }),
    );

    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        // 「记住并批准」可在弹窗出现后随时生效：信任一旦建立立即放行
        if is_trusted(provider_id, model, tool) {
            approvals().lock().unwrap().remove(&id);
            return Ok(());
        }
        let status = approvals()
            .lock()
            .unwrap()
            .get(&id)
            .map(|a| a.status.clone())
            .unwrap_or_else(|| "cancelled".to_string());
        match status.as_str() {
            "approved" => {
                approvals().lock().unwrap().remove(&id);
                return Ok(());
            }
            "rejected" => {
                approvals().lock().unwrap().remove(&id);
                return Err("用户拒绝了该操作".to_string());
            }
            _ => {}
        }
        if Instant::now() > deadline {
            approvals().lock().unwrap().remove(&id);
            return Err("审批超时（10 分钟）".to_string());
        }
    }
}

pub(crate) fn truncate_str(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        s.to_string()
    } else {
        format!("{}…", chars[..n].iter().collect::<String>())
    }
}

// ─── 代理循环 ───

const MAX_AGENT_ROUNDS: usize = 6;

fn emit(ch: &tauri::ipc::Channel<String>, v: Value) {
    let _ = ch.send(v.to_string());
}

/// 执行单个工具调用，返回 (工具名, 结果文本, 是否成功, 耗时毫秒)
async fn execute_tool(
    app: &tauri::AppHandle,
    provider_id: &str,
    model: &str,
    call: &ToolCall,
) -> (String, String, bool, u64) {
    let name = call.function.name.clone();
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(json!({}));
    let started = Instant::now();
    // 插件工具：由前端 WebView 执行（DSH Client 插件同信任级别）
    if let Some((_pid, ptool)) = crate::llm::agent_plugins::find_plugin_tool(&name) {
        if ptool.requires_approval {
            if let Err(e) = request_approval(app, provider_id, model, &name, &args).await {
                return (name, e, false, started.elapsed().as_millis() as u64);
            }
        }
        let (ok, text) = crate::llm::agent_plugins::run_plugin_tool(
            app,
            &call.id,
            &name,
            &call.function.arguments,
            &ptool.code,
        )
        .await;
        return (name, text, ok, started.elapsed().as_millis() as u64);
    }
    let spec = registry().lock().unwrap().get(&name).cloned();
    let Some(spec) = spec else {
        return (
            name.clone(),
            format!("未知工具: {}", name),
            false,
            started.elapsed().as_millis() as u64,
        );
    };
    if spec.requires_approval {
        if let Err(e) = request_approval(app, provider_id, model, &name, &args).await {
            return (name, e, false, started.elapsed().as_millis() as u64);
        }
    }
    // 工具在阻塞线程池执行，避免卡住异步运行时
    let app2 = app.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || spec.run.call(Some(app2), args)).await;
    let (ok, text) = match result {
        Ok(Ok(t)) => (true, t),
        Ok(Err(e)) => (false, e),
        Err(e) => (false, format!("工具执行异常: {}", e)),
    };
    let duration_ms = started.elapsed().as_millis() as u64;
    if !ok {
        log::warn!(
            "[agent] 工具 {} 失败（{}ms）: {}",
            name,
            duration_ms,
            truncate_str(&text, 200)
        );
    }
    (name, text, ok, duration_ms)
}

/// 代理式对话：带工具调用的多轮循环，通过 Channel 推送事件。
/// 事件类型：
///   tool_start {id,name,arguments} / tool_done {id,name,ok,result,duration_ms}
///   approval_requested（同时以 tauri 事件 agent-approval-requested 推送，
///   含完整 arguments 供审批卡展示）
///   delta {content} / done {content,...} / error {message}
#[tauri::command]
pub async fn chat_agent_stream(
    app: tauri::AppHandle,
    request: crate::llm::types::ChatRequest,
    on_chunk: tauri::ipc::Channel<String>,
) -> Result<(), String> {
    let provider_id = request
        .provider_id
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            crate::llm::config::load_config()
                .default_provider_id
                .clone()
        })
        .ok_or_else(|| "未指定提供方，且未配置全局默认提供方".to_string())?;

    let cfg = crate::llm::config::load_config();
    let provider = cfg
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| "指定的提供方不存在".to_string())?
        .clone();

    let model = request
        .model
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| provider.default_model.clone());
    if model.is_empty() {
        return Err("未指定模型，且提供方未配置默认模型".to_string());
    }

    // 配额管控（与普通聊天一致）
    let usage = crate::llm::config::current_month_usage(&provider_id);
    if let Some(limit) = provider.monthly_token_limit {
        if usage.total_tokens >= limit {
            let msg = format!("该提供方本月 token 配额已用尽（上限 {}）", limit);
            emit(&on_chunk, json!({ "type": "error", "message": msg }));
            return Err(msg);
        }
    }

    let mut base_messages = request.messages.clone();
    let role_id: Option<String> = request.role_id.clone().filter(|s| !s.is_empty());
    if role_id.is_some() {
        crate::llm::handlers::inject_role_system_prompt(&mut base_messages, &role_id);
    }
    // 代理循环内部使用原始 JSON 消息：可携带 assistant tool_calls / role=tool
    let mut messages: Vec<Value> = base_messages
        .iter()
        .map(crate::llm::client::chat::build_message)
        .collect();

    let tools = tools_json();
    let mut final_content = String::new();
    let mut total_prompt = 0u64;
    let mut total_completion = 0u64;

    for _round in 1..=MAX_AGENT_ROUNDS {
        let comp = crate::llm::client::chat_completion_with_tools_raw(
            &provider,
            &model,
            &messages,
            request.max_tokens,
            request.temperature,
            request.top_p,
            request.presence_penalty,
            request.frequency_penalty,
            &tools,
            "auto",
        )
        .await
        .map_err(|e| {
            emit(
                &on_chunk,
                json!({ "type": "error", "message": format!("模型调用失败: {}", e) }),
            );
            e
        })?;
        let content = comp.content;
        let tool_calls = comp.tool_calls;
        let prompt = comp.prompt_tokens;
        let completion = comp.completion_tokens;
        total_prompt += prompt;
        total_completion += completion;

        match tool_calls {
            Some(calls) if !calls.is_empty() => {
                // 把 assistant 的工具调用写入历史（OpenAI 格式）
                messages.push(json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": calls,
                }));
                // 逐个执行并回传结果
                for call in &calls {
                    emit(
                        &on_chunk,
                        json!({
                            "type": "tool_start",
                            "id": call.id,
                            "name": call.function.name,
                            "arguments": call.function.arguments,
                        }),
                    );
                    let (name, result, ok, duration_ms) =
                        execute_tool(&app, &provider_id, &model, call).await;
                    emit(
                        &on_chunk,
                        json!({
                            "type": "tool_done",
                            "id": call.id,
                            "name": name,
                            "ok": ok,
                            "result": truncate_str(&result, 4000),
                            "duration_ms": duration_ms,
                        }),
                    );
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call.id,
                        "content": result,
                    }));
                }
            }
            _ => {
                final_content = content;
                break;
            }
        }
    }

    if final_content.trim().is_empty() {
        let msg = "模型在多轮工具调用后仍未给出最终回答".to_string();
        emit(&on_chunk, json!({ "type": "error", "message": msg }));
        return Err(msg);
    }

    let cost = crate::llm::client::estimate_cost(&provider, total_prompt, total_completion);
    emit(
        &on_chunk,
        json!({ "type": "delta", "content": final_content.clone() }),
    );
    emit(
        &on_chunk,
        json!({
            "type": "done",
            "content": final_content,
            "model": model,
            "prompt_tokens": total_prompt,
            "completion_tokens": total_completion,
            "total_tokens": total_prompt + total_completion,
            "cost": cost,
        }),
    );
    Ok(())
}

/// 批准一个待审批的工具调用
#[tauri::command]
pub async fn approve_agent_tool(id: String) -> Result<bool, String> {
    Ok(set_approval_status(&id, "approved"))
}

/// 拒绝一个待审批的工具调用
#[tauri::command]
pub async fn reject_agent_tool(id: String) -> Result<bool, String> {
    Ok(set_approval_status(&id, "rejected"))
}

fn set_approval_status(id: &str, status: &str) -> bool {
    let mut m = approvals().lock().unwrap();
    if let Some(a) = m.get_mut(id) {
        if a.status == "pending" {
            a.status = status.to_string();
            return true;
        }
    }
    false
}

/// 「会话内记住批准」：同一 (提供方, 模型, 工具) 在有效期内不再弹审批
#[tauri::command]
pub async fn trust_agent_tool(
    provider_id: String,
    model: String,
    tool: String,
) -> Result<(), String> {
    if tool.trim().is_empty() {
        return Err("工具名不能为空".to_string());
    }
    trusted_tools()
        .lock()
        .unwrap()
        .insert((provider_id, model, tool), Instant::now());
    Ok(())
}

/// 清空某会话的信任记录（清空对话时调用）
#[tauri::command]
pub async fn clear_agent_trust(provider_id: String, model: String) -> Result<(), String> {
    trusted_tools()
        .lock()
        .unwrap()
        .retain(|(p, m, _), _| p != &provider_id || m != &model);
    Ok(())
}

/// 工具目录（前端展示）：插件工具优先，同名内置工具被遮蔽，名称去重
#[tauri::command]
pub async fn get_agent_tools() -> Result<Vec<ToolInfo>, String> {
    let mut v: Vec<ToolInfo> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (_pid, t) in crate::llm::agent_plugins::enabled_plugin_tools() {
        if seen.insert(t.name.clone()) {
            v.push(ToolInfo {
                name: t.name,
                description: t.description,
                requires_approval: t.requires_approval,
            });
        }
    }
    for t in tool_infos() {
        if seen.insert(t.name.clone()) {
            v.push(t);
        }
    }
    v.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_sandbox_blocks_escape() {
        let root = workspace_root();
        std::fs::create_dir_all(&root).ok();
        // 清理：文件实际落在沙箱根（safe_join 锚定 app base），非 agent_workspace
        let _ = std::fs::remove_file(safe_join("agent_test.txt").unwrap());
        // 正常路径
        assert!(safe_join("a.txt").is_ok());
        // 越界路径（.. 逃逸到上级目录）应被拒绝
        let escape = safe_join("../llm_config.json");
        assert!(escape.is_err(), "越界路径应被拒绝: {:?}", escape);
        // 绝对路径指向工作区外应被拒绝
        let abs_out = safe_join("C:/Windows/System32");
        assert!(abs_out.is_err(), "工作区外绝对路径应被拒绝");
    }

    #[test]
    fn tool_catalog_has_builtins() {
        let infos = tool_infos();
        assert!(infos.iter().any(|t| t.name == "web_search"));
        assert!(infos.iter().any(|t| t.name == "read_file"));
        assert!(infos.iter().any(|t| t.name == "write_file"));
        assert!(infos.iter().any(|t| t.name == "list_dir"));
        assert!(infos.iter().any(|t| t.name == "exec_command"));
        assert!(infos.iter().any(|t| t.name == "get_current_time"));
        assert!(infos.iter().any(|t| t.name == "search_knowledge_base"));
        assert!(infos.iter().any(|t| t.name == "fetch_web_page"));
        assert!(infos.iter().any(|t| t.name == "str_replace_editor"));
        assert!(
            infos
                .iter()
                .find(|t| t.name == "exec_command")
                .unwrap()
                .requires_approval,
            "exec_command 必须审批"
        );
    }

    #[test]
    fn file_tools_roundtrip_in_workspace() {
        // 用 UUID 唯一文件名：读-改-写策略下，失败运行残留的固定名文件会
        // 被视作「未观察的已存在文件」阻断下次写入（stale-file flake）
        let root = workspace_root();
        std::fs::create_dir_all(&root).ok();
        let name = format!("agent_test_{}.txt", uuid::Uuid::new_v4().simple());
        let r = tool_write_file(None, json!({ "path": name, "content": "你好" }));
        assert!(r.is_ok(), "{:?}", r);
        let read = tool_read_file(None, json!({ "path": name }));
        assert!(read.unwrap().contains("你好"), "read_file 应返回带行号内容");
        let list = tool_list_dir(None, json!({ "path": "" }));
        assert!(list.unwrap().contains(&name));
        let _ = std::fs::remove_file(safe_join(&name).unwrap());
    }

    #[test]
    fn time_tool_returns_local_time() {
        let r = tool_get_current_time(None, json!({}));
        assert!(r.is_ok(), "{:?}", r);
        // 格式示例：2025-01-01 12:00:00 星期三（UTC+08:00）
        assert!(r.unwrap().contains("UTC"), "应包含时区信息");
    }

    #[test]
    fn exec_tool_rejects_empty_and_runs_echo() {
        let empty = tool_exec_command(None, json!({ "command": "" }));
        assert!(empty.is_err(), "空命令应被拒绝");
        let r = tool_exec_command(None, json!({ "command": "Write-Output hello-agent" }));
        assert!(r.is_ok(), "{:?}", r);
        assert!(r.unwrap().contains("hello-agent"));
    }

    #[test]
    fn kb_tool_requires_app_context() {
        let r = tool_search_kb(None, json!({ "query": "测试" }));
        assert!(r.is_err(), "缺少应用上下文应报错: {:?}", r);
        assert!(r.unwrap_err().contains("上下文"));
    }

    #[test]
    fn tools_json_dedupes_plugin_name_collisions() {
        use crate::llm::agent_plugins::{plugins_store_mut, AgentPlugin, PluginToolDef};
        // 注册一个与内置工具同名的插件工具（模拟重复工具名）
        let pid = format!("test-{}", uuid::Uuid::new_v4().simple());
        let plugin = AgentPlugin {
            id: pid.clone(),
            name: "冲突测试".to_string(),
            description: "测试".to_string(),
            enabled: true,
            tools: vec![PluginToolDef {
                name: "web_search".to_string(),
                description: "插件版搜索".to_string(),
                parameters: json!({ "type": "object", "properties": {} }),
                requires_approval: false,
                code: "return 'x';".to_string(),
            }],
            versions: vec![],
            created_at: String::new(),
            updated_at: String::new(),
        };
        plugins_store_mut().push(plugin);
        let tools = tools_json();
        let arr = tools.as_array().unwrap();
        let hits = arr
            .iter()
            .filter(|t| t["function"]["name"] == "web_search")
            .count();
        // 清理并断言
        plugins_store_mut().retain(|p| p.id != pid);
        assert_eq!(hits, 1, "同名工具应去重，仅保留插件版 web_search");
        assert_eq!(
            arr.iter()
                .find(|t| t["function"]["name"] == "web_search")
                .unwrap()["function"]["description"],
            "插件版搜索",
            "插件工具应优先于同名内置工具"
        );
    }

    #[test]
    fn truncate_str_is_char_safe_with_chinese() {
        // H2 同类：中文内容按字符截断不切半字符（工具结果/标题截断共用）
        let s = "中文测试内容";
        assert_eq!(truncate_str(s, 100), s, "短于上限原样返回");
        let t = truncate_str(s, 3);
        assert_eq!(t, "中文测…", "按字符截断 + 省略号: {t}");
        // 有效 UTF-8（无 panic 即字符安全）
        assert!(String::from_utf8(t.into_bytes()).is_ok());
        // ASCII 混合
        let mixed = "abc中文def";
        let t = truncate_str(mixed, 4);
        assert_eq!(t, "abc中…");
        // 空字符串
        assert_eq!(truncate_str("", 5), "");
        // n=0
        assert_eq!(truncate_str("x", 0), "…");
    }
}
