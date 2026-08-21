// ============================================================
// Harness — DeepSeek Harness 纯原生迁移运行时
//
// 阶段 1：导航入口 + 运行时骨架（Cordis-lite 服务注册表）+
// 会话核心（追加式事件日志 / 持久化 / 标题投影）+ 流式对话。
// 阶段 2：工具作用域注册表 + 守卫执行管道 + prompt 分区组装 +
// 会话内工具循环 + 审批/信任 + 身份/设置。
// 阶段 3：guard（工具超时/循环卫生，可配置）+ hooks（外部钩子桥）+
// preset（预设组合与会话作用域）+ telemetry（会话用量）。
// 阶段 4：编排（subagent/workflow/todo/plan/goal/schedule）。
// 阶段 5：执行世界（shell/subprocess/terminal/fs/sandbox）。
// 阶段 6：协议与连接器（web 接缝/context/compaction/attachment/sdk/mcp）。
// 后续阶段按 docs/harness-migration-plan.md 推进（skill/feedback/扩展生态、
// CLI 等价物/文档站）。
// ============================================================

pub mod agent;
pub mod approval;
pub mod attachment;
pub mod compaction;
pub mod context;
pub mod credentials;
pub mod feedback;
pub mod fs;
pub mod hooks;
pub mod identity;
pub mod instructions;
pub mod interaction;
pub mod jobs;
pub mod lsp;
pub mod mcp;
pub mod portability;
pub mod preset;
pub mod pty;
pub mod registry;
pub mod schedule;
pub mod sdk;
pub mod session;
pub mod settings;
pub mod shell;
pub mod skill;
pub mod spill;
pub mod storage;
pub mod subagent;
pub mod terminal;
pub mod tools;
pub mod web;
pub mod workflow;
pub mod workspace;

use std::sync::Arc;

/// 全局 AppHandle（SDK/后台任务等无 IPC 上下文场景使用；init 时设置）
static RUNTIME_APP: std::sync::OnceLock<std::sync::Mutex<Option<tauri::AppHandle>>> =
    std::sync::OnceLock::new();

/// 获取全局 AppHandle
pub(crate) fn runtime_app_handle() -> Result<tauri::AppHandle, String> {
    RUNTIME_APP
        .get()
        .and_then(|m| m.lock().unwrap().clone())
        .ok_or_else(|| "Harness 运行时未初始化".to_string())
}

/// 运行时引导：注册基础服务（lib.rs 启动时调用）。
/// 每个 provide 返回的 Disposer 都被 disarm：应用级服务的生命周期
/// 与进程一致，不需要在会话内撤销。
pub fn init(app: Option<&tauri::AppHandle>, db: crate::db::Database) {
    if let Some(app) = app {
        let _ = RUNTIME_APP.get_or_init(|| std::sync::Mutex::new(Some(app.clone())));
    }
    let _disposer = registry::provide("harness.sessions", Arc::new(session::SessionStore::new(db)));
    _disposer.disarm();
    let _fs = fs::provide_service();
    _fs.disarm();
    let _shell = shell::provide_service();
    _shell.disarm();
    let _web = web::provide_service();
    _web.disarm();
    let _storage = storage::provide_service();
    _storage.disarm();
    // 示例预设种子（DSH examples 迁移）
    preset::seed_examples();
    // SDK / JSON-RPC 服务（本地监听 127.0.0.1）
    sdk::start();
    log::info!(
        "[harness] 运行时已初始化（会话核心 + 工具 + 审批 + 预设 + 钩子 + fs/shell/web + SDK）"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_provides_session_store() {
        init(None, crate::db::Database::new().unwrap());
        assert!(registry::get::<session::SessionStore>("harness.sessions").is_some());
    }

    #[test]
    fn init_registers_all_services_and_seeds_presets() {
        // init 引导完整性：5 个核心服务注册 + 示例预设种子
        init(None, crate::db::Database::new().unwrap());
        assert!(
            registry::get::<crate::harness::fs::FsService>("harness.fs").is_some(),
            "fs 服务应注册"
        );
        assert!(
            registry::get::<crate::harness::shell::ShellService>("harness.shell").is_some(),
            "shell 服务应注册"
        );
        assert!(
            registry::get::<crate::harness::web::WebService>("harness.web").is_some(),
            "web 服务应注册"
        );
        assert!(
            registry::get::<crate::harness::storage::StorageService>("harness.storage").is_some(),
            "storage 服务应注册"
        );
        // 示例预设种子（preset-example-readonly 等）
        let presets = crate::harness::preset::presets_store().lock().unwrap();
        assert!(
            presets.iter().any(|p| p.id == "preset-example-readonly"),
            "示例预设应已种子化: {:?}",
            presets.iter().map(|p| p.id.clone()).collect::<Vec<_>>()
        );
    }
}
