// Windows arm of Alt-drag: the input hook, drag lifecycle and hit-testing live here;
// snap/tiling geometry in `snap`, the hover overlay in `hover`, the mover thread in `mover`.
mod hover;
mod mover;
mod snap;

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{BOOL, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_MENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CallNextHookEx, DispatchMessageW, GetAncestor, GetClassNameW,
    GetDesktopWindow, GetForegroundWindow, GetMessageW, GetShellWindow, GetWindowLongW,
    GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, IsZoomed, PostThreadMessageW,
    SetForegroundWindow, SetWindowPos, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WindowFromPoint, GA_ROOT, GWL_STYLE, HHOOK, HWND_TOP, MSG, MSLLHOOKSTRUCT, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_QUIT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WS_CAPTION, WS_THICKFRAME,
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

pub fn enable() -> Result<(), String> {
    if HOOK.load(Ordering::Relaxed) != 0 {
        return Ok(());
    }
    // The mover thread does all SetWindowPos work, decoupled from the hook.
    MOVER_RUN.store(true, Ordering::Relaxed);
    std::thread::Builder::new()
        .name("window-drag-mover".into())
        .spawn(mover::mover_loop)
        .map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("window-drag-hook".into())
        .spawn(|| unsafe {
            // hMod must reference the module containing the hook proc; our
            // Rust is linked into the process's main module.
            let hmod = GetModuleHandleW(None).unwrap_or_default();
            let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), HINSTANCE(hmod.0), 0)
            {
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
            hover::create_indicator(HINSTANCE(hmod.0));

            // Message pump: LL hooks are delivered to the thread that
            // installed them, so this thread must run one; the indicator
            // window's WM_TIMER/paint messages need dispatching too.
            // disable() breaks the loop by posting WM_QUIT.
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            hover::destroy_indicator();
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
            hover::hide_indicator();
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
                        (real_snap, snap::find_neighbors(real_snap, &rect, hwnd))
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
                            if let Some(nb) = snap::clean_tile_edge(e, &rect, hwnd) {
                                let d = snap::edge_dist(e, px, py, &rect);
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
                        snap::pick_edges(px, py, &rect)
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
    let attached =
        fg_thread != 0 && fg_thread != our && AttachThreadInput(fg_thread, our, BOOL(1)).as_bool();
    let _ = BringWindowToTop(hw);
    let _ = SetForegroundWindow(hw);
    let _ = SetWindowPos(
        hw,
        HWND_TOP,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    );
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
                | "TaskSwitcherWnd" // legacy Alt+Tab
    )
}

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
    if (dwm_ok && r.right > r.left && r.bottom > r.top) || GetWindowRect(hwnd, &mut r).is_ok() {
        Some(r)
    } else {
        None
    }
}

/// Timer-driven (hook thread): dim the window under the cursor with the
/// resize region highlighted, or hide when Alt is up / a drag is running /
/// the cursor isn't over a draggable window.
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
