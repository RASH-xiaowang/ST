// ============================================================
// 社交关系图谱 — 真实数据冒烟测试
// ============================================================

use super::*;

/// 真实数据冒烟：关系图谱应返回节点与边
#[test]
#[cfg(target_os = "windows")]
fn graph_smoke_real_data() {
    let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
        eprintln!("未找到配置，跳过");
        return;
    };
    if !cfg
        .decrypted_dir
        .join("session")
        .join("session.db")
        .exists()
    {
        eprintln!("解密库不存在，跳过");
        return;
    }
    let t0 = std::time::Instant::now();
    let v = build_relationship_graph(
        &cfg.decrypted_dir,
        &cfg.wechat_base_dir,
        &cfg.wxid().unwrap_or_default(),
        Some(40),
        Some(12),
        None,
    )
    .expect("生成关系图谱失败");
    let first_ms = t0.elapsed().as_millis();
    let t1 = std::time::Instant::now();
    let _v2 = build_relationship_graph(
        &cfg.decrypted_dir,
        &cfg.wechat_base_dir,
        &cfg.wxid().unwrap_or_default(),
        Some(40),
        Some(12),
        None,
    )
    .expect("二次生成失败");
    let second_ms = t1.elapsed().as_millis();
    let nodes = v
        .get("nodes")
        .and_then(|x| x.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let edges = v
        .get("edges")
        .and_then(|x| x.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let total = v
        .get("summary")
        .and_then(|s| s.get("total_messages"))
        .and_then(|x| x.as_i64())
        .unwrap_or(0);
    let self_avatar = v
        .get("self_avatar")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.starts_with("data:") {
                format!("data:…{} 字符", s.len())
            } else {
                s.to_string()
            }
        })
        .unwrap_or_else(|| "（空）".to_string());
    eprintln!(
        "关系图谱: 节点 {} 边 {} 总消息 {} 我头像={} 首次 {}ms 二次(缓存) {}ms",
        nodes, edges, total, self_avatar, first_ms, second_ms
    );
    assert!(nodes >= 1, "至少应包含「我」节点");
    assert!(edges > 0, "应存在至少一条关系边");
    assert!(total > 0, "应统计到消息总数");
    let self_id = v.get("self").and_then(|x| x.as_str()).unwrap_or("");
    assert!(!self_id.is_empty());
}
