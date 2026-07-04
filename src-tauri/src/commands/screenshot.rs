//! Lightshot-style region screenshot: freeze the screen into a fullscreen
//! overlay window, let the user drag a region, then copy it to the clipboard
//! and save it under Pictures/Screenshots.
//!
//! Capture backends: on Linux a four-tool fallback chain of external CLIs
//! (`cosmic-screenshot` → `gnome-screenshot` → `spectacle` → `grim`), the
//! first one present wins; on Windows a GDI BitBlt of the full virtual screen
//! (all monitors). The frozen frame is written to `<app-cache>/frame.png` and
//! served to the overlay webview via the asset protocol.

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// The pending capture, from trigger until finish/cancel.
pub struct Capture {
    pub frame_path: PathBuf,
    pub width: u32,
    pub height: u32,
    /// Physical origin of the captured virtual screen (for overlay positioning).
    #[cfg(target_os = "windows")]
    pub monitor_origin: (i32, i32),
}

#[derive(Default)]
pub struct ScreenshotState(pub Mutex<Option<Capture>>);

#[derive(serde::Serialize, Clone)]
struct FramePayload {
    path: String,
    width: u32,
    height: u32,
}

#[derive(serde::Deserialize)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

fn frame_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("frame.png"))
}

#[cfg(not(target_os = "windows"))]
fn capture_frame(app: &AppHandle) -> Result<Capture, String> {
    let dest = frame_path(app)?;
    let dir = dest.parent().unwrap().to_path_buf();

    capture_screen_to(&dest, &dir)?;

    let (width, height) = image::image_dimensions(&dest).map_err(|e| e.to_string())?;
    Ok(Capture {
        frame_path: dest,
        width,
        height,
    })
}

/// Grab the screen into `dest` with the first available backend:
/// cosmic-screenshot (COSMIC portal CLI), then the DE/compositor natives —
/// gnome-screenshot, spectacle (KDE), grim (wlroots). A tool that isn't
/// installed just advances the chain; a tool that runs and fails aborts with
/// its stderr. (The XDG Screenshot portal is deliberately not shelled to:
/// its reply arrives as a D-Bus signal that `gdbus call` can't wait for.)
#[cfg(not(target_os = "windows"))]
fn capture_screen_to(dest: &std::path::Path, dir: &std::path::Path) -> Result<(), String> {
    let mut missing: Vec<&str> = Vec::new();

    // cosmic-screenshot picks its own filename; rename to the stable name the
    // asset-protocol scope expects.
    match std::process::Command::new("cosmic-screenshot")
        .args(["--interactive=false", "--notify=false"])
        .arg(format!("--save-dir={}", dir.display()))
        .output()
    {
        Ok(out) if out.status.success() => {
            let saved = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if saved.is_empty() {
                return Err("cosmic-screenshot printed no path".into());
            }
            if PathBuf::from(&saved) != dest {
                std::fs::rename(&saved, dest).map_err(|e| e.to_string())?;
            }
            return Ok(());
        }
        Ok(out) => {
            return Err(format!(
                "cosmic-screenshot failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => missing.push("cosmic-screenshot"),
        Err(e) => return Err(format!("cosmic-screenshot failed to run: {e}")),
    }

    // Direct-to-file tools, in rough desktop-popularity order.
    let dest_str = dest.to_string_lossy().into_owned();
    let candidates: [(&str, Vec<&str>); 3] = [
        ("gnome-screenshot", vec!["-f", &dest_str]),
        ("spectacle", vec!["-b", "-n", "-o", &dest_str]),
        ("grim", vec![&dest_str]),
    ];
    for (program, args) in candidates {
        match std::process::Command::new(program).args(&args).output() {
            Ok(out) if out.status.success() && dest.is_file() => return Ok(()),
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                eprintln!("screenshot: {program} failed ({err}), trying next backend");
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => missing.push(program),
            Err(e) => eprintln!("screenshot: {program} failed to run ({e}), trying next backend"),
        }
    }

    Err(format!(
        "no screenshot tool worked — install one of: {}",
        missing.join(", ")
    ))
}

/// GDI capture of the entire virtual screen (all monitors; the overlay is
/// then positioned over the same bounds, so overlay pixels map 1:1 onto frame
/// pixels). Areas of the bounding box not covered by any monitor come out
/// black, matching other capture tools.
#[cfg(target_os = "windows")]
fn capture_frame(app: &AppHandle) -> Result<Capture, String> {
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
        DIB_RGB_COLORS, ROP_CODE, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    let dest = frame_path(app)?;

    unsafe {
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        if w <= 0 || h <= 0 {
            return Err("empty virtual screen rect".into());
        }

        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmp = CreateCompatibleBitmap(screen_dc, w, h);
        let old = SelectObject(mem_dc, bmp);

        let blit = BitBlt(
            mem_dc,
            0,
            0,
            w,
            h,
            screen_dc,
            vx,
            vy,
            ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
        );

        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        let mut ok = blit.is_ok();
        if ok {
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    // Negative height = top-down rows, matching image's layout.
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            ok = GetDIBits(
                mem_dc,
                bmp,
                0,
                h as u32,
                Some(buf.as_mut_ptr() as *mut _),
                &mut bmi,
                DIB_RGB_COLORS,
            ) != 0;
        }

        SelectObject(mem_dc, old);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);

        if !ok {
            return Err("GDI capture failed".into());
        }

        // BGRA → RGBA in place.
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }

        // Fast PNG: this frame is transient (reloaded once, then deleted), so
        // trade file size for speed — Fast compression + no row filtering
        // encodes several times faster than image::save's defaults. Capture
        // dropped from ~770ms to ~50ms on a 2560x1440 release build.
        {
            use image::codecs::png::{CompressionType, FilterType, PngEncoder};
            use image::{ExtendedColorType, ImageEncoder};
            let file = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
            let writer = std::io::BufWriter::new(file);
            PngEncoder::new_with_quality(writer, CompressionType::Fast, FilterType::NoFilter)
                .write_image(&buf, w as u32, h as u32, ExtendedColorType::Rgba8)
                .map_err(|e| e.to_string())?;
        }

        Ok(Capture {
            frame_path: dest,
            width: w as u32,
            height: h as u32,
            monitor_origin: (vx, vy),
        })
    }
}

/// Trigger the capture flow: (optionally wait for the palette to unmap,)
/// freeze the screen, stash the capture, and hand the frame to the overlay
/// webview. The overlay calls `show_screenshot_overlay` once the frame image
/// has actually loaded, so the user never sees a stale or blank overlay.
#[tauri::command]
pub async fn start_screenshot(app: AppHandle, delay_ms: Option<u64>) -> Result<(), String> {
    start_inner(&app, delay_ms.unwrap_or(0)).await
}

/// Fire-and-forget entry point for non-command triggers (deep link, hotkey).
pub fn start_screenshot_bg(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = start_inner(&app, 0).await {
            eprintln!("screenshot: {e}");
        }
    });
}

