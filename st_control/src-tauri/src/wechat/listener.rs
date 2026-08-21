// ============================================================
// 微信数据监听层 —— HybridListener
// ============================================================
// 架构文档章节：2.1 监听策略 D（推荐）
// 职责：
//   1. 通过 notify 监听微信数据库所在目录（Windows 目录级监听）
//   2. 在回调中过滤 session.db / session.db-wal / message_N.db 等目标文件
//   3. 5s 保底轮询覆盖 notify 遗漏场景
//   4. 30s TimeStamp 水位线校验兜底长期静默
//   5. 30s 动态扫描新增 message_N.db 分库并加入监听
//   6. 抖动抑制：文件事件聚合为 50ms 防抖窗口
// 边界条件：
//   - Windows 上 ReadDirectoryChangesW 只能目录级监听，不能 watch 单文件
//   - 路径不存在时跳过监听，不阻塞启动
//   - 同一文件连续变化只触发一次刷新
//   - 运行期间新创建的 message_N.db 由动态扫描补齐
//   - 取消信号到达时立即释放 watcher 资源
// ============================================================

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::{Config, RecursiveMode, Watcher};
use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;

/// 监听层产生的事件类型
#[derive(Debug, Clone)]
pub enum ListenerEvent {
    /// 某个被监听文件发生变更
    FileChanged(PathBuf),
}

/// 触发 monitor 刷新的原因
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshTrigger {
    /// 文件事件或轮询 tick（正常路径）
    Event,
    /// 30s 水位线兜底 tick
    Watermark,
}

/// 三层混合监听器
#[allow(dead_code)]
pub struct HybridListener {
    /// 必须持有 watcher 才能保持监听生命周期
    watcher: Arc<Mutex<notify::RecommendedWatcher>>,
    debounce_ms: u64,
    db_dir: Option<PathBuf>,
    /// 已加入 watcher 的目录集合，避免重复 watch
    watched: Arc<Mutex<HashSet<PathBuf>>>,
    /// 目标文件集合，用于事件过滤；支持动态扩展
    watched_files: Arc<Mutex<HashSet<PathBuf>>>,
}

impl HybridListener {
    /// 创建监听器并返回事件接收通道
    ///
    /// # 参数
    /// - `db_dir`: 微信数据库根目录；提供后可动态扫描新增分库
    /// - `watch_dirs`: 启动时需要监听的目录路径（非文件路径）
    /// - `debounce_ms`: 防抖窗口（毫秒）
    ///
    /// # 注意
    /// notify 在 Windows 上基于 ReadDirectoryChangesW，只能目录级监听。
    /// 因此 watch_dirs 必须传入目录，事件回调再通过 watched_files 过滤目标文件。
    pub fn new(
        db_dir: Option<PathBuf>,
        watch_dirs: Vec<PathBuf>,
        debounce_ms: u64,
    ) -> Result<(Self, mpsc::Receiver<ListenerEvent>), notify::Error> {
        let (event_tx, event_rx) = mpsc::channel::<ListenerEvent>(1024);

        let watched_files = Arc::new(Mutex::new(build_watched_files(&db_dir)));
        let tx = event_tx.clone();
        let files_filter = watched_files.clone();
        let mut watcher = notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    for path in event.paths {
                        // 过滤：只关注我们关心的文件（在单线程回调中 try_lock 不会阻塞）
                        if !is_interesting_file(&path, &files_filter) {
                            continue;
                        }
                        // 忽略发送失败（意味着接收端已关闭，run 已退出）
                        let _ = tx.blocking_send(ListenerEvent::FileChanged(path));
                    }
                }
            },
            Config::default(),
        )?;

        let mut watched = HashSet::new();
        for dir in &watch_dirs {
            if dir.exists() && dir.is_dir() {
                if watcher.watch(dir, RecursiveMode::NonRecursive).is_ok() {
                    watched.insert(dir.clone());
                    log::info!("[listener] 开始监听目录: {}", dir.display());
                }
            } else {
                log::debug!("[listener] 目录不存在，跳过监听: {}", dir.display());
            }
        }

        let listener = Self {
            watcher: Arc::new(Mutex::new(watcher)),
            debounce_ms,
            db_dir,
            watched: Arc::new(Mutex::new(watched)),
            watched_files,
        };

        Ok((listener, event_rx))
    }

    /// 运行监听循环
    ///
    /// 事件优先级：
    ///   - 取消信号 > debounce 到期 > 文件事件/轮询 tick
    /// - WatermarkTick 不经过防抖，直接触发
    /// - FileChanged / PollTick 经过防抖聚合
    /// - 每 30s 执行一次 db_dir 扫描，把新出现的 message 分库加入监听
    pub async fn run(
        &self,
        mut event_rx: mpsc::Receiver<ListenerEvent>,
        trigger_tx: mpsc::Sender<RefreshTrigger>,
        mut cancel_rx: tokio::sync::watch::Receiver<bool>,
    ) {
        let debounce = Duration::from_millis(self.debounce_ms);
        let mut deadline: Option<Instant> = None;

        let mut poll_tick = tokio::time::interval(Duration::from_secs(1));
        let mut watermark_tick = tokio::time::interval(Duration::from_secs(10));
        let mut rescan_tick = tokio::time::interval(Duration::from_secs(30));

        // 优先处理取消与 debounce 到期，避免高频事件饿死定时器
        loop {
            tokio::select! {
                biased;
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() {
                        log::info!("[listener] 收到取消信号，退出监听循环");
                        break;
                    }
                }
                _ = async {
                    if let Some(d) = deadline {
                        tokio::time::sleep_until(d).await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                }, if deadline.is_some() => {
                    deadline = None;
                    if trigger_tx.send(RefreshTrigger::Event).await.is_err() {
                        log::info!("[listener] 触发通道已关闭，退出监听循环");
                        break;
                    }
                }
                _ = poll_tick.tick() => {
                    deadline = Some(Instant::now() + debounce);
                    log::debug!("[listener] PollTick，设置 debounce 触发器");
                }
                _ = watermark_tick.tick() => {
                    log::debug!("[listener] WatermarkTick，直接触发刷新");
                    if trigger_tx.send(RefreshTrigger::Watermark).await.is_err() {
                        log::info!("[listener] 触发通道已关闭，退出监听循环");
                        break;
                    }
                }
                _ = rescan_tick.tick() => {
                    if let Some(ref db_dir) = self.db_dir {
                        self.rescan(db_dir).await;
                    }
                }
                Some(ListenerEvent::FileChanged(path)) = event_rx.recv() => {
                    log::debug!("[listener] 文件变更: {}", path.display());
                    deadline = Some(Instant::now() + debounce);
                }
            }
        }
    }

    /// 动态扫描 db_dir 下新增 message 分库并加入过滤集合
    async fn rescan(&self, db_dir: &Path) {
        let new_paths = scan_message_dbs(db_dir);
        let mut guard = self.watched_files.lock().await;
        let mut added = 0;
        for path in new_paths {
            if guard.insert(path.clone()) {
                added += 1;
                log::info!("[listener] 动态扫描纳入新分库监听: {}", path.display());
            }
        }
        if added > 0 {
            log::info!("[listener] 本次扫描新增 {} 个 message 分库", added);
        }
    }
}

