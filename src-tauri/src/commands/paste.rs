//! Paste snippet text into whichever window was focused before the palette
//! opened. The foreground window is stashed by `capture_foreground` right
//! before the palette is shown.

use tauri::Manager;

/// Foreground window at the moment the palette was shown, so snippet
/// selections can paste back into it. 0 = nothing captured.
#[cfg(target_os = "windows")]
static PREV_FOREGROUND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// macOS analogue of `PREV_FOREGROUND`: pid of the app that was frontmost when
/// the palette was shown. 0 = nothing captured.
#[cfg(target_os = "macos")]
static PREV_APP_PID: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Called right before the palette is shown.
pub fn capture_foreground() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let hwnd = GetForegroundWindow();
        PREV_FOREGROUND.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(target_os = "macos")]
    unsafe {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        let Some(cls) = AnyClass::get("NSWorkspace") else {
            return;
        };
        let ws: *mut AnyObject = msg_send![cls, sharedWorkspace];
        let pid: i32 = if ws.is_null() {
            0
        } else {
            let front: *mut AnyObject = msg_send![ws, frontmostApplication];
            if front.is_null() {
                0
            } else {
                msg_send![front, processIdentifier]
            }
        };
        PREV_APP_PID.store(pid, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Bring the previously-frontmost app back to the front. AppKit call, so this
/// must run on the main thread.
#[cfg(target_os = "macos")]
fn activate_app(pid: i32) -> Result<(), String> {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    unsafe {
        let cls = AnyClass::get("NSRunningApplication")
            .ok_or("NSRunningApplication class not found")?;
        let target: *mut AnyObject = msg_send![cls, runningApplicationWithProcessIdentifier: pid];
        if target.is_null() {
            return Err(format!("previous app (pid {pid}) is no longer running"));
        }
        // NSApplicationActivateIgnoringOtherApps (1 << 1)
        let _: bool = msg_send![target, activateWithOptions: 2usize];
        Ok(())
    }
}

/// Post ⌘V as HID-level keyboard events. Requires the Accessibility permission
/// (System Settings → Privacy & Security → Accessibility); without it the
/// events are silently dropped, so fail loudly instead.
#[cfg(target_os = "macos")]
fn synthesize_cmd_v() -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    if !unsafe { AXIsProcessTrusted() } {
        return Err(
            "pasting needs the Accessibility permission: System Settings → Privacy & Security → Accessibility"
                .to_string(),
        );
    }

    let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "CGEventSource creation failed".to_string())?;
    const KEY_V: u16 = 9; // kVK_ANSI_V
    for down in [true, false] {
        let ev = CGEvent::new_keyboard_event(src.clone(), KEY_V, down)
            .map_err(|_| "CGEvent creation failed".to_string())?;
        ev.set_flags(CGEventFlags::CGEventFlagCommand);
        ev.post(CGEventTapLocation::HID);
    }
    Ok(())
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

/// Hide the palette, put `text` on the clipboard, refocus the window/app that
/// was active when the palette opened, and synthesize Ctrl+V (⌘V on macOS).
/// Returns whether the paste keystroke was actually delivered: on Linux that
/// needs an input synthesizer (wtype / ydotool on Wayland, xdotool on X11) —
/// without one this degrades to copy-only and returns `false` so the frontend
/// can tell the user to press Ctrl+V themselves. On macOS delivery requires
/// the Accessibility permission and fails loudly without it.
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

    #[cfg(target_os = "macos")]
    {
        // Clipboard first, so the target app reads the new text.
        let clip_text = text;
        tokio::task::spawn_blocking(move || {
            let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
            clipboard.set_text(clip_text).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;

        let pid = PREV_APP_PID.load(std::sync::atomic::Ordering::Relaxed);
        if pid == 0 {
            return Err("no previous app to paste into".to_string());
        }

        // Explicitly reactivate the previous app rather than relying on the
        // focus falling back to it when the palette hides — deterministic, and
        // it also works when something else grabbed focus in between.
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.run_on_main_thread(move || {
            let _ = tx.send(activate_app(pid));
        })
        .map_err(|e| format!("main thread dispatch failed: {e}"))?;
        rx.await
            .map_err(|e| format!("activation channel closed: {e}"))??;

        tokio::task::spawn_blocking(|| {
            // Give the target app a beat to become key before ⌘V.
            std::thread::sleep(std::time::Duration::from_millis(150));
            synthesize_cmd_v().map(|()| true)
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

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
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
