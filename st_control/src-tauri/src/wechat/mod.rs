// ============================================================
// 微信数据管理模块 — 模块入口
// 所有微信相关的子模块统一在此处声明并导出
// ============================================================

pub mod annual;
pub mod archive;
pub mod ask;
pub mod auto_key;
pub mod backup;
pub mod cdn_image;
pub mod chat_search_index;
pub mod config;
pub mod crypto;
pub mod daily_summary;
pub mod db_cache;
pub mod edit_store;
pub mod file;
pub mod general_records;
pub mod graph_export;
pub mod handlers;
#[cfg(target_os = "windows")]
pub mod hevc;
pub mod hook;
pub mod http_api; // IPC handlers（从 ipc_handlers 中独立出来）
pub mod image;
pub mod import_backup;
pub mod insights;
pub mod keys;
pub mod listener;
pub mod media;
pub mod missing_images;
pub mod modules;
pub mod monitor;
pub mod origin_ilink;
pub mod privacy;
pub mod router;
pub mod sns_image;
pub mod voice;
pub mod watermark;
