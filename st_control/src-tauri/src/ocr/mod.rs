// ============================================================
// 图文识别模块
// 1. HTTP 接收资源 API（POST /api/ocr/ingest）
// 2. 开源 OCR（RapidOCR）预检：无有效文本的图片直接过滤，不调用证件分类
// 3. TextIn 证件分类 → 按分类归档文件 → 对应 OCR 识别 → 结果入库
// 4. Tauri 命令：配置 / 列表 / 详情 / 重试 / 删除 / 统计
// ============================================================

pub mod config;
pub mod db;
#[cfg(feature = "onnx-ocr")]
pub mod rapid;
pub mod server;
pub mod textin;

use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock;
use tauri::Emitter;

#[cfg(feature = "onnx-ocr")]
type PrecheckSlot = rapid::PrecheckSlot;
#[cfg(not(feature = "onnx-ocr"))]
type PrecheckSlot = ();

/// 全局状态（Tauri State 托管为 Arc<OcrState>，axum State 共用同一 Arc）
pub struct OcrState {
    pub db: Arc<db::OcrDb>,
    pub config: RwLock<config::OcrConfig>,
    /// 开源 OCR 预检引擎缓存（按模型目录复用，惰性初始化；onnx-ocr 关闭时为空）
    #[cfg_attr(not(feature = "onnx-ocr"), allow(dead_code))]
    // 仅 onnx-ocr 特性的 run_precheck 使用
    pub precheck: std::sync::Mutex<Option<PrecheckSlot>>,
    /// 引擎首次初始化串行锁（ort 全局 Environment 初始化非线程安全，
    /// 并发初始化会导致其中一个失败）
    #[cfg_attr(not(feature = "onnx-ocr"), allow(dead_code))] // 同上
    precheck_init: tokio::sync::Mutex<()>,
    server_task: tokio::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    app: RwLock<Option<tauri::AppHandle>>,
}

impl OcrState {
    pub fn new(db: db::OcrDb) -> Self {
        let db = Arc::new(db);
        let cfg = config::OcrConfig::load(&db);
        Self {
            db,
            config: RwLock::new(cfg),
            precheck: std::sync::Mutex::new(None),
            precheck_init: tokio::sync::Mutex::new(()),
            server_task: tokio::sync::Mutex::new(None),
            app: RwLock::new(None),
        }
    }

    pub fn attach_app(&self, app: tauri::AppHandle) {
        *self.app.write().unwrap_or_else(|p| p.into_inner()) = Some(app);
    }

    /// 执行开源 OCR 预检：返回图片识别出的文本。
    /// 首次调用会在线程池中初始化引擎（含模型下载，可能耗时）；
    /// 返回 Err 表示引擎不可用，调用方应视为处理失败并提示用户。
    #[cfg(feature = "onnx-ocr")]
    pub async fn run_precheck(
        &self,
        cfg: &config::OcrConfig,
        image_path: &Path,
    ) -> Result<String, String> {
        let model_dir = rapid::resolve_model_dir(&cfg.precheck_model_dir);
        // 串行化首次初始化（含模型下载），避免 ort 全局环境并发初始化冲突
        let _init_guard = self.precheck_init.lock().await;
        let slot: rapid::PrecheckSlot = {
            let cached = self
                .precheck
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            match cached {
                Some(s) if s.model_dir == model_dir => s,
                _ => {
                    let dir = model_dir.clone();
                    let s = tauri::async_runtime::spawn_blocking(move || rapid::init_engine(&dir))
                        .await
                        .map_err(|e| format!("OCR 预检初始化任务失败: {e}"))??;
                    *self.precheck.lock().unwrap_or_else(|p| p.into_inner()) = Some(s.clone());
                    s
                }
            }
        };
        let path = image_path.to_path_buf();
        tauri::async_runtime::spawn_blocking(move || rapid::ocr_file(&path, &slot.ocr))
            .await
            .map_err(|e| format!("OCR 预检任务失败: {e}"))?
    }

    /// 未启用 onnx-ocr 特性时预检为空（不拦截任何图片，由下游分类接口兜底）
    #[cfg(not(feature = "onnx-ocr"))]
    pub async fn run_precheck(
        &self,
        _cfg: &config::OcrConfig,
        _image_path: &Path,
    ) -> Result<String, String> {
        Ok(String::new())
    }

    /// 向前端广播处理进度（前端监听 ocr-event）
    pub fn emit_event(&self, id: i64, status: &str, category: &str, error: Option<&str>) {
        let payload = serde_json::json!({
            "id": id,
            "status": status,
            "category": category,
            "error": error.unwrap_or(""),
            "ts": chrono::Local::now().to_rfc3339(),
        });
        if let Some(app) = self.app.read().unwrap_or_else(|p| p.into_inner()).as_ref() {
            let _ = app.emit("ocr-event", &payload);
        }
    }

