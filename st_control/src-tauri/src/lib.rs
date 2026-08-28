mod agents;
mod ai_role;
mod automation;
mod bot;
mod common;
mod db;
mod external_db;
pub mod harness; // pub：诊断二进制（ptytest）与集成面使用
mod ipc_handlers;
pub mod kb;
mod llm;
mod native_tts;
mod ocr;
mod rate_limit;
mod security;
mod sql_browse;
#[cfg(feature = "local-stt")]
pub mod stt;
mod system_metrics;
mod vector_index;
pub mod wechat;
mod ws_server;

use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

/// 默认 WebSocket 服务器端口
const DEFAULT_WS_PORT: u16 = 9786;

/// Tauri 事件名
const EVENT_SERVER_EVENT: &str = "server-event";

/// 事件日志条目（供 db 模块和前端共用）
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EventLog {
    pub id: usize,
    pub timestamp: String,
    pub event_type: String,
    pub source: String,
    pub title: String,
    pub detail: String,
    pub level: String,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 统一目录方案：先确保 <应用基目录>/data（含 logs）存在，
    // 日志初始化前完成旧目录迁移（st-control/st_result/st_role → data/）
    crate::common::ensure_base_dirs();
    let migrated = crate::common::migrate_legacy_dirs();

    // 初始化日志系统：stderr + data/logs/app.log 双写（部署后无控制台也可查日志）
    let log_file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(crate::common::logs_dir().join("app.log"))
    {
        Ok(f) => Some(f),
        Err(e) => {
            eprintln!("日志文件打开失败（仅 stderr）: {e}");
            None
        }
    };
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder.format_timestamp_secs();
    if let Some(f) = log_file {
        let tee = crate::common::LogTee::new(std::io::stderr(), f);
        builder.target(env_logger::Target::Pipe(Box::new(tee)));
    }
    builder.init();

    for m in &migrated {
        log::info!("[migrate] {m}");
    }
    log::info!(
        "[paths] app_base={} data={}",
        crate::common::app_base_dir().display(),
        crate::common::st_data_dir().display()
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 初始化数据库
            let db = db::Database::new().expect("初始化 SQLite 数据库失败");
            let control_db_path = db.path();
            // Harness 运行时（DSH 纯原生迁移）：注册基础服务
            harness::init(Some(app.handle()), db.clone());
            // Harness 定时调度器（每 30 秒检查到期条目）
            harness::schedule::start(app.handle().clone());
            app.manage(db);

            // 创建并自动启动 WebSocket 服务器
            let ws_server = ws_server::create_server(DEFAULT_WS_PORT);
            let app_handle = app.handle().clone();

            // 事件转发：WebSocket 事件 → Tauri 前端
            let event_rx = ws_server.subscribe_events();
            tauri::async_runtime::spawn(async move {
                forward_events_to_frontend(app_handle, event_rx).await;
            });

            // 自动启动服务器
            let server_for_autostart = ws_server.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = server_for_autostart.start().await {
                    log::error!("自动启动服务器失败: {}", e);
                }
            });

            app.manage(ws_server);
            log::info!("ST Control 服务器已就绪，端口: {}", DEFAULT_WS_PORT);

            // ─── 向量索引缓存（加速知识库检索） ───
            let vector_index = std::sync::Arc::new(vector_index::VectorIndex::new());
            app.manage(vector_index.clone());

            // ─── 应用级安全加密器（统一密钥管理） ───
            if let Err(e) = security::AppCipher::init(&crate::common::st_data_dir()) {
                log::warn!(
                    "[security] 应用加密器初始化失败（敏感数据将明文存储）: {}",
                    e
                );
            }

            // ─── 知识库数据库（独立 SQLite 库） ───
            let kb_db = match kb::db::KbDatabase::new() {
                Ok(db) => {
                    // 首次启动 seed 默认用户
                    kb::auth::ensure_seed_users(&db);
                    // 确保存在系统知识库（知识收集核心载体）
                    kb::handlers::ensure_system_kb(&db);
                    log::info!("知识库数据库已挂载");
                    Some(db)
                }
                Err(e) => {
                    log::error!("知识库数据库初始化失败（知识库功能不可用）: {}", e);
                    None
                }
            };

            // ─── 知识库登录态（会话） ───
            // 私有化部署：无权限控制，每次启动强制以管理员身份登录，
            // 避免每次操作都要求手动登录，也避免旧会话文件残留非管理员身份。
            let kb_session = kb::auth::UserSession::load();
            if let Some(kb) = &kb_db {
                if let Some(admin) = kb::auth::default_admin(kb) {
                    kb_session.set(admin);
                }
            }
            app.manage(kb_session);
            if let Some(db) = kb_db {
                app.manage(db);
            }

            // ─── 图文识别（资源接收 + TextIn 分类/OCR） ───
            let ocr_state = std::sync::Arc::new(ocr::OcrState::new(
                ocr::db::OcrDb::open().expect("初始化图文识别数据库失败"),
            ));
            ocr_state.attach_app(app.handle().clone());
            app.manage(ocr_state.clone());
            let ocr_state_for_start = ocr_state.clone();
            tauri::async_runtime::spawn(async move {
                ocr_state_for_start.restart_server().await;
            });

            // ─── 实时系统指标采集器（供数据看板高频轮询） ───
            app.manage(system_metrics::SystemMetrics::new());

            // ─── 微信监控状态管理（自动启动） ───
            let monitor_state = Arc::new(wechat::handlers::WeChatMonitorState::new());
            app.manage(monitor_state.clone());

            // ─── 自动启动微信监控 ───
            let monitor_for_autostart = monitor_state.clone();
            let app_handle2 = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match monitor_for_autostart.start(app_handle2).await {
                    Ok(()) => log::info!("微信监控已自动启动"),
                    Err(e) => log::warn!("微信监控自动启动失败（可忽略）: {}", e),
                }
            });

            // ─── 启动微信数据 HTTP API（127.0.0.1 本地监听） ───
            // 服务状态注册为 Tauri State，支持设置界面热更新令牌/端口/开关
            // 安全默认：启用 API 且未配置 token 时自动生成并持久化，避免免鉴权裸奔
            {
                let cfg = wechat::config::WeChatConfig::load().ok();
                let (api_enabled, api_port, mut api_token) = match &cfg {
                    Some(c) => (c.api_enabled, c.api_port, c.api_token.clone()),
                    None => (true, 5032, None),
                };
                if api_enabled
                    && api_token
                        .as_deref()
                        .map(|t| t.trim().is_empty())
                        .unwrap_or(true)
                {
                    let token = uuid::Uuid::new_v4().simple().to_string();
                    api_token = Some(token.clone());
                    if let Some(mut raw) = wechat::config::load_raw_config_public() {
                        raw.api_token = Some(token);
                        if let Err(e) = wechat::config::save_config(&raw) {
                            log::warn!("自动生成 HTTP API token 保存失败: {e}");
                        } else {
                            wechat::config::WeChatConfig::refresh_cache();
                        }
                    }
                    log::info!("HTTP API 未配置 token，已自动生成默认访问令牌");
                }
                let api_state = std::sync::Arc::new(wechat::http_api::ApiServerState::new(
                    monitor_state.clone(),
                    api_token,
                    api_port,
                    api_enabled,
                ));
                app.manage(api_state.clone());
                tauri::async_runtime::spawn(wechat::http_api::serve(api_state));
            }

            // ─── 每日总结定时调度（每分钟检查到点任务） ───
            wechat::daily_summary::spawn_scheduler();

            // ─── 微信原图 Hook（img_helper.dll，参考 WeFlow） ───
            let hook_manager = wechat::hook::HookManager::new();
            app.manage(hook_manager.clone());
            {
                let hook_cfg = wechat::hook::HookManager::load_config();
                if hook_cfg.enabled {
                    match hook_manager.start(app.handle(), hook_cfg.whitelist) {
                        Ok(s) => log::info!("原图 Hook 已按上次配置恢复（hooked={}）", s.hooked),
                        Err(e) => log::warn!("原图 Hook 自动恢复失败: {e}"),
                    }
                }
            }

            // ─── 自动化管理中心（规则 + 消息任务 + SSE 消费 + 内置 Worker） ───
            match automation::AutomationState::new(&control_db_path, Some(monitor_state.clone())) {
                Ok(automation_state) => {
                    automation_state.ensure_sse(
                        app.handle().clone(),
                        "http://127.0.0.1:5032/api/v1/push/messages".to_string(),
                    );
                    // 内置任务执行器：pending 任务 → KB/角色/LLM 执行 → 回写/待回复
                    automation::worker::spawn_worker(app.handle().clone());
                    app.manage(automation_state);
                    log::info!("自动化管理中心已初始化（SSE 消费 + 内置 Worker 已启动）");
                }
                Err(e) => {
                    log::error!("自动化管理中心初始化失败: {e}");
                }
            }

            // ─── 消息通道（微信 ClawBot / iLink）：多账号扫码绑定 + 双向收发 ───
            let bot_data_dir = common::st_data_dir();
            let bot_manager = match bot::manager::BotManager::new(&bot_data_dir, &control_db_path) {
                Ok(m) => std::sync::Arc::new(m),
                Err(e) => {
                    log::error!("消息通道初始化失败（ClawBot 不可用）: {e}");
                    return Err(Box::new(std::io::Error::other(format!(
                        "消息通道初始化失败: {e}"
                    ))));
                }
            };
            bot_manager.attach_app(app.handle().clone());
            bot_manager.start_all();
            app.manage(bot_manager);
            log::info!("消息通道已初始化（ClawBot / iLink）");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // ─── 服务端 / 系统 IPC ───
            ipc_handlers::get_server_status,
            ipc_handlers::get_app_info,
            ipc_handlers::get_system_info,
            system_metrics::get_realtime_metrics,
            ipc_handlers::send_command_to_agent,
            // ─── 图文识别 IPC ───
            ocr::ocr_get_config,
            ocr::ocr_set_config,
            ocr::ocr_list_resources,
            ocr::ocr_get_resource,
            ocr::ocr_retry_resource,
            ocr::ocr_delete_resource,
            ocr::ocr_get_stats,
            ocr::ocr_simulate_test,
            ocr::ocr_ingest_resource,
            ocr::ocr_ingest_local_files,
            ocr::ocr_update_resource_fields,
            ocr::ocr_export_csv,
            // ─── 自动化管理中心 IPC ───
            automation::handlers::automation_list_rules,
            automation::handlers::automation_save_rule,
            automation::handlers::automation_delete_rule,
            automation::handlers::automation_toggle_rule,
            automation::handlers::automation_list_tasks,
            automation::handlers::automation_get_task,
            automation::handlers::automation_set_task_status,
            automation::handlers::automation_set_task_target,
            automation::handlers::automation_edit_task_reply,
            automation::handlers::automation_edit_ai_extract,
            automation::handlers::automation_delete_task,
            automation::handlers::automation_stats,
            automation::handlers::automation_update_reply_by_key,
            automation::handlers::automation_simulate_push,
            automation::handlers::automation_debug_broadcast,
            automation::handlers::automation_conn_status,
            automation::handlers::automation_reconnect,
            // ─── 消息通道 IPC（ClawBot） ───
            bot::handlers::bot_list_accounts,
            bot::handlers::bot_start_qr,
            bot::handlers::bot_poll_qr,
            bot::handlers::bot_cancel_qr,
            bot::handlers::bot_rename_account,
            bot::handlers::bot_unbind_account,
            bot::handlers::bot_status_summary,
            bot::handlers::bot_add_channel,
            bot::handlers::bot_update_channel,
            bot::handlers::bot_test_channel,
            bot::handlers::bot_send_text,
            bot::handlers::bot_send_media,
            bot::handlers::bot_list_contacts,
            bot::handlers::bot_list_qqbot_contacts,
            bot::handlers::bot_list_logs,
            bot::handlers::bot_clear_logs,
            // ─── 数据库 IPC ───
            ipc_handlers::get_db_info,
            ipc_handlers::list_app_databases,
            ipc_handlers::get_app_data_dirs,
            ipc_handlers::get_db_config,
            ipc_handlers::set_db_config,
            ipc_handlers::query_events,
            ipc_handlers::query_agent_log,
            ipc_handlers::insert_event,
            ipc_handlers::list_tables,
            ipc_handlers::table_schema,
            ipc_handlers::query_table,
            ipc_handlers::insert_row,
            ipc_handlers::update_row,
            ipc_handlers::delete_row,
            ipc_handlers::cleanup_old_data,
            // ─── 外部数据库浏览/CRUD IPC ───
            ipc_handlers::scan_external_dbs,
            ipc_handlers::check_db_header,
            ipc_handlers::external_list_tables,
            ipc_handlers::external_table_schema,
            ipc_handlers::external_query_table,
            ipc_handlers::get_cell_value,
            ipc_handlers::write_file,
            ipc_handlers::get_table_detail,
            ipc_handlers::db_integrity,
            ipc_handlers::run_sql,
            ipc_handlers::table_stats,
            ipc_handlers::export_table_csv,
            ipc_handlers::backup_internal_db,
            ipc_handlers::restore_internal_db,
            ipc_handlers::get_security_status,
            // ─── 微信监控 IPC（从 wechat::handlers 注册） ───
            wechat::handlers::get_session_snapshots,
            wechat::handlers::get_session_list,
            wechat::handlers::refresh_wechat_sessions,
            wechat::handlers::get_conversation_messages,
            wechat::handlers::export_session_messages,
            wechat::handlers::get_user_avatar,
            wechat::handlers::get_contacts,
            wechat::handlers::get_contacts_by_category,
            wechat::handlers::get_contact_profile,
            wechat::handlers::get_moments_page,
            wechat::handlers::get_moments_insights,
            wechat::handlers::refresh_wechat_moments,
            wechat::handlers::get_moment_image,
            wechat::handlers::get_moment_video,
            wechat::handlers::get_favorites,
            wechat::handlers::get_favorite_detail,
            wechat::handlers::get_general_settings,
            wechat::handlers::export_general_category_csv,
            wechat::handlers::get_emoticons,
            wechat::handlers::get_static_emoticons,
            wechat::handlers::get_bizchats,
            wechat::handlers::get_official_accounts,
            wechat::handlers::get_resource_files,
            wechat::handlers::get_wechat_status,
            wechat::handlers::get_wechat_history,
            wechat::handlers::get_wechat_db_status,
            wechat::handlers::list_wechat_revokes,
            wechat::handlers::list_wechat_transfers,
            wechat::handlers::list_wechat_red_envelopes,
            wechat::handlers::list_wechat_finder,
            wechat::handlers::list_wechat_mini_programs,
            wechat::handlers::export_wechat_records_csv,
            wechat::handlers::list_wechat_friend_verifications,
            wechat::handlers::get_cdn_image_status,
            wechat::handlers::set_cdn_image_enabled,
            wechat::handlers::set_cdn_image_local_decrypt,
            wechat::handlers::open_wechat_folder,
            wechat::handlers::open_wechat_path,
            wechat::handlers::open_wechat_protocol,
            wechat::handlers::open_wechat_attach_folder,
            // ─── 微信原图 Hook IPC（img_helper.dll） ───
            wechat::hook::img_hook_start,
            wechat::hook::img_hook_stop,
            wechat::hook::img_hook_set_whitelist,
            wechat::hook::img_hook_status,
            // ─── 微信数据管理 IPC（删除/批量导出）───
            wechat::handlers::delete_conversation_messages,
            wechat::handlers::delete_favorite_items,
            wechat::handlers::clear_session_draft,
            wechat::handlers::clear_all_session_drafts,
            wechat::handlers::batch_export_sessions,
            wechat::handlers::export_contacts_csv,
            wechat::handlers::export_favorites_csv,
            wechat::handlers::export_moments,
            // ─── 微信消息编辑 / 全局搜索 / 年度总结 IPC ───
            wechat::handlers::get_chat_edit_status,
            wechat::handlers::list_session_edited_messages,
            wechat::handlers::edit_chat_message,
            wechat::handlers::reset_edited_message,
            wechat::handlers::get_message_raw_row,
            wechat::handlers::update_message_raw_fields,
            wechat::handlers::search_wechat_messages,
            wechat::handlers::build_wechat_search_index,
            wechat::handlers::get_wechat_search_index_status,
            wechat::handlers::get_chat_daily_counts,
            wechat::handlers::get_session_message_stats,
            wechat::handlers::get_annual_available_years,
            wechat::handlers::get_annual_summary,
            wechat::handlers::export_wechat_archive,
            wechat::handlers::import_wechat_backup,
            // ─── 微信每日总结 IPC ───
            wechat::handlers::list_daily_summary_tasks,
            wechat::handlers::save_daily_summary_task,
            wechat::handlers::delete_daily_summary_task,
            wechat::handlers::toggle_daily_summary_task,
            wechat::handlers::run_daily_summary_task,
            wechat::handlers::run_daily_summary_range,
            wechat::handlers::list_daily_summary_records,
            wechat::handlers::delete_daily_summary_record,
            wechat::handlers::get_daily_summary_formats,
            wechat::handlers::get_group_members,
            // ─── 微信配置与管理 IPC ───
            wechat::handlers::get_wechat_config,
            wechat::handlers::save_wechat_config,
            wechat::handlers::verify_database_key,
            wechat::handlers::generate_keys_file,
            wechat::handlers::decrypt_all_databases,
            wechat::handlers::verify_image_key,
            wechat::handlers::decode_all_images,
            wechat::handlers::detect_wechat_accounts,
            wechat::handlers::scan_wechat_accounts,
            wechat::handlers::get_wechat_keys_info,
            wechat::handlers::auto_get_db_key,
            wechat::handlers::auto_get_db_key_v2,
            wechat::handlers::auto_get_image_key,
            wechat::handlers::auto_get_wechat_keys,
            wechat::handlers::start_wechat_monitor,
            wechat::handlers::stop_wechat_monitor,
            wechat::handlers::get_wechat_monitor_status,
            wechat::handlers::ack_wechat_message,
            wechat::handlers::resync_wechat_messages,
            wechat::handlers::get_wechat_missing_images,
            wechat::handlers::export_wechat_missing_images_csv,
            wechat::handlers::get_wechat_account_status,
            wechat::handlers::switch_wechat_account_to_live,
            wechat::handlers::get_message_image,
            wechat::handlers::get_ilink_origin_status,
            wechat::handlers::get_wechat_storage_stats,
            wechat::handlers::get_wechat_data_overview,
            wechat::handlers::get_wechat_revoked_messages,
            wechat::handlers::get_message_voice,
            wechat::handlers::get_favorite_voice,
            wechat::handlers::transcribe_message_voice,
            #[cfg(feature = "local-stt")]
            stt::get_local_stt_status,
            #[cfg(feature = "local-stt")]
            stt::set_local_stt_config,
            #[cfg(feature = "local-stt")]
            stt::download_local_stt_model,
            wechat::handlers::resolve_wechat_file,
            wechat::handlers::get_favorite_image,
            wechat::handlers::get_api_settings,
            wechat::handlers::apply_api_settings,
            // ─── 微信数据 AI 问答（「问我的微信」）───
            wechat::ask::ask_wechat,
            // ─── 微信社交关系图谱 ───
            wechat::insights::get_relationship_graph,
            wechat::insights::get_relationship_graph_cached,
            wechat::graph_export::fetch_image_data_url,
            // ─── 微信数据隐私体检 ───
            wechat::privacy::scan_privacy_risks_cmd,
            // ─── 微信加密备份与恢复（备份管家）───
            wechat::backup::create_wechat_backup,
            wechat::backup::restore_wechat_backup,
            wechat::backup::list_wechat_backups,
            wechat::backup::delete_wechat_backup,
            // ─── 大模型管理 IPC ───
            llm::handlers::get_llm_config,
            llm::handlers::get_llm_config_path,
            llm::handlers::upsert_llm_provider,
            llm::handlers::delete_llm_provider,
            llm::handlers::set_llm_default_provider,
            llm::handlers::test_llm_connection,
            llm::handlers::list_llm_models,
            llm::handlers::add_llm_model,
            llm::handlers::remove_llm_model,
            llm::handlers::remove_llm_models,
            llm::handlers::set_llm_default_model,
            llm::handlers::chat_with_llm,
            llm::handlers::chat_with_llm_stream,
            llm::agent::chat_agent_stream,
            llm::agent::approve_agent_tool,
            llm::agent::reject_agent_tool,
            llm::agent::get_agent_tools,
            llm::agent_plugins::list_agent_plugins,
            llm::agent_plugins::save_agent_plugin,
            llm::agent_plugins::delete_agent_plugin,
            llm::agent_plugins::set_agent_plugin_enabled,
            llm::agent_plugins::submit_agent_tool_result,
            llm::handlers::generate_image,
            llm::handlers::generate_video,
            llm::handlers::create_speech,
            llm::handlers::transcribe_voice_audio,
            llm::handlers::synthesize_native_speech,
            llm::handlers::create_embedding,
            llm::handlers::rerank,
            llm::handlers::save_uploaded_file,
            llm::handlers::save_resource_from_url,
            llm::handlers::get_llm_usage,
            llm::handlers::reset_llm_usage,
            llm::handlers::get_llm_usage_summary,
            llm::handlers::get_llm_provider_types,
            llm::handlers::get_llm_chat_history,
            llm::handlers::append_llm_chat_messages,
            llm::handlers::clear_llm_chat_history,
            llm::handlers::save_agent_tool_steps,
            llm::handlers::get_agent_tool_steps,
            llm::agent::trust_agent_tool,
            llm::agent::clear_agent_trust,
            harness::session::harness_list_sessions,
            harness::session::harness_create_session,
            harness::session::harness_set_session_workspace,
            harness::session::harness_set_session_archived,
            harness::session::harness_set_session_order,
            harness::session::harness_swap_session_order,
            harness::session::harness_generate_title,
            harness::session::harness_session_lineage,
            harness::session::harness_rename_session,
            harness::session::harness_delete_session,
            harness::session::harness_session_events,
            harness::session::harness_display_messages,
            harness::session::harness_trajectory,
            harness::session::harness_turn_files,
            harness::fs::harness_open_path,
            harness::session::harness_fork_session,
            harness::session::harness_set_session_preset,
            harness::session::harness_set_session_role,
            harness::session::harness_get_session_role,
            harness::session::harness_clear_session,
            harness::session::harness_export_session,
            harness::portability::harness_export_bundle,
            harness::portability::harness_import_bundle,
            harness::jobs::harness_job_list,
            harness::jobs::harness_job_output,
            harness::jobs::harness_job_kill,
            harness::interaction::harness_answer_question,
            harness::storage::harness_storage_backends,
            harness::workspace::list_harness_workspaces,
            harness::workspace::create_harness_workspace,
            harness::workspace::delete_harness_workspace,
            harness::workspace::set_harness_workspace_status,
            harness::agent::harness_workflow_agent,
            harness::agent::harness_chat_stream,
            harness::agent::harness_cancel_turn,
            harness::agent::harness_goal_action,
            harness::tools::get_harness_tools,
            harness::approval::approve_harness_tool,
            harness::approval::reject_harness_tool,
            harness::approval::trust_harness_tool,
            harness::identity::get_harness_identity,
            harness::settings::get_harness_settings,
            harness::settings::save_harness_settings,
            harness::preset::list_harness_presets,
            harness::preset::save_harness_preset,
            harness::preset::delete_harness_preset,
            harness::preset::get_harness_scope,
            harness::hooks::list_harness_hooks,
            harness::hooks::save_harness_hooks,
            harness::session::harness_usage_summary,
            harness::session::harness_session_state,
            harness::agent::harness_execute_tool,
            harness::agent::harness_execute_tool_nolock,
            harness::schedule::list_harness_schedules,
            harness::schedule::save_harness_schedule,
            harness::schedule::delete_harness_schedule,
            harness::schedule::run_harness_schedule_now,
            harness::workflow::list_harness_workflows,
            harness::workflow::save_harness_workflow,
            harness::workflow::delete_harness_workflow,
            harness::workflow::run_harness_workflow,
            harness::subagent::harness_subagent_catalog,
            harness::fs::harness_fs_read,
            harness::fs::harness_fs_delete,
            harness::shell::harness_shell_run,
            harness::terminal::list_harness_terminals,
            harness::terminal::create_harness_terminal,
            harness::terminal::delete_harness_terminal,
            harness::terminal::harness_terminal_logs,
            harness::terminal::harness_terminal_send,
            harness::pty::harness_terminal_start_pty,
            harness::pty::harness_terminal_stop_pty,
            harness::pty::harness_terminal_send_pty,
            harness::pty::harness_terminal_resize_pty,
            harness::pty::harness_terminal_pty_status,
            harness::attachment::harness_attach_file,
            harness::attachment::harness_list_attachments,
            harness::mcp::list_harness_mcp_servers,
            harness::mcp::save_harness_mcp_servers,
            harness::skill::list_harness_skills,
            harness::skill::save_harness_skill,
            harness::skill::delete_harness_skill,
            harness::feedback::harness_submit_feedback,
            harness::feedback::harness_list_feedback,
            harness::storage::harness_kv_put,
            harness::storage::harness_kv_get,
            harness::storage::harness_kv_delete,
            harness::session::harness_search_sessions,
            harness::compaction::harness_list_spills,
            harness::compaction::harness_context_meter,
            harness::sdk::harness_cli,
            harness::credentials::harness_credential_list,
            harness::credentials::harness_credential_put,
            harness::credentials::harness_credential_delete,
            harness::lsp::list_harness_lsp_servers,
            harness::lsp::save_harness_lsp_servers,
            llm::handlers::set_last_chat,
            llm::handlers::set_llm_model_meta,
            // ─── AI 角色外部调用接口（跨模块，供全局调用检索）───
            ai_role::get_ai_roles,
            ai_role::get_ai_role,
            ai_role::save_ai_role,
            ai_role::delete_ai_role,
            // ─── 智能体管理 IPC ───
            agents::agent_list,
            agents::agent_get,
            agents::agent_create,
            agents::agent_update,
            agents::agent_delete,
            agents::agent_chat_stream,
            // ─── 知识库管理 IPC ───
            kb::handlers::kb_create,
            kb::handlers::kb_list,
            kb::handlers::kb_delete,
            kb::handlers::kb_update,
            kb::handlers::kb_set_pin,
            kb::handlers::kb_list_dirs,
            kb::handlers::kb_create_dir,
            kb::handlers::kb_rename_dir,
            kb::handlers::kb_delete_dir,
            kb::handlers::kb_upload_document,
            kb::handlers::kb_multimodal_analyze,
            kb::handlers::kb_upload_new_version,
            kb::handlers::kb_fetch_url,
            kb::handlers::kb_update_chunk,
            kb::handlers::kb_move_doc,
            kb::handlers::kb_rename_document,
            kb::handlers::kb_set_doc_tags,
            kb::handlers::kb_list_tags,
            kb::handlers::kb_faq_import,
            kb::handlers::kb_faq_list,
            kb::handlers::kb_faq_delete,
            kb::handlers::kb_search,
            kb::handlers::kb_rag,
            kb::handlers::kb_rag_stream,
            kb::handlers::kb_rag_cancel,
            kb::handlers::kb_highlight,
            kb::handlers::kb_list_versions,
            kb::handlers::kb_version_diff,
            kb::handlers::kb_set_acl,
            kb::handlers::kb_acl_delete,
            kb::handlers::kb_list_documents,
            kb::handlers::kb_get_document,
            kb::handlers::kb_delete_document,
            kb::handlers::kb_restore_version,
            kb::handlers::kb_download_document,
            kb::handlers::kb_batch_download,
            kb::handlers::kb_reprocess_document,
            kb::handlers::kb_get_acl,
            kb::handlers::kb_list_models,
            kb::handlers::kb_get_default_model,
            kb::handlers::kb_get_default_chat_model,
            kb::handlers::kb_get_model_settings,
            kb::handlers::kb_set_model_settings,
            kb::handlers::kb_get_chunk_settings,
            kb::handlers::kb_set_chunk_settings,
            kb::handlers::kb_get_rag_system_prompt,
            kb::handlers::kb_set_rag_system_prompt,
            kb::handlers::kb_test_model,
            kb::handlers::kb_export,
            kb::handlers::kb_import,
            kb::handlers::kb_batch_fetch_url,
            kb::handlers::kb_get_stats,
            kb::handlers::kb_get_analytics,
            kb::handlers::kb_track_event,
            kb::handlers::kb_recommend_questions,
            kb::handlers::kb_get_analytics_settings,
            kb::handlers::kb_set_analytics_settings,
            kb::handlers::kb_housekeeping,
            kb::handlers::kb_list_users,
            kb::handlers::kb_create_user,
            kb::handlers::kb_change_password,
            kb::handlers::kb_delete_user,
            kb::handlers::kb_reset_password,
            kb::handlers::kb_set_admin,
            kb::handlers::kb_list_roles,
            kb::handlers::kb_list_members,
            kb::handlers::kb_add_member,
            kb::handlers::kb_remove_member,
            kb::handlers::kb_update_member_role,
            kb::handlers::kb_qa_create_session,
            kb::handlers::kb_qa_list_sessions,
            kb::handlers::kb_qa_list_messages,
            kb::handlers::kb_qa_delete_session,
            kb::handlers::kb_search_history,
            kb::handlers::kb_list_jobs,
            kb::handlers::kb_get_job_logs,
            kb::handlers::kb_clear_activity,
            kb::handlers::kb_stop_processing,
            kb::handlers::kb_retry_job,
            kb::handlers::kb_retry_failed_jobs,
            kb::handlers::kb_wiki_list_pages,
            kb::handlers::kb_wiki_dirs,
            kb::handlers::kb_wiki_search,
            kb::handlers::kb_wiki_get_page,
            kb::handlers::kb_wiki_graph,
            kb::handlers::kb_wiki_create_page,
            kb::handlers::kb_wiki_update_page,
            kb::handlers::kb_wiki_delete_page,
            kb::handlers::kb_wiki_generate,
            kb::handlers::kb_wiki_extract,
            kb::handlers::kb_wiki_extract_all,
            kb::handlers::kb_wiki_list_versions,
            kb::handlers::kb_wiki_restore_version,
            kb::handlers::kb_backup,
            kb::handlers::kb_list_backups,
            kb::handlers::kb_cleanup_backups,
            kb::handlers::kb_list_audit_logs,
            kb::auth::kb_login,
            kb::auth::kb_logout,
            kb::auth::kb_current_user,
        ])
        .run(tauri::generate_context!())
        .expect("启动 ST Control 失败");
}

