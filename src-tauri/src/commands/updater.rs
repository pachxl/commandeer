//! Background application updates from signed GitHub Release artifacts.

#[cfg(not(debug_assertions))]
use std::time::Duration;

use tauri::AppHandle;

#[cfg(any(not(debug_assertions), test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlatformKind {
    #[cfg(any(target_os = "windows", test))]
    Windows,
    #[cfg(any(target_os = "linux", test))]
    Linux,
    #[cfg(any(target_os = "macos", test))]
    Macos,
}

#[cfg(not(debug_assertions))]
const INITIAL_CHECK_DELAY: Duration = Duration::from_secs(30);
#[cfg(not(debug_assertions))]
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// Starts the release-only update loop. Development builds deliberately skip
/// it so a local binary never replaces itself with a published package.
pub fn start(app: AppHandle) {
    #[cfg(debug_assertions)]
    let _ = app;

    #[cfg(not(debug_assertions))]
    if !is_packaged_install() {
        eprintln!("automatic updates disabled outside an installed package");
        return;
    }

    #[cfg(not(debug_assertions))]
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(INITIAL_CHECK_DELAY).await;

        loop {
            // Re-read the setting each cycle so a Settings toggle applies
            // without restarting the app.
            if !crate::commands::config::load_config(&app)
                .auto_update
                .unwrap_or(true)
            {
                tokio::time::sleep(CHECK_INTERVAL).await;
                continue;
            }

            match check_and_install(&app).await {
                Ok(true) => {
                    // Windows' installer normally exits the process itself.
                    // Linux and macOS return after installing, so request a
                    // clean restart; the request is harmless if exit is
                    // already underway.
                    app.request_restart();
                    return;
                }
                Ok(false) => {}
                Err(error) => eprintln!("automatic update check failed: {error}"),
            }

            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

#[cfg(not(debug_assertions))]
fn is_packaged_install() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };

    #[cfg(target_os = "windows")]
    let platform = PlatformKind::Windows;
    #[cfg(target_os = "linux")]
    let platform = PlatformKind::Linux;
    #[cfg(target_os = "macos")]
    let platform = PlatformKind::Macos;

    packaged_path(
        platform,
        &exe.to_string_lossy(),
        std::env::var_os("APPIMAGE").is_some(),
    )
}

#[cfg(any(not(debug_assertions), test))]
fn packaged_path(platform: PlatformKind, executable: &str, appimage: bool) -> bool {
    let normalized = executable.replace('\\', "/").to_ascii_lowercase();
    if normalized.contains("/target/debug/") || normalized.contains("/target/release/") {
        return false;
    }

    match platform {
        #[cfg(any(target_os = "macos", test))]
        PlatformKind::Macos => normalized.contains(".app/contents/macos/"),
        #[cfg(any(target_os = "linux", test))]
        PlatformKind::Linux => {
            appimage
                || normalized.starts_with("/usr/bin/")
                || normalized.starts_with("/usr/local/bin/")
                || normalized.starts_with("/opt/")
        }
        #[cfg(any(target_os = "windows", test))]
        PlatformKind::Windows => {
            normalized.contains("/program files/")
                || normalized.contains("/appdata/local/commandeer/")
        }
    }
}

#[cfg(not(debug_assertions))]
async fn check_and_install(app: &AppHandle) -> Result<bool, String> {
    use tauri_plugin_updater::UpdaterExt;

    let updater = app.updater().map_err(|error| error.to_string())?;
    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        return Ok(false);
    };

    eprintln!(
        "installing Commandeer update {} (current {})",
        update.version, update.current_version
    );
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{packaged_path, PlatformKind};

    #[test]
    fn raw_build_outputs_never_update() {
        assert!(!packaged_path(
            PlatformKind::Macos,
            "/repo/src-tauri/target/release/commandeer",
            false
        ));
        assert!(!packaged_path(
            PlatformKind::Linux,
            "/repo/src-tauri/target/release/commandeer",
            false
        ));
        assert!(!packaged_path(
            PlatformKind::Windows,
            r"C:\repo\src-tauri\target\release\commandeer.exe",
            false
        ));
    }

    #[test]
    fn packaged_locations_can_update() {
        assert!(packaged_path(
            PlatformKind::Macos,
            "/Applications/commandeer.app/Contents/MacOS/commandeer",
            false
        ));
        assert!(packaged_path(
            PlatformKind::Linux,
            "/tmp/.mount_commandeer/commandeer",
            true
        ));
        assert!(packaged_path(
            PlatformKind::Windows,
            r"C:\Users\Alex\AppData\Local\commandeer\commandeer.exe",
            false
        ));
    }
}
