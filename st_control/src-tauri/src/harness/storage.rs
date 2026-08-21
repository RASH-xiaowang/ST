// ============================================================
// Harness — 存储能力（DSH storage 迁移）
//
// 命名后端注册表（DSH backend registry 语义）：
// - "default"（缺省）：SQLite KV（harness_kv 表）
// - "json:<名称>"：JSON 文件后端（data/harness/kv/<名称>.json，原子写）
// 会话日志持久化同样落在 SQLite（SessionStore），本能力提供通用
// 键值存储供技能/扩展/外部集成使用（人工命令 + SDK 暴露）。
// ============================================================

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// KV 存储服务（默认后端 = SessionStore 同一数据库的 SQLite）
pub struct StorageService;

/// JSON 后端缓存（名称 → key/value 表），进程内 memo + 文件持久化
fn json_backends() -> &'static Mutex<HashMap<String, HashMap<String, String>>> {
    static B: OnceLock<Mutex<HashMap<String, HashMap<String, String>>>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(HashMap::new()))
}

fn json_backend_path(name: &str) -> std::path::PathBuf {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    crate::common::st_data_dir()
        .join("harness")
        .join("kv")
        .join(format!("{clean}.json"))
}

fn load_json_backend(name: &str) -> HashMap<String, String> {
    std::fs::read_to_string(json_backend_path(name))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn persist_json_backend(name: &str, map: &HashMap<String, String>) -> Result<(), String> {
    let path = json_backend_path(name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建 KV 目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(map).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换失败: {}", e))?;
    Ok(())
}

/// 解析后端名："default" / "" → SQLite；"json:<名称>" → JSON 文件
fn split_backend(backend: Option<&str>) -> (bool, String) {
    match backend.filter(|b| !b.trim().is_empty()) {
        Some(b) if b.starts_with("json:") => (true, b[5..].to_string()),
        _ => (false, String::new()),
    }
}

impl StorageService {
    pub fn put(&self, key: &str, value: &str) -> Result<(), String> {
        self.put_in(None, key, value)
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, String> {
        self.get_in(None, key)
    }

    pub fn delete(&self, key: &str) -> Result<(), String> {
        self.delete_in(None, key)
    }

    /// 命名后端写入（DSH backend registry 语义）
    pub fn put_in(&self, backend: Option<&str>, key: &str, value: &str) -> Result<(), String> {
        let (is_json, name) = split_backend(backend);
        if is_json {
            let mut backends = json_backends().lock().unwrap();
            let map = backends
                .entry(name.clone())
                .or_insert_with(|| load_json_backend(&name));
            map.insert(key.to_string(), value.to_string());
            persist_json_backend(&name, map)
        } else {
            let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
                "harness.sessions",
            )
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
            store.kv_put(key, value)
        }
    }

    /// 命名后端读取
    pub fn get_in(&self, backend: Option<&str>, key: &str) -> Result<Option<String>, String> {
        let (is_json, name) = split_backend(backend);
        if is_json {
            let mut backends = json_backends().lock().unwrap();
            let map = backends
                .entry(name.clone())
                .or_insert_with(|| load_json_backend(&name));
            Ok(map.get(key).cloned())
        } else {
            let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
                "harness.sessions",
            )
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
            store.kv_get(key)
        }
    }

    /// 命名后端删除
    pub fn delete_in(&self, backend: Option<&str>, key: &str) -> Result<(), String> {
        let (is_json, name) = split_backend(backend);
        if is_json {
            let mut backends = json_backends().lock().unwrap();
            let map = backends
                .entry(name.clone())
                .or_insert_with(|| load_json_backend(&name));
            map.remove(key);
            persist_json_backend(&name, map)
        } else {
            let store = crate::harness::registry::get::<crate::harness::session::SessionStore>(
                "harness.sessions",
            )
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
            store.kv_delete(key)
        }
    }

    /// 已初始化的 JSON 后端名列表（storage_list）
    pub fn backends(&self) -> Vec<String> {
        let dir = crate::common::st_data_dir().join("harness").join("kv");
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                if let Some(name) = e
                    .file_name()
                    .to_str()
                    .and_then(|s| s.strip_suffix(".json").map(|s| s.to_string()))
                {
                    out.push(format!("json:{name}"));
                }
            }
        }
        out.sort();
        out
    }
}

/// 注册存储能力（Cordis-lite 服务）
pub fn provide_service() -> crate::harness::registry::Disposer {
    crate::harness::registry::provide("harness.storage", std::sync::Arc::new(StorageService))
}

#[tauri::command]
pub async fn harness_kv_put(
    key: String,
    value: String,
    backend: Option<String>,
) -> Result<(), String> {
    StorageService.put_in(backend.as_deref(), &key, &value)
}

#[tauri::command]
pub async fn harness_kv_get(
    key: String,
    backend: Option<String>,
) -> Result<Option<String>, String> {
    StorageService.get_in(backend.as_deref(), &key)
}

#[tauri::command]
pub async fn harness_kv_delete(key: String, backend: Option<String>) -> Result<(), String> {
    StorageService.delete_in(backend.as_deref(), &key)
}

/// 后端名册（DSH storage backend registry 投影）
#[tauri::command]
pub async fn harness_storage_backends() -> Result<Vec<String>, String> {
    let mut out = vec!["default".to_string()];
    out.extend(StorageService.backends());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kv_roundtrip_via_db() {
        // 直接经 SessionStore 的 db 委托验证（避免依赖全局注册表时序）
        let db = crate::db::Database::new().unwrap();
        db.harness_kv_put("test-key", "v1").unwrap();
        assert_eq!(
            db.harness_kv_get("test-key").unwrap().as_deref(),
            Some("v1")
        );
        db.harness_kv_delete("test-key").unwrap();
        assert!(db.harness_kv_get("test-key").unwrap().is_none());
    }

    #[test]
    fn json_backend_roundtrip_and_isolation() {
        let name = format!("test-{}", uuid::Uuid::new_v4().simple());
        let backend = format!("json:{name}");
        let svc = StorageService;
        svc.put_in(Some(&backend), "k1", "v1").unwrap();
        assert_eq!(
            svc.get_in(Some(&backend), "k1").unwrap().as_deref(),
            Some("v1")
        );
        // 与默认后端（SQLite）隔离
        let db = crate::db::Database::new().unwrap();
        assert!(db.harness_kv_get("k1").unwrap().is_none());
        svc.delete_in(Some(&backend), "k1").unwrap();
        assert!(svc.get_in(Some(&backend), "k1").unwrap().is_none());
        // 清理文件
        let _ = std::fs::remove_file(json_backend_path(&name));
    }

    #[test]
    fn backend_split_and_name_sanitization() {
        // split_backend：None/空/default → SQLite；json:<名称> → JSON 后端
        assert_eq!(split_backend(None), (false, String::new()));
        assert_eq!(split_backend(Some("")), (false, String::new()));
        assert_eq!(split_backend(Some("default")), (false, String::new()));
        assert_eq!(
            split_backend(Some("json:notes")),
            (true, "notes".to_string())
        );
        // 带空白：starts_with 前未 trim → 视为 default（现有行为）
        assert_eq!(
            split_backend(Some("  json:notes  ")),
            (false, String::new())
        );
        // json_backend_path 名称清理：非字母数字字符 → 下划线
        let p = json_backend_path("a b/c");
        let fname = p.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(fname, "a_b_c.json", "非法字符应清理为下划线: {fname}");
        let p = json_backend_path("ok-name_1");
        let fname = p.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(fname, "ok-name_1.json");
    }
}
