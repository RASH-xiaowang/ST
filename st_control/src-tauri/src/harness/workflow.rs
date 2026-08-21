// ============================================================
// Harness — 工作流（DSH workflow 迁移）
//
// 工作流 = 有序阶段列表（名称 + 提示词）：按顺序逐阶段执行一轮
// 代理对话，前序阶段输出作为上下文注入后序阶段（同一会话日志）。
// 持久化：data/harness/workflows.json（原子写）。
// 运行：手动触发（IPC）或模型经 task 工具委派（见 agent 循环）。
// ============================================================

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

/// 工作流阶段
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorkflowStage {
    pub name: String,
    pub prompt: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HarnessWorkflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub stages: Vec<WorkflowStage>,
    pub created_at: String,
    pub updated_at: String,
}

/// 单次运行结果（阶段输出列表）
#[derive(Serialize, Clone, Debug)]
pub struct WorkflowRunResult {
    pub workflow_id: String,
    pub stages: Vec<WorkflowStageResult>,
}

#[derive(Serialize, Clone, Debug)]
pub struct WorkflowStageResult {
    pub name: String,
    pub ok: bool,
    pub output: String,
}

fn workflows_path() -> std::path::PathBuf {
    crate::common::st_data_dir()
        .join("harness")
        .join("workflows.json")
}

fn workflows_store() -> &'static Mutex<Vec<HarnessWorkflow>> {
    static W: OnceLock<Mutex<Vec<HarnessWorkflow>>> = OnceLock::new();
    W.get_or_init(|| {
        let list = std::fs::read_to_string(workflows_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Mutex::new(list)
    })
}

fn persist(list: &[HarnessWorkflow]) -> Result<(), String> {
    let path = workflows_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建工作流目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(list).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换失败: {}", e))?;
    Ok(())
}

fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// 工作流列表（模型工具 workflow_list 使用）
pub fn list() -> Result<Vec<HarnessWorkflow>, String> {
    Ok(workflows_store().lock().unwrap().clone())
}

/// 运行工作流：逐阶段执行一轮对话；前序输出注入后序提示词
pub async fn run_workflow(
    app: &tauri::AppHandle,
    workflow_id: &str,
    session_id: &str,
) -> Result<WorkflowRunResult, String> {
    let workflow = workflows_store()
        .lock()
        .unwrap()
        .iter()
        .find(|w| w.id == workflow_id)
        .cloned()
        .ok_or("指定的工作流不存在")?;
    if workflow.stages.is_empty() {
        return Err("工作流没有阶段".to_string());
    }
    let store =
        crate::harness::registry::get::<crate::harness::session::SessionStore>("harness.sessions")
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    let mut prior: Vec<String> = Vec::new();
    let total = workflow.stages.len();
    let mut results: Vec<WorkflowStageResult> = Vec::new();
    for (i, stage) in workflow.stages.iter().enumerate() {
        // 阶段提示词 = 原始提示词 + 前序输出（显式注入，模型可见 ⟺ 落日志）
        let prompt = if prior.is_empty() {
            stage.prompt.clone()
        } else {
            format!(
                "{}\n\n[前序阶段输出]\n{}",
                stage.prompt,
                prior.join("\n\n---\n\n")
            )
        };
        let run =
            crate::harness::agent::run_turn_internal(app, session_id, None, None, &prompt).await;
        let (ok, output) = match run {
            Ok(()) => {
                // 取本轮最终回答（日志投影的最后一条 assistant 消息）
                let msgs = store.derive_display_messages(session_id)?;
                let last = msgs.iter().rev().find(|m| {
                    matches!(m, crate::harness::session::DisplayMessage::Assistant { .. })
                });
                let text = match last {
                    Some(crate::harness::session::DisplayMessage::Assistant {
                        content, ..
                    }) => content.clone(),
                    _ => String::new(),
                };
                (true, text)
            }
            Err(e) => (false, e),
        };
        store
            .append(
                session_id,
                &crate::harness::session::HarnessEvent::WorkflowRun {
                    workflow_id: workflow_id.to_string(),
                    name: workflow.name.clone(),
                    stage: i + 1,
                    total,
                    output: output.clone(),
                },
            )
            .ok();
        if ok {
            prior.push(format!("[{}]\n{}", stage.name, output));
        }
        results.push(WorkflowStageResult {
            name: stage.name.clone(),
            ok,
            output,
        });
    }
    Ok(WorkflowRunResult {
        workflow_id: workflow_id.to_string(),
        stages: results,
    })
}

/// Ralph 循环（DSH tool-ralph 迁移）：固定轮次的全新子代理迭代——
/// 每轮启动一个全新上下文子代理（不继承对话，共享工作区记忆），
/// 每轮报告落日志（WorkflowRun 事件，模型可见 ⟺ 落日志）；
/// 子代理以「已完成」或「已阻塞」开头汇报时提前结束。
pub async fn run_ralph(
    app: &tauri::AppHandle,
    session_id: &str,
    provider: &crate::llm::types::ProviderConfig,
    model: &str,
    objective: &str,
    max_rounds: usize,
) -> Result<String, String> {
    let max_rounds = max_rounds.clamp(1, 16);
    let scope = crate::harness::preset::scope_for_session_id(session_id);
    let store =
        crate::harness::registry::get::<crate::harness::session::SessionStore>("harness.sessions")
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    let mut rounds_log: Vec<String> = Vec::new();
    for round in 1..=max_rounds {
        let task = format!(
            "你是第 {round} 轮独立执行者（全新上下文，不继承此前对话）。\n\n目标：{objective}\n\n\
             请独立推进该目标。完成后以「已完成」开头汇报最终结果；\
             若无法推进（缺前置、条件不满足、需要人工决策），以「已阻塞：<原因>」开头汇报。"
        );
        let result = super::subagent::run_subagent(app, provider, model, &task, &scope).await?;
        store
            .append(
                session_id,
                &crate::harness::session::HarnessEvent::WorkflowRun {
                    workflow_id: "ralph".to_string(),
                    name: format!("Ralph 轮 {}/{}", round, max_rounds),
                    stage: round,
                    total: max_rounds,
                    output: result.clone(),
                },
            )
            .ok();
        let done = ralph_done(&result);
        rounds_log.push(format!("[轮 {round}] {}", result));
        if done {
            break;
        }
    }
    Ok(format!(
        "Ralph 迭代完成（共 {} 轮，上限 {}）：\n{}",
        rounds_log.len(),
        max_rounds,
        rounds_log.join("\n\n---\n\n")
    ))
}

