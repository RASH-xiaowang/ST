// ============================================================
// 微信密钥获取 — Weixin.dll 静态定位（PE 解析）
// 自 auto_key.rs 拆分：PE 段表解析、key-set 函数 RVA 定位、
// 微信安装路径定位（注册表 + 常见目录）。
// ============================================================

// ============ Weixin.dll 静态定位（4.1.10.31+ 调试器方案） ============
//
// 4.1.10.31 起微信移除了内存中的明文 x'..' 密钥缓存，老式被动扫描失效。
// 本方案（chatlog-keeper 同源思路）：
//   1. 在 Weixin.dll 的 .text 中扫描 wx_key key-set 函数序言签名（4.1.6.14+ 通用）；
//   2. 经异常目录 (exception directory) 二分找到包含该签名的最内层函数 RVA；
//   3. 以 DEBUG_PROCESS 启动微信，在该函数下 INT3 断点，命中后从寄存器/栈收集
//      32 字节候选，用 message_0.db page-1 的 HMAC 预言机验证出真正的 master key。

/// PE 节表项（仅取定位所需字段）
struct PeSection {
    name: [u8; 8],
    virtual_address: u64,
    virtual_size: u64,
    raw_pointer: u64,
    raw_size: u64,
}

impl PeSection {
    fn name_str(&self) -> String {
        String::from_utf8_lossy(&self.name)
            .trim_end_matches('\0')
            .to_string()
    }

    /// RVA 是否落在此节（含虚拟大小）
    fn contains_rva(&self, rva: u64) -> bool {
        rva >= self.virtual_address && rva < self.virtual_address + self.virtual_size
    }
}

struct PeInfo {
    sections: Vec<PeSection>,
    exception_rva: u64,
    exception_size: u64,
    text_index: Option<usize>,
}

fn u16_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// 解析 PE32+，返回节表与异常目录
fn parse_pe(d: &[u8]) -> Result<PeInfo, String> {
    if d.len() < 0x40 || u16_at(d, 0) != 0x5A4D {
        return Err("不是有效的 PE 文件（DOS 头）".to_string());
    }
    let pe_off = u32_at(d, 0x3C) as usize;
    if pe_off + 24 > d.len() || u32_at(d, pe_off) != 0x0000_4550 {
        return Err("不是有效的 PE 文件（PE 签名）".to_string());
    }
    let coff = pe_off + 4;
    let num_sections = u16_at(d, coff + 2) as usize;
    let opt_size = u16_at(d, coff + 16) as usize;
    let opt = coff + 20;
    if opt + 2 > d.len() || u16_at(d, opt) != 0x20B {
        return Err("仅支持 PE32+（64 位）".to_string());
    }
    // 数据目录起始：PE32+ 固定偏移 112（可选头内），异常目录是索引 3
    let dd = opt + 112;
    let exception_rva = u32_at(d, dd + 3 * 8) as u64;
    let exception_size = u32_at(d, dd + 3 * 8 + 4) as u64;

    let sec_off = opt + opt_size;
    let mut sections = Vec::with_capacity(num_sections);
    let mut text_index = None;
    for i in 0..num_sections {
        let so = sec_off + i * 40;
        if so + 40 > d.len() {
            return Err("PE 节表越界".to_string());
        }
        let mut name = [0u8; 8];
        name.copy_from_slice(&d[so..so + 8]);
        let sec = PeSection {
            name,
            virtual_address: u32_at(d, so + 12) as u64,
            virtual_size: u32_at(d, so + 8) as u64,
            raw_pointer: u32_at(d, so + 20) as u64,
            raw_size: u32_at(d, so + 16) as u64,
        };
        if sec.name_str() == ".text" && text_index.is_none() {
            text_index = Some(sections.len());
        }
        sections.push(sec);
    }
    if text_index.is_none() {
        return Err("未找到 .text 节".to_string());
    }
    Ok(PeInfo {
        sections,
        exception_rva,
        exception_size,
        text_index,
    })
}

/// RVA → 文件偏移
fn rva_to_file_offset(pe: &PeInfo, rva: u64) -> Option<u64> {
    pe.sections
        .iter()
        .find(|s| s.contains_rva(rva))
        .map(|s| s.raw_pointer + (rva - s.virtual_address))
}

/// wx_key key-set 函数序言签名（>4.1.6.14 配置；idx5 通配）。
/// 签名: 24 50 48 C7 45 ?? FE FF FF FF 44 89 CF 44 89 C3 49 89 D6
const KEYSET_SIG: &[u8] = &[
    0x24, 0x50, 0x48, 0xC7, 0x45, 0x00, 0xFE, 0xFF, 0xFF, 0xFF, 0x44, 0x89, 0xCF, 0x44, 0x89, 0xC3,
    0x49, 0x89, 0xD6,
];
const KEYSET_SIG_WILDCARDS: &[bool] = &[
    false, false, false, false, false, true, false, false, false, false, false, false, false,
    false, false, false, false, false, false,
];

