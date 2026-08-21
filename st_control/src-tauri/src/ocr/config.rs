// ============================================================
// 图文识别 — 配置持久化（TextIn 凭证 + HTTP 接收服务参数）
// 配置存于主库 control.db 的 _config 表（与系统其它配置同源）
// ============================================================

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::db::OcrDb;

/// 单个分类的 OCR 接口规则
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointRule {
    /// 是否对该分类调用 OCR
    pub enabled: bool,
    /// OCR 接口：TextIn 接口名（如 id_card）或完整 URL（自定义接口）
    pub endpoint: String,
}

impl Default for EndpointRule {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: String::new(),
        }
    }
}

/// 图文识别配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrConfig {
    /// TextIn x-ti-app-id
    pub app_id: String,
    /// TextIn x-ti-secret-code
    pub secret_code: String,
    /// 是否启用资源接收服务
    pub enabled: bool,
    /// 监听地址（默认 0.0.0.0，供外部推送）
    pub bind_host: String,
    /// 监听端口（默认 9787）
    pub port: u16,
    /// 访问令牌（空 = 免鉴权，仅建议内网使用）
    pub token: String,
    /// 是否先使用开源 OCR（RapidOCR，本地推理）预检：
    /// 图片识别出有效文本后才调用证件分类，过滤无文字图片，避免浪费分类接口
    pub precheck_enabled: bool,
    /// 预检文本最小字符数（低于该值视为"无有效文本"，跳过证件分类）
    pub precheck_min_chars: usize,
    /// RapidOCR 模型缓存目录（空 = 默认应用数据目录，首次使用自动下载）
    pub precheck_model_dir: String,
    /// 分类 → OCR 接口规则（可配置；未覆盖的分类用内置映射）
    #[serde(default)]
    pub endpoint_map: HashMap<String, EndpointRule>,
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            secret_code: String::new(),
            enabled: true,
            bind_host: "0.0.0.0".to_string(),
            port: 9787,
            token: String::new(),
            precheck_enabled: true,
            precheck_min_chars: 2,
            precheck_model_dir: String::new(),
            endpoint_map: default_endpoint_map(),
        }
    }
}

/// 默认分类 → 接口规则（覆盖全部 cert_classify 类型；有内置映射的默认启用）
pub fn default_endpoint_map() -> HashMap<String, EndpointRule> {
    super::textin::ALL_CATEGORIES
        .iter()
        .map(|cat| {
            let builtin = super::textin::builtin_endpoint(cat);
            (
                cat.to_string(),
                EndpointRule {
                    enabled: builtin.is_some(),
                    endpoint: builtin.unwrap_or_default().to_string(),
                },
            )
        })
        .collect()
}

impl OcrConfig {
    /// 从 _config 表加载配置（缺省值回落）
    pub fn load(db: &OcrDb) -> Self {
        let kv = db.get_config_map();
        let get = |k: &str| kv.get(k).cloned().unwrap_or_default();
        Self {
            app_id: get("ocr_app_id"),
            secret_code: get("ocr_secret_code"),
            enabled: get("ocr_enabled") != "0",
            bind_host: {
                let v = get("ocr_bind_host");
                if v.is_empty() {
                    "0.0.0.0".to_string()
                } else {
                    v
                }
            },
            port: kv
                .get("ocr_port")
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(9787),
            token: get("ocr_token"),
            precheck_enabled: get("ocr_precheck_enabled") != "0",
            precheck_min_chars: kv
                .get("ocr_precheck_min_chars")
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v >= 1)
                .unwrap_or(2),
            precheck_model_dir: get("ocr_precheck_model_dir"),
            endpoint_map: kv
                .get("ocr_endpoint_map")
                .and_then(|v| serde_json::from_str::<HashMap<String, EndpointRule>>(v).ok())
                .map(|mut map| {
                    // 补齐新出现的分类（用户升级后新类型也能配置）
                    let defaults = default_endpoint_map();
                    for (k, v) in defaults {
                        map.entry(k).or_insert(v);
                    }
                    map
                })
                .unwrap_or_else(default_endpoint_map),
        }
    }

    /// 持久化到 _config 表
    pub fn save(&self, db: &OcrDb) -> Result<(), String> {
        let mut items = vec![
            ("ocr_app_id".to_string(), self.app_id.clone()),
            ("ocr_secret_code".to_string(), self.secret_code.clone()),
            (
                "ocr_enabled".to_string(),
                if self.enabled {
                    "1".to_string()
                } else {
                    "0".to_string()
                },
            ),
            ("ocr_bind_host".to_string(), self.bind_host.clone()),
            ("ocr_port".to_string(), self.port.to_string()),
            ("ocr_token".to_string(), self.token.clone()),
            (
                "ocr_precheck_enabled".to_string(),
                if self.precheck_enabled {
                    "1".to_string()
                } else {
                    "0".to_string()
                },
            ),
            (
                "ocr_precheck_min_chars".to_string(),
                self.precheck_min_chars.to_string(),
            ),
            (
                "ocr_precheck_model_dir".to_string(),
                self.precheck_model_dir.clone(),
            ),
            (
                "ocr_endpoint_map".to_string(),
                serde_json::to_string(&self.endpoint_map)
                    .map_err(|e| format!("序列化接口映射失败: {e}"))?,
            ),
        ];
        for (k, v) in items.drain(..) {
            db.set_config(&k, &v)
                .map_err(|e| format!("保存配置 {k} 失败: {e}"))?;
        }
        Ok(())
    }

    /// 是否已配置有效的 TextIn 凭证
    pub fn has_credentials(&self) -> bool {
        !self.app_id.trim().is_empty() && !self.secret_code.trim().is_empty()
    }
}
