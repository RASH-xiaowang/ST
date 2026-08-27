// ============================================================
// 每日总结模块
// - 任务：为「某群聊 × 指定成员（单个/多个/全部）」配置每日定时总结
// - 定时触发后：读取该群聊前一天的聊天记录 → 调用所选模型生成总结
// - 结果汇总写入 SQLite：<st_result>/daily_summary.db
// ============================================================

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::modules::common;

mod crud;
pub use crud::*;
mod retrieve;
pub(crate) use retrieve::*;

#[allow(unused_imports)]
use chrono::TimeZone;

// ─── 数据库 ───

pub fn db_path() -> PathBuf {
    crate::wechat::config::default_st_result_dir().join("daily_summary.db")
}

fn connect() -> Result<Connection, String> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(&path).map_err(|e| format!("打开每日总结库失败: {}", e))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS summary_tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            group_username TEXT NOT NULL,
            group_name TEXT NOT NULL DEFAULT '',
            target_users TEXT NOT NULL DEFAULT '[]',
            provider_id TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            format TEXT NOT NULL DEFAULT 'brief',
            custom_prompt TEXT NOT NULL DEFAULT '',
            schedule_time TEXT NOT NULL DEFAULT '08:00',
            enabled INTEGER NOT NULL DEFAULT 1,
            last_run_at INTEGER,
            last_status TEXT NOT NULL DEFAULT '',
            last_error TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS summary_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id INTEGER NOT NULL,
            group_username TEXT NOT NULL,
            group_name TEXT NOT NULL DEFAULT '',
            target_users TEXT NOT NULL DEFAULT '[]',
            summary_date TEXT NOT NULL,
            provider_id TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            format TEXT NOT NULL DEFAULT 'brief',
            summary TEXT NOT NULL DEFAULT '',
            char_count INTEGER NOT NULL DEFAULT 0,
            message_count INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'done',
            error TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_records_task ON summary_records(task_id, summary_date);
        CREATE INDEX IF NOT EXISTS idx_tasks_enabled ON summary_tasks(enabled, schedule_time);",
    )
    .map_err(|e| format!("初始化每日总结库失败: {}", e))?;
    // 迁移：老库补充遥测字段（耗时 / token / 消息片段样例）
    for (col, ddl) in [
        ("duration_ms", "duration_ms INTEGER NOT NULL DEFAULT 0"),
        ("prompt_tokens", "prompt_tokens INTEGER NOT NULL DEFAULT 0"),
        (
            "completion_tokens",
            "completion_tokens INTEGER NOT NULL DEFAULT 0",
        ),
        ("total_tokens", "total_tokens INTEGER NOT NULL DEFAULT 0"),
        ("message_sample", "message_sample TEXT NOT NULL DEFAULT ''"),
    ] {
        ensure_column(&conn, "summary_records", col, ddl)?;
    }
    Ok(conn)
}

/// 幂等迁移：列不存在时追加
fn ensure_column(conn: &Connection, table: &str, col: &str, ddl: &str) -> Result<(), String> {
    let has = {
        let mut stmt = conn
            .prepare(&format!(
                "PRAGMA table_info(\"{}\")",
                table.replace('"', "")
            ))
            .map_err(|e| format!("读取表结构失败: {}", e))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| format!("读取表结构失败: {}", e))?;
        let mut found = false;
        for c in rows.flatten() {
            if c == col {
                found = true;
                break;
            }
        }
        found
    };
    if !has {
        conn.execute_batch(&format!(
            "ALTER TABLE \"{}\" ADD COLUMN {};",
            table.replace('"', ""),
            ddl
        ))
        .map_err(|e| format!("迁移列 {} 失败: {}", col, e))?;
    }
    Ok(())
}

// ─── 总结格式 ───

