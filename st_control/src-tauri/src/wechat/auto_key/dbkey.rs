// ============================================================
// 微信密钥获取 — 数据库密钥自动获取
// 自 auto_key.rs 拆分：hook 主流程 / 调试器回退 / 进程管理 /
// 密钥轮询与落盘。
// ============================================================

use std::ffi::{c_int, CStr};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{
    debugger, emit_progress, find_keyset_function_rvas, find_wechat_pids, get_dll,
    locate_weixin_dll, locate_weixin_exe, read_db_page1_shared, WxKeyDll, DB_KEY_BUF,
    DB_KEY_POLL_MS, STATUS_MSG_BUF,
};

// ============ 数据库密钥自动获取 ============

pub fn auto_get_db_key(
    app: &tauri::AppHandle,
    op: &str,
    timeout_ms: u64,
) -> Result<serde_json::Value, String> {
    #[cfg(target_os = "windows")]
    {
        // wx_key.dll（v2.x 含 py_wx_key 系）对主进程注入在 4.1.12.26 实测可用；
        // 注入成功但轮询超时（密钥只在 DB 打开时回调）时回退调试器方案。
        match auto_get_db_key_hook_main(app, op, timeout_ms) {
            Ok(v) => Ok(v),
            Err(hook_err) => {
                log::warn!("hook 方案失败（{}），回退调试器方案…", hook_err);
                auto_get_db_key_debugger(app, op, timeout_ms)
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, op, timeout_ms);
        Err("仅支持 Windows 微信 4.x".to_string())
    }
}

/// 4.1.10.31+ 调试器方案：DEBUG_PROCESS 启动微信、断点提取 master key。
/// 与老式 hook 方案的区别：需要临时重启微信（微信单实例），提取后自动恢复。
pub fn auto_get_db_key_v2(
    app: &tauri::AppHandle,
    op: &str,
    timeout_ms: u64,
) -> Result<serde_json::Value, String> {
    #[cfg(target_os = "windows")]
    {
        auto_get_db_key_debugger(app, op, timeout_ms)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, op, timeout_ms);
        Err("仅支持 Windows 微信 4.1.10.31+".to_string())
    }
}

#[cfg(target_os = "windows")]
fn auto_get_db_key_debugger(
    app: &tauri::AppHandle,
    op: &str,
    timeout_ms: u64,
) -> Result<serde_json::Value, String> {
    use std::sync::mpsc;

    // 给密钥提取后的 PBKDF2 校验预留时间（最大 60s，至少保留 30s）
    let reserve = Duration::from_secs(60).min(Duration::from_millis(timeout_ms.max(60_000) / 2));
    let deadline =
        Instant::now() + Duration::from_millis(timeout_ms.max(60_000)).saturating_sub(reserve);
    emit_progress(app, op, 0, 0, "正在定位微信安装目录与数据库…");

    let exe = locate_weixin_exe().ok_or_else(|| {
        "未找到 Weixin.exe（请确认微信已安装或设置注册表 InstallPath）".to_string()
    })?;
    let dll_path =
        locate_weixin_dll().ok_or_else(|| "未找到 Weixin.dll（微信版本目录缺失）".to_string())?;
    let dll_bytes = std::fs::read(&dll_path).map_err(|e| format!("读取 Weixin.dll 失败: {}", e))?;

    emit_progress(app, op, 0, 0, "正在静态定位 WCDB key-set 函数…");
    let func_rvas = find_keyset_function_rvas(&dll_bytes)?;
    emit_progress(
        app,
        op,
        0,
        0,
        &format!("定位到 {} 个 key-set 候选函数", func_rvas.len()),
    );

    // HMAC 预言机：取任一 message_0.db 的 page-1（与账号无关，master key 全局唯一）
    let oracle_db = find_message_0_db();
    let page1 = match oracle_db {
        Some(db) => {
            emit_progress(app, op, 0, 0, &format!("HMAC 校验库: {}", db.display()));
            read_db_page1_shared(&db)?
        }
        None => {
            return Err("未找到 message_0.db（微信数据目录缺失），无法做 HMAC 密钥校验".to_string())
        }
    };

    // 微信单实例：先关闭已运行实例
    let running = find_wechat_pids();
    if !running.is_empty() {
        emit_progress(
            app,
            op,
            0,
            0,
            &format!(
                "检测到微信正在运行（{} 个进程），将临时关闭并以调试方式重启（需要重新扫码登录一次）…",
                running.len()
            ),
        );
        kill_wechat_processes(&running);
        std::thread::sleep(Duration::from_millis(800));
    }

    let (tx, rx) = mpsc::channel::<String>();
    let mut debugger = debugger::WeChatDebugger::new(exe.clone(), func_rvas, page1, Some(tx));

    // 状态转发：单线程循环转发，避免与 finish_db_key 的进度事件冲突
    let status_handle = {
        let app = app.clone();
        let op = op.to_string();
        std::thread::spawn(move || {
            while let Ok(msg) = rx.recv() {
                emit_progress(&app, &op, 0, 0, &msg);
            }
        })
    };

    emit_progress(
        app,
        op,
        0,
        0,
        "正在以调试方式启动微信（请在弹出的微信窗口扫码登录）…",
    );
    let key = debugger.run(deadline).ok_or_else(|| {
        "调试器未提取到 master key（超时或未完成登录）。请重试并在微信窗口完成扫码登录".to_string()
    })?;
    let _ = status_handle.join();

    emit_progress(app, op, 1, 1, "master key 已提取并通过 HMAC 校验");

    // 清理调试实例，恢复正常启动微信（让用户回到已登录状态）
    let debug_pids = find_wechat_pids();
    if !debug_pids.is_empty() {
        kill_wechat_processes(&debug_pids);
        std::thread::sleep(Duration::from_millis(500));
        let _ = relaunch_wechat(&exe);
    }

    finish_db_key(app, op, &key)
}

