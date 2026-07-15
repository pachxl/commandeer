use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub memory_bytes: u64,
    /// Full path to the executable, for resolving the app icon
    pub exe_path: Option<String>,
}

#[tauri::command]
pub async fn list_processes() -> Result<Vec<ProcessInfo>, String> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(windows_processes)
            .await
            .map_err(|e| e.to_string())?
    }

    #[cfg(target_os = "macos")]
    {
        tokio::task::spawn_blocking(macos_processes)
            .await
            .map_err(|e| e.to_string())?
    }

    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(linux_processes)
            .await
            .map_err(|e| e.to_string())?
    }
}

/// Best-effort synchronous process snapshot shared by the app-running indicator
/// (`running_app_paths`). Returns an empty list rather than an error so a
/// transient enumeration failure just hides the dots instead of surfacing.
pub(crate) fn snapshot_processes() -> Vec<ProcessInfo> {
    #[cfg(target_os = "windows")]
    {
        windows_processes().unwrap_or_default()
    }
    #[cfg(target_os = "macos")]
    {
        macos_processes().unwrap_or_default()
    }
    #[cfg(target_os = "linux")]
    {
        linux_processes().unwrap_or_default()
    }
}

#[cfg(target_os = "windows")]
fn windows_processes() -> Result<Vec<ProcessInfo>, String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::{
        EnumProcesses, K32GetModuleBaseNameW, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_INFORMATION,
        PROCESS_VM_READ,
    };

    let mut pids = vec![0u32; 8192];
    let mut needed: u32 = 0;
    unsafe {
        EnumProcesses(
            pids.as_mut_ptr(),
            (pids.len() * std::mem::size_of::<u32>()) as u32,
            &mut needed,
        )
        .map_err(|e| e.to_string())?;
    }
    let count = (needed as usize) / std::mem::size_of::<u32>();

    let mut out = Vec::new();
    for &pid in &pids[..count] {
        if pid == 0 {
            continue;
        }
        unsafe {
            let Ok(handle) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
            else {
                continue;
            };
            let mut name_buf = [0u16; 260];
            let len = K32GetModuleBaseNameW(handle, None, &mut name_buf);
            let mut mem = PROCESS_MEMORY_COUNTERS {
                cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                ..Default::default()
            };
            let _ = K32GetProcessMemoryInfo(handle, &mut mem, mem.cb);
            let mut path_buf = [0u16; 1024];
            let mut path_len = path_buf.len() as u32;
            let exe_path = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(path_buf.as_mut_ptr()),
                &mut path_len,
            )
            .ok()
            .filter(|_| path_len > 0)
            .map(|_| String::from_utf16_lossy(&path_buf[..path_len as usize]).replace('\\', "/"));
            let _ = CloseHandle(handle);
            if len == 0 {
                continue;
            }
            out.push(ProcessInfo {
                pid,
                name: String::from_utf16_lossy(&name_buf[..len as usize]),
                memory_bytes: mem.WorkingSetSize as u64,
                exe_path,
            });
        }
    }
    Ok(out)
}

#[cfg(target_os = "macos")]
fn macos_processes() -> Result<Vec<ProcessInfo>, String> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_memory()
            .with_exe(UpdateKind::OnlyIfNotSet),
    );
    let out = sys
        .processes()
        .iter()
        .map(|(pid, p)| ProcessInfo {
            pid: pid.as_u32(),
            name: p.name().to_string_lossy().into_owned(),
            memory_bytes: p.memory(),
            exe_path: p.exe().map(|e| e.to_string_lossy().into_owned()),
        })
        .collect();
    Ok(out)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    // Real sysinfo enumeration; the test process itself must be in the list.
    #[test]
    fn smoke_list_processes() {
        let procs = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(super::list_processes())
            .expect("list_processes");
        assert!(!procs.is_empty(), "no processes listed");
        let me = std::process::id();
        assert!(
            procs.iter().any(|p| p.pid == me),
            "own pid {me} missing from the process list"
        );
    }
}

/// Walk /proc: numeric dirs are pids. Kernel threads (empty cmdline) are
/// skipped so the list matches the "real apps" the Windows enumeration shows.
#[cfg(target_os = "linux")]
fn linux_processes() -> Result<Vec<ProcessInfo>, String> {
    let entries = std::fs::read_dir("/proc").map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let proc_dir = entry.path();

        let cmdline = std::fs::read(proc_dir.join("cmdline")).unwrap_or_default();
        if cmdline.is_empty() {
            continue; // kernel thread
        }

        // exe readlink fails (EACCES) for other users' processes; keep the
        // entry, just without an icon path. Deleted binaries keep a marker.
        let exe_path = std::fs::read_link(proc_dir.join("exe"))
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .map(|p| p.trim_end_matches(" (deleted)").to_string());

        let name = exe_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| {
                std::fs::read_to_string(proc_dir.join("comm"))
                    .ok()
                    .map(|c| c.trim().to_string())
            })
            .filter(|n| !n.is_empty());
        let Some(name) = name else {
            continue;
        };

        // VmRSS from /proc/PID/status (kB); avoids needing the page size.
        let memory_bytes = std::fs::read_to_string(proc_dir.join("status"))
            .ok()
            .and_then(|status| {
                status.lines().find_map(|line| {
                    line.strip_prefix("VmRSS:")?
                        .trim()
                        .trim_end_matches("kB")
                        .trim()
                        .parse::<u64>()
                        .ok()
                })
            })
            .unwrap_or(0)
            * 1024;

        out.push(ProcessInfo {
            pid,
            name,
            memory_bytes,
            exe_path,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn kill_process(pid: u32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(move || {
            use windows::Win32::Foundation::CloseHandle;
            use windows::Win32::System::Threading::{
                OpenProcess, TerminateProcess, PROCESS_TERMINATE,
            };
            unsafe {
                let handle = OpenProcess(PROCESS_TERMINATE, false, pid)
                    .map_err(|e| format!("could not open process {pid}: {e}"))?;
                let result = TerminateProcess(handle, 1);
                let _ = CloseHandle(handle);
                result.map_err(|e| format!("could not terminate process {pid}: {e}"))
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }

    #[cfg(target_os = "macos")]
    {
        // SIGKILL to match the Windows TerminateProcess semantics: forceful,
        // no chance for the target to ignore it.
        let ret = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
        if ret == 0 {
            Ok(())
        } else {
            Err(format!(
                "could not kill process {pid}: {}",
                std::io::Error::last_os_error()
            ))
        }
    }

    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(move || {
            // SIGTERM first for a graceful exit; escalate to SIGKILL if the
            // process is still around after a short window (the Windows arm's
            // TerminateProcess is forceful, so match that outcome).
            let rc = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
            if rc != 0 {
                return Err(format!(
                    "could not signal process {pid}: {}",
                    std::io::Error::last_os_error()
                ));
            }
            for _ in 0..10 {
                if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            unsafe { libc::kill(pid as i32, libc::SIGKILL) };
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())?
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        Err("kill_process is not implemented on this platform".to_string())
    }
}
