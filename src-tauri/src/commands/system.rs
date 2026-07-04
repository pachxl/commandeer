//! System power/session actions via direct Win32 calls. The legacy branch
//! spawned `powershell.exe` per action (slow, and its `powercfg -hibernate`
//! toggle needed admin rights, so it silently failed); everything here is a
//! single API call in-process.
//!
//! These APIs expect to run on the thread that owns the GUI message queue,
//! so we dispatch them to the main thread rather than a Tokio blocking pool.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SystemAction {
    Lock,
    Sleep,
    Hibernate,
    Shutdown,
    Restart,
    Logout,
    EmptyTrash,
}

#[tauri::command]
pub async fn system_action(action: SystemAction, app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.run_on_main_thread(move || {
            let result = win::run(action);
            let _ = tx.send(result);
        })
        .map_err(|e| format!("main thread dispatch failed: {e}"))?;
        rx.await
            .map_err(|e| format!("system action channel closed: {e}"))?
    }

    #[cfg(target_os = "macos")]
    {
        let _ = app;
        tokio::task::spawn_blocking(move || mac::run(action))
            .await
            .map_err(|e| e.to_string())?
    }

    #[cfg(target_os = "linux")]
    {
        let _ = (action, app);
        Err("system actions are only implemented on Windows".to_string())
    }
}

/// macOS actions shell out: `pmset` for power (no privileges needed) and
/// AppleScript for session actions — System Events shuts down/restarts/logs
/// out *gracefully* (apps get to save), unlike `shutdown(8)`, which also needs
/// root. The System Events and Finder scripts each trigger a one-time
/// Automation permission prompt on first use.
#[cfg(target_os = "macos")]
mod mac {
    use super::SystemAction;

    fn run_cmd(program: &str, args: &[&str]) -> Result<(), String> {
        let out = std::process::Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("{program} failed to run: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!(
                "{program} failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }

    fn osascript(script: &str) -> Result<(), String> {
        run_cmd("osascript", &["-e", script])
    }

    pub fn run(action: SystemAction) -> Result<(), String> {
        match action {
            // Sleeps the display; with the default "require password after
            // sleep" setting that is the lock screen.
            SystemAction::Lock => run_cmd("pmset", &["displaysleepnow"]),
            SystemAction::Sleep => run_cmd("pmset", &["sleepnow"]),
            SystemAction::Hibernate => {
                Err("Hibernate is not a separate action on macOS (sleep already writes a safe-sleep image)".to_string())
            }
            SystemAction::Shutdown => osascript("tell application \"System Events\" to shut down"),
            SystemAction::Restart => osascript("tell application \"System Events\" to restart"),
            SystemAction::Logout => osascript("tell application \"System Events\" to log out"),
            SystemAction::EmptyTrash => osascript("tell application \"Finder\" to empty trash"),
        }
    }
}

#[cfg(target_os = "windows")]
mod win {
    use super::SystemAction;
    use windows::core::{HRESULT, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, HWND, LUID, WIN32_ERROR};
    use windows::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
        SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows::Win32::System::Power::SetSuspendState;
    use windows::Win32::System::Shutdown::{
        ExitWindowsEx, LockWorkStation, EWX_FORCEIFHUNG, EWX_LOGOFF, EWX_REBOOT, EWX_SHUTDOWN,
        SHUTDOWN_REASON,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::Win32::UI::Shell::SHEmptyRecycleBinW;

    // SHEmptyRecycleBinW flags (shellapi.h; not exported by the windows crate)
    const SHERB_NOCONFIRMATION: u32 = 0x1;
    const SHERB_NOPROGRESSUI: u32 = 0x2;
    const SHERB_NOSOUND: u32 = 0x4;

    const ERROR_NOT_SUPPORTED: u32 = 50;
    const ERROR_NOT_ALL_ASSIGNED: u32 = 1300;

    fn last_error_string(context: &str) -> String {
        let code = unsafe { GetLastError().0 };
        format!("{} failed (error {})", context, code)
    }

    /// Enable SeShutdownPrivilege on our process token. ExitWindowsEx
    /// (shutdown/restart) and SetSuspendState require it; a normal user
    /// *holds* the privilege but it starts disabled.
    fn enable_shutdown_privilege() -> Result<(), String> {
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                &mut token,
            )
            .map_err(|_| last_error_string("OpenProcessToken"))?;

            let result = (|| {
                let mut luid = LUID::default();
                LookupPrivilegeValueW(PCWSTR::null(), SE_SHUTDOWN_NAME, &mut luid)
                    .map_err(|_| last_error_string("LookupPrivilegeValueW(SeShutdownPrivilege)"))?;
                let tp = TOKEN_PRIVILEGES {
                    PrivilegeCount: 1,
                    Privileges: [LUID_AND_ATTRIBUTES {
                        Luid: luid,
                        Attributes: SE_PRIVILEGE_ENABLED,
                    }],
                };
                AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None)
                    .map_err(|_| last_error_string("AdjustTokenPrivileges"))?;

                // AdjustTokenPrivileges can return OK without enabling the
                // privilege if the token doesn't hold it.
                let err = GetLastError();
                if err == WIN32_ERROR(ERROR_NOT_ALL_ASSIGNED) {
                    return Err(
                        "SeShutdownPrivilege is not held by this process".to_string(),
                    );
                }
                Ok(())
            })();
            let _ = CloseHandle(token);
            result
        }
    }

    pub fn run(action: SystemAction) -> Result<(), String> {
        unsafe {
            match action {
                SystemAction::Lock => LockWorkStation()
                    .map_err(|_| last_error_string("LockWorkStation")),

                SystemAction::Sleep | SystemAction::Hibernate => {
                    enable_shutdown_privilege()?;
                    let hibernate = matches!(action, SystemAction::Hibernate);
                    if SetSuspendState(hibernate, false, false).as_bool() {
                        Ok(())
                    } else {
                        let code = GetLastError().0;
                        if code == ERROR_NOT_SUPPORTED {
                            Err(format!(
                                "{} is not supported on this system (error {})",
                                if hibernate { "Hibernate" } else { "Sleep" },
                                code
                            ))
                        } else {
                            Err(last_error_string("SetSuspendState"))
                        }
                    }
                }

                SystemAction::Restart | SystemAction::Shutdown => {
                    enable_shutdown_privilege()?;
                    let what = if matches!(action, SystemAction::Restart) {
                        EWX_REBOOT
                    } else {
                        EWX_SHUTDOWN
                    };
                    ExitWindowsEx(what | EWX_FORCEIFHUNG, SHUTDOWN_REASON(0))
                        .map_err(|_| last_error_string("ExitWindowsEx"))
                }

                SystemAction::Logout => {
                    ExitWindowsEx(EWX_LOGOFF, SHUTDOWN_REASON(0))
                        .map_err(|_| last_error_string("ExitWindowsEx"))
                }

                SystemAction::EmptyTrash => {
                    match SHEmptyRecycleBinW(
                        HWND::default(),
                        PCWSTR::null(),
                        SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
                    ) {
                        Ok(()) => Ok(()),
                        // An already-empty bin reports E_UNEXPECTED on some
                        // Windows versions — not a failure for our purposes.
                        Err(e) if e.code() == HRESULT(0x8000FFFFu32 as i32) => Ok(()),
                        Err(_) => Err(last_error_string("SHEmptyRecycleBinW")),
                    }
                }
            }
        }
    }
}