#[cfg(target_os = "windows")]
fn find_message_0_db() -> Option<std::path::PathBuf> {
    // 1) config 指定账号
    if let Ok(cfg) = crate::wechat::config::WeChatConfig::load() {
        let cand = cfg.db_dir.join("message").join("message_0.db");
        if cand.is_file() {
            return Some(cand);
        }
    }
    // 2) 数据根目录扫描（微信 ini 指向的 xwechat_files / 用户文档，跨机器可移植）
    for root in crate::wechat::config::candidate_xwechat_roots() {
        if let Ok(entries) = std::fs::read_dir(&root) {
            for e in entries.flatten() {
                let cand = e
                    .path()
                    .join("db_storage")
                    .join("message")
                    .join("message_0.db");
                if cand.is_file() {
                    return Some(cand);
                }
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn kill_wechat_processes(pids: &[u32]) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    for &pid in pids {
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                if !h.is_invalid() {
                    let _ = TerminateProcess(h, 0);
                    let _ = CloseHandle(h);
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn relaunch_wechat(exe: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::CreateProcessW;

    let wide: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut si: windows::Win32::System::Threading::STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<windows::Win32::System::Threading::STARTUPINFOW>() as u32;
    let mut pi: windows::Win32::System::Threading::PROCESS_INFORMATION =
        unsafe { std::mem::zeroed() };
    unsafe {
        let r = CreateProcessW(
            PCWSTR(wide.as_ptr()),
            None,
            None,
            None,
            false,
            windows::Win32::System::Threading::CREATE_NEW_CONSOLE,
            None,
            None,
            &si,
            &mut pi,
        );
        if let Err(e) = r {
            return Err(format!("重新启动微信失败: {}", e));
        }
        let _ = CloseHandle(pi.hThread);
        let _ = CloseHandle(pi.hProcess);
    }
    Ok(())
}

/// 主进程注入方案（与 wx_key.exe 一致）：
/// 微信有多个进程，只有加载 Weixin.dll 的主进程能注入成功；
/// 注入后密钥在数据库打开（登录/同步）时回调，轮询等待。
#[cfg(target_os = "windows")]
fn auto_get_db_key_hook_main(
    app: &tauri::AppHandle,
    op: &str,
    timeout_ms: u64,
) -> Result<serde_json::Value, String> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(30_000));
    emit_progress(app, op, 0, 0, "正在查找微信主进程…");

    let mut main_pid = find_wechat_main_process();
    if main_pid.is_none() {
        return Err("未找到微信进程，请先启动微信（Weixin.exe）".to_string());
    }
    // 登录界面阶段 Weixin.dll 可能尚未加载：轮询等待，最多 60s
    emit_progress(app, op, 0, 0, "等待微信核心模块加载…");
    let dll_deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < dll_deadline {
        let Some(pid) = main_pid else {
            return Err("微信进程已退出，请重新启动微信".to_string());
        };
        if process_has_module(pid, "Weixin.dll") {
            break;
        }
        // 主进程可能退出/重启，重新枚举
        std::thread::sleep(Duration::from_millis(500));
        main_pid = find_wechat_main_process();
    }
    let main_pid = main_pid.ok_or_else(|| "微信进程已退出，请重新启动微信".to_string())?;
    if !process_has_module(main_pid, "Weixin.dll") {
        return Err(format!(
            "等待超时：微信主进程 PID {} 未加载 Weixin.dll，请确认微信已启动到登录/主界面",
            main_pid
        ));
    }
    emit_progress(
        app,
        op,
        0,
        0,
        &format!("定位到微信主进程 PID={}，注入密钥钩子…", main_pid),
    );

    let mut dll = get_dll(Some(app))?;
    let d = dll
        .as_mut()
        .ok_or_else(|| "wx_key.dll 未加载".to_string())?;

    let hooked = unsafe { (d.init_hook)(main_pid) };
    if !hooked {
        let err = d.last_error_string();
        let _ = unsafe { (d.cleanup_hook)() };
        return Err(format!("主进程 PID {} 注入失败：{}", main_pid, err));
    }

    emit_progress(
        app,
        op,
        0,
        0,
        "钩子已注入，正在等待密钥回调（数据库打开时触发；若刚重启过微信，请稍候或重新登录一次）…",
    );
    let remaining = deadline.saturating_duration_since(Instant::now());
    let poll = poll_db_key(d, main_pid, remaining, app, op, 0, 0);
    let _ = unsafe { (d.cleanup_hook)() };
    match poll {
        Ok(key) => {
            emit_progress(app, op, 1, 1, "数据库密钥获取成功");
            finish_db_key(app, op, &key)
        }
        Err(e) => Err(format!("主进程 PID {} 轮询超时：{}", main_pid, e)),
    }
}

/// 定位可注入的微信主进程：优先加载 Weixin.dll 的进程；否则取启动最早的。
#[cfg(target_os = "windows")]
fn find_wechat_main_process() -> Option<u32> {
    let pids = find_wechat_pids();
    if pids.is_empty() {
        return None;
    }
    find_main_wechat_pid(&pids).or_else(|| {
        // 微信多开时多个主进程都加载 Weixin.dll；取 PID 最小的
        pids.into_iter().min()
    })
}

/// 进程是否加载了指定模块（Toolhelp 模块快照）
#[cfg(target_os = "windows")]
fn process_has_module(pid: u32, module_name: &str) -> bool {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE,
        TH32CS_SNAPMODULE32,
    };
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid)
        else {
            return false;
        };
        let mut me = MODULEENTRY32W {
            dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
        if Module32FirstW(snap, &mut me).is_ok() {
            loop {
                let len = me
                    .szModule
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(me.szModule.len());
                let name = String::from_utf16_lossy(&me.szModule[..len]);
                if name.eq_ignore_ascii_case(module_name) {
                    found = true;
                    break;
                }
                if Module32NextW(snap, &mut me).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snap);
        found
    }
}

/// 微信多进程中选择加载了 Weixin.dll 的主进程（唯一可注入目标）。
#[cfg(target_os = "windows")]
pub(crate) fn find_main_wechat_pid(pids: &[u32]) -> Option<u32> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE,
        TH32CS_SNAPMODULE32,
    };
    for &pid in pids {
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid)
            else {
                continue;
            };
            let mut me = MODULEENTRY32W {
                dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
                ..Default::default()
            };
            let mut found = false;
            if Module32FirstW(snap, &mut me).is_ok() {
                loop {
                    let len = me
                        .szModule
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(me.szModule.len());
                    let name = String::from_utf16_lossy(&me.szModule[..len]);
                    if name.eq_ignore_ascii_case("Weixin.dll") {
                        found = true;
                        break;
                    }
                    if Module32NextW(snap, &mut me).is_err() {
                        break;
                    }
                }
            }
            let _ = windows::Win32::Foundation::CloseHandle(snap);
            if found {
                return Some(pid);
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn poll_db_key(
    dll: &WxKeyDll,
    pid: u32,
    timeout: Duration,
    app: &tauri::AppHandle,
    op: &str,
    done: u64,
    total: u64,
) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    let mut key = [0i8; DB_KEY_BUF];
    let mut status = [0i8; STATUS_MSG_BUF];
    let mut last_status = String::new();

    while Instant::now() < deadline {
        let polled = unsafe { (dll.poll_key_data)(key.as_mut_ptr(), key.len() as c_int) };
        if polled {
            let s = unsafe { CStr::from_ptr(key.as_ptr()) }
                .to_string_lossy()
                .to_string();
            if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Ok(s);
            }
        }
        // 排空状态消息（最多 5 条/轮），实时转发给前端
        for _ in 0..5 {
            let mut level: c_int = 0;
            let got = unsafe {
                (dll.get_status_message)(status.as_mut_ptr(), status.len() as c_int, &mut level)
            };
            if !got {
                break;
            }
            let msg = unsafe { CStr::from_ptr(status.as_ptr()) }
                .to_string_lossy()
                .to_string();
            if !msg.is_empty() {
                last_status = msg.clone();
                emit_progress(app, op, done, total, &msg);
            }
            let _ = level;
        }
        std::thread::sleep(Duration::from_millis(DB_KEY_POLL_MS));
    }
    let _ = pid;
    Err(if last_status.is_empty() {
        "轮询超时".to_string()
    } else {
        last_status
    })
}

/// 密钥到手后：校验全部数据库并生成 all_keys.json，同时把口令写回 config.json
fn finish_db_key(app: &tauri::AppHandle, op: &str, key: &str) -> Result<serde_json::Value, String> {
    let accounts = crate::wechat::config::detect_accounts();
    let cfg = crate::wechat::config::WeChatConfig::load().ok();

    let (db_dir, account_wxid) = match cfg
        .as_ref()
        .map(|c| c.db_dir.clone())
        .filter(|d| d.is_dir())
    {
        Some(d) => {
            let wxid = accounts
                .iter()
                .find(|a| Path::new(&a.db_dir) == d.as_path())
                .map(|a| a.wxid.clone())
                .unwrap_or_default();
            (d, wxid)
        }
        None => match accounts.first() {
            Some(a) => (PathBuf::from(&a.db_dir), a.wxid.clone()),
            None => {
                return Err("未检测到微信账号数据目录，请先在配置中选择数据库目录".to_string());
            }
        },
    };

    let keys_file = cfg
        .map(|c| c.keys_file.clone())
        .unwrap_or_else(|| crate::wechat::config::default_st_result_dir().join("all_keys.json"));
    let db_dir_str = db_dir.to_string_lossy().to_string();
    let keys_file_str = keys_file.to_string_lossy().to_string();

    emit_progress(
        app,
        op,
        0,
        0,
        "正在校验数据库并生成密钥文件（PBKDF2 派生，约数十秒）…",
    );
    let gen = crate::wechat::handlers::config::generate_keys_file_impl(
        app.clone(),
        db_dir_str.clone(),
        keys_file_str.clone(),
        key.to_string(),
        Some("wx_key_v4.1".to_string()),
    )?;

    let _ = crate::wechat::config::patch_config(crate::wechat::config::KeyConfigPatch {
        db_dir: Some(&db_dir_str),
        db_enc_key: Some(key),
        image_aes_key: None,
        image_xor_key: None,
    });

    Ok(serde_json::json!({
        "success": true,
        "key": key,
        "account": account_wxid,
        "db_dir": db_dir_str,
        "keys_file": keys_file_str,
        "total": gen.get("total").cloned().unwrap_or(serde_json::Value::Null),
        "valid": gen.get("valid").cloned().unwrap_or(serde_json::Value::Null),
        "errors": gen.get("errors").cloned().unwrap_or(serde_json::Value::Null),
    }))
}
