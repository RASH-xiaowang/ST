//! 微信数据库密钥管理模块
//!
//! 管理 `all_keys.json` 的加载、查询和路径兼容。
//!
//! all_keys.json 格式:
//! ```json
//! {
//!   "session/session.db": { "enc_key": "49851dc5...", "salt": "abcd...", "size_mb": 1.5 },
//!   "message/message_0.db": { ... },
//!   "_key_format": "wx_key_v4.1",
//!   "_db_dir": "D:\\\\xwechat_files\\\\..."
//! }
//! ```
//! 以 `_` 开头的键为元数据，不是实际数据库密钥。

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// 单个数据库的密钥信息
#[derive(Debug, Clone, Deserialize)]
pub struct KeyInfo {
    pub enc_key: String,
    #[serde(default)]
    pub salt: Option<String>,
    #[serde(default)]
    pub size_mb: Option<f64>,
}

/// 密钥集合（已剥离元数据）
#[derive(Debug)]
pub struct Keys {
    /// 相对路径 → 密钥信息
    pub entries: HashMap<String, KeyInfo>,
    /// 密钥格式 (None = v4.0, Some("wx_key_v4.1") = PBKDF2 passphrase)
    pub key_format: Option<String>,
    /// 数据库中记录的 db_dir (仅参考)
    pub db_dir: Option<String>,
}

impl Keys {
    /// 从 all_keys.json 文件加载密钥
    pub fn from_file(path: &Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| std::io::Error::new(e.kind(), format!("读取密钥文件失败: {}", e)))?;
        Self::from_json(&content)
    }

    /// 从 JSON 字符串解析密钥
    pub fn from_json(json: &str) -> std::io::Result<Self> {
        let raw: HashMap<String, serde_json::Value> = serde_json::from_str(json).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("解析密钥 JSON 失败: {}", e),
            )
        })?;

        let mut entries = HashMap::new();
        let mut key_format = None;
        let mut db_dir = None;

        for (k, v) in raw {
            if k.starts_with('_') {
                // 元数据字段
                match k.as_str() {
                    "_key_format" => key_format = v.as_str().map(String::from),
                    "_db_dir" => db_dir = v.as_str().map(String::from),
                    _ => {}
                }
            } else {
                // 数据库密钥
                if let Ok(info) = serde_json::from_value::<KeyInfo>(v) {
                    entries.insert(k, info);
                }
            }
        }

        Ok(Self {
            entries,
            key_format,
            db_dir,
        })
    }

    /// 按相对路径查找密钥，自动兼容不同平台分隔符
    pub fn get_key_info(&self, rel_path: &str) -> Option<&KeyInfo> {
        if !is_safe_rel_path(rel_path) {
            return None;
        }
        for variant in key_path_variants(rel_path) {
            if !variant.starts_with('_') {
                if let Some(info) = self.entries.get(&variant) {
                    return Some(info);
                }
            }
        }
        None
    }

    /// 数据库条目数量
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// 检查路径不包含 `..` 等遍历组件
fn is_safe_rel_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    !normalized.split('/').any(|component| component == "..")
}

/// 生成同一路径的多种分隔符表示，兼容 Windows/Linux JSON key
fn key_path_variants(rel_path: &str) -> Vec<String> {
    let normalized = rel_path.replace('\\', "/");
    let mut variants = Vec::with_capacity(4);

    for candidate in [
        rel_path.to_string(),
        normalized.clone(),
        normalized.replace('/', "\\"),
        normalized.replace('/', std::path::MAIN_SEPARATOR_STR),
    ] {
        if !variants.contains(&candidate) {
            variants.push(candidate);
        }
    }
    variants
}

/// 从密钥映射中移除以下划线开头的元数据字段
pub fn strip_metadata(keys: &HashMap<String, serde_json::Value>) -> HashMap<String, KeyInfo> {
    let mut result = HashMap::new();
    for (k, v) in keys {
        if !k.starts_with('_') {
            if let Ok(info) = serde_json::from_value::<KeyInfo>(v.clone()) {
                result.insert(k.clone(), info);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_path() {
        assert!(is_safe_rel_path("session/session.db"));
        assert!(is_safe_rel_path("message/message_0.db"));
        assert!(!is_safe_rel_path("../etc/passwd"));
        assert!(!is_safe_rel_path("session/../../etc"));
    }

    #[test]
    fn test_key_path_variants() {
        let v = key_path_variants("message/message_0.db");
        assert!(v.contains(&"message/message_0.db".to_string()));
        assert!(v.contains(&"message\\message_0.db".to_string()));
    }

    #[test]
    fn test_parse_keys_json() {
        let json = r#"{
            "session/session.db": {
                "enc_key": "49851dc532ed4aa1af8a3980315c416a4f8a6ae8756f4bb18dbf2521bd7f30a8",
                "salt": "abcd1234efgh5678",
                "size_mb": 1.5
            },
            "message/message_0.db": {
                "enc_key": "49851dc532ed4aa1af8a3980315c416a4f8a6ae8756f4bb18dbf2521bd7f30a8",
                "size_mb": 128.0
            },
            "_key_format": "wx_key_v4.1",
            "_db_dir": "D:\\xwechat_files\\wxid_abc\\db_storage"
        }"#;

        let keys = Keys::from_json(json).unwrap();
        assert_eq!(keys.entries.len(), 2);
        assert_eq!(keys.key_format.as_deref(), Some("wx_key_v4.1"));
        assert!(keys.db_dir.as_deref().unwrap().contains("xwechat_files"));

        let info = keys.get_key_info("session/session.db");
        assert!(info.is_some());
        assert_eq!(info.unwrap().enc_key.len(), 64);

        // Windows 分隔符
        let info2 = keys.get_key_info("session\\session.db");
        assert!(info2.is_some());
    }

    #[test]
    fn test_parse_v4_0_format() {
        let json = r#"{
            "session/session.db": {
                "enc_key": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
            }
        }"#;

        let keys = Keys::from_json(json).unwrap();
        assert_eq!(keys.key_format, None);
        assert!(keys.get_key_info("session/session.db").is_some());

        // 不存在的 key
        assert!(keys.get_key_info("nonexistent.db").is_none());
    }

    #[test]
    fn test_get_key_info_with_traversal() {
        let json = r#"{"session/session.db": { "enc_key": "0000000000000000000000000000000000000000000000000000000000000000" }}"#;
        let keys = Keys::from_json(json).unwrap();
        assert!(keys.get_key_info("../etc/passwd").is_none());
    }
}
