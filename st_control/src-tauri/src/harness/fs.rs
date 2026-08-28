// ============================================================
// Harness — 文件系统能力（DSH fs 迁移）
//
// 能力接缝三角色：
// - Service Definition：FsService（read_text / write_text / list_dir / delete）
// - Service Provider：本地工作区沙箱（路径规范化 + 越界防护，
//   复用 agent_workspace 的 safe_join；政策允许时可访问工作区外）
// - Consumer：模型工具（read_file / write_file / list_dir）、终端会话
// 政策（FsPolicy）来自用户设置：allow_workspace_escape（默认 false）。
// ============================================================

use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// 文件系统政策（来自用户设置）
#[derive(Clone, Debug, Default)]
pub struct FsPolicy {
    /// 允许访问工作区外路径（默认 false：一切文件操作限制在 agent_workspace）
    pub allow_workspace_escape: bool,
}

impl FsPolicy {
    pub fn current() -> Self {
        FsPolicy {
            // danger-full-access 或旧的布尔开关均视为可越界
            allow_workspace_escape: crate::harness::settings::current()
                .effective_workspace_escape(),
        }
    }
}

/// 项目根工作区下需要跳过的重目录（构建产物/依赖/版本库，避免全树扫描）
fn is_heavy_dir(name: &str) -> bool {
    matches!(
        name,
        "target" | "node_modules" | ".git" | "dist" | ".svelte-kit" | "build"
    )
}

// ─── 读-改-写观察策略（DSH fs-observation-policy 迁移） ───
// edit/write 现有文件前须先读取过该文件（read_file / str_replace_editor view），
// 并校验当前指纹与观察指纹一致（防陈旧覆盖）；新建文件豁免。
// 全局简化版：真实 owner（会话）粒度需把 session 传入工具执行
// （execute_tool_guarded 走 spawn_blocking，线程局部不可用），故以
// 「最近被读取过」为最小安全门槛。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct FileFingerprint {
    len: u64,
    modified_ms: u64,
}

fn observations() -> &'static Mutex<HashMap<std::path::PathBuf, FileFingerprint>> {
    static O: OnceLock<Mutex<HashMap<std::path::PathBuf, FileFingerprint>>> = OnceLock::new();
    O.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fingerprint_of(p: &std::path::Path) -> Option<FileFingerprint> {
    let meta = std::fs::metadata(p).ok()?;
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Some(FileFingerprint {
        len: meta.len(),
        modified_ms,
    })
}

fn record_observation(p: &std::path::Path) {
    if let Some(fp) = fingerprint_of(p) {
        observations().lock().unwrap().insert(p.to_path_buf(), fp);
    }
}

fn forget_observation(p: &std::path::Path) {
    observations().lock().unwrap().remove(p);
}

/// 校验观察：存在且指纹未变则 Ok；未观察或已变更则 Err（引导先 read_file）
fn require_observation(p: &std::path::Path) -> Result<(), String> {
    let obs = observations().lock().unwrap();
    match obs.get(p) {
        Some(observed) => match fingerprint_of(p) {
            Some(cur) if cur == *observed => Ok(()),
            _ => Err(format!(
                "文件已被修改：{}（请先 read_file 重新读取后再编辑/写入，避免基于陈旧内容覆盖）",
                p.display()
            )),
        },
        None => Err(format!(
            "尚未读取过该文件：{}（请先 read_file 读取后再编辑/写入）",
            p.display()
        )),
    }
}

/// 文件系统能力服务（本地工作区沙箱提供者）
pub struct FsService;

#[derive(Serialize, Clone, Debug)]
pub struct FsEntry {
    pub name: String,
    pub is_dir: bool,
}

impl FsService {
    /// 解析并校验路径：默认限制在 agent_workspace 沙箱内；
    /// 政策允许时可访问任意绝对路径
    pub fn resolve(
        &self,
        user_path: &str,
        policy: &FsPolicy,
    ) -> Result<std::path::PathBuf, String> {
        if policy.allow_workspace_escape {
            let p = std::path::Path::new(user_path);
            if p.is_absolute() {
                return Ok(p.to_path_buf());
            }
            return Ok(std::env::current_dir()
                .map_err(|e| format!("获取当前目录失败: {}", e))?
                .join(p));
        }
        crate::llm::agent::safe_join(user_path)
    }

    pub fn read_text(&self, path: &str, policy: &FsPolicy) -> Result<String, String> {
        let p = self.resolve(path, policy)?;
        if !p.is_file() {
            return Err(format!("不是文件: {}", p.display()));
        }
        let bytes = std::fs::read(&p).map_err(|e| format!("读取失败: {}", e))?;
        let text = String::from_utf8_lossy(&bytes);
        record_observation(&p);
        // 上限 64KB，超出截断（字符边界安全，避免中文内容 panic）
        Ok(if text.len() > 64 * 1024 {
            let end = text.floor_char_boundary(64 * 1024);
            format!("{}…（内容过长已截断）", &text[..end])
        } else {
            text.into_owned()
        })
    }

