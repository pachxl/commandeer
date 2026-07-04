//! Lightshot-style region screenshot: freeze the screen into a fullscreen
//! overlay window, let the user drag a region, then copy it to the clipboard
//! and save it under Pictures/Screenshots.
//!
//! Capture backends: `cosmic-screenshot` (portal CLI) on Linux, GDI BitBlt of
//! the cursor monitor on Windows. The frozen frame is written to
//! `<app-cache>/frame.png` and served to the overlay webview via the asset
//! protocol.

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

    let out = std::process::Command::new("cosmic-screenshot")
        .args(["--interactive=false", "--notify=false"])
        .arg(format!("--save-dir={}", dir.display()))
        .output()
        .map_err(|e| format!("cosmic-screenshot failed to run: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cosmic-screenshot failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let saved = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if saved.is_empty() {
        return Err("cosmic-screenshot printed no path".into());
    }
    // Move to the stable name the asset-protocol scope expects.
    if PathBuf::from(&saved) != dest {
        std::fs::rename(&saved, &dest).map_err(|e| e.to_string())?;
    }

    let (width, height) = image::image_dimensions(&dest).map_err(|e| e.to_string())?;
    Ok(Capture {
        frame_path: dest,
        width,
        height,
    })
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

async fn start_inner(app: &AppHandle, mut delay_ms: u64) -> Result<(), String> {
    // Never freeze our own windows into the frame: hide them first and give
    // the compositor a beat to actually unmap them. (A re-trigger while the
    // overlay is open restarts the flow with a fresh frame.)
    for label in ["palette", "screenshot"] {
        if let Some(win) = app.get_webview_window(label) {
            if win.is_visible().unwrap_or(false) {
                let _ = win.hide();
                delay_ms = delay_ms.max(220);
            }
        }
    }
    if delay_ms > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
    }

    let capture = capture_frame(app)?;
    let payload = FramePayload {
        path: capture.frame_path.to_string_lossy().into_owned(),
        width: capture.width,
        height: capture.height,
    };

    // Position/size the (still hidden) overlay BEFORE handing the frame to the
    // webview: resizing at show time makes WebView2 clear to its background
    // color until the renderer catches up — a fullscreen black flash on every
    // monitor. Sized now, the frame is laid out and painted at the final size
    // by the time show_screenshot_overlay runs, so showing presents finished
    // pixels.
    #[cfg(target_os = "windows")]
    if let Some(win) = app.get_webview_window("screenshot") {
        let (x, y) = capture.monitor_origin;
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
        let _ = win.set_size(tauri::PhysicalSize::new(capture.width, capture.height));
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
        if pending && !visible {
            eprintln!("screenshot: overlay did not self-show, forcing show");
            if let Err(e) = show_screenshot_overlay(app.clone()) {
                eprintln!("screenshot: fallback show failed: {e}");
            }
        }
    });
    Ok(())
}

/// Called by the overlay webview once the frame <img> has decoded — showing
/// only now avoids a flash of the previous frame or an empty window.
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

    // Positioning/sizing already happened at capture time (see start_inner) —
    // doing it here, right before show, caused a black flash while WebView2
    // repainted at the new size. On Linux the layer-shell surface is anchored
    // to all four edges, so the compositor sizes it to the output.

    win.show().map_err(|e| e.to_string())?;
    win.set_focus().map_err(|e| e.to_string())
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

/// Linux/Wayland: hand the PNG to `wl-copy`, which forks and keeps serving the
/// clipboard after we return — arboard's image offer would die with its
/// Clipboard object here.
#[cfg(not(target_os = "windows"))]
fn copy_image_to_clipboard(path: &std::path::Path, _img: &image::RgbaImage) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("wl-copy")
        .args(["--type", "image/png"])
        .stdin(file)
        .status()
        .map_err(|e| format!("wl-copy failed to run: {e}"))?;
    if !status.success() {
        return Err("wl-copy failed".into());
    }
    Ok(())
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
