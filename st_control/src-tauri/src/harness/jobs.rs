// ============================================================
// Harness — 后台作业运行时（DSH jobs 迁移）
//
// JobRegistry：进程内作业注册表（DSH jobs-local 等价）。作业由
// exec_command 的 run_in_background=true 启动：进程在后台运行，
// 输出重定向到临时文件；模型工具 job_list / job_output / job_kill
// 按会话隔离读写。完成/失败状态与输出可随时取回，job_kill 强制
// 终止进程并回收临时文件。会话隔离：job_list 仅返回本会话作业，
// job_output/job_kill 校验属主会话。
// ============================================================

use serde::Serialize;
use std::collections::HashMap;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// 作业记录（模型可见视图）
#[derive(Serialize, Clone, Debug)]
pub struct JobRecord {
    pub id: String,
    pub session_id: String,
    pub name: String,
    /// running / done / error / killed
    pub status: String,
    pub created_at: String,
    pub finished_at: Option<String>,
}

struct JobRuntime {
    record: JobRecord,
    stop: Arc<AtomicBool>,
    child: Option<Child>,
    out_path: std::path::PathBuf,
    err_path: std::path::PathBuf,
    /// 进程退出后从临时文件读入的完整输出缓存
    finished_output: Option<String>,
}

struct Registry {
    map: HashMap<String, JobRuntime>,
}

fn registry() -> &'static Mutex<Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(|| {
        Mutex::new(Registry {
            map: HashMap::new(),
        })
    })
}

fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// 启动后台作业（powershell 命令），立即返回作业记录
pub(crate) fn start(session_id: &str, name: &str, command: &str) -> Result<JobRecord, String> {
    let id = format!("job-{}", uuid::Uuid::new_v4().simple());
    let tag = uuid::Uuid::new_v4().simple();
    let out_path = std::env::temp_dir().join(format!("st-job-{tag}.out"));
    let err_path = std::env::temp_dir().join(format!("st-job-{tag}.err"));
    let out_file =
        std::fs::File::create(&out_path).map_err(|e| format!("创建作业输出文件失败: {}", e))?;
    let err_file =
        std::fs::File::create(&err_path).map_err(|e| format!("创建作业错误文件失败: {}", e))?;
    // 锚定当前工作区目录（与前台 exec_command 沙箱锚定一致，防止后台作业
    // 在应用进程 cwd 运行导致沙箱约束失效）
    let cwd = super::workspace::sandbox_root();
    let child = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", command])
        .current_dir(&cwd)
        .stdout(std::process::Stdio::from(out_file))
        .stderr(std::process::Stdio::from(err_file))
        .spawn()
        .map_err(|e| format!("启动后台命令失败: {}", e))?;
    let record = JobRecord {
        id: id.clone(),
        session_id: session_id.to_string(),
        name: name.to_string(),
        status: "running".to_string(),
        created_at: now_iso(),
        finished_at: None,
    };
    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut reg = registry().lock().unwrap();
        reg.map.insert(
            id.clone(),
            JobRuntime {
                record: record.clone(),
                stop: Arc::clone(&stop),
                child: Some(child),
                out_path: out_path.clone(),
                err_path: err_path.clone(),
                finished_output: None,
            },
        );
    }
    // 后台线程：取出运行时数据（短锁）→ 无锁等待退出 → 短锁回写状态。
    // 注意：等待循环期间绝不持有 registry 锁，否则 job_kill 无法置位停止标志
    // （同一把 Mutex 不可重入），形成互相等待的死锁。
    let id2 = id.clone();
    std::thread::spawn(move || {
        let (mut child, stop, out_path, err_path) = {
            let mut reg = registry().lock().unwrap();
            let Some(job) = reg.map.get_mut(&id2) else {
                return;
            };
            let Some(child) = job.child.take() else {
                return;
            };
            (
                child,
                Arc::clone(&job.stop),
                job.out_path.clone(),
                job.err_path.clone(),
            )
        };
        let status = loop {
            match child.try_wait() {
                Ok(Some(code)) => {
                    break if stop.load(Ordering::SeqCst) {
                        "killed".to_string()
                    } else if code.success() {
                        "done".to_string()
                    } else {
                        "error".to_string()
                    };
                }
                Ok(None) => {
                    if stop.load(Ordering::SeqCst) {
                        // 进程树级终止（DSH subprocess 语义）
                        if !super::shell::kill_tree(child.id()) {
                            let _ = child.kill();
                        }
                        let _ = child.wait();
                        break "killed".to_string();
                    }
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                Err(_) => break "error".to_string(),
            }
        };
        let out = std::fs::read_to_string(&out_path).unwrap_or_default();
        let err = std::fs::read_to_string(&err_path).unwrap_or_default();
        let mut text = if out.trim().is_empty() && !err.trim().is_empty() {
            err
        } else {
            out
        };
        // 字符边界安全截断（中文内容不得在多字节字符中间 panic）
        text.truncate(text.floor_char_boundary(64 * 1024));
        {
            let mut reg = registry().lock().unwrap();
            if let Some(job) = reg.map.get_mut(&id2) {
                job.finished_output = Some(text.clone());
                job.record.status = status.clone();
                job.record.finished_at = Some(now_iso());
            }
        }
        let _ = std::fs::remove_file(&out_path);
        let _ = std::fs::remove_file(&err_path);
        log::info!("[harness.jobs] 作业 {id2} 结束（{status}）");
    });
    Ok(record)
}

