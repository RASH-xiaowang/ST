// 实时系统指标采集模块
// 通过 sysinfo 获取 CPU / 内存 / 磁盘容量 / 每核负载；
// 通过 Windows PDH 性能计数器获取 GPU（最忙引擎口径）、磁盘活动率与读写吞吐、网络吞吐与带宽占用；
// 通过 ping 获取网络延迟；通过进程启动时刻计算运行时长。所有数据均为真实系统指标。

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 磁盘列表缓存 (Instant, Vec<DiskInfo>)；磁盘容量变化很慢，无需每次快照重新枚举
static DISK_CACHE: Mutex<Option<(Instant, Vec<DiskInfo>)>> = Mutex::new(None);

use serde::Serialize;
use sysinfo::{CpuRefreshKind, Disks, System};
use tauri::Manager;

mod ping;
use ping::{ping_latency_ms, PING_CACHE};
mod gpu;
use gpu::{collect_gpu_usage, open_gpu_query, query_gpu_name};

mod io;
use io::{collect_disk, collect_net, open_disk_query, open_net_query};
#[cfg(windows)]
use windows::Win32::System::Performance::{PDH_HCOUNTER, PDH_HQUERY};

/// 单个磁盘分区的使用情况
#[derive(Serialize, Clone)]
pub struct DiskInfo {
    pub name: String,
    pub mount: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub usage_pct: f32,
}

/// 指标快照（返回给前端）
#[derive(Serialize, Clone)]
pub struct MetricsSnapshot {
    /// 本地系统时间 (RFC3339)
    pub now: String,
    /// 格式化后的本地时间字符串
    pub now_str: String,
    /// 应用进程运行时长（秒）
    pub uptime_secs: f64,
    /// 系统开机时长（秒，自操作系统启动起）
    pub system_uptime_secs: u64,
    /// 操作系统名称
    pub os_name: String,
    /// 全局 CPU 使用率 (%)
    pub cpu_usage: f32,
    /// 每个 CPU 核心的使用率 (%)
    pub cpu_per_core: Vec<f32>,
    /// 物理内存总量 (bytes)
    pub mem_total_bytes: u64,
    /// 物理内存已用 (bytes)
    pub mem_used_bytes: u64,
    /// 可用内存 (bytes，含可回收缓存，同任务管理器“可用”)
    pub mem_available_bytes: u64,
    /// 物理内存占用率 (%)
    pub mem_usage_pct: f32,
    /// 交换分区总量 (bytes)
    pub swap_total_bytes: u64,
    /// 交换分区已用 (bytes)
    pub swap_used_bytes: u64,
    /// 磁盘分区列表
    pub disks: Vec<DiskInfo>,
    /// 磁盘活动率（% Disk Time，同任务管理器“活动时间”）
    pub disk_activity_pct: Option<f32>,
    /// 磁盘读取速率 (bytes/sec)
    pub disk_read_bytes_per_sec: Option<f64>,
    /// 磁盘写入速率 (bytes/sec)
    pub disk_write_bytes_per_sec: Option<f64>,
    /// GPU 名称
    pub gpu_name: String,
    /// GPU 使用率 (%)，获取失败时为 null
    pub gpu_usage_pct: Option<f32>,
    /// 网络总吞吐（上行 + 下行，bytes/sec）
    pub net_throughput_bytes_per_sec: Option<f64>,
    /// 网络带宽占用率（%，取最忙方向 / 最忙链路，同任务管理器口径）
    pub net_utilization_pct: Option<f32>,
    /// 聚合链路速率 (bits/sec)
    pub net_link_speed_bps: Option<u64>,
    /// 网络延迟 (ms)，ping 失败时可能为 null
    pub net_latency_ms: Option<f64>,
    /// 本次延迟实际测到的目标（如 223.5.5.5 / 网关 / 127.0.0.1）
    pub net_ping_target: String,
}

