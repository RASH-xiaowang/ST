// ============================================================
// 微信密钥获取 — HMAC 预言机（SQLCipher 4 page-1 校验）
// 自 auto_key.rs 拆分：master key 候选校验（raw-key / password 两种
// 模式）与共享读库 page-1。
// ============================================================

// ============ HMAC 预言机（SQLCipher 4 page-1 校验） ============

/// 校验候选 32 字节是否为该库的 master key：
///   1. raw-key 模式：直接作为 AES 密钥验证 page-1 HMAC（≤4.0.x）
///   2. password 模式：PBKDF2-HMAC-SHA512(cand, page1_salt, 256000) 派生后验证（4.1.10.31+）
///
/// 碰撞不可行，只有真 key 能通过。
pub(crate) fn is_valid_master_key(cand: &[u8], page1: &[u8]) -> bool {
    if cand.len() != 32 || page1.len() < 4096 {
        return false;
    }
    // 熵门：跳过全零/低熵缓冲，避免对每个栈槽做 256k 轮 PBKDF2
    let nz = cand.iter().filter(|&&b| b != 0).count();
    if nz < 8 {
        return false;
    }
    if hmac_check(cand, page1) {
        return true;
    }
    let salt = &page1[0..16];
    let derived = crate::wechat::crypto::derive_enc_key(cand, salt, Some("wx_key_v4.1"));
    hmac_check(&derived, page1)
}

/// 用候选 enc_key 验证 message_0.db page-1 的 HMAC-SHA512（reserve=80 布局）
pub(crate) fn hmac_check(enc_key: &[u8], page1: &[u8]) -> bool {
    crate::wechat::crypto::verify_page1_hmac(enc_key, &page1[0..16], page1)
}

/// 以 FILE_SHARE_READ|WRITE|DELETE 打开数据库，读取 page-1 4096 字节。
/// 微信进程持有独占锁时仍可读取（HMAC 预言机需要）。
#[cfg(target_os = "windows")]
pub(crate) fn read_db_page1_shared(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    if path.metadata().map(|m| m.len()).unwrap_or(0) < 4096 {
        return Err(format!("{} 文件过小，无法读取 page-1", path.display()));
    }

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let handle = CreateFileW(
            PCWSTR(wide.as_ptr()),
            windows::Win32::Foundation::GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
        .map_err(|e| format!("打开 {} 失败: {}", path.display(), e))?;
        let mut buf = vec![0u8; 4096];
        let ok = ReadFile(handle, Some(&mut buf), None, None);
        let _ = CloseHandle(handle);
        if let Err(e) = ok {
            return Err(format!("读取 {} page-1 失败: {}", path.display(), e));
        }
        Ok(buf)
    }
}

#[cfg(not(target_os = "windows"))]
fn read_db_page1_shared(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 4096];
    f.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}