/// 已完成作业记录的保留时长（M7：防注册表与 job_list 无界增长）
const FINISHED_RETENTION_SECS: i64 = 3600;

/// 惰性清理：完成超过保留期的作业记录移除（运行中/未到期的保留；
/// list 等读取入口触发，无独立清理线程）
fn prune_finished() {
    let mut reg = registry().lock().unwrap();
    let now = chrono::Local::now().naive_local();
    reg.map.retain(|_, j| {
        if j.record.status == "running" {
            return true;
        }
        match &j.record.finished_at {
            Some(ts) => chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S")
                .map(|t| now.signed_duration_since(t).num_seconds() < FINISHED_RETENTION_SECS)
                .unwrap_or(true), // 解析失败保守保留
            None => true,
        }
    });
}

/// 作业列表（本会话隔离；先惰性清理过期已完成记录）
pub(crate) fn list(session_id: &str) -> Vec<JobRecord> {
    prune_finished();
    let reg = registry().lock().unwrap();
    let mut v: Vec<JobRecord> = reg
        .map
        .values()
        .filter(|j| j.record.session_id == session_id)
        .map(|j| j.record.clone())
        .collect();
    v.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    v
}

/// 读取作业输出：运行中读临时文件尾部，结束后读缓存
pub(crate) fn output(id: &str) -> Result<String, String> {
    let mut reg = registry().lock().unwrap();
    let Some(job) = reg.map.get_mut(id) else {
        return Err(format!("作业不存在: {id}"));
    };
    if let Some(finished) = &job.finished_output {
        return Ok(finished.clone());
    }
    // 运行中：读临时文件（有界尾部），不阻塞
    let mut out = std::fs::read_to_string(&job.out_path).unwrap_or_default();
    let err = std::fs::read_to_string(&job.err_path).unwrap_or_default();
    if out.trim().is_empty() && !err.trim().is_empty() {
        out = err;
    }
    // 字符边界安全截断（避免在多字节字符中间切出 panic）
    if out.len() > 64 * 1024 {
        let start = out.floor_char_boundary(out.len() - 64 * 1024);
        out = out[start..].to_string();
    }
    if out.is_empty() {
        Ok("（作业运行中，暂无输出）".to_string())
    } else {
        Ok(out)
    }
}

/// 终止作业：置停止标志（后台线程负责 kill 与回收）
pub(crate) fn kill(id: &str) -> Result<(), String> {
    let reg = registry().lock().unwrap();
    let Some(job) = reg.map.get(id) else {
        return Err(format!("作业不存在: {id}"));
    };
    job.stop.store(true, Ordering::SeqCst);
    Ok(())
}

/// 校验作业属主会话（会话隔离）
pub(crate) fn check_owner(id: &str, session_id: &str) -> Result<(), String> {
    let reg = registry().lock().unwrap();
    match reg.map.get(id) {
        Some(job) if job.record.session_id == session_id => Ok(()),
        Some(_) => Err(format!("作业不存在: {id}")),
        None => Err(format!("作业不存在: {id}")),
    }
}

// ─── IPC ───

#[tauri::command]
pub async fn harness_job_list(session_id: String) -> Result<Vec<JobRecord>, String> {
    Ok(list(&session_id))
}

#[tauri::command]
pub async fn harness_job_output(id: String) -> Result<String, String> {
    output(&id)
}

