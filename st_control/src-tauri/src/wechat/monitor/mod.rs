//! 微信实时消息监听器
//!
//! 核心架构:
//! 1. 30ms 轮询 session.db-wal 的 mtime 变化
//! 2. 检测到变化后：全量解密 DB + WAL patch
//! 3. 对比前后 SessionTable 状态，提取新消息
//! 4. 通过 Tauri Event 推送到前端

use crate::wechat::db_cache::MonitorDBCache;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

mod util;

mod query;

mod check;
mod start;
pub use start::{start_monitor, MonitorStartCtx};
pub(crate) use util::*;

// ============ 数据结构 ============

/// 推送到前端的微信消息
#[derive(Debug, Clone, Serialize)]
pub struct WeChatMessage {
    pub time: String,
    pub timestamp: i64, // 微秒
    pub local_id: Option<i64>,
    /// 排序序号（≈毫秒时间戳）：跨分库时 local_id 可能重复，
    /// 前端/去重以 (local_id, sort_seq) 为准
    pub sort_seq: Option<i64>,
    pub session_type: String, // "group" | "private"
    pub chat: String,
    pub username: String,
    pub is_group: bool,
    pub sender: String,
    pub sender_username: String,
    /// 是否本人发送（实时方向判断：私聊与群聊均适用）
    pub is_send: bool,
    pub msg_type: i32,
    pub content: String,
    pub media_type: Option<String>,
    pub decrypt_ms: f64,
    pub pages: u32,
    /// 解密后的图片 URL (仅图片消息)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    /// 富媒体解析结果 (表情/链接/文件等)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rich: Option<serde_json::Value>,
}

/// 会话状态条目 (来自 SessionTable)
#[derive(Debug, Clone)]
pub(crate) struct SessionEntry {
    unread: i32,
    summary: String,
    timestamp: i64,
    msg_type: i32,
    sender: String,
    sender_name: String,
}

/// 联系人映射
type ContactMap = HashMap<String, String>;

// ============ 会话监控器 ============

/// 会话监控器核心
pub struct SessionMonitor {
    enc_key: Vec<u8>,
    session_db: PathBuf,
    decrypted_session: PathBuf,
    db_cache: Arc<MonitorDBCache>,
    contact_names: Arc<tokio::sync::RwLock<ContactMap>>,
    username_db_map: Arc<tokio::sync::RwLock<HashMap<String, Vec<String>>>>,
    prev_state: tokio::sync::RwLock<HashMap<String, SessionEntry>>,
    /// (username, local_id, sort_seq) 三元组：跨分库时 local_id 可能重复
    shown_keys: tokio::sync::RwLock<HashSet<(String, i64, i64)>>,
    decrypt_ms: std::sync::atomic::AtomicU64,
    patched_pages: std::sync::atomic::AtomicU32,
    /// 上次成功刷新时 session.db 的 mtime（毫秒），用于变化门控
    last_session_db_ms: std::sync::atomic::AtomicU64,
    /// 上次成功刷新时 session.db-wal 的 mtime（毫秒）
    last_session_wal_ms: std::sync::atomic::AtomicU64,
    /// 上次检测到的 message/biz_message 分库签名（mtime 之和）。
    /// 微信可能先写消息分库、稍后才更新 session 表；单独门控消息分库
    /// 变化可在 session 更新滞后时立即触发一次检测，避免等 10s 水位线兜底。
    last_msg_sig: std::sync::atomic::AtomicU64,
    /// 本机 wxid，用于实时消息的 is_send（本人发送）判断
    self_username: String,
    db_dir: PathBuf,
    /// 图片解析器 (可选，当有 AES key 时启用)
    /// 注意：图片解码（尤其 HEVC→JPEG 转码）耗时 50~300ms+，
    /// 若内联到推送热路径会阻塞监控任务、拖垮所有消息延迟。
    /// 因此热路径不再内联解码，改由前端懒加载（get_message_image IPC）。
    /// 该解析器保留以备后续按需/后台解码使用。
    #[allow(dead_code)]
    image_resolver: Option<crate::wechat::image::ImageResolver>,
    /// 会话级消息水位线，用于增量同步与漏消息兜底
    watermark_store: Arc<crate::wechat::watermark::WatermarkStore>,
}

