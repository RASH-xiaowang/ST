// ============================================================
// 年度总结 — 测试
// 自 annual.rs 拆分：真实解密库冒烟测试。
// ============================================================

use super::*;

#[test]
#[ignore = "需要真实解密库"]
fn smoke_annual_summary_real() {
    let cfg = crate::wechat::config::WeChatConfig::load().expect("加载微信配置");
    let years = available_years(&cfg.decrypted_dir);
    assert!(!years.is_empty(), "没有任何可统计年份");
    eprintln!("可用年份: {:?}", years);
    let s = annual_summary(&cfg.decrypted_dir, years[0]).expect("年度总结");
    assert!(s.total_messages > 0);
    assert!(s.active_days > 0);
    assert_eq!(s.monthly_counts.len(), 12);
    // 前端派生逻辑的等价校验：占比均在 0-100，最佳月份即月度最大值
    let total = s.total_messages;
    let heat: Vec<Vec<i64>> =
        serde_json::from_value(s.heatmap["matrix"].clone()).unwrap_or_default();
    let heat_total: i64 = heat.iter().map(|r| r.iter().sum::<i64>()).sum();
    let night: i64 = heat
        .iter()
        .map(|r| [23usize, 0, 1, 2, 3, 4].iter().map(|&h| r[h]).sum::<i64>())
        .sum();
    let weekend: i64 = if heat.len() >= 7 {
        heat[5].iter().sum::<i64>() + heat[6].iter().sum::<i64>()
    } else {
        0
    };
    let night_pct = if heat_total > 0 {
        night as f64 / heat_total as f64 * 100.0
    } else {
        0.0
    };
    let weekend_pct = if heat_total > 0 {
        weekend as f64 / heat_total as f64 * 100.0
    } else {
        0.0
    };
    assert!((0.0..=100.0).contains(&night_pct));
    assert!((0.0..=100.0).contains(&weekend_pct));
    let best = s
        .monthly_counts
        .iter()
        .enumerate()
        .max_by_key(|(_, &c)| c)
        .map(|(i, _)| i);
    eprintln!(
        "year={} total={} active_days={} chars={} avg={:.1} text={} heat_total={} night={:.1}% weekend={:.1}% best_month={:?}",
        s.year, s.total_messages, s.active_days, s.total_chars, s.avg_chars,
        s.text_messages, heat_total, night_pct, weekend_pct, best
    );
    eprintln!("monthly: {:?}", s.monthly_counts);
    eprintln!("kinds: {:?}", s.kind_counts);
    eprintln!(
        "top_contacts: {:?}",
        s.top_contacts
            .iter()
            .map(|t| (&t.name, t.count))
            .collect::<Vec<_>>()
    );
    eprintln!(
        "top_groups: {:?}",
        s.top_groups
            .iter()
            .map(|t| (&t.name, t.count))
            .collect::<Vec<_>>()
    );
    eprintln!(
        "top_emojis: {:?}",
        s.top_emojis
            .iter()
            .map(|t| (&t.key, t.count))
            .collect::<Vec<_>>()
    );
    eprintln!(
        "top_phrases: {:?}",
        s.top_phrases
            .iter()
            .map(|t| (&t.key, t.count))
            .collect::<Vec<_>>()
    );
    assert!((total > 0 && !s.top_contacts.is_empty()) || s.top_contacts.is_empty());
}
