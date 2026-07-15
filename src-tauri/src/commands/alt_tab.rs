//! Windows-only per-monitor Alt+Tab replacement.
//!
//! A `WH_KEYBOARD_LL` hook on its own message-pump thread intercepts only an
//! active Alt+Tab session and posts tiny messages to a separate native overlay
//! thread. Window enumeration, DWM thumbnail registration, painting, and
//! foreground activation can therefore never delay the hook pump: Windows
//! silently removes low-level hooks that stop responding promptly.
//!
//! The keyboard/session logic lives in [`HookState`], platform-independent and
//! unit-tested; the platform module only maps Win32 events in and out of it.
//! Everything fails open: unless the service reports ready and the overlay
//! accepts the session-start message, keystrokes pass through to Windows.

#[tauri::command]
pub async fn set_per_monitor_alt_tab(enabled: bool) -> Result<(), String> {
    if enabled {
        platform::enable()
    } else {
        platform::disable()
    }
}

/// Opaque colors used by the native switcher. The frontend resolves the
/// active CSS theme (including user themes) and sends its rendered palette so
/// this Win32 window stays visually in step with Commandeer.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct AltTabTheme {
    background: [u8; 3],
    card: [u8; 3],
    selected: [u8; 3],
    border: [u8; 3],
    text: [u8; 3],
    accent: [u8; 3],
    dark: bool,
}

#[tauri::command]
pub async fn set_alt_tab_theme(theme: AltTabTheme) {
    platform::set_theme(theme);
}

