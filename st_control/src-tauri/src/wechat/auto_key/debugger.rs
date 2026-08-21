// ============================================================
// 微信密钥获取 — Rust 调试器（DEBUG_PROCESS 提取 master key）
// 自 auto_key.rs 拆分（嵌套 mod 去缩进迁移）。
// ============================================================

use super::*;
use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use std::sync::mpsc::Sender;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::{
    ContinueDebugEvent, FlushInstructionCache, ReadProcessMemory, WaitForDebugEvent,
    WriteProcessMemory, CONTEXT, CONTEXT_CONTROL_AMD64, CONTEXT_INTEGER_AMD64,
    CREATE_PROCESS_DEBUG_EVENT, DEBUG_EVENT, EXCEPTION_DEBUG_EVENT, EXIT_PROCESS_DEBUG_EVENT,
    LOAD_DLL_DEBUG_EVENT,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE,
    TH32CS_SNAPMODULE32,
};
use windows::Win32::System::Threading::{CreateProcessW, OpenThread, DEBUG_PROCESS};

// GetThreadContext / SetThreadContext 位于 Debug 模块
use windows::Win32::System::Diagnostics::Debug::{GetThreadContext, SetThreadContext};

const DEBUG_WAIT_MS: u32 = 500;
const TF_FLAG: u32 = 0x100;

struct ProcCtx {
    h_process: HANDLE,
    module_base: u64,
    installed: bool,
    bps: HashMap<u64, u8>, // addr -> 原字节
}

/// 调试器主体。`status_tx` 用于向调用方实时转发进度（断点命中、等待登录等）。
pub struct WeChatDebugger {
    exe_path: std::path::PathBuf,
    function_rvas: Vec<u64>,
    page1: Vec<u8>,
    status_tx: Option<Sender<String>>,
    procs: HashMap<u32, ProcCtx>,
    stepping_threads: HashMap<u32, u64>,
    main_pid: u32,
    hit_count: u32,
}

impl WeChatDebugger {
    pub fn new(
        exe_path: std::path::PathBuf,
        function_rvas: Vec<u64>,
        page1: Vec<u8>,
        status_tx: Option<Sender<String>>,
    ) -> Self {
        WeChatDebugger {
            exe_path,
            function_rvas,
            page1,
            status_tx,
            procs: HashMap::new(),
            stepping_threads: HashMap::new(),
            main_pid: 0,
            hit_count: 0,
        }
    }

    fn log(&self, msg: &str) {
        if let Some(tx) = &self.status_tx {
            let _ = tx.send(msg.to_string());
        }
    }

    fn read(&self, h: HANDLE, addr: u64, n: usize) -> Option<Vec<u8>> {
        if !(0x1_0000..=0x7FFF_FFFF_FFFF).contains(&addr) {
            return None;
        }
        let mut buf = vec![0u8; n];
        let mut read = 0usize;
        unsafe {
            if ReadProcessMemory(
                h,
                addr as *const _,
                buf.as_mut_ptr() as *mut _,
                n,
                Some(&mut read),
            )
            .is_ok()
                && read == n
            {
                Some(buf)
            } else {
                None
            }
        }
    }

