// ============================================================
// 微信富媒体消息解析 — 转账域
// 自 media.rs 拆分：转账状态标签（paysubtype → 文案/方向）与
// wcpayinfo XML 解析。
// ============================================================

use quick_xml::events::Event;
use quick_xml::Reader;

use super::collapse_text;

// ============ 转账状态标签 ============

const TRANSFER_PAYSUBTYPE_LABEL: &[(&str, &str)] = &[
    ("1", "待收款"),
    ("3", "已收款"),
    ("4", "已退还"),
    ("5", "已过期退回"),
    ("7", "待领取"),
    ("8", "已领取"),
];

/// 转账状态标签
pub(crate) fn transfer_label(paysubtype: &str) -> &str {
    TRANSFER_PAYSUBTYPE_LABEL
        .iter()
        .find(|&&(k, _)| k == paysubtype)
        .map(|&(_, v)| v)
        .unwrap_or("")
}

/// 从 fee_desc 清洗出金额文本（如 "￥150.00" → "150.00"）
pub(crate) fn clean_amount(fee_desc: &str) -> String {
    let s = fee_desc
        .trim()
        .trim_start_matches(['¥', '￥', ' '])
        .trim()
        .to_string();
    if s.is_empty() {
        String::new()
    } else {
        s
    }
}

/// 转账状态文案（区分我发起 / 我收到）
pub fn transfer_status_label(is_self: bool, paysubtype: &str) -> String {
    match paysubtype {
        "3" | "8" => {
            if is_self {
                "已被接收".to_string()
            } else {
                "已收款".to_string()
            }
        }
        "4" | "9" => "已退还".to_string(),
        "5" | "10" => "已过期退回".to_string(),
        "7" => "待领取".to_string(),
        "1" => {
            if is_self {
                "等待对方领取".to_string()
            } else {
                "待收款".to_string()
            }
        }
        _ => String::new(),
    }
}

/// 是否为转账“状态更新行”类型（即非发起行 paysubtype=1）。
///
/// 这类行的真实发送者是收款方（不是付款方），当库中只存了这一行时
/// （发起行缺失的异常数据），气泡方向需要取反，才能还原成
/// “我发出 → 已被接收 / 我收到 → 已收款”的正确显示。
pub fn is_transfer_status_type(paysubtype: &str) -> bool {
    matches!(paysubtype, "3" | "4" | "5" | "7" | "8" | "9" | "10")
}

// ============ 转账解析 ============

/// 提取转账信息 (appmsg type=2000)
#[allow(dead_code)] // 供测试与潜在外部调用使用（pub API 保留）
pub fn extract_transfer_info(xml: &str) -> Option<serde_json::Value> {
    extract_wcpayinfo(xml).map(|info| {
        let paysubtype = info.get("paysubtype").map(|s| s.as_str()).unwrap_or("");
        serde_json::json!({
            "paysubtype": paysubtype,
            "paysubtype_label": transfer_label(paysubtype),
            "fee_desc": info.get("feedesc"),
            "pay_memo": info.get("pay_memo"),
            "transcation_id": info.get("transcation_id"),
            "transfer_id": info.get("transfer_id"),
            "payer_username": info.get("payer_username"),
            "receiver_username": info.get("receiver_username"),
        })
    })
}

pub(crate) fn extract_wcpayinfo(xml: &str) -> Option<std::collections::HashMap<String, String>> {
    // 快速查找 wcpayinfo 区域
    let start = xml
        .find("<wcpayinfo>")
        .or_else(|| xml.find("<wcpayinfo\n"))?;
    let end = xml[start..].find("</wcpayinfo>")?;
    let info_str = &xml[start..start + end + 12];

    let mut reader = Reader::from_str(info_str);
    reader.config_mut().trim_text(true);

    let mut result = std::collections::HashMap::new();
    let mut current_field = String::new();
    let mut in_field = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                if name != "wcpayinfo" {
                    current_field = name;
                    in_field = true;
                }
            }
            Ok(Event::Text(ref e)) if in_field => {
                let text = e.unescape().unwrap_or_default().to_string();
                if !text.is_empty() {
                    // 优先存匹配大小写的原始字段名
                    let key = current_field.clone();
                    result.entry(key).or_insert(collapse_text(&text));
                }
            }
            Ok(Event::CData(ref e)) if in_field => {
                let text = String::from_utf8_lossy(e).to_string();
                if !text.is_empty() {
                    let key = current_field.clone();
                    result.entry(key).or_insert(collapse_text(&text));
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                if name == "wcpayinfo" {
                    break;
                }
                in_field = false;
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    // 确保 paysubtype 存在
    result.get("paysubtype").or_else(|| {
        // 尝试不同的大小写变体
        result
            .keys()
            .find(|k| k.to_lowercase() == "paysubtype")
            .and_then(|k| result.get(k))
    })?;

    Some(result)
}
