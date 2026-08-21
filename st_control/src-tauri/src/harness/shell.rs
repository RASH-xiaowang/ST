// ============================================================
// Harness — Shell 能力（DSH shell 迁移）
//
// 能力接缝三角色：
// - Service Definition：ShellService::run（command + 可选 cwd + 超时）
// - Service Provider：本地 PowerShell（输出重定向临时文件防死锁，
//   超时强制终止）；受限执行世界：默认 cwd 限制在 agent_workspace
//   （SandboxPolicy：allow_workspace_escape 时放行）
// - Consumer：exec_command 工具（AI 聊天 + Harness 共用核心）、
//   终端会话、人工命令 shell_run
// ============================================================

use serde::Serialize;

/// 命令执行结果
#[derive(Serialize, Clone, Debug)]
pub struct ShellResult {
    pub ok: bool,
    pub output: String,
    pub timed_out: bool,
    pub duration_ms: u64,
}

/// 受限执行世界政策（来自用户设置）
#[derive(Clone, Debug, Default)]
pub struct SandboxPolicy {
    /// 允许在 agent_workspace 之外执行（默认 false）
    pub allow_workspace_escape: bool,
}

impl SandboxPolicy {
    pub fn current() -> Self {
        SandboxPolicy {
            allow_workspace_escape: crate::harness::settings::current()
                .effective_workspace_escape(),
        }
    }
}

/// 进程树级终止（DSH subprocess tree-scoped terminate 语义）：
/// taskkill /T /F 终止整个进程树（含孙进程），失败时回退直接 kill。
/// 返回是否成功发起树级终止。
pub fn kill_tree(pid: u32) -> bool {
    let out = std::process::Command::new("taskkill.exe")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
    match out {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

/// Shell 能力服务（本地 PowerShell 提供者）
pub struct ShellService;

impl ShellService {
    /// 沙箱根（跟随当前工作区：默认 = 应用项目根）
    pub fn workspace_root() -> std::path::PathBuf {
        crate::harness::workspace::sandbox_root()
    }

    /// 校验 cwd：受限世界下必须位于工作区内（目录自身规范化 + 归属校验）
    pub fn resolve_cwd(&self, cwd: Option<&str>, policy: &SandboxPolicy) -> Result<String, String> {
        let cwd = cwd.filter(|s| !s.trim().is_empty());
        match cwd {
            Some(c) if !policy.allow_workspace_escape => {
                let raw = if std::path::Path::new(c).is_absolute() {
                    std::path::PathBuf::from(c)
                } else {
                    Self::workspace_root().join(c)
                };
                let canon = raw
                    .canonicalize()
                    .map_err(|e| format!("目录不存在或不可访问: {}", e))?;
                let root_canon = Self::workspace_root()
                    .canonicalize()
                    .map_err(|e| format!("工作区异常: {}", e))?;
                if !canon.starts_with(&root_canon) {
                    return Err("路径超出允许的工作区范围".to_string());
                }
                Ok(canon.display().to_string())
            }
            Some(c) => Ok(c.to_string()),
            None if policy.allow_workspace_escape => Ok(String::new()),
            None => Ok(Self::workspace_root().display().to_string()),
        }
    }

    /// 执行命令（同步）：cwd 为 None 时用工作区根（受限世界）
    pub fn run(&self, command: &str, cwd: Option<&str>, timeout_secs: u64) -> ShellResult {
        let policy = SandboxPolicy::current();
        self.run_with_policy(command, cwd, timeout_secs, &policy)
    }

    /// 带显式政策的执行（逐调用升级：danger-full-access 审批后越界执行）
    pub fn run_with_policy(
        &self,
        command: &str,
        cwd: Option<&str>,
        timeout_secs: u64,
        policy: &SandboxPolicy,
    ) -> ShellResult {
        let started = std::time::Instant::now();
        let cwd = match self.resolve_cwd(cwd, policy) {
            Ok(c) => c,
            Err(e) => {
                return ShellResult {
                    ok: false,
                    output: e,
                    timed_out: false,
                    duration_ms: 0,
                }
            }
        };
        let effective = if cwd.is_empty() {
            command.to_string()
        } else {
            // 受限世界：命令前先进入工作目录（路径已转义为单引号字面量）
            format!("Set-Location -LiteralPath '{}'; {}", cwd, command)
        };
        let tag = uuid::Uuid::new_v4().simple();
        let out_path = std::env::temp_dir().join(format!("st-shell-{tag}.out"));
        let err_path = std::env::temp_dir().join(format!("st-shell-{tag}.err"));
        let out_file = match std::fs::File::create(&out_path) {
            Ok(f) => f,
            Err(e) => {
                return ShellResult {
                    ok: false,
                    output: format!("创建输出文件失败: {}", e),
                    timed_out: false,
                    duration_ms: 0,
                }
            }
        };
        let err_file = match std::fs::File::create(&err_path) {
            Ok(f) => f,
            Err(e) => {
                return ShellResult {
                    ok: false,
                    output: format!("创建输出文件失败: {}", e),
                    timed_out: false,
                    duration_ms: 0,
                }
            }
        };
        let mut cmd = std::process::Command::new("powershell.exe");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &effective])
            .stdout(std::process::Stdio::from(out_file))
            .stderr(std::process::Stdio::from(err_file));
        // 凭据注入（credentials：HARNESS_CREDENTIAL_<KEY>）
        crate::harness::credentials::inject_env(&mut cmd);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = std::fs::remove_file(&out_path);
                let _ = std::fs::remove_file(&err_path);
                return ShellResult {
                    ok: false,
                    output: format!("执行失败: {}", e),
                    timed_out: false,
                    duration_ms: 0,
                };
            }
        };
        let deadline = started + std::time::Duration::from_secs(timeout_secs);
        let mut timed_out = false;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if std::time::Instant::now() > deadline {
                        // 进程树级终止：命令可能派生孙进程（DSH 语义）
                        if !kill_tree(child.id()) {
                            let _ = child.kill();
                        }
                        let _ = child.wait();
                        timed_out = true;
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    if !kill_tree(child.id()) {
                        let _ = child.kill();
                    }
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&out_path);
                    let _ = std::fs::remove_file(&err_path);
                    return ShellResult {
                        ok: false,
                        output: format!("等待命令失败: {}", e),
                        timed_out: false,
                        duration_ms: started.elapsed().as_millis() as u64,
                    };
                }
            }
        }
        let mut text = std::fs::read(&out_path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let err_text = std::fs::read(&err_path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let _ = std::fs::remove_file(&out_path);
        let _ = std::fs::remove_file(&err_path);
        if !err_text.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str("[stderr] ");
            text.push_str(&err_text);
        }
        let text = truncate_8k(&text);
        ShellResult {
            ok: !timed_out,
            output: if timed_out {
                format!(
                    "命令执行超时（{} 秒），进程已强制终止。已产生的输出：\n{}",
                    timeout_secs,
                    if text.is_empty() {
                        "（无输出）".to_string()
                    } else {
                        text
                    }
                )
            } else if text.is_empty() {
                "命令执行完成（无输出）".to_string()
            } else {
                text
            },
            timed_out,
            duration_ms: started.elapsed().as_millis() as u64,
        }
    }
}

