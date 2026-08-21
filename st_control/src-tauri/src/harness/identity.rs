// ============================================================
// Harness — 匿名身份（DSH identity 迁移）
//
// 匿名身份：首次启动生成固定 UUID 并持久化（data/harness/identity.json），
// 供遥测/会话归属使用；不收集任何个人信息。
// ============================================================

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HarnessIdentity {
    pub id: String,
    pub created_at: String,
}

fn identity_path() -> std::path::PathBuf {
    crate::common::st_data_dir()
        .join("harness")
        .join("identity.json")
}

fn identity() -> &'static HarnessIdentity {
    static I: OnceLock<HarnessIdentity> = OnceLock::new();
    I.get_or_init(|| {
        if let Ok(text) = std::fs::read_to_string(identity_path()) {
            if let Ok(id) = serde_json::from_str::<HarnessIdentity>(&text) {
                return id;
            }
        }
        let id = HarnessIdentity {
            id: format!("huser-{}", uuid::Uuid::new_v4().simple()),
            created_at: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        };
        if let Some(dir) = identity_path().parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let _ = std::fs::write(
            identity_path(),
            serde_json::to_string_pretty(&id).unwrap_or_default(),
        );
        id
    })
}

#[tauri::command]
pub async fn get_harness_identity() -> Result<HarnessIdentity, String> {
    Ok(identity().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_stable_and_well_formed() {
        let a = identity().clone();
        let b = identity().clone();
        assert_eq!(a.id, b.id, "同一进程内身份稳定");
        assert!(a.id.starts_with("huser-"));
        assert!(!a.created_at.is_empty());
    }
}
