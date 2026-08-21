//! 微信密钥全自动获取（对标 WeFlow Windows 路径）
//!
//! 数据库密钥：
//!   - 加载 `wx_key.dll`，对微信进程 `InitializeHook(pid)` 注入密钥钩子
//!   - 轮询 `PollKeyData` 取回 64 位 hex passphrase
//!   - 复用现有 PBKDF2-per-DB 校验逻辑，生成 `all_keys.json`
//!
//! 图片密钥：
//!   - `GetImageKey` 读取微信 kvcomm 缓存中的密钥码 `code`
//!   - `md5(code + wxid)[:16]` 派生 AES key（16 字符 ASCII hex），`code & 0xFF` 为 XOR key
//!   - 用 `*_t.dat` 模板（头 6 字节 `07 08 56 32 08 07`，密文偏移 15..31）
//!     做 AES-128-ECB 魔数校验，通过才算 verified

const DB_KEY_POLL_MS: u64 = 120;
const DB_KEY_BUF: usize = 128;
const STATUS_MSG_BUF: usize = 256;
const IMAGE_KEY_BUF: usize = 8192;

mod pe;
pub(crate) use pe::*;
mod oracle;
pub(crate) use oracle::*;
mod ffi;
pub(crate) use ffi::*;
mod imagekey;
pub(crate) use imagekey::*;
mod dbkey;
pub(crate) use dbkey::*;

// ============ Rust 调试器：DEBUG_PROCESS 提取 master key ============

#[cfg(target_os = "windows")]
mod debugger;

// ============ 一键全自动：DB 密钥 + 图片密钥 ============

