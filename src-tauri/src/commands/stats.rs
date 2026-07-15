//! Minimal system stats for the palette's task-manager widget.
//! Windows: CPU from GetSystemTimes deltas, RAM from GlobalMemoryStatusEx,
//! GPU from the same PDH performance counters Task Manager reads.
//! Linux: CPU from /proc/stat deltas, RAM from /proc/meminfo, GPU from
//! amdgpu's sysfs busy percentage or nvidia-smi.
//! State (previous CPU sample, open PDH query / detected GPU source)
//! persists between polls.

use serde::Serialize;
use std::sync::Mutex;

#[derive(Serialize)]
pub struct SystemStats {
    /// 0-100; 0.0 on the very first poll (needs two samples)
    pub cpu: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub mem_percent: f32,
    /// None when GPU counters are unavailable or not yet primed
    pub gpu: Option<f32>,
}

/// (idle, total) cumulative CPU ticks from the previous poll — 100ns units on
/// Windows (kernel+user, kernel includes idle), jiffies on Linux
#[cfg(any(target_os = "windows", target_os = "linux"))]
static PREV_CPU: Mutex<Option<(u64, u64)>> = Mutex::new(None);

/// Open PDH (query, counter) handles; Err-like None after a failed init so
/// we don't retry every poll
#[cfg(target_os = "windows")]
static GPU_QUERY: Mutex<Option<Option<(isize, isize)>>> = Mutex::new(None);

#[cfg(target_os = "windows")]
fn filetime_u64(ft: windows::Win32::Foundation::FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

#[cfg(target_os = "windows")]
fn cpu_percent() -> f32 {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::GetSystemTimes;

    let (mut idle, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    if unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.is_err() {
        return 0.0;
    }
    let idle = filetime_u64(idle);
    // Kernel time includes idle time
    let total = filetime_u64(kernel) + filetime_u64(user);

    let mut prev = PREV_CPU.lock().unwrap();
    let pct = match *prev {
        Some((prev_idle, prev_total)) if total > prev_total => {
            let total_d = (total - prev_total) as f64;
            let idle_d = idle.saturating_sub(prev_idle) as f64;
            (((total_d - idle_d) / total_d) * 100.0) as f32
        }
        _ => 0.0,
    };
    *prev = Some((idle, total));
    pct.clamp(0.0, 100.0)
}

#[cfg(target_os = "windows")]
fn memory() -> (u64, u64, f32) {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    if unsafe { GlobalMemoryStatusEx(&mut status) }.is_err() {
        return (0, 0, 0.0);
    }
    let used = status.ullTotalPhys - status.ullAvailPhys;
    (used, status.ullTotalPhys, status.dwMemoryLoad as f32)
}

/// Sum of all 3D-engine utilization counters, clamped to 100 (Task Manager's
/// headline GPU number is essentially this). Returns None until the counter
/// has two samples, or if PDH setup failed.
#[cfg(target_os = "windows")]
fn gpu_percent() -> Option<f32> {
    use windows::core::w;
    use windows::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE,
    };

    const PDH_MORE_DATA: u32 = 0x800007D2;

    let mut guard = GPU_QUERY.lock().unwrap();
    if guard.is_none() {
        // First use: open the query and prime it with one collection
        *guard = Some(unsafe {
            let mut query: isize = 0;
            let mut counter: isize = 0;
            if PdhOpenQueryW(None, 0, &mut query) == 0
                && PdhAddEnglishCounterW(
                    query,
                    w!("\\GPU Engine(*engtype_3D)\\Utilization Percentage"),
                    0,
                    &mut counter,
                ) == 0
                && PdhCollectQueryData(query) == 0
            {
                Some((query, counter))
            } else {
                None
            }
        });
        return None;
    }
    let (query, counter) = (*guard)?.as_ref().copied()?;

    unsafe {
        if PdhCollectQueryData(query) != 0 {
            return None;
        }
        let mut buf_size: u32 = 0;
        let mut count: u32 = 0;
        if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut buf_size, &mut count, None)
            != PDH_MORE_DATA
        {
            return None;
        }
        let mut buf = vec![0u8; buf_size as usize];
        let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
        if PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut buf_size,
            &mut count,
            Some(items),
        ) != 0
        {
            return None;
        }
        let mut total = 0.0f64;
        for i in 0..count as usize {
            let item = &*items.add(i);
            if item.FmtValue.CStatus == 0 {
                total += item.FmtValue.Anonymous.doubleValue;
            }
        }
        Some((total as f32).clamp(0.0, 100.0))
    }
}

#[cfg(target_os = "linux")]
fn cpu_percent() -> f32 {
    // First line of /proc/stat: "cpu  user nice system idle iowait irq softirq steal ..."
    let stat = match std::fs::read_to_string("/proc/stat") {
        Ok(s) => s,
        Err(_) => return 0.0,
    };
    let vals: Vec<u64> = stat
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .skip(1)
        .filter_map(|v| v.parse().ok())
        .collect();
    if vals.len() < 4 {
        return 0.0;
    }
    // iowait counts as idle, like most task managers
    let idle = vals[3] + vals.get(4).copied().unwrap_or(0);
    let total: u64 = vals.iter().sum();

    let mut prev = PREV_CPU.lock().unwrap();
    let pct = match *prev {
        Some((prev_idle, prev_total)) if total > prev_total => {
            let total_d = (total - prev_total) as f64;
            let idle_d = idle.saturating_sub(prev_idle) as f64;
            (((total_d - idle_d) / total_d) * 100.0) as f32
        }
        _ => 0.0,
    };
    *prev = Some((idle, total));
    pct.clamp(0.0, 100.0)
}

