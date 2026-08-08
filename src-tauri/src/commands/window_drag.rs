//! Alt-drag window management — hold Alt and drag anywhere in a window to move
//! it, hold Alt and right-drag to resize it. This mirrors Hyprland's
//! `movewindow` / `resizewindow` mouse binds, applied to *any* window
//! system-wide.
//!
//! Per platform, because the capability lives in completely different places:
//!
//! - **Windows**: a low-level mouse hook (`WH_MOUSE_LL`) on a dedicated
//!   message-pump thread watches for Alt + mouse button and swallows only the
//!   button events; a separate mover thread polls the real cursor with
//!   `GetCursorPos` at ~120 Hz and repositions the window with `SetWindowPos`
//!   (same architecture as AltSnap/AltDrag). Mouse-move events are NEVER
//!   swallowed: an LL hook runs *before* the system applies the input, so
//!   returning nonzero for a WM_MOUSEMOVE discards it and the on-screen cursor
//!   does not move at all. With relative (real-mouse) input every hardware
//!   packet is a delta applied to the *current* cursor position — swallow the
//!   moves and that position never advances, so each reported `pt` is just
//!   grab-point + one packet's delta (±a few px) and snaps back to the grab
//!   point the instant the hand stops. That was the "window springs back to
//!   origin / have to fight the mouse" bug. (Injected MOUSEEVENTF_ABSOLUTE
//!   events masked it because their `pt` is absolute, not accumulated.) The
//!   modifier state is sampled with `GetAsyncKeyState`.
//! - **macOS**: a `CGEventTap` watches the same gesture and moves/resizes the
//!   window under the cursor via the Accessibility API (`AXUIElement`). Needs
//!   the **Accessibility** permission (the same one paste-to-previous uses).
//! - **Linux/Wayland**: NOT supported. Wayland forbids a client from touching
//!   other apps' windows — window management is the compositor's job (which is
//!   exactly why Hyprland, *being* the compositor, can do it). COSMIC already
//!   provides Super+drag move / Super+right-drag resize natively. See TODO.md.
//!
//! The gesture is edge-aware like Hyprland: for a resize, the window is divided
//! into a 3×3 grid and the cursor's starting cell picks which edge(s) move
//! (center falls back to the bottom-right corner).

/// Enable/disable Alt-drag window management. The frontend persists the choice
/// separately (`config.window_drag`); this call only starts/stops the OS hook.
#[tauri::command]
pub async fn set_window_drag(enabled: bool) -> Result<(), String> {
    if enabled {
        platform::enable()
    } else {
        platform::disable()
    }
}

/// Start the hook at launch if the user left the feature enabled. Best-effort:
/// a failure (e.g. missing macOS Accessibility permission) is logged, not fatal.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn apply_from_config(app: &tauri::AppHandle) {
    if crate::commands::config::load_config(app)
        .window_drag
        .unwrap_or(false)
    {
        if let Err(e) = platform::enable() {
            eprintln!("window_drag: enable at startup failed: {e}");
        }
    }
}

// Windows is split by concern: win.rs (hook + drag lifecycle), win/snap.rs
// (snap/tiling geometry), win/hover.rs (hover overlay), win/mover.rs (mover
// thread). macOS lives in mac.rs.
#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "windows")]
mod win;
#[cfg(target_os = "macos")]
use self::mac as platform;
#[cfg(target_os = "windows")]
use self::win as platform;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    pub fn enable() -> Result<(), String> {
        Err("Alt-drag window management isn't available on this platform".into())
    }
    pub fn disable() -> Result<(), String> {
        Ok(())
    }
}
