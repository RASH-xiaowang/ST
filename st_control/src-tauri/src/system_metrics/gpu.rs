// 系统指标 — GPU（PDH + 回退采集链）
// 自 system_metrics.rs 拆分：PDH 引擎枚举聚合、nvidia-smi/
// PowerShell 回退、GPU 名称查询。

use std::sync::Mutex;
use std::time::Instant;

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};

use super::GpuHandle;

static GPU_FALLBACK_CACHE: Mutex<Option<(Instant, f32)>> = Mutex::new(None);

// ─────────────────────────── Windows GPU (PDH) ───────────────────────────

#[cfg(windows)]
pub(crate) fn open_gpu_query() -> Option<GpuHandle> {
    unsafe {
        let mut query: PDH_HQUERY = std::mem::zeroed();
        // Pdh* 返回 u32 错误码，ERROR_SUCCESS == 0
        if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
            return None;
        }
        // 通配符优先：可枚举各“进程×引擎”实例并取最忙引擎（同任务管理器口径）。
        // _Total 是所有引擎求和（≠任务管理器口径），仅在通配符不可用时作兜底。
        let paths = [
            "\\GPU Engine(*)\\Utilization Percentage",
            "\\GPU Engine(_Total)\\Utilization Percentage",
        ];
        for path in paths {
            let wpath: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let mut counter: PDH_HCOUNTER = std::mem::zeroed();
            if PdhAddEnglishCounterW(query, PCWSTR(wpath.as_ptr()), 0, &mut counter) == 0 {
                // 预热一次，避免首次读取返回 0
                let _ = PdhCollectQueryData(query);
                return Some(GpuHandle { query, counter });
            }
        }
        let _ = PdhCloseQuery(query);
        None
    }
}

#[cfg(windows)]
fn read_gpu(gpu: &mut Option<GpuHandle>) -> Option<f32> {
    let handle = gpu.as_mut()?;
    unsafe {
        // 通配符路径（\GPU Engine(*)\Utilization Percentage）是多实例计数器：
        // 必须先收集数据，再枚举所有实例并聚合，不能直接 PdhGetFormattedCounterValue。
        let _ = PdhCollectQueryData(handle.query);

        // 第一遍：查询所需缓冲区大小与实例数量（返回 PDH_MORE_DATA 属正常）
        let mut buf_size: u32 = 0;
        let mut item_count: u32 = 0;
        let r = PdhGetFormattedCounterArrayW(
            handle.counter,
            PDH_FMT_DOUBLE,
            &mut buf_size,
            &mut item_count,
            None,
        );
        if !(r == 0 || r == 1 || r == 0x800007D2/* PDH_MORE_DATA */) {
            return None;
        }
        if item_count == 0 {
            return None;
        }

        // 第二遍：按 PDH 要求的缓冲区大小分配（实例名字符串也在同一缓冲区，不能只按 count*sizeof）
        let item_size = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>() as u32;
        let byte_len = buf_size.max(item_count * item_size);
        let elem_count = byte_len.div_ceil(item_size) as usize;
        let mut items = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); elem_count];
        let mut got_count: u32 = 0;
        let r2 = PdhGetFormattedCounterArrayW(
            handle.counter,
            PDH_FMT_DOUBLE,
            &mut buf_size,
            &mut got_count,
            Some(items.as_mut_ptr()),
        );
        if r2 != 0 && r2 != 1 {
            return None;
        }

        // 聚合：任务管理器口径 = 最忙引擎（busiest engine）的利用率，不是求和也不是平均。
        // 通配符查询可能带回 _Total 伪实例（值为求和），必须排除。
        let best = busiest_engine(
            items
                .iter()
                .take(got_count.min(items.len() as u32) as usize)
                .filter_map(|item| {
                    // 0 = PDH_CSTATUS_VALID_DATA, 1 = PDH_CSTATUS_NEW_DATA（均视为有效）
                    if (item.FmtValue.CStatus != 0 && item.FmtValue.CStatus != 1)
                        || instance_is_total(item.szName)
                    {
                        return None;
                    }
                    let v = item.FmtValue.Anonymous.doubleValue as f32;
                    if v.is_finite() && v > 0.0 {
                        Some(v)
                    } else {
                        None
                    }
                }),
        );
        if let Some(v) = best {
            return Some(v.clamp(0.0, 100.0));
        }
    }
    None
}

