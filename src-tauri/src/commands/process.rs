use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub memory_bytes: u64,
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
            use windows::Win32::System::Threading::{
                OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
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
                    let _ = CloseHandle(handle);
                    if len == 0 {
                        continue;
                    }
                    out.push(ProcessInfo {
                        pid,
                        name: String::from_utf16_lossy(&name_buf[..len as usize]),
                        memory_bytes: mem.WorkingSetSize as u64,
                    });
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(vec![])
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

    #[cfg(not(target_os = "windows"))]
    {
        let _ = pid;
        Err("kill_process is only implemented on Windows".to_string())
    }
}
