//! 关系图谱性能基准（示例二进制，与 GUI 相同链接方式）
//!
//! 用法：cargo run --example graph_bench
//! 输出首次（冷统计）与二次（缓存命中）耗时，便于验证性能优化。

fn main() {
    let cfg = st_control_lib::wechat::config::WeChatConfig::load().expect("加载微信配置失败");
    let self_username = cfg.wxid().unwrap_or_default();

    let t0 = std::time::Instant::now();
    let v = st_control_lib::wechat::insights::build_relationship_graph(
        &cfg.decrypted_dir,
        &self_username,
        Some(80),
        Some(27),
        None,
    )
    .expect("首次生成关系图谱失败");
    let first_ms = t0.elapsed().as_millis();

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
    println!(
        "首次(冷统计): {}ms  节点={} 边={} 总消息={}",
        first_ms, nodes, edges, total
    );

    let t1 = std::time::Instant::now();
    let v2 = st_control_lib::wechat::insights::build_relationship_graph(
        &cfg.decrypted_dir,
        &self_username,
        Some(80),
        Some(27),
        None,
    )
    .expect("二次生成关系图谱失败");
    let second_ms = t1.elapsed().as_millis();
    println!("二次(内存缓存): {}ms", second_ms);
    let _ = v2;
}