pub fn summary_formats() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "key": "brief", "label": "简洁总结",
            "desc": "3–5 句话概括当天聊天的重点内容",
            "prompt": "请用简洁的中文概括当天聊天记录的重点，3-5 句话以内，不要分点。"
        }),
        serde_json::json!({
            "key": "detailed", "label": "详细总结",
            "desc": "按主题分点，包含关键事件、话题与结论",
            "prompt": "请对当天聊天记录做详细总结：按主题分点（Markdown 列表），包含关键事件、讨论的话题、达成的共识与结论；只依据记录内容，不编造。"
        }),
        serde_json::json!({
            "key": "bullets", "label": "要点列表",
            "desc": "用要点列表提炼当天核心信息",
            "prompt": "请用 Markdown 无序列表提炼当天聊天记录的核心要点，每条一句话，控制在 10 条以内。"
        }),
        serde_json::json!({
            "key": "story", "label": "叙事总结",
            "desc": "以第三人称叙述当天交流的来龙去脉",
            "prompt": "请以第三人称、叙事的方式回顾当天聊天记录：谁和谁聊了什么、发生了什么、有什么进展或插曲，读起来像一篇日记。"
        }),
        serde_json::json!({
            "key": "custom", "label": "自定义格式",
            "desc": "使用自定义提示词模板（支持 {date} {group} {targets} 占位符）",
            "prompt": ""
        }),
    ]
}

// ─── 执行总结 ───

