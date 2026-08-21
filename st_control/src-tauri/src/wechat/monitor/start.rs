//! 微信实时消息监听 — 监控线程启动器
//! 自 monitor.rs 拆分：启动参数（MonitorStartCtx）与事件驱动主循环
//! （HybridListener 文件监听 + 5s 轮询 + 30s 水位线 + 背压保护 + 推送）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::ContactMap;
use super::SessionMonitor;
use crate::wechat::db_cache::MonitorDBCache;

// ============ 监控线程 ============

/// 启动监控线程
///
/// 核心流程（事件驱动）:
/// 1. 通过 HybridListener 监听文件事件（notify）+ 5s 轮询 + 30s 水位线校验
/// 2. do_full_refresh() — 解密 session.db + WAL patch，确保数据最新
/// 3. check_updates() — 对比前后 SessionTable 状态，提取新消息
/// 4. 通过 EventRouter 推送消息（Tauri Event 主通道 + WebSocket 回退）
///
/// `cancel_rx` 为取消信号: 收到 `true` 时优雅退出轮询循环。
///
/// 返回监控主循环的 `JoinHandle`，调用方必须持有并监管：
/// 任务因 panic/异常退出时可通过 `is_finished()` 检测并重启，
/// 避免监控静默死亡而状态查询仍报告 running。
/// 监控启动参数（连接上下文 + 图片解密参数；cancel_rx 消费型 receiver 单独传入）
pub struct MonitorStartCtx {
    pub enc_key: Vec<u8>,
    pub session_db: PathBuf,
    pub decrypted_session: PathBuf,
    pub db_cache: Arc<MonitorDBCache>,
    pub db_dir: PathBuf,
    pub contact_names: ContactMap,
    pub username_db_map: HashMap<String, Vec<String>>,
    pub self_username: String,
    pub router: std::sync::Arc<crate::wechat::router::EventRouter>,
    pub wechat_base_dir: PathBuf,
    pub decoded_image_dir: PathBuf,
    pub image_aes_key: Option<Vec<u8>>,
    pub image_xor_key: u8,
}

