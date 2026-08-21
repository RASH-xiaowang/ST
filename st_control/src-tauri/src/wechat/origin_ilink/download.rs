// ============================================================
// 消息原图官方通道回退 — 下载主流程域
// 自 origin_ilink.rs 拆分：可用性快照 / 超时执行 / 校验回退。
// ============================================================

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use md5::{Digest, Md5};

use crate::wechat::modules::common::msg_table_name;

use super::{
    ensure_sandbox, extract_image_xml, ilink_compatible, origin_bridge_path, origin_exe_path,
    parse_origin_secret, wechat_install_dir, IlinkStatus, KNOWN_ILINK_VERSIONS,
};

/// 单次官方通道下载超时（首次 ilink 初始化可能较慢）
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(150);

fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<std::process::Output, String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动原图下载器失败: {e}"))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("等待下载器失败: {e}"))?
        {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut so) = child.stdout.take() {
                let _ = so.read_to_end(&mut stdout);
            }
            if let Some(mut se) = child.stderr.take() {
                let _ = se.read_to_end(&mut stderr);
            }
            return Ok(std::process::Output {
                status,
                stdout,
                stderr,
            });
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            return Err("原图官方通道下载超时".to_string());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// 官方通道可用性快照（供配置页/状态提示）
pub fn ilink_status() -> IlinkStatus {
    let install = wechat_install_dir();
    let exe = origin_exe_path();
    let bridge = origin_bridge_path();
    let sandbox = ensure_sandbox();
    let version = install
        .as_ref()
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str())
        .map(str::to_owned);
    let wrapper = install
        .as_ref()
        .map(|d| d.join("ilink_wrapper.dll"))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned());
    let sandbox_ready = sandbox.is_ok();
    let (enabled, reason) = match (
        install,
        wrapper.as_ref(),
        exe.as_ref(),
        bridge.as_ref(),
        sandbox.as_ref(),
    ) {
        (Some(_), Some(_), Some(_), Some(_), Ok(sb)) => {
            if ilink_compatible(sb, version.as_deref()) {
                (true, None)
            } else {
                (
                    false,
                    Some(format!(
                        "微信版本 {:?} 未经 ilink 原图通道验证，已按版本护栏禁用",
                        version
                    )),
                )
            }
        }
        (None, _, _, _, _) => (false, Some("未找到微信安装目录".to_string())),
        (_, None, _, _, _) => (
            false,
            Some("微信安装目录缺少 ilink_wrapper.dll".to_string()),
        ),
        (_, _, None, _, _) => (false, Some("缺少原图下载器 wechat-cdn-poc.exe".to_string())),
        (_, _, _, None, _) => (
            false,
            Some("缺少桥接 DLL wxcdn_origin_bridge.dll".to_string()),
        ),
        (_, _, _, _, Err(e)) => (false, Some(e.clone())),
    };
    IlinkStatus {
        enabled,
        wechat_version: version,
        wrapper,
        sandbox_ready,
        downloader: exe.map(|p| p.to_string_lossy().into_owned()),
        reason,
    }
}

