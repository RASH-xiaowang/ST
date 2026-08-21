// ============================================================
// 微信 IPC — 监控生命周期管理
// ============================================================

use std::sync::Arc;

use crate::wechat::db_cache::MonitorDBCache;
use crate::wechat::router::EventRouter;
use tauri::Emitter;

/// 微信消息监控状态
pub struct WeChatMonitorState {
    cancel_tx: std::sync::Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    forward_handle: std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// 监控主循环任务句柄（tokio JoinHandle，支持 is_finished 存活检测），
    /// 用于监管任务存活状态、停止时等待退出
    monitor_handle: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    router: std::sync::Mutex<Option<Arc<EventRouter>>>,
    /// 实时解密缓存。图片解析（HTTP API / IPC）优先使用它读取最新
    /// `message_resource.db`，避免静态解密副本过期导致新消息图片 NOT_FOUND。
    db_cache: std::sync::Mutex<Option<Arc<MonitorDBCache>>>,
}

/// 获取互斥锁；锁中毒（持锁线程 panic）时恢复数据而非二次 panic。
/// 原实现多处 `.unwrap()`，一旦中毒 start/stop/status 等 IPC 命令会永久崩溃。
fn lock_recover<'a, T>(m: &'a std::sync::Mutex<T>) -> std::sync::MutexGuard<'a, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Default for WeChatMonitorState {
    fn default() -> Self {
        Self {
            cancel_tx: std::sync::Mutex::new(None),
            forward_handle: std::sync::Mutex::new(None),
            monitor_handle: std::sync::Mutex::new(None),
            router: std::sync::Mutex::new(None),
            db_cache: std::sync::Mutex::new(None),
        }
    }
}

impl WeChatMonitorState {
    /// 等价于 `Self::default()`；保留显式构造入口便于语义清晰
    pub fn new() -> Self {
        Self::default()
    }

    /// 真实的运行状态：取消信号存在且监控主循环任务仍存活。
    /// 原实现只看 cancel_tx，监控任务 panic/退出后仍误报 running。
    pub fn is_running(&self) -> bool {
        if lock_recover(&self.cancel_tx).is_none() {
            return false;
        }
        match lock_recover(&self.monitor_handle).as_ref() {
            Some(h) => !h.is_finished(),
            None => false,
        }
    }

    /// 获取事件路由器（HTTP API 状态查询 / SSE 订阅用）
    pub fn router(&self) -> Option<std::sync::Arc<EventRouter>> {
        lock_recover(&self.router).as_ref().cloned()
    }

    /// 获取实时解密缓存（图片解析优先使用，确保读取最新 message_resource.db）
    pub fn db_cache(&self) -> Option<Arc<MonitorDBCache>> {
        lock_recover(&self.db_cache).as_ref().cloned()
    }

