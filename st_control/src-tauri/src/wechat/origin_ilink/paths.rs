// ============================================================
// 消息原图官方通道回退 — 路径与版本护栏域
// 自 origin_ilink.rs 拆分：资源/安装目录定位与兼容性判定。
// ============================================================

use std::path::{Path, PathBuf};

use crate::wechat::auto_key::locate_weixin_exe;
use crate::wechat::config::default_st_result_dir;

/// 已验证兼容的微信版本（ilink2.dll 原图下载通道）
pub(crate) const KNOWN_ILINK_VERSIONS: &[&str] = &["4.1.11.24", "4.1.12.26"];

/// 打包在应用资源中的下载器（wechat-cdn-poc.exe）
pub(crate) fn origin_exe_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ST_ORIGIN_EXE") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/origin/wechat-cdn-poc.exe");
    if dev.is_file() {
        return Some(dev);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for cand in [
                dir.join("resources/origin/wechat-cdn-poc.exe"),
                dir.join("wechat-cdn-poc.exe"),
            ] {
                if cand.is_file() {
                    return Some(cand);
                }
            }
        }
    }
    None
}

pub(crate) fn origin_bridge_path() -> Option<PathBuf> {
    let dev =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/origin/wxcdn_origin_bridge.dll");
    if dev.is_file() {
        return Some(dev);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for cand in [
                dir.join("resources/origin/wxcdn_origin_bridge.dll"),
                dir.join("wxcdn_origin_bridge.dll"),
            ] {
                if cand.is_file() {
                    return Some(cand);
                }
            }
        }
    }
    None
}

/// 微信安装目录（Weixin.exe 所在，含 ilink_wrapper.dll / ilink2.dll）
pub fn wechat_install_dir() -> Option<PathBuf> {
    let exe = locate_weixin_exe().or_else(locate_weixin_exe_process)?;
    let dir = exe.parent()?;
    if dir.join("ilink_wrapper.dll").is_file() {
        return Some(dir.to_path_buf());
    }
    // 版本子目录 4.x.y.z\ilink_wrapper.dll（Weixin.exe 常为启动器，DLL 在版本子目录）
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut versions: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| {
                            n.split('.').all(|part| {
                                !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())
                            })
                        })
                        .unwrap_or(false)
            })
            .collect();
        versions.sort();
        for v in versions.into_iter().rev() {
            if v.join("ilink_wrapper.dll").is_file() {
                return Some(v);
            }
        }
    }
    dir.parent()
        .filter(|p| p.join("ilink_wrapper.dll").is_file())
        .map(Path::to_path_buf)
}

/// 按运行中的微信进程路径定位（覆盖注册表/常见目录之外的安装位置）
pub(crate) fn locate_weixin_exe_process() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use std::mem::size_of;
        use windows::core::PWSTR;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
            let mut pe = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut result = None;
            if Process32FirstW(snap, &mut pe).is_ok() {
                loop {
                    let len = pe
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(pe.szExeFile.len());
                    let name = String::from_utf16_lossy(&pe.szExeFile[..len]).to_lowercase();
                    if name == "weixin.exe" || name == "wechat.exe" {
                        if let Ok(h) =
                            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pe.th32ProcessID)
                        {
                            let mut buf = [0u16; 1024];
                            let mut sz = buf.len() as u32;
                            let ok = QueryFullProcessImageNameW(
                                h,
                                PROCESS_NAME_FORMAT(0),
                                PWSTR(buf.as_mut_ptr()),
                                &mut sz,
                            )
                            .is_ok();
                            let _ = CloseHandle(h);
                            if ok {
                                result = Some(PathBuf::from(String::from_utf16_lossy(
                                    &buf[..sz as usize],
                                )));
                                break;
                            }
                        }
                    }
                    if Process32NextW(snap, &mut pe).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
            result
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// 隔离沙箱根目录（会话复制 + 下载产物，全部落在 st_result 下）
pub(crate) fn sandbox_dir() -> PathBuf {
    default_st_result_dir().join("origin_ilink")
}

/// 版本护栏：白名单版本直接放行；未知版本仅当历史端到端校验通过（compat_ok）才放行
pub(crate) fn ilink_compatible(sandbox: &Path, version: Option<&str>) -> bool {
    if let Some(v) = version {
        if KNOWN_ILINK_VERSIONS.contains(&v) {
            return true;
        }
    }
    sandbox.join("compat_ok").is_file()
}
