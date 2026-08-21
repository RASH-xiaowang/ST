// ============================================================
// 微信密钥获取 — 进度事件 + wx_key.dll FFI
// 自 auto_key.rs 拆分：操作进度回传、动态库加载与微信进程定位。
// ============================================================

use std::ffi::{c_char, c_int, CStr};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

// ============ 进度事件（与前端 wechat-op-progress 约定一致） ============

pub(crate) fn emit_progress(
    app: &tauri::AppHandle,
    op: &str,
    done: u64,
    total: u64,
    message: &str,
) {
    use tauri::Emitter;
    let percent = if total == 0 {
        0u32
    } else {
        (done as f64 * 100.0 / total as f64).round() as u32
    };
    let _ = app.emit(
        "wechat-op-progress",
        serde_json::json!({
            "op": op,
            "done": done,
            "total": total,
            "percent": percent.min(100),
            "message": message,
        }),
    );
}

// ============ wx_key.dll FFI ============

/// wx_key.dll 的 6 个导出函数（与 WeFlow/koffi 绑定签名一致）
pub struct WxKeyDll {
    pub(crate) _lib: libloading::Library,
    pub(crate) init_hook: unsafe extern "C" fn(u32) -> bool,
    pub(crate) poll_key_data: unsafe extern "C" fn(*mut c_char, c_int) -> bool,
    pub(crate) get_status_message: unsafe extern "C" fn(*mut c_char, c_int, *mut c_int) -> bool,
    pub(crate) cleanup_hook: unsafe extern "C" fn() -> bool,
    pub(crate) get_last_error_msg: unsafe extern "C" fn() -> *const c_char,
    /// 部分构建（如 py_wx_key 系 v2.x）不导出 GetImageKey，图片密钥走 kvcomm 直接读取。
    pub(crate) get_image_key: Option<unsafe extern "C" fn(*mut c_char, c_int) -> bool>,
}

static DLL: OnceLock<Mutex<Option<WxKeyDll>>> = OnceLock::new();

impl WxKeyDll {
    #[cfg(target_os = "windows")]
    pub(crate) fn load(path: &Path) -> Result<Self, String> {
        use libloading::Library;
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::System::LibraryLoader::SetDllDirectoryW;

        // 先让依赖的 VC 运行库（msvcp140/vcruntime140 等）可从 resources/runtime/win32 解析
        // path = .../resources/key/win32/x64/wx_key.dll，ancestors[4] = resources 根
        let search_dir = path
            .ancestors()
            .nth(4)
            .map(|r| r.join("runtime").join("win32"))
            .filter(|p| p.is_dir())
            .or_else(|| {
                path.parent()
                    .map(|p| p.to_path_buf())
                    .filter(|p| p.is_dir())
            });

        unsafe {
            let restore_after = if let Some(dir) = search_dir.as_ref() {
                let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
                let r = SetDllDirectoryW(PCWSTR(wide.as_ptr()));
                r.is_ok()
            } else {
                false
            };

            let load = (|| -> Result<Self, String> {
                let lib = Library::new(path).map_err(|e| format!("加载 wx_key.dll 失败: {}", e))?;
                unsafe fn sym<T: Copy>(
                    lib: &libloading::Library,
                    name: &[u8],
                ) -> Result<T, String> {
                    lib.get::<T>(name).map(|s| *s).map_err(|e| {
                        format!("找不到导出函数 {:?}: {}", String::from_utf8_lossy(name), e)
                    })
                }
                let init_hook = sym(&lib, b"InitializeHook\0")?;
                let poll_key_data = sym(&lib, b"PollKeyData\0")?;
                let get_status_message = sym(&lib, b"GetStatusMessage\0")?;
                let cleanup_hook = sym(&lib, b"CleanupHook\0")?;
                let get_last_error_msg = sym(&lib, b"GetLastErrorMsg\0")?;
                let get_image_key = lib
                    .get::<unsafe extern "C" fn(*mut c_char, c_int) -> bool>(b"GetImageKey\0")
                    .ok()
                    .map(|s| *s);
                Ok(Self {
                    _lib: lib,
                    init_hook,
                    poll_key_data,
                    get_status_message,
                    cleanup_hook,
                    get_last_error_msg,
                    get_image_key,
                })
            })();

            if restore_after {
                let _ = SetDllDirectoryW(PCWSTR(std::ptr::null()));
            }
            load
        }
    }

