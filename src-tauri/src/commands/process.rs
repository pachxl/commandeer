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
        tokio::task::spawn_blocking(|| {
            use windows::Win32::Foundation::CloseHandle;
            use windows::Win32::System::ProcessStatus::{
                EnumProcesses, K32GetModuleBaseNameW, K32GetProcessMemoryInfo,
                PROCESS_MEMORY_COUNTERS,
            };
            use windows::core::PWSTR;
            use windows::Win32::System::Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
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
                    let Ok(handle) =
                        OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
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
                    .map(|_| {
                        String::from_utf16_lossy(&path_buf[..path_len as usize]).replace('\\', "/")
                    });
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
        })
        .await
        .map_err(|e| e.to_string())?
    }

    #[cfg(target_os = "macos")]
    {
        tokio::task::spawn_blocking(|| {
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
        })
        .await
        .map_err(|e| e.to_string())?
    }

    #[cfg(target_os = "linux")]
    {
        Ok(vec![])
    }
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
        let _ = pid;
        Err("kill_process is only implemented on Windows".to_string())
    }
}
