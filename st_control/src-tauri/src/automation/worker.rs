// ============================================================
// 自动化管理中心 — 内置 Worker 执行器
//
// 任务执行闭环（七步流水线的第 ④⑤ 步）：
//   轮询候选任务（pending + 派发给本地智能体的 claimed）→ 原子认领 →
//   分发执行：派发给本地智能体 → 按智能体配置执行（角色/模型/知识库），
//   否则规则绑定模型 → KB 检索 + 角色提示词 + LLM 执行 →
//   结果回写任务库（有 reply → to_reply 待回复队列；否则 done）。
//
// 与外部执行者（HTTP API claim / st_agent）天然互斥：双方都从
// pending/claimed 状态抢占，先到先得。processing 超时由 reaper 回收，
// 避免 Worker 崩溃后任务永久卡死。
// ============================================================

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::{AppHandle, Manager};

use super::db::{self, AutomationRule};

/// 任务执行上下文（消息快照，用于构建提示词）
struct TaskContext {
    id: i64,
    content: String,
    sender: String,
    username: String,
    rule_id: i64,
    /// 派发目标类型（agent=本地智能体 / agent_instance=已接入 Agent）
    target_type: String,
    /// 派发目标 id（智能体 id 或外部 Agent id）
    target_id: String,
}

/// 启动内置 Worker（幂等）
pub fn spawn_worker(app: AppHandle) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        // 启动宽限：等待控制台 DB 与微信监控就绪
        tokio::time::sleep(Duration::from_secs(3)).await;
        log::info!("[automation] 内置 Worker 启动（任务执行器就绪）");
        let mut last_session_exec: std::collections::HashMap<String, std::time::Instant> =
            std::collections::HashMap::new();
        loop {
            let (reaped, executed) = run_cycle(&app, &mut last_session_exec).await;
            if reaped > 0 || executed > 0 {
                log::info!(
                    "[automation] Worker 周期：回收 {reaped} 条超时任务，执行 {executed} 条"
                );
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}

async fn run_cycle(
    app: &AppHandle,
    last_session_exec: &mut std::collections::HashMap<String, std::time::Instant>,
) -> (usize, usize) {
    let db_path = super::control_db_path();
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => {
            c.execute_batch("PRAGMA busy_timeout=5000;").ok();
            c
        }
        Err(e) => {
            log::warn!("[automation] Worker 打开任务库失败: {e}");
            return (0, 0);
        }
    };
    // 1) 超时回收：processing 超过 5 分钟视为 Worker 崩溃遗留，回 pending；
    //    error 任务超过 10 分钟且重试未达 3 次 → 回 pending 自动重试
    let reaped = db::reap_stale_processing(&conn, 5).unwrap_or(0)
        + db::reap_retryable_errors(&conn, 3, 10).unwrap_or(0);
    // 2) 取候选任务（最多 5 条，pending 优先、按入队顺序）：
    //    - pending：派发给本地智能体（target_type='agent' 且有目标），或规则绑定了模型（provider_id 非空）
    //    - claimed：仅取派发给本地智能体的（「派发即执行」，与外部执行者先到先得）
    //    无模型且无智能体目标的 pending 任务留给外部 Agent（HTTP API）/ 人工，
    //    不进入候选，避免无效认领与长尾饥饿。
    let candidates: Vec<TaskContext> = {
        let mut stmt = match conn.prepare(
            "SELECT t.id, t.content, t.sender_username, t.username, t.rule_id, t.target_type, t.target_id
             FROM task_wechat_info t
             LEFT JOIN automation_rules r ON r.id = t.rule_id
             WHERE (t.status='pending'
                    AND ((t.target_type='agent' AND t.target_id != '') OR COALESCE(r.provider_id,'') != ''))
                OR (t.status='claimed' AND t.target_type='agent' AND t.target_id != '')
             ORDER BY CASE WHEN t.status='claimed' THEN 1 ELSE 0 END, t.id ASC
             LIMIT 5",
        ) {
            Ok(s) => s,
            Err(_) => return (reaped, 0),
        };
        let rows = stmt.query_map([], |r| {
            Ok(TaskContext {
                id: r.get(0)?,
                content: r.get::<_, String>(1).unwrap_or_default(),
                sender: r.get::<_, String>(2).unwrap_or_default(),
                username: r.get::<_, String>(3).unwrap_or_default(),
                rule_id: r.get(4)?,
                target_type: r.get::<_, String>(5).unwrap_or_default(),
                target_id: r.get::<_, String>(6).unwrap_or_default(),
            })
        });
        match rows {
            Ok(rs) => rs.flatten().collect(),
            Err(_) => return (reaped, 0),
        }
    };
    drop(conn);

    let mut executed = 0usize;
    for task in candidates {
        // 3) 原子认领（pending 或已派发给智能体的 claimed；与外部 agent 的 claim 互斥）
        let claim_conn = match rusqlite::Connection::open(&db_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let claimed = db::claim_task(&claim_conn, task.id).unwrap_or(false);
        drop(claim_conn);
        if !claimed {
            continue; // 已被外部执行者抢走
        }
        // 4) 加载规则：仅执行绑定了模型（provider_id 非空）的规则；
        //    无模型的固定派发任务留给外部 agent / 人工
        let rule = {
            let c2 = match rusqlite::Connection::open(&db_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            match db::get_rule(&c2, task.rule_id) {
                Ok(Some(r)) => r,
                Ok(None) => {
                    let _ = db::mark_failed(&c2, task.id, "规则已删除");
                    continue;
                }
                Err(e) => {
                    let _ = db::mark_failed(&c2, task.id, &e.to_string());
                    continue;
                }
            }
        };
        // 5) 分发：派发给本地智能体的任务按智能体配置执行（角色/模型/知识库），
        //    否则按规则模型执行；两者都没有则归还 pending，留给外部执行者
        let agent_target = if task.target_type == "agent" && !task.target_id.is_empty() {
            load_agent_target(app, &task.target_id)
        } else {
            None
        };
        if task.target_type == "agent" && !task.target_id.is_empty() && agent_target.is_none() {
            // 派发的智能体已删除：终止任务并记录原因，避免死循环重试
            if let Ok(c5) = rusqlite::Connection::open(&db_path) {
                let _ = db::mark_failed(
                    &c5,
                    task.id,
                    &format!("派发的智能体 #{} 不存在或已删除", task.target_id),
                );
            }
            continue;
        }
        if agent_target.is_none() && rule.provider_id.is_empty() {
            // 无模型规则：归还 pending，留给外部执行者
            if let Ok(c3) = rusqlite::Connection::open(&db_path) {
                let _ = c3.execute(
                    "UPDATE task_wechat_info SET status='pending',
                     updated_at=datetime('now','localtime') WHERE id=?1",
                    rusqlite::params![task.id],
                );
            }
            continue;
        }
        // 6) 同会话频控：60 秒内同一会话只自动执行一条
        let now = std::time::Instant::now();
        if let Some(last) = last_session_exec.get(&task.username) {
            if now.duration_since(*last).as_secs() < 60 {
                if let Ok(c4) = rusqlite::Connection::open(&db_path) {
                    let _ = db::mark_failed(
                        &c4,
                        task.id,
                        "同会话 60 秒频控内，已放回待处理（稍后自动重试）",
                    );
                    let _ = c4.execute(
                        "UPDATE task_wechat_info SET status='pending',
                         updated_at=datetime('now','localtime') WHERE id=?1",
                        rusqlite::params![task.id],
                    );
                }
                continue;
            }
        }

        // 7) 执行：智能体目标 → 智能体执行器；否则 KB 检索 + 角色提示词 + LLM
        let outcome = match &agent_target {
            Some(agent) => execute_as_agent(app, agent, &task).await,
            None => execute_task(app, &rule, &task).await,
        };
        match outcome {
            Ok((extract, reply)) => {
                if let Ok(c5) = rusqlite::Connection::open(&db_path) {
                    if let Some(reply_text) = reply {
                        let _ = db::update_task_reply(&c5, task.id, &reply_text, "to_reply");
                        log::info!(
                            "[automation] Worker 任务 {} 执行完成，回复进入待发队列",
                            task.id
                        );
                    } else {
                        let extract_str = serde_json::to_string(&extract).unwrap_or_default();
                        let _ = db::mark_done(&c5, task.id, &extract_str);
                        log::info!("[automation] Worker 任务 {} 执行完成（无回复）", task.id);
                    }
                }
                last_session_exec.insert(task.username.clone(), Instant::now());
            }
            Err(e) => {
                if let Ok(c6) = rusqlite::Connection::open(&db_path) {
                    let _ = db::mark_failed(&c6, task.id, &e);
                }
                log::warn!("[automation] Worker 任务 {} 执行失败: {e}", task.id);
            }
        }
        executed += 1;
    }
    (reaped, executed)
}

/// 按任务目标加载本地智能体（target_id 为 agents 表 id；不存在返回 None）
fn load_agent_target(app: &AppHandle, target_id: &str) -> Option<crate::agents::AgentItem> {
    let id: i64 = target_id.trim().parse().ok()?;
    let db = app.try_state::<crate::db::Database>()?;
    crate::agents::get_agent_by_id(&db, id).ok().flatten()
}

/// 按智能体配置执行任务：模型直接输出即回复文本（角色提示词决定话术），
/// 输出记入 ai_extract（{"reply": ...}）供面板查看。
async fn execute_as_agent(
    app: &AppHandle,
    agent: &crate::agents::AgentItem,
    task: &TaskContext,
) -> Result<(Value, Option<String>), String> {
    let Some(kb_db) = app.try_state::<crate::kb::db::KbDatabase>() else {
        return Err("知识库服务不可用".to_string());
    };
    let text = crate::agents::agent_execute(agent, &kb_db, &task.content).await?;
    if text.trim().is_empty() {
        return Err("模型返回为空".to_string());
    }
    let reply = text.trim().to_string();
    let extract = serde_json::json!({
        "reply": reply,
        "agent_id": agent.id,
        "agent_name": agent.name,
    });
    Ok((extract, Some(reply)))
}

/// 执行单个任务：知识库上下文 → 角色系统提示词 → LLM → (提取结果, 可选回复)
async fn execute_task(
    app: &AppHandle,
    rule: &AutomationRule,
    task: &TaskContext,
) -> Result<(Value, Option<String>), String> {
    // ── 1) 知识库检索：以消息内容为查询，取相关页面摘要作为上下文 ──
    let kb_context = collect_kb_context(app, &task.content).await;

    // ── 2) 角色提示词：规则绑定 role_id → roles.json 角色合成系统提示词 ──
    let role_prompt = if rule.role_id.trim().is_empty() {
        String::new()
    } else {
        crate::ai_role::get_ai_roles()
            .into_iter()
            .find(|r| r.id == rule.role_id && r.enabled)
            .map(|r| crate::ai_role::compose_system_prompt(&r))
            .unwrap_or_default()
    };

    // ── 3) 组装提示词 ──
    let task_instruction = if !rule.prompt_override.trim().is_empty() {
        rule.prompt_override.clone()
    } else {
        let fields: Vec<String> = rule
            .analyze_fields
            .iter()
            .map(|f| {
                if f.desc.is_empty() {
                    f.name.clone()
                } else {
                    format!("{}（{}）", f.name, f.desc)
                }
            })
            .collect();
        if fields.is_empty() {
            "理解消息意图并给出恰当的处理结果；如需回复消息，回复内容应简洁友好。".to_string()
        } else {
            format!(
                "提取以下业务字段：{}；如需回复消息请给出回复内容。",
                fields.join("、")
            )
        }
    };
    let mut system = String::new();
    if !role_prompt.is_empty() {
        system.push_str(&role_prompt);
        system.push_str("\n\n");
    }
    system.push_str(
        "你是消息自动化执行助手。根据消息内容完成任务，只输出 JSON：\n\
         {\"task\":\"任务名或意图\", \"fields\":{...字段...}, \"reply\":\"回复内容或 null\"}\n\
         不需要回复时 reply 为 null；只输出 JSON，不要多余文字。",
    );
    system.push_str(&format!("\n任务说明：{task_instruction}"));

    let mut user = format!(
        "发送人：{}\n会话：{}\n消息内容：{}\n时间：{}",
        task.sender,
        task.username,
        truncate_str(&task.content, 2000),
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
    );
    if !kb_context.is_empty() {
        user.push_str("\n\n【知识库参考】\n");
        user.push_str(&kb_context);
    }

    let text = super::engine::llm_chat(rule, &system, &user).await?;
    if text.trim().is_empty() {
        return Err("模型返回为空".to_string());
    }
    // JSON 解析兜底：模型不按格式输出时，把整段文本作为任务结论记录，
    // 而不是整体判失败（保证任务有产出、链路可观测）
    let extract = match super::engine::parse_json_response(&text) {
        Ok(v) => v,
        Err(_) => serde_json::json!({
            "task": truncate_str(text.trim(), 300),
            "fields": {},
            "reply": null,
        }),
    };
    let reply = super::engine::extract_reply(&extract);
    Ok((extract, reply))
}

/// 从知识库检索与消息相关的页面摘要（优先系统知识库，最多 3 个库、每库 3 条）
async fn collect_kb_context(app: &AppHandle, query: &str) -> String {
    let q = query.trim();
    if q.is_empty() || q.chars().count() > 200 {
        return String::new();
    }
    let Some(db) = app.try_state::<crate::kb::db::KbDatabase>() else {
        return String::new();
    };
    // 阻塞线程池执行 SQLite 检索（连接池 + FTS，不能阻塞异步运行时）
    let db = db.inner().clone();
    let query = q.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db.conn_lock();
        let kb_ids: Vec<i64> = {
            let mut stmt = match conn
                .prepare("SELECT id FROM knowledge_bases ORDER BY is_system DESC, id ASC LIMIT 3")
            {
                Ok(s) => s,
                Err(_) => return String::new(),
            };
            let rows = stmt.query_map([], |r| r.get::<_, i64>(0));
            match rows {
                Ok(rs) => rs.flatten().collect(),
                Err(_) => return String::new(),
            }
        };
        drop(conn);
        let mut parts: Vec<String> = Vec::new();
        for kb_id in kb_ids {
            if let Ok(pages) = crate::kb::wiki::search_pages(&db, kb_id, &query, 3) {
                for p in pages.into_iter().take(3) {
                    let summary = p.summary.trim();
                    if !summary.is_empty() {
                        parts.push(format!(
                            "《{}》{}",
                            p.title,
                            crate::common::truncate(summary, 160)
                        ));
                    }
                }
            }
        }
        parts.join("\n")
    })
    .await
    .unwrap_or_default()
}

fn truncate_str(s: &str, n: usize) -> String {
    crate::common::truncate(s, n)
}