/// 昨天的日期字符串 YYYY-MM-DD
fn yesterday_str() -> String {
    (chrono::Local::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string()
}

fn day_range(date: &str) -> Option<(i64, i64)> {
    use chrono::TimeZone;
    let dt = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let start = chrono::Local
        .from_local_datetime(&dt.and_hms_opt(0, 0, 0)?)
        .earliest()?
        .timestamp();
    let end = chrono::Local
        .from_local_datetime(&(dt + chrono::Duration::days(1)).and_hms_opt(0, 0, 0)?)
        .earliest()?
        .timestamp();
    Some((start, end))
}

/// 多日范围（起止日期均为闭区间，按本地时区 00:00 划分）
fn day_range_multi(start: &str, end: &str) -> Result<(i64, i64), String> {
    use chrono::TimeZone;
    let sd = chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d")
        .map_err(|_| format!("开始日期格式错误: {}", start))?;
    let ed = chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d")
        .map_err(|_| format!("结束日期格式错误: {}", end))?;
    if ed < sd {
        return Err("结束日期不能早于开始日期".to_string());
    }
    if (ed - sd).num_days() > 366 {
        return Err("日期范围不能超过 366 天".to_string());
    }
    let start_ts = chrono::Local
        .from_local_datetime(
            &sd.and_hms_opt(0, 0, 0)
                .ok_or_else(|| "时间构造失败".to_string())?,
        )
        .earliest()
        .ok_or_else(|| "时间构造失败".to_string())?
        .timestamp();
    let end_ts = chrono::Local
        .from_local_datetime(
            &(ed + chrono::Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| "时间构造失败".to_string())?,
        )
        .earliest()
        .ok_or_else(|| "时间构造失败".to_string())?
        .timestamp();
    Ok((start_ts, end_ts))
}

/// 并发防重：正在执行的任务 id 集合
static RUNNING_TASKS: Mutex<Vec<i64>> = Mutex::new(Vec::new());

fn try_acquire(id: i64) -> bool {
    let mut guard = RUNNING_TASKS.lock().unwrap_or_else(|e| e.into_inner());
    if guard.contains(&id) {
        return false;
    }
    guard.push(id);
    true
}

fn release(id: i64) {
    if let Ok(mut guard) = RUNNING_TASKS.lock() {
        guard.retain(|x| *x != id);
    }
}

/// 执行一次总结任务（同步函数，内部 await LLM）
///
/// `range` 为 None 时总结前一天；Some((start, end)) 时总结该闭区间内的聊天记录。
pub async fn run_task(
    decrypted_dir: &Path,
    task: &SummaryTask,
    range: Option<(String, String)>,
) -> Result<SummaryRecord, String> {
    let (summary_date, start_ts, end_ts) = match range {
        Some((start, end)) => {
            let (st, en) = day_range_multi(&start, &end)?;
            (format!("{}~{}", start, end), st, en)
        }
        None => {
            let d = yesterday_str();
            let (st, en) = day_range(&d).ok_or_else(|| "日期解析失败".to_string())?;
            (d, st, en)
        }
    };

    // 1) 解析显示名（错误提示与记录行都要用）
    let names = super::annual::load_display_names(decrypted_dir, &[]);
    let targets_label = if task.target_users.is_empty() {
        "全部成员".to_string()
    } else {
        task.target_users
            .iter()
            .map(|u| names.get(u).cloned().unwrap_or_else(|| u.clone()))
            .collect::<Vec<_>>()
            .join("、")
    };

    // 2) 读取聊天记录
    let msgs = fetch_day_messages(
        decrypted_dir,
        &task.group_username,
        &task.target_users,
        start_ts,
        end_ts,
        1200,
    );
    if msgs.is_empty() {
        // 说明原因：区分“群聊当天没消息”与“关注成员当天没发言”
        let group_total =
            count_group_messages(decrypted_dir, &task.group_username, start_ts, end_ts);
        let who = if task.target_users.is_empty() {
            "群聊".to_string()
        } else {
            format!("关注成员（{}）", targets_label)
        };
        let hint = if group_total > 0 {
            format!("；当天群内共有 {} 条消息，但这些成员没有发言", group_total)
        } else {
            String::new()
        };
        return Err(format!(
            "{} 没有找到可总结的消息（群聊：{}，{}）{}",
            summary_date, task.group_name, who, hint
        ));
    }

    let mut lines = Vec::with_capacity(msgs.len());
    let mut text_count = 0usize;
    for m in &msgs {
        let sender = if m.sender.is_empty() {
            "未知成员".to_string()
        } else {
            names
                .get(&m.sender)
                .cloned()
                .unwrap_or_else(|| m.sender.clone())
        };
        lines.push(format!("{}: {}", sender, m.text));
        text_count += 1;
    }
    let transcript = lines.join("\n");
    // 消息片段样例：前 8 条（每条截断），供前端核对输入内容
    let message_sample = lines
        .iter()
        .take(8)
        .map(|l| {
            let mut s = l.as_str();
            if s.chars().count() > 60 {
                s = &s[..s.char_indices().nth(60).map(|(i, _)| i).unwrap_or(s.len())];
                format!("{}…", s)
            } else {
                s.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // 3) 解析模型配置
    let llm_cfg = crate::llm::config::load_config();
    let provider = crate::llm::config::find_provider(&llm_cfg, &task.provider_id)
        .cloned()
        .ok_or_else(|| format!("模型提供方不存在：{}", task.provider_id))?;
    if !provider.enabled {
        return Err(format!("模型提供方已被禁用：{}", provider.name));
    }
    let model = if task.model.is_empty() {
        provider.default_model.clone()
    } else {
        task.model.clone()
    };
    if model.is_empty() {
        return Err("任务未指定模型，且提供方未配置默认模型".to_string());
    }

    // 4) 组装提示词
    let fmt = summary_formats()
        .into_iter()
        .find(|f| f.get("key").and_then(|v| v.as_str()) == Some(task.format.as_str()))
        .unwrap_or_else(|| serde_json::json!({ "prompt": "请总结当天聊天记录。" }));
    let format_instruction = fmt
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let custom = if task.format == "custom" {
        task.custom_prompt
            .replace("{date}", &summary_date)
            .replace("{group}", &task.group_name)
            .replace("{targets}", &targets_label)
    } else {
        String::new()
    };
    let mut system_prompt = "你是一位专业的微信群聊每日总结助手。你只依据聊天记录内容进行客观总结，不编造、不补全、不评价成员隐私。输出使用中文。"
        .to_string();
    if task.format == "custom" && !custom.is_empty() {
        system_prompt = custom;
    } else {
        system_prompt.push('\n');
        system_prompt.push_str(&format_instruction);
    }
    let user_content = format!(
        "【群聊名称】{}\n【日期范围】{}\n【关注成员】{}\n【聊天记录（{} 条）】\n{}",
        task.group_name,
        summary_date.replace('~', " 至 "),
        targets_label,
        text_count,
        transcript
    );
    let messages = vec![
        crate::llm::types::ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
            parts: None,
        },
        crate::llm::types::ChatMessage {
            role: "user".to_string(),
            content: user_content,
            parts: None,
        },
    ];

    // 5) 调用模型
    let t0 = std::time::Instant::now();
    let (content, prompt_tokens, completion_tokens, total_tokens) =
        crate::llm::client::chat_completion(
            &provider,
            &crate::llm::client::CompletionParams {
                model: &model,
                messages: &messages,
                max_tokens: Some(2048),
                temperature: Some(0.4),
                top_p: None,
                presence_penalty: None,
                frequency_penalty: None,
                tools: None,
                tool_choice: None,
            },
        )
        .await
        .map_err(|e| format!("模型调用失败: {}", e))?;
    let duration_ms = t0.elapsed().as_millis() as i64;
    // 用量与成本已由 client::chat_completion 统一计入「大模型管理 → 流量与成本」

    Ok(SummaryRecord {
        id: 0,
        task_id: task.id,
        group_username: task.group_username.clone(),
        group_name: task.group_name.clone(),
        target_users: task.target_users.clone(),
        summary_date,
        provider_id: provider.id.clone(),
        model,
        format: task.format.clone(),
        summary: content.trim().to_string(),
        char_count: content.trim().chars().count() as i64,
        message_count: text_count as i64,
        status: "done".to_string(),
        error: String::new(),
        created_at: common::now_ms(),
        duration_ms,
        prompt_tokens: prompt_tokens as i64,
        completion_tokens: completion_tokens as i64,
        total_tokens: total_tokens as i64,
        message_sample,
    })
}

async fn execute_task_inner(
    id: i64,
    range: Option<(String, String)>,
) -> Result<SummaryRecord, String> {
    let task = get_task(id)?.ok_or_else(|| "任务不存在".to_string())?;
    let cfg =
        crate::wechat::config::WeChatConfig::load().map_err(|e| format!("读取配置失败: {}", e))?;
    let rec = run_task(&cfg.decrypted_dir, &task, range).await?;
    insert_record(&rec)?;
    update_task_run_state(id, true, "")?;
    Ok(rec)
}

/// 执行任务（手动触发或定时触发共用），返回生成记录
pub async fn execute_task(id: i64) -> Result<SummaryRecord, String> {
    if !try_acquire(id) {
        return Err("该任务正在执行中，请稍候".to_string());
    }
    let result = execute_task_inner(id, None).await;
    release(id);
    match result {
        Ok(rec) => Ok(rec),
        Err(e) => {
            let _ = update_task_run_state(id, false, &e);
            Err(e)
        }
    }
}

/// 按自定义日期范围执行总结（用于“立即总结历史聊天”）
pub async fn execute_task_range(
    id: i64,
    start_date: String,
    end_date: String,
) -> Result<SummaryRecord, String> {
    if !try_acquire(id) {
        return Err("该任务正在执行中，请稍候".to_string());
    }
    let range = Some((start_date, end_date));
    let result = execute_task_inner(id, range).await;
    release(id);
    match result {
        Ok(rec) => Ok(rec),
        Err(e) => {
            let _ = update_task_run_state(id, false, &e);
            Err(e)
        }
    }
}

/// 定时调度：每分钟检查一次是否有到点的任务
pub fn spawn_scheduler() {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        // 先触发一次立即 tick，避免等待一个完整周期
        interval.tick().await;
        loop {
            interval.tick().await;
            let now = chrono::Local::now();
            let hhmm = now.format("%H:%M").to_string();
            let today = now.format("%Y-%m-%d").to_string();
            let tasks = match list_tasks() {
                Ok(t) => t,
                Err(e) => {
                    log::warn!("[daily_summary] 读取任务失败: {}", e);
                    continue;
                }
            };
            for task in tasks {
                if !task.enabled || task.schedule_time != hhmm {
                    continue;
                }
                // 避免同一天重复执行（以 last_run_at 日期判断）
                let last_day = task
                    .last_run_at
                    .map(|ts| {
                        chrono::Local
                            .timestamp_opt(ts / 1000, 0)
                            .single()
                            .map(|dt| dt.format("%Y-%m-%d").to_string())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                if last_day == today {
                    continue;
                }
                let task_id = task.id;
                tauri::async_runtime::spawn(async move {
                    match execute_task(task_id).await {
                        Ok(rec) => log::info!(
                            "[daily_summary] 任务 {} 完成，{} 条消息，{} 字",
                            task_id,
                            rec.message_count,
                            rec.char_count
                        ),
                        Err(e) => log::warn!("[daily_summary] 任务 {} 失败: {}", task_id, e),
                    }
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    #[ignore = "需要真实 daily_summary.db"]
    fn smoke_daily_summary_db() {
        let conn = connect().expect("打开/迁移每日总结库");
        assert!(conn.is_autocommit());
        let tasks = list_tasks().expect("读取任务");
        let records = list_records(None).expect("读取记录");
        eprintln!("tasks={} records={}", tasks.len(), records.len());
        for t in &tasks {
            eprintln!(
                "task id={} group={} name={} targets={:?}",
                t.id, t.group_username, t.group_name, t.target_users
            );
            // 自愈修复：群名不应再是 JSON 数组垃圾值
            assert!(
                !t.group_name.trim_start().starts_with('['),
                "任务 {} 群名仍是垃圾 JSON: {}",
                t.id,
                t.group_name
            );
            if t.id == 2 {
                assert!(
                    !t.target_users.is_empty(),
                    "任务 2 关注成员读取为空（列映射 bug）"
                );
                assert_eq!(t.group_name, "黑龙江沃融-燎引擎", "任务 2 群名修复失败");
            }
        }
        for r in &records {
            assert!(!r.group_name.trim_start().starts_with('['));
        }
        for r in records.iter().take(2) {
            eprintln!(
                "record id={} date={} status={} msgs={} chars={} dur={} tokens={} sample_len={}",
                r.id,
                r.summary_date,
                r.status,
                r.message_count,
                r.char_count,
                r.duration_ms,
                r.total_tokens,
                r.message_sample.len()
            );
        }
        for r in &records {
            assert!(r.duration_ms >= 0);
            assert!(r.prompt_tokens >= 0);
            assert!(r.completion_tokens >= 0);
            assert!(r.total_tokens >= 0);
        }
    }

    /// 复现用户场景：关注成员当天没有发言时，报错要说明原因而非笼统的“没有消息”
    #[test]
    #[cfg(target_os = "windows")]
    #[ignore = "需要真实解密库"]
    fn empty_target_error_message() {
        let cfg = crate::wechat::config::WeChatConfig::load().expect("加载微信配置");
        let task = SummaryTask {
            id: -1,
            group_username: "45862433809@chatroom".to_string(),
            group_name: "黑龙江沃融-燎引擎".to_string(),
            target_users: vec!["wxid_umyqa86if3lm22".to_string()],
            provider_id: "unused".to_string(),
            model: "unused".to_string(),
            format: "brief".to_string(),
            custom_prompt: String::new(),
            schedule_time: "08:00".to_string(),
            enabled: true,
            last_run_at: None,
            last_status: String::new(),
            last_error: String::new(),
            created_at: 0,
            updated_at: 0,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("构建运行时");
        let err = rt
            .block_on(run_task(
                &cfg.decrypted_dir,
                &task,
                Some(("2000-01-01".to_string(), "2000-01-01".to_string())),
            ))
            .expect_err("应返回无消息错误");
        eprintln!("错误信息: {}", err);
        assert!(err.contains("没有找到可总结的消息"), "{}", err);
        assert!(err.contains("关注成员"), "{}", err);
        assert!(
            !err.contains("[\"wxid"),
            "错误里不应出现 JSON 群名: {}",
            err
        );
    }
}