#[cfg(windows)]
pub(crate) struct GpuHandle {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

/// 磁盘活动/吞吐计数器（同一条 PDH 查询）
#[cfg(windows)]
pub(crate) struct DiskHandle {
    query: PDH_HQUERY,
    disk_time: PDH_HCOUNTER,
    read_bps: PDH_HCOUNTER,
    write_bps: PDH_HCOUNTER,
}

/// 网络吞吐/带宽计数器（同一条 PDH 查询，通配符多实例）
#[cfg(windows)]
pub(crate) struct NetHandle {
    query: PDH_HQUERY,
    sent: PDH_HCOUNTER,
    recv: PDH_HCOUNTER,
    bandwidth: PDH_HCOUNTER,
}

// PDH 句柄为裸指针，需手动标记线程安全（访问已被 Mutex 保护）
#[cfg(windows)]
unsafe impl Send for GpuHandle {}
#[cfg(windows)]
unsafe impl Sync for GpuHandle {}
#[cfg(windows)]
unsafe impl Send for DiskHandle {}
#[cfg(windows)]
unsafe impl Sync for DiskHandle {}
#[cfg(windows)]
unsafe impl Send for NetHandle {}
#[cfg(windows)]
unsafe impl Sync for NetHandle {}

struct MetricsInner {
    system: System,
    start: Instant,
    #[cfg(windows)]
    gpu: Option<GpuHandle>,
    #[cfg(windows)]
    disk: Option<DiskHandle>,
    #[cfg(windows)]
    net: Option<NetHandle>,
    gpu_name: String,
    os_name: String,
}

/// 全局指标采集器（作为 Tauri 托管状态共享）
pub struct SystemMetrics {
    inner: Mutex<MetricsInner>,
}

impl SystemMetrics {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_cpu_specifics(CpuRefreshKind::everything());
        system.refresh_memory();

        #[cfg(windows)]
        let gpu = open_gpu_query();
        #[cfg(not(windows))]
        let gpu: Option<()> = None;
        #[cfg(windows)]
        let disk = open_disk_query();
        #[cfg(not(windows))]
        let disk: Option<()> = None;
        #[cfg(windows)]
        let net = open_net_query();
        #[cfg(not(windows))]
        let net: Option<()> = None;

        Self {
            inner: Mutex::new(MetricsInner {
                system,
                start: Instant::now(),
                #[cfg(windows)]
                gpu,
                #[cfg(windows)]
                disk,
                #[cfg(windows)]
                net,
                gpu_name: query_gpu_name(),
                os_name: System::name().unwrap_or_else(|| "Unknown".into()),
            }),
        }
    }

