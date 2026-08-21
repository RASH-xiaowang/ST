//! 微信数据管理 - 功能模块集合
//!
//! 每个功能模块独立封装，分别对应 PC 微信客户端的一个界面：
//!
//! - `sessions`  会话列表（微信主界面左侧）
//! - `messages`  聊天消息窗口（与某会话的聊天记录）
//! - `contacts`  通讯录（联系人 / 群聊 / 公众号）
//! - `moments`   朋友圈
//! - `favorites` 收藏
//! - `emoticons` 表情
//! - `official`  公众号 / 商家客服消息
//! - `files`     文件管理（图片 / 视频 / 文件硬链接库）
//! - `settings`  通用数据（好友验证 / 转账 / 红包 / 撤回等）
//! - `avatar`    用户头像
//!
//! 所有模块只以**只读方式**访问解密后的数据库副本，
//! 绝不对微信原始数据库或解密副本做任何写入，确保数据安全。

pub mod avatar;
pub mod common;
pub mod contacts;
pub mod emoticons;
pub mod favorites;
pub mod files;
pub mod messages;
pub mod moments;
pub mod official;
pub mod sessions;
pub mod settings;
