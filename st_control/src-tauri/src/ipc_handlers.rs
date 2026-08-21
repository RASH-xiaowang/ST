// ============================================================
// 系统 / 数据库 IPC — 门面
// ============================================================

mod external;
mod internal;
mod maintain;
mod system;
mod tables;
pub use external::*;
pub use internal::*;
pub use maintain::*;
pub use system::*;
pub use tables::*;

/// 允许外部命令访问的根目录白名单（统一 data 目录 + 旧目录过渡 + 配置的扫描目录）
fn allowed_db_roots(db: &crate::db::Database) -> Vec<String> {
    let mut roots = Vec::new();
    roots.push(crate::common::st_data_dir().display().to_string());
    roots.push(crate::common::wechat_data_dir().display().to_string());
    // 旧版散落目录（未迁移完成时仍可浏览，迁移后目录被改名不再命中）
    if let Some(d) = dirs::data_dir() {
        roots.push(d.join("st-control").display().to_string());
        roots.push(d.join("st_result").display().to_string());
        roots.push(d.join("st_wechat").display().to_string());
    }
    if let Ok(items) = db.get_config() {
        for it in items {
            if it.key == "ext_db_dirs" {
                if let Ok(arr) = serde_json::from_str::<Vec<String>>(&it.value) {
                    roots.extend(arr);
                }
            }
        }
    }
    roots
}
