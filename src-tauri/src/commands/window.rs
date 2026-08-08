const MIN_PALETTE_DIMENSION: f64 = 1.0;

#[cfg(target_os = "macos")]
const PALETTE_RESIZE_DURATION_SECONDS: f64 = 0.15;

fn palette_dimension(value: f64, name: &str) -> Result<f64, String> {
    if !value.is_finite() || value < MIN_PALETTE_DIMENSION {
        return Err(format!("palette {name} must be a finite positive number"));
    }
    Ok(value)
}

#[cfg(any(target_os = "macos", test))]
fn top_fixed_origin_y(current_y: f64, current_height: f64, target_height: f64) -> f64 {
    current_y + current_height - target_height
}

/// Resize the palette in logical points. macOS animates expansion from the
/// window's fixed top edge so Onix grows downward like a native search panel;
/// every other platform uses Tauri's ordinary logical-size path.
#[tauri::command]
pub fn resize_palette_window(
    width: f64,
    height: f64,
    animated: bool,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    let width = palette_dimension(width, "width")?;
    let height = palette_dimension(height, "height")?;

    #[cfg(target_os = "macos")]
    {
        let ns_window = window.ns_window().map_err(|e| e.to_string())? as usize;
        if ns_window == 0 {
            return Err("macOS palette NSWindow is null".to_string());
        }

        window
            .run_on_main_thread(move || {
                use objc2::runtime::{AnyClass, AnyObject};
                use objc2_foundation::{NSPoint, NSRect, NSSize};

                let ns_window = ns_window as *mut AnyObject;
                unsafe {
                    let current: NSRect = objc2::msg_send![ns_window, frame];
                    let target = NSRect::new(
                        NSPoint::new(
                            current.origin.x,
                            top_fixed_origin_y(current.origin.y, current.size.height, height),
                        ),
                        NSSize::new(width, height),
                    );

                    if animated {
                        if let Some(context_class) = AnyClass::get("NSAnimationContext") {
                            let _: () = objc2::msg_send![context_class, beginGrouping];
                            let context: *mut AnyObject =
                                objc2::msg_send![context_class, currentContext];
                            if !context.is_null() {
                                let _: () = objc2::msg_send![
                                    context,
                                    setDuration: PALETTE_RESIZE_DURATION_SECONDS
                                ];
                                let animator: *mut AnyObject =
                                    objc2::msg_send![ns_window, animator];
                                if !animator.is_null() {
                                    // AppKit emits ordinary windowDidResize callbacks for
                                    // every animation frame. Arm the surface interpolation
                                    // immediately before the first one can arrive.
                                    super::palette_surface::begin_palette_surface_resize(
                                        current.size.height,
                                        height,
                                    );
                                    let _: () =
                                        objc2::msg_send![animator, setFrame: target display: true];
                                    let _: () = objc2::msg_send![context_class, endGrouping];
                                    return;
                                }
                            }
                            let _: () = objc2::msg_send![context_class, endGrouping];
                        }
                    }

                    let _: () = objc2::msg_send![ns_window, setFrame: target display: true];
                }
            })
            .map_err(|e| e.to_string())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = animated;
        window
            .set_size(tauri::LogicalSize::new(width, height))
            .map_err(|e| e.to_string())
    }
}

/// Set window transparency (0.0 = fully opaque, 1.0 = fully transparent)
#[tauri::command]
pub async fn set_window_transparency(
    transparency: f64,
    window: tauri::Window,
) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_dimensions_must_be_finite_and_positive() {
        assert_eq!(palette_dimension(770.0, "width"), Ok(770.0));
        assert!(palette_dimension(0.0, "height").is_err());
        assert!(palette_dimension(-1.0, "height").is_err());
        assert!(palette_dimension(f64::NAN, "width").is_err());
        assert!(palette_dimension(f64::INFINITY, "width").is_err());
    }

    #[test]
    fn top_fixed_resize_moves_only_the_bottom_edge() {
        let current_y = 400.0;
        let current_height = 66.0;
        let target_height = 420.0;
        let target_y = top_fixed_origin_y(current_y, current_height, target_height);

        assert_eq!(target_y, 46.0);
        assert_eq!(target_y + target_height, current_y + current_height);
    }
}
