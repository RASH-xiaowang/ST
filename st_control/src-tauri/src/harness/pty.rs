// ============================================================
// Harness — PTY 真终端（ConPTY）
//
// DSH PTY 迁移：CreatePseudoConsole + CreateProcessW（
// EXTENDED_STARTUPINFO_PRESENT + PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE）
// 把交互式 shell 挂进伪终端。输入管道写 UTF-8（\r 提交行），输出管道
// 由独占读线程持续收取 UTF-8。旧系统（CreatePseudoConsole 返回
// 不支持）或启动失败时由调用方优雅降级到非 PTY 状态保持终端
// （terminal.rs 原有实现，每次 send 独立进程 + cwd 标记）。
// ============================================================

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
    TerminateProcess, UpdateProcThreadAttribute, EXTENDED_STARTUPINFO_PRESENT,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
    STARTUPINFOEXW,
};

/// cwd 定位标记（命令尾部注入，解析输出末尾行；与 terminal.rs 一致）
pub(crate) const CWD_MARKER: &str = "__HNS_CWD__";

/// 默认 PTY 尺寸（列 x 行）
const DEFAULT_COLS: i16 = 120;
const DEFAULT_ROWS: i16 = 30;

struct Pty {
    hpc: HPCON,
    /// ConPTY 直接持有的两端（input 读端 / output 写端）：停止时随伪终端一并关闭
    conpty_input: HANDLE,
    conpty_output: HANDLE,
    input_write: HANDLE,
    output_read: HANDLE,
    process: PROCESS_INFORMATION,
    buffer: Arc<Mutex<Vec<u8>>>,
    exited: Arc<AtomicBool>,
    cols: i16,
    rows: i16,
}

/// windows-rs 的 HANDLE 包装裸指针、未实现 Send/Sync。
/// 本模块独占这些 OS 资源的句柄所有权：输出读端只在读线程中使用，
/// 其余字段的跨线程访问全部经外层 Mutex 串行化；读线程先于 Pty 释放
/// 退出（ClosePseudoConsole 使读端断开）。因此此标记是安全的。
struct PtyBox(Pty);
unsafe impl Send for PtyBox {}
unsafe impl Sync for PtyBox {}

/// 读线程独占句柄：所有权随闭包移动进读线程，线程内独占使用
struct SendHandle(HANDLE);
unsafe impl Send for SendHandle {}
impl SendHandle {
    /// 经方法取原始句柄：方法调用使 move 闭包整体捕获 SendHandle
    /// （避免 Rust 2021 分离捕获按字段捕获 HANDLE 而丢失 Send 标记）
    fn raw(&self) -> HANDLE {
        self.0
    }
}

fn ptys() -> &'static Mutex<HashMap<String, PtyBox>> {
    static P: OnceLock<Mutex<HashMap<String, PtyBox>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 停止并清理指定终端的 PTY（不存在时静默成功）
pub(crate) fn stop(id: &str) {
    if let Some(pty) = ptys().lock().unwrap().remove(id) {
        let p = pty.0;
        unsafe {
            // 先关伪终端：读线程随即因管道断开退出
            ClosePseudoConsole(p.hpc);
            let _ = TerminateProcess(p.process.hProcess, 1);
            let _ = CloseHandle(p.process.hProcess);
            let _ = CloseHandle(p.process.hThread);
            let _ = CloseHandle(p.conpty_input);
            let _ = CloseHandle(p.conpty_output);
            let _ = CloseHandle(p.input_write);
            let _ = CloseHandle(p.output_read);
        }
    }
}

