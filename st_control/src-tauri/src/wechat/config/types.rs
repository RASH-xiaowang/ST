// ============================================================
// 微信配置 — 数据类型
// 自 config.rs 拆分：路径配置/原始 JSON/检测账号/密钥补丁。
// ============================================================

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 微信数据路径配置
#[derive(Debug, Clone, Serialize)]
pub struct WeChatConfig {
    /// 加密数据库目录 (如 `D:\xwechat_files\<wxid>\db_storage`)
    pub db_dir: PathBuf,

    /// 微信数据根目录 (db_dir 的父目录)
    pub wechat_base_dir: PathBuf,

    /// 解密后的数据库输出目录
    pub decrypted_dir: PathBuf,

    /// 解密后的图片输出目录
    pub decoded_image_dir: PathBuf,

    /// 实时监控缓存目录
    pub monitor_cache_dir: PathBuf,

    /// all_keys.json 文件路径
    pub keys_file: PathBuf,

    /// V2 图片 AES 密钥 (可选)
    pub image_aes_key: Option<String>,

    /// V2 图片 XOR 密钥
    pub image_xor_key: u8,

    /// 微信进程名称
    pub wechat_process: String,

    /// 数据库密钥格式
    pub key_format: Option<String>,

    /// 是否启用 HTTP API 服务（默认 true）
    pub api_enabled: bool,

    /// HTTP API 监听端口（默认 5032，仅 127.0.0.1）
    pub api_port: u16,

    /// HTTP API 访问令牌（None/空 = 免鉴权，仅建议本机使用）
    pub api_token: Option<String>,
}

/// config.json 的原始数据结构
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct RawConfig {
    #[serde(default)]
    pub(crate) db_dir: Option<String>,
    #[serde(default)]
    pub(crate) keys_file: Option<String>,
    #[serde(default)]
    pub(crate) decrypted_dir: Option<String>,
    #[serde(default)]
    pub(crate) decoded_image_dir: Option<String>,
    #[serde(default)]
    pub(crate) wechat_process: Option<String>,
    #[serde(default)]
    pub(crate) image_aes_key: Option<String>,
    #[serde(default)]
    pub(crate) image_xor_key: Option<u8>,
    #[serde(default)]
    pub(crate) key_format: Option<String>,
    #[serde(default)]
    pub(crate) db_enc_key: Option<String>,
    #[serde(default)]
    pub(crate) wechat_root: Option<String>,
    #[serde(default)]
    pub(crate) api_enabled: Option<bool>,
    #[serde(default)]
    pub(crate) api_port: Option<u16>,
    #[serde(default)]
    pub api_token: Option<String>,
}

/// 检测到的微信账号信息
#[derive(Debug, Clone, Serialize)]
pub struct DetectedAccount {
    /// wxid (目录名)
    pub wxid: String,
    /// db_storage 完整路径
    pub db_dir: String,
    /// 微信数据根目录
    pub base_dir: String,
    /// message 目录最后修改时间 (Unix 秒，0 表示不存在)
    pub last_active: u64,
}

/// 密钥自动获取时使用的配置补丁（只覆盖传入的字段，其余原样保留）
#[derive(Debug, Clone, Default)]
pub struct KeyConfigPatch<'a> {
    pub db_dir: Option<&'a str>,
    pub db_enc_key: Option<&'a str>,
    pub image_aes_key: Option<&'a str>,
    pub image_xor_key: Option<u8>,
}
