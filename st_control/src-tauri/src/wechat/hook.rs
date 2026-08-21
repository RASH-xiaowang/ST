//! 微信原图 Hook —— 通过 `img_helper.dll` 模拟打开图片，强制微信下载高清原图。
//!
//! 实现参考 WeFlow（CC BY-NC-SA 4.0）的 ImageDownloadService：
//! ```text
//!   bool InitImgHelper(uint32 pid, const char* whitelist);
//!   void UninstallImgHelper();
//!   const char* GetImgHelperError();
//! ```
//! `whitelist` 为以 NUL 分隔的会话 username 列表；空串 = 全部会话生效。
//! 仅支持 Windows x64 + 微信 4.0（Weixin.exe）。

use std::ffi::CString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::config::default_st_result_dir;

/// Hook 持久化配置（存于 `AppData\Roaming\st_result\hook_config.json`）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub whitelist: Vec<String>,
}

/// 前端可见的 Hook 状态
#[derive(Debug, Clone, Serialize)]
pub struct HookStatus {
    pub supported: bool,
    pub enabled: bool,
    pub hooked: bool,
    pub pid: Option<u32>,
    pub whitelist: Vec<String>,
    pub error: String,
    pub dll_ok: bool,
}

#[derive(Default)]
struct HookInner {
    lib: Option<libloading::Library>,
    error: String,
}

/// 全局原图 Hook 管理器（由 Tauri 托管为 `Arc<HookManager>`）
pub struct HookManager {
    inner: Mutex<HookInner>,
    enabled: AtomicBool,
    pid: AtomicU32,
    whitelist: Mutex<Vec<String>>,
    reloop: AtomicBool,
}

impl Default for HookManager {
    fn default() -> Self {
        Self {
            inner: Mutex::new(HookInner::default()),
            enabled: AtomicBool::new(false),
            pid: AtomicU32::new(0),
            whitelist: Mutex::new(Vec::new()),
            reloop: AtomicBool::new(false),
        }
    }
}

