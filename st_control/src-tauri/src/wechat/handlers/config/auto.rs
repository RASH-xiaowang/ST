// ============================================================
// 微信 IPC — 全自动密钥获取域
// 依赖：auto_key（完全限定），零顶层导入
// ============================================================

// ─── 全自动密钥获取（对标 WeFlow：wx_key.dll Hook + 图片密钥模板校验） ───

/// 自动获取数据库密钥：查找微信进程 → 注入 Hook → 轮询 PollKeyData →
/// 校验并生成 all_keys.json → 写回 config.json
#[tauri::command]
pub async fn auto_get_db_key(
    app: tauri::AppHandle,
    timeout_ms: Option<u64>,
) -> Result<serde_json::Value, String> {
    let timeout = timeout_ms.unwrap_or(120_000).max(10_000);
    tauri::async_runtime::spawn_blocking(move || {
        crate::wechat::auto_key::auto_get_db_key(&app, "auto_db_key", timeout)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 自动获取数据库密钥（4.1.10.31+ 调试器方案）：DEBUG_PROCESS 启动微信 → 断点 →
/// HMAC 预言机验证 master key → 生成 all_keys.json → 写回 config.json。
/// 需要临时重启微信并重新扫码登录一次（微信单实例限制）。
#[tauri::command]
pub async fn auto_get_db_key_v2(
    app: tauri::AppHandle,
    timeout_ms: Option<u64>,
) -> Result<serde_json::Value, String> {
    let timeout = timeout_ms.unwrap_or(300_000).max(60_000);
    tauri::async_runtime::spawn_blocking(move || {
        crate::wechat::auto_key::auto_get_db_key_v2(&app, "auto_db_key_v2", timeout)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 自动获取图片密钥：GetImageKey 读 kvcomm 缓存 → *_t.dat 模板派生校验 →
/// 写回 config.json（image_aes_key / image_xor_key）
#[tauri::command]
pub async fn auto_get_image_key(
    app: tauri::AppHandle,
    base_dir: Option<String>,
    wxid: Option<String>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::wechat::auto_key::auto_get_image_key(&app, "auto_img_key", base_dir, wxid)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/// 一键全自动：依次获取数据库密钥与图片密钥
#[tauri::command]
pub async fn auto_get_wechat_keys(
    app: tauri::AppHandle,
    timeout_ms: Option<u64>,
) -> Result<serde_json::Value, String> {
    let timeout = timeout_ms.unwrap_or(120_000).max(10_000);
    tauri::async_runtime::spawn_blocking(move || {
        crate::wechat::auto_key::auto_get_wechat_keys(&app, "auto_keys", timeout)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}