/// 在 .text 中扫描签名，返回所有命中位置的 RVA 与包含函数 RVA（经异常目录）。
pub fn find_keyset_function_rvas(dll_bytes: &[u8]) -> Result<Vec<u64>, String> {
    let pe = parse_pe(dll_bytes)?;
    let ti = pe.text_index.unwrap();
    let text = &pe.sections[ti];
    let t_start = text.raw_pointer as usize;
    let t_end = (text.raw_pointer + text.raw_size) as usize;
    if t_end > dll_bytes.len() {
        return Err(".text 节超出文件范围".to_string());
    }

    // 异常目录文件偏移
    let exc_fo = match rva_to_file_offset(&pe, pe.exception_rva) {
        Some(f) => f as usize,
        None => return Err("异常目录不在文件映射内".to_string()),
    };
    let n_exc = (pe.exception_size / 12) as usize;

    let mut rvas = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if t_start + KEYSET_SIG.len() > t_end {
        return Err(".text 节过小，无法扫描".to_string());
    }
    for i in t_start..=(t_end - KEYSET_SIG.len()) {
        if dll_bytes[i] != KEYSET_SIG[0] {
            continue;
        }
        let mut ok = true;
        for j in 1..KEYSET_SIG.len() {
            if KEYSET_SIG_WILDCARDS[j] {
                continue;
            }
            if dll_bytes[i + j] != KEYSET_SIG[j] {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        let match_rva = text.virtual_address + (i as u64 - text.raw_pointer);
        // 异常目录二分找包含 match_rva 的函数
        let mut lo = 0usize;
        let mut hi = n_exc;
        let mut func: Option<u64> = None;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let eo = exc_fo + mid * 12;
            if eo + 8 > dll_bytes.len() {
                break;
            }
            let begin = u32_at(dll_bytes, eo) as u64;
            let end_a = u32_at(dll_bytes, eo + 4) as u64;
            if match_rva < begin {
                hi = mid;
            } else if match_rva >= end_a {
                lo = mid + 1;
            } else {
                func = Some(begin);
                break;
            }
        }
        let fr = func.unwrap_or(match_rva.saturating_sub(3));
        if seen.insert(fr) {
            rvas.push(fr);
        }
        if rvas.len() >= 8 {
            break;
        }
    }
    if rvas.is_empty() {
        return Err(
            "wx_key 函数签名未在 Weixin.dll 找到（微信版本变化，需更新特征码）".to_string(),
        );
    }
    Ok(rvas)
}

/// 读取微信安装目录下 Weixin.dll（exe 旁的版本子目录或同目录）
pub fn locate_weixin_dll() -> Option<std::path::PathBuf> {
    let exe = locate_weixin_exe()?;
    let install_dir = exe.parent()?;
    // 版本子目录 4.x.y.z\Weixin.dll
    if let Ok(entries) = std::fs::read_dir(install_dir) {
        let mut vers: Vec<std::path::PathBuf> = entries
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
        vers.sort();
        for v in vers.into_iter().rev() {
            let cand = v.join("Weixin.dll");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    let fallback = install_dir.join("Weixin.dll");
    if fallback.is_file() {
        Some(fallback)
    } else {
        None
    }
}

/// 读取微信安装目录下 Weixin.exe（注册表 InstallPath > 常见目录）
pub fn locate_weixin_exe() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use windows::core::PCWSTR;
        use windows::Win32::System::Registry::*;

        fn read_reg_str(key: HKEY, sub: &str, name: &str) -> Option<String> {
            let subw: Vec<u16> = sub.encode_utf16().chain(Some(0)).collect();
            let namew: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            let mut hkey = HKEY::default();
            unsafe {
                if RegOpenKeyExW(key, PCWSTR(subw.as_ptr()), Some(0), KEY_READ, &mut hkey).is_err()
                {
                    return None;
                }
                let mut buf = [0u16; 1024];
                let mut len = (buf.len() as u32) * 2;
                let r = RegQueryValueExW(
                    hkey,
                    PCWSTR(namew.as_ptr()),
                    None,
                    None,
                    Some(buf.as_mut_ptr() as *mut u8),
                    Some(&mut len),
                );
                let _ = RegCloseKey(hkey);
                if r.is_err() {
                    return None;
                }
                let n = (len as usize) / 2;
                Some(String::from_utf16_lossy(&buf[..n.min(buf.len())]))
            }
        }

        for (root, sub) in [
            (HKEY_CURRENT_USER, "Software\\Tencent\\Weixin"),
            (HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Tencent\\Weixin"),
            (HKEY_LOCAL_MACHINE, "SOFTWARE\\Tencent\\Weixin"),
        ] {
            if let Some(ip) = read_reg_str(root, sub, "InstallPath") {
                let p = std::path::PathBuf::from(ip);
                let exe = p.join("Weixin.exe");
                if exe.is_file() {
                    return Some(exe);
                }
            }
        }
    }
    for cand in [
        r"D:\Weixin\Tencent\Weixin\Weixin.exe",
        r"C:\Program Files\Tencent\Weixin\Weixin.exe",
        r"C:\Program Files (x86)\Tencent\Weixin\Weixin.exe",
    ] {
        let p = std::path::PathBuf::from(cand);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}
