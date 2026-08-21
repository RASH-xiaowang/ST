// ============================================================
// 内部数据库 IPC — 库信息 / 事件日志 / 配置
// 依赖：db（完全限定），零顶层导入
// ============================================================

// ─────────────────────────────────────────────
// 内部数据库 IPC (control.db)
// ─────────────────────────────────────────────

#[tauri::command]
pub async fn get_db_info(
    db: tauri::State<'_, crate::db::Database>,
) -> Result<crate::db::DbInfo, String> {
    db.db_info().map_err(|e| e.to_string())
}

/// 应用自管理的业务数据库列表（每日总结 / 消息编辑 / 知识库等），供数据库管理界面快捷打开
#[tauri::command]
pub async fn list_app_databases() -> Result<Vec<serde_json::Value>, String> {
    let data_root = crate::common::st_data_dir();
    let wechat_root = crate::common::wechat_data_dir();
    let mut out = Vec::new();
    let candidates: Vec<(&str, std::path::PathBuf, &str)> = vec![
        (
            "control",
            data_root.join("control.db"),
            "🧭 控制台主库（事件/配置/任务）",
        ),
        (
            "knowledge_base",
            data_root.join("knowledge_base.db"),
            "📚 知识库",
        ),
        (
            "llm_gateway",
            data_root.join("llm_gateway.db"),
            "🤖 大模型网关",
        ),
        (
            "daily_summary",
            wechat_root.join("daily_summary.db"),
            "📅 每日总结",
        ),
        (
            "message_edits",
            wechat_root.join("message_edits.db"),
            "✏️ 消息编辑",
        ),
        (
            "wechat_search",
            wechat_root.join("wechat_search.db"),
            "🔍 微信全文检索",
        ),
    ];
    for (key, p, label) in candidates {
        if p.exists() {
            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            out.push(serde_json::json!({
                "key": key,
                "label": label,
                "name": p.file_name().and_then(|n| n.to_str()).unwrap_or(key),
                "path": p.to_string_lossy().to_string(),
                "size_bytes": size,
            }));
        }
    }
    Ok(out)
}

/// 应用数据目录一览（供前端「数据库管理」扫描目录初始化，替代前端硬编码路径）
#[tauri::command]
pub async fn get_app_data_dirs() -> Result<serde_json::Value, String> {
    let mut scan_roots = vec![
        crate::common::st_data_dir().display().to_string(),
        crate::common::wechat_data_dir().display().to_string(),
    ];
    // 旧散落目录仍存在时一并纳入扫描（迁移后可人工删除）
    if let Some(d) = dirs::data_dir() {
        for name in ["st-control", "st_result", "st_wechat"] {
            let p = d.join(name);
            if p.is_dir() {
                scan_roots.push(p.display().to_string());
            }
        }
    }
    Ok(serde_json::json!({
        "appBase": crate::common::app_base_dir().display().to_string(),
        "dataDir": crate::common::st_data_dir().display().to_string(),
        "wechatDataDir": crate::common::wechat_data_dir().display().to_string(),
        "logsDir": crate::common::logs_dir().display().to_string(),
        "scanDirs": scan_roots,
    }))
}

#[tauri::command]
pub async fn query_events(
    db: tauri::State<'_, crate::db::Database>,
    limit: usize,
    offset: usize,
) -> Result<Vec<crate::EventLog>, String> {
    db.query_events(limit, offset).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn query_agent_log(
    db: tauri::State<'_, crate::db::Database>,
    agent_id: String,
    limit: usize,
) -> Result<Vec<crate::db::AgentLogRow>, String> {
    db.query_agent_log(&agent_id, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn insert_event(
    db: tauri::State<'_, crate::db::Database>,
    timestamp: String,
    event_type: String,
    source: String,
    title: String,
    detail: String,
    level: String,
) -> Result<(), String> {
    db.insert_event(&timestamp, &event_type, &source, &title, &detail, &level)
        .map_err(|e| e.to_string())
}

// ─── 数据库配置 ───

#[tauri::command]
pub async fn get_db_config(
    db: tauri::State<'_, crate::db::Database>,
) -> Result<Vec<crate::db::ConfigItem>, String> {
    db.get_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_db_config(
    db: tauri::State<'_, crate::db::Database>,
    key: String,
    value: String,
) -> Result<(), String> {
    db.set_config(&key, &value).map_err(|e| e.to_string())
}