#[cfg(target_os = "linux")]
fn memory() -> (u64, u64, f32) {
    fn kib(line: &str) -> u64 {
        line.split_whitespace()
            .nth(1)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
            * 1024
    }

    let info = match std::fs::read_to_string("/proc/meminfo") {
        Ok(s) => s,
        Err(_) => return (0, 0, 0.0),
    };
    let (mut total, mut avail) = (0u64, 0u64);
    for line in info.lines() {
        if line.starts_with("MemTotal:") {
            total = kib(line);
        } else if line.starts_with("MemAvailable:") {
            avail = kib(line);
        }
        if total > 0 && avail > 0 {
            break;
        }
    }
    if total == 0 {
        return (0, 0, 0.0);
    }
    let used = total.saturating_sub(avail);
    (used, total, ((used as f64 / total as f64) * 100.0) as f32)
}

#[cfg(target_os = "linux")]
enum GpuSource {
    /// amdgpu (and some others) expose a ready-made 0-100 busy percentage
    Amd(std::path::PathBuf),
    /// NVIDIA proprietary driver; queried via nvidia-smi
    NvidiaSmi,
}

/// Detected GPU source; outer None = not probed yet, inner None = no usable
/// source (so we don't re-probe every poll)
#[cfg(target_os = "linux")]
static GPU_SOURCE: Mutex<Option<Option<GpuSource>>> = Mutex::new(None);

#[cfg(target_os = "linux")]
fn nvidia_smi_utilization() -> Option<f32> {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    // One line per GPU; report the busiest
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<f32>().ok())
        .fold(None, |acc: Option<f32>, v| {
            Some(acc.map_or(v, |a| a.max(v)))
        })
        .map(|v| v.clamp(0.0, 100.0))
}

#[cfg(target_os = "linux")]
fn detect_gpu_source() -> Option<GpuSource> {
    if let Ok(cards) = std::fs::read_dir("/sys/class/drm") {
        for entry in cards.flatten() {
            let path = entry.path().join("device/gpu_busy_percent");
            if path.exists() {
                return Some(GpuSource::Amd(path));
            }
        }
    }
    if nvidia_smi_utilization().is_some() {
        return Some(GpuSource::NvidiaSmi);
    }
    None
}

#[cfg(target_os = "linux")]
fn gpu_percent() -> Option<f32> {
    let mut guard = GPU_SOURCE.lock().unwrap();
    match guard.get_or_insert_with(detect_gpu_source) {
        Some(GpuSource::Amd(path)) => std::fs::read_to_string(path)
            .ok()?
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v.clamp(0.0, 100.0)),
        Some(GpuSource::NvidiaSmi) => nvidia_smi_utilization(),
        None => None,
    }
}

// Async so the poll runs off the main thread: nvidia-smi is a subprocess
// (tens of ms, more if the dGPU was runtime-suspended) and must not jank the UI
#[tauri::command]
pub async fn system_stats() -> SystemStats {
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        let (mem_used, mem_total, mem_percent) = memory();
        SystemStats {
            cpu: cpu_percent(),
            mem_used,
            mem_total,
            mem_percent,
            gpu: gpu_percent(),
        }
    }

    #[cfg(target_os = "macos")]
    {
        let (mem_used, mem_total, mem_percent) = memory();
        SystemStats {
            cpu: cpu_percent(),
            mem_used,
            mem_total,
            mem_percent,
            gpu: gpu_percent(),
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    SystemStats {
        cpu: 0.0,
        mem_used: 0,
        mem_total: 0,
        mem_percent: 0.0,
        gpu: None,
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[tokio::test]
    async fn smoke_system_stats() {
        let stats = super::system_stats().await;
        assert!(
            (0.0..=100.0).contains(&stats.cpu),
            "cpu {} out of range",
            stats.cpu
        );
        assert!(stats.mem_total > 0, "mem_total should be positive");
        assert!(stats.mem_used <= stats.mem_total, "mem_used > mem_total");
        assert!(
            (0.0..=100.0).contains(&stats.mem_percent),
            "mem_percent {} out of range",
            stats.mem_percent
        );
    }
}

/// macOS system stats backed by `sysinfo` (already pulled in for process.rs).
/// A single cached `System` instance is kept across polls so CPU usage has a
/// previous sample to delta against.
#[cfg(target_os = "macos")]
static MAC_SYS: Mutex<Option<sysinfo::System>> = Mutex::new(None);

#[cfg(target_os = "macos")]
fn cpu_percent() -> f32 {
    use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind};

    let mut guard = MAC_SYS.lock().unwrap();
    let sys = guard.get_or_insert_with(|| {
        sysinfo::System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::nothing()),
        )
    });
    sys.refresh_cpu_usage();
    let cpus = sys.cpus();
    if cpus.is_empty() {
        return 0.0;
    }
    let total: f32 = cpus.iter().map(|c| c.cpu_usage()).sum();
    (total / cpus.len() as f32).clamp(0.0, 100.0)
}

#[cfg(target_os = "macos")]
fn memory() -> (u64, u64, f32) {
    use sysinfo::{MemoryRefreshKind, RefreshKind};

    let mut guard = MAC_SYS.lock().unwrap();
    let sys = guard.get_or_insert_with(|| {
        sysinfo::System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing()),
        )
    });
    sys.refresh_memory();
    let total = sys.total_memory();
    let used = sys.used_memory();
    let pct = if total > 0 {
        ((used as f64 / total as f64) * 100.0) as f32
    } else {
        0.0
    };
    (used, total, pct)
}

#[cfg(target_os = "macos")]
fn gpu_percent() -> Option<f32> {
    // No reliable, unprivileged cross-vendor GPU utilization metric on macOS.
    // ioreg/powermetrics exist but require root or report vendor-specific keys;
    // returning None keeps the widget hidden rather than showing stale zeroes.
    None
}