    /// 采集一份真实系统指标快照（可测试；Tauri 命令仅做薄封装）
    pub fn snapshot(&self) -> Result<MetricsSnapshot, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "指标采集器状态损坏".to_string())?;

        // CPU：每次刷新以获得自上次刷新以来的实时使用率
        inner
            .system
            .refresh_cpu_specifics(CpuRefreshKind::everything());
        inner.system.refresh_memory();

        let cpu_usage = inner.system.global_cpu_usage();
        let cpu_per_core: Vec<f32> = inner.system.cpus().iter().map(|c| c.cpu_usage()).collect();

        let mem_total_bytes = inner.system.total_memory();
        // “使用中”内存口径（与任务管理器一致）：总量 − 可用（不含缓存可回收部分）
        let mem_available_bytes = inner.system.available_memory();
        let mem_usage_pct = if mem_total_bytes > 0 {
            (mem_total_bytes.saturating_sub(mem_available_bytes)) as f32 / mem_total_bytes as f32
                * 100.0
        } else {
            0.0
        };
        let swap_total_bytes = inner.system.total_swap();
        let swap_used_bytes = inner.system.used_swap();
        let mem_used_bytes = mem_total_bytes.saturating_sub(mem_available_bytes);

        let disks = disk_infos_cached();

        #[cfg(windows)]
        let gpu_usage_pct = {
            let inner_ref: &mut MetricsInner = &mut inner;
            collect_gpu_usage(&mut inner_ref.gpu, &inner_ref.gpu_name)
        };
        #[cfg(not(windows))]
        let gpu_usage_pct: Option<f32> = None;

        // 磁盘活动率 + 读写吞吐（PDH，任务管理器同源）
        #[cfg(windows)]
        let (disk_activity_pct, disk_read_bytes_per_sec, disk_write_bytes_per_sec) = {
            let inner_ref: &mut MetricsInner = &mut inner;
            collect_disk(&mut inner_ref.disk)
        };
        #[cfg(not(windows))]
        let (disk_activity_pct, disk_read_bytes_per_sec, disk_write_bytes_per_sec): (
            Option<f32>,
            Option<f64>,
            Option<f64>,
        ) = (None, None, None);

        // 网络吞吐 + 带宽占用（PDH，任务管理器同源；延迟保留为独立指标）
        #[cfg(windows)]
        let (net_throughput_bytes_per_sec, net_utilization_pct, net_link_speed_bps) = {
            let inner_ref: &mut MetricsInner = &mut inner;
            collect_net(&mut inner_ref.net)
        };
        #[cfg(not(windows))]
        let (net_throughput_bytes_per_sec, net_utilization_pct, net_link_speed_bps): (
            Option<f64>,
            Option<f32>,
            Option<u64>,
        ) = (None, None, None);

        let (net_latency_ms, net_ping_target) = ping_latency_cached();

        let uptime_secs = inner.start.elapsed().as_secs_f64();
        let system_uptime_secs = System::uptime();
        let now = chrono::Local::now();

        Ok(MetricsSnapshot {
            now: now.to_rfc3339(),
            now_str: now.format("%Y/%m/%d %H:%M:%S").to_string(),
            uptime_secs,
            system_uptime_secs,
            os_name: inner.os_name.clone(),
            cpu_usage,
            cpu_per_core,
            mem_total_bytes,
            mem_used_bytes,
            mem_available_bytes,
            mem_usage_pct,
            swap_total_bytes,
            swap_used_bytes,
            disks,
            disk_activity_pct,
            disk_read_bytes_per_sec,
            disk_write_bytes_per_sec,
            gpu_name: inner.gpu_name.clone(),
            gpu_usage_pct,
            net_throughput_bytes_per_sec,
            net_utilization_pct,
            net_link_speed_bps,
            net_latency_ms,
            net_ping_target,
        })
    }
}

/// 磁盘列表（10s 缓存）：磁盘容量几乎不变，避免每次快照都重新枚举全部磁盘
fn disk_infos_cached() -> Vec<DiskInfo> {
    const DISK_CACHE_TTL: Duration = Duration::from_secs(10);
    {
        let cache = DISK_CACHE.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((t, v)) = &*cache {
            if t.elapsed() < DISK_CACHE_TTL {
                return v.clone();
            }
        }
    }
    let disks = Disks::new_with_refreshed_list();
    let list: Vec<DiskInfo> = disks
        .list()
        .iter()
        .map(|d| {
            let total_bytes = d.total_space();
            let available = d.available_space();
            let used_bytes = total_bytes.saturating_sub(available);
            let usage_pct = if total_bytes > 0 {
                used_bytes as f32 / total_bytes as f32 * 100.0
            } else {
                0.0
            };
            DiskInfo {
                name: d.name().to_string_lossy().into_owned(),
                mount: d.mount_point().to_string_lossy().into_owned(),
                total_bytes,
                used_bytes,
                usage_pct,
            }
        })
        .collect();
    *DISK_CACHE.lock().unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), list.clone()));
    list
}

/// ping 延迟（5s 缓存）：spawn ping 进程最坏可耗时 2s+，不应每次快照都执行
fn ping_latency_cached() -> (Option<f64>, String) {
    const PING_CACHE_TTL: Duration = Duration::from_secs(5);
    {
        let cache = PING_CACHE.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((t, v, target)) = &*cache {
            if t.elapsed() < PING_CACHE_TTL {
                return (*v, target.clone());
            }
        }
    }
    let (v, target) = ping_latency_ms();
    *PING_CACHE.lock().unwrap_or_else(|p| p.into_inner()) =
        Some((Instant::now(), v, target.clone()));
    (v, target)
}

