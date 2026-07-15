//! Background application updates from signed GitHub Release artifacts.

#[cfg(not(debug_assertions))]
use std::time::Duration;

use tauri::AppHandle;

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
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(INITIAL_CHECK_DELAY).await;

        loop {
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
