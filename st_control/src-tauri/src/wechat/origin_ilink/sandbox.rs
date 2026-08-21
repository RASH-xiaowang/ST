// ============================================================
// 消息原图官方通道回退 — 隔离沙箱域
// 自 origin_ilink.rs 拆分：启动配置构建与会话复制。
// ============================================================

use std::path::{Path, PathBuf};

use super::sandbox_dir;

/// 重建 ilink 启动配置（字段 1=data_root，6=client_version；与 PoC build_ilink_start_config 一致）
fn build_start_config_bytes(data_root: &str, client_version: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(data_root.len() + 16);
    out.push(0x0a);
    encode_varint(data_root.len() as u64, &mut out);
    out.extend_from_slice(data_root.as_bytes());
    out.extend_from_slice(&[0x10, 0x00]); // 字段 2：微信置零
    out.push(0x30); // 字段 6：桌面客户端版本
    encode_varint(u64::from(client_version), &mut out);
    out
}

fn encode_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn read_kv_client_version(config_ini: &Path) -> Option<u32> {
    let content = std::fs::read_to_string(config_ini).ok()?;
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix("kv_clientversion="))
        .and_then(|v| v.parse().ok())
}

/// 准备隔离沙箱：目录 + 复制真实会话（cloud_account.txt / kvcomm / CDN 路由）+ 启动配置
pub(crate) fn ensure_sandbox() -> Result<PathBuf, String> {
    let sandbox = sandbox_dir();
    let app_data = dirs::data_dir().ok_or("无法获取 AppData 目录")?;
    let real_ilink = app_data.join("Tencent").join("xwechat").join("ilink");
    for sub in ["wechat", "kvcomm", "netbridge/cdn"] {
        std::fs::create_dir_all(sandbox.join(sub)).map_err(|e| format!("创建隔离目录失败: {e}"))?;
    }

    // cloud_account.txt：真实登录态复制进沙箱（PoC 读取 kTdiKeyCloudSession）
    let real_acct = real_ilink.join("wechat").join("cloud_account.txt");
    let iso_acct = sandbox.join("wechat").join("cloud_account.txt");
    if !iso_acct.is_file() {
        if !real_acct.is_file() {
            return Err("本机微信未登录或缺少 ilink 会话（cloud_account.txt）".to_string());
        }
        std::fs::copy(&real_acct, &iso_acct)
            .map_err(|e| format!("复制微信会话到隔离沙箱失败: {e}"))?;
    }

    // kvcomm 配置（clientversion / uin）
    for f in ["config.ini", "new_strategy_file_kv"] {
        let src = real_ilink.join("kvcomm").join(f);
        let dst = sandbox.join("kvcomm").join(f);
        if !dst.is_file() && src.is_file() {
            let _ = std::fs::copy(&src, &dst);
        }
    }

    // CDN 路由缓存：新版下载器必需（netbridge/cdn），缺了会直接拒绝启动
    let cdn_source = app_data
        .join("Tencent")
        .join("xwechat")
        .join("net")
        .join("cdncomm");
    for f in ["cdninfo_new.cache", "cdnmisc.cfg"] {
        let src = cdn_source.join(f);
        let dst = sandbox.join("netbridge").join("cdn").join(f);
        if !dst.is_file() && src.is_file() {
            std::fs::copy(&src, &dst)
                .map_err(|e| format!("复制微信 CDN 路由缓存到沙箱失败: {e}"))?;
        }
    }

    // ilink 启动配置
    let cfg_path = sandbox.join("ilink-start-config.bin");
    if !cfg_path.is_file() {
        let cv = read_kv_client_version(&real_ilink.join("kvcomm").join("config.ini"))
            .unwrap_or(4065598490);
        let root = sandbox.to_string_lossy().replace('/', "\\");
        std::fs::write(&cfg_path, build_start_config_bytes(&root, cv))
            .map_err(|e| format!("写 ilink 启动配置失败: {e}"))?;
    }
    Ok(sandbox)
}
