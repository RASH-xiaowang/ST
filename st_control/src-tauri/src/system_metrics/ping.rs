// 系统指标 — 网络延迟（ping）
// 自 system_metrics.rs 拆分：探测目标链、默认网关、延迟解析与缓存。

use std::sync::Mutex;
use std::time::Instant;

pub(crate) static PING_CACHE: Mutex<Option<(Instant, Option<f64>, String)>> = Mutex::new(None);

// ─────────────────────────── 网络延迟 (ping) ───────────────────────────

/// 延迟探测目标链：优先公网（阿里 DNS），其次本机默认网关，最后回环兜底。
/// 保证展示的“网络延迟”是真实网络可达性（而非始终 1ms 的回环自测）。
pub(crate) fn ping_targets() -> Vec<String> {
    let mut targets = vec!["223.5.5.5".to_string()];
    if let Some(gw) = default_gateway() {
        targets.push(gw);
    }
    targets.push("127.0.0.1".to_string());
    targets
}

/// 取本机默认网关（IPv4）
#[cfg(windows)]
pub(crate) fn default_gateway() -> Option<String> {
    let script = "(Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue | Sort-Object RouteMetric | Select-Object -First 1).NextHop";
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let ip = s.trim().to_string();
    if ip.is_empty() || !ip.starts_with(|c: char| c.is_ascii_digit()) {
        None
    } else {
        Some(ip)
    }
}

#[cfg(not(windows))]
pub(crate) fn default_gateway() -> Option<String> {
    None
}

pub(crate) fn ping_latency_ms() -> (Option<f64>, String) {
    for target in ping_targets() {
        let out = std::process::Command::new("ping")
            .args(["-n", "1", "-w", "700", &target])
            .output()
            .ok();
        if let Some(out) = out {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(v) = parse_first_ms(&s) {
                return (Some(v), target);
            }
        }
    }
    (None, "不可达".to_string())
}

/// 在 ping 输出中查找第一个 "…<Nms" / "…=Nms" 形式的延迟值（兼容中英文 locale）
pub(crate) fn parse_first_ms(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    let pos = text.find("ms")?; // 'm' 的字节索引
    let mut end = pos; // 数字紧贴 'm' 之前
    while end > 0 && (bytes[end - 1] == b' ' || bytes[end - 1] == b'<') {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && (bytes[start - 1].is_ascii_digit() || bytes[start - 1] == b'.') {
        start -= 1;
    }
    if start < end {
        if let Ok(v) = text[start..end].parse::<f64>() {
            return Some(v);
        }
    }
    None
}