/// Ralph 轮次提前结束判定（子代理以「已完成/已阻塞」开头汇报）
fn ralph_done(result: &str) -> bool {
    let trimmed = result.trim_start();
    trimmed.starts_with("已完成") || trimmed.starts_with("已阻塞")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ralph_rounds_stop_on_done_or_blocked() {
        assert!(ralph_done("已完成：全部测试通过"));
        assert!(ralph_done("已阻塞：缺少 API 密钥"));
        assert!(!ralph_done("正在推进：第一步完成"));
        assert!(!ralph_done(""));
        // 完成判定对大小写/空白稳健
        assert!(ralph_done("  已完成任务"));
    }

    #[test]
    fn workflow_validation_shape() {
        let w = HarnessWorkflow {
            id: String::new(),
            name: "测试".into(),
            description: String::new(),
            stages: vec![WorkflowStage {
                name: "s1".into(),
                prompt: "p".into(),
            }],
            created_at: String::new(),
            updated_at: String::new(),
        };
        assert_eq!(w.stages.len(), 1);
        assert!(w.name == "测试");
    }

    #[tokio::test]
    async fn workflow_save_validation_rules() {
        // save_harness_workflow 校验（与 IPC 分支一致）：
        // 名称非空 / 至少一个阶段 / 每阶段提示词非空
        let base = HarnessWorkflow {
            id: String::new(),
            name: "wf".into(),
            description: String::new(),
            stages: vec![WorkflowStage {
                name: "s1".into(),
                prompt: "p".into(),
            }],
            created_at: String::new(),
            updated_at: String::new(),
        };
        // 名称空 → 拒绝
        let mut bad = base.clone();
        bad.name = "  ".into();
        assert!(save_harness_workflow(bad).await.is_err(), "空名称应拒绝");
        // 无阶段 → 拒绝
        let mut bad = base.clone();
        bad.stages = vec![];
        assert!(save_harness_workflow(bad).await.is_err(), "无阶段应拒绝");
        // 阶段提示词空 → 拒绝
        let mut bad = base.clone();
        bad.stages = vec![WorkflowStage {
            name: "s".into(),
            prompt: " ".into(),
        }];
        assert!(
            save_harness_workflow(bad).await.is_err(),
            "空提示词阶段应拒绝"
        );
        // 合法 → 新建（id 生成 wf- 前缀）；清理
        let saved = save_harness_workflow(base).await.unwrap();
        assert!(saved.id.starts_with("wf-"), "新建应生成 wf- id: {saved:?}");
        assert!(saved.created_at == saved.updated_at);
        delete_harness_workflow(saved.id.clone()).await.unwrap();
    }
}

#[tauri::command]
pub async fn list_harness_workflows() -> Result<Vec<HarnessWorkflow>, String> {
    Ok(workflows_store().lock().unwrap().clone())
}

/// 新建或更新工作流（id 为空 → 新建）
#[tauri::command]
pub async fn save_harness_workflow(workflow: HarnessWorkflow) -> Result<HarnessWorkflow, String> {
    if workflow.name.trim().is_empty() {
        return Err("工作流名称不能为空".to_string());
    }
    if workflow.stages.is_empty() {
        return Err("工作流至少需要一个阶段".to_string());
    }
    for s in &workflow.stages {
        if s.prompt.trim().is_empty() {
            return Err(format!("阶段「{}」的提示词不能为空", s.name));
        }
    }
    let mut list = workflows_store().lock().unwrap();
    let now = now_iso();
    let saved = if workflow.id.is_empty() {
        let mut w = workflow;
        w.id = format!("wf-{}", uuid::Uuid::new_v4().simple());
        w.created_at = now.clone();
        w.updated_at = now;
        list.push(w.clone());
        w
    } else {
        let Some(existing) = list.iter().find(|w| w.id == workflow.id) else {
            return Err("指定的工作流不存在".to_string());
        };
        let mut w = workflow;
        w.created_at = existing.created_at.clone();
        w.updated_at = now;
        let idx = list.iter().position(|x| x.id == w.id).unwrap();
        list[idx] = w.clone();
        w
    };
    persist(&list)?;
    Ok(saved)
}

#[tauri::command]
pub async fn delete_harness_workflow(id: String) -> Result<(), String> {
    let mut list = workflows_store().lock().unwrap();
    let before = list.len();
    list.retain(|w| w.id != id);
    if list.len() == before {
        return Err("指定的工作流不存在".to_string());
    }
    persist(&list)
}

/// 在指定会话运行工作流（异步执行，结果落会话日志）
#[tauri::command]
pub async fn run_harness_workflow(
    app: tauri::AppHandle,
    workflow_id: String,
    session_id: String,
) -> Result<WorkflowRunResult, String> {
    run_workflow(&app, &workflow_id, &session_id).await
}