    /// 行窗口读取（DSH read(file_path, offset?, limit?) 语义）：返回带行号的
    /// [offset, offset+limit) 行区间与总行数；offset 1-based。记录观察
    /// （读-改-写策略），返回的窗口内容授权后续 edit/write。
    pub fn read_lines(
        &self,
        path: &str,
        offset: usize,
        limit: usize,
        policy: &FsPolicy,
    ) -> Result<(Vec<(usize, String)>, usize), String> {
        let p = self.resolve(path, policy)?;
        if !p.is_file() {
            return Err(format!("不是文件: {}", p.display()));
        }
        let bytes = std::fs::read(&p).map_err(|e| format!("读取失败: {}", e))?;
        let text = String::from_utf8_lossy(&bytes);
        let all: Vec<&str> = text.lines().collect();
        let total = all.len();
        let start = (offset.max(1) - 1).min(total);
        let end = (start + limit).min(total);
        let mut out = Vec::new();
        for (i, line) in all[start..end].iter().enumerate() {
            out.push((start + i + 1, line.to_string()));
        }
        record_observation(&p);
        Ok((out, total))
    }
    pub fn write_text(
        &self,
        path: &str,
        content: &str,
        policy: &FsPolicy,
    ) -> Result<usize, String> {
        let p = self.resolve(path, policy)?;
        if p.is_dir() {
            return Err(format!("目标路径是目录: {}", p.display()));
        }
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
        }
        if p.exists() {
            require_observation(&p)?;
        }
        std::fs::write(&p, content).map_err(|e| format!("写入失败: {}", e))?;
        record_observation(&p);
        Ok(content.len())
    }

    pub fn list_dir(&self, path: &str, policy: &FsPolicy) -> Result<Vec<FsEntry>, String> {
        let p = self.resolve(path, policy)?;
        let mut entries: Vec<FsEntry> = Vec::new();
        for e in std::fs::read_dir(&p).map_err(|e| format!("列目录失败: {}", e))? {
            let e = e.map_err(|e| e.to_string())?;
            let name = e.file_name().to_string_lossy().to_string();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            entries.push(FsEntry { name, is_dir });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    pub fn delete(&self, path: &str, policy: &FsPolicy) -> Result<(), String> {
        let p = self.resolve(path, policy)?;
        if p.is_dir() {
            std::fs::remove_dir_all(&p).map_err(|e| format!("删除目录失败: {}", e))?;
        } else {
            std::fs::remove_file(&p).map_err(|e| format!("删除文件失败: {}", e))?;
            forget_observation(&p);
        }
        Ok(())
    }

    /// 字面替换编辑（DSH edit 工具语义）：old_string 必须恰好出现一次
    /// （replace_all 时替换全部）；不匹配响亮报错，绝不静默改错文件
    pub fn edit_text(
        &self,
        path: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
        policy: &FsPolicy,
    ) -> Result<usize, String> {
        if old_string.is_empty() {
            return Err("old_string 不能为空".to_string());
        }
        let p = self.resolve(path, policy)?;
        if !p.is_file() {
            return Err(format!("不是文件: {}", p.display()));
        }
        require_observation(&p)?;
        let text = std::fs::read_to_string(&p).map_err(|e| format!("读取失败: {}", e))?;
        let count = text.matches(old_string).count();
        if count == 0 {
            return Err("未找到待替换内容（old_string 不匹配，请先读取文件确认内容）".to_string());
        }
        if count > 1 && !replace_all {
            return Err(format!(
                "待替换内容出现 {count} 次：请扩大 old_string 使匹配唯一，或传 replace_all=true"
            ));
        }
        let new_text = if replace_all {
            text.replace(old_string, new_string)
        } else {
            text.replacen(old_string, new_string, 1)
        };
        std::fs::write(&p, new_text.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
        record_observation(&p);
        Ok(count)
    }

    /// DSH str_replace_editor `view`：文件 → 带行号视图（可选 view_range）；
    /// 目录 → 2 层深列表（跳过隐藏项与 node_modules/__pycache__）。
    /// 输出按字符边界截断到 max_output_chars 并标注 <response clipped>。
    pub fn str_replace_view(
        &self,
        path: &str,
        view_range: Option<Vec<i64>>,
        policy: &FsPolicy,
    ) -> Result<String, String> {
        let p = self.resolve(path, policy)?;
        if p.is_dir() {
            let mut rows: Vec<String> = Vec::new();
            rows.push(format!("d\t{}", p.display()));
            let mut stack: Vec<(std::path::PathBuf, usize)> = vec![(p.clone(), 1)];
            while let Some((dir, depth)) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') || name == "node_modules" || name == "__pycache__" {
                        continue;
                    }
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    rows.push(format!(
                        "{}\t{}",
                        if is_dir { "d" } else { "f" },
                        e.path().display()
                    ));
                    if is_dir && depth < 2 {
                        stack.push((e.path(), depth + 1));
                    }
                }
            }
            let mut listing = rows.join("\n");
            listing.push('\n');
            return Ok(truncate_16k(
                &listing,
                "这里列出了该目录及其子目录（2 层内，跳过隐藏项与 node_modules）：",
            ));
        }
        if !p.is_file() {
            return Err(format!("不是文件也不是目录: {}", p.display()));
        }
        let text = std::fs::read_to_string(&p).map_err(|e| format!("读取失败: {}", e))?;
        record_observation(&p);
        let all_lines: Vec<&str> = text.split('\n').collect();
        let total = all_lines.len();
        let (start, end) = match view_range {
            None => (1usize, total),
            Some(v) if v.len() != 2 => {
                return Err("view_range 应为两个整数 [start, end]".to_string());
            }
            Some(v) => {
                let (s, e) = (v[0], v[1]);
                if s < 1 || s as usize > total {
                    return Err(format!("view_range 起始行 {} 超出范围 [1, {}]", s, total));
                }
                let end = if e == -1 { total as i64 } else { e };
                if end < s || end as usize > total {
                    return Err(format!(
                        "view_range 结束行 {} 超出范围或小于起始行 {}",
                        e, s
                    ));
                }
                (s as usize, end as usize)
            }
        };
        let mut numbered = String::new();
        for (i, line) in all_lines[(start - 1)..end].iter().enumerate() {
            numbered.push_str(&format!("{:6}  {}\n", start + i, line));
        }
        let header = format!(
            "{} 的内容（共 {} 行{}）：",
            p.display(),
            total,
            if start == 1 && end == total {
                String::new()
            } else {
                format!("，显示 {}..{}", start, end)
            }
        );
        Ok(truncate_16k(&numbered, &header))
    }

    /// DSH str_replace_editor `create`：以 file_text 创建新文件；
    /// 路径已存在时报错（绝不覆盖）。
    pub fn create_if_absent(
        &self,
        path: &str,
        file_text: &str,
        policy: &FsPolicy,
    ) -> Result<String, String> {
        let p = self.resolve(path, policy)?;
        if p.exists() {
            return Err(format!("文件已存在，create 命令不覆盖: {}", p.display()));
        }
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
        }
        std::fs::write(&p, file_text.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
        record_observation(&p);
        Ok(format!("已创建新文件: {}", p.display()))
    }

    /// DSH str_replace_editor `str_replace`：old_str 唯一匹配替换。
    /// 0 匹配 / 多处匹配（列出所在行）均响亮报错，绝不静默改错。
    pub fn str_replace(
        &self,
        path: &str,
        old_str: &str,
        new_str: &str,
        policy: &FsPolicy,
    ) -> Result<String, String> {
        let p = self.resolve(path, policy)?;
        if !p.is_file() {
            return Err(format!("不是文件: {}", p.display()));
        }
        require_observation(&p)?;
        let text = std::fs::read_to_string(&p).map_err(|e| format!("读取失败: {}", e))?;
        let mut offsets: Vec<usize> = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = text[from..].find(old_str) {
            offsets.push(from + rel);
            from += rel + old_str.len();
        }
        match offsets.len() {
            0 => Err(format!(
                "未执行替换：old_str `{}` 未在 {} 中原样出现",
                old_str,
                p.display()
            )),
            1 => {
                let offset = offsets[0];
                let new_text = format!(
                    "{}{}{}",
                    &text[..offset],
                    new_str,
                    &text[offset + old_str.len()..]
                );
                std::fs::write(&p, new_text.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
                record_observation(&p);
                Ok(format!("文件已编辑成功: {}", p.display()))
            }
            _ => {
                let lines = offsets
                    .iter()
                    .map(|off| text[..*off].matches('\n').count() + 1)
                    .collect::<Vec<_>>();
                Err(format!(
                    "未执行替换：old_str `{}` 在 {} 中出现多处（行 {:?}），请使其唯一",
                    old_str,
                    p.display(),
                    lines
                ))
            }
        }
    }

    /// DSH str_replace_editor `insert`：在 insert_line 行之后插入 new_str
    /// （insert_line ∈ [0, 当前行数]，0 = 文件开头）。
    pub fn insert_lines(
        &self,
        path: &str,
        insert_line: i64,
        new_str: &str,
        policy: &FsPolicy,
    ) -> Result<String, String> {
        let p = self.resolve(path, policy)?;
        if !p.is_file() {
            return Err(format!("不是文件: {}", p.display()));
        }
        require_observation(&p)?;
        let text = std::fs::read_to_string(&p).map_err(|e| format!("读取失败: {}", e))?;
        let lines: Vec<&str> = text.split('\n').collect();
        let n = lines.len() as i64;
        if insert_line < 0 || insert_line > n {
            return Err(format!(
                "insert_line 参数非法: {}（应在 [0, {}] 范围内）",
                insert_line, n
            ));
        }
        let at = insert_line as usize;
        let mut out = lines[..at].to_vec();
        out.extend(new_str.split('\n'));
        out.extend_from_slice(&lines[at..]);
        let joined = out.join("\n");
        std::fs::write(&p, joined.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
        record_observation(&p);
        Ok(format!("文件已编辑成功: {}", p.display()))
    }

    /// glob 文件发现（DSH glob 工具语义）：工作区内按模式发现文件/目录。
    /// pattern 支持 **、*、?、[...] 字符类与 {a,b} 交替（先编译为等价正则，
    /// 无效模式提前报错）；path 为搜索根（空 = 工作区根，相对路径锚定工作区）。
    pub fn glob(
        &self,
        pattern: &str,
        path: &str,
        policy: &FsPolicy,
    ) -> Result<Vec<String>, String> {
        let pattern = pattern.trim().replace('\\', "/");
        if pattern.is_empty() {
            return Err("pattern 不能为空".to_string());
        }
        let root = crate::harness::workspace::sandbox_root();
        // 搜索根：path 空 = 工作区根；否则按政策解析（越界需 policy 放行）
        let base = if path.trim().is_empty() {
            root.clone()
        } else {
            self.resolve(path, policy)?
        };
        if !base.is_dir() {
            return Err(format!("搜索根不是目录: {}", base.display()));
        }
        // 模式锚定：以 / 开头视为相对搜索根
        let pattern = pattern.trim_start_matches('/').to_string();
        let re = compile_glob(&pattern)?;
        let mut out = Vec::new();
        let mut stack = vec![base.clone()];
        while let Some(dir) = stack.pop() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for e in entries.flatten() {
                let p = e.path();
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir && is_heavy_dir(&e.file_name().to_string_lossy()) {
                    continue; // 跳过构建/依赖重目录（项目根工作区下避免扫 target/node_modules）
                }
                let rel = p
                    .strip_prefix(&base)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                let rel = rel.strip_prefix('/').unwrap_or(&rel).to_string();
                if is_dir {
                    // ** 段可匹配任意深度：目录也尝试匹配，并继续下钻
                    if re.is_match(&rel) {
                        out.push(format!("{}/", rel));
                    }
                    stack.push(p);
                    if out.len() > 2000 {
                        break;
                    }
                } else if re.is_match(&rel) {
                    out.push(rel);
                    if out.len() > 2000 {
                        break;
                    }
                }
            }
        }
        out.sort();
        let total = out.len();
        if total > 200 {
            // 截断感知 + 完整列表落盘（DSH sampleOverCapGlobResults）：
            // 模型经 spill_read 可跨会话取回全部匹配；提示位于首行
            let full = out.join("\n");
            let note = match crate::harness::spill::SpillStore::save_shared(&full) {
                Ok(r) => format!(
                    "（共 {total} 条匹配，仅显示前 200 条；完整列表 locator: {}，可用 spill_read 取回）",
                    r.locator
                ),
                Err(_) => format!(
                    "（共 {total} 条匹配，仅显示前 200 条；可缩小 glob 模式或指定 path 收窄范围）"
                ),
            };
            out.insert(0, note);
            out.truncate(201);
        } else if out.is_empty() {
            return Ok(Vec::new());
        }
        Ok(out)
    }

    /// grep 文本搜索（DSH grep 工具语义）：regex 匹配，返回 file:line:内容
    /// grep 文本搜索（DSH grep 工具语义）：regex 匹配，返回 file:line:内容。
    /// include 为正向 glob 过滤器（仅搜索路径匹配该 glob 的文件）；
    /// 二进制文件（探测段含 NUL）与超大文件（> 8MB）自动跳过（对齐 ripgrep）。
    pub fn grep(
        &self,
        pattern: &str,
        path: &str,
        include: &str,
        case_insensitive: bool,
        policy: &FsPolicy,
    ) -> Result<String, String> {
        let re = regex::RegexBuilder::new(pattern)
            .case_insensitive(case_insensitive)
            .build()
            .map_err(|e| format!("正则表达式无效: {e}"))?;
        let include_re = if include.trim().is_empty() {
            None
        } else {
            Some(compile_glob(&include.trim().replace('\\', "/"))?)
        };
        let base = self.resolve(if path.is_empty() { "." } else { path }, policy)?;
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        if base.is_file() {
            files.push(base);
        } else {
            let mut stack = vec![base];
            while let Some(dir) = stack.pop() {
                let entries = match std::fs::read_dir(&dir) {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                for e in entries.flatten() {
                    let p = e.path();
                    if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        if is_heavy_dir(&e.file_name().to_string_lossy()) {
                            continue;
                        }
                        stack.push(p);
                    } else {
                        files.push(p);
                    }
                }
            }
        }
        files.sort();
        let sandbox_root = crate::harness::workspace::sandbox_root();
        let mut out_lines = Vec::new();
        'outer: for f in files {
            if out_lines.len() >= GREP_MAX_COLLECT {
                break;
            }
            // include 过滤器：相对工作区根（或绝对路径原文）的 / 归一化路径
            if let Some(inc) = &include_re {
                let rel = f
                    .strip_prefix(&sandbox_root)
                    .unwrap_or(&f)
                    .to_string_lossy()
                    .replace('\\', "/");
                let rel = rel.strip_prefix("//?/").unwrap_or(&rel);
                let rel = rel.strip_prefix('/').unwrap_or(rel);
                if !inc.is_match(rel) {
                    // ripgrep -g 语义：无 / 的 glob（如 *.rs）匹配任意层级 basename
                    let basename = std::path::Path::new(&f)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if !inc.is_match(&basename) {
                        continue;
                    }
                }
            }
            // 大小上限 + 二进制检测（DSH ripgrep：默认跳过二进制与超大文件）
            let Ok(meta) = std::fs::metadata(&f) else {
                continue;
            };
            if meta.len() > GREP_MAX_FILE_BYTES {
                continue;
            }
            let Ok(bytes) = std::fs::read(&f) else {
                continue;
            };
            if looks_binary(&bytes) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            let rel = f
                .strip_prefix(&sandbox_root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            // canonicalize 可能引入 \\?\ 前缀，剥离保证展示整洁
            let rel = rel.strip_prefix("//?/").unwrap_or(&rel).to_string();
            for (i, line) in text.lines().enumerate() {
                if re.is_match(line) {
                    out_lines.push(format!(
                        "{}:{}: {}",
                        rel,
                        i + 1,
                        line.trim_end().chars().take(300).collect::<String>()
                    ));
                    if out_lines.len() >= GREP_MAX_COLLECT {
                        break 'outer;
                    }
                }
            }
        }
        if out_lines.is_empty() {
            return Ok("（无匹配）".to_string());
        }
        let mut body = out_lines.join("\n");
        // 截断感知 + 完整列表落盘（DSH sampleOverCapGlobResults）：
        // 命中超过内联上限时把完整列表存共享溢写，模型经 spill_read 取回
        if out_lines.len() > 200 {
            let full = out_lines.join("\n");
            let note = match crate::harness::spill::SpillStore::save_shared(&full) {
                Ok(r) => format!(
                    "（共 {} 条匹配，已显示前 200 条；完整列表 locator: {}，可用 spill_read 取回）",
                    out_lines.len(),
                    r.locator
                ),
                Err(_) => "（匹配超过 200 条已截断，可加 include 过滤或缩小 path）".to_string(),
            };
            body = format!("{}\n{}", note, body);
        }
        Ok(body)
    }

    /// 读取图片为 base64 data URL（DSH read_image 工具语义；
    /// 模型支持视觉输入时可直接引用，否则作为原始数据返回）
    pub fn read_image_base64(&self, path: &str, policy: &FsPolicy) -> Result<String, String> {
        let p = self.resolve(path, policy)?;
        if !p.is_file() {
            return Err(format!("不是文件: {}", p.display()));
        }
        let bytes = std::fs::read(&p).map_err(|e| format!("读取失败: {}", e))?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("图片超过 4MB，请先压缩".to_string());
        }
        let mime = match p
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .as_deref()
        {
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("webp") => "image/webp",
            Some("gif") => "image/gif",
            other => {
                return Err(format!("不支持的图片格式: {:?}", other));
            }
        };
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(format!(
            "data:{mime};base64,{b64}（图片 {} 字节，可作视觉输入引用）",
            bytes.len()
        ))
    }
}

