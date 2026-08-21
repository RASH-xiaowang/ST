//! 微信 CDN 原图下载（纯网络，非 hook）
//!
//! 参考 WeChatDataAnalysis 的 `cdn_image_service`：
//! 1. 读取账号目录下微信内部文件 `all_users/config/global_config` + `.crc`，
//!    `POST https://view.free.c3o.re/api/token` 换取 Bearer token（按账号缓存 45 分钟）
//! 2. `GET https://wxcdn.c3o.re/download?fileid=<cdnbigimgurl>&type=orig`
//!    （不带 key）返回 CDN 原始加密字节；默认在本地用消息 XML 的 `aeskey`
//!    做 AES-128/192/256-ECB 解密（aeskey 不出本机）。
//!    可切换为 `type=orig&key=<aeskey>` 由 CDN 端解密。
//! 3. fileid/aeskey 从图片消息 XML（`cdnbigimgurl` / `aeskey` 属性）解析
//!
//! 仅当本地找不到原图且用户未关闭「自动获取原图(CDN)」时调用；
//! 客户端不设下载上限，下载结果按 fileid 缓存到解码目录。

mod download;
pub use download::download_original_image;
mod fallback;
pub use fallback::{resolve_wxid_dir, try_cdn_fallback};
mod settings;
pub use settings::*;
mod token;
pub use token::fetch_cdn_token;
mod xml;
pub(crate) use xml::extract_xml_value;
pub use xml::{lookup_image_cdn_info, lookup_image_md5_variants};
