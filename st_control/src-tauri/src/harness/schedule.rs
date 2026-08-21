// ============================================================
// Harness — 定时任务（DSH schedule 迁移）
//
// 定时条目：按固定间隔（分钟）在指定会话自动发送提示词并执行一轮
// 代理对话（复用 agent 循环；结果照常落日志/用量/钩子）。
// 定时器每 30 秒检查一次到期条目；提供「立即运行」手动触发。
// 持久化：data/harness/schedule.json（原子写）。
// ============================================================

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HarnessSchedule {
    pub id: String,
    pub name: String,
    /// 目标会话 id
    pub session_id: String,
    /// 每轮发送的提示词
    pub prompt: String,
    /// 间隔（分钟，1~10080）
    pub interval_minutes: u64,
    pub enabled: bool,
    /// 一次性任务（DSH after_seconds 语义：执行一次后自动停用）
    #[serde(default)]
    pub one_shot: bool,
    /// 下次运行时间（Unix 秒）
    pub next_run_at: u64,
    pub last_run_at: Option<u64>,
    pub created_at: String,
}

fn schedule_path() -> std::path::PathBuf {
    crate::common::st_data_dir()
        .join("harness")
        .join("schedule.json")
}

fn schedule_store() -> &'static Mutex<Vec<HarnessSchedule>> {
    static S: OnceLock<Mutex<Vec<HarnessSchedule>>> = OnceLock::new();
    S.get_or_init(|| {
        let list = std::fs::read_to_string(schedule_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Mutex::new(list)
    })
}

fn persist(list: &[HarnessSchedule]) -> Result<(), String> {
    let path = schedule_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建定时目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(list).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换失败: {}", e))?;
    Ok(())
}

pub(crate) fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 启动调度器（lib.rs 引导时调用）：每 30 秒检查到期条目
pub fn start(app: tauri::AppHandle) {
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let due: Vec<HarnessSchedule> = schedule_store()
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.enabled && s.next_run_at <= now_unix())
                .cloned()
                .collect();
            for s in due {
                run_due(&app2, s).await;
            }
        }
    });
}

/// 执行一个到期条目：追加用户消息 + 运行一轮代理对话
async fn run_due(app: &tauri::AppHandle, mut s: HarnessSchedule) {
    log::info!(
        "[harness] 定时任务「{}」触发（会话 {}）",
        s.name,
        s.session_id
    );
    // 会话级互斥：与用户聊天/SDK 调用串行化，防止并发写日志（H3）
    let _ = crate::harness::agent::run_turn_locked(app, &s.session_id, None, None, &s.prompt).await;
    // 更新下次运行时间与最后运行时间；一次性任务执行后自动停用
    {
        let mut list = schedule_store().lock().unwrap();
        if let Some(entry) = list.iter_mut().find(|x| x.id == s.id) {
            entry.last_run_at = Some(now_unix());
            if entry.one_shot {
                entry.enabled = false;
                s.next_run_at = entry.next_run_at;
            } else {
                entry.next_run_at = now_unix() + entry.interval_minutes * 60;
                s.next_run_at = entry.next_run_at;
            }
        }
    }
    let _ = persist(&schedule_store().lock().unwrap().clone());
    log::info!(
        "[harness] 定时任务「{}」完成，下次 {}",
        s.name,
        s.next_run_at
    );
}

// ─── IPC ───

#[tauri::command]
pub async fn list_harness_schedules() -> Result<Vec<HarnessSchedule>, String> {
    Ok(schedule_store().lock().unwrap().clone())
}

/// 新建或更新定时条目（id 为空 → 新建）
#[tauri::command]
pub async fn save_harness_schedule(schedule: HarnessSchedule) -> Result<HarnessSchedule, String> {
    if schedule.name.trim().is_empty() {
        return Err("定时任务名称不能为空".to_string());
    }
    if schedule.session_id.trim().is_empty() {
        return Err("必须指定目标会话".to_string());
    }
    if schedule.prompt.trim().is_empty() {
        return Err("提示词不能为空".to_string());
    }
    if !(1..=10080).contains(&schedule.interval_minutes) {
        return Err("间隔需在 1~10080 分钟之间".to_string());
    }
    let mut list = schedule_store().lock().unwrap();
    let saved = if schedule.id.is_empty() {
        let mut s = schedule;
        s.id = format!("sch-{}", uuid::Uuid::new_v4().simple());
        s.created_at = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        s.next_run_at = now_unix() + s.interval_minutes * 60;
        s.last_run_at = None;
        list.push(s.clone());
        s
    } else {
        let Some(existing) = list.iter().find(|x| x.id == schedule.id) else {
            return Err("指定的定时任务不存在".to_string());
        };
        let mut s = schedule;
        s.created_at = existing.created_at.clone();
        // 间隔变化时重算下次运行
        if s.interval_minutes != existing.interval_minutes {
            s.next_run_at = now_unix() + s.interval_minutes * 60;
        }
        let idx = list.iter().position(|x| x.id == s.id).unwrap();
        list[idx] = s.clone();
        s
    };
    persist(&list)?;
    Ok(saved)
}

#[tauri::command]
pub async fn delete_harness_schedule(id: String) -> Result<(), String> {
    let mut list = schedule_store().lock().unwrap();
    let before = list.len();
    list.retain(|s| s.id != id);
    if list.len() == before {
        return Err("指定的定时任务不存在".to_string());
    }
    persist(&list)
}

