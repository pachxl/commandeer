/// Set window transparency (0.0 = fully opaque, 1.0 = fully transparent)
#[tauri::command]
pub async fn set_window_transparency(transparency: f64, window: tauri::Window) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongW, SetLayeredWindowAttributes, SetWindowLongW, GWL_EXSTYLE, LWA_ALPHA,
            WS_EX_LAYERED,
        };

        // Note: no spawn_blocking here because HWND can't be sent across threads.
        let hwnd = match window.hwnd() {
            Ok(h) => HWND(h.0 as *mut _),
            Err(_) => return Err("Failed to get window handle".to_string()),
        };

        unsafe {
            // Ensure the window has the layered style
            let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            if (style & WS_EX_LAYERED.0 as i32) == 0 {
                let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED.0 as i32);
            }

            // Alpha 0-255, where 0 is transparent and 255 is opaque
            let alpha = ((1.0 - transparency.clamp(0.0, 1.0)) * 255.0) as u8;

            SetLayeredWindowAttributes(
                hwnd,
                windows::Win32::Foundation::COLORREF(0),
                alpha,
                LWA_ALPHA,
            )
            .map_err(|_| "Failed to set window transparency".to_string())
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Native NSWindow alpha — the macOS analogue of the Windows LWA_ALPHA
        // above. Setting alphaValue makes the whole window (vibrancy + content)
        // genuinely translucent, revealing the desktop behind, instead of just
        // fading the webview onto the opaque vibrancy layer (which reads as a
        // blurred wallpaper patch, not transparency). AppKit must be touched on
        // the main thread; the raw NSWindow pointer isn't Send, so it crosses
        // the closure boundary as a usize.
        let alpha = 1.0 - transparency.clamp(0.0, 1.0);
        let ns_window = window
            .ns_window()
            .map_err(|_| "Failed to get NSWindow".to_string())? as usize;
        window
            .run_on_main_thread(move || {
                use objc2::runtime::AnyObject;
                let ns_window = ns_window as *mut AnyObject;
                unsafe {
                    let _: () = objc2::msg_send![ns_window, setAlphaValue: alpha];
                }
            })
            .map_err(|e| e.to_string())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // Linux never reaches here: Wayland has no whole-window alpha, so the
        // frontend applies CSS opacity to the webview root instead (the window
        // background is fully transparent, making that visually equivalent).
        // See setWindowTransparency in src/lib/tauri.ts.
        let _ = (transparency, window);
        Err("native window transparency is only implemented on Windows".to_string())
    }
}