pub fn auto_get_wechat_keys(
    app: &tauri::AppHandle,
    op: &str,
    timeout_ms: u64,
) -> Result<serde_json::Value, String> {
    let db = auto_get_db_key(app, op, timeout_ms)?;
    let img = auto_get_image_key(app, op, None, None)?;
    Ok(serde_json::json!({ "db_key": db, "image_key": img }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{c_int, CStr};
    use std::time::{Duration, Instant};

    /// 静态定位冒烟测试：本机 Weixin.dll 应能解析出 key-set 函数 RVA。
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_locate_keyset_function() {
        let Some(dll_path) = locate_weixin_dll() else {
            eprintln!("未找到本机 Weixin.dll，跳过");
            return;
        };
        let bytes = std::fs::read(&dll_path).expect("读取 Weixin.dll 失败");
        let rvas = find_keyset_function_rvas(&bytes).expect("静态定位 key-set 函数失败");
        println!("Weixin.dll: {}", dll_path.display());
        println!("key-set 函数 RVA 候选: {:?}", rvas);
        assert!(!rvas.is_empty(), "应至少命中 1 个函数 RVA");
    }

    /// HMAC 预言机冒烟测试：用 config 中已知密钥验证 message_0.db page-1。
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_hmac_oracle() {
        // 1) 找已知 enc_key（config.json db_enc_key 或已知值）
        let known = std::fs::read_to_string(crate::wechat::config::get_config_path())
            .ok()
            .and_then(|s| {
                serde_json::from_str::<serde_json::Value>(&s)
                    .ok()
                    .and_then(|v| {
                        v.get("db_enc_key")
                            .and_then(|k| k.as_str())
                            .map(String::from)
                    })
            })
            .unwrap_or_else(|| {
                "4304020f3020400d967181c0f2f68a45ae9eb6e806f442a2a96c470e7fb62e34".to_string()
            });
        // 2) 找 message_0.db
        let mut db: Option<std::path::PathBuf> = None;
        if let Some(cfg) = crate::wechat::config::WeChatConfig::load().ok() {
            // cfg.db_dir 是 E:\...\wxid_x\db_storage，message 库在 message 子目录
            let cand = cfg.db_dir.join("message").join("message_0.db");
            if cand.is_file() {
                db = Some(cand);
            }
        }
        if db.is_none() {
            for root in crate::wechat::config::candidate_xwechat_roots() {
                if let Ok(entries) = std::fs::read_dir(&root) {
                    for e in entries.flatten() {
                        let cand = e
                            .path()
                            .join("db_storage")
                            .join("message")
                            .join("message_0.db");
                        if cand.is_file() {
                            db = Some(cand);
                            break;
                        }
                    }
                }
                if db.is_some() {
                    break;
                }
            }
        }
        let Some(db_path) = db else {
            eprintln!("未找到 message_0.db，跳过");
            return;
        };
        let bytes = read_db_page1_shared(&db_path).expect("读取 message_0.db page-1 失败");
        println!("HMAC oracle 测试库: {}", db_path.display());
        let key_hex = known.trim();
        let cand: Vec<u8> = (0..key_hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&key_hex[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(cand.len(), 32);
        let ok = is_valid_master_key(&cand, &bytes);
        // 诊断：先用 config 密钥做 password 派生，再分别验证派生结果与原始 key
        let salt = &bytes[0..16];
        let derived = crate::wechat::crypto::derive_enc_key(&cand, salt, Some("wx_key_v4.1"));
        println!(
            "HMAC oracle: key={} valid={} raw_valid={} derived_valid={} salt={} stored_hmac={}",
            key_hex,
            ok,
            hmac_check(&cand, &bytes),
            hmac_check(&derived, &bytes),
            hex::encode(salt),
            hex::encode(&bytes[4096 - 64..4096]),
        );
        assert!(ok, "已知密钥应通过 HMAC 预言机");
        // 反例：翻转 1 字节应失败
        let mut bad = cand.clone();
        bad[0] ^= 0x01;
        assert!(!is_valid_master_key(&bad, &bytes), "篡改密钥不应通过");
    }
    use aes::cipher::{BlockEncrypt, KeyInit};
    use aes::Aes128;

    #[test]
    fn test_clean_wxid() {
        assert_eq!(clean_wxid("wxid_abc"), "wxid_abc");
        assert_eq!(clean_wxid("wxid_abc_f312"), "wxid_abc");
        assert_eq!(clean_wxid("a_b_c"), "a_b");
        assert_eq!(clean_wxid("unknown"), "unknown");
    }

    #[test]
    fn test_derive_image_keys_matches_weflow() {
        // 与 WeFlow JS: md5(`${code}${cleanWxid}`).hex.substr(0,16)
        let (xor, aes) = derive_image_keys(60, "wxid_umyqa86if3lm22_f312");
        assert_eq!(xor, 60);
        assert_eq!(aes.len(), 16);
        let (_, aes2) = derive_image_keys(60, "wxid_umyqa86if3lm22");
        assert_eq!(aes, aes2);
        let (_, aes3) = derive_image_keys(61, "wxid_umyqa86if3lm22");
        assert_ne!(aes, aes3);
    }

    #[test]
    fn test_verify_derived_aes_key_magic() {
        // 构造：真实 AES key 派生后加密 JPEG 头，验证校验函数可识别
        use aes::cipher::generic_array::GenericArray;
        let (_, aes_hex) = derive_image_keys(60, "wxid_umyqa86if3lm22");
        let key = &aes_hex.as_bytes()[..16];
        let cipher = Aes128::new_from_slice(key).unwrap();
        let mut block = [0u8; 16];
        block[..3].copy_from_slice(&[0xFF, 0xD8, 0xFF]);
        let mut ga = GenericArray::clone_from_slice(&block);
        cipher.encrypt_block(&mut ga);
        assert!(verify_derived_aes_key(&aes_hex, ga.as_slice()));
        assert!(!verify_derived_aes_key("0123456789abcdef", ga.as_slice()));
    }

    #[test]
    fn test_find_template_data_synthetic() {
        let dir = std::env::temp_dir().join("st_auto_key_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 构造最小模板：头 6 字节 + 密文 16 字节 + 末两字节 (xor^255, xor^217)
        let xor = 0x3Cu8;
        let mut data = Vec::new();
        data.extend_from_slice(b"\x07\x08V2\x08\x07");
        data.extend_from_slice(&[0u8; 9]); // 补到密文起始 15
        data.extend_from_slice(&[0xABu8; 16]); // 密文占位
        data.push(xor ^ 255);
        data.push(xor ^ 217);
        let f = dir.join("img_1_t.dat");
        std::fs::write(&f, &data).unwrap();

        let (ct, xk) = find_template_data(&dir, 32);
        assert!(ct.is_some());
        assert_eq!(ct.unwrap().len(), 16);
        assert_eq!(xk, Some(xor));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 只读冒烟测试：真实加载 wx_key.dll 并调用 GetImageKey（不注入 Hook，安全）
    #[test]
    #[cfg(target_os = "windows")]
    #[ignore = "需要本机存在 wx_key.dll 与微信缓存"]
    fn smoke_get_image_key() {
        let Some(path) = locate_wx_key_dll(None) else {
            panic!("找不到 wx_key.dll");
        };
        println!("dll: {}", path.display());
        let dll = WxKeyDll::load(&path).expect("加载 wx_key.dll 失败");
        let mut buf = vec![0i8; IMAGE_KEY_BUF];
        let Some(get_image_key) = dll.get_image_key else {
            panic!("当前 wx_key.dll 未导出 GetImageKey");
        };
        let ok = unsafe { get_image_key(buf.as_mut_ptr(), buf.len() as c_int) };
        if ok {
            let text = unsafe { CStr::from_ptr(buf.as_ptr()) }
                .to_string_lossy()
                .to_string();
            println!("GetImageKey: {}", text);
            let resp: Result<ImageKeyResponse, _> = serde_json::from_str(&text);
            match resp {
                Ok(r) => {
                    let codes: Vec<u64> = r
                        .accounts
                        .iter()
                        .flat_map(|a| a.keys.iter().map(|k| k.code))
                        .collect();
                    println!("accounts={} codes={:?}", r.accounts.len(), codes);
                    assert!(!codes.is_empty(), "kvcomm 缓存为空");
                }
                Err(e) => println!(
                    "JSON 解析失败: {}（原始: {}）",
                    e,
                    &text[..text.len().min(200)]
                ),
            }
        } else {
            println!("GetImageKey 返回 false: {}", dll.last_error_string());
        }
    }

    /// 真实 Hook 冒烟测试：对本机微信进程注入密钥钩子并轮询取回 DB 密钥。
    /// 与前端「自动获取数据库密钥」按钮完全同一路径（注入完成后清理 Hook）。
    #[test]
    #[cfg(target_os = "windows")]
    #[ignore = "会对运行中的微信进程注入钩子，需本机微信已登录"]
    fn smoke_db_key_hook() {
        let Some(path) = locate_wx_key_dll(None) else {
            panic!("找不到 wx_key.dll");
        };
        let pids = find_wechat_pids();
        assert!(!pids.is_empty(), "未找到微信进程");
        println!("微信进程: {:?}", pids);
        let dll = WxKeyDll::load(&path).expect("加载 wx_key.dll 失败");

        for (i, pid) in pids.iter().take(3).enumerate() {
            println!("[{}] InitializeHook(pid={})…", i, pid);
            if !unsafe { (dll.init_hook)(*pid) } {
                println!("[{}] 注入失败: {}", i, dll.last_error_string());
                continue;
            }
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut key = [0i8; DB_KEY_BUF];
            let mut status = [0i8; STATUS_MSG_BUF];
            let mut found: Option<String> = None;
            while Instant::now() < deadline {
                if unsafe { (dll.poll_key_data)(key.as_mut_ptr(), key.len() as c_int) } {
                    let s = unsafe { CStr::from_ptr(key.as_ptr()) }
                        .to_string_lossy()
                        .to_string();
                    if s.len() == 64 {
                        found = Some(s);
                        break;
                    }
                }
                for _ in 0..5 {
                    let mut level: c_int = 0;
                    if !unsafe {
                        (dll.get_status_message)(
                            status.as_mut_ptr(),
                            status.len() as c_int,
                            &mut level,
                        )
                    } {
                        break;
                    }
                    let msg = unsafe { CStr::from_ptr(status.as_ptr()) }
                        .to_string_lossy()
                        .to_string();
                    if !msg.is_empty() {
                        println!("[{}] 状态: {}", i, msg);
                    }
                }
                std::thread::sleep(Duration::from_millis(120));
            }
            let _ = unsafe { (dll.cleanup_hook)() };
            if let Some(k) = found {
                println!("PID {} 取回数据库密钥: {}", pid, k);
                return;
            }
            println!("[{}] PID {} 轮询超时", i, pid);
        }
        panic!("未能从微信进程取回数据库密钥");
    }

    /// 调试器机制自测：以 DEBUG_PROCESS 启动 cmd 并运行事件循环，验证无死锁、正常清理。
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_debugger_loop_mechanics() {
        let exe =
            std::path::PathBuf::from(std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into()));
        let mut dbg = debugger::WeChatDebugger::new(exe.clone(), vec![], vec![0u8; 4096], None);
        // cmd /c exit 会在 1~2 秒内退出，验证主进程 EXIT 事件能终止循环
        let deadline = Instant::now() + Duration::from_secs(8);
        let r = dbg.run(deadline);
        println!("debugger run() 返回: {:?}（None 表示无密钥=预期）", r);
        // 进程句柄清理后，被调试进程应已不存在
        let alive = find_process_by_name("cmd.exe");
        println!("残留 cmd.exe 进程数: {}", alive.len());
    }

    fn find_process_by_name(name: &str) -> Vec<u32> {
        use sysinfo::{ProcessesToUpdate, System};
        let mut sys = System::new_all();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        sys.processes()
            .iter()
            .filter(|(_, p)| p.name().to_string_lossy().eq_ignore_ascii_case(name))
            .map(|(pid, _)| pid.as_u32())
            .collect()
    }

    /// 用 v2.1.8 新 DLL 对本机微信注入测试（验证 4.1.12.26 兼容性）。
    #[test]
    #[cfg(target_os = "windows")]
    #[ignore = "会对运行中的微信进程注入钩子，需本机微信已登录"]
    fn smoke_db_key_hook_v2() {
        let path = std::path::PathBuf::from(
            r"C:\Users\Administrator\Desktop\ST_Server\wx_key-windows-v2.1.8\data\flutter_assets\assets\dll\wx_key.dll",
        );
        assert!(path.is_file(), "v2.1.8 wx_key.dll 不存在");
        let dll = WxKeyDll::load(&path).expect("加载 wx_key.dll 失败");
        let pids = find_wechat_pids();
        assert!(!pids.is_empty(), "未找到微信进程");
        println!("微信进程: {:?}", pids);

        for (i, pid) in pids.iter().take(3).enumerate() {
            println!("[{}] InitializeHook(pid={})…", i, pid);
            if !unsafe { (dll.init_hook)(*pid) } {
                println!("[{}] 注入失败: {}", i, dll.last_error_string());
                continue;
            }
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut key = [0i8; DB_KEY_BUF];
            let mut status = [0i8; STATUS_MSG_BUF];
            let mut found: Option<String> = None;
            while Instant::now() < deadline {
                if unsafe { (dll.poll_key_data)(key.as_mut_ptr(), key.len() as c_int) } {
                    let s = unsafe { CStr::from_ptr(key.as_ptr()) }
                        .to_string_lossy()
                        .to_string();
                    if s.len() == 64 {
                        found = Some(s);
                        break;
                    }
                }
                for _ in 0..5 {
                    let mut level: c_int = 0;
                    if !unsafe {
                        (dll.get_status_message)(
                            status.as_mut_ptr(),
                            status.len() as c_int,
                            &mut level,
                        )
                    } {
                        break;
                    }
                    let msg = unsafe { CStr::from_ptr(status.as_ptr()) }
                        .to_string_lossy()
                        .to_string();
                    if !msg.is_empty() {
                        println!("[{}] 状态: {}", i, msg);
                    }
                }
                std::thread::sleep(Duration::from_millis(120));
            }
            let _ = unsafe { (dll.cleanup_hook)() };
            if let Some(k) = found {
                println!("PID {} 取回数据库密钥: {}", pid, k);
                return;
            }
            println!("[{}] PID {} 轮询超时", i, pid);
        }
        panic!("v2.1.8 DLL 未能从微信进程取回数据库密钥");
    }

    /// 诊断：对全部微信进程逐一遍历 v2.1.8 DLL 注入，打印完整状态消息。
    #[test]
    #[cfg(target_os = "windows")]
    #[ignore = "会对运行中的微信进程注入钩子，需本机微信已登录"]
    fn diag_v218_dll_all_pids() {
        let path = std::path::PathBuf::from(
            r"C:\Users\Administrator\Desktop\ST_Server\wx_key-windows-v2.1.8\data\flutter_assets\assets\dll\wx_key.dll",
        );
        let dll = WxKeyDll::load(&path).expect("加载 wx_key.dll 失败");
        let pids = find_wechat_pids();
        println!("全部微信进程: {:?}", pids);

        for pid in &pids {
            println!("===== PID {} =====", pid);
            if !unsafe { (dll.init_hook)(*pid) } {
                println!(
                    "  InitializeHook 失败, GetLastErrorMsg: {}",
                    dll.last_error_string()
                );
            } else {
                println!("  InitializeHook 成功，开始轮询状态…");
            }
            // 排空状态消息
            let mut status = [0i8; STATUS_MSG_BUF];
            for _ in 0..20 {
                let mut level: c_int = 0;
                if !unsafe {
                    (dll.get_status_message)(status.as_mut_ptr(), status.len() as c_int, &mut level)
                } {
                    break;
                }
                let msg = unsafe { CStr::from_ptr(status.as_ptr()) }
                    .to_string_lossy()
                    .to_string();
                println!("  状态[{level}]: {msg}");
            }
            // 轮询 key 几秒
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut key = [0i8; DB_KEY_BUF];
            let mut got = false;
            while Instant::now() < deadline {
                if unsafe { (dll.poll_key_data)(key.as_mut_ptr(), key.len() as c_int) } {
                    let s = unsafe { CStr::from_ptr(key.as_ptr()) }
                        .to_string_lossy()
                        .to_string();
                    println!("  PollKeyData -> {s}");
                    if s.len() == 64 {
                        got = true;
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            println!("  key got: {got}");
            let _ = unsafe { (dll.cleanup_hook)() };
        }
    }

    /// 诊断：枚举当前微信进程与 Weixin.dll 模块加载状态（Toolhelp）。
    #[test]
    #[cfg(target_os = "windows")]
    fn diag_wechat_modules() {
        let pids = find_wechat_pids();
        println!("微信进程: {:?}", pids);
        for &pid in &pids {
            let main = find_main_wechat_pid(&[pid]).is_some();
            println!("  PID {}: 主进程(加载Weixin.dll)={}", pid, main);
        }
    }
}
