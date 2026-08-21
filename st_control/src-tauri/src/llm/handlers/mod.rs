// ============================================================
// 大模型管理 — IPC 命令
// 覆盖：接入配置(CRUD) / 连接测试 / 模型管理 / 全局调用 / 流量与成本管控
// ============================================================

use serde_json::json;
use tauri::Emitter;

mod chat;

mod audio;
pub use audio::*;
pub use chat::*;

mod generation;
pub use generation::*;
mod usage;

mod resource;
pub use resource::*;

mod history;
pub use history::*;

mod embedding;
pub use embedding::*;
pub use usage::*;

mod providers;
pub use providers::*;

/// 大模型配置变更事件名：前端所有使用模型的界面统一监听，收到后实时刷新，
/// 无需在各界面手动点击「刷新」。
pub const LLM_CONFIG_CHANGED_EVENT: &str = "llm-config-changed";

/// 配置落盘成功后广播变更事件，通知所有界面同步最新数据。
fn notify_llm_config_changed<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let _ = app.emit(
        LLM_CONFIG_CHANGED_EVENT,
        json!({
            "changed_at": crate::llm::config::now_iso(),
        }),
    );
}
