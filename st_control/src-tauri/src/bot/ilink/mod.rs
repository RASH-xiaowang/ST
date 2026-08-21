// ============================================================
// 微信 iLink（ClawBot）协议实现
// types：协议类型（serde 容错）
// crypto：AES-128-ECB + PKCS7、aes_key 三格式解析
// client：HTTP 客户端（统一请求头 / 长轮询）
// auth：二维码登录
// cdn：媒体下载解密 / 加密上传
// sender：getconfig → sendtyping → sendmessage（文本+媒体）
// poller：getupdates 长轮询解析
// ============================================================

pub mod auth;
pub mod cdn;
pub mod client;
pub mod crypto;
pub mod poller;
pub mod sender;
pub mod types;