#[tauri::command]
pub async fn harness_job_kill(id: String) -> Result<(), String> {
    kill(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_lifecycle_background_command() {
        // 后台作业：写文件并输出标记后退出（幂等：每次用唯一文件名）
        let tag = uuid::Uuid::new_v4().simple();
        let target = std::env::temp_dir().join(format!("st-job-test-{tag}.txt"));
        let cmd = format!(
            "Start-Sleep -Milliseconds 300; Write-Output 'JOB_OUT_XYZ'; Set-Content '{}' 'JOB_OK'",
            target.display()
        );
        let rec = start("sess-1", "测试作业", &cmd).unwrap();
        assert_eq!(rec.status, "running");
        assert!(list("sess-1").iter().any(|j| j.id == rec.id));
        assert!(list("sess-2").is_empty()); // 会话隔离
                                            // 等完成（满负载并行测试下 powershell 启动可能较慢，时限放宽）
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let status = {
                let reg = registry().lock().unwrap();
                reg.map.get(&rec.id).map(|j| j.record.status.clone())
            };
            if status.as_deref() == Some("done") {
                break;
            }
            if std::time::Instant::now() > deadline {
                panic!("作业超时未完成");
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(std::fs::read_to_string(&target).unwrap().contains("JOB_OK"));
        let out = output(&rec.id).unwrap();
        assert!(out.contains("JOB_OUT_XYZ"), "输出应含 stdout 标记: {out}");
        check_owner(&rec.id, "sess-1").unwrap();
        assert!(check_owner(&rec.id, "sess-2").is_err());
        // 清理：进程已退出，直接移除运行时记录
        {
            let mut reg = registry().lock().unwrap();
            reg.map.remove(&rec.id);
        }
        let _ = std::fs::remove_file(&target);
    }

    #[test]
    fn job_kill_terminates() {
        let rec = start("sess-k", "长任务", "Start-Sleep 300").unwrap();
        // 等 powershell 真正起来再杀（否则 kill 落空、测试变成等自然退出）
        std::thread::sleep(std::time::Duration::from_secs(1));
        kill(&rec.id).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let status = {
                let reg = registry().lock().unwrap();
                reg.map.get(&rec.id).map(|j| j.record.status.clone())
            };
            if status.as_deref() == Some("killed") {
                break;
            }
            if std::time::Instant::now() > deadline {
                panic!("kill 超时未生效");
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        {
            let mut reg = registry().lock().unwrap();
            reg.map.remove(&rec.id);
        }
    }

    #[test]
    fn job_output_tail_truncates_at_char_boundary_with_chinese() {
        // H2 回归：>64KB 中文输出（3 万汉字 ≈ 90KB）的尾部截断不得在
        // 多字节字符中间 panic（完成缓存与运行中读取两条路径）
        let cmd = "(1..30000 | ForEach-Object { '汉' }) -join ''";
        let rec = start("sess-cn", "中文长输出", cmd).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let status = {
                let reg = registry().lock().unwrap();
                reg.map.get(&rec.id).map(|j| j.record.status.clone())
            };
            if status.as_deref() == Some("done") {
                break;
            }
            if std::time::Instant::now() > deadline {
                panic!("作业超时未完成");
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let out = output(&rec.id).unwrap();
        assert!(
            String::from_utf8(out.clone().into_bytes()).is_ok(),
            "输出必须是有效 UTF-8（未在字符中间截断）"
        );
        assert!(out.len() <= 64 * 1024, "输出应被截断到 64KB 内");
        {
            let mut reg = registry().lock().unwrap();
            reg.map.remove(&rec.id);
        }
    }

    #[test]
    fn finished_jobs_pruned_after_retention() {
        // M7：完成超过保留期（1 小时）的作业记录在 list 时被惰性清理，
        // 新完成的保留；运行中/解析失败保守保留
        let old_ts = (chrono::Local::now().naive_local() - chrono::Duration::hours(2))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        let fresh_ts = now_iso();
        {
            let mut reg = registry().lock().unwrap();
            reg.map.insert(
                "job-old".to_string(),
                JobRuntime {
                    record: JobRecord {
                        id: "job-old".into(),
                        session_id: "sess-prune".into(),
                        name: "旧作业".into(),
                        status: "done".into(),
                        created_at: old_ts.clone(),
                        finished_at: Some(old_ts),
                    },
                    stop: Arc::new(AtomicBool::new(false)),
                    child: None,
                    out_path: std::path::PathBuf::new(),
                    err_path: std::path::PathBuf::new(),
                    finished_output: Some("x".into()),
                },
            );
            reg.map.insert(
                "job-fresh".to_string(),
                JobRuntime {
                    record: JobRecord {
                        id: "job-fresh".into(),
                        session_id: "sess-prune".into(),
                        name: "新作业".into(),
                        status: "done".into(),
                        created_at: fresh_ts.clone(),
                        finished_at: Some(fresh_ts),
                    },
                    stop: Arc::new(AtomicBool::new(false)),
                    child: None,
                    out_path: std::path::PathBuf::new(),
                    err_path: std::path::PathBuf::new(),
                    finished_output: Some("y".into()),
                },
            );
        }
        let got = list("sess-prune");
        assert_eq!(got.len(), 1, "过期作业应被清理: {got:?}");
        assert_eq!(got[0].id, "job-fresh");
        {
            let mut reg = registry().lock().unwrap();
            reg.map.remove("job-old");
            reg.map.remove("job-fresh");
        }
    }
}