/// 原有的服务端事件转发
async fn forward_events_to_frontend(
    app: AppHandle,
    mut event_rx: tokio::sync::broadcast::Receiver<String>,
) {
    log::info!("事件转发器已启动，等待服务器事件...");

    // 前端仍实时收到事件；DB 写入改为批量（满 50 条或 1s 触发一次），
    // 经 spawn_blocking 落库，避免高频事件逐条阻塞 tokio worker。
    const BATCH_MAX: usize = 50;
    const BATCH_FLUSH_MS: u64 = 1000;

    let mut pending: Vec<crate::EventLog> = Vec::with_capacity(BATCH_MAX);
    let mut flush_tick = tokio::time::interval(std::time::Duration::from_millis(BATCH_FLUSH_MS));
    flush_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let flush = |pending: &mut Vec<crate::EventLog>, app: &AppHandle| {
        if pending.is_empty() {
            return;
        }
        let events = std::mem::take(pending);
        let app2 = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Some(database) = app2.try_state::<db::Database>() {
                if let Err(e) = database.insert_events_batch(&events) {
                    log::warn!("批量持久化事件到数据库失败: {}", e);
                }
            }
        });
    };

    loop {
        tokio::select! {
            _ = flush_tick.tick() => {
                flush(&mut pending, &app);
            }
            recv = event_rx.recv() => {
                match recv {
                    Ok(payload) => {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                            let ts    = val.get("timestamp").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let etype = val.get("event_type").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let src   = val.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let title = val.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let det   = val.get("detail").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let lvl   = val.get("level").and_then(|v| v.as_str()).unwrap_or("info").to_string();
                            pending.push(crate::EventLog {
                                id: 0,
                                timestamp: ts,
                                event_type: etype,
                                source: src,
                                title,
                                detail: det,
                                level: lvl,
                            });
                            if pending.len() >= BATCH_MAX {
                                flush(&mut pending, &app);
                            }
                        }
                        if let Err(e) = app.emit(EVENT_SERVER_EVENT, &payload) {
                            log::error!("转发事件到前端失败: {}", e);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        flush(&mut pending, &app);
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("事件接收滞后，跳过 {} 条", n);
                    }
                }
            }
        }
    }
}
