// 系统指标 — 磁盘 / 网络 IO（PDH）
// 自 system_metrics.rs 拆分：磁盘活动/吞吐、网络吞吐/带宽占用。

use std::collections::HashMap;

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhGetFormattedCounterValue, PdhOpenQueryW, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W,
    PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};

use super::{DiskHandle, NetHandle};

// ─────────────────────────── 磁盘活动 / 网络吞吐 (PDH) ───────────────────────────

#[cfg(windows)]
pub(crate) fn open_disk_query() -> Option<DiskHandle> {
    unsafe {
        let mut query: PDH_HQUERY = std::mem::zeroed();
        if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
            return None;
        }
        let add_counter = |path: &str| -> Option<PDH_HCOUNTER> {
            let wpath: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let mut counter: PDH_HCOUNTER = std::mem::zeroed();
            if PdhAddEnglishCounterW(query, PCWSTR(wpath.as_ptr()), 0, &mut counter) == 0 {
                Some(counter)
            } else {
                None
            }
        };
        let disk_time = match add_counter("\\PhysicalDisk(_Total)\\% Disk Time") {
            Some(c) => c,
            None => {
                let _ = PdhCloseQuery(query);
                return None;
            }
        };
        let read_bps = match add_counter("\\PhysicalDisk(_Total)\\Disk Read Bytes/sec") {
            Some(c) => c,
            None => {
                let _ = PdhCloseQuery(query);
                return None;
            }
        };
        let write_bps = match add_counter("\\PhysicalDisk(_Total)\\Disk Write Bytes/sec") {
            Some(c) => c,
            None => {
                let _ = PdhCloseQuery(query);
                return None;
            }
        };
        // 预热：速率类计数器需要两次收集之间的时间差才能算出速率
        let _ = PdhCollectQueryData(query);
        Some(DiskHandle {
            query,
            disk_time,
            read_bps,
            write_bps,
        })
    }
}

#[cfg(windows)]
pub(crate) fn open_net_query() -> Option<NetHandle> {
    unsafe {
        let mut query: PDH_HQUERY = std::mem::zeroed();
        if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
            return None;
        }
        let add_counter = |path: &str| -> Option<PDH_HCOUNTER> {
            let wpath: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let mut counter: PDH_HCOUNTER = std::mem::zeroed();
            if PdhAddEnglishCounterW(query, PCWSTR(wpath.as_ptr()), 0, &mut counter) == 0 {
                Some(counter)
            } else {
                None
            }
        };
        let sent = match add_counter("\\Network Interface(*)\\Bytes Sent/sec") {
            Some(c) => c,
            None => {
                let _ = PdhCloseQuery(query);
                return None;
            }
        };
        let recv = match add_counter("\\Network Interface(*)\\Bytes Received/sec") {
            Some(c) => c,
            None => {
                let _ = PdhCloseQuery(query);
                return None;
            }
        };
        let bandwidth = match add_counter("\\Network Interface(*)\\Current Bandwidth") {
            Some(c) => c,
            None => {
                let _ = PdhCloseQuery(query);
                return None;
            }
        };
        let _ = PdhCollectQueryData(query);
        Some(NetHandle {
            query,
            sent,
            recv,
            bandwidth,
        })
    }
}

/// 读取单个 PDH 计数器的格式化值（double）
#[cfg(windows)]
unsafe fn fmt_counter_double(counter: PDH_HCOUNTER) -> Option<f64> {
    let mut val = PDH_FMT_COUNTERVALUE::default();
    let r = PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, None, &mut val);
    if r != 0 && r != 1 {
        return None;
    }
    // 0 = PDH_CSTATUS_VALID_DATA, 1 = PDH_CSTATUS_NEW_DATA
    if val.CStatus == 0 || val.CStatus == 1 {
        let v = val.Anonymous.doubleValue;
        if v.is_finite() {
            return Some(v);
        }
    }
    None
}

