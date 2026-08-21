// ============================================================
// 图文识别 — TextIn（合合信息）外部接口客户端
// 1. 证件分类 cert_classify：判定图片属于哪类证照
// 2. 按分类调用对应证件 OCR 接口，返回结构化字段
// 接口鉴权：请求头 x-ti-app-id / x-ti-secret-code
// 请求体：application/octet-stream（图片二进制）或 text/plain（图片 URL）
// ============================================================

use super::config::OcrConfig;

const BASE_URL: &str = "https://api.textin.com/robot/v1.0/api";
const CLASSIFY_PATH: &str = "cert_classify";
const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

/// 证件分类接口支持的全部类型（cert_classify 文档枚举）
pub const ALL_CATEGORIES: &[&str] = &[
    "id_card",
    "id_card_front",
    "id_card_back",
    "id_card_front_and_back",
    "drive_license",
    "vehicle_license",
    "bank_card",
    "business_card",
    "business_license",
    "passport",
    "hongkong_idcard",
    "macau_id_card",
    "social_security_cards",
    "family_register",
    "marriage_certificate",
    "divorce_certificate",
    "house_property_owner_ship",
    "real_estate",
    "opening_license",
    "organization_certificate",
    "vehicle_certificate",
    "vehicle_registration",
    "tax_certificate",
    "other",
];

/// 内置映射：分类 → TextIn OCR 接口名（可被配置覆盖）
pub const BUILTIN_MAPPING: &[(&str, &str)] = &[
    ("id_card", "id_card"),
    ("id_card_front", "id_card"),
    ("id_card_back", "id_card"),
    ("id_card_front_and_back", "id_card"),
    ("drive_license", "driver_license"),
    ("vehicle_license", "vehicle_license"),
    ("bank_card", "bank_card"),
    ("business_card", "business_card"),
    ("business_license", "business_license"),
    ("passport", "passport"),
    ("hongkong_idcard", "hk_id_card"),
    ("macau_id_card", "mac_id_card"),
    ("social_security_cards", "social_security_card"),
    ("organization_certificate", "organization_code_certificate"),
    ("opening_license", "account_opening_permit"),
];

/// 分类结果
#[derive(Debug, Clone)]
pub struct ClassifyOutcome {
    pub category: String,
    pub description: String,
}

/// OCR 结果
#[derive(Debug, Clone)]
pub struct OcrOutcome {
    pub fields: serde_json::Value,
}

/// 内置默认端点：分类 → OCR 接口名（无映射返回 None）
pub fn builtin_endpoint(category: &str) -> Option<&'static str> {
    BUILTIN_MAPPING
        .iter()
        .find(|(c, _)| *c == category)
        .map(|(_, e)| *e)
}

/// 解析某分类实际使用的 OCR 接口：
/// 1. 优先读配置（endpoint_map）：启用且填写了接口 → 使用配置值（可为 TextIn 接口名或完整 URL）
/// 2. 未配置时回落内置映射
pub fn resolve_endpoint(cfg: &OcrConfig, category: &str) -> Option<String> {
    if let Some(rule) = cfg.endpoint_map.get(category) {
        // 显式禁用：不调用 OCR（即使有内置映射）
        if !rule.enabled {
            return None;
        }
        let ep = rule.endpoint.trim();
        if !ep.is_empty() {
            return Some(ep.to_string());
        }
        // 启用但未填写接口 → 回落内置映射
    }
    builtin_endpoint(category).map(|e| e.to_string())
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        // 直连 api.textin.com（国内服务，本机直连可达）：
        // 避免终端环境代理（HTTPS_PROXY 等）失效时出现 tunnel 连接被拒
        .no_proxy()
        .build()
        .unwrap_or_default()
}

/// 完整展开 reqwest 错误链，便于定位（DNS / 连接 / TLS / 超时等）
fn describe_reqwest_error(e: &reqwest::Error) -> String {
    let mut out = format!("{e}");
    let mut src = std::error::Error::source(e);
    let mut depth = 0;
    while let Some(s) = src {
        if depth >= 4 {
            break;
        }
        out.push_str(" <- ");
        out.push_str(&s.to_string());
        src = std::error::Error::source(s);
        depth += 1;
    }
    out
}

