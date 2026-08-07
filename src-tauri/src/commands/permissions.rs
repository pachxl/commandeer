//! Platform permission diagnostics and links to the owning system settings.

#[derive(serde::Serialize)]
pub struct PermissionStatus {
    pub supported: bool,
    pub screen_recording: Option<bool>,
    pub accessibility: Option<bool>,
}

#[tauri::command]
pub fn permission_status() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn AXIsProcessTrusted() -> bool;
        }

        PermissionStatus {
            supported: true,
            screen_recording: Some(core_graphics::access::ScreenCaptureAccess.preflight()),
            accessibility: Some(unsafe { AXIsProcessTrusted() }),
        }
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    PermissionStatus {
        supported: false,
        screen_recording: None,
        accessibility: None,
    }
}

fn permission_settings_url(permission: &str) -> Option<&'static str> {
    match permission {
        "screen-recording" => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        }
        "accessibility" => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        }
        "automation" => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation")
        }
        _ => None,
    }
}

#[tauri::command]
pub fn open_permission_settings(permission: String) -> Result<(), String> {
    let Some(url) = permission_settings_url(&permission) else {
        return Err(format!("unknown permission: {permission}"));
    };

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        let _ = url;
        Err("permission settings are only available on macOS".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{permission_settings_url, permission_status};

    #[test]
    fn maps_only_known_permission_panes() {
        assert!(permission_settings_url("screen-recording")
            .expect("screen recording URL")
            .contains("Privacy_ScreenCapture"));
        assert!(permission_settings_url("accessibility")
            .expect("accessibility URL")
            .contains("Privacy_Accessibility"));
        assert!(permission_settings_url("automation")
            .expect("automation URL")
            .contains("Privacy_Automation"));
        assert_eq!(permission_settings_url("camera"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reads_macos_grants_without_prompting() {
        let status = permission_status();
        assert!(status.supported);
        assert!(status.screen_recording.is_some());
        assert!(status.accessibility.is_some());
    }
}