    /// 最近一次失败的中文错误消息（DLL 内部缓存）
    pub(crate) fn last_error_string(&self) -> String {
        unsafe {
            let p = (self.get_last_error_msg)();
            if p.is_null() {
                "未知错误".to_string()
            } else {
                CStr::from_ptr(p).to_string_lossy().to_string()
            }
        }
    }
}

/// 定位 wx_key.dll：环境变量 > v2.1.8（桌面版，新版本表）> 应用资源目录（生产包）>
/// 源码资源目录（dev）> WeFlow 原始路径
pub fn locate_wx_key_dll(app: Option<&tauri::AppHandle>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ST_WX_KEY_DLL") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    // v2.1.8 Flutter 版捆绑的 DLL（实测 4.1.12.26 主进程注入成功）
    let v218 = PathBuf::from(
        r"C:\Users\Administrator\Desktop\ST_Server\wx_key-windows-v2.1.8\data\flutter_assets\assets\dll\wx_key.dll",
    );
    if v218.is_file() {
        return Some(v218);
    }
    if let Some(app) = app {
        use tauri::Manager;
        if let Ok(res) = app.path().resource_dir() {
            for cand in [
                res.join("wx_key.dll"),
                res.join("resources")
                    .join("key")
                    .join("win32")
                    .join("x64")
                    .join("wx_key.dll"),
            ] {
                if cand.is_file() {
                    return Some(cand);
                }
            }
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev = manifest
        .join("resources")
        .join("key")
        .join("win32")
        .join("x64")
        .join("wx_key.dll");
    if dev.is_file() {
        return Some(dev);
    }
    let weflow = PathBuf::from(r"D:\WeFlow\resources\resources\key\win32\x64\wx_key.dll");
    if weflow.is_file() {
        return Some(weflow);
    }
    None
}

/// 获取 DLL 句柄（首次使用时加载，之后复用）
pub(crate) fn get_dll(
    app: Option<&tauri::AppHandle>,
) -> Result<std::sync::MutexGuard<'static, Option<WxKeyDll>>, String> {
    let m = DLL.get_or_init(|| Mutex::new(None));
    let mut guard = m.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let path = locate_wx_key_dll(app)
            .ok_or_else(|| "找不到 wx_key.dll。请确认资源已随应用打包，或设置环境变量 ST_WX_KEY_DLL 指向微信密钥 DLL。".to_string())?;
        *guard = Some(WxKeyDll::load(&path)?);
    }
    Ok(guard)
}

/// 查找本机微信进程（Weixin.exe / WeChat.exe）
pub fn find_wechat_pids() -> Vec<u32> {
    #[cfg(target_os = "windows")]
    {
        // 与 wx_key.exe 一致：用 Toolhelp 快照枚举（sysinfo 在某些会话下会漏进程）
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return Vec::new();
            };
            let mut pe = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut pids = Vec::new();
            if Process32FirstW(snap, &mut pe).is_ok() {
                loop {
                    let len = pe
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(pe.szExeFile.len());
                    let name = String::from_utf16_lossy(&pe.szExeFile[..len]).to_lowercase();
                    if name == "weixin.exe" || name == "wechat.exe" {
                        pids.push(pe.th32ProcessID);
                    }
                    if Process32NextW(snap, &mut pe).is_err() {
                        break;
                    }
                }
            }
            let _ = windows::Win32::Foundation::CloseHandle(snap);
            pids.sort_unstable();
            pids.dedup();
            pids
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        use sysinfo::{ProcessesToUpdate, System};
        let mut sys = System::new_all();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        let mut pids: Vec<u32> = sys
            .processes()
            .iter()
            .filter(|(_, p)| {
                let n = p.name().to_string_lossy().to_lowercase();
                n == "weixin.exe" || n == "wechat.exe" || n == "wechat"
            })
            .map(|(pid, _)| pid.as_u32())
            .collect();
        pids.sort_unstable();
        pids.dedup();
        pids
    }
}