/// 启动 PTY：powershell.exe 挂进伪终端，cwd 取会话工作目录
pub(crate) fn start(
    id: &str,
    cwd: &str,
    rows: Option<u16>,
    cols: Option<u16>,
) -> Result<(), String> {
    stop(id); // 重复启动先回收旧 PTY
    let rows = rows.unwrap_or(DEFAULT_ROWS as u16).clamp(2, 300) as i16;
    let cols = cols.unwrap_or(DEFAULT_COLS as u16).clamp(20, 500) as i16;

    // ── 管道（可继承句柄：子进程经 STARTF_USESTDHANDLES 直接持有） ──
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: windows::core::BOOL(1),
    };
    let mut input_read = HANDLE(std::ptr::null_mut());
    let mut input_write = HANDLE(std::ptr::null_mut());
    let mut output_read = HANDLE(std::ptr::null_mut());
    let mut output_write = HANDLE(std::ptr::null_mut());
    unsafe {
        CreatePipe(&mut input_read, &mut input_write, Some(&sa), 0)
            .map_err(|e| format!("创建输入管道失败: {e}"))?;
        CreatePipe(&mut output_read, &mut output_write, Some(&sa), 0)
            .map_err(|e| format!("创建输出管道失败: {e}"))?;
    }

    // ── 伪终端（返回值即 HPCON） ──
    let hpc = unsafe {
        CreatePseudoConsole(COORD { X: cols, Y: rows }, input_read, output_write, 0).map_err(
            |e| {
                format!(
                    "CreatePseudoConsole 失败（系统可能不支持 ConPTY，可降级为普通命令模式）: {e}"
                )
            },
        )?
    };

    // ── 进程属性 + 启动 ──
    let pi: PROCESS_INFORMATION = unsafe {
        let mut attr_size: usize = 0;
        // 第一次调用只取所需大小（预期返回缓冲区不足错误）
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut attr_size);
        let mut attr_buf = vec![0u8; attr_size];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as *mut _);
        InitializeProcThreadAttributeList(Some(list), 1, None, &mut attr_size)
            .map_err(|e| format!("初始化进程属性失败: {e}"))?;
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            Some(hpc.0 as *const std::ffi::c_void),
            std::mem::size_of::<HPCON>(),
            None,
            None,
        )
        .map_err(|e| format!("设置伪终端属性失败: {e}"))?;

        let mut si: STARTUPINFOEXW = std::mem::zeroed();
        si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        si.lpAttributeList = list;
        // 关键修复：显式把 ConPTY 输出端作为子进程 stdout/stderr。
        // 控制台子系统父进程（debug 构建 st-control.exe）的 stdout 被重定向时，
        // 其子进程会继承该重定向句柄（绕过伪终端），导致 PTY 输出丢失。
        // stdin 保持 NULL：交互式 shell 经 ConPTY 控制台输入缓冲区读取命令
        // （input_read 由 ConPTY 独占，避免子进程 stdin 与 ConPTY 争抢同一管道）。
        si.StartupInfo.dwFlags = windows::Win32::System::Threading::STARTF_USESTDHANDLES;
        si.StartupInfo.hStdInput = HANDLE(std::ptr::null_mut());
        si.StartupInfo.hStdOutput = output_write;
        si.StartupInfo.hStdError = output_write;
        let mut cmd: Vec<u16> = "powershell.exe -NoLogo"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let cwd_wide: Vec<u16> = cwd.encode_utf16().chain(std::iter::once(0)).collect();
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        let spawn = CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(cmd.as_mut_ptr())),
            None,
            None,
            true,
            EXTENDED_STARTUPINFO_PRESENT,
            None,
            PCWSTR(cwd_wide.as_ptr()),
            &si.StartupInfo,
            &mut pi,
        );
        DeleteProcThreadAttributeList(list);
        spawn.map_err(|e| {
            ClosePseudoConsole(hpc);
            let _ = CloseHandle(input_read);
            let _ = CloseHandle(input_write);
            let _ = CloseHandle(output_read);
            let _ = CloseHandle(output_write);
            format!("启动 shell 失败: {e}")
        })?;
        // 注意：ConPTY 直接使用传入的两端管道句柄（不复制），
        // input_read / output_write 必须保持打开直到 PTY 停止（stop() 中关闭）
        pi
    };
    log::info!(
        "[harness.pty] shell 已启动 pid={} hpc={}",
        pi.dwProcessId,
        hpc.0
    );

    // ── 读线程：独占输出读端，收取 UTF-8 到共享缓冲 ──
    let buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let exited = Arc::new(AtomicBool::new(false));
    {
        let buffer = buffer.clone();
        let exited = exited.clone();
        // 复制读端句柄给读线程独占：即使原句柄值被误关闭（句柄值复用），
        // 读线程持有的副本仍有效
        let mut dup_read = HANDLE(std::ptr::null_mut());
        unsafe {
            let _ = windows::Win32::Foundation::DuplicateHandle(
                windows::Win32::System::Threading::GetCurrentProcess(),
                output_read,
                windows::Win32::System::Threading::GetCurrentProcess(),
                &mut dup_read,
                0,
                false,
                windows::Win32::Foundation::DUPLICATE_SAME_ACCESS,
            );
        }
        let reader = SendHandle(if !dup_read.is_invalid() {
            dup_read
        } else {
            output_read
        });
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            loop {
                let mut read: u32 = 0;
                let ok = unsafe { ReadFile(reader.raw(), Some(&mut chunk), Some(&mut read), None) };
                if ok.is_err() {
                    log::info!("[harness.pty] 读线程退出: {ok:?}");
                    break; // 管道断开（伪终端关闭/进程退出）
                }
                if read > 0 {
                    buffer
                        .lock()
                        .unwrap()
                        .extend_from_slice(&chunk[..read as usize]);
                }
            }
            exited.store(true, Ordering::SeqCst);
        });
    }

    ptys().lock().unwrap().insert(
        id.to_string(),
        PtyBox(Pty {
            hpc,
            conpty_input: input_read,
            conpty_output: output_write,
            input_write,
            output_read,
            process: pi,
            buffer,
            exited,
            cols,
            rows,
        }),
    );
    // 等待首屏输出（横幅 + 提示符）落缓冲；首次 send 前统一清空
    std::thread::sleep(std::time::Duration::from_millis(800));
    Ok(())
}

