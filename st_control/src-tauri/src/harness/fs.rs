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
        // 上限 64KB，超出截断（字符边界安全，避免中文内容 panic）
        Ok(if text.len() > 64 * 1024 {
            let end = text.floor_char_boundary(64 * 1024);
            format!("{}…（内容过长已截断）", &text[..end])
        } else {
            text.into_owned()
        })
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
        std::fs::write(&p, content).map_err(|e| format!("写入失败: {}", e))?;
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
        Ok(format!("文件已编辑成功: {}", p.display()))
    }

    /// glob 文件发现（DSH glob 工具语义）：当前工作区内匹配，支持 **、*、?
    pub fn glob(&self, pattern: &str, policy: &FsPolicy) -> Result<Vec<String>, String> {
        let pattern = pattern.trim().replace('\\', "/");
        if pattern.is_empty() {
            return Err("pattern 不能为空".to_string());
        }
        let root = crate::harness::workspace::sandbox_root();
        // 模式锚定：以 / 开头视为相对工作区根
        let pattern = pattern.trim_start_matches('/').to_string();
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for e in entries.flatten() {
                let path = e.path();
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir && is_heavy_dir(&e.file_name().to_string_lossy()) {
                    continue; // 跳过构建/依赖重目录（项目根工作区下避免扫 target/node_modules）
                }
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if is_dir {
                    // ** 段可匹配任意深度：目录也尝试匹配，并继续下钻
                    if glob_match(&pattern, &rel) {
                        out.push(format!("{}/", rel));
                    }
                    stack.push(path);
                    if out.len() > 2000 {
                        break;
                    }
                } else if glob_match(&pattern, &rel) {
                    out.push(rel);
                    if out.len() > 2000 {
                        break;
                    }
                }
            }
        }
        out.sort();
        out.truncate(200);
        if out.is_empty() {
            return Ok(Vec::new());
        }
        let _ = policy; // 工作区锚定即沙箱约束
        Ok(out)
    }

    /// grep 文本搜索（DSH grep 工具语义）：regex 匹配，返回 file:line:内容
    pub fn grep(&self, pattern: &str, path: &str, policy: &FsPolicy) -> Result<String, String> {
        let re = regex::Regex::new(pattern).map_err(|e| format!("正则表达式无效: {e}"))?;
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
        let mut out_lines = Vec::new();
        'outer: for f in files {
            if out_lines.len() >= 200 {
                break;
            }
            let Ok(text) = std::fs::read_to_string(&f) else {
                continue;
            };
            let rel = f
                .strip_prefix(crate::harness::workspace::sandbox_root())
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
                    if out_lines.len() >= 200 {
                        break 'outer;
                    }
                }
            }
        }
        if out_lines.is_empty() {
            return Ok("（无匹配）".to_string());
        }
        Ok(out_lines.join("\n"))
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

/// glob 模式匹配（** 跨目录、* 任意字符、? 单字符；纯函数便于单测）
fn glob_match(pattern: &str, path: &str) -> bool {
    let ps: Vec<&str> = pattern.split('/').collect();
    let xs: Vec<&str> = path.split('/').collect();
    // 段级递归匹配
    fn seg_match(p: &str, x: &str) -> bool {
        // 单段 glob：* / ? 展开
        let pc: Vec<char> = p.chars().collect();
        let xc: Vec<char> = x.chars().collect();
        let mut dp = vec![vec![false; xc.len() + 1]; pc.len() + 1];
        dp[0][0] = true;
        for i in 0..pc.len() {
            if pc[i] == '*' {
                dp[i + 1][0] = dp[i][0];
            }
        }
        for i in 0..pc.len() {
            for j in 0..xc.len() {
                match pc[i] {
                    '*' => dp[i + 1][j + 1] = dp[i][j + 1] || dp[i + 1][j],
                    '?' => dp[i + 1][j + 1] = dp[i][j],
                    c => dp[i + 1][j + 1] = dp[i][j] && c == xc[j],
                }
            }
        }
        dp[pc.len()][xc.len()]
    }
    fn rec(pi: usize, xi: usize, ps: &[&str], xs: &[&str]) -> bool {
        if pi == ps.len() {
            return xi == xs.len();
        }
        if ps[pi] == "**" {
            // ** 可匹配零段或任意多段
            for k in xi..=xs.len() {
                if rec(pi + 1, k, ps, xs) {
                    return true;
                }
            }
            return false;
        }
        if xi == xs.len() {
            return false;
        }
        seg_match(ps[pi], xs[xi]) && rec(pi + 1, xi + 1, ps, xs)
    }
    rec(0, 0, &ps, &xs)
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
    let p = std::path::PathBuf::from(path.trim());
    if !p.exists() {
        return Err(format!("路径不存在: {}", p.display()));
    }
    // L3：沙箱校验——非越界模式下仅允许打开工作区（含附件，复制进工作区）
    // 内的路径，防止模型/用户输入打开任意系统路径（cmd /c start 任意目标）
    let policy = FsPolicy::current();
    if !policy.allow_workspace_escape {
        let root = crate::harness::workspace::sandbox_root();
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
    fn glob_match_basics() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(!glob_match("*.rs", "main.ts"));
        assert!(glob_match("src/**/*.rs", "src/lib/a.rs"));
        assert!(glob_match("src/**/*.rs", "src/a.rs"));
        assert!(glob_match("**/README.md", "a/b/README.md"));
        assert!(glob_match("a?c.txt", "abc.txt"));
        assert!(!glob_match("a?c.txt", "ac.txt"));
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
        let hits = svc.glob("hfs_glob/*.txt", &policy).unwrap();
        assert_eq!(hits.len(), 2);
        let g = svc.grep("needle", "hfs_glob", &policy).unwrap();
        assert!(g.contains("hfs_glob/a.txt:1: needle-one"));
        assert!(!g.contains("b.txt"));
        svc.delete("hfs_glob", &policy).unwrap();
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