/// True while a capture is mid-flight (trigger → frame handed to the overlay).
/// A second trigger in that window must not start a parallel flow; a
/// re-trigger while the overlay is already open (capture done) restarts the
/// flow with a fresh frame instead.
static CAPTURING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

async fn start_inner(app: &AppHandle, delay_ms: u64) -> Result<(), String> {
    use std::sync::atomic::Ordering;

    if CAPTURING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = start_capture(app, delay_ms).await;
    CAPTURING.store(false, Ordering::SeqCst);
    return result;

    async fn start_capture(app: &AppHandle, mut delay_ms: u64) -> Result<(), String> {
        // Never freeze our own windows into the frame: hide them first and give
        // the compositor a beat to actually unmap them. (A re-trigger while the
        // overlay is open restarts the flow with a fresh frame.)
        for label in ["palette", "screenshot"] {
            if let Some(win) = app.get_webview_window(label) {
                if win.is_visible().unwrap_or(false) {
                    // Linux: the overlay clears itself to a fully transparent frame
                    // before hiding (see the screenshot-clear listener) so the
                    // webview's last composite — which WebKitGTK replays as the
                    // first frame at the next map — is invisible instead of the
                    // old capture. The hide after the sleep below is the fallback
                    // if the webview never answers.
                    #[cfg(not(target_os = "windows"))]
                    let deferred =
                        label == "screenshot" && app.emit_to(label, "screenshot-clear", ()).is_ok();
                    #[cfg(target_os = "windows")]
                    let deferred = false;
                    if !deferred {
                        let _ = win.hide();
                    }
                    delay_ms = delay_ms.max(220);
                }
            }
        }
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
        // Ensure the overlay really is unmapped before we capture (idempotent; the
        // preferred path is the webview hiding itself after its clear paint).
        #[cfg(not(target_os = "windows"))]
        if let Some(win) = app.get_webview_window("screenshot") {
            let _ = win.hide();
        }

        let capture = capture_frame(app)?;
        let payload = FramePayload {
            path: capture.frame_path.to_string_lossy().into_owned(),
            width: capture.width,
            height: capture.height,
        };

        // Position/size the overlay and show it CLOAKED before handing the frame
        // to the webview. WebView2 only renders while its window is visible, and
        // a cloaked window is composited without being displayed — so the frame
        // image loads, rasterizes and is presented to the DWM surface entirely
        // off-screen. The webview then reports the actual on-screen paint of the
        // <img> (element timing) and reveal_screenshot_overlay drops the cloak,
        // which is atomic: the overlay pops in fully formed, no black or stale
        // frame. (Resizing at show time, or showing before the paint, both flash.)
        #[cfg(target_os = "windows")]
        if let Some(win) = app.get_webview_window("screenshot") {
            let (x, y) = capture.monitor_origin;
            let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
            let _ = win.set_size(tauri::PhysicalSize::new(capture.width, capture.height));
            set_cloak(&win, true);
            let _ = win.show();
            let _ = win.set_focus();
        }

        {
            let state = app.state::<ScreenshotState>();
            *state.0.lock().unwrap() = Some(capture);
        }
        app.emit_to("screenshot", "screenshot-frame", payload)
            .map_err(|e| e.to_string())?;

        // Safety net: if the overlay hasn't shown itself shortly (frame <img>
        // onload → show_screenshot_overlay), show it anyway so a hiccup in event
        // delivery or image decode can't leave the user with a silent no-op. Kept
        // comfortably longer than a normal capture+decode so it never races the
        // preferred onload path (which shows only once the frame is actually
        // painted) — racing it caused a dim→undim→dim flicker.
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let pending = app
                .state::<ScreenshotState>()
                .0
                .lock()
                .unwrap()
                .is_some();
            let visible = app
                .get_webview_window("screenshot")
                .and_then(|w| w.is_visible().ok())
                .unwrap_or(false);
            if pending {
                if !visible {
                    eprintln!("screenshot: overlay did not self-show, forcing show");
                    if let Err(e) = show_screenshot_overlay(app.clone()) {
                        eprintln!("screenshot: fallback show failed: {e}");
                    }
                }
                // Always uncloak: if the frontend's paint handshake never arrived,
                // the window could otherwise sit shown-but-cloaked (invisible yet
                // eating input) indefinitely.
                let _ = reveal_screenshot_overlay(app.clone());
            }
        });
        Ok(())
    }
}