// ─── 模型工具核心（DSH schedule tools 语义） ───

/// 会话内定时条目（模型 schedule_list）
pub(crate) fn list_for_session(session_id: &str) -> Vec<HarnessSchedule> {
    schedule_store()
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.session_id == session_id)
        .cloned()
        .collect()
}

/// 创建定时任务（模型 schedule_create）：
/// every_minutes 周期执行；after_seconds 一次性延时执行（DSH after_seconds 语义）
pub(crate) fn create_for_session(
    session_id: &str,
    name: &str,
    prompt: &str,
    every_minutes: Option<u64>,
    after_seconds: Option<u64>,
) -> Result<HarnessSchedule, String> {
    if prompt.trim().is_empty() {
        return Err("prompt 不能为空".to_string());
    }
    let (interval, one_shot, next_run) = match (every_minutes, after_seconds) {
        (Some(m), _) => {
            if !(1..=10080).contains(&m) {
                return Err("every_minutes 需在 1~10080 之间".to_string());
            }
            (m, false, now_unix() + m * 60)
        }
        (_, Some(secs)) => {
            if !(1..=2592000).contains(&secs) {
                return Err("after_seconds 需在 1~2592000 之间".to_string());
            }
            (1, true, now_unix() + secs)
        }
        _ => return Err("需指定 every_minutes 或 after_seconds".to_string()),
    };
    let s = HarnessSchedule {
        id: format!("sch-{}", uuid::Uuid::new_v4().simple()),
        name: if name.trim().is_empty() {
            "定时任务".to_string()
        } else {
            name.trim().to_string()
        },
        session_id: session_id.to_string(),
        prompt: prompt.trim().to_string(),
        interval_minutes: interval,
        enabled: true,
        one_shot,
        next_run_at: next_run,
        last_run_at: None,
        created_at: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    };
    let mut list = schedule_store().lock().unwrap();
    list.push(s.clone());
    persist(&list)?;
    Ok(s)
}

/// 删除定时任务（模型 schedule_delete）
pub(crate) fn delete_for_session(id: &str, session_id: &str) -> Result<(), String> {
    let mut list = schedule_store().lock().unwrap();
    let before = list.len();
    list.retain(|s| !(s.id == id && s.session_id == session_id));
    if list.len() == before {
        return Err("指定的定时任务不存在".to_string());
    }
    persist(&list)
}

/// 立即运行一次（不等到期；手动触发/测试用）
#[tauri::command]
pub async fn run_harness_schedule_now(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let s = schedule_store()
        .lock()
        .unwrap()
        .iter()
        .find(|x| x.id == id)
        .cloned()
        .ok_or("指定的定时任务不存在")?;
    // 复用到期执行路径（不判定 enabled：手动触发即执行）
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        run_due(&app2, s).await;
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_validation() {
        let s = HarnessSchedule {
            id: String::new(),
            name: String::new(),
            session_id: "h-1".into(),
            prompt: "p".into(),
            interval_minutes: 5,
            enabled: true,
            one_shot: false,
            next_run_at: 0,
            last_run_at: None,
            created_at: String::new(),
        };
        // 名称必填（在 IPC 校验，此处仅验证字段模型）
        assert!(s.id.is_empty());
        let mut ok = s.clone();
        ok.name = "n".into();
        assert!(ok.interval_minutes == 5);
    }

    #[test]
    fn schedule_due_filter_and_state_transitions() {
        // 到期判定（enabled && next_run_at <= now）与执行后状态机
        // （间隔推进 / one_shot 停用 / last_run_at 记录）——纯字段逻辑，
        // 与 run_due 的 store 更新分支一致
        let now = now_unix();
        let mk = |id: &str, next_run_at: u64, one_shot: bool, enabled: bool| HarnessSchedule {
            id: id.into(),
            name: id.into(),
            session_id: "h-1".into(),
            prompt: "p".into(),
            interval_minutes: 5,
            enabled,
            one_shot,
            next_run_at,
            last_run_at: None,
            created_at: String::new(),
        };
        // 到期：enabled 且 next_run_at <= now
        let due_now = mk("due", now - 1, false, true);
        let due_future = mk("future", now + 9999, false, true);
        let due_disabled = mk("disabled", now - 1, false, false);
        let due: Vec<_> = vec![due_now.clone(), due_future, due_disabled]
            .into_iter()
            .filter(|s| s.enabled && s.next_run_at <= now)
            .collect();
        assert_eq!(due.len(), 1, "仅到期且启用的条目应触发");
        assert_eq!(due[0].id, "due");
        // 执行后：常规条目推进 next_run_at = now + interval*60
        let mut ran = due_now;
        ran.last_run_at = Some(now);
        if ran.one_shot {
            ran.enabled = false;
        } else {
            ran.next_run_at = now + ran.interval_minutes * 60;
        }
        assert_eq!(ran.last_run_at, Some(now));
        assert!(ran.next_run_at > now, "下次运行应推进");
        // 一次性任务：执行后停用（enabled=false）
        let mut os = mk("oneshot", now - 1, true, true);
        os.last_run_at = Some(now);
        if os.one_shot {
            os.enabled = false;
        }
        assert!(!os.enabled, "一次性任务执行后应停用");
        assert_eq!(os.next_run_at, now - 1, "one_shot 不推进 next_run_at");
    }
}
