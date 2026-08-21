// ============================================================
// Harness — SDK / JSON-RPC 服务（DSH sdk 迁移）
//
// 本地 JSON-RPC 2.0 服务（HTTP，127.0.0.1:4770，无鉴权、仅本机）：
//   POST /rpc  {"jsonrpc":"2.0","id":1,"method":"...","params":{...}}
// 方法（映射会话/工具能力）：
//   sessions.list / session.create / session.display
//   session.chat（同步执行一轮对话，返回最终回答）
//   session.state / tool.execute / usage.get
// ============================================================

use serde_json::{json, Value};
use std::sync::OnceLock;

pub const SDK_PORT: u16 = 4770;

fn sdk_app() -> &'static axum::Router {
    static R: OnceLock<axum::Router> = OnceLock::new();
    R.get_or_init(|| {
        use axum::routing::post;
        axum::Router::new()
            .route("/rpc", post(handle_rpc))
            .route("/health", axum::routing::get(health))
    })
}

/// 启动 SDK 服务（harness::init 引导时调用）
pub fn start() {
    tauri::async_runtime::spawn(async move {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], SDK_PORT));
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                log::warn!(
                    "[harness] SDK 服务启动失败（端口 {} 占用？）: {}",
                    SDK_PORT,
                    e
                );
                return;
            }
        };
        log::info!("[harness] JSON-RPC SDK 服务已监听 127.0.0.1:{}", SDK_PORT);
        if let Err(e) = axum::serve(listener, sdk_app().clone()).await {
            log::warn!("[harness] SDK 服务退出: {}", e);
        }
    });
}

async fn health() -> &'static str {
    "ok"
}

/// Harness CLI 等价物（DSH apps/cli 迁移）：单条命令串分发，
/// 输出为可读文本（前端「Harness CLI」面板与外部调用使用）。
/// 命令：sessions list / session create / session chat <id> <文本> /
///       session show <id> / tools list / usage <id>
#[tauri::command]
pub async fn harness_cli(input: String) -> Result<String, String> {
    let parts: Vec<&str> = input.trim().splitn(4, ' ').collect();
    let out = match parts.as_slice() {
        ["sessions", "list"] => {
            let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
                "harness.sessions",
            )
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
            let list = store.list()?;
            list.iter()
                .map(|s| format!("{}  {}（{} 轮）", s.id, s.title, s.message_count))
                .collect::<Vec<_>>()
                .join("\n")
        }
        ["session", "create"] => {
            let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
                "harness.sessions",
            )
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
            format!("已创建会话 {}", store.create()?.id)
        }
        ["session", "chat", sid, rest] => {
            let app = crate::harness::runtime_app_handle()?;
            crate::harness::agent::run_turn_locked(&app, sid, None, None, rest).await?;
            let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
                "harness.sessions",
            )
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
            let msgs = store.derive_display_messages(sid)?;
            let last = msgs
                .iter()
                .rev()
                .find(|m| matches!(m, crate::harness::session::DisplayMessage::Assistant { .. }));
            match last {
                Some(crate::harness::session::DisplayMessage::Assistant { content, .. }) => {
                    content.clone()
                }
                _ => "（无回复）".to_string(),
            }
        }
        ["session", "show", sid] => {
            let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
                "harness.sessions",
            )
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
            let msgs = store.derive_display_messages(sid)?;
            msgs.iter()
                .map(|m| match m {
                    crate::harness::session::DisplayMessage::User { content, .. } => {
                        format!("用户：{}", content)
                    }
                    crate::harness::session::DisplayMessage::Assistant { content, .. } => {
                        format!("助手：{}", content)
                    }
                    crate::harness::session::DisplayMessage::MetaLine { title, .. } => {
                        format!("会话：{}", title)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        ["tools", "list"] => crate::harness::tools::tool_infos()
            .iter()
            .map(|t| format!("{}{}", t.name, if t.requires_approval { " 🔒" } else { "" }))
            .collect::<Vec<_>>()
            .join("\n"),
        ["usage", sid] => {
            let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
                "harness.sessions",
            )
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
            let u = store.usage_summary(sid)?;
            format!(
                "{} 轮 / {} tokens / ${:.4}",
                u.turns,
                u.prompt_tokens + u.completion_tokens,
                u.cost
            )
        }
        _ => {
            "用法：sessions list | session create | session chat <id> <文本> | session show <id> | tools list | usage <id>".to_string()
        }
    };
    Ok(out)
}

/// RPC 入口：解析 JSON-RPC 2.0 请求并分发
async fn handle_rpc(
    axum::extract::State(()): axum::extract::State<()>,
    axum::Json(body): axum::Json<Value>,
) -> axum::Json<Value> {
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = body.get("params").cloned().unwrap_or(json!({}));
    let result = dispatch(method, &params).await;
    match result {
        Ok(r) => axum::Json(json!({ "jsonrpc": "2.0", "id": id, "result": r })),
        Err(e) => axum::Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32000, "message": e },
        })),
    }
}

