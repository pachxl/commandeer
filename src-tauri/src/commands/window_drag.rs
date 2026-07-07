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

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------
#[cfg(target_os = "windows")]
mod platform {
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use windows::core::w;
    use windows::Win32::Foundation::{
        BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
    };
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetMonitorInfoW,
        MonitorFromPoint, MonitorFromWindow, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::{
        AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
        KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_MENU,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BeginDeferWindowPos, BringWindowToTop, CallNextHookEx, CreateWindowExW, DeferWindowPos,
        DefWindowProcW, DestroyWindow, DispatchMessageW, EndDeferWindowPos, GetAncestor,
        GetClassNameW, GetCursorPos, GetDesktopWindow, GetForegroundWindow, GetMessageW,
        GetShellWindow, GetWindowLongW, SetForegroundWindow,
        GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, IsZoomed, PostThreadMessageW,
        RegisterClassW, SetTimer, SetWindowPos, SetWindowsHookExW, ShowWindow, TranslateMessage,
        UnhookWindowsHookEx, UpdateLayeredWindow, WindowFromPoint, GA_ROOT, GWL_STYLE, HHOOK,
        HWND_TOP, MSG,
        MSLLHOOKSTRUCT, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER,
        SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_MAXIMIZE, SW_RESTORE, SW_SHOWNA, ULW_ALPHA,
        WH_MOUSE_LL,
        WM_LBUTTONDOWN,
        WM_LBUTTONUP, WM_MOUSEMOVE, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_TIMER, WNDCLASSW,
        WS_CAPTION, WS_THICKFRAME,
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
        WS_POPUP,
    };

    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Move,
        Resize,
    }

    #[derive(Clone, Copy, Default)]
    struct Edges {
        left: bool,
        right: bool,
        top: bool,
        bottom: bool,
    }

    /// Which screen edge a window is snapped against (fills that half of the
    /// monitor work area). The one edge NOT against a wall is the only edge a
    /// resize may move — e.g. a `Right`-snapped window resizes from its left
    /// edge only, keeping its snapped height. `None` = a normal, freely
    /// resizable window (all four edges available, corner picked by quadrant).
    #[derive(Clone, Copy, PartialEq)]
    enum SnapKind {
        None,
        Left,
        Right,
        Top,
        Bottom,
    }

    impl SnapKind {
        /// The single edge a resize may move for a snapped window.
        fn free_edge(self) -> Edges {
            match self {
                SnapKind::None => Edges::default(),
                SnapKind::Left => Edges {
                    right: true,
                    ..Edges::default()
                },
                SnapKind::Right => Edges {
                    left: true,
                    ..Edges::default()
                },
                SnapKind::Top => Edges {
                    bottom: true,
                    ..Edges::default()
                },
                SnapKind::Bottom => Edges {
                    top: true,
                    ..Edges::default()
                },
            }
        }
    }

    // One window snapped flush against the resized target's free edge. Its
    // facing edge tracks the target's shared boundary (three edges stay fixed)
    // so shrinking the target grows the neighbour — tiling. `overlap` is how far
    // its facing edge is pushed past the boundary into the invisible-border
    // region, to halve the visible gap; it's per-neighbour because border insets
    // can differ between windows.
    #[derive(Clone, Copy)]
    struct Neighbor {
        hwnd: isize,
        rect: RECT,
        overlap: i32,
    }

    // Drag parameters, set once per grab by the hook and read by the mover
    // thread. The hook never tracks the moving cursor at all — the mover polls
    // `GetCursorPos` itself — so the hook's hot path does zero work.
    struct DragState {
        active: bool,
        mode: Mode,
        hwnd: isize,
        start_x: i32,
        start_y: i32,
        rect: RECT,
        edges: Edges,
        // The grabbed window was maximized; the mover restores it on its first
        // frame (ShowWindow can block on the target app, so never in the hook).
        restore_max: bool,
        // Snap orientation of a resized window (None for a normal window).
        snap: SnapKind,
        // Every window snapped flush along the target's free edge. Resizing the
        // target moves all of their facing edges together, so an arbitrary grid
        // (e.g. one tall window beside a stack of two) stays tiled. Empty = none.
        neighbors: Vec<Neighbor>,
        // The moved window's invisible-border insets, captured at grab so an
        // edge-snap on release lines the visible frame up with the work area.
        border: RECT,
    }

    impl Default for DragState {
        fn default() -> Self {
            Self {
                active: false,
                mode: Mode::Move,
                hwnd: 0,
                start_x: 0,
                start_y: 0,
                rect: RECT::default(),
                edges: Edges::default(),
                restore_max: false,
                snap: SnapKind::None,
                neighbors: Vec::new(),
                border: RECT::default(),
            }
        }
    }

    // The mover thread applies window moves at ~200 Hz regardless of mouse
    // polling rate. `thread::sleep` only honours a sub-16 ms interval when the
    // system timer resolution is raised — the mover does that (timeBeginPeriod)
    // for the duration of each drag, so this interval is real, not rounded up
    // to the default ~15.6 ms tick (which would cap us near 64 Hz and look
    // choppy next to a native title-bar drag).
    const MOVE_INTERVAL: Duration = Duration::from_millis(5);

    static STATE: OnceLock<Mutex<DragState>> = OnceLock::new();
    fn state() -> &'static Mutex<DragState> {
        STATE.get_or_init(|| Mutex::new(DragState::default()))
    }

    // HHOOK / thread id of the running hook (0 = not running). ACTIVE mirrors
    // DragState.active as a lock-free gate for the mover thread.
    static HOOK: AtomicIsize = AtomicIsize::new(0);
    static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    // Bumped on each grab so the mover resets its per-drag dead-zone state.
    static GEN: AtomicU64 = AtomicU64::new(0);
    // Keeps the mover thread alive while the feature is enabled.
    static MOVER_RUN: AtomicBool = AtomicBool::new(false);

    const MIN_SIZE: i32 = 120;

    pub fn enable() -> Result<(), String> {
        if HOOK.load(Ordering::Relaxed) != 0 {
            return Ok(());
        }
        // The mover thread does all SetWindowPos work, decoupled from the hook.
        MOVER_RUN.store(true, Ordering::Relaxed);
        std::thread::Builder::new()
            .name("window-drag-mover".into())
            .spawn(mover_loop)
            .map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("window-drag-hook".into())
            .spawn(|| unsafe {
                // hMod must reference the module containing the hook proc; our
                // Rust is linked into the process's main module.
                let hmod = GetModuleHandleW(None).unwrap_or_default();
                let hook = match SetWindowsHookExW(
                    WH_MOUSE_LL,
                    Some(mouse_proc),
                    HINSTANCE(hmod.0),
                    0,
                ) {
                    Ok(h) => h,
                    Err(e) => {
                        eprintln!("window_drag: SetWindowsHookExW failed: {e}");
                        return;
                    }
                };
                HOOK.store(hook.0 as isize, Ordering::Relaxed);
                HOOK_THREAD.store(GetCurrentThreadId(), Ordering::Relaxed);

                // The hover indicator lives on this thread so its timer + paint
                // messages ride the same pump the hook needs.
                create_indicator(HINSTANCE(hmod.0));

                // Message pump: LL hooks are delivered to the thread that
                // installed them, so this thread must run one; the indicator
                // window's WM_TIMER/paint messages need dispatching too.
                // disable() breaks the loop by posting WM_QUIT.
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                destroy_indicator();
                let _ = UnhookWindowsHookEx(hook);
                HOOK.store(0, Ordering::Relaxed);
                HOOK_THREAD.store(0, Ordering::Relaxed);
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn disable() -> Result<(), String> {
        MOVER_RUN.store(false, Ordering::Relaxed);
        if let Ok(mut st) = state().lock() {
            st.active = false;
        }
        ACTIVE.store(false, Ordering::Relaxed);
        let tid = HOOK_THREAD.load(Ordering::Relaxed);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        Ok(())
    }

    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && handle(wparam.0 as u32, lparam) {
            // Swallow the event so the underlying app never sees the Alt+click.
            return LRESULT(1);
        }
        CallNextHookEx(HHOOK::default(), code, wparam, lparam)
    }

    /// Returns true if the event should be swallowed. CRITICAL: this runs inside
    /// the OS input pipeline, so it must be fast and must never perform window
    /// operations (`SetWindowPos`/`ShowWindow` block on the target app's thread
    /// and would stall system-wide input until the LL-hook timeout). It only
    /// records the drag; the mover thread does the actual moving.
    ///
    /// Equally CRITICAL: mouse-MOVES are never swallowed, only the grab's
    /// button events. Swallowing a WM_MOUSEMOVE at the LL-hook layer prevents
    /// the system from moving the physical cursor at all; with a real
    /// (relative-input) mouse the next packet's `pt` is then computed against
    /// the still-frozen cursor, so the reported position stays pinned within a
    /// few px of the grab point and snaps back to it when the hand stops — and
    /// the dragged window followed it back to its origin. See module docs.
    unsafe fn handle(msg: u32, lparam: LPARAM) -> bool {
        if msg == WM_MOUSEMOVE {
            // Fast path out: no lock, no work, and crucially NO swallowing.
            // The mover thread polls GetCursorPos itself.
            return false;
        }

        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let (px, py) = (info.pt.x, info.pt.y);
        let mut st = match state().lock() {
            Ok(g) => g,
            Err(_) => return false,
        };

        match msg {
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
                if st.active {
                    return true;
                }
                if !alt_down() {
                    return false;
                }
                let mode = if msg == WM_LBUTTONDOWN {
                    Mode::Move
                } else {
                    Mode::Resize
                };
                // Hide the hover overlay before hit-testing so WindowFromPoint
                // resolves the app under the cursor, not our click-through
                // window. It stays hidden for the duration of the drag.
                hide_indicator();
                if let Some((hwnd, rect, restore_max)) = begin_target(px, py) {
                    st.active = true;
                    st.mode = mode;
                    st.hwnd = hwnd.0 as isize;
                    st.start_x = px;
                    st.start_y = py;
                    st.rect = rect;
                    st.restore_max = restore_max;
                    st.snap = SnapKind::None;
                    st.neighbors.clear();
                    if mode == Mode::Resize {
                        // Decide how the resize behaves:
                        //  1. A half-screen snapped window resizes from its one
                        //     free edge and tiles along it.
                        //  2. Otherwise, if the edge nearest the cursor is shared
                        //     with flush neighbors, resize just that edge and tile
                        //     along it — so a stacked/side-by-side grid (e.g.
                        //     windows 2 and 3) redistributes space like Windows.
                        //  3. Failing both, a normal quadrant-corner resize.
                        // Maximized targets are restored first, so never tile.
                        let real_snap = if restore_max {
                            SnapKind::None
                        } else {
                            snap_kind(hwnd, &rect)
                        };
                        let (snap, raw) = if real_snap != SnapKind::None {
                            (real_snap, find_neighbors(real_snap, &rect, hwnd))
                        } else if !restore_max {
                            // Among the edges cleanly tiled with neighbors, resize
                            // the one nearest the cursor; the rest (screen borders,
                            // partially-shared edges) stay locked so the window's
                            // other dimensions and position are preserved.
                            let mut best: Option<(SnapKind, Vec<(HWND, RECT)>)> = None;
                            let mut best_dist = i32::MAX;
                            for e in [
                                SnapKind::Left,
                                SnapKind::Right,
                                SnapKind::Top,
                                SnapKind::Bottom,
                            ] {
                                if let Some(nb) = clean_tile_edge(e, &rect, hwnd) {
                                    let d = edge_dist(e, px, py, &rect);
                                    if d < best_dist {
                                        best_dist = d;
                                        best = Some((e, nb));
                                    }
                                }
                            }
                            match best {
                                Some((e, nb)) => (e, nb),
                                None => (SnapKind::None, Vec::new()),
                            }
                        } else {
                            (SnapKind::None, Vec::new())
                        };
                        st.snap = snap;
                        st.edges = if snap == SnapKind::None {
                            pick_edges(px, py, &rect)
                        } else {
                            snap.free_edge()
                        };
                        // Every window flush along the resized edge tiles with the
                        // target: shrinking it grows all of them. Push each
                        // neighbor's facing edge past the boundary by half the
                        // combined invisible border to halve the visible gap.
                        if snap != SnapKind::None {
                            let ti = border_insets(hwnd, &rect);
                            for (nh, nrect) in raw {
                                let ni = border_insets(nh, &nrect);
                                // Push the neighbor's facing edge 3/4 of the way
                                // across the combined invisible border, leaving a
                                // visible gap of 1/4 of it — half of the previous
                                // 1/2 (i.e. the gap halved once more).
                                let overlap = match snap {
                                    SnapKind::Right => (ti.left + ni.right) * 3 / 4,
                                    SnapKind::Left => (ti.right + ni.left) * 3 / 4,
                                    SnapKind::Top => (ti.bottom + ni.top) * 3 / 4,
                                    SnapKind::Bottom => (ti.top + ni.bottom) * 3 / 4,
                                    SnapKind::None => 0,
                                };
                                st.neighbors.push(Neighbor {
                                    hwnd: nh.0 as isize,
                                    rect: nrect,
                                    overlap,
                                });
                            }
                        }
                    } else {
                        // Move: capture the border insets so an edge-snap on
                        // release lines the visible frame up with the work area.
                        st.border = border_insets(hwnd, &rect);
                    }
                    GEN.fetch_add(1, Ordering::Relaxed);
                    ACTIVE.store(true, Ordering::Relaxed);
                    return true;
                }
                false
            }
            WM_LBUTTONUP => {
                if st.active && st.mode == Mode::Move {
                    st.active = false;
                    ACTIVE.store(false, Ordering::Relaxed);
                    suppress_alt_menu();
                    return true;
                }
                false
            }
            WM_RBUTTONUP => {
                if st.active && st.mode == Mode::Resize {
                    st.active = false;
                    ACTIVE.store(false, Ordering::Relaxed);
                    suppress_alt_menu();
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    unsafe fn alt_down() -> bool {
        (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0
    }

    /// Resolve the top-level window under the cursor, rejecting our own windows
    /// and the shell/desktop. Returns the rect the drag deltas are measured
    /// against plus whether the window is currently maximized — the mover
    /// thread restores + recenters it on its first frame, because
    /// ShowWindow/SetWindowPos block on the target app's thread and must never
    /// run inside the hook callback (LL-hook timeout would silently disable the
    /// whole hook). Only cheap, non-blocking reads happen here.
    unsafe fn begin_target(px: i32, py: i32) -> Option<(HWND, RECT, bool)> {
        let root = GetAncestor(WindowFromPoint(POINT { x: px, y: py }), GA_ROOT);
        if !is_draggable_root(root) {
            return None;
        }
        let mut rect = RECT::default();
        if GetWindowRect(root, &mut rect).is_err() {
            return None;
        }
        Some((root, rect, IsZoomed(root).as_bool()))
    }

    /// A top-level window we're allowed to move/resize: visible, not one of our
    /// own windows (palette / screenshot / the hover indicator), and not the
    /// desktop, shell, or a task switcher (see `is_shell_class`).
    unsafe fn is_draggable_root(root: HWND) -> bool {
        if root.0.is_null() || !IsWindowVisible(root).as_bool() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(root, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            return false;
        }
        !(root == GetDesktopWindow()
            || root == GetShellWindow()
            || is_shell_class(root)
            || is_fullscreen_window(root))
    }

    /// A borderless / exclusive window that covers its whole monitor — the shape
    /// a game takes in fullscreen or borderless mode. We keep hands off these so
    /// an Alt-drag never accidentally moves or resizes a running game (League,
    /// etc.). A normal maximized window is explicitly excluded: it's `IsZoomed`,
    /// keeps its caption + resize frame, and only fills the work area (not the
    /// taskbar), so it stays draggable.
    unsafe fn is_fullscreen_window(hwnd: HWND) -> bool {
        if IsZoomed(hwnd).as_bool() {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return false;
        }
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(hmon, &mut mi).as_bool() {
            return false;
        }
        // Covers the entire monitor, taskbar included (a maximized window only
        // reaches the work area, and its invisible borders make it IsZoomed).
        const TOL: i32 = 2;
        let m = mi.rcMonitor;
        let covers = rect.left <= m.left + TOL
            && rect.top <= m.top + TOL
            && rect.right >= m.right - TOL
            && rect.bottom >= m.bottom - TOL;
        if !covers {
            return false;
        }
        // ...and is borderless — no caption bar and no resize frame. A normal
        // window this large still carries both, so this only catches games.
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let has_caption = style & WS_CAPTION.0 == WS_CAPTION.0;
        let has_thickframe = style & WS_THICKFRAME.0 != 0;
        !(has_caption || has_thickframe)
    }

    /// Classify a window as snapped to a screen edge: it fills the work area in
    /// one dimension and sits against exactly one wall in the other. Used both
    /// to constrain resizing (only the free edge moves) and to draw the hover
    /// indicator as halves rather than quarters.
    /// The work area (screen minus taskbar) of the monitor the window is on.
    // Force `hw` to the front of the Z-order (and give it focus). Called once
    // per grab from the mover thread. A cross-process SetWindowPos(HWND_TOP) is
    // ignored by the foreground/Z-order lock, so temporarily attach our input
    // thread to whoever currently owns the foreground — that lets
    // SetForegroundWindow/BringWindowToTop actually take effect — then detach.
    unsafe fn raise_window(hw: HWND) {
        let fg = GetForegroundWindow();
        if !fg.0.is_null() && fg.0 == hw.0 {
            return;
        }
        let our = GetCurrentThreadId();
        let fg_thread = if fg.0.is_null() {
            0
        } else {
            GetWindowThreadProcessId(fg, None)
        };
        let attached = fg_thread != 0
            && fg_thread != our
            && AttachThreadInput(fg_thread, our, BOOL(1)).as_bool();
        let _ = BringWindowToTop(hw);
        let _ = SetForegroundWindow(hw);
        let _ = SetWindowPos(hw, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        if attached {
            let _ = AttachThreadInput(fg_thread, our, BOOL(0));
        }
    }

    unsafe fn work_area(hwnd: HWND) -> Option<RECT> {
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            Some(mi.rcWork)
        } else {
            None
        }
    }

    unsafe fn snap_kind(hwnd: HWND, rect: &RECT) -> SnapKind {
        let Some(work) = work_area(hwnd) else {
            return SnapKind::None;
        };
        // Snapped windows align to the work area; allow a few px for the
        // invisible resize border GetWindowRect includes on Win10/11.
        const TOL: i32 = 10;
        let near = |a: i32, b: i32| (a - b).abs() <= TOL;
        let full_w = near(rect.left, work.left) && near(rect.right, work.right);
        let full_h = near(rect.top, work.top) && near(rect.bottom, work.bottom);
        if full_h && !full_w {
            if near(rect.left, work.left) {
                SnapKind::Left
            } else if near(rect.right, work.right) {
                SnapKind::Right
            } else {
                SnapKind::None
            }
        } else if full_w && !full_h {
            if near(rect.top, work.top) {
                SnapKind::Top
            } else if near(rect.bottom, work.bottom) {
                SnapKind::Bottom
            } else {
                SnapKind::None
            }
        } else {
            SnapKind::None
        }
    }

    /// Find a window snapped flush against the resized window's free edge, so a
    /// resize can move both edges together (tiling). Probes just past the free
    /// edge, at the mid-point of the shared side, and accepts the window there
    /// if its facing edge sits on the boundary.
    /// Every distinct window snapped flush along the target's free edge. The
    /// free edge is sampled at many points just outside it (not one midpoint):
    /// a full-height window beside a stack of two shorter ones borders *both*,
    /// and both must tile. `WindowFromPoint` respects Z-order, so only the
    /// windows actually visible against the edge are picked up. Deduped by HWND.
    unsafe fn find_neighbors(snap: SnapKind, rect: &RECT, self_hwnd: HWND) -> Vec<(HWND, RECT)> {
        const PROBE: i32 = 8;
        // GetWindowRect includes the ~7px invisible border on both windows, so
        // two flush windows' facing edges can differ by ~2 borders.
        const TOL: i32 = 20;
        // Evenly spaced samples along the edge; even a MIN_SIZE-wide cell on a
        // wide monitor lands on at least one. Inset a little from the corners so
        // a probe never straddles a perpendicular neighbor.
        const SAMPLES: i32 = 48;
        const INSET: i32 = 6;
        let mut out: Vec<(HWND, RECT)> = Vec::new();
        if snap == SnapKind::None {
            return out;
        }
        // (start, end) is the range swept along the edge; the axis is x for the
        // Top/Bottom free edges and y for Left/Right.
        let along_x = matches!(snap, SnapKind::Top | SnapKind::Bottom);
        let (start, end) = if along_x {
            (rect.left + INSET, rect.right - INSET)
        } else {
            (rect.top + INSET, rect.bottom - INSET)
        };
        if end <= start {
            return out;
        }
        for i in 0..=SAMPLES {
            let t = start + (end - start) * i / SAMPLES;
            let probe = match snap {
                SnapKind::Right => POINT { x: rect.left - PROBE, y: t },
                SnapKind::Left => POINT { x: rect.right + PROBE, y: t },
                SnapKind::Top => POINT { x: t, y: rect.bottom + PROBE },
                SnapKind::Bottom => POINT { x: t, y: rect.top - PROBE },
                SnapKind::None => continue,
            };
            let root = GetAncestor(WindowFromPoint(probe), GA_ROOT);
            if root == self_hwnd || !is_draggable_root(root) {
                continue;
            }
            if out.iter().any(|(h, _)| *h == root) {
                continue;
            }
            let mut nr = RECT::default();
            if GetWindowRect(root, &mut nr).is_err() {
                continue;
            }
            let flush = match snap {
                SnapKind::Right => (nr.right - rect.left).abs() <= TOL,
                SnapKind::Left => (nr.left - rect.right).abs() <= TOL,
                SnapKind::Top => (nr.top - rect.bottom).abs() <= TOL,
                SnapKind::Bottom => (nr.bottom - rect.top).abs() <= TOL,
                SnapKind::None => false,
            };
            if flush {
                out.push((root, nr));
            }
        }
        out
    }

    /// The (possibly clamped) target edges `(l, t, r, b)` plus each neighbor's
    /// `(hwnd, rect)` to apply.
    type NeighborPlan = ((i32, i32, i32, i32), Vec<(isize, RECT)>);

    /// Given the target's resized edges `(tl, tt, tr, tb)`, move every neighbor's
    /// facing edge to the shared boundary (keeping their other three edges), and
    /// clamp the boundary so neither the target nor *any* neighbor drops below
    /// `MIN_SIZE`. Returns the (possibly clamped) target edges plus each
    /// neighbor's `(hwnd, rect)` to apply.
    fn coordinate_neighbors(
        snap: SnapKind,
        neighbors: &[Neighbor],
        tl: i32,
        tt: i32,
        tr: i32,
        tb: i32,
    ) -> NeighborPlan {
        // The target's free edge sits at the shared boundary `b`; each neighbor's
        // facing edge is pushed its own `overlap` past it (into the invisible
        // border) so the visible gap is halved. Use `.max(lo).min(hi)` rather
        // than `.clamp(lo, hi)`: if the combined span is too small the bounds
        // cross and `clamp` would panic — this degrades gracefully instead. The
        // boundary is clamped against the *tightest* neighbor so none collapses.
        match snap {
            // Target free edge = left; neighbors are the windows on the left
            // (facing edge = their right). Boundary = shared vertical line.
            SnapKind::Right => {
                let lo = neighbors
                    .iter()
                    .map(|n| n.rect.left + MIN_SIZE)
                    .max()
                    .unwrap_or(i32::MIN);
                let b = tl.max(lo).min(tr - MIN_SIZE);
                let nrects = neighbors
                    .iter()
                    .map(|n| {
                        (
                            n.hwnd,
                            RECT {
                                left: n.rect.left,
                                top: n.rect.top,
                                right: b + n.overlap,
                                bottom: n.rect.bottom,
                            },
                        )
                    })
                    .collect();
                ((b, tt, tr, tb), nrects)
            }
            // Free edge = right; neighbors on the right (facing = their left).
            SnapKind::Left => {
                let hi = neighbors
                    .iter()
                    .map(|n| n.rect.right - MIN_SIZE)
                    .min()
                    .unwrap_or(i32::MAX);
                let b = tr.max(tl + MIN_SIZE).min(hi);
                let nrects = neighbors
                    .iter()
                    .map(|n| {
                        (
                            n.hwnd,
                            RECT {
                                left: b - n.overlap,
                                top: n.rect.top,
                                right: n.rect.right,
                                bottom: n.rect.bottom,
                            },
                        )
                    })
                    .collect();
                ((tl, tt, b, tb), nrects)
            }
            // Free edge = bottom; neighbors below (facing = their top).
            SnapKind::Top => {
                let hi = neighbors
                    .iter()
                    .map(|n| n.rect.bottom - MIN_SIZE)
                    .min()
                    .unwrap_or(i32::MAX);
                let b = tb.max(tt + MIN_SIZE).min(hi);
                let nrects = neighbors
                    .iter()
                    .map(|n| {
                        (
                            n.hwnd,
                            RECT {
                                left: n.rect.left,
                                top: b - n.overlap,
                                right: n.rect.right,
                                bottom: n.rect.bottom,
                            },
                        )
                    })
                    .collect();
                ((tl, tt, tr, b), nrects)
            }
            // Free edge = top; neighbors above (facing = their bottom).
            SnapKind::Bottom => {
                let lo = neighbors
                    .iter()
                    .map(|n| n.rect.top + MIN_SIZE)
                    .max()
                    .unwrap_or(i32::MIN);
                let b = tt.max(lo).min(tb - MIN_SIZE);
                let nrects = neighbors
                    .iter()
                    .map(|n| {
                        (
                            n.hwnd,
                            RECT {
                                left: n.rect.left,
                                top: n.rect.top,
                                right: n.rect.right,
                                bottom: b + n.overlap,
                            },
                        )
                    })
                    .collect();
                ((tl, b, tr, tb), nrects)
            }
            SnapKind::None => ((tl, tt, tr, tb), Vec::new()),
        }
    }

    /// The window's invisible-border insets (visible frame minus window rect):
    /// ~7px on the left/right/bottom, ~0 on top for a normal Win10/11 window.
    /// A window snapped flush to the screen extends its rect into these, so
    /// snapping must offset by them to line the *visible* edge up with the work
    /// area.
    unsafe fn border_insets(hwnd: HWND, rect: &RECT) -> RECT {
        let vb = visible_bounds(hwnd).unwrap_or(*rect);
        RECT {
            left: vb.left - rect.left,
            top: vb.top - rect.top,
            right: rect.right - vb.right,
            bottom: rect.bottom - vb.bottom,
        }
    }

    /// A screen-edge snap target for a *moved* window, mirroring Aero Snap:
    /// left/right halves, top = maximize, and the four quarter corners — all
    /// exact even splits of the work area.
    #[derive(Clone, Copy, PartialEq)]
    enum SnapZone {
        Left,
        Right,
        Maximize,
        TopLeft,
        TopRight,
        BottomLeft,
        BottomRight,
    }

    /// The work area of the monitor under a screen point (the cursor's monitor,
    /// so snapping follows the pointer across displays).
    unsafe fn work_area_at(p: POINT) -> Option<RECT> {
        let hmon = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            Some(mi.rcWork)
        } else {
            None
        }
    }

    /// Which snap zone the cursor is in while moving a window, or None — same
    /// regions as dragging a title bar in Windows: side edges give halves,
    /// corners give quarters, the top edge maximizes.
    fn snap_zone(p: POINT, work: RECT) -> Option<SnapZone> {
        // Trigger band along each screen edge. Generous so a fast drag lands in
        // it well before the cursor reaches the edge — on multi-monitor setups
        // that means you can snap without slowing down to avoid overshooting
        // onto the next display.
        const EDGE: i32 = 160;
        const CORNER: i32 = 100; // how far along an edge still counts as a corner
        let near_left = p.x <= work.left + EDGE;
        let near_right = p.x >= work.right - EDGE;
        let near_top = p.y <= work.top + EDGE;
        if near_left {
            if p.y <= work.top + CORNER {
                Some(SnapZone::TopLeft)
            } else if p.y >= work.bottom - CORNER {
                Some(SnapZone::BottomLeft)
            } else {
                Some(SnapZone::Left)
            }
        } else if near_right {
            if p.y <= work.top + CORNER {
                Some(SnapZone::TopRight)
            } else if p.y >= work.bottom - CORNER {
                Some(SnapZone::BottomRight)
            } else {
                Some(SnapZone::Right)
            }
        } else if near_top {
            Some(SnapZone::Maximize)
        } else {
            None
        }
    }

    /// The interior vertical boundary a half-snap should fill up to, if a window
    /// is already snapped against the opposite wall — Windows-style: snapping
    /// left when the right half is occupied fills the *remaining* space (up to
    /// that window's visible edge) instead of a fixed 50%. `None` (no opposite
    /// snapped window) falls back to the work-area midpoint. Probes 3/4 of the
    /// way across, so a half-height quarter won't be mistaken for a full column.
    unsafe fn snap_fill_x(zone: SnapZone, work: RECT, dragged: HWND) -> Option<i32> {
        let w = work.right - work.left;
        let (probe_x, want) = match zone {
            SnapZone::Left => (work.left + w * 3 / 4, SnapKind::Right),
            SnapZone::Right => (work.left + w / 4, SnapKind::Left),
            _ => return None,
        };
        let probe = POINT {
            x: probe_x,
            y: (work.top + work.bottom) / 2,
        };
        let root = GetAncestor(WindowFromPoint(probe), GA_ROOT);
        if root == dragged || !is_draggable_root(root) {
            return None;
        }
        let mut wr = RECT::default();
        if GetWindowRect(root, &mut wr).is_err() || snap_kind(root, &wr) != want {
            return None;
        }
        // Fill up to the occupant's *visible* facing edge so the two touch flush.
        let vb = visible_bounds(root).unwrap_or(wr);
        Some(match want {
            SnapKind::Right => vb.left, // occupant on the right; fill our right edge to its left
            _ => vb.right,             // occupant on the left; fill our left edge to its right
        })
    }

    /// The target window rect for a snap zone — a half / quarter / full split of
    /// the work area, offset by the window's invisible border so the visible
    /// edges line up. `fill_x` overrides the interior vertical boundary of a
    /// half-snap so it fills the space beside an already-snapped window. The
    /// bool marks a full-screen (maximize) target (drawn with square corners);
    /// `Maximize` uses the OS maximize on commit, this rect is only for preview.
    fn zone_rect(zone: SnapZone, work: RECT, border: RECT, fill_x: Option<i32>) -> (RECT, bool) {
        let midx = (work.left + work.right) / 2;
        let midy = (work.top + work.bottom) / 2;
        // For a left/right half, the interior edge fills up to an existing
        // neighbor when there is one; otherwise it's the midpoint.
        let bound = fill_x.unwrap_or(midx);
        let (bl, bt, br, bb) = (border.left, border.top, border.right, border.bottom);
        let mk = |l: i32, t: i32, r: i32, b: i32| RECT {
            left: l,
            top: t,
            right: r,
            bottom: b,
        };
        match zone {
            SnapZone::Left => (
                mk(work.left - bl, work.top - bt, bound + br, work.bottom + bb),
                false,
            ),
            SnapZone::Right => (
                mk(bound - bl, work.top - bt, work.right + br, work.bottom + bb),
                false,
            ),
            SnapZone::Maximize => (mk(work.left, work.top, work.right, work.bottom), true),
            SnapZone::TopLeft => (
                mk(work.left - bl, work.top - bt, midx + br, midy + bb),
                false,
            ),
            SnapZone::TopRight => (
                mk(midx - bl, work.top - bt, work.right + br, midy + bb),
                false,
            ),
            SnapZone::BottomLeft => (
                mk(work.left - bl, midy - bt, midx + br, work.bottom + bb),
                false,
            ),
            SnapZone::BottomRight => (
                mk(midx - bl, midy - bt, work.right + br, work.bottom + bb),
                false,
            ),
        }
    }

    unsafe fn is_shell_class(hwnd: HWND) -> bool {
        let mut buf = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut buf);
        if n <= 0 {
            return false;
        }
        let name = String::from_utf16_lossy(&buf[..n as usize]);
        matches!(
            name.as_str(),
            "Progman"
                | "WorkerW"
                | "Shell_TrayWnd"
                | "Shell_SecondaryTrayWnd"
                | "Windows.UI.Core.CoreWindow"
                // Task switchers / shell overlays: these are hosted by
                // explorer.exe but are NOT File Explorer windows (those are
                // "CabinetWClass"), so blacklisting by class leaves Explorer
                // draggable while excluding the Alt+Tab / Task View UI. Without
                // this, Alt held for Alt+Tab makes every click an Alt-drag (so
                // clicks never reach the switcher) and Alt+right-drag resizes
                // the switcher window itself.
                | "XamlExplorerHostIslandWindow" // Win11 Alt+Tab / Task View / Snap
                | "MultitaskingViewFrame"        // Task View frame
                | "TaskSwitcherWnd"              // legacy Alt+Tab
        )
    }

    // ---- Snap-aware hover indicator ---------------------------------------
    //
    // While Alt is held over a draggable window (and no drag is in progress),
    // the window is dimmed with a translucent black overlay; the region a resize
    // would grab is dimmed *less* so it stands out. A normal window shows four
    // quadrants (the hovered corner is lightest); a snapped window shows two
    // halves (the free-edge side is lightest), since it only resizes along one
    // axis.
    //
    // It's a single layered, click-through Win32 window owned by the hook thread
    // (which already pumps messages). A window timer repaints it at ~60 Hz via
    // UpdateLayeredWindow with a per-pixel-alpha bitmap, so it can (a) match the
    // window's DWM visible frame exactly instead of GetWindowRect's oversized
    // bounds and (b) round its corners to sit flush. It never touches the drag
    // hot path: hidden the instant a grab starts and while ACTIVE.
    static IND_HWND: AtomicIsize = AtomicIsize::new(0);
    static IND_VISIBLE: AtomicBool = AtomicBool::new(false);
    // Last window the indicator locked onto — reused when the cursor sits over
    // the overlay itself (WindowFromPoint would otherwise return our own
    // click-through window instead of the app beneath it).
    static IND_TARGET: AtomicIsize = AtomicIsize::new(0);
    // Last painted frame (visible-bounds rect + active-region code), so we only
    // rebuild the bitmap when something actually changes.
    static IND_LL: AtomicI32 = AtomicI32::new(i32::MIN);
    static IND_LT: AtomicI32 = AtomicI32::new(0);
    static IND_LR: AtomicI32 = AtomicI32::new(0);
    static IND_LB: AtomicI32 = AtomicI32::new(0);
    static IND_LACT: AtomicI32 = AtomicI32::new(i32::MIN);

    const IND_TIMER: usize = 1;
    // Black-overlay opacity: the active region is barely dimmed, the rest are
    // dimmed enough to make it pop without hiding the window's contents.
    const DIM_ACTIVE: u8 = 26; // ~10%
    const DIM_INACTIVE: u8 = 84; // ~33%
    const PREVIEW_ALPHA: u8 = 110; // move-snap preview: uniform, clearly visible
    const CORNER_RADIUS: i32 = 8; // ~matches the Win11 rounded-corner radius

    unsafe extern "system" fn indicator_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_TIMER {
            update_indicator();
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    unsafe fn create_indicator(hinst: HINSTANCE) {
        let class = w!("CommandeerDragQuadrant");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(indicator_proc),
            hInstance: hinst,
            lpszClassName: class,
            ..Default::default()
        };
        // Ignore the result: re-enabling registers the same class again, which
        // fails with ERROR_CLASS_ALREADY_EXISTS but leaves it usable.
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            class,
            w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            hinst,
            None,
        );
        let Ok(hwnd) = hwnd else { return };
        IND_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
        IND_LL.store(i32::MIN, Ordering::Relaxed);
        IND_LACT.store(i32::MIN, Ordering::Relaxed);
        SetTimer(hwnd, IND_TIMER, 16, None);
    }

    unsafe fn destroy_indicator() {
        IND_VISIBLE.store(false, Ordering::Relaxed);
        IND_TARGET.store(0, Ordering::Relaxed);
        let hwnd = HWND(IND_HWND.swap(0, Ordering::Relaxed) as *mut _);
        if !hwnd.0.is_null() {
            let _ = DestroyWindow(hwnd);
        }
    }

    unsafe fn hide_indicator() {
        if !IND_VISIBLE.swap(false, Ordering::Relaxed) {
            return;
        }
        let hwnd = HWND(IND_HWND.load(Ordering::Relaxed) as *mut _);
        if !hwnd.0.is_null() {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }

    /// The window's visible frame in screen coordinates. GetWindowRect includes
    /// the invisible resize border on Win10/11 (so an overlay sized to it spills
    /// past the window); the DWM extended-frame-bounds is the true visible rect.
    unsafe fn visible_bounds(hwnd: HWND) -> Option<RECT> {
        let mut r = RECT::default();
        let dwm_ok = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut RECT as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_ok();
        // Fall back to GetWindowRect (which refills `r`) when the DWM bounds
        // are unavailable or degenerate.
        if (dwm_ok && r.right > r.left && r.bottom > r.top) || GetWindowRect(hwnd, &mut r).is_ok()
        {
            Some(r)
        } else {
            None
        }
    }

    /// Timer-driven (hook thread): dim the window under the cursor with the
    /// resize region highlighted, or hide when Alt is up / a drag is running /
    /// the cursor isn't over a draggable window.
    unsafe fn update_indicator() {
        if IND_HWND.load(Ordering::Relaxed) == 0 {
            return;
        }
        if !alt_down() {
            hide_indicator();
            return;
        }
        // During a move drag, show the Aero-Snap-style preview instead; during a
        // resize drag, show nothing.
        if ACTIVE.load(Ordering::Relaxed) {
            move_snap_preview();
            return;
        }
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_err() {
            hide_indicator();
            return;
        }
        let root = GetAncestor(WindowFromPoint(p), GA_ROOT);
        let mut pid = 0u32;
        GetWindowThreadProcessId(root, Some(&mut pid));
        // If the cursor is over our own window, it's the overlay itself — fall
        // back to the window we last locked onto (still under it).
        let target = if pid == GetCurrentProcessId() {
            let cached = HWND(IND_TARGET.load(Ordering::Relaxed) as *mut _);
            if cached.0.is_null() || !IsWindowVisible(cached).as_bool() {
                hide_indicator();
                return;
            }
            cached
        } else if is_draggable_root(root) {
            IND_TARGET.store(root.0 as isize, Ordering::Relaxed);
            root
        } else {
            IND_TARGET.store(0, Ordering::Relaxed);
            hide_indicator();
            return;
        };

        // Edge selection uses GetWindowRect (what pick_edges/snap_kind see at
        // grab); the overlay itself is drawn against the DWM visible bounds.
        let mut wr = RECT::default();
        let Some(vb) = visible_bounds(target) else {
            hide_indicator();
            return;
        };
        if GetWindowRect(target, &mut wr).is_err() || wr.right - wr.left < 2 || wr.bottom - wr.top < 2
        {
            hide_indicator();
            return;
        }

        // A window whose resize is locked to a single shared divider has exactly
        // one possible drag, so the overlay would be noise — don't show it. That
        // covers both a half-snapped window tiled against a neighbor and a
        // quarter-tiled window (e.g. windows 2/3) with one cleanly-tiled edge.
        let sk = snap_kind(target, &wr);
        let tiled = if sk != SnapKind::None {
            !find_neighbors(sk, &wr, target).is_empty()
        } else {
            [
                SnapKind::Left,
                SnapKind::Right,
                SnapKind::Top,
                SnapKind::Bottom,
            ]
            .into_iter()
            .any(|e| clean_tile_edge(e, &wr, target).is_some())
        };
        if tiled {
            hide_indicator();
            return;
        }

        // Active region as (horizontal, vertical) sides: 0 = min-edge (left /
        // top) half, 1 = max-edge (right / bottom) half, 2 = the whole span. A
        // normal window keys off the cursor's quadrant; a snapped window
        // highlights the half on its free-edge side.
        let (ah, av) = match sk {
            SnapKind::None => {
                let midx = wr.left + (wr.right - wr.left) / 2;
                let midy = wr.top + (wr.bottom - wr.top) / 2;
                (
                    if p.x < midx { 0 } else { 1 },
                    if p.y < midy { 0 } else { 1 },
                )
            }
            SnapKind::Left => (1, 2),   // free edge = right
            SnapKind::Right => (0, 2),  // free edge = left
            SnapKind::Top => (2, 1),    // free edge = bottom
            SnapKind::Bottom => (2, 0), // free edge = top
        };
        paint_indicator(
            vb,
            ah,
            av,
            IsZoomed(target).as_bool(),
            DIM_ACTIVE,
            DIM_INACTIVE,
        );
    }

    /// While moving a window, preview the Aero-Snap zone under the cursor as a
    /// uniform translucent fill of the target half / quarter / full-screen, or
    /// hide it when the cursor isn't in a zone (or the drag is a resize).
    unsafe fn move_snap_preview() {
        let (mode, border, dragged) = match state().lock() {
            Ok(st) => (st.mode, st.border, HWND(st.hwnd as *mut _)),
            Err(_) => {
                hide_indicator();
                return;
            }
        };
        if mode != Mode::Move {
            hide_indicator();
            return;
        }
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_err() {
            hide_indicator();
            return;
        }
        let Some(work) = work_area_at(p) else {
            hide_indicator();
            return;
        };
        let Some(zone) = snap_zone(p, work) else {
            hide_indicator();
            return;
        };
        let fill_x = snap_fill_x(zone, work, dragged);
        let (rect, maximized) = zone_rect(zone, work, border, fill_x);
        paint_indicator(rect, 2, 2, maximized, PREVIEW_ALPHA, PREVIEW_ALPHA);
    }

    /// Rebuild the overlay bitmap for the visible bounds `vb`, filling the four
    /// quadrants — the active region (matching `ah`/`av`, where 2 means "either
    /// side") gets `active_alpha`, the rest `inactive_alpha` — with rounded
    /// outer corners. Only repaints when the bounds or active region changed.
    unsafe fn paint_indicator(
        vb: RECT,
        ah: i32,
        av: i32,
        maximized: bool,
        active_alpha: u8,
        inactive_alpha: u8,
    ) {
        let hwnd = HWND(IND_HWND.load(Ordering::Relaxed) as *mut _);
        if hwnd.0.is_null() {
            return;
        }
        let w = vb.right - vb.left;
        let h = vb.bottom - vb.top;
        if w < 2 || h < 2 {
            hide_indicator();
            return;
        }
        let act_code = ah * 4 + av;
        let changed = IND_LL.load(Ordering::Relaxed) != vb.left
            || IND_LT.load(Ordering::Relaxed) != vb.top
            || IND_LR.load(Ordering::Relaxed) != vb.right
            || IND_LB.load(Ordering::Relaxed) != vb.bottom
            || IND_LACT.load(Ordering::Relaxed) != act_code;

        if changed {
            // Top-down 32bpp premultiplied BGRA. The dim is pure black, so
            // premultiplied color channels are 0 and every pixel is just
            // (alpha << 24); transparent outside the rounded corners.
            let radius = if maximized { 0 } else { CORNER_RADIUS };
            let midx = w / 2;
            let midy = h / 2;
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // negative = top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let screen_dc = GetDC(None);
            let mem_dc = CreateCompatibleDC(screen_dc);
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            if let Ok(dib) = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                if !bits.is_null() {
                    let px = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
                    for y in 0..h {
                        let qv = if y < midy { 0 } else { 1 };
                        let v_active = av == 2 || av == qv;
                        for x in 0..w {
                            let alpha = if rounded_out(x, y, w, h, radius) {
                                0u32
                            } else {
                                let qh = if x < midx { 0 } else { 1 };
                                if v_active && (ah == 2 || ah == qh) {
                                    active_alpha as u32
                                } else {
                                    inactive_alpha as u32
                                }
                            };
                            px[(y * w + x) as usize] = alpha << 24;
                        }
                    }
                    let old = SelectObject(mem_dc, HGDIOBJ(dib.0));
                    let pt_dst = POINT {
                        x: vb.left,
                        y: vb.top,
                    };
                    let sz = SIZE { cx: w, cy: h };
                    let pt_src = POINT { x: 0, y: 0 };
                    let blend = BLENDFUNCTION {
                        BlendOp: 0, // AC_SRC_OVER
                        BlendFlags: 0,
                        SourceConstantAlpha: 255,
                        AlphaFormat: 1, // AC_SRC_ALPHA (premultiplied)
                    };
                    let _ = UpdateLayeredWindow(
                        hwnd,
                        screen_dc,
                        Some(&pt_dst),
                        Some(&sz),
                        mem_dc,
                        Some(&pt_src),
                        COLORREF(0),
                        Some(&blend),
                        ULW_ALPHA,
                    );
                    SelectObject(mem_dc, old);
                }
                let _ = DeleteObject(HGDIOBJ(dib.0));
            }
            let _ = DeleteDC(mem_dc);
            ReleaseDC(None, screen_dc);

            IND_LL.store(vb.left, Ordering::Relaxed);
            IND_LT.store(vb.top, Ordering::Relaxed);
            IND_LR.store(vb.right, Ordering::Relaxed);
            IND_LB.store(vb.bottom, Ordering::Relaxed);
            IND_LACT.store(act_code, Ordering::Relaxed);
        }

        if !IND_VISIBLE.swap(true, Ordering::Relaxed) {
            let _ = ShowWindow(hwnd, SW_SHOWNA);
        }
    }

    /// True if pixel (x, y) falls outside the window's rounded corner of radius
    /// `r` — so it's painted fully transparent and the overlay's corners match
    /// the window's.
    fn rounded_out(x: i32, y: i32, w: i32, h: i32, r: i32) -> bool {
        if r <= 0 {
            return false;
        }
        let cx = if x < r {
            r
        } else if x >= w - r {
            w - 1 - r
        } else {
            return false;
        };
        let cy = if y < r {
            r
        } else if y >= h - r {
            h - 1 - r
        } else {
            return false;
        };
        let dx = x - cx;
        let dy = y - cy;
        dx * dx + dy * dy > r * r
    }

    fn pick_edges(px: i32, py: i32, r: &RECT) -> Edges {
        let w = (r.right - r.left).max(1);
        let h = (r.bottom - r.top).max(1);
        let rx = px - r.left;
        let ry = py - r.top;
        // 2x2 quadrants: the grabbed corner follows the cursor. Left half drives
        // the left edge, right half the right edge; top half the top edge,
        // bottom half the bottom edge. Every grab resolves to exactly one
        // horizontal and one vertical edge, so there's no dead center.
        let mut e = Edges::default();
        if rx < w / 2 {
            e.left = true;
        } else {
            e.right = true;
        }
        if ry < h / 2 {
            e.top = true;
        } else {
            e.bottom = true;
        }
        e
    }

    /// Distance from the cursor to the edge whose free edge is `e` — used to
    /// pick which of a window's tileable edges a resize grabs.
    fn edge_dist(e: SnapKind, px: i32, py: i32, r: &RECT) -> i32 {
        match e {
            SnapKind::Right => px - r.left,  // free edge = left
            SnapKind::Left => r.right - px,  // free edge = right
            SnapKind::Bottom => py - r.top,  // free edge = top
            SnapKind::Top => r.bottom - py,  // free edge = bottom
            SnapKind::None => i32::MAX,
        }
    }

    /// If the window's edge (named by the virtual `SnapKind` whose free edge is
    /// that edge) is shared with flush neighbors that *cleanly* tile it — they
    /// span the whole edge and none overhangs past it — return those neighbors.
    ///
    /// This is what keeps a quarter-tiled window's width fixed: window 2's bottom
    /// edge is cleanly tiled by window 3 (same width, aligned), so it resizes;
    /// but its left edge is shared with the full-height window 1, which overhangs
    /// below window 2, so that edge stays locked (moving it would misalign
    /// window 3). Screen-border edges have no neighbor and are never resizable.
    unsafe fn clean_tile_edge(
        e: SnapKind,
        rect: &RECT,
        self_hwnd: HWND,
    ) -> Option<Vec<(HWND, RECT)>> {
        const TOL: i32 = 24;
        let nb = find_neighbors(e, rect, self_hwnd);
        if nb.is_empty() {
            return None;
        }
        // The edge runs along the perpendicular axis: x for a Top/Bottom free
        // edge, y for Left/Right.
        let horizontal = matches!(e, SnapKind::Top | SnapKind::Bottom);
        let (w_start, w_end) = if horizontal {
            (rect.left, rect.right)
        } else {
            (rect.top, rect.bottom)
        };
        let mut cover_start = i32::MAX;
        let mut cover_end = i32::MIN;
        for (_, nr) in &nb {
            let (n_start, n_end) = if horizontal {
                (nr.left, nr.right)
            } else {
                (nr.top, nr.bottom)
            };
            // A neighbor that spills past our edge is shared with other windows
            // too (a full-height window beside a half-height one) — moving it
            // would misalign them, so this edge isn't cleanly tileable.
            if n_start < w_start - TOL || n_end > w_end + TOL {
                return None;
            }
            cover_start = cover_start.min(n_start);
            cover_end = cover_end.max(n_end);
        }
        // The neighbors must span our whole edge, or resizing would leave a gap.
        if cover_start <= w_start + TOL && cover_end >= w_end - TOL {
            Some(nb)
        } else {
            None
        }
    }

    /// Resolve the target frame (x, y, w, h) for the current cursor position.
    fn compute_target(
        mode: Mode,
        sx: i32,
        sy: i32,
        r: RECT,
        edges: Edges,
        cx: i32,
        cy: i32,
    ) -> (i32, i32, i32, i32) {
        let dx = cx - sx;
        let dy = cy - sy;
        match mode {
            Mode::Move => (r.left + dx, r.top + dy, r.right - r.left, r.bottom - r.top),
            Mode::Resize => {
                let mut left = r.left;
                let mut top = r.top;
                let mut right = r.right;
                let mut bottom = r.bottom;
                // Only the selected edges move. For a snapped window the grab
                // sets `edges` to just the single free edge, so the snapped
                // dimension is left untouched automatically.
                if edges.left {
                    left = r.left + dx;
                }
                if edges.right {
                    right = r.right + dx;
                }
                if edges.top {
                    top = r.top + dy;
                }
                if edges.bottom {
                    bottom = r.bottom + dy;
                }
                if right - left < MIN_SIZE {
                    if edges.left {
                        left = right - MIN_SIZE;
                    } else {
                        right = left + MIN_SIZE;
                    }
                }
                if bottom - top < MIN_SIZE {
                    if edges.top {
                        top = bottom - MIN_SIZE;
                    } else {
                        bottom = top + MIN_SIZE;
                    }
                }
                (left, top, right - left, bottom - top)
            }
        }
    }

    /// The window mover. Runs on its own thread so no window operation ever
    /// executes inside the input-hook callback (SetWindowPos/ShowWindow send
    /// synchronous messages to the target app's thread and can stall the OS
    /// input pipeline past the LL-hook timeout). Wakes ~120x/sec, polls the
    /// *real* cursor with GetCursorPos, and repositions the window. Because the
    /// hook never blocks mouse-moves, the physical cursor travels freely,
    /// relative input accumulates normally, and the window simply follows the
    /// pointer at a fixed grab offset.
    fn mover_loop() {
        let mut was_active = false;
        let mut local_gen = u64::MAX;
        let mut last: Option<(i32, i32, i32, i32)> = None;
        // Raise the system timer resolution to 1 ms only while a drag is live
        // (AltSnap does the same), so MOVE_INTERVAL sleeps are accurate without
        // holding a 1 ms tick system-wide when idle.
        let mut hires = false;
        loop {
            std::thread::sleep(MOVE_INTERVAL);
            if !MOVER_RUN.load(Ordering::Relaxed) {
                break;
            }
            let active = ACTIVE.load(Ordering::Relaxed);
            let finishing = was_active && !active; // apply one exact final frame
            was_active = active;
            if active && !hires {
                unsafe { timeBeginPeriod(1) };
                hires = true;
            } else if !active && !finishing && hires {
                unsafe { timeEndPeriod(1) };
                hires = false;
            }
            if !active && !finishing {
                continue;
            }

            // Snapshot the per-grab params under a brief lock (never held during
            // SetWindowPos, so the hook's button handlers don't stall).
            let (mode, hwnd, sx, sy, mut rect, mut edges, restore_max, snap, neighbors, border) =
                match state().lock() {
                    Ok(st) => (
                        st.mode,
                        st.hwnd,
                        st.start_x,
                        st.start_y,
                        st.rect,
                        st.edges,
                        st.restore_max,
                        st.snap,
                        st.neighbors.clone(),
                        st.border,
                    ),
                    Err(_) => continue,
                };
            let gen = GEN.load(Ordering::Relaxed);
            let new_grab = gen != local_gen;
            if new_grab {
                local_gen = gen;
                last = None;
            }
            let hw = HWND(hwnd as *mut _);

            // Bring the grabbed window to the front on the first frame of each
            // grab. We swallow the Alt+click, so the window never gets the click
            // that would normally raise it — replicate that here. A plain
            // SetWindowPos(HWND_TOP) is silently ignored across processes (a
            // background app can't reorder another app's window), so use the
            // AttachThreadInput + SetForegroundWindow recipe. Done on the mover
            // thread, never the hook thread.
            if new_grab {
                unsafe {
                    raise_window(hw);
                }
            }

            // Deferred maximized-restore: a maximized window can't be usefully
            // moved/resized in place. Restore it here, on the grab's first
            // frame, then (for a move) recenter it under the grab point so the
            // drag continues naturally.
            if restore_max {
                unsafe {
                    let _ = ShowWindow(hw, SW_RESTORE);
                    let mut r = RECT::default();
                    if GetWindowRect(hw, &mut r).is_ok() {
                        if mode == Mode::Move {
                            let w = r.right - r.left;
                            let h = r.bottom - r.top;
                            let nx = sx - w / 2;
                            let ny = sy - 15;
                            let _ = SetWindowPos(
                                hw,
                                HWND::default(),
                                nx,
                                ny,
                                0,
                                0,
                                SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE,
                            );
                            rect = RECT {
                                left: nx,
                                top: ny,
                                right: nx + w,
                                bottom: ny + h,
                            };
                        } else {
                            rect = r;
                            edges = pick_edges(sx, sy, &r);
                        }
                    }
                }
                if let Ok(mut st) = state().lock() {
                    // Persist the corrected grab rect/edges unless a newer grab
                    // already replaced this one (GEN bumps under the same lock).
                    if GEN.load(Ordering::Relaxed) == gen {
                        st.rect = rect;
                        st.edges = edges;
                        st.restore_max = false;
                    }
                }
            }

            // Ground truth for the drag: the actual on-screen cursor. Never
            // fall back to (0,0) if the read fails (secure desktop etc.).
            let mut p = POINT::default();
            if unsafe { GetCursorPos(&mut p) }.is_err() {
                continue;
            }
            let (cx, cy) = (p.x, p.y);
            let (mut x, mut y, mut w, mut h) = compute_target(mode, sx, sy, rect, edges, cx, cy);

            // Aero-Snap on move: committed on release (the finishing frame). If
            // the cursor is in an edge zone, snap to that half / quarter, or
            // maximize for the top edge. During the drag the window just follows
            // the cursor; the preview overlay shows where it will land.
            let mut maximize_apply = false;
            let mut snapped_move = false;
            if mode == Mode::Move && finishing {
                let cp = POINT { x: cx, y: cy };
                if let Some(work) = unsafe { work_area_at(cp) } {
                    if let Some(zone) = snap_zone(cp, work) {
                        if zone == SnapZone::Maximize {
                            maximize_apply = true;
                        } else {
                            let fill_x = unsafe { snap_fill_x(zone, work, hw) };
                            let (rc, _) = zone_rect(zone, work, border, fill_x);
                            x = rc.left;
                            y = rc.top;
                            w = rc.right - rc.left;
                            h = rc.bottom - rc.top;
                            snapped_move = true;
                        }
                    }
                }
            }

            // Tiling: a snapped resize whose free edge is shared with neighbors
            // moves every neighbor's facing edge too (clamped so none drops
            // below MIN_SIZE), so shrinking the target grows all of them.
            let mut neighbor_apply: Vec<(isize, i32, i32, i32, i32)> = Vec::new();
            if mode == Mode::Resize && snap != SnapKind::None && !neighbors.is_empty() {
                let ((tl, tt, tr, tb), nrects) =
                    coordinate_neighbors(snap, &neighbors, x, y, x + w, y + h);
                x = tl;
                y = tt;
                w = tr - tl;
                h = tb - tt;
                neighbor_apply = nrects
                    .into_iter()
                    .map(|(nh, r)| (nh, r.left, r.top, r.right - r.left, r.bottom - r.top))
                    .collect();
            }

            // Dead-zone: skip sub-2px changes to absorb high-polling-mouse
            // tremor; always apply the final frame so the landing is exact.
            let skip = !finishing
                && matches!(last, Some((ax, ay, aw, ah))
                    if (x - ax).abs() <= 1 && (y - ay).abs() <= 1
                        && (w - aw).abs() <= 1 && (h - ah).abs() <= 1);
            if skip {
                continue;
            }
            unsafe {
                if !neighbor_apply.is_empty() {
                    // Tiling: move the target and ALL its neighbors in one
                    // deferred batch so the window manager applies them in a
                    // single screen-refresh cycle — the shared edges stay locked
                    // instead of the neighbors trailing the target. Synchronous
                    // (no SWP_ASYNCWINDOWPOS) so they land together this frame.
                    let dflags = SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE;
                    if let Ok(mut hdwp) = BeginDeferWindowPos(1 + neighbor_apply.len() as i32) {
                        if let Ok(h) = DeferWindowPos(hdwp, hw, HWND::default(), x, y, w, h, dflags) {
                            hdwp = h;
                            for (nh, nx, ny, nw, nhh) in &neighbor_apply {
                                let nhw = HWND(*nh as *mut _);
                                match DeferWindowPos(
                                    hdwp,
                                    nhw,
                                    HWND::default(),
                                    *nx,
                                    *ny,
                                    *nw,
                                    *nhh,
                                    dflags,
                                ) {
                                    Ok(h) => hdwp = h,
                                    Err(_) => break,
                                }
                            }
                            let _ = EndDeferWindowPos(hdwp);
                        }
                    }
                } else if maximize_apply {
                    // Top-edge move-snap: use the real OS maximize.
                    let _ = ShowWindow(hw, SW_MAXIMIZE);
                } else if mode == Mode::Move && !snapped_move {
                    // Plain move: SWP_ASYNCWINDOWPOS (same as AltSnap) posts the
                    // reposition to the target's thread instead of waiting on it,
                    // so a busy or hung app never stalls this loop. A move is
                    // cheap for the app (no relayout), so there's no flood risk.
                    let _ = SetWindowPos(
                        hw,
                        HWND::default(),
                        x,
                        y,
                        w,
                        h,
                        SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOSIZE | SWP_NOACTIVATE
                            | SWP_ASYNCWINDOWPOS,
                    );
                } else {
                    // Resize: SYNCHRONOUS (no SWP_ASYNCWINDOWPOS) so the mover
                    // self-paces to how fast the app can relayout. Firing async
                    // resizes at ~200 Hz floods a slow app (File Explorer relays
                    // out its view on every WM_SIZE): its queue backs up and the
                    // window falls further and further behind the cursor.
                    // Synchronous keeps latency bounded — the native experience.
                    let _ = SetWindowPos(
                        hw,
                        HWND::default(),
                        x,
                        y,
                        w,
                        h,
                        SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
                    );
                }
            }
            last = Some((x, y, w, h));
        }
        if hires {
            unsafe { timeEndPeriod(1) };
        }
    }

    /// After an Alt+drag, releasing Alt would otherwise activate the window's
    /// menu bar (Alt-up with no intervening keystroke). Inject a keystroke for
    /// an unassigned virtual key so Windows treats the Alt chord as "used".
    unsafe fn suppress_alt_menu() {
        fn ev(up: bool) -> INPUT {
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0x07), // 0x07 is unassigned
                        wScan: 0,
                        dwFlags: if up {
                            KEYEVENTF_KEYUP
                        } else {
                            KEYBD_EVENT_FLAGS(0)
                        },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        }
        let inputs = [ev(false), ev(true)];
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

// ---------------------------------------------------------------------------
// macOS  (UNVERIFIED — written on Windows; see TODO.md for on-device testing)
// ---------------------------------------------------------------------------
#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};

    // --- Framework types (opaque pointers) ---
    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFRunLoopRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFMachPortRef = *mut c_void;
    type AXUIElementRef = *const c_void;
    type AXValueRef = *const c_void;
    type CGEventRef = *mut c_void;
    type CGEventTapProxy = *mut c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    type CGEventTapCallBack = unsafe extern "C" fn(
        proxy: CGEventTapProxy,
        etype: u32,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: CGEventTapCallBack,
            user_info: *mut c_void,
        ) -> CFMachPortRef;
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
        fn CGEventGetType(event: CGEventRef) -> u32;
        fn CGEventGetFlags(event: CGEventRef) -> u64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFAllocatorDefault: CFTypeRef;
        static kCFRunLoopCommonModes: CFStringRef;
        fn CFStringCreateWithCString(
            alloc: CFTypeRef,
            cstr: *const std::os::raw::c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFMachPortCreateRunLoopSource(
            alloc: CFTypeRef,
            port: CFMachPortRef,
            order: isize,
        ) -> CFRunLoopSourceRef;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
        fn CFRunLoopRun();
        fn CFRunLoopStop(rl: CFRunLoopRef);
        fn CFRelease(cf: CFTypeRef);
    }

    // The kAX* constants are CFSTR("...") macros in the SDK headers, not
    // exported linkable symbols (verified: they're absent from
    // HIServices.tbd on modern macOS). Build the CFStringRefs at runtime
    // instead and cache them for the process lifetime.
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyElementAtPosition(
            app: AXUIElementRef,
            x: f32,
            y: f32,
            element: *mut AXUIElementRef,
        ) -> i32;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attr: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
        fn AXUIElementSetAttributeValue(
            element: AXUIElementRef,
            attr: CFStringRef,
            value: CFTypeRef,
        ) -> i32;
        fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> i32;
        fn AXValueCreate(the_type: u32, value_ptr: *const c_void) -> AXValueRef;
        fn AXValueGetValue(value: AXValueRef, the_type: u32, value_ptr: *mut c_void) -> bool;
    }

    // kCFStringEncodingUTF8
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    // CGEventType
    const K_LEFT_DOWN: u32 = 1;
    const K_LEFT_UP: u32 = 2;
    const K_RIGHT_DOWN: u32 = 3;
    const K_RIGHT_UP: u32 = 4;
    const K_LEFT_DRAG: u32 = 6;
    const K_RIGHT_DRAG: u32 = 7;
    const K_TAP_DISABLED_TIMEOUT: u32 = 0xFFFF_FFFE;
    const K_TAP_DISABLED_USERINPUT: u32 = 0xFFFF_FFFF;

    // kCGEventFlagMaskAlternate (the Option / Alt modifier)
    const FLAG_ALT: u64 = 0x0008_0000;

    // AXValueType
    const K_AXVALUE_CGPOINT: u32 = 1;
    const K_AXVALUE_CGSIZE: u32 = 2;

    const MIN_SIZE: f64 = 120.0;

    /// Create a CFStringRef from a literal and cache it for the process
    /// lifetime (CFStringRefs are immutable and never need releasing).
    fn ax_cfstr(literal: &'static str) -> CFStringRef {
        use std::sync::OnceLock;
        // CFStringRef is *const c_void, which isn't Send/Sync by default.
        // CFStrings are immutable and thread-safe, so sharing the pointer is
        // sound — wrap it in a Send+Sync newtype to satisfy OnceLock.
        struct CFStr(CFStringRef);
        unsafe impl Send for CFStr {}
        unsafe impl Sync for CFStr {}
        static POS: OnceLock<CFStr> = OnceLock::new();
        static SIZE: OnceLock<CFStr> = OnceLock::new();
        static WIN: OnceLock<CFStr> = OnceLock::new();
        static TOP: OnceLock<CFStr> = OnceLock::new();
        static RAISE: OnceLock<CFStr> = OnceLock::new();
        macro_rules! get {
            ($cell:expr) => {{
                $cell
                    .get_or_init(|| CFStr(unsafe {
                        CFStringCreateWithCString(
                            kCFAllocatorDefault,
                            literal.as_ptr() as *const std::os::raw::c_char,
                            K_CF_STRING_ENCODING_UTF8,
                        )
                    }))
                    .0
            }};
        }
        match literal {
            "AXPosition" => get!(POS),
            "AXSize" => get!(SIZE),
            "AXWindow" => get!(WIN),
            "AXTopLevelUIElement" => get!(TOP),
            "AXRaise" => get!(RAISE),
            _ => std::ptr::null(),
        }
    }
    fn k_position() -> CFStringRef {
        ax_cfstr("AXPosition")
    }
    fn k_size() -> CFStringRef {
        ax_cfstr("AXSize")
    }
    fn k_window() -> CFStringRef {
        ax_cfstr("AXWindow")
    }
    fn k_toplevel() -> CFStringRef {
        ax_cfstr("AXTopLevelUIElement")
    }
    fn k_raise() -> CFStringRef {
        ax_cfstr("AXRaise")
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Move,
        Resize,
    }
    #[derive(Clone, Copy, Default)]
    struct Edges {
        left: bool,
        right: bool,
        top: bool,
        bottom: bool,
    }

    struct DragState {
        active: bool,
        mode: Mode,
        window: usize, // retained AXUIElementRef; 0 = none
        start_cursor: CGPoint,
        origin: CGPoint,
        size: CGSize,
        edges: Edges,
    }
    impl Default for DragState {
        fn default() -> Self {
            Self {
                active: false,
                mode: Mode::Move,
                window: 0,
                start_cursor: CGPoint { x: 0.0, y: 0.0 },
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: 0.0,
                    height: 0.0,
                },
                edges: Edges::default(),
            }
        }
    }

    static STATE: OnceLock<Mutex<DragState>> = OnceLock::new();
    fn state() -> &'static Mutex<DragState> {
        STATE.get_or_init(|| Mutex::new(DragState::default()))
    }

    static ACTIVE: AtomicBool = AtomicBool::new(false);
    static RUNNING: AtomicBool = AtomicBool::new(false);
    static RUNLOOP: AtomicUsize = AtomicUsize::new(0);
    static TAP: AtomicUsize = AtomicUsize::new(0);

    pub fn enable() -> Result<(), String> {
        if RUNNING.load(Ordering::Relaxed) {
            return Ok(());
        }
        unsafe {
            if !AXIsProcessTrusted() {
                return Err("Alt-drag needs the Accessibility permission: System Settings → Privacy & Security → Accessibility".into());
            }
        }
        RUNNING.store(true, Ordering::Relaxed);
        std::thread::Builder::new()
            .name("window-drag-tap".into())
            .spawn(|| unsafe {
                let mask: u64 = (1u64 << K_LEFT_DOWN)
                    | (1u64 << K_LEFT_UP)
                    | (1u64 << K_LEFT_DRAG)
                    | (1u64 << K_RIGHT_DOWN)
                    | (1u64 << K_RIGHT_UP)
                    | (1u64 << K_RIGHT_DRAG);
                // kCGHIDEventTap=0, kCGHeadInsertEventTap=0, kCGEventTapOptionDefault=0
                let tap = CGEventTapCreate(0, 0, 0, mask, tap_callback, std::ptr::null_mut());
                if tap.is_null() {
                    RUNNING.store(false, Ordering::Relaxed);
                    eprintln!("window_drag: CGEventTapCreate returned null (permission?)");
                    return;
                }
                TAP.store(tap as usize, Ordering::Relaxed);
                let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
                let rl = CFRunLoopGetCurrent();
                RUNLOOP.store(rl as usize, Ordering::Relaxed);
                CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
                CFRunLoopRun(); // returns when disable() calls CFRunLoopStop

                CGEventTapEnable(tap, false);
                CFRelease(source as CFTypeRef);
                CFRelease(tap as CFTypeRef);
                TAP.store(0, Ordering::Relaxed);
                RUNLOOP.store(0, Ordering::Relaxed);
                RUNNING.store(false, Ordering::Relaxed);
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn disable() -> Result<(), String> {
        if let Ok(mut st) = state().lock() {
            if st.window != 0 {
                unsafe { CFRelease(st.window as CFTypeRef) };
                st.window = 0;
            }
            st.active = false;
        }
        ACTIVE.store(false, Ordering::Relaxed);
        let rl = RUNLOOP.load(Ordering::Relaxed);
        if rl != 0 {
            unsafe { CFRunLoopStop(rl as CFRunLoopRef) };
        }
        Ok(())
    }

    unsafe extern "C" fn tap_callback(
        _proxy: CGEventTapProxy,
        etype: u32,
        event: CGEventRef,
        _info: *mut c_void,
    ) -> CGEventRef {
        // The OS disables the tap if our callback is slow or on some input;
        // re-enable and pass the event through.
        if etype == K_TAP_DISABLED_TIMEOUT || etype == K_TAP_DISABLED_USERINPUT {
            let tap = TAP.load(Ordering::Relaxed);
            if tap != 0 {
                CGEventTapEnable(tap as CFMachPortRef, true);
            }
            return event;
        }

        let loc = CGEventGetLocation(event);
        let alt = (CGEventGetFlags(event) & FLAG_ALT) != 0;
        let _ = CGEventGetType; // (type comes in via `etype`)

        let mut st = match state().lock() {
            Ok(g) => g,
            Err(_) => return event,
        };

        match etype {
            K_LEFT_DOWN | K_RIGHT_DOWN => {
                if st.active {
                    return std::ptr::null_mut();
                }
                if !alt {
                    return event;
                }
                let mode = if etype == K_LEFT_DOWN {
                    Mode::Move
                } else {
                    Mode::Resize
                };
                if let Some(win) = window_at(loc.x, loc.y) {
                    if let (Some(origin), Some(size)) = (
                        read_point(win, k_position()),
                        read_size(win, k_size()),
                    ) {
                        st.active = true;
                        st.mode = mode;
                        st.window = win as usize;
                        st.start_cursor = loc;
                        st.origin = origin;
                        st.size = size;
                        if mode == Mode::Resize {
                            st.edges = pick_edges(loc, origin, size);
                        }
                        // Raise the grabbed window to the front, matching the
                        // Windows arm's raise-on-grab. Best-effort: a failure
                        // (e.g. the element doesn't implement AXRaise) doesn't
                        // abort the drag.
                        let _ = AXUIElementPerformAction(win, k_raise());
                        ACTIVE.store(true, Ordering::Relaxed);
                        return std::ptr::null_mut(); // consume the click
                    }
                    CFRelease(win as CFTypeRef);
                }
                event
            }
            K_LEFT_DRAG | K_RIGHT_DRAG => {
                if !st.active {
                    return event;
                }
                apply(&st, loc);
                std::ptr::null_mut()
            }
            K_LEFT_UP => end(&mut st, Mode::Move, event),
            K_RIGHT_UP => end(&mut st, Mode::Resize, event),
            _ => event,
        }
    }

    unsafe fn end(st: &mut DragState, mode: Mode, event: CGEventRef) -> CGEventRef {
        if st.active && st.mode == mode {
            if st.window != 0 {
                CFRelease(st.window as CFTypeRef);
                st.window = 0;
            }
            st.active = false;
            ACTIVE.store(false, Ordering::Relaxed);
            std::ptr::null_mut()
        } else {
            event
        }
    }

    unsafe fn copy_attr(el: AXUIElementRef, attr: CFStringRef) -> CFTypeRef {
        let mut val: CFTypeRef = std::ptr::null();
        if AXUIElementCopyAttributeValue(el, attr, &mut val) == 0 {
            val
        } else {
            std::ptr::null()
        }
    }

    /// The window under the cursor, retained (+1). Caller must CFRelease.
    unsafe fn window_at(px: f64, py: f64) -> Option<AXUIElementRef> {
        let sys = AXUIElementCreateSystemWide();
        if sys.is_null() {
            return None;
        }
        let mut el: AXUIElementRef = std::ptr::null();
        let err = AXUIElementCopyElementAtPosition(sys, px as f32, py as f32, &mut el);
        CFRelease(sys as CFTypeRef);
        if err != 0 || el.is_null() {
            return None;
        }
        // The hit element is usually a control; climb to its window.
        let mut win = copy_attr(el, k_window());
        if win.is_null() {
            win = copy_attr(el, k_toplevel());
        }
        CFRelease(el as CFTypeRef);
        if win.is_null() {
            None
        } else {
            Some(win as AXUIElementRef)
        }
    }

    unsafe fn read_point(el: AXUIElementRef, attr: CFStringRef) -> Option<CGPoint> {
        let v = copy_attr(el, attr);
        if v.is_null() {
            return None;
        }
        let mut p = CGPoint { x: 0.0, y: 0.0 };
        let ok = AXValueGetValue(
            v as AXValueRef,
            K_AXVALUE_CGPOINT,
            &mut p as *mut _ as *mut c_void,
        );
        CFRelease(v);
        if ok {
            Some(p)
        } else {
            None
        }
    }

    unsafe fn read_size(el: AXUIElementRef, attr: CFStringRef) -> Option<CGSize> {
        let v = copy_attr(el, attr);
        if v.is_null() {
            return None;
        }
        let mut s = CGSize {
            width: 0.0,
            height: 0.0,
        };
        let ok = AXValueGetValue(
            v as AXValueRef,
            K_AXVALUE_CGSIZE,
            &mut s as *mut _ as *mut c_void,
        );
        CFRelease(v);
        if ok {
            Some(s)
        } else {
            None
        }
    }

    unsafe fn set_point(el: AXUIElementRef, attr: CFStringRef, p: CGPoint) {
        let v = AXValueCreate(K_AXVALUE_CGPOINT, &p as *const _ as *const c_void);
        if !v.is_null() {
            AXUIElementSetAttributeValue(el, attr, v as CFTypeRef);
            CFRelease(v as CFTypeRef);
        }
    }

    unsafe fn set_size(el: AXUIElementRef, attr: CFStringRef, s: CGSize) {
        let v = AXValueCreate(K_AXVALUE_CGSIZE, &s as *const _ as *const c_void);
        if !v.is_null() {
            AXUIElementSetAttributeValue(el, attr, v as CFTypeRef);
            CFRelease(v as CFTypeRef);
        }
    }

    fn pick_edges(cursor: CGPoint, origin: CGPoint, size: CGSize) -> Edges {
        let w = size.width.max(1.0);
        let h = size.height.max(1.0);
        let rx = cursor.x - origin.x;
        let ry = cursor.y - origin.y;
        // 2x2 quadrants: the grabbed corner follows the cursor (see the Windows
        // pick_edges). Every grab resolves to one horizontal + one vertical edge.
        let mut e = Edges::default();
        if rx < w / 2.0 {
            e.left = true;
        } else {
            e.right = true;
        }
        if ry < h / 2.0 {
            e.top = true;
        } else {
            e.bottom = true;
        }
        e
    }

    unsafe fn apply(st: &DragState, cursor: CGPoint) {
        let win = st.window as AXUIElementRef;
        if win.is_null() {
            return;
        }
        let dx = cursor.x - st.start_cursor.x;
        let dy = cursor.y - st.start_cursor.y;
        match st.mode {
            Mode::Move => {
                set_point(
                    win,
                    k_position(),
                    CGPoint {
                        x: st.origin.x + dx,
                        y: st.origin.y + dy,
                    },
                );
            }
            Mode::Resize => {
                let mut left = st.origin.x;
                let mut top = st.origin.y;
                let mut right = st.origin.x + st.size.width;
                let mut bottom = st.origin.y + st.size.height;
                if st.edges.left {
                    left = st.origin.x + dx;
                }
                if st.edges.right {
                    right = st.origin.x + st.size.width + dx;
                }
                if st.edges.top {
                    top = st.origin.y + dy;
                }
                if st.edges.bottom {
                    bottom = st.origin.y + st.size.height + dy;
                }
                if right - left < MIN_SIZE {
                    if st.edges.left {
                        left = right - MIN_SIZE;
                    } else {
                        right = left + MIN_SIZE;
                    }
                }
                if bottom - top < MIN_SIZE {
                    if st.edges.top {
                        top = bottom - MIN_SIZE;
                    } else {
                        bottom = top + MIN_SIZE;
                    }
                }
                // Move the origin first so AX doesn't clamp the new size against
                // the old frame when a top/left edge is being dragged.
                set_point(win, k_position(), CGPoint { x: left, y: top });
                set_size(
                    win,
                    k_size(),
                    CGSize {
                        width: right - left,
                        height: bottom - top,
                    },
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Other platforms (Linux/Wayland): unsupported — see TODO.md.
// ---------------------------------------------------------------------------
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    pub fn enable() -> Result<(), String> {
        Err("Alt-drag window management isn't available on this platform".into())
    }
    pub fn disable() -> Result<(), String> {
        Ok(())
    }
}