/// grep 单文件大小上限（跳过超大文件，避免内存/耗时失控）
const GREP_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// grep 单次收集上限（超过内联上限 200 后继续收集到此，供完整列表落盘）
const GREP_MAX_COLLECT: usize = 2000;

/// 二进制探测：前 8KB 含 NUL 即视为二进制（与 ripgrep 默认行为一致）
fn looks_binary(bytes: &[u8]) -> bool {
    let probe = &bytes[..bytes.len().min(8192)];
    probe.contains(&0u8)
}

/// 编译 glob 模式为等价正则（^…$ 锚定；无效模式提前报错）
fn compile_glob(pattern: &str) -> Result<regex::Regex, String> {
    // DSH glob 语义（2026-07-27 glob-sampling）：无 / 的模式匹配任意深度
    // 的 basename（如 *.rs 匹配 src/a.rs），等价于 **/<pattern>；
    // 含 / 的模式按字面路径层级匹配。
    let anchored = if !pattern.contains('/') {
        format!("**/{}", pattern)
    } else {
        pattern.to_string()
    };
    let rx = glob_to_regex(&anchored)?;
    regex::Regex::new(&format!("^{}$", rx)).map_err(|e| format!("glob 模式无效: {e}"))
}

/// glob → 正则翻译（ripgrep/DSH glob 语义子集）：** 任意深度、* 段内任意、
/// ? 单字符、[...] 字符类（[!…] 否定）、{a,b} 交替；其余字符字面（正则
/// 元字符转义）。纯函数便于单测。
fn glob_to_regex(pattern: &str) -> Result<String, String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    i += 2;
                    if i < chars.len() && chars[i] == '/' {
                        out.push_str("(?:.*/)?");
                        i += 1;
                    } else {
                        out.push_str(".*");
                    }
                } else {
                    out.push_str("[^/]*");
                    i += 1;
                }
            }
            '?' => {
                out.push_str("[^/]");
                i += 1;
            }
            '[' => {
                let mut j = i + 1;
                let mut cls = String::from("[");
                if j < chars.len() && (chars[j] == '!' || chars[j] == '^') {
                    cls.push('^');
                    j += 1;
                }
                let mut closed = false;
                while j < chars.len() {
                    let ch = chars[j];
                    if ch == ']' && cls.len() > 1 {
                        closed = true;
                        cls.push(']');
                        j += 1;
                        break;
                    }
                    if ch == '\\' && j + 1 < chars.len() {
                        cls.push('\\');
                        cls.push(chars[j + 1]);
                        j += 2;
                        continue;
                    }
                    cls.push(ch);
                    j += 1;
                }
                if closed {
                    out.push_str(&cls);
                    i = j;
                } else {
                    return Err("glob 模式无效：未闭合的字符类 [".to_string());
                }
            }
            '{' => match find_brace_close(&chars, i) {
                Some(close) => {
                    let inner: String = chars[i + 1..close].iter().collect();
                    let alts = split_brace_alts(&inner);
                    let parts: Result<Vec<String>, String> =
                        alts.iter().map(|a| glob_to_regex(a)).collect();
                    out.push_str(&format!("(?:{})", parts?.join("|")));
                    i = close + 1;
                }
                None => {
                    out.push_str("\\{");
                    i += 1;
                }
            },
            '\\' => {
                if i + 1 < chars.len() {
                    out.push_str(&format!("\\{}", chars[i + 1]));
                    i += 2;
                } else {
                    out.push_str("\\\\");
                    i += 1;
                }
            }
            c if "()+^$|.".contains(c) => {
                out.push('\\');
                out.push(c);
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(out)
}

