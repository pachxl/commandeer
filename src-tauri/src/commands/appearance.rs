//! Cross-platform dark/light mode toggle.
//!
//! Windows sets the AppsUseLightTheme / SystemUsesLightTheme registry values
//! and broadcasts WM_SETTINGCHANGE so listening apps refresh. macOS uses the
//! public AppleScript appearance-preferences API. Linux targets GTK desktops
//! via gsettings; on non-GTK desktops the command fails with a clear message.

#[tauri::command]
pub async fn set_dark_mode(enabled: bool) -> Result<(), String> {
    tokio::task::spawn_blocking(move || set_dark_mode_sync(enabled))
        .await
        .map_err(|e| e.to_string())?
}

fn set_dark_mode_sync(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows_set_dark_mode(enabled)
    }

    #[cfg(target_os = "macos")]
    {
        macos_set_dark_mode(enabled)
    }

    #[cfg(target_os = "linux")]
    {
        linux_set_dark_mode(enabled)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = enabled;
        Err("appearance toggle is not implemented on this platform".to_string())
    }
}

#[cfg(target_os = "windows")]
fn windows_set_dark_mode(enabled: bool) -> Result<(), String> {
    use windows::core::w;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    let key = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";
    let value = if enabled { "0" } else { "1" };

    for name in ["AppsUseLightTheme", "SystemUsesLightTheme"] {
        let out = std::process::Command::new("reg")
            .args([
                "add",
                &format!("HKCU\\{key}"),
                "/v",
                name,
                "/t",
                "REG_DWORD",
                "/d",
                value,
                "/f",
            ])
            .output()
            .map_err(|e| format!("reg failed to run: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "reg add {name} failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }

    // Notify running apps that the immersive color set changed.
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(w!("ImmersiveColorSet").0 as isize),
            SMTO_ABORTIFHUNG,
            100,
            None,
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_set_dark_mode(enabled: bool) -> Result<(), String> {
    let mode = if enabled { "true" } else { "false" };
    let script = format!(
        "tell application \"System Events\" to tell appearance preferences to set dark mode to {mode}"
    );
    let out = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("osascript failed to run: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "appearance toggle failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(target_os = "linux")]
fn linux_set_dark_mode(enabled: bool) -> Result<(), String> {
    let scheme = if enabled {
        "prefer-dark"
    } else {
        "prefer-light"
    };
    let out = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.desktop.interface", "color-scheme", scheme])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "gsettings not found — appearance toggle requires a GTK/GNOME desktop".to_string()
            } else {
                format!("gsettings failed to run: {e}")
            }
        })?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "gsettings failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}
