// ============================================================
// 消息原图官方通道回退（ilink2.dll C2C 原图下载）
// 原则：仅在现有解密/CDN 解析失败时回退调用微信官方 ilink 通道；
//   1. 从解密消息库提取图片 XML（message_content 为 zstd 压缩）
//   2. 调用打包的 wechat-cdn-poc 下载器（复用 E:\wechat_image 已验证链路）
//   3. 版本护栏：仅在已知兼容微信版本或历史端到端校验通过后放行
//   4. 隔离沙箱：复制真实 cloud_account.txt / kvcomm 会话，不动真实数据
// ============================================================

mod download;
pub use download::*;
mod extract;
pub(crate) use extract::{extract_image_xml, parse_origin_secret};
mod paths;
pub use paths::wechat_install_dir;
pub(crate) use paths::{
    ilink_compatible, origin_bridge_path, origin_exe_path, sandbox_dir, KNOWN_ILINK_VERSIONS,
};
mod sandbox;
pub(crate) use sandbox::ensure_sandbox;
mod types;
pub use types::*;

#[cfg(test)]
mod tests;