/// 读取通配符多实例计数器，返回 实例名 -> 值 的映射（跳过 _Total 伪实例）
#[cfg(windows)]
unsafe fn counter_array_map(counter: PDH_HCOUNTER) -> HashMap<String, f64> {
    let mut map = HashMap::new();
    let mut buf_size: u32 = 0;
    let mut item_count: u32 = 0;
    let r = PdhGetFormattedCounterArrayW(
        counter,
        PDH_FMT_DOUBLE,
        &mut buf_size,
        &mut item_count,
        None,
    );
    if !(r == 0 || r == 1 || r == 0x800007D2/* PDH_MORE_DATA */) {
        return map;
    }
    if item_count == 0 {
        return map;
    }

    // 按 PDH 要求的缓冲区大小分配（实例名字符串也在同一缓冲区，不能只按 count*sizeof）
    let item_size = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>() as u32;
    let byte_len = buf_size.max(item_count * item_size);
    let elem_count = byte_len.div_ceil(item_size) as usize;
    let mut items = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); elem_count];
    let mut got: u32 = 0;
    let r2 = PdhGetFormattedCounterArrayW(
        counter,
        PDH_FMT_DOUBLE,
        &mut buf_size,
        &mut got,
        Some(items.as_mut_ptr()),
    );
    if r2 != 0 && r2 != 1 {
        return map;
    }

    for item in items.iter().take(got.min(items.len() as u32) as usize) {
        // 0 = PDH_CSTATUS_VALID_DATA, 1 = PDH_CSTATUS_NEW_DATA
        if item.FmtValue.CStatus == 0 || item.FmtValue.CStatus == 1 {
            let v = item.FmtValue.Anonymous.doubleValue;
            if v.is_finite() {
                let name = item.szName.to_string().unwrap_or_default();
                if !name.is_empty() && name != "_Total" {
                    map.insert(name, v);
                }
            }
        }
    }
    map
}

/// 排除回环/隧道等非物理网卡实例，避免虚链路吞吐污染面板
#[cfg(windows)]
fn is_network_instance(name: &str) -> bool {
    let upper = name.to_uppercase();
    !(upper.contains("LOOPBACK") || upper.contains("ISATAP") || upper.contains("TEREDO"))
}

/// 网络带宽占用率：把最忙方向的字节速率换算为比特，除以链路速率（同任务管理器口径）
pub(crate) fn net_utilization_pct(direction_bytes_per_sec: f64, link_bps: f64) -> Option<f32> {
    if !direction_bytes_per_sec.is_finite() || !link_bps.is_finite() || link_bps <= 0.0 {
        return None;
    }
    Some(((direction_bytes_per_sec * 8.0) / link_bps * 100.0).clamp(0.0, 100.0) as f32)
}

#[cfg(windows)]
pub(crate) fn collect_disk(h: &mut Option<DiskHandle>) -> (Option<f32>, Option<f64>, Option<f64>) {
    let Some(handle) = h.as_mut() else {
        return (None, None, None);
    };
    unsafe {
        let _ = PdhCollectQueryData(handle.query);
        let activity = fmt_counter_double(handle.disk_time).map(|v| v.clamp(0.0, 100.0) as f32);
        let read = fmt_counter_double(handle.read_bps);
        let write = fmt_counter_double(handle.write_bps);
        (activity, read, write)
    }
}

#[cfg(windows)]
pub(crate) fn collect_net(h: &mut Option<NetHandle>) -> (Option<f64>, Option<f32>, Option<u64>) {
    let Some(handle) = h.as_mut() else {
        return (None, None, None);
    };
    unsafe {
        let _ = PdhCollectQueryData(handle.query);
        let sent_map = counter_array_map(handle.sent);
        let recv_map = counter_array_map(handle.recv);
        let bw_map = counter_array_map(handle.bandwidth);

        let mut sent_sum = 0.0f64;
        let mut recv_sum = 0.0f64;
        let mut bw_sum = 0.0f64;
        let mut busiest_link_util = 0.0f32;
        let mut any_bw = false;

        for (name, &bw) in &bw_map {
            if !is_network_instance(name) || !bw.is_finite() || bw <= 0.0 {
                continue;
            }
            any_bw = true;
            let s = sent_map.get(name).copied().unwrap_or(0.0);
            let r = recv_map.get(name).copied().unwrap_or(0.0);
            sent_sum += s;
            recv_sum += r;
            bw_sum += bw;
            // 单条链路取“最忙方向”（发送/接收取大者），多链路聚合取各链路的最大占用率
            if let Some(util) = net_utilization_pct(s.max(r), bw) {
                if util > busiest_link_util {
                    busiest_link_util = util;
                }
            }
        }

        if any_bw {
            let throughput = if sent_sum > 0.0 || recv_sum > 0.0 {
                Some(sent_sum + recv_sum)
            } else {
                None
            };
            (
                throughput,
                Some(busiest_link_util.clamp(0.0, 100.0)),
                Some(bw_sum as u64),
            )
        } else {
            // 链路速率不可用（个别虚拟网卡）时，仍尽力给出吞吐
            let mut s2 = 0.0f64;
            let mut r2 = 0.0f64;
            for (name, &v) in &sent_map {
                if is_network_instance(name) {
                    s2 += v;
                }
            }
            for (name, &v) in &recv_map {
                if is_network_instance(name) {
                    r2 += v;
                }
            }
            let throughput = if s2 > 0.0 || r2 > 0.0 {
                Some(s2 + r2)
            } else {
                None
            };
            (throughput, None, None)
        }
    }
}