/// POST 图片到指定接口；传输层错误（DNS/连接/超时/TLS）自动重试 3 次
async fn post_image(url: &str, cfg: &OcrConfig, bytes: &[u8]) -> Result<String, String> {
    let mut last_err: Option<reqwest::Error> = None;
    for attempt in 0..3u32 {
        let resp = client()
            .post(url)
            .headers(headers(cfg))
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(bytes.to_vec())
            .send()
            .await;
        match resp {
            Ok(r) => {
                return r
                    .text()
                    .await
                    .map_err(|e| format!("读取接口响应失败: {}", describe_reqwest_error(&e)))
            }
            Err(e) => {
                let retryable = e.is_timeout() || e.is_connect() || e.is_request();
                last_err = Some(e);
                if !retryable || attempt == 2 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
            }
        }
    }
    Err(format!(
        "请求接口失败: {}",
        last_err
            .as_ref()
            .map(describe_reqwest_error)
            .unwrap_or_else(|| "未知网络错误".to_string())
    ))
}

fn headers(cfg: &OcrConfig) -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    if let Ok(v) = reqwest::header::HeaderValue::from_str(&cfg.app_id) {
        h.insert("x-ti-app-id", v);
    }
    if let Ok(v) = reqwest::header::HeaderValue::from_str(&cfg.secret_code) {
        h.insert("x-ti-secret-code", v);
    }
    h
}

/// 校验凭证是否已配置
pub fn ensure_credentials(cfg: &OcrConfig) -> Result<(), String> {
    if !cfg.has_credentials() {
        return Err(
            "未配置 TextIn 凭证（x-ti-app-id / x-ti-secret-code），请先在图文识别设置中填写"
                .to_string(),
        );
    }
    Ok(())
}

fn check_file_size(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("资源内容为空".to_string());
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!(
            "资源文件超过 10M 上限（当前 {} bytes）",
            bytes.len()
        ));
    }
    Ok(())
}

/// 调用证件分类接口；返回 (原始 JSON, 分类, 分类描述)
pub async fn classify(cfg: &OcrConfig, bytes: &[u8]) -> Result<(String, ClassifyOutcome), String> {
    ensure_credentials(cfg)?;
    check_file_size(bytes)?;
    let url = format!("{BASE_URL}/{CLASSIFY_PATH}");
    let text = post_image(&url, cfg, bytes).await?;
    parse_classify(&text)
}

fn parse_classify(raw: &str) -> Result<(String, ClassifyOutcome), String> {
    let v: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("分类响应解析失败: {e}"))?;
    let code = v.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    // TextIn 成功返回 code=0 或 code=200（HTTP 风格）
    if code != 0 && code != 200 {
        let msg = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("未知错误")
            .to_string();
        return Err(format!("证件分类接口返回错误 (code={code}): {msg}"));
    }
    let result = v
        .get("result")
        .ok_or_else(|| "分类响应缺少 result 字段".to_string())?;
    let category = result
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("other")
        .to_string();
    let description = result
        .get("type_description")
        .and_then(|d| d.as_str())
        .unwrap_or(category.as_str())
        .to_string();
    Ok((
        raw.to_string(),
        ClassifyOutcome {
            category,
            description,
        },
    ))
}

/// 调用指定证件的 OCR 接口；endpoint 可为 TextIn 接口名（如 id_card）
/// 或完整 URL（自定义接口）；返回 (原始 JSON, 结构化字段 {key: value})
pub async fn ocr(
    cfg: &OcrConfig,
    endpoint: &str,
    bytes: &[u8],
) -> Result<(String, OcrOutcome), String> {
    ensure_credentials(cfg)?;
    check_file_size(bytes)?;
    let ep = endpoint.trim();
    if ep.is_empty() {
        return Err("OCR 接口为空".to_string());
    }
    let url = if ep.starts_with("http://") || ep.starts_with("https://") {
        ep.to_string()
    } else {
        format!("{BASE_URL}/{ep}")
    };
    let text = post_image(&url, cfg, bytes)
        .await
        .map_err(|e| format!("{e}（接口 {endpoint}）"))?;
    parse_ocr(&text)
}

