use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 默认任务存储根路径
const DEFAULT_TASK_DIR: &str = r"C:\Users\Administrator\AppData\Roaming\st_task";

/// 配置文件存储路径（与默认目录同级）
const CONFIG_FILE_NAME: &str = "st_task_config.json";

/// 配置结构
#[derive(Debug, Serialize, Deserialize)]
struct TaskConfig {
    current_path: String,
}

/// 任务路径管理器
pub struct TaskManager {
    config_dir: PathBuf,
    current_path: Mutex<PathBuf>,
}

impl TaskManager {
    /// 初始化任务管理器：确保默认目录存在，加载配置
    pub fn new() -> Result<Self, String> {
        let config_dir = Self::get_config_dir();
        let config_path = config_dir.join(CONFIG_FILE_NAME);

        // 确保配置目录存在
        fs::create_dir_all(&config_dir)
            .map_err(|e| format!("创建配置目录失败: {}", e))?;

        // 加载或创建配置
        let current_path = if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .map_err(|e| format!("读取配置文件失败: {}", e))?;
            let config: TaskConfig = serde_json::from_str(&content)
                .map_err(|e| format!("解析配置文件失败: {}", e))?;
            PathBuf::from(&config.current_path)
        } else {
            // 首次启动，使用默认路径
            let default = PathBuf::from(DEFAULT_TASK_DIR);
            let config = TaskConfig {
                current_path: default.to_string_lossy().to_string(),
            };
            let json = serde_json::to_string_pretty(&config)
                .map_err(|e| format!("序列化配置失败: {}", e))?;
            fs::write(&config_path, &json)
                .map_err(|e| format!("写入配置文件失败: {}", e))?;
            default
        };

        // 确保当前任务目录存在
        fs::create_dir_all(&current_path)
            .map_err(|e| format!("创建任务目录失败 ({}): {}", current_path.display(), e))?;