pub async fn start_monitor(
    ctx: MonitorStartCtx,
    cancel_rx: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    let MonitorStartCtx {
        enc_key,
        session_db,
        decrypted_session,
        db_cache,
        db_dir,
        contact_names,
        username_db_map,
        self_username,
        router,
        wechat_base_dir,
        decoded_image_dir,
        image_aes_key,
        image_xor_key,
    } = ctx;
    let image_resolver = Some(crate::wechat::image::ImageResolver::new(
        wechat_base_dir,
        decoded_image_dir,
        db_cache.clone(),
        image_aes_key,
        image_xor_key,
    ));

    let monitor = Arc::new(SessionMonitor::new(
        enc_key,
        session_db.clone(),
        decrypted_session,
        db_cache,
        contact_names,
        username_db_map,
        db_dir.clone(),
        self_username,
        image_resolver,
    ));

    // ═══ 同步初始化解密（关键修复）═══
    // 必须在 start() 返回前完成，确保前端在 start_wechat_monitor 返回后
    // 立即读取 get_session_list() 时拿到的就是最新解密数据。
    // 之前放在 tokio::spawn 内异步执行，导致前端读到旧数据。
    // =================================
    log::info!("[monitor] 初始全量解密 session.db");
    {
        let this = monitor.clone();
        let res = tauri::async_runtime::spawn_blocking(move || this.do_full_refresh()).await;
        if let Err(e) = res.unwrap_or_else(|e| {
            Err(std::io::Error::other(format!(
                "[monitor] 初始解密任务异常: {}",
                e
            )))
        }) {
            log::error!("[monitor] 初始解密失败: {}", e);
        }
    }

    // 播种 prev_state（同步，使用 blocking_write 绕过 async 限制）
    let initial_state = {
        let this = monitor.clone();
        let res = tauri::async_runtime::spawn_blocking(move || this.query_state()).await;
        match res {
            Ok(Ok(initial)) => Some(initial),
            Ok(Err(e)) => {
                log::error!("[monitor] query_state 播种失败: {}", e);
                if e.to_string().contains("malformed") && monitor.decrypted_session.exists() {
                    let _ = std::fs::remove_file(&monitor.decrypted_session);
                    log::warn!(
                        "[monitor] 已删除损坏的解密数据库: {}",
                        monitor.decrypted_session.display()
                    );
                }
                None
            }
            Err(e) => {
                log::error!("[monitor] query_state 播种任务异常: {}", e);
                None
            }
        }
    };
    if let Some(initial) = initial_state {
        let n = initial.len();
        *monitor.prev_state.write().await = initial;
        log::info!("[monitor] 已播种 {} 个会话基线状态", n);
    }

    // ═══ 异步事件驱动循环 ═══
    tokio::spawn(async move {
        log::info!("[monitor] 启动实时消息监控 (notify + 5s 轮询 + 30s 水位线)");

        // 构建监听目录集合：session 目录 + db_dir 根目录
        // 注意：Windows 上 notify 只能目录级监听，不能 watch 单文件。
        let watched_dirs = crate::wechat::listener::default_watched_dirs(&db_dir);
        let (listener, file_rx) = match crate::wechat::listener::HybridListener::new(
            Some(db_dir.clone()),
            watched_dirs,
            10,
        ) {
            Ok(v) => v,
            Err(e) => {
                log::error!("[monitor] 创建 HybridListener 失败: {}，将回退到纯轮询", e);
                // 回退：使用空路径创建监听器，仅依赖其内部轮询/水位线 tick
                match crate::wechat::listener::HybridListener::new(None, vec![], 10) {
                    Ok(v) => v,
                    Err(e2) => {
                        // 双层回退均失败：上报异常状态后退出任务。
                        // 调用方持有 JoinHandle，可通过 is_finished() 发现任务死亡并重启。
                        log::error!("[monitor] 回退监听器也失败: {}，监控无法启动", e2);
                        router.emit_status("listener_error").await;
                        return;
                    }
                }
            }
        };

        let (trigger_tx, mut trigger_rx) =
            tokio::sync::mpsc::channel::<crate::wechat::listener::RefreshTrigger>(8);
        let listener_cancel = cancel_rx.clone();
        let listener_handle = tokio::spawn(async move {
            // listener 必须保持存活，否则 watcher 会被 drop
            let _listener = listener;
            _listener.run(file_rx, trigger_tx, listener_cancel).await;
        });

        let mut cancel_rx = cancel_rx;
        let mut error_count: u32 = 0;

        // 周期持久化水位线，避免进程重启后重复拉取历史消息
        let watermark_store_for_save = monitor.watermark_store.clone();
        let mut save_cancel = cancel_rx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                tokio::select! {
                    _ = save_cancel.changed() => {
                        if *save_cancel.borrow() {
                            // 退出前再保存一次
                            if let Err(e) = watermark_store_for_save.save().await {
                                log::warn!("[monitor] 退出前保存水位线失败: {}", e);
                            }
                            break;
                        }
                    }
                    _ = interval.tick() => {
                        if let Err(e) = watermark_store_for_save.save().await {
                            log::warn!("[monitor] 保存水位线失败: {}", e);
                        }
                    }
                }
            }
        });

        loop {
            let trigger = tokio::select! {
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() {
                        log::info!("[monitor] 收到取消信号，停止监控");
                        break;
                    }
                    continue;
                }
                Some(t) = trigger_rx.recv() => t,
            };

            let is_watermark_tick = trigger == crate::wechat::listener::RefreshTrigger::Watermark;
            log::debug!("[monitor] 收到触发器: {:?}", trigger);

            // 背压保护：如果前端未确认消息过多，跳过本次刷新
            // WatermarkTick 优先级较高，即使背压也允许执行，避免长期漏消息
            let pending = router.pending_count().await;
            if pending > 512 && !is_watermark_tick {
                log::warn!("[monitor] 前端未确认消息 {}，跳过本次刷新", pending);
                // 上报背压状态：前端收到后会立即 flush 积压的 ACK 并触发补拉，
                // 避免页面隐藏导致 ACK 节流时推送无限期暂停。
                router.emit_status("backpressure").await;
                continue;
            }

            // 调用 check_updates 检测新消息（内部会做全量解密+对比）
            let new_msgs = match if is_watermark_tick {
                tokio::time::timeout(Duration::from_secs(30), monitor.check_updates_forced()).await
            } else {
                tokio::time::timeout(Duration::from_secs(30), monitor.check_updates()).await
            } {
                Ok(msgs) => msgs,
                Err(_) => {
                    error_count += 1;
                    log::warn!(
                        "[monitor] check_updates 超时 (30s), 第{}次错误",
                        error_count
                    );
                    if error_count > 10 {
                        log::error!("[monitor] 连续错误过多，尝试重新初始化...");
                        let this = monitor.clone();
                        let res =
                            tauri::async_runtime::spawn_blocking(move || this.do_full_refresh())
                                .await;
                        if let Err(e) = res.unwrap_or_else(|e| {
                            Err(std::io::Error::other(format!(
                                "[monitor] 重新初始化解密任务异常: {}",
                                e
                            )))
                        }) {
                            log::error!("[monitor] 重新解密失败: {}", e);
                        }
                        error_count = 0;
                    }
                    continue;
                }
            };

            if !new_msgs.is_empty() {
                error_count = 0; // 重置错误计数
                log::info!("[monitor] 检测到 {} 条新消息", new_msgs.len());

                for msg in &new_msgs {
                    // 注意：不要按全局 local_id 去重。local_id 是每个会话消息表内独立的
                    // 自增 ID，并非全局单调；用单个全局阈值会误删 local_id 较小的新消息。
                    // 真正的去重由 check_updates 内的 shown_keys（按 (username, local_id)）
                    // 完成，这里直接推送即可。
                    match serde_json::to_value(msg) {
                        Ok(payload) => {
                            router.broadcast(payload).await;
                        }
                        Err(e) => {
                            log::warn!("[monitor] 序列化消息失败: {}", e);
                        }
                    }
                }
            }

            // 推送状态监控：每次水位线兜底 tick（30s）发送一次心跳，
            // 前端看门狗据此判断监控任务存活；长时间无心跳则自动重启监控。
            if is_watermark_tick {
                router.emit_status("heartbeat").await;
            }
        }

        listener_handle.abort();
        log::info!("[monitor] 监控线程已退出");
        // 任务退出（含异常路径）时主动上报，前端看门狗可立即感知
        router.emit_status("monitor_exited").await;
    })
}