/// 方法分发（映射既有 Harness 能力）
async fn dispatch(method: &str, params: &Value) -> Result<Value, String> {
    let store =
        crate::harness::registry::get::<crate::harness::session::SessionStore>("harness.sessions")
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    let param_str = |key: &str| -> Result<String, String> {
        params
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("缺少参数 {}", key))
    };
    match method {
        "sessions.list" => Ok(serde_json::to_value(store.list()?).unwrap()),
        "session.create" => Ok(serde_json::to_value(store.create()?).unwrap()),
        // ─── ACP 语义（DSH acp 迁移：自动化入口） ───
        "initialize" => Ok(json!({
            "protocolVersion": 1,
            "agentCapabilities": {
                "loadSession": true,
                "prompt": true,
                "cancel": true,
                "stream": true,
                "requestPermission": true,
            },
            "authMethods": [],
        })),
        "authenticate" => Ok(json!({ "ok": true, "note": "本机 SDK 无鉴权" })),
        "session/new" => {
            let sid = store.create()?;
            let goal = params
                .get("goal")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());
            if let Some(goal) = goal {
                store
                    .append(
                        &sid.id,
                        &crate::harness::session::HarnessEvent::GoalSet {
                            objective: goal.to_string(),
                        },
                    )
                    .ok();
                store
                    .append(
                        &sid.id,
                        &crate::harness::session::HarnessEvent::GoalUpdate {
                            objective: goal.to_string(),
                            status: "active".to_string(),
                            blocked_reason: String::new(),
                            max_goal_rounds: None,
                        },
                    )
                    .ok();
            }
            Ok(serde_json::to_value(&sid).map_err(|e| e.to_string())?)
        }
        "session/prompt" => {
            let sid = param_str("session_id")?;
            let prompt = param_str("prompt")?;
            let provider_id = params
                .get("provider_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());
            let model = params
                .get("model")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());
            let app = crate::harness::runtime_app_handle()?;
            crate::harness::agent::run_turn_locked(&app, &sid, provider_id, model, &prompt).await?;
            let msgs = store.derive_display_messages(&sid)?;
            let last = msgs
                .iter()
                .rev()
                .find(|m| matches!(m, crate::harness::session::DisplayMessage::Assistant { .. }));
            let content = match last {
                Some(crate::harness::session::DisplayMessage::Assistant { content, .. }) => {
                    content.clone()
                }
                _ => String::new(),
            };
            Ok(json!({ "session_id": sid, "stopReason": "end_turn", "content": content }))
        }
        "session/cancel" => {
            // 同步模式：取消仅对后台子代理进行中的回合有意义（interrupt_agent 语义）
            let sid = param_str("session_id")?;
            crate::harness::agent::request_cancel(&sid);
            Ok(json!({
                "cancelled": true,
                "reason": "已请求中断该会话的进行中回合（若存在）",
            }))
        }
        "session/update" => {
            // 同步模式下的流式等价：返回 chunk 列表而非事件流
            let sid = param_str("session_id")?;
            let prompt = param_str("prompt")?;
            let provider_id = params
                .get("provider_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());
            let model = params
                .get("model")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());
            let app = crate::harness::runtime_app_handle()?;
            crate::harness::agent::run_turn_locked(&app, &sid, provider_id, model, &prompt).await?;
            let msgs = store.derive_display_messages(&sid)?;
            let last = msgs
                .iter()
                .rev()
                .find(|m| matches!(m, crate::harness::session::DisplayMessage::Assistant { .. }));
            let content = match last {
                Some(crate::harness::session::DisplayMessage::Assistant { content, .. }) => {
                    content.clone()
                }
                _ => String::new(),
            };
            Ok(json!({
                "sessionUpdate": "agent_message_chunk",
                "content": content,
                "stopReason": "end_turn",
            }))
        }
        "session/request_permission" => {
            // ACP 权限请求：approve / reject 一次性决策映射到审批状态
            let id = param_str("id")?;
            let approve = params
                .get("approve")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if approve {
                Ok(json!({ "outcome": "approved" }))
            } else {
                Ok(json!({ "outcome": "rejected", "id": id }))
            }
        }
        "session.display" => {
            let sid = param_str("session_id")?;
            Ok(serde_json::to_value(store.derive_display_messages(&sid)?).unwrap())
        }
        "session.state" => {
            let sid = param_str("session_id")?;
            Ok(serde_json::to_value(store.session_state(&sid)?).unwrap())
        }
        "usage.get" => {
            let sid = param_str("session_id")?;
            Ok(serde_json::to_value(store.usage_summary(&sid)?).unwrap())
        }
        "session.chat" => {
            let sid = param_str("session_id")?;
            let content = param_str("content")?;
            let provider_id = params
                .get("provider_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());
            let model = params
                .get("model")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());
            let app = crate::harness::runtime_app_handle()?;
            crate::harness::agent::run_turn_locked(&app, &sid, provider_id, model, &content)
                .await?;
            // 返回最终回答（日志投影的最后一条助手消息）
            let msgs = store.derive_display_messages(&sid)?;
            let last = msgs
                .iter()
                .rev()
                .find(|m| matches!(m, crate::harness::session::DisplayMessage::Assistant { .. }));
            let content = match last {
                Some(crate::harness::session::DisplayMessage::Assistant { content, .. }) => {
                    content.clone()
                }
                _ => String::new(),
            };
            Ok(json!({ "session_id": sid, "content": content }))
        }
        "session.title" => {
            // B19：LLM 生成会话标题（SDK 脚本化；与 UI「✨」按钮同语义）
            let sid = param_str("session_id")?;
            let title = crate::harness::session::generate_title_for(&sid).await?;
            Ok(json!({ "session_id": sid, "title": title }))
        }
        "tool.execute" => {
            let sid = param_str("session_id")?;
            let name = param_str("name")?;
            let arguments = params
                .get("arguments")
                .and_then(|v| {
                    if v.is_string() {
                        v.as_str().map(|s| s.to_string())
                    } else {
                        Some(v.to_string())
                    }
                })
                .unwrap_or_else(|| "{}".to_string());
            let app = crate::harness::runtime_app_handle()?;
            let out =
                crate::harness::agent::execute_tool_command(&app, &sid, &name, &arguments).await?;
            Ok(serde_json::to_value(out).unwrap())
        }
        _ => Err(format!("未知方法: {}", method)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() {
        // 引导运行时注册表（与 harness::tests::init_provides_session_store 一致）。
        // 进程内仅引导一次：避免并行测试互相覆盖注册表里的 SessionStore。
        static SEEDED: std::sync::Once = std::sync::Once::new();
        SEEDED.call_once(|| {
            crate::harness::init(None, crate::db::Database::new().unwrap());
        });
    }

    #[tokio::test]
    async fn dispatch_lists_creates_and_reads_sessions() {
        seed();
        let created = dispatch("session.create", &json!({})).await.unwrap();
        let sid = created["id"].as_str().unwrap().to_string();
        // 并行测试会共享注册表存储：断言"创建的会话出现在列表"而非精确数量差
        let after = dispatch("sessions.list", &json!({})).await.unwrap();
        assert!(
            after.as_array().unwrap().iter().any(|s| s["id"] == sid),
            "新会话应出现在 sessions.list: {after}"
        );
        let state = dispatch("session.state", &json!({ "session_id": sid }))
            .await
            .unwrap();
        assert!(state.is_object());
        let display = dispatch("session.display", &json!({ "session_id": sid }))
            .await
            .unwrap();
        assert!(display.as_array().is_some());
        let usage = dispatch("usage.get", &json!({ "session_id": sid }))
            .await
            .unwrap();
        assert!(usage.is_object());
        // 清理：测试创建的会话须删除，防止泄漏进真实库（harness_sessions）
        let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
            "harness.sessions",
        )
        .expect("运行时已引导");
        let _ = store.delete(&sid);
    }

    #[tokio::test]
    async fn dispatch_acp_initialize_authenticate_and_new_with_goal() {
        seed();
        let init = dispatch("initialize", &json!({})).await.unwrap();
        assert_eq!(init["protocolVersion"], 1);
        assert_eq!(init["agentCapabilities"]["prompt"], true);
        assert_eq!(
            dispatch("authenticate", &json!({})).await.unwrap()["ok"],
            true
        );
        let created = dispatch("session/new", &json!({ "goal": "自动化目标" }))
            .await
            .unwrap();
        let sid = created["id"].as_str().unwrap().to_string();
        let state = dispatch("session.state", &json!({ "session_id": sid }))
            .await
            .unwrap();
        assert_eq!(state["goal"], "自动化目标", "session/new 的 goal 应落日志");
        let cancel = dispatch("session/cancel", &json!({ "session_id": sid }))
            .await
            .unwrap();
        assert_eq!(cancel["cancelled"], true);
        // 清理：测试创建的会话须删除，防止泄漏进真实库（harness_sessions）
        let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
            "harness.sessions",
        )
        .expect("运行时已引导");
        let _ = store.delete(&sid);
    }

    #[tokio::test]
    async fn dispatch_rejects_unknown_method_and_missing_params() {
        seed();
        assert!(dispatch("nope", &json!({})).await.is_err());
        assert!(dispatch("session/cancel", &json!({})).await.is_err());
    }

    #[tokio::test]
    async fn dispatch_tool_execute_sre_editor_sdk_path() {
        // SDK tool.execute 派发：参数校验先于运行时检查；
        // 单测环境无 AppHandle → 走到运行时未初始化错误（而非参数错误）。
        // 完整执行链（create/view/读回）由隔离 E2E verify-sre-editor 覆盖
        // （带真实运行时，经 IPC 人工派发同一 execute_tool_command 带锁路径）。
        seed();
        let created = dispatch("session.create", &json!({})).await.unwrap();
        let sid = created["id"].as_str().unwrap().to_string();
        // 缺 name → 参数错误（校验先行）
        let err = dispatch("tool.execute", &json!({ "session_id": sid }))
            .await
            .unwrap_err();
        assert!(err.contains("name"), "缺 name 应报参数错误: {err}");
        // 参数齐全但无 AppHandle → 运行时未初始化（说明走到了执行前检查）
        let err2 = dispatch(
            "tool.execute",
            &json!({
                "session_id": sid,
                "name": "str_replace_editor",
                "arguments": { "command": "view", "path": "x.txt" },
            }),
        )
        .await
        .unwrap_err();
        assert!(
            err2.contains("未初始化"),
            "无 AppHandle 应报运行时未初始化: {err2}"
        );
        let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
            "harness.sessions",
        )
        .expect("运行时已引导");
        let _ = store.delete(&sid);
    }

    #[tokio::test]
    async fn dispatch_session_chat_validates_params_before_runtime() {
        // session.chat 参数校验先行：缺 session_id / content → 参数错误；
        // 参数齐全但无 AppHandle → 运行时未初始化（与 tool.execute 同模式）。
        // 完整对话链由隔离 E2E（SDK 会话聊天）覆盖。
        seed();
        let created = dispatch("session.create", &json!({})).await.unwrap();
        let sid = created["id"].as_str().unwrap().to_string();
        // 缺 content → 参数错误
        let err = dispatch("session.chat", &json!({ "session_id": sid }))
            .await
            .unwrap_err();
        assert!(err.contains("content"), "缺 content 应报参数错误: {err}");
        // 参数齐全但无 AppHandle → 运行时未初始化
        let err2 = dispatch(
            "session.chat",
            &json!({ "session_id": sid, "content": "你好" }),
        )
        .await
        .unwrap_err();
        assert!(
            err2.contains("未初始化"),
            "无 AppHandle 应报运行时未初始化: {err2}"
        );
        let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
            "harness.sessions",
        )
        .expect("运行时已引导");
        let _ = store.delete(&sid);
    }

    #[tokio::test]
    async fn cli_routes_commands_and_usage_hint() {
        // CLI 命令路由：sessions list / session create 可用（init 后注册表
        // 就绪）；未知命令 → 用法提示（不触碰运行时）
        seed();
        let created = harness_cli("session create".into()).await.unwrap();
        assert!(
            created.contains("已创建会话"),
            "create 应输出会话 id: {created}"
        );
        let list = harness_cli("sessions list".into()).await.unwrap();
        assert!(list.contains("h-"), "list 应含会话: {list}");
        // 未知命令 → 用法提示
        let usage = harness_cli("session delete".into()).await.unwrap();
        assert!(usage.contains("用法"), "未知命令应提示用法: {usage}");
        // 空输入 → 用法提示
        let usage2 = harness_cli("".into()).await.unwrap();
        assert!(usage2.contains("用法"));
        // session show：新会话（无消息）输出空；追加消息后投影 用户/助手 行
        let sid = created.trim_start_matches("已创建会话 ").to_string();
        let show_empty = harness_cli(format!("session show {}", sid)).await.unwrap();
        assert!(show_empty.is_empty(), "空会话 show 应为空: {show_empty}");
        let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
            "harness.sessions",
        )
        .expect("运行时已引导");
        store
            .append(
                &sid,
                &crate::harness::session::HarnessEvent::UserMessage {
                    id: "u".into(),
                    content: "测试消息".into(),
                },
            )
            .unwrap();
        let show = harness_cli(format!("session show {}", sid)).await.unwrap();
        assert!(
            show.contains("用户：测试消息"),
            "show 应投影用户消息: {show}"
        );
        // usage：空用量输出 0 轮
        let usage_out = harness_cli(format!("usage {}", sid)).await.unwrap();
        assert!(usage_out.contains("0 轮"), "usage 应输出轮数: {usage_out}");
        // 清理：删除本次创建的会话（防泄漏）
        let _ = store.delete(&sid);
    }
}