    /// 启动（或按新配置重启）HTTP 接收服务
    pub async fn restart_server(self: &Arc<Self>) {
        let mut slot = self.server_task.lock().await;
        if let Some(h) = slot.take() {
            h.abort();
        }
        let state = self.clone();
        *slot = Some(tauri::async_runtime::spawn(async move {
            server::serve(state).await;
        }));
    }
}

/// 校验五要素并写入一条资源、触发处理管线（HTTP 接口与微信推送共用）
pub(crate) fn submit_resource(
    state: &Arc<OcrState>,
    sender_username: &str,
    session_type: &str,
    timestamp: &str,
    username: &str,
    media_url: &str,
) -> Result<i64, String> {
    let mut missing: Vec<&str> = Vec::new();
    if sender_username.trim().is_empty() {
        missing.push("sender_username");
    }
    if session_type.trim().is_empty() {
        missing.push("session_type");
    }
    if timestamp.trim().is_empty() {
        missing.push("timestamp");
    }
    if username.trim().is_empty() {
        missing.push("username");
    }
    if media_url.trim().is_empty() {
        missing.push("mediaUrl");
    }
    if !missing.is_empty() {
        return Err(format!("缺少必填参数: {}", missing.join(", ")));
    }

    let id = state
        .db
        .insert_resource(
            sender_username.trim(),
            session_type.trim(),
            timestamp.trim(),
            username.trim(),
            media_url.trim(),
        )
        .map_err(|e| format!("写入数据库失败: {e}"))?;
    let st = state.clone();
    tauri::async_runtime::spawn(async move {
        server::process_resource(st, id).await;
    });
    Ok(id)
}

// ─────────────────────────── Tauri 命令 ───────────────────────────

#[tauri::command]
pub fn ocr_get_config(state: tauri::State<'_, Arc<OcrState>>) -> config::OcrConfig {
    state
        .config
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

#[tauri::command]
pub fn ocr_set_config(
    state: tauri::State<'_, Arc<OcrState>>,
    config: config::OcrConfig,
) -> Result<(), String> {
    log::info!(
        "[ocr] ocr_set_config 开始: port={} bind={} enabled={}",
        config.port,
        config.bind_host,
        config.enabled
    );
    let old = state
        .config
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let changed = old.port != config.port
        || old.bind_host != config.bind_host
        || old.enabled != config.enabled;
    log::info!("[ocr] ocr_set_config 写入配置 (changed={changed})");
    config
        .save(&state.db)
        .map_err(|e| format!("保存配置失败: {e}"))?;
    log::info!("[ocr] ocr_set_config 配置已落库");
    *state.config.write().unwrap_or_else(|p| p.into_inner()) = config;
    if changed {
        log::info!("[ocr] ocr_set_config 触发服务重启");
        let st = state.inner().clone();
        tauri::async_runtime::spawn(async move {
            st.restart_server().await;
        });
    }
    log::info!("[ocr] ocr_set_config 完成");
    Ok(())
}

#[tauri::command]
pub fn ocr_list_resources(
    state: tauri::State<'_, Arc<OcrState>>,
    page: Option<u32>,
    page_size: Option<u32>,
    status: Option<String>,
    category: Option<String>,
    keyword: Option<String>,
) -> Result<db::OcrPage, String> {
    let page = page.unwrap_or(1).max(1) as i64;
    let size = page_size.unwrap_or(20).clamp(1, 200) as i64;
    let offset = (page - 1) * size;
    state
        .db
        .list_resources(
            size,
            offset,
            status.as_deref(),
            category.as_deref(),
            keyword.as_deref(),
        )
        .map_err(|e| format!("查询资源列表失败: {e}"))
}

#[tauri::command]
pub fn ocr_get_resource(
    state: tauri::State<'_, Arc<OcrState>>,
    id: i64,
) -> Result<Option<db::OcrResource>, String> {
    state
        .db
        .get_resource(id)
        .map_err(|e| format!("查询资源失败: {e}"))
}

#[tauri::command]
pub fn ocr_retry_resource(state: tauri::State<'_, Arc<OcrState>>, id: i64) -> Result<(), String> {
    if state
        .db
        .get_resource(id)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err("资源不存在".to_string());
    }
    state
        .db
        .update_ocr_result(id, "pending", "", "{}", "")
        .map_err(|e| format!("重置状态失败: {e}"))?;
    let st = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        server::process_resource(st, id).await;
    });
    Ok(())
}