/// 判断路径是否为目标数据库文件
fn is_interesting_file(path: &Path, watched_files: &Mutex<HashSet<PathBuf>>) -> bool {
    // notify 回调在单线程中运行，try_lock 基本不会失败
    if let Ok(guard) = watched_files.try_lock() {
        if guard.contains(path) {
            return true;
        }
        // 兼容 Windows 路径大小写/短路径差异：同时比较文件名
        let file_name = path.file_name().map(|n| n.to_string_lossy().to_lowercase());
        guard.iter().any(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .as_ref()
                == file_name.as_ref()
        })
    } else {
        // 极小概率锁竞争：保守处理，先放行，后续过滤逻辑会忽略无关文件
        true
    }
}

/// 构造目标文件集合
fn build_watched_files(db_dir: &Option<PathBuf>) -> HashSet<PathBuf> {
    let mut files = HashSet::new();
    if let Some(db_dir) = db_dir {
        files.extend(default_watched_files(db_dir));
    }
    files
}

/// 扫描 message/biz_message 分库及其 WAL。
///
/// 微信 4.x 的分库位于 `db_dir/message/message_N.db`、`db_dir/biz_message/` 子目录，
/// 同时兼容少数版本直接放在 db_dir 根目录的情况。
fn scan_message_dbs(db_dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut dirs = vec![db_dir.to_path_buf()];
    for sub in ["message", "biz_message"] {
        let p = db_dir.join(sub);
        if p.is_dir() {
            dirs.push(p);
        }
    }
    for dir in dirs {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if (name.starts_with("message_") || name.starts_with("biz_message_"))
                    && name.ends_with(".db")
                {
                    paths.push(entry.path());
                    paths.push(entry.path().with_extension("db-wal"));
                }
            }
        }
    }
    paths
}

/// 构造默认监听文件集合
///
/// 包含：
///   - session.db / session.db-wal（会话列表变化）
///   - db_dir 下已存在的 message_N.db / biz_message_N.db
///
/// 运行期间新增的分库由 30s 动态扫描兜底。
pub fn default_watched_files(db_dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let session_dir = db_dir.join("session");
    paths.push(session_dir.join("session.db"));
    paths.push(session_dir.join("session.db-wal"));

    paths.extend(scan_message_dbs(db_dir));
    paths
}

/// 构造默认监听目录集合
///
/// notify 在 Windows 上只能目录级监听，因此返回目录路径。
pub fn default_watched_dirs(db_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.push(db_dir.join("session"));
    dirs.push(db_dir.to_path_buf());
    // 消息分库目录：notify 只做目录级监听，必须显式加入子目录
    for sub in ["message", "biz_message"] {
        let p = db_dir.join(sub);
        if p.exists() && p.is_dir() {
            dirs.push(p);
        }
    }
    dirs
}
