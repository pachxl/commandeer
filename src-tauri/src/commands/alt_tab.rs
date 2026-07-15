//! Windows-only per-monitor Alt+Tab replacement.
//!
//! A `WH_KEYBOARD_LL` hook intercepts only an active Alt+Tab session and posts
//! tiny messages to a dedicated native overlay thread. Window enumeration,
//! DWM thumbnail registration, painting, and foreground activation never run
//! inside the hook callback: a slow low-level hook can stall input and Windows
//! may silently remove it.
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
    /// True when the foreground window is (borderless-)fullscreen — the
    /// shortcut then passes through to the native switcher.
    fn fullscreen_foreground(&mut self) -> bool;
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
                    if host.fullscreen_foreground() {
                        return false;
                    }
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

const MAX_GRID_COLUMNS: usize = 6;

fn scaled(value: i32, dpi: u32) -> i32 {
    ((value as i64 * dpi.max(96) as i64) / 96) as i32
}

fn grid_layout(count: usize, work_w: i32, work_h: i32, dpi: u32) -> GridLayout {
    let gap = scaled(13, dpi);
    let padding = scaled(25, dpi);
    let title_h = scaled(41, dpi);
    let desired_w = scaled(268, dpi);
    let desired_h = scaled(190, dpi);
    let min_w = scaled(160, dpi);
    let min_h = scaled(120, dpi);
    let max_w = ((work_w as f32) * 0.9) as i32;
    let max_h = ((work_h as f32) * 0.9) as i32;

    let usable_w = (max_w - padding * 2).max(min_w);
    let usable_h = (max_h - padding * 2).max(min_h);
    let desired_cols =
        (((usable_w + gap) / (desired_w + gap)).max(1) as usize).min(MAX_GRID_COLUMNS);
    let desired_rows = ((usable_h + gap) / (desired_h + gap)).max(1) as usize;
    let mut cols = count.max(1).min(desired_cols.max(1));
    let mut rows = count.max(1).div_ceil(cols).min(desired_rows.max(1));

    if cols * rows < count {
        let max_cols = (((usable_w + gap) / (min_w + gap)).max(1) as usize).min(MAX_GRID_COLUMNS);
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

/// Horizontal position for a card within its page. Full rows begin at the
/// panel padding; partial rows are centered as a group, including a final row
/// containing only one card.
fn grid_card_left(layout: GridLayout, local: usize, visible: usize) -> i32 {
    let cols = layout.cols.max(1);
    let row_start = (local / cols) * cols;
    let row_items = visible.saturating_sub(row_start).min(cols).max(1);
    let row_width =
        layout.card_w * row_items as i32 + layout.gap * row_items.saturating_sub(1) as i32;
    let row_left = (layout.panel_w - row_width) / 2;
    row_left + (local % cols) as i32 * (layout.card_w + layout.gap)
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{
        cycled_index, grid_card_left, grid_layout, initial_selection, selection_after_prune,
        AltTabTheme, Direction, GridLayout, GridMove, HookHost, HookKey, HookState, SessionEvent,
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
        GetCurrentProcessId, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
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
        GetShellWindow, GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
        IsWindow, IsWindowVisible, IsZoomed, KillTimer, LoadCursorW, PostMessageW,
        PostThreadMessageW, RegisterClassW, SendMessageTimeoutW, SetTimer, SetWindowPos,
        SetWindowsHookExW, ShowWindow, TranslateMessage, UnhookWindowsHookEx, CS_HREDRAW,
        CS_VREDRAW, DI_NORMAL, FLASHWINFO, FLASHW_TRAY, GA_ROOTOWNER, GCLP_HICON, GCLP_HICONSM,
        GWL_EXSTYLE, GWL_STYLE, HHOOK, HICON, HWND_TOPMOST, ICON_BIG, ICON_SMALL, ICON_SMALL2,
        IDC_ARROW, KBDLLHOOKSTRUCT, LLKHF_ALTDOWN, LLKHF_INJECTED, LLKHF_UP, MSG, SC_CLOSE,
        SMTO_ABORTIFHUNG, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE,
        WH_KEYBOARD_LL, WM_APP, WM_DESTROY, WM_DISPLAYCHANGE, WM_GETICON, WM_LBUTTONUP,
        WM_MOUSEMOVE, WM_PAINT, WM_QUIT, WM_SYSCOMMAND, WM_TIMER, WNDCLASSW, WS_CAPTION,
        WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        WS_THICKFRAME,
    };

    const MSG_START: u32 = WM_APP + 1;
    const MSG_CYCLE: u32 = WM_APP + 2;
    const MSG_MOVE: u32 = WM_APP + 3;
    const MSG_COMMIT: u32 = WM_APP + 4;
    const MSG_CANCEL: u32 = WM_APP + 5;
    const MSG_STICKY: u32 = WM_APP + 6;
    const PRUNE_TIMER: usize = 1;

    static READY: AtomicBool = AtomicBool::new(false);
    static OVERLAY: AtomicIsize = AtomicIsize::new(0);
    static THREAD_ID: AtomicU32 = AtomicU32::new(0);
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
        thread: JoinHandle<()>,
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

        let (tx, rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("per-monitor-alt-tab".into())
            .spawn(move || service_thread(tx))
            .map_err(|e| e.to_string())?;

        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => {
                *guard = Some(Service { thread });
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let tid = THREAD_ID.load(Ordering::Acquire);
                if tid != 0 {
                    unsafe {
                        let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
                    }
                }
                let _ = thread.join();
                Err("Alt+Tab service timed out during startup".into())
            }
        }
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
        let tid = THREAD_ID.load(Ordering::Acquire);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        service
            .thread
            .join()
            .map_err(|_| "Alt+Tab service thread panicked".to_string())
    }

    fn service_thread(started: mpsc::SyncSender<Result<(), String>>) {
        THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
        let result = unsafe { initialize_service() };
        match result {
            Ok((hwnd, hook)) => {
                OVERLAY.store(hwnd.0 as isize, Ordering::Release);
                READY.store(true, Ordering::Release);
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
                    let _ = UnhookWindowsHookEx(hook);
                    let _ = KillTimer(hwnd, PRUNE_TIMER);
                    let _ = DestroyWindow(hwnd);
                }
            }
            Err(e) => {
                let _ = started.send(Err(e));
            }
        }
        READY.store(false, Ordering::Release);
        OVERLAY.store(0, Ordering::Release);
        THREAD_ID.store(0, Ordering::Release);
    }

    unsafe fn initialize_service() -> Result<(HWND, HHOOK), String> {
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
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
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

        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_proc),
            HINSTANCE(module.0),
            0,
        )
        .map_err(|e| format!("could not install Alt+Tab keyboard hook: {e}"))?;
        Ok((hwnd, hook))
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
        fn fullscreen_foreground(&mut self) -> bool {
            unsafe { native_fullscreen_fallback() }
        }

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

    unsafe fn native_fullscreen_fallback() -> bool {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() || IsZoomed(hwnd).as_bool() {
            return false;
        }
        // Tauri's transparent, undecorated palette can occasionally be
        // reported with monitor-sized extended bounds while it is visible.
        // It is our UI, never a game that should opt into native Alt+Tab.
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return false;
        }
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }
        let m = info.rcMonitor;
        let covers = rect.left <= m.left + 2
            && rect.top <= m.top + 2
            && rect.right >= m.right - 2
            && rect.bottom >= m.bottom - 2;
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        covers && style & WS_CAPTION.0 != WS_CAPTION.0 && style & WS_THICKFRAME.0 == 0
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
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
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
            let visible = last - first;
            for absolute in first..last {
                let local = absolute - first;
                let row = local / self.layout.cols;
                let left = grid_card_left(self.layout, local, visible);
                let top = self.layout.padding + row as i32 * (self.layout.card_h + self.layout.gap);
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
                KEYS.with(|keys| keys.borrow_mut().reset_session());
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
            self.hide();
        }

        unsafe fn hide(&mut self) {
            self.visible = false;
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
                self.hide();
                KEYS.with(|keys| keys.borrow_mut().reset_session());
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
            let font = CreateFontW(
                -((15 * GetDpiForWindow(self.hwnd).max(96) as i32) / 96),
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
                    11,
                    11,
                );
                SelectObject(dc, old_brush);
                SelectObject(dc, old_pen);
                let _ = DeleteObject(brush);
                let _ = DeleteObject(pen);

                let icon_size = (self.layout.title_h - 14).clamp(16, 28);
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
        for kind in [ICON_SMALL2, ICON_SMALL, ICON_BIG] {
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
        let small = GetClassLongPtrW(hwnd, GCLP_HICONSM);
        let class_icon = if small != 0 {
            small
        } else {
            GetClassLongPtrW(hwnd, GCLP_HICON)
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
        let extracted = ExtractIconExW(PCWSTR(path.as_ptr()), 0, None, Some(&mut icon), 1);
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
        fullscreen: bool,
        post_ok: bool,
        events: Vec<SessionEvent>,
    }

    impl Recorder {
        fn new() -> Self {
            Self {
                fullscreen: false,
                post_ok: true,
                events: Vec::new(),
            }
        }
    }

    impl HookHost for Recorder {
        fn fullscreen_foreground(&mut self) -> bool {
            self.fullscreen
        }
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
    fn fullscreen_foreground_passes_through() {
        let mut state = HookState::default();
        let mut host = Recorder::new();
        host.fullscreen = true;
        key(&mut state, &mut host, HookKey::LeftAlt, true);
        assert!(!key(&mut state, &mut host, HookKey::Tab, true));
        assert!(host.events.is_empty());
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
        assert!(layout.card_w > 260);
        assert!(layout.card_h > 180);
        assert!(layout.title_h > 38);
    }

    #[test]
    fn layout_caps_wide_monitors_at_six_columns() {
        let layout = grid_layout(40, 7680, 4320, 96);
        assert_eq!(layout.cols, MAX_GRID_COLUMNS);
    }

    #[test]
    fn partial_rows_are_centered() {
        let layout = grid_layout(7, 1920, 1080, 96);
        assert_eq!(layout.cols, 6);
        assert_eq!(grid_card_left(layout, 0, 7), layout.padding);

        let last_left = grid_card_left(layout, 6, 7);
        assert_eq!(last_left, (layout.panel_w - layout.card_w) / 2);
        assert!(last_left > layout.padding);
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