#[tauri::command]
pub fn ocr_delete_resource(state: tauri::State<'_, Arc<OcrState>>, id: i64) -> Result<(), String> {
    log::info!("[ocr] ocr_delete_resource 开始: id={id}");
    let item = state
        .db
        .delete_resource(id)
        .map_err(|e| format!("删除记录失败: {e}"))?;
    log::info!(
        "[ocr] ocr_delete_resource 记录已删除: id={id} item={}",
        item.is_some()
    );
    if let Some(r) = item {
        if !r.media_path.is_empty() {
            let p = std::path::Path::new(&r.media_path);
            if p.exists() {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    log::info!("[ocr] ocr_delete_resource 完成: id={id}");
    Ok(())
}

#[tauri::command]
pub fn ocr_get_stats(state: tauri::State<'_, Arc<OcrState>>) -> Result<db::OcrStats, String> {
    state.db.stats().map_err(|e| format!("统计失败: {e}"))
}

/// 模拟测试：插入一条内置测试图片的资源并跑完整管线，
/// 便于在不推送真实数据的情况下验证 接收→分类→归档→OCR 链路。
#[tauri::command]
pub fn ocr_simulate_test(
    state: tauri::State<'_, Arc<OcrState>>,
    index: Option<usize>,
) -> Result<i64, String> {
    let idx = index
        .unwrap_or(0)
        .min(server::TEST_IMAGES.len().saturating_sub(1));
    let media_url = format!("builtin://test/test{}.jpg", idx + 1);
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let id = state
        .db
        .insert_resource("simulate", "simulate", &now, "模拟测试", &media_url)
        .map_err(|e| format!("创建模拟资源失败: {e}"))?;
    let st = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        server::process_resource(st, id).await;
    });
    Ok(id)
}

/// 从微信数据管理推送资源：五要素与 HTTP /api/ocr/ingest 一致
#[tauri::command]
pub fn ocr_ingest_resource(
    state: tauri::State<'_, Arc<OcrState>>,
    sender_username: String,
    session_type: String,
    timestamp: String,
    username: String,
    media_url: String,
) -> Result<i64, String> {
    submit_resource(
        state.inner(),
        &sender_username,
        &session_type,
        &timestamp,
        &username,
        &media_url,
    )
}

/// 批量导入本地图片：逐个建资源并进入 OCR 管线，返回导入数量
#[tauri::command]
pub fn ocr_ingest_local_files(
    state: tauri::State<'_, Arc<OcrState>>,
    paths: Vec<String>,
) -> Result<i64, String> {
    let mut count = 0i64;
    for p in &paths {
        let path = Path::new(p);
        if !path.is_file() {
            continue;
        }
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let id = state
            .db
            .insert_local_resource(
                "本地导入",
                "local",
                &chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                "local",
                &filename,
                p,
            )
            .map_err(|e| format!("写入数据库失败: {e}"))?;
        let st = state.inner().clone();
        tauri::async_runtime::spawn(async move {
            server::process_resource(st, id).await;
        });
        count += 1;
    }
    Ok(count)
}

/// 人工校对识别字段（覆盖 ocr_fields，状态置 corrected）
#[tauri::command]
pub fn ocr_update_resource_fields(
    state: tauri::State<'_, Arc<OcrState>>,
    id: i64,
    fields: String,
) -> Result<(), String> {
    state
        .db
        .update_ocr_fields(id, &fields)
        .map_err(|e| format!("保存校对失败: {e}"))
}

/// 导出全部 OCR 资源为 CSV（写入 ocr 库同级的 exports/）
#[tauri::command]
pub fn ocr_export_csv(state: tauri::State<'_, Arc<OcrState>>) -> Result<serde_json::Value, String> {
    let resources = state.db.all_resources().map_err(|e| e.to_string())?;
    let cell = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut csv = String::from(
        "id,发送人,会话类型,时间,用户,分类,状态,错误,预检文本,识别字段,媒体URL,创建时间\n",
    );
    for r in &resources {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            r.id,
            cell(&r.sender_username),
            cell(&r.session_type),
            cell(&r.timestamp),
            cell(&r.username),
            cell(&r.category),
            cell(&r.status),
            cell(&r.error),
            cell(&r.precheck_text),
            cell(&r.ocr_fields),
            cell(&r.media_url),
            cell(&r.created_at),
        ));
    }
    let dir = db::db_path()
        .parent()
        .map(|p| p.join("exports"))
        .ok_or_else(|| "无法定位 OCR 数据目录".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建导出目录失败: {e}"))?;
    let filename = format!(
        "OCR资源_{}.csv",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    );
    let filepath = dir.join(&filename);
    std::fs::write(&filepath, format!("\u{feff}{}", csv).as_bytes())
        .map_err(|e| format!("写入导出文件失败: {e}"))?;
    Ok(serde_json::json!({
        "path": filepath.to_string_lossy().to_string(),
        "filename": filename,
        "count": resources.len(),
    }))
}