/// 清空并返回当前缓冲内容（剥离 ANSI 转义）
fn drain(id: &str) -> String {
    let mut map = ptys().lock().unwrap();
    let Some(p) = map.get_mut(id) else {
        return String::new();
    };
    let text = {
        let mut buf = p.0.buffer.lock().unwrap();
        let text = String::from_utf8_lossy(&buf).to_string();
        buf.clear();
        text
    };
    strip_ansi(&text)
}

/// 发送命令到 PTY（\r\n 提交行——管道 stdin 的行读取器按 \n 判行结束；
/// 尾部注入 cwd 定位标记作为命令完成信号），返回输出
pub(crate) fn send(id: &str, input: &str) -> Result<String, String> {
    let effective = format!(
        "{}; Write-Output ('{}' + (Get-Location).Path)\r\n",
        input, CWD_MARKER
    );
    {
        let mut map = ptys().lock().unwrap();
        let Some(p) = map.get_mut(id) else {
            return Err("该终端未启动 PTY（请先「启动 PTY」，或使用普通命令模式）".to_string());
        };
        if p.0.exited.load(Ordering::SeqCst) {
            map.remove(id);
            return Err("PTY 进程已退出，请重新启动".to_string());
        }
        // 清空旧输出（首屏横幅等），确保本次 drain 只捕获本命令的输出
        p.0.buffer.lock().unwrap().clear();
        let bytes = effective.as_bytes();
        let mut written: u32 = 0;
        unsafe {
            WriteFile(p.0.input_write, Some(bytes), Some(&mut written), None)
                .map_err(|e| format!("写入 PTY 失败: {e}"))?;
        }
    }
    // 等待命令完成：注入的 cwd 定位标记（命令末尾语句）作为完成信号，
    // 最多等 8 秒（覆盖 shell 启动/命令执行延迟）；期间轮询共享缓冲
    let buffer_arc = {
        let map = ptys().lock().unwrap();
        map.get(id).map(|p| p.0.buffer.clone()).unwrap_or_default()
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let exited_flag = {
        let map = ptys().lock().unwrap();
        map.get(id).map(|p| p.0.exited.clone()).unwrap_or_default()
    };
    loop {
        let done = {
            let buf = buffer_arc.lock().unwrap();
            String::from_utf8_lossy(&buf).contains(CWD_MARKER)
        };
        if done || exited_flag.load(Ordering::SeqCst) || std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let raw = drain(id);
    // 解析新 cwd（最后一行标记）
    let new_cwd = raw.lines().rev().find_map(|l| {
        l.split_once(CWD_MARKER)
            .map(|(_m, p)| super::terminal::normalize_cwd(p.trim()))
    });
    if let Some(c) = new_cwd {
        super::terminal::update_cwd(id, &c);
    }
    // 展示输出：剥离标记行
    Ok(raw
        .lines()
        .filter(|l| !l.contains(CWD_MARKER))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// 运行状态（terminal 模型工具用）
pub(crate) fn is_running(id: &str) -> bool {
    ptys()
        .lock()
        .unwrap()
        .get(id)
        .map(|p| !p.0.exited.load(Ordering::SeqCst))
        .unwrap_or(false)
}

/// 原始字节写入输入管道（终端信号：\x03 = Ctrl+C 等）
pub(crate) fn send_raw(id: &str, bytes: &str) -> Result<(), String> {
    let mut map = ptys().lock().unwrap();
    let Some(p) = map.get_mut(id) else {
        return Err("该终端未启动 PTY".to_string());
    };
    if p.0.exited.load(Ordering::SeqCst) {
        map.remove(id);
        return Err("PTY 进程已退出，请重新启动".to_string());
    }
    let mut written: u32 = 0;
    unsafe {
        WriteFile(
            p.0.input_write,
            Some(bytes.as_bytes()),
            Some(&mut written),
            None,
        )
        .map_err(|e| format!("写入 PTY 失败: {e}"))?;
    }
    Ok(())
}

/// 调整伪终端尺寸
pub(crate) fn resize(id: &str, rows: u16, cols: u16) -> Result<(), String> {
    let map = ptys().lock().unwrap();
    let Some(p) = map.get(id) else {
        return Err("该终端未启动 PTY".to_string());
    };
    let size = COORD {
        X: cols.clamp(20, 500) as i16,
        Y: rows.clamp(2, 300) as i16,
    };
    unsafe { ResizePseudoConsole(p.0.hpc, size) }.map_err(|e| format!("调整 PTY 尺寸失败: {e}"))
}

/// 剥离 ANSI 转义序列（CSI/OSC；输出在 <pre> 中展示）
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                // CSI：吞到终结字节（@..~）
                for c2 in chars.by_ref() {
                    if ('@'..='~').contains(&c2) {
                        break;
                    }
                }
            }
            Some(']') => {
                // OSC（如窗口标题）：吞到 BEL 或 ST
                for c2 in chars.by_ref() {
                    if c2 == '\x07' {
                        break;
                    }
                }
            }
            Some(_) => {}
            None => break,
        }
    }
    out
}