/// 定位 { 的匹配 }（嵌套深度跟踪；无匹配返回 None）
fn find_brace_close(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }
    None
}

/// 按顶层逗号拆分 {a,b} 交替项（嵌套花括号不受影响）
fn split_brace_alts(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut cur = String::new();
    for c in inner.chars() {
        match c {
            '{' => {
                depth += 1;
                cur.push(c);
            }
            '}' => {
                depth = depth.saturating_sub(1);
                cur.push(c);
            }
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// str_replace_editor 输出截断：按字符边界截到 16K 并标注 <response clipped>
/// （与 DSH tool-str-replace-editor maxOutputChars=16000 一致）
fn truncate_16k(body: &str, header: &str) -> String {
    const MAX: usize = 16_000;
    let mut out = String::from(header);
    out.push('\n');
    if body.chars().count() <= MAX {
        out.push_str(body);
    } else {
        // 按字符边界截断（UTF-8 安全，避免中文被切成半个字符）
        let end = body.floor_char_boundary(MAX);
        out.push_str(&body[..end]);
        out.push_str("\n<response clipped><NOTE>输出过长已截断，请用 view_range 分段查看。</NOTE>");
    }
    out
}

/// 注册文件系统能力（Cordis-lite 服务；后续阶段工具统一经此消费）
pub fn provide_service() -> crate::harness::registry::Disposer {
    crate::harness::registry::provide("harness.fs", std::sync::Arc::new(FsService))
}

/// 人工命令：读取文件（能力验证/调试）
#[tauri::command]
pub async fn harness_fs_read(path: String) -> Result<String, String> {
    let svc = crate::harness::registry::get::<FsService>("harness.fs")
        .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    svc.read_text(&path, &FsPolicy::current())
}

/// 人工命令：删除文件/目录（能力验证/调试）
#[tauri::command]
pub async fn harness_fs_delete(path: String) -> Result<(), String> {
    let svc = crate::harness::registry::get::<FsService>("harness.fs")
        .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    svc.delete(&path, &FsPolicy::current())
}

/// 打开文件/目录（DSH Host openPath 迁移：产物文件 chip / 工具路径点击打开）。
/// 与微信板块 open_wechat_path 同模式：Windows 走 `cmd /c start`（raw_arg 引号
/// 防重解释），其他平台 xdg-open。
#[tauri::command]
pub async fn harness_open_path(path: String) -> Result<(), String> {
    // 与读/写同一套路径解析：相对路径锚定当前沙箱根（默认工作区 = 应用项目根），
    // 否则产物相对路径（如「广西今日天气_2026-08-21.csv」）会按进程 CWD 解析
    // 而报「路径不存在」。
    let svc = crate::harness::registry::get::<FsService>("harness.fs")
        .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    let policy = FsPolicy::current();
    let p = svc.resolve(path.trim(), &policy)?;
    if !p.exists() {
        return Err(format!("路径不存在: {}", p.display()));
    }
    // L3：沙箱校验——非越界模式下仅允许打开工作区（含附件，复制进工作区）
    // 内的路径，防止模型/用户输入打开任意系统路径（cmd /c start 任意目标）；
    // resolve 已按策略锚定并校验，这里防御性复核。
    if !policy.allow_workspace_escape {
        // sandbox_root() 可能含非规范成分（dev 下由 exe 目录上溯定位），
        // 先规范化再比较，避免与已 canonicalize 的目标路径前缀不匹配而误拒。
        let root = crate::harness::workspace::sandbox_root()
            .canonicalize()
            .map_err(|e| format!("工作区解析失败: {}", e))?;
        let canon = p
            .canonicalize()
            .map_err(|e| format!("路径解析失败: {}", e))?;
        if !canon.starts_with(&root) {
            return Err(format!(
                "路径超出允许的工作区范围（沙箱模式 {}）: {}",
                crate::harness::settings::current().effective_sandbox_mode(),
                p.display()
            ));
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let raw = p.to_string_lossy().replace('"', "");
        // 去掉结尾反斜杠，避免 `"C:\dir\"` 中反斜杠转义引号
        let quoted = format!("\"{}\"", raw.trim_end_matches('\\'));
        // quoted 已自带引号，必须用 raw_arg 原样拼接，否则 Command 再次加引号
        let mut cmd = std::process::Command::new("cmd");
        cmd.arg("/c").arg("start").arg("").raw_arg(&quoted);
        cmd.spawn().map_err(|e| format!("打开失败: {}", e))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("xdg-open").arg(&p).spawn();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fs_roundtrip_inside_workspace() {
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        svc.write_text("hfs_test.txt", "你好", &policy).unwrap();
        assert_eq!(svc.read_text("hfs_test.txt", &policy).unwrap(), "你好");
        let list = svc.list_dir("", &policy).unwrap();
        assert!(list.iter().any(|e| e.name == "hfs_test.txt"));
        svc.delete("hfs_test.txt", &policy).unwrap();
    }
    #[test]
    fn open_path_resolves_relative_produced_files() {
        // 回归：产物以相对路径写入（如「广西今日天气_2026-08-21.csv」），
        // harness_open_path 必须与读/写同一套 resolve 锚定沙箱根，
        // 否则按进程 CWD 解析会报「路径不存在」。
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        svc.write_text("hfs_open_test.csv", "data", &policy)
            .unwrap();
        let resolved = svc.resolve("hfs_open_test.csv", &policy).unwrap();
        assert!(resolved.is_absolute(), "相对路径应锚定为绝对路径");
        assert!(resolved.exists(), "解析后的路径必须存在");
        let root = crate::harness::workspace::sandbox_root()
            .canonicalize()
            .unwrap();
        assert!(resolved.starts_with(&root), "解析结果必须在沙箱根内");
        svc.delete("hfs_open_test.csv", &policy).unwrap();
    }

    #[test]
    fn fs_policy_blocks_escape() {
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        assert!(svc.read_text("../llm_config.json", &policy).is_err());
        assert!(svc.resolve("C:/Windows/System32", &policy).is_err());
    }

    #[test]
    fn fs_policy_allow_escape_resolves_absolute() {
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: true,
        };
        assert!(svc.resolve("C:/", &policy).is_ok());
    }

    #[test]
    fn edit_text_replaces_unique_and_errors_on_ambiguity() {
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        svc.write_text("hfs_edit.txt", "aaa\nbbb\naaa\n", &policy)
            .unwrap();
        // 多处匹配且未声明 replace_all → 报错
        assert!(svc
            .edit_text("hfs_edit.txt", "aaa", "zzz", false, &policy)
            .is_err());
        // replace_all → 全部替换
        let n = svc
            .edit_text("hfs_edit.txt", "aaa", "zzz", true, &policy)
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            svc.read_text("hfs_edit.txt", &policy).unwrap(),
            "zzz\nbbb\nzzz\n"
        );
        // 唯一匹配 → 替换一次
        let n = svc
            .edit_text("hfs_edit.txt", "bbb", "yyy", false, &policy)
            .unwrap();
        assert_eq!(n, 1);
        // 不匹配 → 报错
        assert!(svc
            .edit_text("hfs_edit.txt", "nonexistent", "x", false, &policy)
            .is_err());
        svc.delete("hfs_edit.txt", &policy).unwrap();
    }

    #[test]
    fn glob_compiles_and_matches_patterns() {
        // ** 任意深度 / * 段内 / ? 单字符
        assert!(compile_glob("*.rs").unwrap().is_match("main.rs"));
        assert!(!compile_glob("*.rs").unwrap().is_match("main.ts"));
        assert!(compile_glob("src/**/*.rs")
            .unwrap()
            .is_match("src/lib/a.rs"));
        assert!(compile_glob("src/**/*.rs").unwrap().is_match("src/a.rs"));
        assert!(compile_glob("**/README.md")
            .unwrap()
            .is_match("a/b/README.md"));
        assert!(compile_glob("a?c.txt").unwrap().is_match("abc.txt"));
        assert!(!compile_glob("a?c.txt").unwrap().is_match("ac.txt"));
        // {a,b} 交替
        let re = compile_glob("src/**/*.{ts,js}").unwrap();
        assert!(re.is_match("src/lib/a.ts"));
        assert!(re.is_match("src/lib/a.js"));
        assert!(!re.is_match("src/lib/a.rs"));
        // [...] 字符类与否定类
        assert!(compile_glob("file[0-9].txt").unwrap().is_match("file7.txt"));
        assert!(!compile_glob("file[0-9].txt").unwrap().is_match("filex.txt"));
        assert!(compile_glob("[!a]x.txt").unwrap().is_match("bx.txt"));
        assert!(!compile_glob("[!a]x.txt").unwrap().is_match("ax.txt"));
        // 正则元字符按字面（无 glob 语义时）
        assert!(compile_glob("a.b").unwrap().is_match("a.b"));
        assert!(!compile_glob("a.b").unwrap().is_match("axb"));
        // 无效模式提前报错
        assert!(compile_glob("[").is_err());
    }

    #[test]
    fn glob_brace_alts_and_search_root() {
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        svc.write_text("hfs_glob2/x.rs", "one", &policy).unwrap();
        svc.write_text("hfs_glob2/y.ts", "two", &policy).unwrap();
        svc.write_text("hfs_glob2/z.md", "three", &policy).unwrap();
        // {a,b} 交替 + 工作区根搜索
        let hits = svc.glob("hfs_glob2/*.{rs,ts}", "", &policy).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.ends_with("x.rs")));
        assert!(hits.iter().any(|h| h.ends_with("y.ts")));
        // path 参数：搜索根定位到子目录，模式相对该根
        let hits2 = svc.glob("*.{rs,ts}", "hfs_glob2", &policy).unwrap();
        assert_eq!(hits2.len(), 2);
        svc.delete("hfs_glob2", &policy).unwrap();
    }

    #[test]
    fn glob_and_grep_workdir() {
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        svc.write_text("hfs_glob/a.txt", "needle-one", &policy)
            .unwrap();
        svc.write_text("hfs_glob/b.txt", "nothing", &policy)
            .unwrap();
        let hits = svc.glob("hfs_glob/*.txt", "", &policy).unwrap();
        assert_eq!(hits.len(), 2);
        let g = svc.grep("needle", "hfs_glob", "", false, &policy).unwrap();
        assert!(g.contains("hfs_glob/a.txt:1: needle-one"));
        assert!(!g.contains("b.txt"));
        svc.delete("hfs_glob", &policy).unwrap();
    }

    #[test]
    fn grep_include_filter_and_binary_skip() {
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        svc.write_text("hfs_grep3/a.rs", "needle-rust", &policy)
            .unwrap();
        svc.write_text("hfs_grep3/b.ts", "needle-ts", &policy)
            .unwrap();
        // include 过滤器：只搜 *.rs
        let g = svc
            .grep("needle", "hfs_grep3", "*.rs", false, &policy)
            .unwrap();
        assert!(g.contains("a.rs:1: needle-rust"));
        assert!(!g.contains("b.ts"));
        // 二进制文件跳过：写一个含 NUL 的文件，不应出现在结果中
        std::fs::write(
            crate::harness::workspace::sandbox_root().join("hfs_grep3/c.bin"),
            b"needle-bin\x00needle",
        )
        .unwrap();
        let g2 = svc.grep("needle", "hfs_grep3", "", false, &policy).unwrap();
        assert!(!g2.contains("c.bin"), "二进制文件应被跳过: {g2}");
        // 显式指向单文件时仍跳过二进制
        let g3 = svc
            .grep("needle", "hfs_grep3/c.bin", "", false, &policy)
            .unwrap();
        assert!(!g3.contains("c.bin"));
        svc.delete("hfs_grep3", &policy).unwrap();
    }

    #[test]
    fn grep_case_insensitive_option() {
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        svc.write_text("hfs_grep4/a.txt", "Hello World", &policy)
            .unwrap();
        // 默认大小写敏感：hello 不命中
        let g = svc.grep("hello", "hfs_grep4", "", false, &policy).unwrap();
        assert!(!g.contains("a.txt"));
        // case_insensitive=true 命中
        let g2 = svc.grep("hello", "hfs_grep4", "", true, &policy).unwrap();
        assert!(g2.contains("a.txt:1: Hello World"));
        svc.delete("hfs_grep4", &policy).unwrap();
    }

    #[test]
    fn glob_reports_truncation_with_total_count() {
        // DSH glob maxResults：命中超过内联上限时提示总数，模型可据此收窄
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        let dir = format!("hfs_many/{}", uuid::Uuid::new_v4().simple());
        for i in 0..205 {
            svc.write_text(&format!("{dir}/f{i:03}.txt"), "x", &policy)
                .unwrap();
        }
        let hits = svc.glob(&format!("{dir}/*.txt"), "", &policy).unwrap();
        // 205 条匹配：内联 200 + 首行提示（共 205 条）
        assert_eq!(hits.len(), 201, "200 条内联 + 1 条提示: len={}", hits.len());
        assert!(
            hits[0].contains("共 205 条匹配"),
            "提示应含总数: {}",
            hits[0]
        );

        // 完整列表已落盘：spill_read 可跨会话取回全部 205 条匹配
        let loc = hits[0]
            .find("完整列表 locator: ")
            .map(|p| {
                hits[0][p + "完整列表 locator: ".len()..]
                    .split('，')
                    .next()
                    .unwrap_or("")
                    .to_string()
            })
            .unwrap();
        let back = crate::harness::spill::SpillStore::read(&loc).unwrap();
        assert_eq!(back.lines().count(), 205, "共享溢写应含全部 205 条匹配");
        let _ = std::fs::remove_file(&loc);
        // 少量匹配不触发提示
        let few = svc.glob(&format!("{dir}/f000.txt"), "", &policy).unwrap();
        assert_eq!(few.len(), 1);
        assert!(!few[0].contains("条匹配"));
        svc.delete(&format!("hfs_many"), &policy).unwrap();
    }

    #[test]
    fn grep_reports_truncation_note() {
        // DSH grep maxMatches：命中超过内联上限时提示已截断
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        let dir = format!("hfs_grep_many/{}", uuid::Uuid::new_v4().simple());
        let big: String = (0..250)
            .map(|i| format!("needle-{i}\n"))
            .collect::<String>();
        svc.write_text(&format!("{dir}/a.txt"), &big, &policy)
            .unwrap();
        let out = svc.grep("needle", &dir, "", false, &policy).unwrap();
        // 完整列表已落盘：提示含总数与 locator（DSH sampleOverCapGlobResults）
        assert!(
            out.starts_with("（共 250 条匹配，已显示前 200 条；完整列表 locator:"),
            "应提示总数与 locator: {}",
            &out[..80.min(out.len())]
        );
        let loc = out
            .lines()
            .find_map(|l| {
                l.find("完整列表 locator: ")
                    .map(|p| l[p + "完整列表 locator: ".len()..].to_string())
            })
            .map(|s| s.split('，').next().unwrap_or("").to_string())
            .unwrap();
        let back = crate::harness::spill::SpillStore::read(&loc).unwrap();
        assert_eq!(back.lines().count(), 250, "共享溢写应含全部 250 条匹配");
        let lines = out.lines().count();
        assert!(lines >= 201, "提示 + 至少 200 条命中: {lines}");
        svc.write_text(&format!("{dir}/b.txt"), "needle-only", &policy)
            .unwrap();
        let out2 = svc.grep("needle-only", &dir, "", false, &policy).unwrap();
        assert!(
            !out2.starts_with("（匹配超过 200 条"),
            "少量匹配不应提示: {out2}"
        );
        svc.delete("hfs_grep_many", &policy).unwrap();
    }

    #[test]
    fn read_image_rejects_unknown_and_oversized() {
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        svc.write_text("hfs_pic.txt", "not-an-image", &policy)
            .unwrap();
        assert!(svc.read_image_base64("hfs_pic.txt", &policy).is_err());
        svc.delete("hfs_pic.txt", &policy).unwrap();
    }

    #[test]
    fn read_text_truncates_at_char_boundary_with_chinese() {
        // H2 回归：超限中文内容按字节切片曾在中文字符中间 panic；
        // 现在必须在字符边界截断且不 panic
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        // 3 万汉字 ≈ 90KB（> 64KB 上限），截断点必落在多字节字符内
        let big: String = "汉".repeat(30_000);
        svc.write_text("hns_big.txt", &big, &policy).unwrap();
        let text = svc.read_text("hns_big.txt", &policy).unwrap();
        // 有效 UTF-8（无 panic 即说明未在字符中间切开）
        assert!(String::from_utf8(text.clone().into_bytes()).is_ok());
        assert!(text.contains("内容过长已截断"));
        assert!(text.len() <= 64 * 1024 + 64, "截断后应接近 64KB 上限");
        svc.delete("hns_big.txt", &policy).unwrap();
    }

    #[test]
    fn read_before_write_policy_blocks_stale_and_unobserved() {
        // DSH 2026-06-17 fs-observation-policy：edit/write 现有文件须先读取；
        // 新建文件豁免；写/编辑后指纹更新，外部变更触发陈旧错误
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        let dir = format!("hfs_rbw/{}", uuid::Uuid::new_v4().simple());
        let f = format!("{dir}/a.txt");
        // 未读取过就编辑 → 拒绝（引导先 read_file）
        // 用 std::fs 直接创建（不经 FsService），保持「未观察」状态
        std::fs::create_dir_all(crate::harness::workspace::sandbox_root().join(&dir)).unwrap();
        std::fs::write(crate::harness::workspace::sandbox_root().join(&f), "hello").unwrap();
        let err = svc
            .edit_text(&f, "hello", "world", false, &policy)
            .unwrap_err();
        assert!(err.contains("尚未读取过该文件"), "未观察编辑应报错: {err}");
        // 读取后再编辑 → 放行
        let _ = svc.read_text(&f, &policy).unwrap();
        svc.edit_text(&f, "hello", "world", false, &policy).unwrap();
        // 外部变更后（指纹变化）再编辑 → 陈旧错误
        let _ = svc.read_text(&f, &policy).unwrap();
        std::fs::write(
            crate::harness::workspace::sandbox_root().join(&f),
            "external-change",
        )
        .unwrap();
        let err2 = svc
            .edit_text(&f, "world", "again", false, &policy)
            .unwrap_err();
        assert!(err2.contains("已被修改"), "陈旧编辑应报错: {err2}");
        // 陈旧覆盖写 → 拒绝（已被外部修改）
        let err3 = svc.write_text(&f, "overwrite", &policy).unwrap_err();
        assert!(err3.contains("已被修改"), "陈旧覆盖写应报错: {err3}");
        // 读取后覆盖写 → 放行
        let _ = svc.read_text(&f, &policy).unwrap();
        svc.write_text(&f, "overwrite", &policy).unwrap();
        svc.delete(&dir, &policy).unwrap();
    }

    #[test]
    fn read_lines_windows_with_line_numbers() {
        // DSH read(file_path, offset?, limit?)：1-based 行窗口 + 行号
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        let dir = format!("hfs_rl/{}", uuid::Uuid::new_v4().simple());
        let f = format!("{dir}/a.txt");
        svc.write_text(&f, "l1\nl2\nl3\nl4\nl5", &policy).unwrap();
        let (rows, total) = svc.read_lines(&f, 2, 3, &policy).unwrap();
        assert_eq!(total, 5);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], (2, "l2".to_string()));
        assert_eq!(rows[2], (4, "l4".to_string()));
        // 越界窗口：返回空 + 总数
        let (rows2, total2) = svc.read_lines(&f, 10, 3, &policy).unwrap();
        assert_eq!(total2, 5);
        assert!(rows2.is_empty());
        svc.delete(&dir, &policy).unwrap();
    }

    #[test]
    fn str_replace_editor_view_create_replace_insert() {
        // B24：DSH str_replace_editor 四命令（view/create/str_replace/insert）
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        // create：新文件
        svc.create_if_absent("hfs_sre.txt", "line1\nline2\nline3\n", &policy)
            .unwrap();
        // create：已存在 → 报错不覆盖
        assert!(svc.create_if_absent("hfs_sre.txt", "x", &policy).is_err());
        // view：全文带行号
        let view = svc.str_replace_view("hfs_sre.txt", None, &policy).unwrap();
        assert!(
            view.contains("1  line1"),
            "行号视图应含首行，实际: {}",
            view
        );
        assert!(view.contains("3  line3"));
        // view：view_range 区间
        let range = svc
            .str_replace_view("hfs_sre.txt", Some(vec![2, 2]), &policy)
            .unwrap();
        assert!(range.contains("2  line2"));
        assert!(!range.contains("line1"));
        // view_range 非法 → 报错
        assert!(svc
            .str_replace_view("hfs_sre.txt", Some(vec![0, 2]), &policy)
            .is_err());
        // str_replace：唯一匹配
        svc.str_replace("hfs_sre.txt", "line2", "LINE2", &policy)
            .unwrap();
        assert_eq!(
            svc.read_text("hfs_sre.txt", &policy).unwrap(),
            "line1\nLINE2\nline3\n"
        );
        // str_replace：多处匹配 → 报错列行号
        let amb = svc
            .str_replace("hfs_sre.txt", "line", "x", &policy)
            .unwrap_err();
        assert!(
            amb.contains("多处"),
            "多处匹配应列出行号提示，实际: {}",
            amb
        );
        // str_replace：不匹配 → 报错
        assert!(svc.str_replace("hfs_sre.txt", "zzz", "x", &policy).is_err());
        // insert：行 2 后插入
        svc.insert_lines("hfs_sre.txt", 2, "INSERTED", &policy)
            .unwrap();
        assert_eq!(
            svc.read_text("hfs_sre.txt", &policy).unwrap(),
            "line1\nLINE2\nINSERTED\nline3\n"
        );
        // insert：非法行号 → 报错
        assert!(svc.insert_lines("hfs_sre.txt", 99, "x", &policy).is_err());
        // view：目录 2 层列表
        svc.write_text("hfs_sre_dir/nested/deep.txt", "hi", &policy)
            .unwrap();
        let dir_view = svc.str_replace_view("hfs_sre_dir", None, &policy).unwrap();
        assert!(
            dir_view.contains("hfs_sre_dir"),
            "目录视图应含根，实际: {}",
            dir_view
        );
        assert!(dir_view.contains("nested"));
        svc.delete("hfs_sre.txt", &policy).unwrap();
        svc.delete("hfs_sre_dir", &policy).unwrap();
    }

    #[test]
    fn str_replace_editor_view_truncates_at_char_boundary() {
        // B24：超长文件视图按字符边界截断 + <response clipped> 标注
        let svc = FsService;
        let policy = FsPolicy {
            allow_workspace_escape: false,
        };
        let big: String = (0..6000).map(|i| format!("汉行 {}\n", i)).collect();
        svc.write_text("hfs_sre_big.txt", &big, &policy).unwrap();
        let view = svc
            .str_replace_view("hfs_sre_big.txt", None, &policy)
            .unwrap();
        assert!(String::from_utf8(view.clone().into_bytes()).is_ok());
        assert!(view.contains("<response clipped>"));
        assert!(view.chars().count() <= 16_000 + 128);
        svc.delete("hfs_sre_big.txt", &policy).unwrap();
    }
}
