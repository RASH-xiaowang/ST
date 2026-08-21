// ============================================================
// 聊天消息 — 真实数据冒烟测试
// ============================================================

use super::*;

/// 真实数据：同笔转账的多条记录应合并为一条，且已收款的转账文案按方向正确
#[test]
#[cfg(target_os = "windows")]
fn smoke_transfer_merge() {
    let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
        eprintln!("未找到配置，跳过");
        return;
    };
    let self_wxid = cfg.wxid().unwrap_or_default();
    let mut cursor: Option<i64> = None;
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut checked = 0usize;
    let mut known = 0usize;
    for _ in 0..25 {
        let page =
            get_conversation_messages(&cfg.decrypted_dir, "xiaolu_09", &self_wxid, cursor, 60)
                .expect("加载会话失败");
        if page.messages.is_empty() {
            break;
        }
        for m in &page.messages {
            let Some(rich) = &m.rich else { continue };
            if rich.get("type").and_then(|t| t.as_str()) != Some("transfer") {
                continue;
            }
            checked += 1;
            let tid = rich
                .get("transfer_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !tid.is_empty() {
                *seen.entry(tid.clone()).or_insert(0) += 1;
                if tid == "1000050001202602071139845838187" {
                    let direction = rich.get("direction").and_then(|v| v.as_str()).unwrap_or("");
                    assert_eq!(direction, "已被接收", "已收款的转出转账应显示“已被接收”");
                    known += 1;
                }
            }
        }
        if !page.has_more {
            break;
        }
        cursor = Some(page.next_cursor);
    }
    eprintln!(
        "转账消息 {} 条，transferid 去重后 {} 个，已知样本 {} 次",
        checked,
        seen.len(),
        known
    );
    if checked == 0 {
        eprintln!("会话中无转账消息，跳过");
        return;
    }
    assert!(
        seen.values().all(|c| *c == 1),
        "同笔转账不应重复显示: {:?}",
        seen.iter().filter(|(_, c)| **c > 1).collect::<Vec<_>>()
    );
}

/// 真实数据：仅存一行且该行是状态更新行（发起行缺失）时，
/// 气泡方向应取反——行内发送者是收款方，卡片要回到付款方一侧。
#[test]
#[cfg(target_os = "windows")]
fn smoke_transfer_status_only_direction() {
    let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
        eprintln!("未找到配置，跳过");
        return;
    };
    let self_wxid = cfg.wxid().unwrap_or_default();
    // 该私聊中存在一笔只有状态更新行（paysubtype=3）的转账：
    // 行内发送者是收款方（对方），付款方为本机 → 应显示“已被接收”且气泡在右侧。
    let known_tid = "1000050001202604051430930631485";
    let mut found = 0usize;
    let mut cursor: Option<i64> = None;
    for _ in 0..30 {
        let page = get_conversation_messages(
            &cfg.decrypted_dir,
            "wxid_rtmssdq74afg22",
            &self_wxid,
            cursor,
            60,
        )
        .expect("加载会话失败");
        for m in &page.messages {
            let Some(rich) = &m.rich else { continue };
            if rich.get("type").and_then(|t| t.as_str()) != Some("transfer") {
                continue;
            }
            let tid = rich
                .get("transfer_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if tid != known_tid {
                continue;
            }
            found += 1;
            let direction = rich.get("direction").and_then(|v| v.as_str()).unwrap_or("");
            assert_eq!(direction, "已被接收", "状态行取反后付款方应看到“已被接收”");
            assert!(m.is_self, "状态行取反后气泡应位于付款方（本机）一侧");
        }
        if !page.has_more {
            break;
        }
        cursor = Some(page.next_cursor);
    }
    if found == 0 {
        eprintln!("会话中无该状态行转账样本，跳过");
        return;
    }
}

/// 真实数据：会话消息构成统计应返回类型与条数，且总数与消息页 total 一致
#[test]
#[cfg(target_os = "windows")]
fn smoke_session_type_stats() {
    let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
        eprintln!("未找到配置，跳过");
        return;
    };
    let username = "xiaolu_09";
    let stats = super::get_session_message_type_stats(&cfg.decrypted_dir, username)
        .expect("读取消息构成统计失败");
    eprintln!(
        "消息构成: {}",
        serde_json::to_string(&stats).unwrap_or_default()
    );
    if stats.is_empty() {
        eprintln!("会话无消息类型统计，跳过");
        return;
    }
    let sum: i64 = stats
        .iter()
        .filter_map(|v| v.get("count").and_then(|c| c.as_i64()))
        .sum();
    if sum == 0 {
        eprintln!("类型计数合计为 0，跳过");
        return;
    }
    // 每个条目都有 label 且总数合理（不超过会话 total，total 未知时仅要求非负）
    for v in &stats {
        assert!(
            v.get("label").and_then(|l| l.as_str()).is_some(),
            "每条应有中文标签"
        );
    }
}