// ─── IPC ───

#[derive(Serialize, Clone, Debug)]
pub struct PtyStatus {
    pub running: bool,
    pub rows: u16,
    pub cols: u16,
}

#[tauri::command]
pub async fn harness_terminal_start_pty(
    id: String,
    rows: Option<u16>,
    cols: Option<u16>,
) -> Result<(), String> {
    let cwd = super::terminal::session_cwd(&id)?;
    start(&id, &cwd, rows, cols)
}

#[tauri::command]
pub async fn harness_terminal_stop_pty(id: String) -> Result<(), String> {
    stop(&id);
    Ok(())
}

#[tauri::command]
pub async fn harness_terminal_send_pty(id: String, input: String) -> Result<String, String> {
    let input = input.trim().to_string();
    if input.is_empty() {
        return Err("输入不能为空".to_string());
    }
    let out = send(&id, &input)?;
    // 日志复用 terminal.rs 的 logs_store（与普通命令同一视图）
    super::terminal::push_log(&id, input, out.clone());
    Ok(out)
}

#[tauri::command]
pub async fn harness_terminal_resize_pty(id: String, rows: u16, cols: u16) -> Result<(), String> {
    resize(&id, rows, cols)
}

#[tauri::command]
pub async fn harness_terminal_pty_status(id: String) -> Result<PtyStatus, String> {
    let map = ptys().lock().unwrap();
    match map.get(&id) {
        Some(p) => Ok(PtyStatus {
            running: !p.0.exited.load(Ordering::SeqCst),
            rows: p.0.rows as u16,
            cols: p.0.cols as u16,
        }),
        None => Ok(PtyStatus {
            running: false,
            rows: DEFAULT_ROWS as u16,
            cols: DEFAULT_COLS as u16,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_csi_and_osc() {
        let s = "\x1b[32mgreen\x1b[0m \x1b]0;title\x07 plain \x1b[1;31mred\x1b[m";
        assert_eq!(strip_ansi(s), "green  plain red");
    }

    #[test]
    fn strip_ansi_keeps_plain_text() {
        assert_eq!(strip_ansi("hello\nworld"), "hello\nworld");
    }

    #[test]
    fn strip_ansi_handles_unterminated_and_variants() {
        // 未闭合转义（CSI 无终结字节 / OSC 无 BEL）：吞到末尾不 panic
        assert_eq!(strip_ansi("\x1b[31mred"), "red");
        assert_eq!(strip_ansi("\x1b]0;untitled"), "");
        assert_eq!(strip_ansi("\x1b"), "", "孤立 ESC 应移除");
        // 尾随 ESC + 非括号字符：单字符吞掉
        assert_eq!(strip_ansi("\x1bX"), "");
        // 多参数 CSI 变体
        assert_eq!(strip_ansi("\x1b[2;3H"), "");
        assert_eq!(strip_ansi("\x1b[?25l"), "", "私用 CSI 也移除");
        // 混合：ANSI + 普通文本顺序无关
        assert_eq!(strip_ansi("a\x1b[1mb\x1b[mc"), "abc");
    }
}
