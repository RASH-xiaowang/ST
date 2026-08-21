// ============================================================
// 消息通道 — 多账号生命周期管理
// 每个账号一个长轮询 task；扫码绑定（长期有效，服务端会话失效时
// 自动下线）/ 解绑 / 一键重扫 / 桌面事件推送 / 待回复任务应答器
// ============================================================

use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

mod qr;

mod channel;
use super::db;

mod send;

mod account;

mod r#loop;
use super::secret::TokenCipher;

mod contacts;

mod utils;
pub(crate) use utils::*;

/// 账号运行时状态
pub struct AccountRuntime {
    pub cancel: CancellationToken,
    pub sync_buf: Mutex<String>,
    pub context_tokens: Mutex<HashMap<String, String>>,
    pub status: Mutex<String>,
    pub last_error: Mutex<String>,
    pub expiring_notified: AtomicBool,
}

/// 二维码会话记录
pub(crate) struct QrRecord {
    qrcode: String,
    account_id: Option<i64>,
    created_at: Instant,
}

pub struct BotManager {
    app: Mutex<Option<AppHandle>>,
    data_dir: PathBuf,
    db_path: PathBuf,
    cipher: TokenCipher,
    accounts: RwLock<HashMap<i64, Arc<AccountRuntime>>>,
    qr_sessions: RwLock<HashMap<String, QrRecord>>,
    responder_running: AtomicBool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrView {
    pub session_id: String,
    pub image_data_url: String,
    pub raw_url: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountContact {
    pub peer: String,
    pub last_text: String,
    pub last_ts: i64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BotStatusSummary {
    pub total: i64,
    pub online: i64,
    pub expired: i64,
    pub error: i64,
}

impl BotManager {
    pub fn new(data_dir: &Path, db_path: &Path) -> Result<Self, String> {
        let cipher = TokenCipher::load(data_dir)?;
        let conn = Connection::open(db_path).map_err(|e| format!("打开数据库失败: {e}"))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(|e| format!("设置 PRAGMA 失败: {e}"))?;
        db::init_tables(&conn).map_err(|e| format!("初始化 bot 表失败: {e}"))?;
        db::migrate(&conn);
        let media_dir = data_dir.join("bot_media");
        std::fs::create_dir_all(&media_dir).ok();
        Ok(Self {
            app: Mutex::new(None),
            data_dir: data_dir.to_path_buf(),
            db_path: db_path.to_path_buf(),
            cipher,
            accounts: RwLock::new(HashMap::new()),
            qr_sessions: RwLock::new(HashMap::new()),
            responder_running: AtomicBool::new(false),
        })
    }

    pub fn attach_app(&self, app: AppHandle) {
        *self.app.lock().unwrap_or_else(|p| p.into_inner()) = Some(app);
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub(crate) fn conn(&self) -> Result<Connection, String> {
        let c = Connection::open(&self.db_path).map_err(|e| format!("打开数据库失败: {e}"))?;
        c.execute_batch("PRAGMA busy_timeout=5000;").ok();
        Ok(c)
    }

    pub(crate) fn emit(&self, event: &str, payload: &impl Serialize) {
        if let Some(app) = self.app.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            let _ = app.emit(event, payload);
        }
    }
}

#[cfg(test)]
mod tests;
