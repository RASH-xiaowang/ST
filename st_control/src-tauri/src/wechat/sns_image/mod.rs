//! 朋友圈图片解密模块（CDN 下载 + ISAAC-64 XOR 解密）
//!
//! 微信朋友圈图片的 CDN 直链（`*.qpic.cn`）返回的是经 ISAAC-64 密钥流
//! XOR 加密的数据（响应头 `X-Enc: 1`），浏览器直接展示必然失败。
//!
//! 解密流程（与 WeFlow 等逆向实现一致，已在真实数据上验证）：
//!   1. 用 XML 中该媒体自带的完整 token 拼接 URL：`.../150 -> .../0`，
//!      追加 `?token=<token>&idx=1`；
//!   2. 以 XML 的 `key`（十进制数字字符串）为种子初始化 ISAAC-64；
//!      golden 常数为 `0x9e3779b97f4a7c13`，初始化执行一轮 `isaac64()`，
//!      之后按 `randrsl[255] -> randrsl[0]` 降序取块、每块大端 8 字节；
//!   3. 密钥流与响应体逐字节 XOR，得到 JPEG/PNG。
//!
//! 所有结果落盘到 `decoded_image_dir/moments/<md5(url)>.<ext>` 磁盘缓存，
//! 命中后直接读文件，避免重复下载/解密。

mod image;
pub use image::resolve_moment_image_data_url;
mod isaac;
pub(crate) use isaac::Isaac64;
mod net;
pub(crate) use net::{data_url, diag_log, fetch_and_decrypt, normalize_cdn_url, sniff_image};
mod video;
pub use video::{moment_video_file_key, resolve_moment_video};

#[cfg(test)]
mod tests;
