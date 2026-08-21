// ============================================================
// 通用纯函数（跨 feature 共享）
// 收敛 bot/ilink 与 bot/channels 等处的重复实现；不依赖业务状态。
// ============================================================

use std::error::Error;

/// 双写 writer：日志同时输出到 stderr 与文件。
/// 部署后的 GUI 应用没有控制台，stderr 无人消费，日志必须落盘到
/// `<base>/data/logs/app.log`；开发时保留 stderr 便于终端观察。
pub struct LogTee {
    primary: std::io::Stderr,
    file: std::fs::File,
}

impl LogTee {
    pub fn new(primary: std::io::Stderr, file: std::fs::File) -> Self {
        Self { primary, file }
    }
}

impl std::io::Write for LogTee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = self.primary.write(buf);
        self.file.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let _ = self.primary.flush();
        self.file.flush()
    }
}

/// 完整错误链描述（reqwest 顶层信息 + 逐层 cause，便于定位 TLS/代理/连接问题）
pub fn describe_reqwest_error(e: &reqwest::Error) -> String {
    let mut msg = format!("{e}");
    let mut src = e.source();
    while let Some(s) = src {
        msg.push_str(&format!(" ← {s}"));
        src = s.source();
    }
    msg
}

/// 按字符数截断（超过 n 追加省略号；按 char 计，兼容 emoji 等宽字符）
pub fn truncate(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…")
    } else {
        t
    }
}

/// 应用数据根目录：`<应用基目录>/data`。
///
/// 全部应用资源统一收敛到应用基目录下的 `data/`（部署后即安装目录），
/// 不再散落 %APPDATA%。旧数据由 `migrate_legacy_dirs` 在启动时迁移。
pub fn st_data_dir() -> std::path::PathBuf {
    app_base_dir().join("data")
}

/// 微信数据根目录：`<应用基目录>/data/wechat`（原 %APPDATA%/st_result）。
/// 收敛 llm/stt 语音缓存目录构造重复与各微信模块的派生数据文件。
pub fn wechat_data_dir() -> std::path::PathBuf {
    st_data_dir().join("wechat")
}

/// 兼容旧名：微信解密结果根目录（现与 wechat_data_dir 同一目录）。
#[allow(dead_code)] // 旧名兼容：llm/stt 语音缓存等测试与部分模块仍引用
pub fn st_result_dir() -> std::path::PathBuf {
    wechat_data_dir()
}

/// 角色数据目录：`<应用基目录>/data/roles`（原 %APPDATA%/st_role）
pub fn role_data_dir() -> std::path::PathBuf {
    st_data_dir().join("roles")
}

/// 应用日志目录：`<应用基目录>/data/logs`
pub fn logs_dir() -> std::path::PathBuf {
    st_data_dir().join("logs")
}