        Ok(Self {
            config_dir,
            current_path: Mutex::new(current_path),
        })
    }

    /// 获取配置目录（AppData\Roaming\st_task 同级）
    fn get_config_dir() -> PathBuf {
        let default = PathBuf::from(DEFAULT_TASK_DIR);
        if let Some(parent) = default.parent() {
            parent.to_path_buf()
        } else {
            default
        }
    }

    /// 获取当前任务存储路径
    pub fn get_current_path(&self) -> PathBuf {
        self.current_path.lock().unwrap().clone()
    }

    /// 获取路径存在状态
    pub fn get_path_info(&self) -> Result<PathInfo, String> {
        let path = self.get_current_path();
        let exists = path.exists();
        let is_dir = exists && path.is_dir();
        let item_count = if is_dir {
            count_items(&path).unwrap_or(0)
        } else {
            0
        };
        Ok(PathInfo {
            path: path.to_string_lossy().to_string(),
            exists,
            is_dir,
            item_count,
        })
    }

    /// 读取所有任务文件，按状态汇总数量
    pub fn count_task_statuses(&self) -> Result<TaskStatusSummary, String> {
        let base = self.get_current_path();
        let mut summary = TaskStatusSummary::default();

        let mut stack = vec![base];
        while let Some(dir) = stack.pop() {
            let entries = fs::read_dir(&dir)
                .map_err(|e| format!("读取目录失败 ({}): {}", dir.display(), e))?;
            for entry in entries.flatten() {
                let ft = entry.file_type().map_err(|_| "无法获取文件类型".to_string())?;
                let path = entry.path();
                if ft.is_dir() {
                    stack.push(path);
                } else if ft.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                    summary.total += 1;
                    // 读取文件并解析 status 字段
                    match std::fs::read_to_string(&path) {
                        Ok(content) => {
                            let status = serde_json::from_str::<serde_json::Value>(&content)
                                .ok()
                                .and_then(|v| v.get("status").and_then(|s| s.as_str().map(String::from)))
                                .unwrap_or_else(|| "pending".to_string());
                            match status.as_str() {
                                "completed" => summary.completed += 1,
                                "failed" => summary.failed += 1,
                                "running" => {
                                    summary.running += 1;
                                    summary.capture_running(&path);
                                }
                                _ => summary.pending += 1,
                            }
                        }
                        Err(_) => summary.pending += 1,
                    }
                }
            }
        }
        Ok(summary)
    }

    /// 按状态查询任务文件列表
    pub fn get_files_by_status(&self, filter_status: &str) -> Result<Vec<TaskFileEntry>, String> {
        let base = self.get_current_path();
        let mut files = Vec::new();

        let mut stack = vec![base];
        while let Some(dir) = stack.pop() {
            let entries = fs::read_dir(&dir)
                .map_err(|e| format!("读取目录失败 ({}): {}", dir.display(), e))?;
            for entry in entries.flatten() {
                let ft = entry.file_type().map_err(|_| "无法获取文件类型".to_string())?;
                let path = entry.path();
                if ft.is_dir() {
                    stack.push(path);
                } else if ft.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                    let content = std::fs::read_to_string(&path).unwrap_or_default();
                    let json: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
                    let status = json.get("status")
                        .and_then(|s| s.as_str())
                        .unwrap_or("pending")
                        .to_string();

                    // 匹配过滤状态（pending 用特殊处理：除 running/completed/failed 外都属于 pending）
                    let matches = match filter_status {
                        "pending" => {
                            status != "completed" && status != "failed" && status != "running"
                        }
                        _ => status == filter_status,
                    };
                    if !matches {
                        continue;
                    }

                    let filename = path.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let method = json.get("method")
                        .and_then(|s| s.as_str())
                        .unwrap_or("-")
                        .to_string();
                    let task_id = json.get("taskId")
                        .and_then(|s| s.as_str())
                        .unwrap_or("-")
                        .to_string();
                    let received_at = json.get("receivedAt")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    let filepath = path.to_string_lossy().to_string();

                    files.push(TaskFileEntry {
                        filename,
                        filepath,
                        method,
                        task_id,
                        status,
                        received_at,
                    });
                }
            }
        }

        // 按 receivedAt 降序排列（最新的在前）
        files.sort_by(|a, b| b.received_at.cmp(&a.received_at));
        Ok(files)
    }

    /// 读取指定文件的完整 JSON 内容
    pub fn get_file_content(&self, file_path: &str) -> Result<String, String> {
        let p = std::path::Path::new(file_path);
        if !p.exists() {
            return Err(format!("文件不存在: {}", file_path));
        }
        std::fs::read_to_string(p)
            .map_err(|e| format!("读取文件失败: {}", e))
    }

    /// 根据 taskId 更新任务状态（在任务执行完成后调用）
    pub fn update_task_status(&self, task_id: &str, new_status: &str) -> Result<(), String> {
        let base = self.get_current_path();
        let mut stack = vec![base];
        while let Some(dir) = stack.pop() {
            let entries = fs::read_dir(&dir)
                .map_err(|e| format!("读取目录失败: {}", e))?;
            for entry in entries.flatten() {
                let ft = entry.file_type().map_err(|_| "无法获取文件类型".to_string())?;
                let path = entry.path();
                if ft.is_dir() {
                    stack.push(path);
                } else if ft.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                    // 检查文件名是否包含 taskId
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        if name.starts_with(task_id) {
                            // 读取、修改状态、写回
                            let content = std::fs::read_to_string(&path)
                                .map_err(|e| format!("读取任务文件失败: {}", e))?;
                            let mut json: serde_json::Value =
                                serde_json::from_str(&content).map_err(|e| e.to_string())?;
                            if let Some(obj) = json.as_object_mut() {
                                obj.insert(
                                    "status".to_string(),
                                    serde_json::Value::String(new_status.to_string()),
                                );
                                let updated =
                                    serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
                                std::fs::write(&path, &updated)
                                    .map_err(|e| format!("写入文件失败: {}", e))?;
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
        Err(format!("未找到 taskId 为 {} 的任务文件", task_id))
    }

    /// 保存任务到存储路径
    /// payload 是 serde_json::Value，直接嵌入 JSON，避免字符串转义
    pub fn save_task(&self, task_id: &str, method: &str, payload: &serde_json::Value) -> Result<String, String> {
        let base = self.get_current_path();
        let date_dir = chrono::Utc::now().format("%Y%m%d").to_string();
        let task_dir = base.join(&date_dir);
        fs::create_dir_all(&task_dir)
            .map_err(|e| format!("创建任务子目录失败: {}", e))?;

        let filename = format!("{}_{}.json", task_id, method.replace('.', "_"));
        let filepath = task_dir.join(&filename);

        let content = serde_json::json!({
            "taskId": task_id,
            "method": method,
            "source": payload.get("targetAgentId"),
            "task": payload.get("task"),
            "status": "pending",
            "receivedAt": chrono::Utc::now().to_rfc3339(),
        });

        let json = serde_json::to_string_pretty(&content)
            .map_err(|e| format!("序列化任务失败: {}", e))?;

        fs::write(&filepath, &json)
            .map_err(|e| format!("写入任务文件失败 ({}): {}", filepath.display(), e))?;

        Ok(filepath.to_string_lossy().to_string())
    }

    /// 更新任务存储路径：迁移数据后更新配置
    pub fn set_path(&self, new_path_str: &str) -> Result<PathInfo, String> {
        let new_path = PathBuf::from(new_path_str);

        // 1. 校验新路径格式
        if new_path_str.trim().is_empty() {
            return Err("路径不能为空".to_string());
        }

        // 2. 新旧路径相同则无需操作
        let old_path = self.get_current_path();
        if same_path(&old_path, &new_path) {
            return Ok(PathInfo {
                path: new_path.to_string_lossy().to_string(),
                exists: new_path.exists(),
                is_dir: new_path.is_dir(),
                item_count: count_items(&new_path).unwrap_or(0),
            });
        }

        // 3. 校验新路径不是旧路径的子路径（避免循环迁移）
        if new_path.starts_with(&old_path) || old_path.starts_with(&new_path) {
            return Err("新路径不能是原路径的子目录，请选择其他路径".to_string());
        }

        // 4. 确保新路径的父目录存在
        if let Some(parent) = new_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("创建父目录失败 ({}): {}", parent.display(), e))?;
        }

        // 5. 执行迁移（原路径存在且有内容时）
        if old_path.exists() && old_path.is_dir() {
            let has_items = count_items(&old_path).unwrap_or(0) > 0;
            if has_items {
                migrate_directory(&old_path, &new_path)?;
                // 迁移成功后删除原目录
                fs::remove_dir_all(&old_path)
                    .map_err(|e| format!("删除原目录失败: {}", e))?;
            }
        }

        // 6. 确保新路径目录存在
        fs::create_dir_all(&new_path)
            .map_err(|e| format!("创建新任务目录失败: {}", e))?;

        // 7. 保存配置（迁移成功后）
        let config = TaskConfig {
            current_path: new_path.to_string_lossy().to_string(),
        };
        let json = serde_json::to_string_pretty(&config)
            .map_err(|e| format!("序列化配置失败: {}", e))?;
        let config_path = self.config_dir.join(CONFIG_FILE_NAME);
        fs::write(&config_path, &json)
            .map_err(|e| format!("写入配置文件失败: {}", e))?;

        // 8. 更新内存中的路径
        let mut current = self.current_path.lock().unwrap();
        *current = new_path.clone();

        Ok(PathInfo {
            path: new_path.to_string_lossy().to_string(),
            exists: true,
            is_dir: true,
            item_count: count_items(&new_path).unwrap_or(0),
        })
    }
}

/// 路径信息（返回给前端）
#[derive(Debug, Serialize)]
pub struct PathInfo {
    pub path: String,
    pub exists: bool,
    pub is_dir: bool,
    pub item_count: u64,
}

/// 任务状态汇总（返回给前端，用于图表展示）
#[derive(Debug, Default, Serialize)]
pub struct TaskStatusSummary {
    pub total: u64,
    pub completed: u64,
    pub failed: u64,
    pub running: u64,
    pub pending: u64,
    /// 当前执行中的任务文件名（仅当 running>0 时有值）
    pub running_file_name: Option<String>,
    /// 当前执行中的任务文件完整路径
    pub running_file_path: Option<String>,
}

impl TaskStatusSummary {
    /// 记录一个 running 任务的信息（仅记录第一个找到的）
    fn capture_running(&mut self, path: &std::path::Path) {
        if self.running_file_name.is_some() {
            return; // 已有记录，不再覆盖
        }
        // 文件名格式：{taskId}_{method}.json → 提取 method 作为显示名
        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
            // 去掉 taskId_ 前缀，保留 method 部分
            let display_name = name.splitn(2, '_').nth(1).unwrap_or(name).to_string();
            self.running_file_name = Some(display_name);
        }
        self.running_file_path = Some(path.to_string_lossy().to_string());
    }
}