/// DWM-cloak (or uncloak) the overlay: a cloaked window is fully composited
/// but not displayed, so the webview can lay out and present the frame while
/// nothing reaches the screen. Flipping the cloak off is atomic in DWM — the
/// finished pixels appear with no intermediate black/stale frame.
#[cfg(target_os = "windows")]
fn set_cloak(win: &tauri::WebviewWindow, cloak: bool) {
    use windows::Win32::Foundation::{BOOL, HWND};
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK};
    if let Ok(hwnd) = win.hwnd() {
        let value = BOOL::from(cloak);
        unsafe {
            let _ = DwmSetWindowAttribute(
                HWND(hwnd.0 as *mut _),
                DWMWA_CLOAK,
                &value as *const _ as *const _,
                std::mem::size_of::<BOOL>() as u32,
            );
        }
    }
}

/// Called by the overlay webview once the frame <img> has decoded — showing
/// only now avoids a flash of the previous frame or an empty window. The
/// window is shown *cloaked*; the webview then confirms a real paint and
/// calls `reveal_screenshot_overlay`, which uncloaks.
#[tauri::command]
pub fn show_screenshot_overlay(app: AppHandle) -> Result<(), String> {
    let state = app.state::<ScreenshotState>();
    // Ignore stale onload events (e.g. finish/cancel already ran).
    #[allow(unused_variables)]
    let Some(capture) = &*state.0.lock().unwrap() else {
        return Ok(());
    };
    let win = app
        .get_webview_window("screenshot")
        .ok_or("no screenshot window")?;

    // Idempotent: the frame <img> onload and the Rust-side fallback both call
    // this, and a re-show/re-focus of an already-visible overlay causes a
    // visible flicker (and a focus flap that can trip the click-away cancel).
    if win.is_visible().unwrap_or(false) {
        return Ok(());
    }

    // Windows: positioning/sizing already happened at capture time (see
    // start_inner) — doing it here, right before show, caused a black flash
    // while WebView2 repainted at the new size. On Linux/Wayland the
    // layer-shell surface is anchored to all four edges, so the compositor
    // sizes it to the output — nothing to position. On X11 there is no layer
    // shell: cover the captured area from the origin (the fallback capture
    // tools grab the whole screen).
    #[cfg(not(target_os = "windows"))]
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        let _ = win.set_position(tauri::PhysicalPosition::new(0, 0));
        let _ = win.set_size(tauri::PhysicalSize::new(capture.width, capture.height));
    }

    #[cfg(target_os = "windows")]
    set_cloak(&win, true);

    win.show().map_err(|e| e.to_string())?;
    win.set_focus().map_err(|e| e.to_string())
}

