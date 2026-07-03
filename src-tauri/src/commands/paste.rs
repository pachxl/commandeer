//! Paste snippet text into whichever window was focused before the palette
//! opened. The foreground window is stashed by `capture_foreground` right
//! before the palette is shown.

use tauri::Manager;

/// Foreground window at the moment the palette was shown, so snippet
/// selections can paste back into it. 0 = nothing captured.
#[cfg(target_os = "windows")]
static PREV_FOREGROUND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Called right before the palette is shown.
pub fn capture_foreground() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let hwnd = GetForegroundWindow();
        PREV_FOREGROUND.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(target_os = "windows")]
pub fn previous_foreground() -> isize {
    PREV_FOREGROUND.load(std::sync::atomic::Ordering::Relaxed)
}

/// Refocus `hwnd` even when Windows would reject a plain SetForegroundWindow
/// from an unfocused process (attach our input thread to the target's first).
#[cfg(target_os = "windows")]
pub unsafe fn force_foreground(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowThreadProcessId, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    if IsIconic(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }
    if SetForegroundWindow(hwnd).as_bool() {
        return;
    }
    let target_thread = GetWindowThreadProcessId(hwnd, None);
    let our_thread = GetCurrentThreadId();
    if target_thread != 0 && target_thread != our_thread {
        let _ = AttachThreadInput(our_thread, target_thread, true);
        let _ = SetForegroundWindow(hwnd);
        let _ = AttachThreadInput(our_thread, target_thread, false);
    }
}

/// Hide the palette, put `text` on the clipboard, refocus the window that was
/// active when the palette opened, and synthesize Ctrl+V. Returns whether the
/// paste keystroke was actually delivered: on Linux that needs an input
/// synthesizer (wtype / ydotool on Wayland, xdotool on X11) — without one this
/// degrades to copy-only and returns `false` so the frontend can tell the user
/// to press Ctrl+V themselves.
#[tauri::command]
pub async fn paste_to_previous(app: tauri::AppHandle, text: String) -> Result<bool, String> {
    if let Some(win) = app.get_webview_window("palette") {
        let _ = win.hide();
    }

    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(move || {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
                KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_V,
            };

            let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
            clipboard.set_text(text).map_err(|e| e.to_string())?;

            let prev = previous_foreground();
            if prev == 0 {
                return Err("no previous window to paste into".to_string());
            }

            fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: vk,
                            wScan: 0,
                            dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                }
            }

            unsafe {
                force_foreground(HWND(prev as *mut _));
                // Give the target window a beat to take focus before Ctrl+V.
                std::thread::sleep(std::time::Duration::from_millis(80));
                let inputs = [
                    key(VK_CONTROL, false),
                    key(VK_V, false),
                    key(VK_V, true),
                    key(VK_CONTROL, true),
                ];
                SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            }
            Ok(true)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(move || {
            super::clipboard::set_clipboard_detached(text)?;
            // The compositor refocuses the previous surface once the palette
            // unmaps; give it a beat before synthesizing the keystroke (there
            // is no Wayland equivalent of force_foreground).
            std::thread::sleep(std::time::Duration::from_millis(150));
            Ok(synthesize_ctrl_v())
        })
        .await
        .map_err(|e| e.to_string())?
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        std::thread::spawn(move || {
            let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
            clipboard.set_text(text).map_err(|e| e.to_string())
        })
        .join()
        .map_err(|e| format!("{:?}", e))?
        .map(|_| false)
    }
}

/// Best-effort Ctrl+V into the focused window via whichever input synthesizer
/// is installed. Returns false when none worked (copy-only).
#[cfg(target_os = "linux")]
fn synthesize_ctrl_v() -> bool {
    let try_tool = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    };

    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        // wtype needs the compositor's virtual-keyboard protocol (cosmic-comp
        // and wlroots have it); ydotool needs its daemon + uinput permissions.
        try_tool("wtype", &["-M", "ctrl", "-k", "v", "-m", "ctrl"])
            || try_tool("ydotool", &["key", "29:1", "47:1", "47:0", "29:0"])
    } else {
        try_tool("xdotool", &["key", "--clearmodifiers", "ctrl+v"])
    }
}