impl HookManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn config_path() -> PathBuf {
        default_st_result_dir().join("hook_config.json")
    }

    pub fn load_config() -> HookConfig {
        std::fs::read(Self::config_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save_config(&self) {
        let cfg = HookConfig {
            enabled: self.enabled.load(Ordering::SeqCst),
            whitelist: self.whitelist_raw(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&cfg) {
            if let Some(parent) = Self::config_path().parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(Self::config_path(), json);
        }
    }

    fn whitelist_raw(&self) -> Vec<String> {
        self.whitelist.lock().unwrap().clone()
    }

    fn supported() -> bool {
        cfg!(target_os = "windows") && std::env::consts::ARCH == "x86_64"
    }

    /// 查找微信主进程（与 WeFlow 一致：取 commandline 最短的 Weixin.exe 主进程）
    fn find_weixin_pid() -> Option<u32> {
        #[cfg(target_os = "windows")]
        {
            let mut sys = sysinfo::System::new_all();
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            let mut best: Option<(u32, usize)> = None;
            for (pid, proc) in sys.processes() {
                let name = proc.name().to_string_lossy();
                if !name.eq_ignore_ascii_case("Weixin.exe")
                    && !name.eq_ignore_ascii_case("WeChat.exe")
                {
                    continue;
                }
                let cmd_len = proc.cmd().iter().map(|s| s.len()).sum::<usize>();
                if best.map(|(_, l)| cmd_len < l).unwrap_or(true) {
                    best = Some((pid.as_u32(), cmd_len));
                }
            }
            best.map(|(p, _)| p)
        }
        #[cfg(not(target_os = "windows"))]
        {
            None
        }
    }

    /// 定位 img_helper.dll（资源目录 → 可执行文件旁 → 当前目录 → st_result）
    fn dll_path(app: &AppHandle) -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(p) = app.path().resolve(
            "resources/hook/win32/x64/img_helper.dll",
            tauri::path::BaseDirectory::Resource,
        ) {
            candidates.push(p);
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("resources/hook/win32/x64/img_helper.dll"));
                candidates.push(dir.join("img_helper.dll"));
            }
        }
        candidates.push(PathBuf::from("resources/hook/win32/x64/img_helper.dll"));
        candidates.push(default_st_result_dir().join("hook").join("img_helper.dll"));
        candidates.into_iter().find(|p| p.exists())
    }

    fn call_error_fn(lib: &libloading::Library) -> String {
        unsafe {
            let f: libloading::Symbol<unsafe extern "C" fn() -> *const std::os::raw::c_char> =
                match lib.get(b"GetImgHelperError\0") {
                    Ok(s) => s,
                    Err(_) => return "无法解析 GetImgHelperError".to_string(),
                };
            let p = f();
            if p.is_null() {
                return String::new();
            }
            std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }

    /// 注入微信进程（whitelist 空 = 全部会话）。成功后接管 lib 句柄。
    fn init_hook(&self, app: &AppHandle, pid: u32) -> Result<(), String> {
        let dll = Self::dll_path(app)
            .ok_or_else(|| "未找到 img_helper.dll（请检查资源目录）".to_string())?;
        let lib = unsafe { libloading::Library::new(&dll) }
            .map_err(|e| format!("加载 img_helper.dll 失败: {e}"))?;

        let whitelist = self.whitelist_raw();
        let buf = if whitelist.is_empty() {
            CString::new("").map_err(|e| format!("白名单编码失败: {e}"))?
        } else {
            CString::new(format!("{}\0\0", whitelist.join("\0")))
                .map_err(|e| format!("白名单编码失败: {e}"))?
        };

        let ok = {
            let init: libloading::Symbol<
                unsafe extern "C" fn(u32, *const std::os::raw::c_char) -> bool,
            > = unsafe { lib.get(b"InitImgHelper\0") }
                .map_err(|e| format!("解析 InitImgHelper 失败: {e}"))?;
            unsafe { init(pid, buf.as_ptr()) }
        };

        if !ok {
            let err = Self::call_error_fn(&lib);
            let msg = if err.is_empty() {
                "InitImgHelper 返回失败".to_string()
            } else {
                err
            };
            self.inner.lock().unwrap().error = msg.clone();
            return Err(msg);
        }

        // 替换旧句柄（先卸载旧的）
        let mut inner = self.inner.lock().unwrap();
        if let Some(old) = inner.lib.take() {
            unsafe {
                if let Ok(f) = old.get::<unsafe extern "C" fn()>(b"UninstallImgHelper\0") {
                    f();
                }
            }
            drop(old);
        }
        inner.error.clear();
        inner.lib = Some(lib);
        self.pid.store(pid, Ordering::SeqCst);
        Ok(())
    }

    fn uninstall(&self) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(lib) = inner.lib.take() {
            unsafe {
                if let Ok(f) = lib.get::<unsafe extern "C" fn()>(b"UninstallImgHelper\0") {
                    f();
                }
            }
            drop(lib);
        }
        inner.error.clear();
        self.pid.store(0, Ordering::SeqCst);
    }

    /// 启动 Hook 服务（持久化配置 + 立即注入 + 30s 轮询重挂）
    pub fn start(
        self: &Arc<Self>,
        app: &AppHandle,
        whitelist: Vec<String>,
    ) -> Result<HookStatus, String> {
        *self.whitelist.lock().unwrap() = whitelist;
        self.enabled.store(true, Ordering::SeqCst);
        self.save_config();

        if !Self::supported() {
            let msg = "当前平台不支持原图 Hook（仅 Windows x64）".to_string();
            self.inner.lock().unwrap().error = msg.clone();
            return Err(msg);
        }

        match Self::find_weixin_pid() {
            Some(pid) => {
                self.init_hook(app, pid)?;
            }
            None => {
                self.uninstall();
                self.inner.lock().unwrap().error = "等待微信启动".to_string();
            }
        }
        self.ensure_reloop(app);
        Ok(self.status(app))
    }

    /// 停止 Hook 服务
    pub fn stop(&self, app: &AppHandle) -> Result<HookStatus, String> {
        self.uninstall();
        self.enabled.store(false, Ordering::SeqCst);
        self.save_config();
        Ok(self.status(app))
    }

    /// 更新白名单；服务启用中立即以新名单重新注入
    pub fn set_whitelist(
        &self,
        app: &AppHandle,
        whitelist: Vec<String>,
    ) -> Result<HookStatus, String> {
        *self.whitelist.lock().unwrap() = whitelist;
        self.save_config();
        if self.enabled.load(Ordering::SeqCst) && Self::supported() {
            match Self::find_weixin_pid() {
                Some(pid) => {
                    self.init_hook(app, pid)?;
                }
                None => {
                    self.uninstall();
                    self.inner.lock().unwrap().error = "等待微信启动".to_string();
                }
            }
        }
        Ok(self.status(app))
    }

    pub fn status(&self, app: &AppHandle) -> HookStatus {
        let inner = self.inner.lock().unwrap();
        HookStatus {
            supported: Self::supported(),
            enabled: self.enabled.load(Ordering::SeqCst),
            hooked: inner.lib.is_some(),
            pid: if inner.lib.is_some() {
                Some(self.pid.load(Ordering::SeqCst))
            } else {
                None
            },
            whitelist: self.whitelist_raw(),
            error: inner.error.clone(),
            dll_ok: Self::dll_path(app).is_some(),
        }
    }

    /// 后台 30s 轮询：微信重启后自动重挂；微信退出后卸载等待
    fn ensure_reloop(self: &Arc<Self>, app: &AppHandle) {
        if self.reloop.swap(true, Ordering::SeqCst) {
            return;
        }
        let handle = app.clone();
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                if !this.enabled.load(Ordering::SeqCst) || !Self::supported() {
                    break;
                }
                match Self::find_weixin_pid() {
                    Some(pid) => {
                        if this.pid.load(Ordering::SeqCst) != pid {
                            let _ = this.init_hook(&handle, pid);
                        }
                    }
                    None => {
                        this.uninstall();
                        this.inner.lock().unwrap().error = "等待微信启动".to_string();
                    }
                }
            }
            this.reloop.store(false, Ordering::SeqCst);
        });
    }
}

// ============ Tauri IPC ============

#[tauri::command]
pub async fn img_hook_start(app: AppHandle, whitelist: Vec<String>) -> Result<HookStatus, String> {
    let state = app.state::<Arc<HookManager>>();
    state.start(&app, whitelist)
}

#[tauri::command]
pub async fn img_hook_stop(app: AppHandle) -> Result<HookStatus, String> {
    let state = app.state::<Arc<HookManager>>();
    state.stop(&app)
}

#[tauri::command]
pub async fn img_hook_set_whitelist(
    app: AppHandle,
    whitelist: Vec<String>,
) -> Result<HookStatus, String> {
    let state = app.state::<Arc<HookManager>>();
    state.set_whitelist(&app, whitelist)
}

#[tauri::command]
pub async fn img_hook_status(app: AppHandle) -> Result<HookStatus, String> {
    let state = app.state::<Arc<HookManager>>();
    Ok(state.status(&app))
}