/// Uncloak the (already shown) overlay once the webview has actually painted
/// the frame. Idempotent; on Linux the show path never cloaks so this is a
/// no-op.
#[tauri::command]
pub fn reveal_screenshot_overlay(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    if let Some(win) = app.get_webview_window("screenshot") {
        set_cloak(&win, false);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = app;
    Ok(())
}

/// Hide the overlay window without touching the pending capture. Called by the
/// webview after it has painted a cleared (fully transparent) frame, so the
/// composite WebKitGTK replays at the next map shows nothing instead of the
/// previous capture.
#[tauri::command]
pub fn hide_screenshot_overlay(app: AppHandle) {
    if let Some(win) = app.get_webview_window("screenshot") {
        let _ = win.hide();
    }
}

/// Crop the frozen frame to `region` (image pixels), save a timestamped PNG
/// under Pictures/Screenshots, and put the PNG on the clipboard.
#[tauri::command]
pub async fn finish_screenshot(app: AppHandle, region: Region) -> Result<String, String> {
    // Hide first: the snip should feel instant even while we encode/copy.
    if let Some(win) = app.get_webview_window("screenshot") {
        let _ = win.hide();
    }

    let state = app.state::<ScreenshotState>();
    let capture = state
        .0
        .lock()
        .unwrap()
        .take()
        .ok_or("no pending capture")?;

    let frame = image::open(&capture.frame_path)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let x = region.x.min(capture.width.saturating_sub(1));
    let y = region.y.min(capture.height.saturating_sub(1));
    let w = region.w.clamp(1, capture.width - x);
    let h = region.h.clamp(1, capture.height - y);
    let cropped = image::imageops::crop_imm(&frame, x, y, w, h).to_image();

    let dir = app
        .path()
        .picture_dir()
        .map_err(|e| e.to_string())?
        .join("Screenshots");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = chrono::Local::now()
        .format("Screenshot-%Y-%m-%d-%H%M%S.png")
        .to_string();
    let path = dir.join(name);
    cropped.save(&path).map_err(|e| e.to_string())?;

    copy_image_to_clipboard(&path, &cropped)?;

    let _ = std::fs::remove_file(&capture.frame_path);
    Ok(path.to_string_lossy().into_owned())
}

/// Linux: hand the PNG to `wl-copy` (Wayland) or `xclip` (X11), which fork
/// and keep serving the clipboard after we return — arboard's image offer
/// would die with its Clipboard object here. When neither tool is installed,
/// fall back to arboard on a detached thread held open by `wait()` (same
/// pattern as clipboard.rs::set_clipboard_detached).
#[cfg(not(target_os = "windows"))]
fn copy_image_to_clipboard(path: &std::path::Path, img: &image::RgbaImage) -> Result<(), String> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();

    let tool_result = if wayland {
        std::fs::File::open(path).map_err(|e| e.to_string()).and_then(|file| {
            match std::process::Command::new("wl-copy")
                .args(["--type", "image/png"])
                .stdin(file)
                .status()
            {
                Ok(status) if status.success() => Ok(()),
                Ok(_) => Err("wl-copy failed".to_string()),
                Err(e) => Err(format!("wl-copy failed to run: {e}")),
            }
        })
    } else {
        match std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-t", "image/png", "-i"])
            .arg(path)
            .status()
        {
            Ok(status) if status.success() => Ok(()),
            Ok(_) => Err("xclip failed".to_string()),
            Err(e) => Err(format!("xclip failed to run: {e}")),
        }
    };
    if tool_result.is_ok() {
        return Ok(());
    }

    let (width, height) = (img.width() as usize, img.height() as usize);
    let bytes = img.as_raw().clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = arboard::Clipboard::new().and_then(|mut c| {
            let set = c.set();
            #[cfg(target_os = "linux")]
            let set = {
                use arboard::SetExtLinux;
                set.wait()
            };
            set.image(arboard::ImageData {
                width,
                height,
                bytes: std::borrow::Cow::Owned(bytes),
            })
        });
        let _ = tx.send(result.map_err(|e| e.to_string()));
    });
    match rx.recv_timeout(std::time::Duration::from_millis(500)) {
        Ok(result) => result,
        // Timeout: the thread is still alive and serving the selection = success.
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(()),
        // Disconnected: the arboard thread panicked without sending = failure.
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("clipboard thread died before setting the image".to_string())
        }
    }
}

#[cfg(target_os = "windows")]
fn copy_image_to_clipboard(_path: &std::path::Path, img: &image::RgbaImage) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_image(arboard::ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: std::borrow::Cow::Borrowed(img.as_raw()),
    })
    .map_err(|e| e.to_string())
}

/// Esc / focus-loss: hide the overlay and drop the pending capture.
#[tauri::command]
pub fn cancel_screenshot(app: AppHandle) {
    if let Some(win) = app.get_webview_window("screenshot") {
        let _ = win.hide();
    }
    let state = app.state::<ScreenshotState>();
    let taken = state.0.lock().unwrap().take();
    if let Some(capture) = taken {
        let _ = std::fs::remove_file(capture.frame_path);
    }
}