/// 递归迁移目录
fn migrate_directory(src: &Path, dst: &Path) -> Result<(), String> {
    // 确保目标目录存在
    fs::create_dir_all(dst)
        .map_err(|e| format!("创建目标目录失败 ({}): {}", dst.display(), e))?;

    // 遍历源目录
    let entries = fs::read_dir(src)
        .map_err(|e| format!("读取源目录失败 ({}): {}", src.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录条目失败: {}", e))?;
        let file_type = entry.file_type().map_err(|e| format!("获取文件类型失败: {}", e))?;
        let src_path = entry.path();
        let relative = src_path.strip_prefix(src).map_err(|_| "路径解析错误".to_string())?;
        let dst_path = dst.join(relative);

        if file_type.is_dir() {
            // 递归复制子目录
            migrate_directory(&src_path, &dst_path)?;
        } else {
            // 复制文件
            // 先检查目标是否存在且内容相同（断点续传保护）
            if dst_path.exists() {
                let src_metadata = fs::metadata(&src_path)
                    .map_err(|e| format!("读取源文件信息失败: {}", e))?;
                let dst_metadata = fs::metadata(&dst_path)
                    .map_err(|e| format!("读取目标文件信息失败: {}", e))?;
                if src_metadata.len() == dst_metadata.len() {
                    // 大小相同，跳过（可能是上次迁移中断的残留）
                    continue;
                }
            }

            // 确保父目录存在
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("创建子目录失败 ({}): {}", parent.display(), e))?;
            }

            // 执行文件复制
            fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("文件复制失败 ({}): {}", src_path.display(), e))?;
        }
    }

    Ok(())
}

/// 统计目录中的任务文件数量（仅统计 .json 文件，不统计目录条目）
fn count_items(path: &Path) -> io::Result<u64> {
    let mut count = 0u64;
    if path.is_dir() {
        let mut stack = vec![path.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let ft = entry.file_type()?;
                if ft.is_dir() {
                    stack.push(entry.path());
                } else if ft.is_file() {
                    count += 1;
                }
            }
        }
    }
    Ok(count)
}

/// 判断两个路径是否指向同一位置（规范化后比较）
fn same_path(a: &Path, b: &Path) -> bool {
    let canonical_a = a.canonicalize();
    let canonical_b = b.canonicalize();
    match (canonical_a, canonical_b) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b, // 如果无法规范化，退化为字符串比较
    }
}

/// 任务文件条目（用于弹窗列表展示）
#[derive(Debug, Serialize)]
pub struct TaskFileEntry {
    pub filename: String,
    pub filepath: String,
    pub method: String,
    pub task_id: String,
    pub status: String,
    pub received_at: String,
}
