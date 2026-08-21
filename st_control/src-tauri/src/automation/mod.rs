// ============================================================
// 自动化管理中心 — 模块入口
// 概览 / 规则管理 / 消息与任务 / 回复机器人
// ============================================================

pub mod db;
pub mod engine;
pub mod handlers;
pub mod sse;
pub mod worker;

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;

/// 自动化模块状态（Tauri 托管）
pub struct AutomationState {
    conn: Mutex<Connection>,
    pub sse_running: Mutex<bool>,
    pub sse_connected: AtomicBool,
    pub sse_received: AtomicU64,
    pub sse_last_at: Mutex<Option<String>>,
    /// 当前 SSE 消费任务句柄：重连时先 abort 旧任务，避免双消费者
    sse_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    pub monitor: Option<Arc<crate::wechat::handlers::WeChatMonitorState>>,
}

impl AutomationState {
    /// 打开 control.db（与主库同文件）并建表
    pub fn new(
        db_path: &Path,
        monitor: Option<Arc<crate::wechat::handlers::WeChatMonitorState>>,
    ) -> Result<Self, String> {
        let conn = Connection::open(db_path).map_err(|e| format!("打开数据库失败: {e}"))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(|e| format!("设置 PRAGMA 失败: {e}"))?;
        db::init_tables(&conn).map_err(|e| format!("初始化自动化表失败: {e}"))?;
        Ok(AutomationState {
            conn: Mutex::new(conn),
            sse_running: Mutex::new(false),
            sse_connected: AtomicBool::new(false),
            sse_received: AtomicU64::new(0),
            sse_last_at: Mutex::new(None),
            sse_task: Mutex::new(None),
            monitor,
        })
    }

    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// 当前监控 router 指针（用于检测 router 替换）
    pub fn router_ptr(&self) -> Option<usize> {
        self.monitor
            .as_ref()
            .and_then(|m| m.router())
            .map(|r| Arc::as_ptr(&r) as usize)
    }

    /// 启动 SSE 消费线程（幂等）：仅当没有运行中的消费者时启动
    pub fn ensure_sse(&self, app: AppHandle, url: String) {
        let mut running = self.sse_running.lock().unwrap_or_else(|p| p.into_inner());
        if *running {
            return;
        }
        *running = true;
        self.sse_connected.store(false, Ordering::Relaxed);
        drop(running);
        let handle = tauri::async_runtime::spawn(async move {
            // 延迟启动，等待同进程的 5032 HTTP 服务完成监听，避免启动竞态
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            sse::run_consumer(app, url).await;
        });
        *self.sse_task.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle);
    }

    /// 重启 SSE 消费：先取消旧消费任务（避免旧 loop 与新 loop 双消费者），
    /// 再幂等启动新任务
    pub fn restart_sse(&self, app: AppHandle, url: String) {
        if let Some(handle) = self
            .sse_task
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            handle.abort();
        }
        self.sse_connected.store(false, Ordering::Relaxed);
        *self.sse_running.lock().unwrap_or_else(|p| p.into_inner()) = false;
        self.ensure_sse(app, url);
    }

    pub fn mark_connected(&self) {
        self.sse_connected.store(true, Ordering::Relaxed);
    }
    pub fn mark_disconnected(&self) {
        self.sse_connected.store(false, Ordering::Relaxed);
    }
    pub fn mark_received(&self) {
        self.sse_received.fetch_add(1, Ordering::Relaxed);
        *self.sse_last_at.lock().unwrap_or_else(|p| p.into_inner()) =
            Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
    }
}

/// control.db 路径（自动化任务接口复用）
pub fn control_db_path() -> PathBuf {
    crate::common::st_data_dir().join("control.db")
}
