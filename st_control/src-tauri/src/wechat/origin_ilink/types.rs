// ============================================================
// 消息原图官方通道回退 — 数据类型
// 自 origin_ilink.rs 拆分：原图密钥与通道可用性快照。
// ============================================================

#[derive(Debug, Clone)]
pub struct OriginSecret {
    pub file_id: String,
    pub aes_key: String,
    pub md5: String,
    pub original_size: u64,
}

/// 官方通道可用性快照（配置页/提示用）
#[derive(Debug, serde::Serialize)]
pub struct IlinkStatus {
    pub enabled: bool,
    pub wechat_version: Option<String>,
    pub wrapper: Option<String>,
    pub sandbox_ready: bool,
    pub downloader: Option<String>,
    pub reason: Option<String>,
}