impl SessionMonitor {
    /// 创建新的会话监控器
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        enc_key: Vec<u8>,
        session_db: PathBuf,
        decrypted_session: PathBuf,
        db_cache: Arc<MonitorDBCache>,
        contact_names: ContactMap,
        username_db_map: HashMap<String, Vec<String>>,
        db_dir: PathBuf,
        self_username: String,
        image_resolver: Option<crate::wechat::image::ImageResolver>,
    ) -> Self {
        let watermark_path = decrypted_session
            .parent()
            .map(|p| p.join("watermarks.json"));
        Self {
            enc_key,
            session_db,
            decrypted_session,
            db_cache,
            contact_names: Arc::new(tokio::sync::RwLock::new(contact_names)),
            username_db_map: Arc::new(tokio::sync::RwLock::new(username_db_map)),
            prev_state: tokio::sync::RwLock::new(HashMap::new()),
            shown_keys: tokio::sync::RwLock::new(HashSet::new()),
            decrypt_ms: std::sync::atomic::AtomicU64::new(0),
            patched_pages: std::sync::atomic::AtomicU32::new(0),
            last_session_db_ms: std::sync::atomic::AtomicU64::new(0),
            last_session_wal_ms: std::sync::atomic::AtomicU64::new(0),
            last_msg_sig: std::sync::atomic::AtomicU64::new(0),
            self_username,
            db_dir,
            image_resolver,
            watermark_store: Arc::new(crate::wechat::watermark::WatermarkStore::new(
                watermark_path,
            )),
        }
    }

    /// 读取 session.db / WAL 的当前 mtime（毫秒）
    fn session_file_state(&self) -> (u64, u64) {
        (
            file_mtime_ms(&self.session_db),
            file_mtime_ms(&self.session_db.with_extension("db-wal")),
        )
    }

    /// 廉价检测 session.db / WAL 是否发生变化（仅 2 次 stat 调用）。
    /// 返回 (db_changed, wal_changed)。首次调用（快照为 0）视为已变化。
    fn session_file_changed(&self) -> (bool, bool) {
        let (db_ms, wal_ms) = self.session_file_state();
        let last_db = self
            .last_session_db_ms
            .load(std::sync::atomic::Ordering::Relaxed);
        let last_wal = self
            .last_session_wal_ms
            .load(std::sync::atomic::Ordering::Relaxed);
        (db_ms != last_db, wal_ms != last_wal)
    }

    /// 记录刷新时捕获的 mtime 快照。
    /// 注意：必须记录"解密前"捕获的值。若在解密完成后才读取，
    /// 微信在解密期间的新写入会让快照与内容不一致，导致该消息被漏检。
    fn mark_session_refreshed(&self, db_ms: u64, wal_ms: u64) {
        self.last_session_db_ms
            .store(db_ms, std::sync::atomic::Ordering::Relaxed);
        self.last_session_wal_ms
            .store(wal_ms, std::sync::atomic::Ordering::Relaxed);
    }

    /// message/biz_message 分库签名：所有分库（含 WAL）mtime 之和。
    /// 仅 stat 元数据，每轮询周期开销极小。
    fn msg_dbs_sig(&self) -> u64 {
        let mut sig: u64 = 0;
        let mut files = crate::wechat::modules::common::find_db_files(&self.db_dir, "message_");
        files.extend(crate::wechat::modules::common::find_db_files(
            &self.db_dir,
            "biz_message_",
        ));
        for path in files {
            sig = sig.wrapping_add(file_mtime_ms(&path));
            sig = sig.wrapping_add(file_mtime_ms(&path.with_extension("db-wal")));
        }
        sig
    }

    /// 消息分库是否有变化（首次调用视为变化，用于播种基线）
    fn message_dbs_changed(&self) -> bool {
        let sig = self.msg_dbs_sig();
        let last = self.last_msg_sig.load(std::sync::atomic::Ordering::Relaxed);
        if sig != last {
            self.last_msg_sig
                .store(sig, std::sync::atomic::Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}
