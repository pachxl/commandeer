mod crypto;
mod db;

pub use db::{ClipboardDb, ClipboardItem};

const MAX_HISTORY: usize = 100;

/// Record the current clipboard text into the encrypted SQLite history (dedup
/// at top, truncate to MAX_HISTORY). No-op when unchanged since the last call.
fn record_current(
    _app: &tauri::AppHandle,
    clipboard: &mut arboard::Clipboard,
    last_text: &mut String,
    db: &ClipboardDb,
) {
    let text = match clipboard.get_text() {
        Ok(t) => t,
        Err(_) => return,
    };
    if text.is_empty() || *last_text == text {
        return;
    }
    *last_text = text.clone();
    let _ = db.record(&text);
}

/// Poll-based monitor: only path on non-Windows, fallback on Windows when the
/// clipboard listener can't be installed. On macOS each tick first checks
/// NSPasteboard's changeCount — an integer AppKit bumps on every clipboard
/// write — so idle polls cost one Objective-C call instead of deserializing
/// the pasteboard contents through arboard every 500 ms.
fn run_poll_loop(app: tauri::AppHandle, db: ClipboardDb) {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut last_text = String::new();
    #[cfg(target_os = "macos")]
    let mut last_count: Option<isize> = None;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        #[cfg(target_os = "macos")]
        {
            let count = pasteboard_change_count();
            if count.is_some() && count == last_count {
                continue;
            }
            last_count = count;
        }
        record_current(&app, &mut clipboard, &mut last_text, &db);
    }
}

/// NSPasteboard.generalPasteboard.changeCount. None if the lookup fails, in
/// which case the caller just polls unconditionally like Linux.
#[cfg(target_os = "macos")]
fn pasteboard_change_count() -> Option<isize> {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    unsafe {
        let cls = AnyClass::get("NSPasteboard")?;
        let pb: *mut AnyObject = msg_send![cls, generalPasteboard];
        if pb.is_null() {
            return None;
        }
        Some(msg_send![pb, changeCount])
    }
}

#[cfg(target_os = "windows")]
mod win_monitor {
    use std::cell::RefCell;
    use super::ClipboardDb;

    pub struct MonitorState {
        pub app: tauri::AppHandle,
        pub clipboard: arboard::Clipboard,
        pub last_text: String,
        pub db: ClipboardDb,
    }

    // The wndproc runs on the thread that owns the window, which is the same
    // thread that pumps the message loop below — a thread-local is enough.
    thread_local! {
        pub static STATE: RefCell<Option<MonitorState>> = const { RefCell::new(None) };
    }

    pub unsafe extern "system" fn wndproc(
        hwnd: windows::Win32::Foundation::HWND,
        msg: u32,
        wparam: windows::Win32::Foundation::WPARAM,
        lparam: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        use windows::Win32::Foundation::LRESULT;
        use windows::Win32::UI::WindowsAndMessaging::{DefWindowProcW, WM_CLIPBOARDUPDATE};
        if msg == WM_CLIPBOARDUPDATE {
            STATE.with(|s| {
                if let Some(state) = s.borrow_mut().as_mut() {
                    super::record_current(&state.app, &mut state.clipboard, &mut state.last_text, &state.db);
                }
            });
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

pub fn start_monitor(app: tauri::AppHandle, db: ClipboardDb) {
    #[cfg(target_os = "windows")]
    std::thread::spawn(move || {
        use windows::core::{w, PCWSTR};
        use windows::Win32::System::DataExchange::AddClipboardFormatListener;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage,
            HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
        };

        let Ok(mut clipboard) = arboard::Clipboard::new() else {
            return;
        };
        let mut last_text = String::new();
        // Seed history with whatever is on the clipboard at startup (the old
        // poll loop had this behavior).
        record_current(&app, &mut clipboard, &mut last_text, &db);

        unsafe {
            let class = w!("commandeer_clipboard_monitor");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(win_monitor::wndproc),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = match CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                PCWSTR::null(),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                None,
                None,
                None,
            ) {
                Ok(h) => h,
                Err(_) => return run_poll_loop(app, db),
            };
            if AddClipboardFormatListener(hwnd).is_err() {
                return run_poll_loop(app, db);
            }

            win_monitor::STATE.with(|s| {
                *s.borrow_mut() = Some(win_monitor::MonitorState {
                    app,
                    clipboard,
                    last_text,
                    db,
                })
            });

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });

    #[cfg(not(target_os = "windows"))]
    std::thread::spawn(move || run_poll_loop(app, db));
}

#[tauri::command]
pub async fn read_clipboard_history(
    db: tauri::State<'_, ClipboardDb>,
) -> Result<Vec<ClipboardItem>, String> {
    db.read_all(MAX_HISTORY)
}

#[tauri::command]
pub async fn clear_clipboard_history(db: tauri::State<'_, ClipboardDb>) -> Result<(), String> {
    db.clear()
}

/// Put `text` on the clipboard and keep serving it after this call returns.
/// On X11/Wayland the selection is owned by the live connection, so dropping
/// the `Clipboard` right after `set_text` (the old behavior) made the content
/// vanish unless a clipboard manager happened to grab it first. `wait()`
/// blocks until another owner replaces the offer, so the thread outlives the
/// call; an early error (no display, protocol failure) arrives immediately,
/// while silence past the timeout means the offer is live.
#[cfg(target_os = "linux")]
pub(crate) fn set_clipboard_detached(text: String) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = arboard::Clipboard::new().and_then(|mut c| {
            use arboard::SetExtLinux;
            c.set().wait().text(text)
        });
        let _ = tx.send(result.map_err(|e| e.to_string()));
    });
    match rx.recv_timeout(std::time::Duration::from_millis(300)) {
        Ok(result) => result,
        // Timeout: the thread is still alive serving the selection = success.
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(()),
        // Disconnected: the arboard thread panicked without sending = failure.
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("clipboard thread died before setting the selection".to_string())
        }
    }
}

#[tauri::command]
pub async fn write_clipboard_text(text: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(move || set_clipboard_detached(text))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "linux"))]
    {
        std::thread::spawn(move || {
            let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
            clipboard.set_text(text).map_err(|e| e.to_string())
        })
        .join()
        .map_err(|e| format!("{:?}", e))?
    }
}

// paste_to_previous and the foreground-window capture live in paste.rs.