fn truncate_8k(s: &str) -> String {
    // 字符边界安全截断（中文内容不得在字节中间 panic）
    if s.len() > 8192 {
        let end = s.floor_char_boundary(8192);
        format!("{}…（输出过长已截断）", &s[..end])
    } else {
        s.to_string()
    }
}

/// 注册 Shell 能力（Cordis-lite 服务）
pub fn provide_service() -> crate::harness::registry::Disposer {
    crate::harness::registry::provide("harness.shell", std::sync::Arc::new(ShellService))
}

/// 人工命令：执行一条 PowerShell（受限执行世界 + 可配置超时）
#[tauri::command]
pub async fn harness_shell_run(
    command: String,
    cwd: Option<String>,
    timeout_secs: Option<u64>,
) -> Result<ShellResult, String> {
    let svc = crate::harness::registry::get::<ShellService>("harness.shell")
        .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    let timeout = timeout_secs
        .unwrap_or(crate::harness::settings::current().effective_timeout_secs())
        .clamp(1, 300);
    if command.trim().is_empty() {
        return Err("命令不能为空".to_string());
    }
    Ok(svc.run(command.trim(), cwd.as_deref(), timeout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_run_echo() {
        let svc = ShellService;
        let r = svc.run("Write-Output hello-shell", None, 30);
        assert!(r.ok, "{:?}", r);
        assert!(r.output.contains("hello-shell"));
    }

    #[test]
    fn shell_timeout_kills() {
        let svc = ShellService;
        let r = svc.run("Start-Sleep 3", None, 1);
        assert!(!r.ok && r.timed_out, "{:?}", r);
        assert!(r.output.contains("超时"));
    }

    #[test]
    fn shell_cwd_confined_to_workspace() {
        let svc = ShellService;
        let policy = SandboxPolicy {
            allow_workspace_escape: false,
        };
        assert!(svc.resolve_cwd(Some("C:/Windows"), &policy).is_err());
        // 工作区内相对目录（先创建）放行
        let sub = ShellService::workspace_root().join("sub");
        std::fs::create_dir_all(&sub).ok();
        assert!(svc.resolve_cwd(Some("sub"), &policy).is_ok());
        let _ = std::fs::remove_dir_all(&sub);
        let open = SandboxPolicy {
            allow_workspace_escape: true,
        };
        assert!(svc.resolve_cwd(Some("C:/Windows"), &open).is_ok());
    }

    #[test]
    fn truncate_8k_char_boundary_safe_with_chinese() {
        // H2 同类：超长中文输出按字符边界截断不 panic
        let short = "短输出";
        assert_eq!(truncate_8k(short), short, "短输出原样");
        // 3 万汉字 ≈ 90KB > 8KB，截断点必落在多字节字符内
        let big: String = "汉".repeat(30_000);
        let t = truncate_8k(&big);
        assert!(
            String::from_utf8(t.clone().into_bytes()).is_ok(),
            "截断结果须有效 UTF-8"
        );
        assert!(t.contains("输出过长已截断"));
        assert!(t.len() <= 8192 + 64, "截断后应接近 8KB 上限");
        // ASCII 边界
        let ascii = "x".repeat(9000);
        let t = truncate_8k(&ascii);
        assert!(t.contains("输出过长已截断"));
    }
}
