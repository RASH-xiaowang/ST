// ============================================================
// 微信 general.db 记录查询 — 测试
// 自 general_records.rs 拆分：真实数据冒烟测试。
// ============================================================

use super::*;

/// 验证 general.db 记录类查询（本机真实数据）
#[test]
#[cfg(target_os = "windows")]
fn smoke_general_records() {
    if super::db::general_db_path().is_none() {
        eprintln!("未找到 general.db，跳过");
        return;
    }
    let r = list_revokes(Some(10), Some(0), None).unwrap();
    println!(
        "撤回: total={} items={}",
        r["total"],
        r["items"].as_array().map(|a| a.len()).unwrap_or(0)
    );
    let r = list_transfers(Some(10), Some(0), None).unwrap();
    println!(
        "转账: total={} items={}",
        r["total"],
        r["items"].as_array().map(|a| a.len()).unwrap_or(0)
    );
    let r = list_red_envelopes(Some(10), Some(0), None).unwrap();
    println!(
        "红包: total={} items={}",
        r["total"],
        r["items"].as_array().map(|a| a.len()).unwrap_or(0)
    );
    let r = list_finder(Some(10), Some(0)).unwrap();
    println!(
        "视频号: total={} items={}",
        r["total"],
        r["items"].as_array().map(|a| a.len()).unwrap_or(0)
    );
    let r = list_mini_programs(Some(10), Some(0), None).unwrap();
    println!(
        "小程序: total={} items={}",
        r["total"],
        r["items"].as_array().map(|a| a.len()).unwrap_or(0)
    );
    let r = list_friend_verifications(Some(10), Some(0), None).unwrap();
    println!(
        "好友验证: total={} items={}",
        r["total"],
        r["items"].as_array().map(|a| a.len()).unwrap_or(0)
    );
}