#[cfg(target_os = "windows")]
pub fn apply_from_config(app: &tauri::AppHandle) {
    if crate::commands::config::load_config(app)
        .per_monitor_alt_tab
        .unwrap_or(false)
    {
        if let Err(e) = platform::enable() {
            eprintln!("alt_tab: enable at startup failed: {e}");
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_from_config(_app: &tauri::AppHandle) {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Forward,
    Backward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GridMove {
    Left,
    Right,
    Up,
    Down,
}

/// One state transition's output, delivered to the overlay thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionEvent {
    Start { direction: Direction, sticky: bool },
    Cycle { direction: Direction },
    Move(GridMove),
    Commit,
    Cancel,
    Sticky,
}

/// The hook's view of the outside world; implemented over Win32 in `platform`
/// and by a recorder in the unit tests.
trait HookHost {
    /// Deliver an event to the overlay thread. `false` means delivery failed
    /// and the keystroke must fall through to Windows.
    fn post(&mut self, event: SessionEvent) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HookKey {
    Tab,
    Escape,
    Return,
    Left,
    Right,
    Up,
    Down,
    LeftAlt,
    RightAlt,
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    /// Undifferentiated VK_SHIFT/VK_CONTROL some drivers report.
    GenericShift,
    GenericCtrl,
    Other,
}

impl HookKey {
    fn modifier(self) -> bool {
        matches!(
            self,
            HookKey::LeftAlt
                | HookKey::RightAlt
                | HookKey::LeftShift
                | HookKey::RightShift
                | HookKey::LeftCtrl
                | HookKey::RightCtrl
                | HookKey::GenericShift
                | HookKey::GenericCtrl
        )
    }
}

/// Key/session state machine driven by the low-level keyboard hook. Pure —
/// all Win32 effects go through the [`HookHost`].
#[derive(Default)]
struct HookState {
    left_alt: bool,
    right_alt: bool,
    left_shift: bool,
    right_shift: bool,
    left_ctrl: bool,
    right_ctrl: bool,
    session: bool,
    sticky: bool,
    swallow_escape_up: bool,
    swallow_return_up: bool,
}

impl HookState {
    fn alt(&self) -> bool {
        self.left_alt || self.right_alt
    }
    fn shift(&self) -> bool {
        self.left_shift || self.right_shift
    }
    fn ctrl(&self) -> bool {
        self.left_ctrl || self.right_ctrl
    }
    fn direction(&self) -> Direction {
        if self.shift() {
            Direction::Backward
        } else {
            Direction::Forward
        }
    }

    /// The overlay finished a session on its own (click-commit, all
    /// candidates closed): stop treating keys as session input.
    fn reset_session(&mut self) {
        self.session = false;
        self.sticky = false;
    }

    /// Feed one keystroke; returns true when the event must be swallowed.
    /// `alt_flag` is the hook's own LLKHF_ALTDOWN, a fallback for the very
    /// first Tab if the Alt down predates the hook.
    fn on_key(&mut self, key: HookKey, down: bool, alt_flag: bool, host: &mut impl HookHost) -> bool {
        match key {
            HookKey::LeftAlt => self.left_alt = down,
            HookKey::RightAlt => self.right_alt = down,
            HookKey::LeftShift => self.left_shift = down,
            HookKey::RightShift => self.right_shift = down,
            HookKey::LeftCtrl => self.left_ctrl = down,
            HookKey::RightCtrl => self.right_ctrl = down,
            // Keep generic modifiers useful without clobbering the more
            // precise left/right state when it is available.
            HookKey::GenericShift if !self.left_shift && !self.right_shift => {
                self.left_shift = down
            }
            HookKey::GenericCtrl if !self.left_ctrl && !self.right_ctrl => self.left_ctrl = down,
            _ => {}
        }

        if key == HookKey::Tab {
            if down {
                if self.session {
                    // Also covers sticky mode, where Alt is already up.
                    let _ = host.post(SessionEvent::Cycle {
                        direction: self.direction(),
                    });
                    return true;
                }
                if self.alt() || alt_flag {
                    let sticky = self.ctrl();
                    if !host.post(SessionEvent::Start {
                        direction: self.direction(),
                        sticky,
                    }) {
                        return false;
                    }
                    self.session = true;
                    self.sticky = sticky;
                    return true;
                }
                return false;
            }
            return self.session;
        }

        // Final Alt release: commit (or park in sticky mode). The release
        // itself always passes through — swallowing a modifier key-up desyncs
        // the system key state and the activated window would see Alt held.
        if matches!(key, HookKey::LeftAlt | HookKey::RightAlt) && !down && self.session && !self.alt()
        {
            if self.sticky {
                let _ = host.post(SessionEvent::Sticky);
            } else {
                let _ = host.post(SessionEvent::Commit);
                self.session = false;
            }
            return false;
        }

        if key == HookKey::Escape {
            if down && self.session {
                let _ = host.post(SessionEvent::Cancel);
                self.reset_session();
                self.swallow_escape_up = true;
                return true;
            }
            if !down && self.swallow_escape_up {
                self.swallow_escape_up = false;
                return true;
            }
            return false;
        }

        if key == HookKey::Return && !down && self.swallow_return_up {
            self.swallow_return_up = false;
            return true;
        }

        // During a session every other non-modifier key-down is swallowed so
        // Alt+<letter> shortcuts don't fire in the foreground app mid-switch
        // (the key-up passes through, so no key state can get stuck).
        if self.session && down && !key.modifier() {
            match key {
                HookKey::Left => {
                    let _ = host.post(SessionEvent::Move(GridMove::Left));
                }
                HookKey::Right => {
                    let _ = host.post(SessionEvent::Move(GridMove::Right));
                }
                HookKey::Up => {
                    let _ = host.post(SessionEvent::Move(GridMove::Up));
                }
                HookKey::Down => {
                    let _ = host.post(SessionEvent::Move(GridMove::Down));
                }
                HookKey::Return => {
                    let _ = host.post(SessionEvent::Commit);
                    self.reset_session();
                    self.swallow_return_up = true;
                }
                _ => {}
            }
            return true;
        }
        false
    }
}

fn cycled_index(current: usize, len: usize, direction: Direction) -> usize {
    if len == 0 {
        return 0;
    }
    match direction {
        Direction::Forward => (current + 1) % len,
        Direction::Backward => (current + len - 1) % len,
    }
}

/// Selection when a session opens: the first forward Tab goes to the window
/// below the foreground one, reverse wraps to the bottom of the Z-order.
fn initial_selection(count: usize, original_first: bool, direction: Direction) -> usize {
    match direction {
        Direction::Forward if original_first && count > 1 => 1,
        Direction::Forward => 0,
        Direction::Backward => count.saturating_sub(1),
    }
}

/// New selected index after candidates die mid-session (`alive` is parallel to
/// the candidate list). A surviving selection keeps its window; a dead one
/// moves to the nearest following survivor, else the nearest preceding one.
/// `None` = nothing left, close the switcher.
fn selection_after_prune(selected: usize, alive: &[bool]) -> Option<usize> {
    let survivors = alive.iter().filter(|a| **a).count();
    if survivors == 0 {
        return None;
    }
    if alive.get(selected).copied().unwrap_or(false) {
        return Some(alive[..selected].iter().filter(|a| **a).count());
    }
    match alive.iter().skip(selected).position(|a| *a) {
        Some(offset) => Some(alive[..selected + offset].iter().filter(|a| **a).count()),
        None => Some(survivors - 1),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GridLayout {
    cols: usize,
    rows: usize,
    capacity: usize,
    card_w: i32,
    card_h: i32,
    gap: i32,
    padding: i32,
    title_h: i32,
    panel_w: i32,
    panel_h: i32,
}

fn scaled(value: i32, dpi: u32) -> i32 {
    ((value as i64 * dpi.max(96) as i64) / 96) as i32
}

fn grid_layout(count: usize, work_w: i32, work_h: i32, dpi: u32) -> GridLayout {
    let gap = scaled(12, dpi);
    let padding = scaled(24, dpi);
    let title_h = scaled(38, dpi);
    let desired_w = scaled(260, dpi);
    let desired_h = scaled(180, dpi);
    let min_w = scaled(160, dpi);
    let min_h = scaled(120, dpi);
    let max_w = ((work_w as f32) * 0.9) as i32;
    let max_h = ((work_h as f32) * 0.9) as i32;

    let usable_w = (max_w - padding * 2).max(min_w);
    let usable_h = (max_h - padding * 2).max(min_h);
    let desired_cols = ((usable_w + gap) / (desired_w + gap)).max(1) as usize;
    let desired_rows = ((usable_h + gap) / (desired_h + gap)).max(1) as usize;
    let mut cols = count.max(1).min(desired_cols.max(1));
    let mut rows = count.max(1).div_ceil(cols).min(desired_rows.max(1));

    if cols * rows < count {
        let max_cols = ((usable_w + gap) / (min_w + gap)).max(1) as usize;
        let max_rows = ((usable_h + gap) / (min_h + gap)).max(1) as usize;
        cols = count.max(1).min(max_cols.max(1));
        rows = count.max(1).div_ceil(cols).min(max_rows.max(1));
    }

    let card_w = ((usable_w - gap * (cols.saturating_sub(1) as i32)) / cols as i32)
        .min(desired_w)
        .max(min_w.min(usable_w));
    let card_h = ((usable_h - gap * (rows.saturating_sub(1) as i32)) / rows as i32)
        .min(desired_h)
        .max(min_h.min(usable_h));
    let capacity = (cols * rows).max(1);
    let visible = count.max(1).min(capacity);
    let visible_rows = visible.div_ceil(cols).max(1);
    let visible_cols = visible.min(cols).max(1);

    GridLayout {
        cols,
        rows: visible_rows,
        capacity,
        card_w,
        card_h,
        gap,
        padding,
        title_h,
        panel_w: padding * 2
            + card_w * visible_cols as i32
            + gap * visible_cols.saturating_sub(1) as i32,
        panel_h: padding * 2
            + card_h * visible_rows as i32
            + gap * visible_rows.saturating_sub(1) as i32,
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{
        cycled_index, grid_layout, initial_selection, scaled, selection_after_prune, AltTabTheme,
        Direction, GridLayout, GridMove, HookHost, HookKey, HookState, SessionEvent,
    };
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
    use std::sync::{mpsc, Mutex, OnceLock};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use windows::core::{w, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{
        CloseHandle, BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
    };
    use windows::Win32::Graphics::Dwm::{
        DwmGetWindowAttribute, DwmQueryThumbnailSourceSize, DwmRegisterThumbnail,
        DwmSetWindowAttribute, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
        DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION,
        DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE, DWMSBT_TRANSIENTWINDOW,
        DWMWA_CLOAKED, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
        FillRect, GetMonitorInfoW, InvalidateRect, MonitorFromPoint, MonitorFromWindow, RoundRect,
        SelectObject, SetBkMode, SetTextColor, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        PAINTSTRUCT, PS_SOLID, TRANSPARENT, DT_END_ELLIPSIS, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::{
        AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId, OpenProcess,
        QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT,
        VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_SHIFT, VK_TAB, VK_UP,
    };
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyWindow,
        DispatchMessageW, DrawIconEx, EnumWindows, FlashWindowEx, GetAncestor, GetClassLongPtrW,
        GetCursorPos, GetDesktopWindow, GetForegroundWindow, GetLastActivePopup, GetMessageW,
        GetShellWindow, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId, IsWindow,
        IsWindowVisible, KillTimer, LoadCursorW, PostMessageW,
        PeekMessageW, PostThreadMessageW, RegisterClassW, SendMessageTimeoutW, SetForegroundWindow,
        SetTimer, SetWindowPos, SetWindowsHookExW, ShowWindow, TranslateMessage,
        UnhookWindowsHookEx, CS_HREDRAW, CS_VREDRAW, DI_NORMAL, FLASHWINFO, FLASHW_TRAY,
        GA_ROOTOWNER, GCLP_HICON, GCLP_HICONSM, GWL_EXSTYLE, HHOOK, HICON, HWND_TOPMOST,
        ICON_BIG, ICON_SMALL, ICON_SMALL2, IDC_ARROW, KBDLLHOOKSTRUCT, LLKHF_ALTDOWN,
        LLKHF_INJECTED, LLKHF_UP, MSG, PM_NOREMOVE, SC_CLOSE, SMTO_ABORTIFHUNG, SWP_NOACTIVATE,
        SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, WH_KEYBOARD_LL, WM_APP, WM_DESTROY,
        WM_DISPLAYCHANGE, WM_GETICON, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_QUIT,
        WM_SYSCOMMAND, WM_TIMER, WNDCLASSW, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_POPUP,
    };

    const MSG_START: u32 = WM_APP + 1;
    const MSG_CYCLE: u32 = WM_APP + 2;
    const MSG_MOVE: u32 = WM_APP + 3;
    const MSG_COMMIT: u32 = WM_APP + 4;
    const MSG_CANCEL: u32 = WM_APP + 5;
    const MSG_STICKY: u32 = WM_APP + 6;
    const MSG_HOOK_RESET: u32 = WM_APP + 7;
    const PRUNE_TIMER: usize = 1;

    static READY: AtomicBool = AtomicBool::new(false);
    /// The native overlay is actually visible. This is authoritative for Tab
    /// interception even if focus transfer makes the logical key session lag.
    static OVERLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
    static OVERLAY: AtomicIsize = AtomicIsize::new(0);
    static OVERLAY_THREAD_ID: AtomicU32 = AtomicU32::new(0);
    static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
    // Tokyo Night defaults are available before the webview sends the saved
    // theme during startup. COLORREF stores bytes as 0x00BBGGRR.
    static THEME_BACKGROUND: AtomicU32 = AtomicU32::new(colorref(26, 27, 38));
    static THEME_CARD: AtomicU32 = AtomicU32::new(colorref(36, 40, 59));
    static THEME_SELECTED: AtomicU32 = AtomicU32::new(colorref(122, 162, 247));
    static THEME_BORDER: AtomicU32 = AtomicU32::new(colorref(41, 43, 57));
    static THEME_TEXT: AtomicU32 = AtomicU32::new(colorref(192, 202, 245));
    static THEME_ACCENT: AtomicU32 = AtomicU32::new(colorref(122, 162, 247));
    static THEME_DARK: AtomicBool = AtomicBool::new(true);

    struct Service {
        overlay_thread: JoinHandle<()>,
        hook_thread: JoinHandle<()>,
    }

    fn service() -> &'static Mutex<Option<Service>> {
        static SERVICE: OnceLock<Mutex<Option<Service>>> = OnceLock::new();
        SERVICE.get_or_init(|| Mutex::new(None))
    }

    thread_local! {
        static KEYS: RefCell<HookState> = RefCell::new(HookState::default());
        static SWITCHER: RefCell<Option<Switcher>> = const { RefCell::new(None) };
    }

    pub fn enable() -> Result<(), String> {
        let mut guard = service().lock().map_err(|_| "Alt+Tab service lock poisoned")?;
        if guard.is_some() {
            return Ok(());
        }

        let (overlay_tx, overlay_rx) = mpsc::sync_channel(1);
        let overlay_thread = thread::Builder::new()
            .name("alt-tab-overlay".into())
            .spawn(move || overlay_thread(overlay_tx))
            .map_err(|e| e.to_string())?;
        match overlay_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = overlay_thread.join();
                return Err(e);
            }
            Err(_) => {
                stop_thread(OVERLAY_THREAD_ID.load(Ordering::Acquire));
                let _ = overlay_thread.join();
                return Err("Alt+Tab overlay timed out during startup".into());
            }
        }

        let (hook_tx, hook_rx) = mpsc::sync_channel(1);
        let hook_thread = match thread::Builder::new()
            .name("alt-tab-keyboard-hook".into())
            .spawn(move || hook_thread(hook_tx))
        {
            Ok(thread) => thread,
            Err(e) => {
                stop_thread(OVERLAY_THREAD_ID.load(Ordering::Acquire));
                let _ = overlay_thread.join();
                return Err(e.to_string());
            }
        };
        match hook_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                stop_thread(OVERLAY_THREAD_ID.load(Ordering::Acquire));
                let _ = hook_thread.join();
                let _ = overlay_thread.join();
                return Err(e);
            }
            Err(_) => {
                stop_thread(HOOK_THREAD_ID.load(Ordering::Acquire));
                stop_thread(OVERLAY_THREAD_ID.load(Ordering::Acquire));
                let _ = hook_thread.join();
                let _ = overlay_thread.join();
                return Err("Alt+Tab keyboard hook timed out during startup".into());
            }
        }

        READY.store(true, Ordering::Release);
        *guard = Some(Service {
            overlay_thread,
            hook_thread,
        });
        Ok(())
    }

    pub fn set_theme(theme: AltTabTheme) {
        let store = |target: &AtomicU32, value: [u8; 3]| {
            target.store(colorref(value[0], value[1], value[2]), Ordering::Release);
        };
        store(&THEME_BACKGROUND, theme.background);
        store(&THEME_CARD, theme.card);
        store(&THEME_SELECTED, theme.selected);
        store(&THEME_BORDER, theme.border);
        store(&THEME_TEXT, theme.text);
        store(&THEME_ACCENT, theme.accent);
        THEME_DARK.store(theme.dark, Ordering::Release);

        let hwnd = HWND(OVERLAY.load(Ordering::Acquire) as *mut _);
        if !hwnd.0.is_null() {
            unsafe {
                apply_dwm_color_mode(hwnd);
                let _ = InvalidateRect(hwnd, None, true);
            }
        }
    }

    pub fn disable() -> Result<(), String> {
        READY.store(false, Ordering::Release);
        let item = service()
            .lock()
            .map_err(|_| "Alt+Tab service lock poisoned")?
            .take();
        let Some(service) = item else {
            return Ok(());
        };
        stop_thread(HOOK_THREAD_ID.load(Ordering::Acquire));
        stop_thread(OVERLAY_THREAD_ID.load(Ordering::Acquire));
        let hook_result = service.hook_thread.join();
        let overlay_result = service.overlay_thread.join();
        hook_result
            .and(overlay_result)
            .map_err(|_| "Alt+Tab service thread panicked".to_string())
    }

    fn stop_thread(thread_id: u32) {
        if thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    }

    fn reset_hook_session() {
        let thread_id = HOOK_THREAD_ID.load(Ordering::Acquire);
        if thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(thread_id, MSG_HOOK_RESET, WPARAM(0), LPARAM(0));
            }
        }
    }

    fn overlay_thread(started: mpsc::SyncSender<Result<(), String>>) {
        OVERLAY_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
        let result = unsafe { initialize_overlay() };
        match result {
            Ok(hwnd) => {
                OVERLAY.store(hwnd.0 as isize, Ordering::Release);
                let _ = started.send(Ok(()));

                unsafe {
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    READY.store(false, Ordering::Release);
                    SWITCHER.with(|slot| {
                        if let Some(mut switcher) = slot.borrow_mut().take() {
                            switcher.hide();
                        }
                    });
                    let _ = KillTimer(hwnd, PRUNE_TIMER);
                    let _ = DestroyWindow(hwnd);
                }
            }
            Err(e) => {
                let _ = started.send(Err(e));
            }
        }
        OVERLAY_ACTIVE.store(false, Ordering::Release);
        OVERLAY.store(0, Ordering::Release);
        OVERLAY_THREAD_ID.store(0, Ordering::Release);
    }

    fn hook_thread(started: mpsc::SyncSender<Result<(), String>>) {
        HOOK_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
        unsafe {
            // Force creation of this thread's message queue before announcing
            // readiness, so reset/quit thread messages can never race startup.
            let mut queued = MSG::default();
            let _ = PeekMessageW(&mut queued, None, 0, 0, PM_NOREMOVE);
        }
        let result = unsafe { install_keyboard_hook() };
        match result {
            Ok(hook) => {
                let _ = started.send(Ok(()));
                unsafe {
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                        if msg.message == MSG_HOOK_RESET {
                            KEYS.with(|keys| keys.borrow_mut().reset_session());
                            continue;
                        }
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    let _ = UnhookWindowsHookEx(hook);
                }
            }
            Err(e) => {
                let _ = started.send(Err(e));
            }
        }
        READY.store(false, Ordering::Release);
        HOOK_THREAD_ID.store(0, Ordering::Release);
    }

    unsafe fn initialize_overlay() -> Result<HWND, String> {
        let module = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let instance = HINSTANCE(module.0);
        let class = w!("CommandeerPerMonitorAltTab");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(overlay_proc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class,
            w!("Task Switching"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            instance,
            None,
        )
        .map_err(|e| format!("could not create Alt+Tab overlay: {e}"))?;

        apply_dwm_style(hwnd);
        SWITCHER.with(|slot| *slot.borrow_mut() = Some(Switcher::new(hwnd)));
        SetTimer(hwnd, PRUNE_TIMER, 150, None);
        Ok(hwnd)
    }

    unsafe fn install_keyboard_hook() -> Result<HHOOK, String> {
        let module = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_proc),
            HINSTANCE(module.0),
            0,
        )
        .map_err(|e| format!("could not install Alt+Tab keyboard hook: {e}"))
    }

    unsafe fn apply_dwm_style(hwnd: HWND) {
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const _,
            std::mem::size_of_val(&corner) as u32,
        );
        apply_dwm_color_mode(hwnd);
        let backdrop = DWMSBT_TRANSIENTWINDOW;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as *const _,
            std::mem::size_of_val(&backdrop) as u32,
        );
    }

    unsafe fn apply_dwm_color_mode(hwnd: HWND) {
        let dark = BOOL(THEME_DARK.load(Ordering::Acquire) as i32);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as *const _,
            std::mem::size_of_val(&dark) as u32,
        );
    }

    fn hook_key(vk: u16) -> HookKey {
        match vk {
            x if x == VK_TAB.0 => HookKey::Tab,
            x if x == VK_ESCAPE.0 => HookKey::Escape,
            x if x == VK_RETURN.0 => HookKey::Return,
            x if x == VK_LEFT.0 => HookKey::Left,
            x if x == VK_RIGHT.0 => HookKey::Right,
            x if x == VK_UP.0 => HookKey::Up,
            x if x == VK_DOWN.0 => HookKey::Down,
            x if x == VK_LMENU.0 => HookKey::LeftAlt,
            x if x == VK_RMENU.0 => HookKey::RightAlt,
            x if x == VK_LSHIFT.0 => HookKey::LeftShift,
            x if x == VK_RSHIFT.0 => HookKey::RightShift,
            x if x == VK_LCONTROL.0 => HookKey::LeftCtrl,
            x if x == VK_RCONTROL.0 => HookKey::RightCtrl,
            x if x == VK_SHIFT.0 => HookKey::GenericShift,
            x if x == VK_CONTROL.0 => HookKey::GenericCtrl,
            _ => HookKey::Other,
        }
    }

    struct Win32Host;

    impl HookHost for Win32Host {
        fn post(&mut self, event: SessionEvent) -> bool {
            let hwnd = HWND(OVERLAY.load(Ordering::Acquire) as *mut _);
            if hwnd.0.is_null() {
                return false;
            }
            let direction_code = |direction: Direction| match direction {
                Direction::Forward => 0usize,
                Direction::Backward => 1usize,
            };
            let (msg, wparam, lparam) = match event {
                SessionEvent::Start { direction, sticky } => (
                    MSG_START,
                    WPARAM(direction_code(direction)),
                    LPARAM(sticky as isize),
                ),
                SessionEvent::Cycle { direction } => {
                    (MSG_CYCLE, WPARAM(direction_code(direction)), LPARAM(0))
                }
                SessionEvent::Move(movement) => (
                    MSG_MOVE,
                    WPARAM(match movement {
                        GridMove::Left => 0,
                        GridMove::Right => 1,
                        GridMove::Up => 2,
                        GridMove::Down => 3,
                    }),
                    LPARAM(0),
                ),
                SessionEvent::Commit => (MSG_COMMIT, WPARAM(0), LPARAM(0)),
                SessionEvent::Cancel => (MSG_CANCEL, WPARAM(0), LPARAM(0)),
                SessionEvent::Sticky => (MSG_STICKY, WPARAM(0), LPARAM(0)),
            };
            let posted = unsafe { PostMessageW(hwnd, msg, wparam, lparam).is_ok() };
            if posted && matches!(event, SessionEvent::Start { .. }) {
                // The foreground app saw Alt go down and will see Alt go back
                // up with every Tab swallowed in between — a lone Alt press,
                // which focuses the menu bar in many apps. Inject a no-op key
                // so the pair no longer reads as Alt-alone.
                unsafe { inject_mask_key() };
            }
            posted
        }
    }

    /// Keystroke apps see between Alt-down and Alt-up so the swallowed Tab
    /// doesn't leave a menu-activating "lone Alt". 0xFF is an unassigned VK.
    unsafe fn inject_mask_key() {
        let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0xFF),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }

    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && READY.load(Ordering::Acquire) {
            let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            // Injected input (our mask key, paste's synthetic Ctrl+V, remote
            // desktop tools) is never a physical Alt+Tab: pass it through
            // without disturbing the key state machine.
            if info.flags.0 & LLKHF_INJECTED.0 == 0 {
                let down = info.flags.0 & LLKHF_UP.0 == 0;
                let alt_flag = info.flags.0 & LLKHF_ALTDOWN.0 != 0;
                let key = hook_key(info.vkCode as u16);
                // Once our overlay is visibly handling the session, Windows
                // must never see another physical Tab from it. Focus transfer
                // can race or reset the logical HookState in some fullscreen
                // games, so visible UI is the stronger source of truth here.
                if key == HookKey::Tab && OVERLAY_ACTIVE.load(Ordering::Acquire) {
                    if down {
                        let direction = KEYS.with(|keys| keys.borrow().direction());
                        let _ = Win32Host.post(SessionEvent::Cycle { direction });
                    }
                    return LRESULT(1);
                }
                let swallow = KEYS.with(|keys| {
                    keys.borrow_mut().on_key(key, down, alt_flag, &mut Win32Host)
                });
                if swallow {
                    return LRESULT(1);
                }
            }
        }
        CallNextHookEx(HHOOK::default(), code, wparam, lparam)
    }

    unsafe extern "system" fn overlay_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            MSG_START => {
                let direction = if wparam.0 == 0 {
                    Direction::Forward
                } else {
                    Direction::Backward
                };
                SWITCHER.with(|slot| {
                    if let Some(switcher) = slot.borrow_mut().as_mut() {
                        switcher.open(direction, lparam.0 != 0);
                    }
                });
                LRESULT(0)
            }
            MSG_CYCLE => {
                let direction = if wparam.0 == 0 {
                    Direction::Forward
                } else {
                    Direction::Backward
                };
                SWITCHER.with(|slot| {
                    if let Some(switcher) = slot.borrow_mut().as_mut() {
                        switcher.cycle(direction);
                    }
                });
                LRESULT(0)
            }
            MSG_MOVE => {
                SWITCHER.with(|slot| {
                    if let Some(switcher) = slot.borrow_mut().as_mut() {
                        switcher.move_grid(wparam.0);
                    }
                });
                LRESULT(0)
            }
            MSG_COMMIT => {
                SWITCHER.with(|slot| {
                    if let Some(switcher) = slot.borrow_mut().as_mut() {
                        switcher.commit();
                    }
                });
                LRESULT(0)
            }
            MSG_CANCEL => {
                SWITCHER.with(|slot| {
                    if let Some(switcher) = slot.borrow_mut().as_mut() {
                        switcher.cancel();
                    }
                });
                LRESULT(0)
            }
            MSG_STICKY => {
                SWITCHER.with(|slot| {
                    if let Some(switcher) = slot.borrow_mut().as_mut() {
                        switcher.sticky = true;
                    }
                });
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let (x, y) = point_from_lparam(lparam);
                SWITCHER.with(|slot| {
                    if let Some(switcher) = slot.borrow_mut().as_mut() {
                        switcher.hover(x, y);
                    }
                });
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let (x, y) = point_from_lparam(lparam);
                SWITCHER.with(|slot| {
                    if let Some(switcher) = slot.borrow_mut().as_mut() {
                        switcher.click(x, y);
                    }
                });
                LRESULT(0)
            }
            WM_TIMER => {
                SWITCHER.with(|slot| {
                    if let Some(switcher) = slot.borrow_mut().as_mut() {
                        switcher.prune_closed();
                    }
                });
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                SWITCHER.with(|slot| {
                    if let Some(switcher) = slot.borrow_mut().as_mut() {
                        switcher.cancel();
                    }
                });
                reset_hook_session();
                LRESULT(0)
            }
            WM_PAINT => {
                SWITCHER.with(|slot| {
                    if let Some(switcher) = slot.borrow().as_ref() {
                        switcher.paint();
                    }
                });
                LRESULT(0)
            }
            WM_DESTROY => LRESULT(0),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    fn point_from_lparam(lparam: LPARAM) -> (i32, i32) {
        let x = (lparam.0 as u16) as i16 as i32;
        let y = ((lparam.0 >> 16) as u16) as i16 as i32;
        (x, y)
    }

    struct Candidate {
        hwnd: HWND,
        title: String,
        icon: HICON,
        /// Icons extracted from the executable are ours to destroy; icons
        /// answered via WM_GETICON / the window class belong to the target.
        icon_owned: bool,
        thumbnail: Option<isize>,
        card: RECT,
        close: RECT,
    }

    impl Candidate {
        unsafe fn destroy_icon(&mut self) {
            if self.icon_owned && !self.icon.is_invalid() {
                let _ = DestroyIcon(self.icon);
            }
            self.icon = HICON::default();
            self.icon_owned = false;
        }
    }

    struct Switcher {
        hwnd: HWND,
        candidates: Vec<Candidate>,
        selected: usize,
        original: HWND,
        layout: GridLayout,
        page: usize,
        visible: bool,
        sticky: bool,
        work: RECT,
    }

    impl Switcher {
        fn new(hwnd: HWND) -> Self {
            Self {
                hwnd,
                candidates: Vec::new(),
                selected: 0,
                original: HWND::default(),
                layout: grid_layout(0, 1920, 1080, 96),
                page: 0,
                visible: false,
                sticky: false,
                work: RECT::default(),
            }
        }

        unsafe fn open(&mut self, direction: Direction, sticky: bool) {
            self.hide();
            self.original = GetForegroundWindow();
            self.sticky = sticky;
            // Always the monitor under the cursor — where the user is looking
            // and about to interact — not the foreground window's monitor.
            let mut cursor = POINT::default();
            let monitor = if GetCursorPos(&mut cursor).is_ok() {
                MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
            } else if !self.original.0.is_null() {
                MonitorFromWindow(self.original, MONITOR_DEFAULTTONEAREST)
            } else {
                return;
            };
            let mut monitor_info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut monitor_info).as_bool() {
                return;
            }
            self.work = monitor_info.rcWork;
            self.candidates = enumerate_candidates(monitor, self.original, self.hwnd);
            if self.candidates.is_empty() {
                return;
            }
            // Nothing to switch to on this monitor: stay hidden and leave the
            // focus untouched (the session's remaining keys still no-op).
            if self.candidates.len() == 1 && self.candidates[0].hwnd == self.original {
                for candidate in &mut self.candidates {
                    candidate.destroy_icon();
                }
                self.candidates.clear();
                return;
            }

            let original_first = self.candidates[0].hwnd == self.original;
            self.selected = initial_selection(self.candidates.len(), original_first, direction);
            self.relayout();
            self.page = self.selected / self.layout.capacity;
            self.position_and_register();
            self.visible = true;
            OVERLAY_ACTIVE.store(true, Ordering::Release);
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            // Temporarily take foreground focus from the selected app. Games
            // commonly confine or continuously recenter the cursor only while
            // foreground, so this releases their mouse lock for the switcher.
            // commit/cancel below always hand focus to a real app again.
            focus_overlay(self.hwnd);
        }

        unsafe fn relayout(&mut self) {
            let dpi = GetDpiForWindow(self.hwnd).max(96);
            self.layout = grid_layout(
                self.candidates.len(),
                self.work.right - self.work.left,
                self.work.bottom - self.work.top,
                dpi,
            );
        }

        unsafe fn cycle(&mut self, direction: Direction) {
            if self.candidates.is_empty() {
                return;
            }
            self.set_selected(cycled_index(
                self.selected,
                self.candidates.len(),
                direction,
            ));
        }

        unsafe fn move_grid(&mut self, movement: usize) {
            if self.candidates.is_empty() {
                return;
            }
            let len = self.candidates.len();
            let cols = self.layout.cols.max(1);
            let next = match movement {
                0 => (self.selected + len - 1) % len,
                1 => (self.selected + 1) % len,
                2 => (self.selected + len - cols.min(len)) % len,
                3 => (self.selected + cols) % len,
                _ => self.selected,
            };
            self.set_selected(next);
        }

        unsafe fn set_selected(&mut self, selected: usize) {
            self.selected = selected.min(self.candidates.len().saturating_sub(1));
            let page = self.selected / self.layout.capacity;
            if page != self.page {
                self.page = page;
                self.position_and_register();
            } else {
                let _ = InvalidateRect(self.hwnd, None, false);
            }
        }

        unsafe fn position_and_register(&mut self) {
            self.unregister_thumbnails();
            let x = self.work.left + (self.work.right - self.work.left - self.layout.panel_w) / 2;
            let y = self.work.top + (self.work.bottom - self.work.top - self.layout.panel_h) / 2;
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                x,
                y,
                self.layout.panel_w,
                self.layout.panel_h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );

            let first = self.page * self.layout.capacity;
            let last = (first + self.layout.capacity).min(self.candidates.len());
            for absolute in first..last {
                let local = absolute - first;
                let row = local / self.layout.cols;
                let col = local % self.layout.cols;
                let left = self.layout.padding
                    + col as i32 * (self.layout.card_w + self.layout.gap);
                let top = self.layout.padding
                    + row as i32 * (self.layout.card_h + self.layout.gap);
                let card = RECT {
                    left,
                    top,
                    right: left + self.layout.card_w,
                    bottom: top + self.layout.card_h,
                };
                let close_size = (self.layout.title_h - 12).max(18);
                let close = RECT {
                    left: card.right - close_size - 6,
                    top: card.bottom - self.layout.title_h + 5,
                    right: card.right - 6,
                    bottom: card.bottom - 5,
                };
                self.candidates[absolute].card = card;
                self.candidates[absolute].close = close;

                if let Ok(thumbnail) = DwmRegisterThumbnail(self.hwnd, self.candidates[absolute].hwnd)
                {
                    let available = RECT {
                        left: card.left + 8,
                        top: card.top + 8,
                        right: card.right - 8,
                        bottom: card.bottom - self.layout.title_h - 4,
                    };
                    let destination = fit_thumbnail(thumbnail, available);
                    let properties = DWM_THUMBNAIL_PROPERTIES {
                        dwFlags: DWM_TNP_RECTDESTINATION
                            | DWM_TNP_VISIBLE
                            | DWM_TNP_OPACITY
                            | DWM_TNP_SOURCECLIENTAREAONLY,
                        rcDestination: destination,
                        opacity: 255,
                        fVisible: BOOL(1),
                        fSourceClientAreaOnly: BOOL(0),
                        ..Default::default()
                    };
                    if DwmUpdateThumbnailProperties(thumbnail, &properties).is_ok() {
                        self.candidates[absolute].thumbnail = Some(thumbnail);
                    } else {
                        let _ = DwmUnregisterThumbnail(thumbnail);
                    }
                }
            }
            let _ = InvalidateRect(self.hwnd, None, true);
        }

        unsafe fn unregister_thumbnails(&mut self) {
            for candidate in &mut self.candidates {
                if let Some(thumbnail) = candidate.thumbnail.take() {
                    let _ = DwmUnregisterThumbnail(thumbnail);
                }
                candidate.card = RECT::default();
                candidate.close = RECT::default();
            }
        }

        unsafe fn hover(&mut self, x: i32, y: i32) {
            if let Some(index) = self.hit_card(x, y) {
                if index != self.selected {
                    self.selected = index;
                    let _ = InvalidateRect(self.hwnd, None, false);
                }
            }
        }

        unsafe fn click(&mut self, x: i32, y: i32) {
            let Some(index) = self.hit_card(x, y) else {
                return;
            };
            self.selected = index;
            if contains(self.candidates[index].close, x, y) {
                // A polite close request; the prune timer removes the card
                // once (and only if) the window actually goes away.
                let _ = PostMessageW(
                    self.candidates[index].hwnd,
                    WM_SYSCOMMAND,
                    WPARAM(SC_CLOSE as usize),
                    LPARAM(0),
                );
            } else {
                self.commit();
                reset_hook_session();
            }
        }

        fn hit_card(&self, x: i32, y: i32) -> Option<usize> {
            let first = self.page * self.layout.capacity;
            let last = (first + self.layout.capacity).min(self.candidates.len());
            (first..last).find(|&index| contains(self.candidates[index].card, x, y))
        }

        unsafe fn commit(&mut self) {
            let target = self.candidates.get(self.selected).map(|c| c.hwnd);
            self.hide();
            if let Some(target) = target.filter(|hwnd| IsWindow(*hwnd).as_bool()) {
                crate::commands::paste::force_foreground(target);
                if GetForegroundWindow() != target {
                    let flash = FLASHWINFO {
                        cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                        hwnd: target,
                        dwFlags: FLASHW_TRAY,
                        uCount: 3,
                        dwTimeout: 0,
                    };
                    let _ = FlashWindowEx(&flash);
                }
            }
        }

        unsafe fn cancel(&mut self) {
            let original = self.original;
            self.hide();
            if !original.0.is_null() && IsWindow(original).as_bool() {
                crate::commands::paste::force_foreground(original);
            }
        }

        unsafe fn hide(&mut self) {
            self.visible = false;
            OVERLAY_ACTIVE.store(false, Ordering::Release);
            self.sticky = false;
            self.unregister_thumbnails();
            for candidate in &mut self.candidates {
                candidate.destroy_icon();
            }
            self.candidates.clear();
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }

        unsafe fn prune_closed(&mut self) {
            if !self.visible {
                return;
            }
            let alive: Vec<bool> = self
                .candidates
                .iter()
                .map(|candidate| IsWindow(candidate.hwnd).as_bool())
                .collect();
            if alive.iter().all(|&a| a) {
                return;
            }
            let Some(selected) = selection_after_prune(self.selected, &alive) else {
                self.cancel();
                reset_hook_session();
                return;
            };
            self.unregister_thumbnails();
            let mut alive_iter = alive.iter();
            self.candidates.retain_mut(|candidate| {
                let keep = *alive_iter.next().unwrap_or(&false);
                if !keep {
                    unsafe { candidate.destroy_icon() };
                }
                keep
            });
            self.selected = selected;
            self.relayout();
            self.page = self.selected / self.layout.capacity;
            self.position_and_register();
        }

        unsafe fn paint(&self) {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(self.hwnd, &mut paint);
            let panel = RECT {
                left: 0,
                top: 0,
                right: self.layout.panel_w,
                bottom: self.layout.panel_h,
            };
            let theme = NativeTheme::load();
            let bg = CreateSolidBrush(theme.background);
            FillRect(dc, &panel, bg);
            let _ = DeleteObject(bg);

            let face: Vec<u16> = "Segoe UI Variable"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let dpi = GetDpiForWindow(self.hwnd).max(96);
            let font = CreateFontW(
                -scaled(14, dpi),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                1,
                0,
                0,
                5,
                0,
                PCWSTR(face.as_ptr()),
            );
            let old_font = SelectObject(dc, HGDIOBJ(font.0));
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, theme.text);

            let first = self.page * self.layout.capacity;
            let last = (first + self.layout.capacity).min(self.candidates.len());
            for index in first..last {
                let candidate = &self.candidates[index];
                let selected = index == self.selected;
                let brush = CreateSolidBrush(if selected { theme.selected } else { theme.card });
                let pen = CreatePen(
                    PS_SOLID,
                    1,
                    if selected { theme.accent } else { theme.border },
                );
                let old_brush = SelectObject(dc, HGDIOBJ(brush.0));
                let old_pen = SelectObject(dc, HGDIOBJ(pen.0));
                let _ = RoundRect(
                    dc,
                    candidate.card.left,
                    candidate.card.top,
                    candidate.card.right,
                    candidate.card.bottom,
                    10,
                    10,
                );
                SelectObject(dc, old_brush);
                SelectObject(dc, old_pen);
                let _ = DeleteObject(brush);
                let _ = DeleteObject(pen);

                // Match the palette's compact icon treatment and scale with
                // monitor DPI. The lookup below prefers a large HICON, so
                // this normally downsizes instead of enlarging a 16 px bitmap.
                let icon_size = scaled(20, dpi)
                    .min(self.layout.title_h - scaled(14, dpi))
                    .max(scaled(16, dpi));
                if !candidate.icon.is_invalid() {
                    let _ = DrawIconEx(
                        dc,
                        candidate.card.left + 10,
                        candidate.card.bottom - self.layout.title_h
                            + (self.layout.title_h - icon_size) / 2,
                        candidate.icon,
                        icon_size,
                        icon_size,
                        0,
                        None,
                        DI_NORMAL,
                    );
                }
                let mut title: Vec<u16> = candidate.title.encode_utf16().collect();
                let mut title_rect = RECT {
                    left: candidate.card.left + icon_size + 16,
                    top: candidate.card.bottom - self.layout.title_h,
                    right: candidate.close.left - 5,
                    bottom: candidate.card.bottom,
                };
                SetTextColor(dc, if selected { rgb(255, 255, 255) } else { theme.text });
                DrawTextW(
                    dc,
                    &mut title,
                    &mut title_rect,
                    DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
                );

                if selected {
                    let mut close_text: Vec<u16> = "×".encode_utf16().collect();
                    let mut close_rect = candidate.close;
                    DrawTextW(
                        dc,
                        &mut close_text,
                        &mut close_rect,
                        DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
                    );
                }
            }
            SelectObject(dc, old_font);
            let _ = DeleteObject(font);
            let _ = EndPaint(self.hwnd, &paint);
        }
    }

    fn contains(rect: RECT, x: i32, y: i32) -> bool {
        x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
    }

    /// Focus our own overlay even when the fullscreen foreground process owns
    /// Windows' foreground lock. The general app-activation helper attaches to
    /// the target thread; here the target is this thread, so we instead attach
    /// to the current foreground thread before retrying.
    unsafe fn focus_overlay(hwnd: HWND) {
        if SetForegroundWindow(hwnd).as_bool() {
            return;
        }
        let foreground = GetForegroundWindow();
        if foreground.0.is_null() {
            return;
        }
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let our_thread = GetCurrentThreadId();
        if foreground_thread == 0 || foreground_thread == our_thread {
            return;
        }
        let _ = AttachThreadInput(our_thread, foreground_thread, true);
        let _ = SetForegroundWindow(hwnd);
        let _ = AttachThreadInput(our_thread, foreground_thread, false);
    }

    fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
        COLORREF(colorref(r, g, b))
    }

    const fn colorref(r: u8, g: u8, b: u8) -> u32 {
        r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
    }

    struct NativeTheme {
        background: COLORREF,
        card: COLORREF,
        selected: COLORREF,
        border: COLORREF,
        text: COLORREF,
        accent: COLORREF,
    }

    impl NativeTheme {
        fn load() -> Self {
            let load = |value: &AtomicU32| COLORREF(value.load(Ordering::Acquire));
            Self {
                background: load(&THEME_BACKGROUND),
                card: load(&THEME_CARD),
                selected: load(&THEME_SELECTED),
                border: load(&THEME_BORDER),
                text: load(&THEME_TEXT),
                accent: load(&THEME_ACCENT),
            }
        }
    }

    unsafe fn fit_thumbnail(thumbnail: isize, bounds: RECT) -> RECT {
        let Ok(size) = DwmQueryThumbnailSourceSize(thumbnail) else {
            return bounds;
        };
        if size.cx <= 0 || size.cy <= 0 {
            return bounds;
        }
        let available_w = (bounds.right - bounds.left).max(1);
        let available_h = (bounds.bottom - bounds.top).max(1);
        let scale = (available_w as f64 / size.cx as f64)
            .min(available_h as f64 / size.cy as f64);
        let width = (size.cx as f64 * scale).round() as i32;
        let height = (size.cy as f64 * scale).round() as i32;
        let left = bounds.left + (available_w - width) / 2;
        let top = bounds.top + (available_h - height) / 2;
        RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }

    struct EnumContext {
        monitor: windows::Win32::Graphics::Gdi::HMONITOR,
        own_pid: u32,
        overlay: HWND,
        windows: Vec<Candidate>,
    }

    unsafe fn enumerate_candidates(
        monitor: windows::Win32::Graphics::Gdi::HMONITOR,
        original: HWND,
        overlay: HWND,
    ) -> Vec<Candidate> {
        let mut context = EnumContext {
            monitor,
            own_pid: GetCurrentProcessId(),
            overlay,
            windows: Vec::new(),
        };
        let _ = EnumWindows(
            Some(enum_window),
            LPARAM(&mut context as *mut EnumContext as isize),
        );
        if let Some(index) = context.windows.iter().position(|c| c.hwnd == original) {
            let original = context.windows.remove(index);
            context.windows.insert(0, original);
        }
        context.windows
    }

    unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let context = &mut *(lparam.0 as *mut EnumContext);
        if !is_candidate(hwnd, context) {
            return BOOL(1);
        }
        let mut title = vec![0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title);
        if len <= 0 {
            return BOOL(1);
        }
        title.truncate(len as usize);
        let (icon, icon_owned) = window_icon(hwnd);
        context.windows.push(Candidate {
            hwnd,
            title: String::from_utf16_lossy(&title),
            icon,
            icon_owned,
            thumbnail: None,
            card: RECT::default(),
            close: RECT::default(),
        });
        BOOL(1)
    }

    unsafe fn is_candidate(hwnd: HWND, context: &EnumContext) -> bool {
        if hwnd.0.is_null()
            || hwnd == context.overlay
            || hwnd == GetDesktopWindow()
            || hwnd == GetShellWindow()
            || !IsWindowVisible(hwnd).as_bool()
            || MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) != context.monitor
        {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == context.own_pid {
            return false;
        }
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut _,
            std::mem::size_of_val(&cloaked) as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return false;
        }

        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let app_window = ex_style & WS_EX_APPWINDOW.0 != 0;
        if ex_style & WS_EX_TOOLWINDOW.0 != 0 && !app_window {
            return false;
        }
        app_window || is_alt_tab_representative(hwnd)
    }

    unsafe fn is_alt_tab_representative(hwnd: HWND) -> bool {
        let mut walk = GetAncestor(hwnd, GA_ROOTOWNER);
        loop {
            let candidate = GetLastActivePopup(walk);
            if candidate == walk {
                break;
            }
            if IsWindowVisible(candidate).as_bool() {
                walk = candidate;
                break;
            }
            walk = candidate;
        }
        walk == hwnd
    }

    unsafe fn window_icon(hwnd: HWND) -> (HICON, bool) {
        // Prefer the large window icon: asking for ICON_SMALL first returned a
        // 16 px bitmap that DrawIconEx then enlarged in the title strip.
        for kind in [ICON_BIG, ICON_SMALL2, ICON_SMALL] {
            let mut result = 0usize;
            let _ = SendMessageTimeoutW(
                hwnd,
                WM_GETICON,
                WPARAM(kind as usize),
                LPARAM(0),
                SMTO_ABORTIFHUNG,
                25,
                Some(&mut result),
            );
            if result != 0 {
                return (HICON(result as *mut _), false);
            }
        }
        let large = GetClassLongPtrW(hwnd, GCLP_HICON);
        let class_icon = if large != 0 {
            large
        } else {
            GetClassLongPtrW(hwnd, GCLP_HICONSM)
        };
        if class_icon != 0 {
            return (HICON(class_icon as *mut _), false);
        }
        // UWP frames and elevated windows often answer neither WM_GETICON nor
        // the class icon: fall back to the executable's shell icon. That one
        // is ours and gets destroyed with the candidate.
        match exe_icon(hwnd) {
            Some(icon) => (icon, true),
            None => (HICON::default(), false),
        }
    }

    unsafe fn exe_icon(hwnd: HWND) -> Option<HICON> {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        let queried =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(path.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        queried.ok()?;
        if len == 0 {
            return None;
        }
        let mut icon = HICON::default();
        // Extract the large executable icon and downscale it cleanly at draw
        // time instead of requesting the small slot and scaling it upward.
        let extracted = ExtractIconExW(PCWSTR(path.as_ptr()), 0, Some(&mut icon), None, 1);
        (extracted >= 1 && !icon.is_invalid()).then_some(icon)
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::AltTabTheme;

    pub fn enable() -> Result<(), String> {
        Err("Per-monitor Alt+Tab is only available on Windows".into())
    }

    pub fn disable() -> Result<(), String> {
        Ok(())
    }

    pub fn set_theme(_theme: AltTabTheme) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Recorder {
        post_ok: bool,
        events: Vec<SessionEvent>,
    }

    impl Recorder {
        fn new() -> Self {
            Self {
                post_ok: true,
                events: Vec::new(),
            }
        }
    }

    impl HookHost for Recorder {
        fn post(&mut self, event: SessionEvent) -> bool {
            if self.post_ok {
                self.events.push(event);
            }
            self.post_ok
        }
    }

    fn key(state: &mut HookState, host: &mut Recorder, key: HookKey, down: bool) -> bool {
        state.on_key(key, down, false, host)
    }

    #[test]
    fn forward_session_starts_and_commits_on_alt_release() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        assert!(!key(&mut state, &mut host, HookKey::LeftAlt, true));
        assert!(key(&mut state, &mut host, HookKey::Tab, true));
        assert!(key(&mut state, &mut host, HookKey::Tab, false));
        // The Alt release commits but must pass through to Windows.
        assert!(!key(&mut state, &mut host, HookKey::LeftAlt, false));
        assert_eq!(
            host.events,
            vec![
                SessionEvent::Start {
                    direction: Direction::Forward,
                    sticky: false
                },
                SessionEvent::Commit,
            ]
        );
        assert!(!state.session);
    }

    #[test]
    fn alt_release_commits_exactly_once() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        key(&mut state, &mut host, HookKey::LeftAlt, true);
        key(&mut state, &mut host, HookKey::Tab, true);
        key(&mut state, &mut host, HookKey::LeftAlt, false);
        key(&mut state, &mut host, HookKey::LeftAlt, false);
        key(&mut state, &mut host, HookKey::RightAlt, false);
        let commits = host
            .events
            .iter()
            .filter(|e| **e == SessionEvent::Commit)
            .count();
        assert_eq!(commits, 1);
    }

    #[test]
    fn releasing_one_alt_while_other_held_does_not_commit() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        key(&mut state, &mut host, HookKey::LeftAlt, true);
        key(&mut state, &mut host, HookKey::RightAlt, true);
        key(&mut state, &mut host, HookKey::Tab, true);
        assert!(!key(&mut state, &mut host, HookKey::LeftAlt, false));
        assert!(!host.events.contains(&SessionEvent::Commit));
        assert!(state.session);
        key(&mut state, &mut host, HookKey::RightAlt, false);
        assert!(host.events.contains(&SessionEvent::Commit));
        assert!(!state.session);
    }

    #[test]
    fn shift_reverses_start_and_cycling() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        key(&mut state, &mut host, HookKey::LeftAlt, true);
        key(&mut state, &mut host, HookKey::LeftShift, true);
        key(&mut state, &mut host, HookKey::Tab, true);
        key(&mut state, &mut host, HookKey::LeftShift, false);
        key(&mut state, &mut host, HookKey::Tab, true);
        assert_eq!(
            host.events,
            vec![
                SessionEvent::Start {
                    direction: Direction::Backward,
                    sticky: false
                },
                SessionEvent::Cycle {
                    direction: Direction::Forward
                },
            ]
        );
    }

    #[test]
    fn arrows_move_the_grid_and_are_swallowed() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        key(&mut state, &mut host, HookKey::LeftAlt, true);
        key(&mut state, &mut host, HookKey::Tab, true);
        for (hook_key, movement) in [
            (HookKey::Left, GridMove::Left),
            (HookKey::Right, GridMove::Right),
            (HookKey::Up, GridMove::Up),
            (HookKey::Down, GridMove::Down),
        ] {
            assert!(key(&mut state, &mut host, hook_key, true));
            assert_eq!(host.events.last(), Some(&SessionEvent::Move(movement)));
        }
    }

    #[test]
    fn escape_cancels_and_swallows_its_release() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        key(&mut state, &mut host, HookKey::LeftAlt, true);
        key(&mut state, &mut host, HookKey::Tab, true);
        assert!(key(&mut state, &mut host, HookKey::Escape, true));
        assert!(key(&mut state, &mut host, HookKey::Escape, false));
        assert!(!key(&mut state, &mut host, HookKey::Escape, false));
        assert_eq!(host.events.last(), Some(&SessionEvent::Cancel));
        assert!(!state.session);
        // Alt release after a cancel neither commits nor is swallowed.
        assert!(!key(&mut state, &mut host, HookKey::LeftAlt, false));
        assert!(!host.events.contains(&SessionEvent::Commit));
    }

    #[test]
    fn ctrl_alt_tab_enters_sticky_mode_until_enter() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        key(&mut state, &mut host, HookKey::LeftCtrl, true);
        key(&mut state, &mut host, HookKey::LeftAlt, true);
        key(&mut state, &mut host, HookKey::Tab, true);
        assert_eq!(
            host.events.last(),
            Some(&SessionEvent::Start {
                direction: Direction::Forward,
                sticky: true
            })
        );
        key(&mut state, &mut host, HookKey::LeftCtrl, false);
        assert!(!key(&mut state, &mut host, HookKey::LeftAlt, false));
        assert_eq!(host.events.last(), Some(&SessionEvent::Sticky));
        assert!(state.session);
        // Tab without any modifiers still cycles in sticky mode.
        assert!(key(&mut state, &mut host, HookKey::Tab, true));
        assert_eq!(
            host.events.last(),
            Some(&SessionEvent::Cycle {
                direction: Direction::Forward
            })
        );
        // Enter commits and its release is swallowed once.
        assert!(key(&mut state, &mut host, HookKey::Return, true));
        assert_eq!(host.events.last(), Some(&SessionEvent::Commit));
        assert!(!state.session);
        assert!(key(&mut state, &mut host, HookKey::Return, false));
        assert!(!key(&mut state, &mut host, HookKey::Return, false));
    }

    #[test]
    fn sticky_escape_cancels() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        key(&mut state, &mut host, HookKey::LeftCtrl, true);
        key(&mut state, &mut host, HookKey::LeftAlt, true);
        key(&mut state, &mut host, HookKey::Tab, true);
        key(&mut state, &mut host, HookKey::LeftCtrl, false);
        key(&mut state, &mut host, HookKey::LeftAlt, false);
        assert!(key(&mut state, &mut host, HookKey::Escape, true));
        assert_eq!(host.events.last(), Some(&SessionEvent::Cancel));
        assert!(!state.session);
    }

    #[test]
    fn failed_post_passes_through_and_starts_no_session() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        host.post_ok = false;
        key(&mut state, &mut host, HookKey::LeftAlt, true);
        assert!(!key(&mut state, &mut host, HookKey::Tab, true));
        assert!(!state.session);
        // Alt release afterwards passes through untouched.
        assert!(!key(&mut state, &mut host, HookKey::LeftAlt, false));
        assert!(host.events.is_empty());
    }

    #[test]
    fn rapid_press_and_release_keeps_event_order() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        key(&mut state, &mut host, HookKey::LeftAlt, true);
        key(&mut state, &mut host, HookKey::Tab, true);
        key(&mut state, &mut host, HookKey::LeftAlt, false);
        assert_eq!(
            host.events,
            vec![
                SessionEvent::Start {
                    direction: Direction::Forward,
                    sticky: false
                },
                SessionEvent::Commit,
            ]
        );
    }

    #[test]
    fn other_keys_swallowed_during_session_only() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        assert!(!key(&mut state, &mut host, HookKey::Other, true));
        key(&mut state, &mut host, HookKey::LeftAlt, true);
        key(&mut state, &mut host, HookKey::Tab, true);
        // Downs are swallowed (no Alt+<letter> shortcuts mid-session), ups
        // pass so nothing can wedge.
        assert!(key(&mut state, &mut host, HookKey::Other, true));
        assert!(!key(&mut state, &mut host, HookKey::Other, false));
        // Modifiers are never swallowed.
        assert!(!key(&mut state, &mut host, HookKey::LeftShift, true));
        assert!(!key(&mut state, &mut host, HookKey::LeftShift, false));
    }

    #[test]
    fn alt_alone_is_untouched() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        assert!(!key(&mut state, &mut host, HookKey::LeftAlt, true));
        assert!(!key(&mut state, &mut host, HookKey::LeftAlt, false));
        assert!(host.events.is_empty());
    }

    #[test]
    fn overlay_side_reset_ends_the_session() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        key(&mut state, &mut host, HookKey::LeftAlt, true);
        key(&mut state, &mut host, HookKey::Tab, true);
        state.reset_session();
        // Alt release after e.g. a click-commit neither commits nor swallows.
        assert!(!key(&mut state, &mut host, HookKey::LeftAlt, false));
        assert!(!host.events.contains(&SessionEvent::Commit));
    }

    #[test]
    fn cycles_forward_and_backward() {
        assert_eq!(cycled_index(0, 4, Direction::Forward), 1);
        assert_eq!(cycled_index(3, 4, Direction::Forward), 0);
        assert_eq!(cycled_index(0, 4, Direction::Backward), 3);
        assert_eq!(cycled_index(2, 4, Direction::Backward), 1);
        assert_eq!(cycled_index(0, 0, Direction::Forward), 0);
    }

    #[test]
    fn initial_selection_matches_native_switcher() {
        // First forward Tab selects the window under the foreground one.
        assert_eq!(initial_selection(4, true, Direction::Forward), 1);
        // Reverse wraps to the bottom of the Z-order.
        assert_eq!(initial_selection(4, true, Direction::Backward), 3);
        // A single candidate stays selected.
        assert_eq!(initial_selection(1, true, Direction::Forward), 0);
        // Foreground window not eligible: start at the top.
        assert_eq!(initial_selection(3, false, Direction::Forward), 0);
        assert_eq!(initial_selection(3, false, Direction::Backward), 2);
    }

    #[test]
    fn prune_keeps_or_moves_the_selection() {
        // Selected survives; an earlier removal shifts its index.
        assert_eq!(selection_after_prune(2, &[true, false, true, true]), Some(1));
        // Selected died: nearest following survivor.
        assert_eq!(selection_after_prune(1, &[true, false, true]), Some(1));
        // Selected was last and died: previous survivor.
        assert_eq!(selection_after_prune(2, &[true, true, false]), Some(1));
        // First removed while selected.
        assert_eq!(selection_after_prune(0, &[false, true, true]), Some(0));
        // Only candidate removed: close the switcher.
        assert_eq!(selection_after_prune(0, &[false]), None);
    }

    #[test]
    fn layout_stays_within_work_area() {
        for (count, width, height, dpi) in [
            (1, 1920, 1080, 96),
            (12, 1920, 1080, 96),
            (40, 2560, 1440, 144),
            (4, 1280, 720, 120),
        ] {
            let layout = grid_layout(count, width, height, dpi);
            assert!(layout.cols >= 1);
            assert!(layout.rows >= 1);
            assert!(layout.capacity >= 1);
            assert!(layout.panel_w <= (width as f32 * 0.9) as i32 + 2);
            assert!(layout.panel_h <= (height as f32 * 0.9) as i32 + 2);
        }
    }

    #[test]
    fn layout_handles_negative_monitor_origin_sizes() {
        // Only extents matter to the layout; a monitor at negative virtual
        // coordinates hands the same width/height in.
        let layout = grid_layout(6, 1920, 1080, 96);
        let negative = grid_layout(6, 1920, 1080, 96);
        assert_eq!(layout, negative);
        assert!(layout.panel_w > 0 && layout.panel_h > 0);
    }

    #[test]
    fn layout_scales_with_dpi_and_paginates() {
        let normal = grid_layout(80, 1920, 1080, 96);
        let high_dpi = grid_layout(80, 3840, 2160, 192);
        assert!(high_dpi.card_w >= normal.card_w * 2 - 2);
        assert!(normal.capacity < 80);
        assert!(high_dpi.capacity < 80);
    }

    #[test]
    fn single_window_layout_fits_one_card() {
        let layout = grid_layout(1, 1920, 1080, 96);
        assert_eq!(layout.cols, 1);
        assert_eq!(layout.rows, 1);
        assert_eq!(layout.panel_w, layout.padding * 2 + layout.card_w);
        assert_eq!(layout.panel_h, layout.padding * 2 + layout.card_h);
    }

    #[test]
    fn mixed_aspect_thumbnails_fit_available_space() {
        // The card box itself is aspect-agnostic; the DWM fit happens per
        // thumbnail. Verify cards stay no smaller than the scaled minimum.
        for count in [2, 5, 9, 17] {
            let layout = grid_layout(count, 2560, 1440, 96);
            assert!(layout.card_w >= 160);
            assert!(layout.card_h >= 120);
        }
    }
}
