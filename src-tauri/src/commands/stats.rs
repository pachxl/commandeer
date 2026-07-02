//! Minimal system stats for the palette's task-manager widget: CPU from
//! GetSystemTimes deltas, RAM from GlobalMemoryStatusEx, GPU from the same
//! PDH performance counters Task Manager reads. State (previous CPU sample,
//! open PDH query) persists between polls.

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

/// (idle, kernel+user) cumulative 100ns ticks from the previous poll
#[cfg(target_os = "windows")]
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

    let (mut idle, mut kernel, mut user) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default());
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
        if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut buf_size, &mut count, Some(items)) != 0
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

#[tauri::command]
pub fn system_stats() -> SystemStats {
    #[cfg(target_os = "windows")]
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

    #[cfg(not(target_os = "windows"))]
    SystemStats { cpu: 0.0, mem_used: 0, mem_total: 0, mem_percent: 0.0, gpu: None }
}