fn parse_ocr(raw: &str) -> Result<(String, OcrOutcome), String> {
    let v: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("OCR 响应解析失败: {e}"))?;
    let code = v.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    // TextIn 成功返回 code=0 或 code=200（HTTP 风格）
    if code != 0 && code != 200 {
        let msg = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("未知错误")
            .to_string();
        return Err(format!("OCR 接口返回错误 (code={code}): {msg}"));
    }
    let result = v.get("result").cloned().unwrap_or(serde_json::Value::Null);
    let mut fields = serde_json::Map::new();
    if let Some(item_list) = result.get("item_list").and_then(|l| l.as_array()) {
        for item in item_list {
            if let (Some(key), Some(value)) = (
                item.get("key").and_then(|k| k.as_str()),
                item.get("value").and_then(|val| val.as_str()),
            ) {
                if !key.is_empty() {
                    fields.insert(
                        key.to_string(),
                        serde_json::Value::String(value.to_string()),
                    );
                }
            }
        }
    }
    // 补充顶层 type / type_description，便于前端直接展示
    for k in ["type", "type_description"] {
        if let Some(val) = result.get(k) {
            fields.insert(k.to_string(), val.clone());
        }
    }
    Ok((
        raw.to_string(),
        OcrOutcome {
            fields: serde_json::Value::Object(fields),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_mapping_core_categories() {
        assert_eq!(builtin_endpoint("id_card"), Some("id_card"));
        assert_eq!(builtin_endpoint("id_card_front"), Some("id_card"));
        assert_eq!(builtin_endpoint("drive_license"), Some("driver_license"));
        assert_eq!(builtin_endpoint("vehicle_license"), Some("vehicle_license"));
        assert_eq!(
            builtin_endpoint("business_license"),
            Some("business_license")
        );
        assert_eq!(builtin_endpoint("other"), None);
        assert_eq!(builtin_endpoint("marriage_certificate"), None);
    }

    #[test]
    fn resolve_endpoint_prefers_config() {
        use crate::ocr::config::EndpointRule;
        use std::collections::HashMap;
        let mut map = HashMap::new();
        // 自定义完整 URL
        map.insert(
            "drive_license".to_string(),
            EndpointRule {
                enabled: true,
                endpoint: "https://custom.example.com/ocr/driver".to_string(),
            },
        );
        // 显式禁用（覆盖内置映射）
        map.insert(
            "id_card".to_string(),
            EndpointRule {
                enabled: false,
                endpoint: "id_card".to_string(),
            },
        );
        // 未配置的分类回落内置
        let cfg = OcrConfig {
            endpoint_map: map,
            ..OcrConfig::default()
        };
        assert_eq!(
            resolve_endpoint(&cfg, "drive_license").as_deref(),
            Some("https://custom.example.com/ocr/driver")
        );
        assert_eq!(resolve_endpoint(&cfg, "id_card"), None);
        assert_eq!(
            resolve_endpoint(&cfg, "vehicle_license").as_deref(),
            Some("vehicle_license")
        );
        assert_eq!(resolve_endpoint(&cfg, "other"), None);
    }

    #[test]
    fn parse_classify_success() {
        let raw = r#"{"code":0,"message":"成功","result":{"type":"drive_license","type_description":"驾驶证","image_angle":0}}"#;
        let (_, out) = parse_classify(raw).expect("应解析成功");
        assert_eq!(out.category, "drive_license");
        assert_eq!(out.description, "驾驶证");
    }

    #[test]
    fn parse_classify_success_with_http_code() {
        // TextIn 实际接口成功时返回 code=200
        let raw = r#"{"code":200,"message":"success","result":{"type":"id_card","type_description":"身份证"}}"#;
        let (_, out) = parse_classify(raw).expect("code=200 应视为成功");
        assert_eq!(out.category, "id_card");
    }

    #[test]
    fn parse_classify_error_code() {
        let raw = r#"{"code":40102,"message":"凭证无效"}"#;
        let err = parse_classify(raw).unwrap_err();
        assert!(err.contains("40102"), "错误信息应包含错误码: {err}");
    }

    #[test]
    fn parse_ocr_extracts_fields() {
        let raw = r#"{"code":0,"result":{"type":"drive_license","item_list":[
            {"key":"name","value":"张三","description":"姓名","confidence":0.99},
            {"key":"drive_type","value":"C1","description":"准驾车型"}
        ]}}"#;
        let (_, out) = parse_ocr(raw).expect("应解析成功");
        let fields = out.fields.as_object().expect("字段应为对象");
        assert_eq!(fields.get("name").and_then(|v| v.as_str()), Some("张三"));
        assert_eq!(
            fields.get("drive_type").and_then(|v| v.as_str()),
            Some("C1")
        );
        assert_eq!(
            fields.get("type").and_then(|v| v.as_str()),
            Some("drive_license")
        );
    }

    #[test]
    fn parse_ocr_success_with_http_code() {
        let raw = r#"{"code":200,"result":{"type":"id_card","item_list":[{"key":"name","value":"李四"}]}}"#;
        let (_, out) = parse_ocr(raw).expect("code=200 应视为成功");
        assert_eq!(
            out.fields.get("name").and_then(|v| v.as_str()),
            Some("李四")
        );
    }

    #[test]
    fn file_size_limit() {
        assert!(check_file_size(b"").is_err());
        assert!(check_file_size(&[0u8; 11 * 1024 * 1024]).is_err());
        assert!(check_file_size(&[0u8; 1024]).is_ok());
    }
}
