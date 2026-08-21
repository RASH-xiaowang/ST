// ============================================================
// 微信配置 — 路径解析
// 自 config.rs 拆分：默认目录、应用基目录、wxid 目录规整。
// ============================================================

use std::path::PathBuf;

/// 默认微信数据根目录：`<应用基目录>/data/wechat`
/// （统一目录方案：全部资源收敛到应用目录下，不再使用 %APPDATA%/st_result）
pub fn default_st_result_dir() -> PathBuf {
    crate::common::wechat_data_dir()
}

/// 默认解密数据库输出目录：`<应用基目录>/data/wechat/decrypted`
pub fn default_decrypted_dir() -> PathBuf {
    default_st_result_dir().join("decrypted")
}

/// 默认解密图片输出目录：`<应用基目录>/data/wechat/decoded_images`
pub fn default_decoded_image_dir() -> PathBuf {
    default_st_result_dir().join("decoded_images")
}

/// 获取应用基目录（统一收敛到 common::app_base_dir，见其文档）
pub fn app_base_dir() -> PathBuf {
    crate::common::app_base_dir()
}

/// 从微信账号目录名中提取真实 wxid（去掉 `_<实例后缀>`）。
///
/// 目录名形如 `wxid_xxxxxx` 或 `wxid_xxxxxx_f312` / `wxid_xxxxxx_9bcd`；
/// 真实 wxid 为 `wxid_` 后到第二个下划线之间的部分（wxid 本身不含下划线）。
/// 非 `wxid_` 前缀或只有一段的名称原样返回。
pub fn normalize_wxid_dir(name: &str) -> String {
    let Some(pos) = name.find('_') else {
        return name.to_string();
    };
    if &name[..pos] != "wxid" {
        return name.to_string();
    }
    let rest = &name[pos + 1..];
    match rest.find('_') {
        Some(pos2) => format!("wxid_{}", &rest[..pos2]),
        None => name.to_string(),
    }
}