    pub async fn stop(&self) {
        let tx = lock_recover(&self.cancel_tx).take();
        if let Some(tx) = tx {
            let _ = tx.send(true);
        }
        // 等待监控主循环真正退出，避免旧循环与新启动的监控并发写同一解密文件
        let monitor = lock_recover(&self.monitor_handle).take();
        if let Some(h) = monitor {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), h).await;
        }
        let forward = lock_recover(&self.forward_handle).take();
        if let Some(h) = forward {
            let _ = h.await;
        }
        if let Some(router) = lock_recover(&self.router).take() {
            router.stop_ws_server();
        }
        log::info!("[wechat] 监控已停止");
    }

    pub async fn start(&self, app: tauri::AppHandle) -> Result<(), String> {
        self.stop().await;
        let cfg = crate::wechat::config::WeChatConfig::load()
            .map_err(|e| format!("加载配置失败: {}", e))?;
        if !cfg.has_keys() {
            return Err("all_keys.json 不存在，无法启动监控".to_string());
        }
        let keys = std::sync::Arc::new(
            crate::wechat::keys::Keys::from_file(&cfg.keys_file)
                .map_err(|e| format!("读取密钥文件失败: {}", e))?,
        );
        log::info!("[wechat] 已加载 {} 个数据库密钥", keys.len());
        let session_key = keys
            .get_key_info("session/session.db")
            .ok_or("密钥文件缺少 session.db".to_string())?;
        let session_db = cfg.db_dir.join("session").join("session.db");
        // 监控器解密后的 session.db 直接写入 decrypted_dir/session/session.db，
        // 与 get_session_list() / get_session_snapshots() 读取同一文件，
        // 确保手动刷新和实时推送都能展示最新会话状态。
        // 之前写入 monitor_cache/session/session.db 导致会话列表始终展示旧数据。
        let decrypted_session = cfg.decrypted_dir.join("session").join("session.db");
        let enc_key = if keys.key_format.as_deref() == Some("wx_key_v4.1") {
            use std::io::Read;
            let mut f = std::fs::File::open(&session_db)
                .map_err(|e| format!("打开 session.db 失败: {}", e))?;
            let mut salt = vec![0u8; crate::wechat::crypto::SALT_SZ];
            f.read_exact(&mut salt)
                .map_err(|e| format!("读取 salt 失败: {}", e))?;
            crate::wechat::crypto::derive_enc_key(
                &hex::decode(&session_key.enc_key).map_err(|e| format!("hex 解码失败: {}", e))?,
                &salt,
                keys.key_format.as_deref(),
            )
        } else {
            hex::decode(&session_key.enc_key).map_err(|e| format!("hex 解码失败: {}", e))?
        };
        let db_cache = std::sync::Arc::new(
            crate::wechat::db_cache::MonitorDBCache::new(
                keys.clone(),
                cfg.db_dir.clone(),
                cfg.decrypted_dir.clone(),
            )
            .with_preserved_structure(),
        );
        *lock_recover(&self.db_cache) = Some(db_cache.clone());
        let contact_path = db_cache.get("contact/contact.db").ok().flatten();
        let contact_names = match &contact_path {
            Some(p) if p.exists() => crate::wechat::monitor::load_contact_names(p),
            _ => {
                log::warn!("[wechat] 无法解密 contact.db");
                std::collections::HashMap::new()
            }
        };
        log::info!("[wechat] 已加载 {} 个联系人", contact_names.len());
        let cache_for_map = db_cache.clone();
        let db_dir_for_map = cfg.db_dir.clone();
        let username_db_map = tokio::task::spawn_blocking(move || {
            crate::wechat::monitor::build_username_db_map(&cache_for_map, &db_dir_for_map)
        })
        .await
        .map_err(|e| format!("构建 username 映射失败: {}", e))?;
        log::info!("[wechat] 已映射 {} 个用户名", username_db_map.len());
        let (wechat_tx, _) = tokio::sync::broadcast::channel::<String>(8192);
        let router = EventRouter::new(app.clone(), wechat_tx);
        *lock_recover(&self.router) = Some(router.clone());
        let image_aes_key_bytes: Option<Vec<u8>> = cfg
            .image_aes_key
            .as_ref()
            .filter(|k| k.len() == 16)
            .map(|k| k.as_bytes().to_vec());
        log::info!(
            "[wechat] 数据库密钥: {} bytes, 图片AES: {:?}, XOR: 0x{:02X}",
            enc_key.len(),
            image_aes_key_bytes
                .as_ref()
                .map(|k| format!("{}B", k.len())),
            cfg.image_xor_key,
        );
        // 启动 WebSocket 回退服务器；主通道仍为 Tauri Event
        let router_for_ws = router.clone();
        let handle = tauri::async_runtime::spawn(async move {
            match router_for_ws.start_ws_server().await {
                Ok(port) => log::info!("[wechat] WebSocket 回退服务器已启动，端口 {}", port),
                Err(e) => log::warn!("[wechat] WebSocket 回退服务器启动失败: {}", e),
            }
        });
        *lock_recover(&self.forward_handle) = Some(handle);
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        *lock_recover(&self.cancel_tx) = Some(cancel_tx);
        // start_monitor 返回主循环 JoinHandle，持有它用于：
        //   1. is_running() 检测任务异常退出（panic/错误）
        //   2. stop() 时等待旧循环真正退出，避免与新旧任务并发写解密文件
        let monitor_handle = crate::wechat::monitor::start_monitor(
            crate::wechat::monitor::MonitorStartCtx {
                enc_key,
                session_db,
                decrypted_session,
                db_cache,
                db_dir: cfg.db_dir.clone(),
                contact_names,
                username_db_map,
                self_username: cfg.wxid().unwrap_or_default(),
                router,
                wechat_base_dir: cfg.wechat_base_dir.clone(),
                decoded_image_dir: cfg.decoded_image_dir.clone(),
                image_aes_key: image_aes_key_bytes,
                image_xor_key: cfg.image_xor_key,
            },
            cancel_rx,
        )
        .await;
        *lock_recover(&self.monitor_handle) = Some(monitor_handle);
        log::info!("[wechat] 监控已启动");
        Ok(())
    }
}