/// 应用基目录（唯一基准，所有相对路径与配置均以此为根）。
///
/// 解析优先级：
/// 1. 环境变量 `ST_WECHAT_APP_DIR`（显式覆盖，部署/测试用）
/// 2. debug 构建：从可执行文件目录向上找项目根（含 `package.json` 与
///    `src-tauri`），找不到再退回 exe 目录——开发时始终指向项目根
/// 3. release 构建：可执行文件所在目录（即应用安装目录）
///
/// 【历史教训】曾用「当前工作目录」作为基目录：从不同目录启动应用会
/// 加载到不同份 config.json、数据写到不同位置，配置散落、密钥丢失。
/// 因此基目录绝不能依赖 CWD。
pub fn app_base_dir() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Ok(d) = std::env::var("ST_WECHAT_APP_DIR") {
        let p = PathBuf::from(&d);
        if p.is_dir() {
            return p;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let exe_dir = exe
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        if cfg!(debug_assertions) {
            // dev 构建（cargo run / target/debug 下的 exe）：向上定位项目根
            let mut dir = exe_dir.clone();
            while !dir.as_os_str().is_empty() {
                if dir.join("package.json").is_file() && dir.join("src-tauri").is_dir() {
                    return dir;
                }
                match dir.parent() {
                    Some(p) => dir = p.to_path_buf(),
                    None => break,
                }
            }
            if exe_dir.is_dir() {
                return exe_dir;
            }
        } else if exe_dir.is_dir() {
            return exe_dir;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// 确保基础目录存在（启动早期调用；日志初始化之前）
pub fn ensure_base_dirs() {
    for d in [
        st_data_dir(),
        wechat_data_dir(),
        logs_dir(),
        role_data_dir(),
    ] {
        std::fs::create_dir_all(&d).ok();
    }
}

/// 启动时迁移旧版散落目录 → 统一 data 目录（幂等、可重复执行）。
///
/// - `%APPDATA%/st-control` → `<base>/data/`（control.db、llm、kb、bot、stt、ocr…）
/// - `%APPDATA%/st_result`  → `<base>/data/wechat/`（解密库、图片、all_keys.json…）
/// - `%APPDATA%/st_role`    → `<base>/data/roles/`
///
/// 策略：目标已存在的文件跳过（保留新版）；全部拷贝成功后把旧目录
/// 改名为 `*.legacy-backup`（改名失败则原样保留并记录，不影响运行）。
/// 返回迁移报告（供日志与前端展示）。
pub fn migrate_legacy_dirs() -> Vec<String> {
    let mut report = Vec::new();
    let Some(legacy_root) = dirs::data_dir() else {
        return report;
    };
    let data_root = st_data_dir();

    let plans: [(&str, std::path::PathBuf); 3] = [
        ("st-control", data_root.clone()),
        ("st_result", wechat_data_dir()),
        ("st_role", role_data_dir()),
    ];
    for (name, target) in plans {
        let legacy = legacy_root.join(name);
        if !legacy.is_dir() {
            continue;
        }
        std::fs::create_dir_all(&target).ok();
        match copy_dir_merge(&legacy, &target) {
            Ok((copied, skipped, failed)) => {
                report.push(format!(
                    "旧目录 {name} → {} 迁移完成（拷贝 {copied}，跳过 {skipped}，失败 {failed}）",
                    target.display()
                ));
                if failed == 0 {
                    // 全部落盘后把旧目录改名备份（可人工删除回收空间）
                    let bak = legacy_root.join(format!("{name}.legacy-backup"));
                    if std::fs::rename(&legacy, &bak).is_ok() {
                        report.push(format!("旧目录已改名备份: {}", bak.display()));
                    }
                }
            }
            Err(e) => report.push(format!("旧目录 {name} 迁移失败: {e}")),
        }
    }
    report
}

/// 递归合并拷贝：目标已存在则跳过；单个文件失败记录但不中断。
/// 返回 (拷贝数, 跳过数, 失败数)。
fn copy_dir_merge(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> std::io::Result<(u64, u64, u64)> {
    let mut copied = 0u64;
    let mut skipped = 0u64;
    let mut failed = 0u64;
    let entries = std::fs::read_dir(src)?;
    for entry in entries.flatten() {
        let p = entry.path();
        let rel = p.strip_prefix(src).unwrap_or(&p);
        let target = dst.join(rel);
        if p.is_dir() {
            std::fs::create_dir_all(&target).ok();
            match copy_dir_merge(&p, &target) {
                Ok((c, s, f)) => {
                    copied += c;
                    skipped += s;
                    failed += f;
                }
                Err(_) => failed += 1,
            }
        } else if target.exists() {
            skipped += 1;
        } else {
            std::fs::create_dir_all(target.parent().unwrap_or(dst)).ok();
            match std::fs::copy(&p, &target) {
                Ok(_) => copied += 1,
                Err(_) => failed += 1,
            }
        }
    }
    Ok((copied, skipped, failed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_unchanged() {
        assert_eq!(truncate("abc", 5), "abc");
    }

    #[test]
    fn truncate_long_appends_ellipsis() {
        assert_eq!(truncate("abcdef", 3), "abc…");
    }

    #[test]
    fn truncate_counts_chars_not_bytes() {
        assert_eq!(truncate("你好世界", 2), "你好…");
    }

    #[test]
    fn truncate_empty() {
        assert_eq!(truncate("", 3), "");
    }

    /// 迁移合并拷贝：递归拷贝、目标已存在跳过、计数正确
    #[test]
    fn copy_dir_merge_copies_and_skips() {
        let root = std::env::temp_dir().join(format!("st-copy-merge-{}", std::process::id()));
        let src = root.join("src");
        let dst = root.join("dst");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), b"aaa").unwrap();
        std::fs::write(src.join("sub").join("b.txt"), b"bbb").unwrap();
        // 目标预置同名文件 → 应被跳过且内容不变
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(dst.join("a.txt"), b"OLD").unwrap();

        let (copied, skipped, failed) = copy_dir_merge(&src, &dst).unwrap();
        assert_eq!(copied, 1, "只应拷贝 b.txt");
        assert_eq!(skipped, 1, "a.txt 应跳过");
        assert_eq!(failed, 0);
        assert_eq!(std::fs::read(dst.join("a.txt")).unwrap(), b"OLD");
        assert_eq!(
            std::fs::read(dst.join("sub").join("b.txt")).unwrap(),
            b"bbb"
        );

        // 幂等：再次执行全部跳过
        let (_, skipped2, _) = copy_dir_merge(&src, &dst).unwrap();
        assert_eq!(skipped2, 2);

        std::fs::remove_dir_all(&root).ok();
    }

    /// 统一数据目录都在应用基目录下
    #[test]
    fn data_dirs_live_under_app_base() {
        let base = app_base_dir();
        assert!(st_data_dir().starts_with(&base));
        assert!(wechat_data_dir().starts_with(&base));
        assert!(role_data_dir().starts_with(&base));
        assert!(logs_dir().starts_with(&base));
    }
}