/// 任务管理器 GPU 口径：取“最忙引擎”的利用率（max），而非所有引擎求和
pub(crate) fn busiest_engine(values: impl IntoIterator<Item = f32>) -> Option<f32> {
    let mut best: Option<f32> = None;
    for v in values {
        if !v.is_finite() || v <= 0.0 {
            continue;
        }
        best = Some(match best {
            Some(b) if b >= v => b,
            _ => v,
        });
    }
    best
}

#[cfg(windows)]
fn instance_is_total(name: windows::core::PWSTR) -> bool {
    unsafe { String::from_utf16_lossy(name.as_wide()) == "_Total" }
}

// ─────────────────────────── GPU 回退采集链 ───────────────────────────
// 优先级：PDH 原生枚举聚合 (最快) → nvidia-smi (NVIDIA) → PowerShell Get-Counter (慢但通用)
// 回退结果缓存 5 秒避免每轮调用

#[cfg(windows)]
pub(crate) fn collect_gpu_usage(pdh: &mut Option<GpuHandle>, gpu_name: &str) -> Option<f32> {
    // 1. PDH (原生最快)
    if let Some(v) = read_gpu(pdh) {
        return Some(v);
    }

    // 2. 检查全局缓存（避免每轮都调用慢速方法）
    {
        let cache = GPU_FALLBACK_CACHE.lock().unwrap();
        if let Some((t, v)) = *cache {
            if t.elapsed().as_secs_f64() < 5.0 {
                return Some(v);
            }
        }
    }

    // 3. 回退采集（AMD/Intel/虚拟显示器等一律走 PowerShell 性能计数器；仅 NVIDIA 优先 nvidia-smi）
    let v = if gpu_name.to_uppercase().contains("NVIDIA") {
        nvidia_smi_gpu_usage().or_else(powershell_gpu_usage)
    } else {
        powershell_gpu_usage()
    };

    // 全部采集方法失败时返回 None（前端显示 N/A），绝不把 0 伪装成真实读数
    match v {
        Some(v) => {
            *GPU_FALLBACK_CACHE.lock().unwrap() = Some((Instant::now(), v));
            Some(v)
        }
        None => None,
    }
}

/// 通过 nvidia-smi 获取 NVIDIA GPU 使用率（快速，仅 NVIDIA）
/// 多 GPU 时取最忙一块（与任务管理器“逐 GPU 展示”的单值口径一致）
fn nvidia_smi_gpu_usage() -> Option<f32> {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let mut best = 0.0f32;
    let mut count = 0u32;
    for line in s.lines() {
        if let Ok(v) = line.trim().parse::<f32>() {
            count += 1;
            if v > best {
                best = v;
            }
        }
    }
    if count > 0 {
        Some(best.clamp(0.0, 100.0))
    } else {
        None
    }
}

/// 通过 Windows 性能计数器获取 GPU 使用率（通用回退，AMD/Intel/NVIDIA 均适用）
/// 使用 \GPU Engine(*)\Utilization Percentage 通配符，取最忙引擎（同任务管理器口径）
fn powershell_gpu_usage() -> Option<f32> {
    let script = "try { $r = Get-Counter '\\GPU Engine(*)\\Utilization Percentage' -ErrorAction Stop; $v = ($r.CounterSamples | Where-Object { $_.InstanceName -ne '_Total' } | Measure-Object -Property CookedValue -Maximum).Maximum; if ($null -ne $v) { $v } else { '' } } catch { '' }";
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    s.trim().parse::<f64>().ok().map(|v| v as f32)
}

// ─────────────────────────── GPU 名称 ───────────────────────────

/// 取真实 GPU 名称：优先真实显卡（AMD/Intel/NVIDIA），排除虚拟显示器、Microsoft 基本显示适配器等。
pub(crate) fn query_gpu_name() -> String {
    #[cfg(windows)]
    {
        let script = r#"$names = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue | ForEach-Object { $_.Name }; $names | ForEach-Object { "{0}" -f $_ }"#;
        if let Ok(out) = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            let mut best: Option<(i32, String)> = None;
            for line in s.lines() {
                let name = line.trim();
                if name.is_empty() {
                    continue;
                }
                let upper = name.to_uppercase();
                let score = if upper.contains("VIRTUAL")
                    || upper.contains("BASIC DISPLAY")
                    || upper.contains("REMOTE")
                {
                    0
                } else if upper.contains("AMD")
                    || upper.contains("RADEON")
                    || upper.contains("NVIDIA")
                    || upper.contains("INTEL")
                {
                    2
                } else {
                    1
                };
                if score > 0 && best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
                    best = Some((score, name.to_string()));
                }
            }
            if let Some((_, name)) = best {
                return name;
            }
        }
    }
    "GPU".to_string()
}