#[tauri::command]
pub async fn start_wechat_monitor(
    app: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Arc<WeChatMonitorState>>,
) -> Result<(), String> {
    state.start(app.clone()).await?;
    // 发送 JSON 对象（与前端 MonitorStatus 类型一致；原来发送字符串导致前端解析失败）
    let _ = app.emit(
        "wechat-status",
        serde_json::json!({"running":true,"status":"started"}),
    );
    Ok(())
}

#[tauri::command]
pub async fn stop_wechat_monitor(
    app: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Arc<WeChatMonitorState>>,
) -> Result<(), String> {
    state.stop().await;
    let _ = app.emit(
        "wechat-status",
        serde_json::json!({"running":false,"status":"stopped"}),
    );
    Ok(())
}

#[tauri::command]
pub async fn get_wechat_monitor_status(
    state: tauri::State<'_, std::sync::Arc<WeChatMonitorState>>,
) -> Result<serde_json::Value, String> {
    let running = state.is_running();
    let router_opt = lock_recover(&state.router).as_ref().cloned();
    let ws_port = router_opt.as_ref().map(|r| r.ws_port()).unwrap_or(0);
    let (pending_acks, sent_total, sent_batch_count, sent_ws_count, latency) =
        if let Some(router) = router_opt {
            let metrics = router.metrics().await;
            (
                metrics.pending_acks,
                metrics.sent_total,
                metrics.sent_batch_count,
                metrics.sent_ws_count,
                serde_json::json!({
                    "buckets": metrics.latency_buckets,
                    "sum_ms": metrics.latency_ms_sum,
                    "count": metrics.latency_ms_count,
                }),
            )
        } else {
            (
                0usize,
                0u64,
                0u64,
                0u64,
                serde_json::json!({"buckets":[0,0,0,0,0],"sum_ms":0,"count":0}),
            )
        };
    Ok(serde_json::json!({
        "running": running,
        "status": if running { "running" } else { "stopped" },
        "ws_port": ws_port,
        "pending_acks": pending_acks,
        "sent_total": sent_total,
        "sent_batch_count": sent_batch_count,
        "sent_ws_count": sent_ws_count,
        "latency": latency,
    }))
}

#[tauri::command]
pub async fn ack_wechat_message(
    ack_id: String,
    state: tauri::State<'_, std::sync::Arc<WeChatMonitorState>>,
) -> Result<(), String> {
    let id = ack_id
        .parse::<u64>()
        .map_err(|e| format!("非法 ack_id: {}", e))?;
    let router_opt = lock_recover(&state.router).as_ref().cloned();
    if let Some(router) = router_opt {
        router.ack(id).await;
    }
    Ok(())
}

/// 断线重连/页面恢复后的消息补推
///
/// 前端传入本地已见的最大 ack_id，后端返回缓冲区中 ack_id 更大的
/// 全部消息文本（JSON 字符串），前端逐条走正常解析流程（含去重）。
#[tauri::command]
pub async fn resync_wechat_messages(
    since_ack_id: Option<String>,
    state: tauri::State<'_, std::sync::Arc<WeChatMonitorState>>,
) -> Result<Vec<String>, String> {
    let since = since_ack_id
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let router_opt = lock_recover(&state.router).as_ref().cloned();
    let texts = match router_opt {
        Some(router) => router.replay_since(since).await,
        None => Vec::new(),
    };
    if !texts.is_empty() {
        log::info!(
            "[wechat] 补推 {} 条缓冲消息 (since ack_id={})",
            texts.len(),
            since
        );
    }
    Ok(texts)
}