/// 通过 ilink 官方通道下载消息原图；返回校验通过的图片字节
pub fn download_origin_via_ilink(username: &str, local_id: i64) -> Result<Vec<u8>, String> {
    let install = wechat_install_dir().ok_or("未找到微信安装目录（Weixin.exe）")?;
    let wrapper = install.join("ilink_wrapper.dll");
    if !wrapper.is_file() {
        return Err("微信安装目录缺少 ilink_wrapper.dll".to_string());
    }
    if !install.join("ilink2.dll").is_file() {
        return Err("微信安装目录缺少 ilink2.dll".to_string());
    }
    let version = install
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_owned);
    let exe =
        origin_exe_path().ok_or("未找到原图下载器 wechat-cdn-poc.exe（请确认已随应用打包）")?;
    let bridge = origin_bridge_path().ok_or("未找到桥接 DLL wxcdn_origin_bridge.dll")?;
    let sandbox = ensure_sandbox()?;
    if !ilink_compatible(&sandbox, version.as_deref()) {
        return Err(format!(
            "微信版本 {:?} 未经 ilink 原图通道验证，已按版本护栏禁用",
            version
        ));
    }

    let cfg = crate::wechat::config::WeChatConfig::load()
        .map_err(|e| format!("读取微信配置失败: {e}"))?;
    let xml = extract_image_xml(&cfg.decrypted_dir, username, local_id)
        .ok_or("未在解密消息库中找到该图片消息的 XML")?;
    let secret = parse_origin_secret(&xml)
        .ok_or("图片消息 XML 缺少原图字段（cdnbigimgurl/aeskey/hdlength）")?;

    let table = msg_table_name(username);
    let source_id = format!("Msg_{table}:{local_id}");
    let json_path = sandbox.join("message.json");
    let doc = serde_json::json!({
        "data": [{ "source_native_id": source_id, "text": xml }]
    });
    std::fs::write(
        &json_path,
        serde_json::to_vec(&doc).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写消息 JSON 失败: {e}"))?;

    let out = sandbox.join("origin.jpg");
    let staging = sandbox.join("official-origin-download.dat");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&staging);
    let work_dir = sandbox.join("work");
    let _ = std::fs::remove_dir_all(&work_dir);

    // 动态生成二进制白名单：微信版本升级后无需重新打包
    let allowlist = sandbox.join("origin-binary-allowlist.json");
    let profile = version.as_deref().unwrap_or("unknown");
    let allow_ok = run_with_timeout(
        Command::new(&exe)
            .arg("create-allowlist")
            .arg("--profile")
            .arg(profile)
            .arg("--wrapper")
            .arg(&wrapper)
            .arg("--bridge")
            .arg(&bridge)
            .arg("--output")
            .arg(&allowlist),
        Duration::from_secs(60),
    )?;
    if !allow_ok.status.success() {
        let err = String::from_utf8_lossy(&allow_ok.stderr);
        return Err(format!("生成原图通道白名单失败: {}", err.trim()));
    }

    let output = run_with_timeout(
        Command::new(&exe)
            .arg("download-origin")
            .arg("--db")
            .arg(cfg.decrypted_dir.join("message").join("message_0.db"))
            .arg("--account")
            .arg(cfg.wxid().unwrap_or_default())
            .arg("--source-id")
            .arg(&source_id)
            .arg("--message-json")
            .arg(&json_path)
            .arg("--wrapper")
            .arg(&wrapper)
            .arg("--bridge")
            .arg(&bridge)
            .arg("--config")
            .arg(sandbox.join("ilink-start-config.bin"))
            .arg("--allowlist")
            .arg(&allowlist)
            .arg("--work-dir")
            .arg(&work_dir)
            .arg("--output")
            .arg(&out),
        DOWNLOAD_TIMEOUT,
    )?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("原图下载器返回失败: {}", err.trim()));
    }
    let bytes = std::fs::read(&out).map_err(|e| format!("读取原图输出失败: {e}"))?;
    if bytes.len() as u64 != secret.original_size {
        return Err(format!(
            "原图大小校验失败：期望 {} 字节，实际 {} 字节",
            secret.original_size,
            bytes.len()
        ));
    }
    if !secret.md5.is_empty() {
        let actual = hex::encode(Md5::digest(&bytes));
        if !actual.eq_ignore_ascii_case(&secret.md5) {
            return Err("原图 MD5 校验失败，已丢弃".to_string());
        }
    }
    // 端到端校验通过：未知版本写入 compat_ok 放行标记（仅本机沙箱）
    if !KNOWN_ILINK_VERSIONS.contains(&version.as_deref().unwrap_or("")) {
        let _ = std::fs::write(sandbox.join("compat_ok"), b"1");
    }
    Ok(bytes)
}