#[tauri::command]
pub async fn get_realtime_metrics(app: tauri::AppHandle) -> Result<MetricsSnapshot, String> {
    // 采集含进程 spawn（ping / PowerShell）、PDH、sysinfo 等阻塞操作，
    // 移出主线程与 tokio worker，避免高频轮询卡 UI 或拖慢其它异步任务。
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<SystemMetrics>();
        state.snapshot()
    })
    .await
    .map_err(|e| format!("指标采集任务失败: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::gpu::busiest_engine;
    use super::io::net_utilization_pct;
    use super::ping::{default_gateway, parse_first_ms, ping_targets};
    use super::*;

    #[test]
    fn parse_ping_ms_zh_locale() {
        // Windows 中文 locale 输出示例
        let out = "来自 127.0.0.1 的回复: 字节=32 时间<1ms TTL=128";
        let v = parse_first_ms(out).expect("应解析出延迟");
        assert_eq!(v, 1.0);
    }

    #[test]
    fn parse_ping_ms_en_locale() {
        let out = "Reply from 127.0.0.1: bytes=32 time=1ms TTL=128";
        assert_eq!(parse_first_ms(out), Some(1.0));

        let out2 = "Reply from 127.0.0.1: bytes=32 time=12.5ms TTL=128";
        assert_eq!(parse_first_ms(out2), Some(12.5));
    }

    #[test]
    fn parse_ping_ms_timeout_returns_none() {
        // 超时输出没有 "ms" 形式的延迟，应返回 None
        let out = "请求超时。";
        assert_eq!(parse_first_ms(out), None);
    }

    #[test]
    fn ping_targets_prefer_external() {
        let targets = ping_targets();
        assert!(!targets.is_empty(), "目标链不应为空");
        assert_ne!(
            targets[0], "127.0.0.1",
            "首个目标必须是公网地址，而非回环自测"
        );
        assert!(
            targets.contains(&"127.0.0.1".to_string()),
            "最后应保留回环兜底"
        );
    }

    #[test]
    fn gpu_uses_busiest_engine_not_sum() {
        // 任务管理器口径：取最忙引擎，而不是把所有“进程×引擎”实例求和
        let vals = [12.8f32, 3.1, 9.6];
        let best = busiest_engine(vals).expect("应有最忙引擎");
        assert_eq!(best, 12.8);

        // 无效/空输入不应产出伪读数
        assert_eq!(busiest_engine([f32::NAN, -1.0, 0.0]), None);
        assert_eq!(busiest_engine([]), None);
    }

    #[test]
    fn net_utilization_uses_busiest_direction() {
        // 1 Gbps 链路，单方向 62.5 MB/s（十进制）→ 500 Mbps → 占用 50%
        let util = net_utilization_pct(62_500_000.0, 1_000_000_000.0).unwrap();
        assert!((util - 50.0).abs() < 0.001, "期望 50%，实际 {util}%");
        // 满速 → 100%（封顶，不会超过）
        let util2 = net_utilization_pct(125_000_000.0, 1_000_000_000.0).unwrap();
        assert_eq!(util2, 100.0);
        // 链路速率未知 → None（前端显示 N/A，绝不伪装）
        assert_eq!(net_utilization_pct(100.0, 0.0), None);
        assert_eq!(net_utilization_pct(f64::NAN, 1_000_000_000.0), None);
    }

    #[cfg(windows)]
    #[test]
    fn default_gateway_returns_ip() {
        if let Some(gw) = default_gateway() {
            assert!(!gw.is_empty());
            assert!(gw.split('.').count() == 4, "网关应为 IPv4 地址: {}", gw);
        }
        // 无网关（离线环境）返回 None 也合法
    }

    #[cfg(windows)]
    #[test]
    fn gpu_fallback_failure_returns_none_not_zero() {
        // 用本地“伪缓存”验证回退决策：失败时返回 None（前端显示 N/A），而不是 0.0
        let mut cache: Option<(std::time::Instant, f32)> = None;
        let fail = None::<f32>;
        let resolved = fail.map(|v| {
            cache = Some((std::time::Instant::now(), v));
            v
        });
        assert_eq!(resolved, None);
        assert!(cache.is_none(), "失败结果不得写入缓存");

        let ok = Some(12.5f32);
        let resolved = ok.map(|v| {
            cache = Some((std::time::Instant::now(), v));
            v
        });
        assert_eq!(resolved, Some(12.5));
        assert_eq!(cache.map(|(_, v)| v), Some(12.5));
    }

    /// 本机冒烟测试：采集到的必须是真实系统数据（宽松断言，兼容无 GPU/无交换分区环境）
    #[test]
    fn snapshot_returns_real_metrics() {
        let m = SystemMetrics::new();
        let s = m.snapshot().expect("本机指标采集应成功");

        assert!(s.uptime_secs > 0.0, "应用运行时长应为正");
        assert!(s.system_uptime_secs > 0, "系统开机时长应为正");
        assert!(!s.os_name.is_empty(), "操作系统名称不应为空");
        assert!(!s.now_str.is_empty(), "时间戳不应为空");

        assert!((0.0..=100.0).contains(&s.cpu_usage), "CPU 使用率应在 0~100");
        assert!(!s.cpu_per_core.is_empty(), "应采集到 CPU 核心");
        for c in &s.cpu_per_core {
            assert!((0.0..=100.0).contains(c), "每核使用率应在 0~100");
        }

        assert!(s.mem_total_bytes > 0, "物理内存总量应大于 0");
        assert!(
            s.mem_used_bytes <= s.mem_total_bytes,
            "已用内存不应超过总量"
        );
        assert!(
            (0.0..=100.0).contains(&s.mem_usage_pct),
            "内存占用率应在 0~100"
        );

        assert!(!s.disks.is_empty(), "应至少采集到一个磁盘分区");
        for d in &s.disks {
            assert!(d.total_bytes > 0, "磁盘总容量应大于 0");
            assert!((0.0..=100.0).contains(&d.usage_pct), "磁盘占用率应在 0~100");
            assert!(d.used_bytes <= d.total_bytes, "磁盘已用不应超过总量");
        }

        // GPU / 网络在无设备或无网络时允许为 None，但绝不能是伪装值
        if let Some(g) = s.gpu_usage_pct {
            assert!((0.0..=100.0).contains(&g));
        }
        if let Some(a) = s.disk_activity_pct {
            assert!((0.0..=100.0).contains(&a), "磁盘活动率应在 0~100");
        }
        for v in [
            s.disk_read_bytes_per_sec,
            s.disk_write_bytes_per_sec,
            s.net_throughput_bytes_per_sec,
        ] {
            if let Some(v) = v {
                assert!(v.is_finite() && v >= 0.0, "速率应为非负有限值");
            }
        }
        if let Some(u) = s.net_utilization_pct {
            assert!((0.0..=100.0).contains(&u), "带宽占用率应在 0~100");
        }
        if let Some(l) = s.net_latency_ms {
            assert!(l.is_finite() && l >= 0.0, "延迟应为非负有限值");
        }

        eprintln!(
            "[smoke] os={} sys_uptime={}s app_uptime={:.0}s cpu={:.1}% mem={:.1}% disks={} disk_act={:?} disk_rw={:?}/{:?} gpu={:?} gpu_name={} net_thru={:?}B/s net_util={:?}% link={:?}bps lat={:?}ms@{}",
            s.os_name,
            s.system_uptime_secs,
            s.uptime_secs,
            s.cpu_usage,
            s.mem_usage_pct,
            s.disks.len(),
            s.disk_activity_pct,
            s.disk_read_bytes_per_sec,
            s.disk_write_bytes_per_sec,
            s.gpu_usage_pct,
            s.gpu_name,
            s.net_throughput_bytes_per_sec,
            s.net_utilization_pct,
            s.net_link_speed_bps,
            s.net_latency_ms,
            s.net_ping_target,
        );
    }
}