    fn read_ptr(&self, h: HANDLE, addr: u64) -> u64 {
        self.read(h, addr, 8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
            .unwrap_or(0)
    }

    fn module_base(&self, pid: u32, name: &str) -> u64 {
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid)
            else {
                return 0;
            };
            let mut me = MODULEENTRY32W {
                dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
                ..Default::default()
            };
            let mut found = 0u64;
            if Module32FirstW(snap, &mut me).is_ok() {
                loop {
                    let len = me
                        .szModule
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(me.szModule.len());
                    let mod_name = String::from_utf16_lossy(&me.szModule[..len]);
                    if mod_name.eq_ignore_ascii_case(name) {
                        found = me.modBaseAddr as u64;
                        break;
                    }
                    if Module32NextW(snap, &mut me).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
            found
        }
    }

    fn set_breakpoints(&mut self, pid: u32) {
        let handle = match self.procs.get(&pid) {
            Some(pc) if !pc.installed && !pc.h_process.is_invalid() => pc.h_process,
            _ => return,
        };
        let b = self.module_base(pid, "Weixin.dll");
        if b == 0 {
            return;
        }
        let rvas = self.function_rvas.clone();
        let mut installed = Vec::new();
        for &rva in &rvas {
            let addr = b + rva;
            let Some(orig) = self.read(handle, addr, 1) else {
                self.log(&format!("  读断点原字节失败 @0x{addr:X}"));
                continue;
            };
            unsafe {
                let mut w = 0usize;
                if WriteProcessMemory(
                    handle,
                    addr as *const _,
                    [0xCCu8].as_ptr() as *const _,
                    1,
                    Some(&mut w),
                )
                .is_err()
                    || w != 1
                {
                    self.log(&format!("  写 INT3 失败 @0x{addr:X}"));
                    continue;
                }
                let _ = FlushInstructionCache(handle, Some(addr as *const _), 1);
            }
            installed.push((addr, orig[0]));
            self.log(&format!("  断点已设 (PID={pid}) 于 0x{addr:X}"));
        }
        if let Some(pc) = self.procs.get_mut(&pid) {
            pc.module_base = b;
            pc.installed = true;
            for (addr, orig) in installed {
                pc.bps.insert(addr, orig);
            }
        }
    }

    fn restore_original(&self, pc: &ProcCtx, addr: u64) {
        if let Some(&orig) = pc.bps.get(&addr) {
            unsafe {
                let mut w = 0usize;
                let _ = WriteProcessMemory(
                    pc.h_process,
                    addr as *const _,
                    [orig].as_ptr() as *const _,
                    1,
                    Some(&mut w),
                );
                let _ = FlushInstructionCache(pc.h_process, Some(addr as *const _), 1);
            }
        }
    }

    fn reinstall_breakpoint(&self, pc: &ProcCtx, addr: u64) {
        if !pc.bps.contains_key(&addr) {
            return;
        }
        unsafe {
            let mut w = 0usize;
            let _ = WriteProcessMemory(
                pc.h_process,
                addr as *const _,
                [0xCCu8].as_ptr() as *const _,
                1,
                Some(&mut w),
            );
            let _ = FlushInstructionCache(pc.h_process, Some(addr as *const _), 1);
        }
    }

    fn open_thread(thread_id: u32) -> Option<HANDLE> {
        unsafe {
            OpenThread(
                windows::Win32::System::Threading::THREAD_ACCESS_RIGHTS(u32::MAX),
                false,
                thread_id,
            )
            .ok()
        }
    }

    /// 断点命中：收集寄存器/栈候选，HMAC 预言机验证，返回 master key hex（64）。
    fn handle_breakpoint(&mut self, pid: u32, thread_id: u32) -> Option<String> {
        let pc = self.procs.get(&pid)?;
        self.hit_count += 1;
        unsafe {
            let h_thread = Self::open_thread(thread_id)?;
            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_INTEGER_AMD64 | CONTEXT_CONTROL_AMD64;
            if GetThreadContext(h_thread, &mut ctx).is_err() {
                let _ = CloseHandle(h_thread);
                return None;
            }
            ctx.Rip -= 1;
            ctx.ContextFlags = CONTEXT_CONTROL_AMD64;
            let _ = SetThreadContext(h_thread, &ctx);
            let _ = CloseHandle(h_thread);

            let hp = pc.h_process;
            let mut tried = std::collections::HashSet::new();
            let try_addr =
                |addr: u64, tried: &mut std::collections::HashSet<u64>| -> Option<String> {
                    if !(0x1_0000..=0x7FFF_FFFF_FFFF).contains(&addr) || !tried.insert(addr) {
                        return None;
                    }
                    let b = self.read(hp, addr, 32)?;
                    if is_valid_master_key(&b, &self.page1) {
                        Some(hex::encode(&b))
                    } else {
                        None
                    }
                };

            // 1) 首选启发：key 指针在 [rdx+0x08]，长度在 [rdx+0x10]==32
            if self.read_ptr(hp, ctx.Rdx + 0x10) == 32 {
                if let Some(k) = try_addr(self.read_ptr(hp, ctx.Rdx + 0x08), &mut tried) {
                    self.log(&format!(
                        "master key 命中 (PID={pid} rdx+0x08, 第{}次)",
                        self.hit_count
                    ));
                    return Some(k);
                }
            }
            // 2) 每个寄存器：直接指针 + [reg+off] 间接
            let regs = [
                ctx.Rdx, ctx.Rcx, ctx.R8, ctx.R9, ctx.Rax, ctx.Rbx, ctx.Rsi, ctx.Rdi, ctx.Rbp,
                ctx.R10, ctx.R11, ctx.R12, ctx.R13, ctx.R14, ctx.R15,
            ];
            let offs = [0x0u64, 0x8, 0x10, 0x18, 0x20, 0x28, 0x30];
            for r in regs {
                if let Some(k) = try_addr(r, &mut tried) {
                    self.log(&format!(
                        "master key 命中 (PID={pid} 寄存器直指, 第{}次)",
                        self.hit_count
                    ));
                    return Some(k);
                }
                for o in offs {
                    let p = self.read_ptr(hp, r + o);
                    if let Some(k) = try_addr(p, &mut tried) {
                        self.log(&format!(
                            "master key 命中 (PID={pid} 寄存器间接, 第{}次)",
                            self.hit_count
                        ));
                        return Some(k);
                    }
                }
            }
            // 3) 栈：rsp 起 0x200 内的指针
            let mut off = 0u64;
            while off <= 0x200 {
                let p = self.read_ptr(hp, ctx.Rsp + off);
                if let Some(k) = try_addr(p, &mut tried) {
                    self.log(&format!(
                        "master key 命中 (PID={pid} 栈指针, 第{}次)",
                        self.hit_count
                    ));
                    return Some(k);
                }
                off += 8;
            }
            self.log(&format!(
                "第{}次断点未命中 key (PID={pid})，继续等待…",
                self.hit_count
            ));
            None
        }
    }

    fn clear_trap(&mut self, thread_id: u32) {
        unsafe {
            let Some(h) = Self::open_thread(thread_id) else {
                return;
            };
            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_CONTROL_AMD64;
            if GetThreadContext(h, &mut ctx).is_ok() {
                ctx.EFlags &= !TF_FLAG;
                let _ = SetThreadContext(h, &ctx);
            }
            let _ = CloseHandle(h);
        }
    }

    fn set_trap_flag(thread_id: u32) {
        unsafe {
            let Some(h) = Self::open_thread(thread_id) else {
                return;
            };
            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_CONTROL_AMD64;
            if GetThreadContext(h, &mut ctx).is_ok() {
                ctx.EFlags |= TF_FLAG;
                let _ = SetThreadContext(h, &ctx);
            }
            let _ = CloseHandle(h);
        }
    }

    /// 主调试循环，返回 master key hex（若有）。
    pub fn run(&mut self, deadline: std::time::Instant) -> Option<String> {
        unsafe {
            let mut si: windows::Win32::System::Threading::STARTUPINFOW = std::mem::zeroed();
            si.cb = std::mem::size_of::<windows::Win32::System::Threading::STARTUPINFOW>() as u32;
            let mut pi: windows::Win32::System::Threading::PROCESS_INFORMATION = std::mem::zeroed();
            let wide: Vec<u16> = self
                .exe_path
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            let ok = CreateProcessW(
                PCWSTR(wide.as_ptr()),
                None,
                None,
                None,
                false,
                DEBUG_PROCESS,
                None,
                PCWSTR(std::ptr::null()),
                &si,
                &mut pi,
            );
            if let Err(e) = ok {
                self.log(&format!("CreateProcessW(DEBUG) 失败: {}", e));
                return None;
            }
            self.main_pid = pi.dwProcessId;
            self.procs.insert(
                pi.dwProcessId,
                ProcCtx {
                    h_process: pi.hProcess,
                    module_base: 0,
                    installed: false,
                    bps: HashMap::new(),
                },
            );
            let _ = CloseHandle(pi.hThread);
            self.log("微信已以调试方式启动，请在微信窗口中扫码/登录（断点会在数据库打开时命中）…");

            let mut found: Option<String> = None;
            while std::time::Instant::now() < deadline && found.is_none() {
                let mut ev: DEBUG_EVENT = std::mem::zeroed();
                if WaitForDebugEvent(&mut ev, DEBUG_WAIT_MS).is_err() {
                    continue;
                }
                let pid = ev.dwProcessId;
                let mut cont = windows::Win32::Foundation::DBG_CONTINUE;
                match ev.dwDebugEventCode {
                    CREATE_PROCESS_DEBUG_EVENT => {
                        let info = ev.u.CreateProcessInfo;
                        let h_proc = info.hProcess;
                        let h_thread = info.hThread;
                        let h_file = info.hFile;
                        match self.procs.get_mut(&pid) {
                            Some(pc) => {
                                if pc.h_process.is_invalid() {
                                    pc.h_process = h_proc;
                                }
                            }
                            None => {
                                self.procs.insert(
                                    pid,
                                    ProcCtx {
                                        h_process: h_proc,
                                        module_base: 0,
                                        installed: false,
                                        bps: HashMap::new(),
                                    },
                                );
                            }
                        }
                        self.set_breakpoints(pid);
                        if !h_thread.is_invalid() {
                            let _ = CloseHandle(h_thread);
                        }
                        if !h_file.is_invalid() {
                            let _ = CloseHandle(h_file);
                        }
                    }
                    LOAD_DLL_DEBUG_EVENT => {
                        let h_file = ev.u.LoadDll.hFile;
                        if !h_file.is_invalid() {
                            let _ = CloseHandle(h_file);
                        }
                        self.set_breakpoints(pid);
                    }
                    EXCEPTION_DEBUG_EVENT => {
                        let code = ev.u.Exception.ExceptionRecord.ExceptionCode;
                        let addr = ev.u.Exception.ExceptionRecord.ExceptionAddress as u64;
                        if code == windows::Win32::Foundation::EXCEPTION_BREAKPOINT {
                            let hit = self
                                .procs
                                .get(&pid)
                                .map(|pc| pc.bps.contains_key(&addr))
                                .unwrap_or(false);
                            if hit {
                                let pc = self.procs.get(&pid).unwrap();
                                self.restore_original(pc, addr);
                                if let Some(k) = self.handle_breakpoint(pid, ev.dwThreadId) {
                                    found = Some(k);
                                } else {
                                    // 单步重装：恢复原字节后让线程执行一步
                                    self.stepping_threads.insert(ev.dwThreadId, addr);
                                    Self::set_trap_flag(ev.dwThreadId);
                                }
                            } else {
                                cont = windows::Win32::Foundation::DBG_EXCEPTION_NOT_HANDLED;
                            }
                        } else if code == windows::Win32::Foundation::EXCEPTION_SINGLE_STEP {
                            if let Some(&rearm) = self.stepping_threads.get(&ev.dwThreadId) {
                                self.clear_trap(ev.dwThreadId);
                                if let Some(pc) = self.procs.get(&pid) {
                                    self.reinstall_breakpoint(pc, rearm);
                                }
                                self.stepping_threads.remove(&ev.dwThreadId);
                            }
                        } else {
                            cont = windows::Win32::Foundation::DBG_EXCEPTION_NOT_HANDLED;
                        }
                    }
                    EXIT_PROCESS_DEBUG_EVENT => {
                        if let Some(pc) = self.procs.remove(&pid) {
                            let _ = CloseHandle(pc.h_process);
                        }
                        if pid == self.main_pid {
                            break;
                        }
                    }
                    _ => {}
                }
                let _ = ContinueDebugEvent(pid, ev.dwThreadId, cont);
            }

            // 清理：恢复所有断点、关闭句柄
            for (_, pc) in self.procs.drain() {
                for &addr in pc.bps.keys() {
                    let orig = pc.bps.get(&addr).copied().unwrap_or(0);
                    let mut w = 0usize;
                    let _ = WriteProcessMemory(
                        pc.h_process,
                        addr as *const _,
                        [orig].as_ptr() as *const _,
                        1,
                        Some(&mut w),
                    );
                    let _ = FlushInstructionCache(pc.h_process, Some(addr as *const _), 1);
                }
                let _ = CloseHandle(pc.h_process);
            }
            found
        }
    }
}
